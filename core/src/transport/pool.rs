//! Finite-incarnation index ownership using native 32-bit atomics.
//! No payload, queue or transport type enters an ordinary application's types.
//! A binding retains a lease until its complete native cleanup finishes.
//! A hot slot, not the sum of spare slots, limits the incarnation's lifetime.
//! Lock-free retries are progress-dependent, not a universal elapsed deadline.
// [impl->req~ssr-runtime-no-heap~1]
// [impl->req~ssr-release-lease-drop~2]
#![deny(unsafe_code)]

#[cfg(not(target_has_atomic = "32"))]
compile_error!("native AtomicU32 CAS and RMW are required");
#[cfg(not(any(target_pointer_width = "32", target_pointer_width = "64")))]
compile_error!("this candidate's addressable-slice sum proof covers 32/64-bit pointers");

use crate::platform::atomic::{AtomicU32, Ordering::SeqCst};

const LOW_BITS: u32 = 30;
const MID_BITS: u32 = 31;
const LOW_MASK: u32 = (1_u32 << LOW_BITS) - 1;
const MID_MASK: u32 = (1_u32 << MID_BITS) - 1;
const CLOSED: u32 = 1_u32 << 31;
const LAST_FREE: u64 = u64::MAX - 1;
const OPEN: u32 = 0;
const CLOSING: u32 = 1;
const RETIRED: u32 = 2;

/// Startup storage. Fields stay private throughout the live incarnation.
/// Nominal components: three AtomicU32; no ABI/layout measurement is claimed.
pub struct IndexSlot {
    top: AtomicU32,    // monotone 0..=7, never wraps
    middle: AtomicU32, // bit31: top parity; bits0..30: digit
    low: AtomicU32,    // bit31: permanent CLOSED; bit30: parent parity; digit0..29
}

impl IndexSlot {
    /// Empty control storage, initialized exclusively before runtime.
    pub const fn new() -> Self {
        Self { top: AtomicU32::new(0), middle: AtomicU32::new(0), low: AtomicU32::new(0) }
    }

    // Explicit digit operations avoid emitted overflow-panic edges. The
    // monotone-counter invariants below prove that wrapping_sub never wraps;
    // they do not permit a generation wrap or reset.
    // All writes to top/middle come from the unique current lease, before its
    // final low-word publication. The parent advances by one at low rollover.
    fn read_parent(&self, hook: &mut impl FnMut(Event)) -> u64 {
        let a = self.top.load(SeqCst);
        hook(Event::ParentTopFirst(a));
        let m = self.middle.load(SeqCst);
        hook(Event::ParentMiddle(m));
        let b = self.top.load(SeqCst);
        hook(Event::ParentTopLast(b));
        if a != b {
            // Monotonicity implies a < b, hence a <= 6. The last parent
            // value of epoch a really occurred inside this read interval.
            (u64::from(a) << MID_BITS) | u64::from(MID_MASK)
        } else if (m >> MID_BITS) != (a & 1) {
            // The top store prepared a carry; middle still holds its old MAX.
            // a >= 1; the logical parent has not advanced yet.
            (u64::from(a).wrapping_sub(1) << MID_BITS) | u64::from(MID_MASK)
        } else {
            (u64::from(a) << MID_BITS) | u64::from(m & MID_MASK)
        }
    }

    fn read_clock(&self, hook: &mut impl FnMut(Event)) -> Observation {
        let a = self.read_parent(hook);
        hook(Event::ClockParentFirst(a));
        let raw = self.low.load(SeqCst);
        hook(Event::ClockLow(raw));
        let b = self.read_parent(hook);
        hook(Event::ClockParentLast(b));
        let value = if a != b {
            // a < b. Returning the last HELD state in parent epoch a is a
            // real-state read, not arbitrary concatenation or a wrap guess.
            (a << LOW_BITS) | u64::from(LOW_MASK)
        } else if ((raw >> LOW_BITS) & 1) != (a & 1) as u32 {
            // Parent prepared a carry, but the low publication has not happened.
            // a >= 1 and raw's digit is LOW_MASK (odd/held).
            (a.wrapping_sub(1) << LOW_BITS) | u64::from(LOW_MASK)
        } else {
            (a << LOW_BITS) | u64::from(raw & LOW_MASK)
        };
        Observation { value, raw }
    }
}

