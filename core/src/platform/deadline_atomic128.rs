//! Private admitted macOS ARM64 / Linux GNU host primitive. See `core/docs/pal-deadline-safety.md` for
//! the FFI/profile lemma.
use core::{cell::UnsafeCell, ffi::c_void};

#[derive(Clone, Copy)]
#[repr(C, align(16))]
struct Words {
    low: u64,
    high: u64,
}

#[repr(transparent)]
pub(crate) struct Atomic128 {
    // Safety invariant: the cell contains two initialized full-width integers,
    // in exactly the C hpr3_word layout (size/alignment 16, offsets 0/8).
    // Construction requires AtomicProfile, certifying CPU support for those
    // builtins throughout this process. All overlapping shared accesses use
    // the same 16-byte C atomic builtins.
    // No address or reference to the contents escapes a synchronous FFI call.
    words: UnsafeCell<Words>,
}

const _: () = {
    assert!(core::mem::size_of::<Words>() == 16);
    assert!(core::mem::align_of::<Words>() == 16);
    assert!(core::mem::offset_of!(Words, low) == 0);
    assert!(core::mem::offset_of!(Words, high) == 8);
    assert!(core::mem::size_of::<Atomic128>() == 16);
    assert!(core::mem::align_of::<Atomic128>() == 16);
};

unsafe extern "C" {
    /// # Safety
    /// Caller supplies for the whole call: storage pointing into one live,
    /// initialized, mutable UnsafeCell<Words> allocation, aligned to 16 bytes,
    /// valid for 16-byte atomic reads/writes; only matching hpr3 builtins may
    /// access overlapping bytes concurrently. low/high are distinct, exclusively
    /// writable initialized u64 objects, disjoint from storage. The linked C
    /// implementation must obey its exact size/alignment/builtin admission.
    /// Postcondition: one acquire load writes both outputs; no pointer retained,
    /// ownership transferred, callback, unwind, allocation or free.
    fn hpr3_load(storage: *const c_void, low: *mut u64, high: *mut u64);

    /// # Safety
    /// Caller supplies live initialized mutable UnsafeCell<Words> storage for
    /// the whole call, valid/aligned for the same 16-byte atomic access as
    /// hpr3_load, and the same linked C/profile contract. No overlapping
    /// non-atomic or mixed-width access may occur concurrently.
    /// Postcondition: one strong acq_rel CAS (failure acquire), returning 0 or 1;
    /// no retained pointer, ownership transfer, callback, unwind, allocation/free.
    fn hpr3_cas(storage: *mut c_void, a: u64, b: u64, c: u64, d: u64) -> i32;
    /// # Safety
    /// No caller storage. Linked baseline-ISA C code queries only documented
    /// CPU/OS capability data, returns 0/1, retains nothing and cannot unwind.
    fn hpr3_admit() -> i32;
    #[cfg(test)]
    fn hpr3_layout() -> u64;
}

// SAFETY: Send transfers an unborrowed value, including its initialized integer
// representation. The primitive has no address identity, retained pointer,
// destructor or thread affinity. Moves cannot overlap shared calls by Rust's
// borrowing rules. UnsafeCell layout and the admitted C builtins are the only
// foreign premises; every shared access is a matching whole-object atomic.
unsafe impl Send for Atomic128 {}
// SAFETY: Sync permits concurrent shared calls. All overlapping accesses use
// the same 16-byte acquire/acq_rel builtins through UnsafeCell::get; no Rust
// reference to Words is formed, no &mut self is manufactured, and C retains
// nothing. No caller-provided callback or safe trait affects this invariant.
unsafe impl Sync for Atomic128 {}

impl Atomic128 {
    /// # Safety-usable invariant
    /// The required AtomicProfile certifies CPU support for every later access.
    /// Produces initialized exact-layout storage. No foreign initialization or
    /// pinned address is required; all bit patterns of both u64 fields are valid.
    pub(crate) const fn new(value: (u64, u64), _profile: AtomicProfile) -> Self {
        Self {
            words: UnsafeCell::new(Words {
                low: value.0,
                high: value.1,
            }),
        }
    }

    /// # Safety-usable invariant
    /// Returns one coherent acquire observation; never exposes the storage.
    pub(crate) fn load(&self) -> (u64, u64) {
        let (mut low, mut high) = (0, 0);
        // SAFETY: words' invariant and &self provide initialized, live,
        // 16-aligned 16-byte interior-mutable storage throughout this call.
        // get() preserves the allocation's provenance and permits interior
        // mutation without creating a reference to Words. Output locals are
        // initialized distinct exclusive u64s disjoint from self. The C contract
        // writes both and retains nothing; these facts survive all shared calls.
        unsafe { hpr3_load(self.words.get().cast(), &mut low, &mut high) };
        (low, high)
    }

    /// # Safety-usable invariant
    /// Strong whole-word CAS: success writes desired with acq_rel ordering;
    /// failure observes a different word with acquire ordering. No spurious fail.
    pub(crate) fn cas(&self, old: (u64, u64), new: (u64, u64)) -> bool {
        // SAFETY: the same exact-layout/liveness/initialized/all-shared-access
        // invariant as load holds. get() supplies mutable cell provenance;
        // arguments are integers, with no aliasing output buffers or pointers
        // retained by C. Mutation remains wholly inside the UnsafeCell. C's
        // strong CAS contract supplies the only state change and ordering.
        unsafe { hpr3_cas(self.words.get().cast(), old.0, old.1, new.0, new.1) != 0 }
    }

    #[cfg(test)]
    pub(crate) fn layout() -> u64 {
        // SAFETY: no arguments or foreign storage; pure integer layout query.
        unsafe { hpr3_layout() }
    }
}

/// Witness of the selected build/platform profile and its local capability check.
/// Linux x86-64 additionally requires the executable's CX16 minimum ISA on every
/// eligible CPU; the build script enforces its compiler selection, not deployment.
#[derive(Clone, Copy)]
pub(crate) struct AtomicProfile(());
impl AtomicProfile {
    /// # Safety-usable invariant
    /// Some certifies the selected shim's CPU feature for this process. Private
    /// construction and no public fields prevent callers forging a witness.
    pub(crate) fn admit() -> Option<Self> { Self::admit_if(true) }
    fn admit_if(permitted: bool) -> Option<Self> {
        if !permitted { return None; }
        // SAFETY: hpr3_admit has no storage inputs and runs baseline ISA only.
        // Its selected platform contract returns true only with CMPXCHG16B on
        // Linux x86-64, LSE on Linux ARM64, or the Apple ARM64 baseline.
        // On Linux x86-64 the build script requires Rust's cmpxchg16b target
        // feature. The executable's minimum-ISA environment contract covers
        // every eligible CPU for its lifetime; CPUID only checks this CPU and
        // does not establish migration safety. Linux ARM64 instead obtains its
        // system-wide LSE premise from the ELF HWCAP contract. Apple ARM64 uses
        // its baseline. No wide instruction occurs before this query returns.
        if unsafe { hpr3_admit() } == 1 { Some(Self(())) } else { None }
    }
}
#[cfg(test)]
mod admission_tests {
    use super::*;
    #[test]
    fn disabled_profile_cannot_construct_witness() {
        assert!(AtomicProfile::admit_if(false).is_none());
    }
    #[test]
    fn running_profile_has_required_capability() {
        assert!(AtomicProfile::admit().is_some());
    }
}