struct Observation { value: u64, raw: u32 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Distinguishes temporary exact exhaustion from permanent incarnation closure.
pub enum PoolError {
    /// All slots were leased at one cut during this call, before retirement.
    Full,
    /// Permanent lifetime fault. Never translate to capacity/Empty/Retry.
    PoolRetired,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Invalid storage capacity, test seed, or warning margin supplied during setup.
pub struct CapacityError;

/// Exclusive initialization of a fixed, nonempty index pool.
pub struct IndexPoolSetup<'storage> { cells: &'storage [IndexSlot], policy: LifetimePolicy }
impl<'storage> IndexPoolSetup<'storage> {
    /// Initializes supplied startup cells. Refuses an empty slice.
    pub fn new(cells: &'storage mut [IndexSlot]) -> Result<Self, CapacityError> {
        if cells.is_empty() { return Err(CapacityError); }
        for cell in cells.iter_mut() { *cell = IndexSlot::new(); }
        Ok(Self { cells, policy: LifetimePolicy::DEFAULT })
    }
    /// Consumes setup and enables shared acquisition; initialization is no longer available.
    pub fn enter_runtime(self) -> IndexPool<'storage> {
        IndexPool { cells: self.cells, gate: AtomicU32::new(OPEN), policy: self.policy, warning: AtomicU32::new(0) }
    }
}

/// Shared runtime index ownership with permanent finite-lifetime retirement.
pub struct IndexPool<'storage> {
    cells: &'storage [IndexSlot],
    gate: AtomicU32,
    policy: LifetimePolicy,
    warning: AtomicU32,
}

// Synchronous private schedule hooks for exact-width schedule tests. Production
// entry points supply no-op closures; later codegen must check their elimination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    ParentTopFirst(u32), ParentMiddle(u32), ParentTopLast(u32),
    ClockParentFirst(u64), ClockLow(u32), ClockParentLast(u64),
    Read(usize, u64), BeforeClaim(usize, u32), Claimed(usize), Collected(u128),
    ReleaseTop(u32), ReleaseMiddle(u32), BeforePublish(u64), Published(u64),
    CloseRequested, ClosedSlot(usize), Closed,
}

impl<'storage> IndexPool<'storage> {
    /// Claims one index. Full proves a pre-closure occupied cut; PoolRetired is permanent.
    /// # Safety-usable invariant
    /// Success returns the only lease for the current held incarnation of one
    /// cell in this pool, with its in-range index and captured odd clock. No
    /// other acquisition can own that cell until this lease's single return.
    /// The successful claim acquires the prior return's release publication.
    /// Retirement never revokes an existing lease; errors grant no ownership.
    pub fn try_acquire(&self) -> Result<IndexLease<'_, 'storage>, PoolError> {
        self.acquire_with(&mut |_| {})
    }

    fn acquire_with(&self, hook: &mut impl FnMut(Event)) -> Result<IndexLease<'_, 'storage>, PoolError> {
        let mut previous: Option<u128> = None;
        'retry: loop {
            match self.gate.load(SeqCst) {
                OPEN => {},
                RETIRED => return Err(PoolError::PoolRetired),
                _ => {
                    self.close_with(hook);
                    return Err(PoolError::PoolRetired);
                }
            }
            let mut sum = 0_u128;
            for (index, cell) in self.cells.iter().enumerate() {
                let seen = cell.read_clock(hook);
                hook(Event::Read(index, seen.value));
                if seen.raw & CLOSED != 0 || seen.value == LAST_FREE {
                    self.close_with(hook);
                    return Err(PoolError::PoolRetired);
                }
                if seen.value & 1 == 0 {
                    if self.gate.load(SeqCst) != OPEN {
                        self.close_with(hook);
                        return Err(PoolError::PoolRetired);
                    }
                    hook(Event::BeforeClaim(index, seen.raw));
                    // Low ABA is deliberately permitted for FREE->HELD. A
                    // successful CAS owns the CURRENT incarnation. Its parent
                    // is captured afterward, never from the old observation.
                    // CLOSED is part of expected, so this cannot succeed after
                    // that slot is closed, including on terminal LAST_FREE.
                    // Carry observations are odd. This even observation is
                    // therefore the actual raw FREE digit, whose bit0 is zero.
                    let claimed = seen.raw | 1;
                    if cell.low.compare_exchange(seen.raw, claimed, SeqCst, SeqCst).is_ok() {
                        hook(Event::Claimed(index));
                        let parent = cell.read_parent(hook);
                        let held = (parent << LOW_BITS) | u64::from(claimed & LOW_MASK);
                        self.warn_for(held);
                        return Ok(IndexLease { pool: self, cell, index, held });
                    }
                    previous = None;
                    continue 'retry;
                }
                // N <= usize::MAX <= 2^64-1 and each stamp <=2^64-2, so
                // even this loose bound is <2^128. Real slices are smaller.
                // Explicit arithmetic removes a codegen panic edge; this sum
                // never actually wraps under that addressable-slice bound.
                sum = sum.wrapping_add(u128::from(seen.value));
            }
            hook(Event::Collected(sum));
            if previous == Some(sum) {
                if self.gate.load(SeqCst) != OPEN {
                    self.close_with(hook);
                    return Err(PoolError::PoolRetired);
                }
                return Err(PoolError::Full);
            }
            previous = Some(sum);
        }
    }

    fn close_with(&self, hook: &mut impl FnMut(Event)) {
        if self.gate.load(SeqCst) == RETIRED { return; }
        // Never regress RETIRED to CLOSING if another helper completes first.
        let _ = self.gate.compare_exchange(OPEN, CLOSING, SeqCst, SeqCst);
        hook(Event::CloseRequested);
        for (index, cell) in self.cells.iter().enumerate() {
            // Permanent independent flag; never overwrites a generation or
            // payload, even if this helper resumes after reuse or closure.
            cell.low.fetch_or(CLOSED, SeqCst);
            hook(Event::ClosedSlot(index));
        }
        // Global retirement LP. Every slot is already closed; no later claim
        // CAS can succeed. Other helpers can complete this without our tail.
        self.gate.store(RETIRED, SeqCst);
        hook(Event::Closed);
    }

    /// # Safety
    /// The caller must own this pool's unique live lease for cell, which must be
    /// a member of this pool with current odd incarnation clock held, at most
    /// u64::MAX - 2. All accesses
    /// and borrows of its associated payload, including destructor accesses,
    /// must have ended. Neither caller nor hook may return or access that payload
    /// incarnation again, reset its control words, or duplicate its ownership.
    /// This call consumes the lease even if hook unwinds; it never authorizes a
    /// retry. A successful low-word publication releases prior payload cleanup
    /// before a later acquisition. Retirement may instead keep the cell closed.
    /// No payload access or destruction is performed by this method.
    #[allow(unsafe_code)]
    unsafe fn return_slot(&self, cell: &IndexSlot, held: u64, hook: &mut impl FnMut(Event)) {
        // Private unique lease proves odd held <=u64::MAX-2. All payload/native
        // cleanup MUST precede this method; control-only code does not prove it.
        // The unique-lease bound proves this cannot overflow. Keep a checked
        // backstop that closes the incarnation without publishing a wrapped
        // free generation if the internal ownership invariant is violated.
        let Some(next) = held.checked_add(1) else {
            self.close_with(hook);
            return;
        };
        let parent = next >> LOW_BITS;
        if next & u64::from(LOW_MASK) == 0 {
            let top = (parent >> MID_BITS) as u32;
            let middle = (parent & u64::from(MID_MASK)) as u32;
            if middle == 0 {
                cell.top.store(top, SeqCst);
                hook(Event::ReleaseTop(top));
            }
            cell.middle.store(((top & 1) << MID_BITS) | middle, SeqCst);
            hook(Event::ReleaseMiddle(middle));
        }
        let free = (((parent & 1) as u32) << LOW_BITS) | (next & u64::from(LOW_MASK)) as u32;
        let terminal = if next == LAST_FREE { CLOSED } else { 0 };
        self.warn_for(next);
        hook(Event::BeforePublish(next));
        let mut old = cell.low.load(SeqCst);
        loop {
            // The only concurrent change while leased is CLOSED:0->1. A strong
            // CAS can mismatch at most once; repeated ORs of 1 do not change it.
            let desired = free | terminal | (old & CLOSED);
            match cell.low.compare_exchange(old, desired, SeqCst, SeqCst) {
                Ok(_) => break,
                Err(now) => old = now,
            }
        }
        // Physical publication; LAST access to this slot's generation fields.
        // For terminal publication, CLOSED is inseparable from the free state.
        hook(Event::Published(next));
        if next == LAST_FREE { self.close_with(hook); }
    }
}

#[must_use = "dropping the lease returns its slot; forgetting retains it"]
/// One independently returnable index. Keep it until all native/payload cleanup completes.
pub struct IndexLease<'pool, 'storage> {
    // Safety invariant: the originating pool and its cell at index; this lease
    // alone owns that cell's current held incarnation until return or transfer.
    pool: &'pool IndexPool<'storage>,
    cell: &'pool IndexSlot,
    // Safety invariant: index is in pool.cells and identifies cell.
    index: usize,
    // Safety invariant: the current odd incarnation clock <=u64::MAX-2, captured after the
    // successful claim; no other owner may advance it before this lease returns.
    held: u64,
}
impl IndexLease<'_, '_> {
    /// The stable slot index exclusively owned by this lease.
    pub fn index(&self) -> usize { self.index }
}
impl Drop for IndexLease<'_, '_> {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        let pool = self.pool;
        let cell = self.cell;
        let held = self.held;
        // SAFETY: the lease field invariants supply membership and the sole
        // current held incarnation. &mut self at Drop excludes safe surviving
        // borrows of this lease; raw payload owners must retain it until their
        // cleanup ends. into_index forgets the lease, so transfer cannot also
        // reach this Drop. The local hook does nothing. This is the one return,
        // and neither this destructor nor its scalar/reference fields access
        // payload or incarnation controls after it.
        unsafe { pool.return_slot(cell, held, &mut |_| {}) };
        // Borrow/scalar fields have no implicit storage-touching destructors.
    }
}

/// Public progression. Observation is a point-in-time sample, not a promise that
/// the monitor runs before retirement. Values never regress within an incarnation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum LifetimeState {
    #[default]
    /// No low-margin publication has been observed in this incarnation.
    Healthy,
    /// Warning is latched; operations remain admitted under their ordinary contract.
    LifetimeMarginLow,
    /// Closure has begun or completed; existing owners remain valid and returnable.
    Retired,
}

/// Initialization policy in successful hottest-slot reuse cycles, never divided
/// by pool capacity. The default is a planning margin, not a timing qualification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LifetimePolicy { warning_remaining: u64 }
impl LifetimePolicy {
    /// Planning default of 2^32 hottest-slot reuse cycles; target timing must be qualified.
    pub const DEFAULT: Self = Self { warning_remaining: 1_u64 << 32 };
    /// Total successful reuse cycles of one initially zero counter.
    pub const MAX_REUSES: u64 = (u64::MAX - 1) / 2;
    /// Validates a nonzero warning margin within the representable lifetime.
    pub const fn new(warning_remaining: u64) -> Result<Self, CapacityError> {
        if warning_remaining == 0 || warning_remaining > Self::MAX_REUSES {
            Err(CapacityError)
        } else { Ok(Self { warning_remaining }) }
    }
    /// Checked setup sizing: hottest-slot reuse/second, monitor interval plus
    /// safe-response latency in milliseconds, and an explicit in-flight reserve.
    /// The integrator must qualify all three assumptions on the actual target.
    pub const fn for_response(rate_per_second: u64, response_ms: u64, in_flight: u64)
        -> Result<Self, CapacityError>
    {
        if rate_per_second == 0 || response_ms == 0 { return Err(CapacityError); }
        let work = (rate_per_second as u128) * (response_ms as u128);
        let cycles = work / 1000 + (if work % 1000 != 0 { 1 } else { 0 }) + in_flight as u128;
        if cycles > Self::MAX_REUSES as u128 { return Err(CapacityError); }
        Self::new(cycles as u64)
    }
    /// Configured hottest-slot reuse reserve.
    pub const fn warning_remaining(self) -> u64 { self.warning_remaining }
}
impl Default for LifetimePolicy { fn default() -> Self { Self::DEFAULT } }
impl Default for IndexSlot { fn default() -> Self { Self::new() } }
impl core::fmt::Debug for IndexSlot {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.write_str("IndexSlot { .. }") }
}
impl core::fmt::Debug for IndexPoolSetup<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_struct("IndexPoolSetup").field("slots", &self.cells.len()).field("policy", &self.policy).finish() }
}
impl core::fmt::Debug for IndexPool<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_struct("IndexPool").field("slots", &self.cells.len()).field("lifetime", &self.lifetime()).finish() }
}
impl core::fmt::Debug for IndexLease<'_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { f.debug_struct("IndexLease").field("index", &self.index).finish() }
}
impl<'storage> IndexPoolSetup<'storage> {
    /// Selects the validated warning policy before enabling runtime acquisition.
    pub fn with_lifetime_policy(mut self, policy: LifetimePolicy) -> Self { self.policy = policy; self }
    /// Test-only initialization of one free counter. There is no runtime reset.
    #[cfg(any(test, up_rust_test_hooks))]
    pub fn __seed_free_for_test(&mut self, index: usize, even: u64) -> Result<(), CapacityError> {
        if even & 1 != 0 || even > LAST_FREE { return Err(CapacityError); }
        let cell = self.cells.get(index).ok_or(CapacityError)?;
        let parent = even >> LOW_BITS;
        let top = (parent >> MID_BITS) as u32;
        cell.top.store(top, SeqCst);
        cell.middle.store(((top & 1) << MID_BITS) | (parent as u32 & MID_MASK), SeqCst);
        cell.low.store((((parent & 1) as u32) << LOW_BITS) | (even as u32 & LOW_MASK), SeqCst);
        Ok(())
    }
}
impl<'storage> IndexPool<'storage> {
    /// Reports closure beginning or completion, without revoking any held index.
    pub fn is_retired(&self) -> bool { self.gate.load(SeqCst) != OPEN }
    /// Samples the latched health state without allocation, waiting, or callbacks.
    pub fn lifetime(&self) -> LifetimeState {
        if self.is_retired() { LifetimeState::Retired }
        else if self.warning.load(SeqCst) != 0 { LifetimeState::LifetimeMarginLow }
        else { LifetimeState::Healthy }
    }
    /// Returns this incarnation's immutable warning policy.
    pub fn lifetime_policy(&self) -> LifetimePolicy { self.policy }
    fn warn_for(&self, clock: u64) {
        let remaining = (LAST_FREE - clock.min(LAST_FREE)) / 2;
        if remaining <= self.policy.warning_remaining { self.warning.store(1, SeqCst); }
    }
    /// Seeds control state only while the complete pool is exclusively in setup.
    /// # Safety
    /// Caller owns initialization rights for this pool and must exclude every
    /// acquisition, queue operation, observer and other reset for the call.
    /// No held/queued/retiring lease or payload reference may exist, including a
    /// forgotten lease or an outstanding raw-index return obligation. The pool
    /// has never entered runtime; reset is not a way to reclaim a live tenure.
    /// On success the selected FREE counter is initialized to `even`; ordinary
    /// acquisition may begin only after this call and setup publication complete.
    #[allow(unsafe_code)]
    #[cfg(any(test, up_rust_test_hooks))]
    pub(crate) unsafe fn seed_for_setup(&self, index: usize, even: u64) -> Result<(), CapacityError> {
        let mut setup = IndexPoolSetup { cells: self.cells, policy: self.policy };
        setup.__seed_free_for_test(index, even)
    }
    pub(crate) fn retire(&self) { self.close_with(&mut |_| {}); }
    pub(crate) fn warn(&self) { self.warning.store(1, SeqCst); }
    pub(crate) fn with_policy(mut self, policy: LifetimePolicy) -> Self { self.policy = policy; self }
    /// Internal raw-index ownership adapter for ready queues. Only the unique
    /// index owner calls this, after its final payload/destructor access. No
    /// mutable reference into a slot or its fields may survive this call.
    /// # Safety
    /// Caller must own the unique held incarnation represented by this index in
    /// this pool (obtained by into_index), with no remaining payload references
    /// or destructor access. This consumes the raw lease exactly once; no later
    /// access or return of that incarnation is permitted. Retirement preserves
    /// existing owners and may keep this cell permanently closed on return.
    // This adapter has an ownership precondition despite using only safe atomics.
    // Unsafe is allowed only at ownership-return boundaries and their callers.
    #[allow(unsafe_code)]
    pub(crate) unsafe fn return_index(&self, index: usize) {
        if let Some(cell) = self.cells.get(index) {
            let held = cell.read_clock(&mut |_| {}).value;
            // SAFETY: return_index's caller owns this pool's current unique raw
            // lease and has ended all payload access. get selects its member
            // cell; that owner excludes another return/advance, so read_clock
            // observes its current held incarnation even during permanent
            // closure (CLOSED changes no clock digit). The local hook is inert.
            // This consumes the obligation once and performs no later access.
            unsafe { self.return_slot(cell, held, &mut |_| {}) };
        }
    }
}
impl IndexPool<'static> {
    pub(crate) fn new_in(memory: &crate::memory::StartupMemory, count: usize)
        -> Result<Self, crate::memory::StartupError>
    {
        let mut cells = memory.fixed_vec::<IndexSlot>(count)?;
        for _ in 0..count { cells.push(IndexSlot::new())?; }
        // Zero capacity is an internal empty table; two equal empty collects
        // prove Full. The public setup rejects an accidental empty declaration.
        Ok(Self { cells: cells.into_static_slice(), gate: AtomicU32::new(OPEN), policy: LifetimePolicy::DEFAULT, warning: AtomicU32::new(0) })
    }
}
impl IndexLease<'_, '_> {
    /// # Safety-usable invariant
    /// Consumes the lease without returning its cell and transfers its unique
    /// return obligation to the raw index. Exactly one later unsafe return_index
    /// on the originating pool may consume it, after complete payload cleanup.
    pub(crate) fn into_index(self) -> usize { let index = self.index; core::mem::forget(self); index }
}

#[cfg(test)]
mod retirement_tests {
    use super::*;
    // [utest->req~ssr-release-lease-drop~2]
    // [utest->req~ssr-runtime-no-heap~1]
    #[test]
    fn retirement_more_than_word_capacity_and_exact_full() {
        let mut cells = [const { IndexSlot::new() }; 65];
        let pool = IndexPoolSetup::new(&mut cells).unwrap().enter_runtime();
        let mut leases = [const { None }; 65];
        for (i, lease) in leases.iter_mut().enumerate() { *lease = Some(pool.try_acquire().unwrap()); assert_eq!(lease.as_ref().unwrap().index(), i); }
        assert!(matches!(pool.try_acquire(), Err(PoolError::Full)));
        drop(leases[47].take());
        assert_eq!(pool.try_acquire().unwrap().index(), 47);
        assert_eq!(pool.lifetime(), LifetimeState::Healthy);
    }
    #[test]
    fn retirement_exact_width_carries() {
        for initial in [(1_u64 << 30) - 2, (1_u64 << 61) - 2] {
            let mut cells = [IndexSlot::new()];
            let mut setup = IndexPoolSetup::new(&mut cells).unwrap();
            setup.__seed_free_for_test(0, initial).unwrap();
            let pool = setup.enter_runtime();
            drop(pool.try_acquire().unwrap());
            assert_eq!(pool.cells[0].read_clock(&mut |_| {}).value, initial + 2);
            drop(pool.try_acquire().unwrap());
            assert_eq!(pool.cells[0].read_clock(&mut |_| {}).value, initial + 4);
        }
    }
    #[test]
    fn retirement_warning_precedes_terminal_and_held_owner_survives() {
        let mut cells = [IndexSlot::new(), IndexSlot::new()];
        let mut setup = IndexPoolSetup::new(&mut cells).unwrap().with_lifetime_policy(LifetimePolicy::new(3).unwrap());
        setup.__seed_free_for_test(0, LAST_FREE - 8).unwrap();
        let pool = setup.enter_runtime();
        assert_eq!(pool.lifetime(), LifetimeState::Healthy);
        let first = pool.try_acquire().unwrap();
        let held = pool.try_acquire().unwrap();
        assert!(matches!(pool.try_acquire(), Err(PoolError::Full)));
        assert_eq!(pool.lifetime(), LifetimeState::LifetimeMarginLow);
        drop(first);
        for _ in 0..3 { drop(pool.try_acquire().unwrap()); }
        assert!(pool.is_retired());
        assert_eq!(held.index(), 1);
        assert!(matches!(pool.try_acquire(), Err(PoolError::PoolRetired)));
        drop(held);
        assert_eq!(pool.lifetime(), LifetimeState::Retired);
        assert!(matches!(pool.try_acquire(), Err(PoolError::PoolRetired)));
    }
    #[test]
    fn retirement_publication_does_not_require_an_observer() {
        let mut cells = [IndexSlot::new()];
        let mut setup = IndexPoolSetup::new(&mut cells).unwrap();
        setup.__seed_free_for_test(0, LAST_FREE - 2).unwrap();
        let pool = setup.enter_runtime();
        drop(pool.try_acquire().unwrap()); // no monitor ran during the last lifetime
        assert_eq!(pool.lifetime(), LifetimeState::Retired);
        assert_eq!(pool.warning.load(SeqCst), 1); // warning was published, not observed
    }
    #[test]
    fn retirement_preclose_claim_completes_without_revocation() {
        let mut cells = [IndexSlot::new(), IndexSlot::new()];
        let mut setup = IndexPoolSetup::new(&mut cells).unwrap();
        setup.__seed_free_for_test(0, LAST_FREE - 2).unwrap();
        let pool = setup.enter_runtime();
        let mut terminal = Some(pool.try_acquire().unwrap());
        let admitted = pool.acquire_with(&mut |event| {
            if event == Event::Claimed(1) { drop(terminal.take()); }
        }).unwrap();
        assert_eq!(admitted.index(), 1);
        assert_eq!(pool.lifetime(), LifetimeState::Retired);
        drop(admitted);
        assert!(matches!(pool.try_acquire(), Err(PoolError::PoolRetired)));
    }
    #[test]
    fn retirement_closure_defeats_paused_unadmitted_claim() {
        let mut cells = [IndexSlot::new(), IndexSlot::new()];
        let mut setup = IndexPoolSetup::new(&mut cells).unwrap();
        setup.__seed_free_for_test(0, LAST_FREE - 2).unwrap();
        let pool = setup.enter_runtime();
        let mut terminal = Some(pool.try_acquire().unwrap());
        let result = pool.acquire_with(&mut |event| {
            if matches!(event, Event::BeforeClaim(1, _)) { drop(terminal.take()); }
        });
        assert!(matches!(result, Err(PoolError::PoolRetired)));
    }
    #[test]
    fn retirement_policy_validation_and_real_width_margin() {
        assert_eq!(LifetimePolicy::for_response(1_000_000, 2_500, 32).unwrap().warning_remaining(), 2_500_032);
        assert_eq!(LifetimePolicy::for_response(1, 1, 0).unwrap().warning_remaining(), 1);
        assert!(LifetimePolicy::new(0).is_err());
        assert!(LifetimePolicy::new(u64::MAX).is_err());
        assert!(LifetimePolicy::for_response(u64::MAX, u64::MAX, u64::MAX).is_err());
        assert!(LifetimePolicy::for_response(0, 1, 0).is_err());
        let mut cells = [IndexSlot::new()];
        let mut setup = IndexPoolSetup::new(&mut cells).unwrap();
        assert!(setup.__seed_free_for_test(0, 1).is_err());
        assert!(setup.__seed_free_for_test(1, 0).is_err());
        setup.__seed_free_for_test(0, LAST_FREE).unwrap();
        let pool = setup.enter_runtime();
        assert!(matches!(pool.try_acquire(), Err(PoolError::PoolRetired)));
    }
}
