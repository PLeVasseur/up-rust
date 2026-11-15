//! Closed, callback-free operations over full ordinary metadata.
//! Exclusion/reference creation is confined to this module. Callers receive
//! copied scalars/results, never a guard, reference, or generic closure API.
#[cfg(test)]
use crate::test_support::FixtureResult as _;
use super::binding::PeerRef;
use super::error::ReceiveError;
use super::inbound::PeerAdmission;
use super::rpc::{
    phase, with_phase, ARMED, DELIVERING, FREE, PEER_LOST, PEER_RESTARTED, RESERVED, TIMED_OUT,
};
use crate::memory::{StartupError, StartupMemory};
#[cfg(all(up_rust_native32_model = "host", target_has_atomic = "64"))]
use crate::platform::atomic::{AtomicU64, Ordering};
use crate::platform::exclusion::Exclusion;
use crate::platform::progress::ProgressBackend;
use core::cell::UnsafeCell;

mod deadline;
pub(crate) use deadline::CoherentDeadline;

// Safety invariant LEAF: data contains an initialized T for this Leaf's
// lifetime; gate is its sole access domain. Every data access uses this same
// domain, and no protected reference escapes. Private fields prevent a safe
// external caller from replacing the gate or accessing data independently.
struct Leaf<T> {
    gate: Exclusion,
    // Safety invariant: this cell contains a live initialized T; every access
    // uses this Leaf's gate and obeys apply's no-reentry contract. No reference
    // into T escapes an operation. The cell and gate remain together when moved.
    data: UnsafeCell<T>,
}
impl<T> Leaf<T> {
    /// # Safety-usable invariant
    /// Stores this initialized T in a fresh exclusive domain; publishes no view.
    const fn new(data: T) -> Self {
        Self {
            gate: Exclusion::new(),
            data: UnsafeCell::new(data),
        }
    }
    /// # Safety
    /// The caller must supply a crate-controlled closure that cannot directly
    /// or transitively reenter any Leaf operation before its temporary mutable
    /// reference ends. It must return only owned copied metadata, invoke no
    /// caller-controlled code, and expose no alias of the protected data.
    /// With those premises this helper gives exclusive access to initialized T
    /// until f returns/unwinds, and releases exclusion only after the borrow ends.
    /// Target callers must also contain no await, destructor, platform callback,
    /// or execution-context handoff; ordinary application code cannot call this
    /// private helper or obtain its guard. Host tests deliberately exercise stalls.
    unsafe fn apply<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let _guard = ProgressBackend::enter(&self.gate);
        // SAFETY: UnsafeCell::get and reference creation require initialized,
        // aligned live T with exclusive access for the borrow. LEAF supplies
        // initialization/liveness; Exclusion::enter's documented platform model
        // excludes other execution contexts. This function's caller contract
        // forbids synchronous reentry while the mutable borrow exists. The FnOnce
        // signature cannot return a reference borrowing its fresh argument. The
        // guard is private and drops after f's final access, including unwind.
        f(unsafe { &mut *self.data.get() })
    }
}
// SAFETY: LEAF and apply's unsafe contract put all data access
// under the same reviewed exclusion domain; no references
// escape apply. T: Send permits sequential access by different execution contexts.
unsafe impl<T: Send> Sync for Leaf<T> {}

/// Target harness control over the actual SDK's full-record operations.
///
/// Returns a failure bitset (zero means the selected observations passed):
/// bit0 full clock/sequence, bit1 full peer/restart/storage reuse, bit2 full
/// RPC identity/deadline/phase, bit3 target nested interrupt-mask restoration.
/// Uses real StartupMemory and private production
/// operations; it does not replace the core with a harness-local implementation.
/// This is deterministic fixture evidence, never clock/entropy qualification,
/// G4 publication integration, transport acceptance, or a WCET measurement.
///
/// # Errors
/// Returns the actual startup memory error while constructing one peer entry.
#[doc(hidden)]
#[cfg(up_rust_test_hooks)]
// [utest->req~ssr-peer-stale-rejected~1]
// [utest->req~ssr-peer-failure-normal~1]
pub fn __native32_records_control(memory: &StartupMemory) -> Result<u32, StartupError> {
    let mut failed = 0_u32;
    #[cfg(not(up_rust_native32_model = "host"))]
    if !crate::platform::exclusion::nested_mask_control() {
        failed |= 8;
    }
    let generator = Generator::new(u64::MAX - 2, 7);
    if generator.clock(Some(u64::MAX)) != u64::MAX
        || generator.clock(None) != u64::MAX
        || generator.advance(5) != u64::MAX - 2
        || generator.advance(0) != 2
    {
        failed |= 1;
    }

    // Use the public SDK peer table as a real consumer of PeerMetadata.
    let peers = super::inbound::PeerTable::new_in(memory, 1)?;
    let peer = PeerRef {
        peer: u64::MAX - 1,
        incarnation: (1_u64 << 61) + 13,
    };
    if peers.admit(peer) != Ok(PeerAdmission::Current)
        || peers.admit(PeerRef {
            incarnation: u64::MAX,
            ..peer
        }) != Ok(PeerAdmission::Restarted)
        || peers.latest() != u64::MAX
        || peers.admit(peer) != Err(ReceiveError::StalePeer)
    {
        failed |= 2;
    }
    // Delayed loss must preserve the newer identity before any re-admission.
    if peers.forget_exact(peer) || peers.latest() != u64::MAX {
        failed |= 2;
    }
    if !peers.forget_exact(PeerRef { incarnation: u64::MAX, ..peer }) {
        failed |= 2;
    }
    if peers.admit(PeerRef {
        peer: 7,
        incarnation: 1,
    }) != Ok(PeerAdmission::Current)
        || peers.latest() != 1
    {
        failed |= 2;
    }

    let call = CallMetadata::new();
    call.reserve(u64::MAX, 1_u64 << 62, u64::MAX, PeerRef { peer: u64::MAX - 3, incarnation: 1_u64 << 60 });
    call.arm();
    let snapshot = call.snapshot();
    if snapshot.msb != u64::MAX
        || snapshot.lsb != 1_u64 << 62
        || snapshot.deadline != u64::MAX
        || snapshot.peer != u64::MAX - 3
        || snapshot.incarnation != 1_u64 << 60
        || call.fail_due(u64::MAX - 1)
        || call.fail_older(PeerRef { peer: u64::MAX - 3, incarnation: 1_u64 << 60 })
        || call.begin_delivery(u64::MAX, 0, PeerRef { peer: u64::MAX - 3, incarnation: 1_u64 << 60 }).is_some()
        || call.begin_delivery(u64::MAX, 1_u64 << 62, PeerRef { peer: u64::MAX - 3, incarnation: 1_u64 << 60 }).is_none()
        || call.fail_due(u64::MAX)
        || call.fail_peer(PeerRef { peer: u64::MAX - 3, incarnation: 1_u64 << 60 })
        || phase(call.state()) != DELIVERING
    {
        failed |= 4;
    }
    Ok(failed)
}

pub(super) struct Generator {
    #[cfg(all(up_rust_native32_model = "host", target_has_atomic = "64"))]
    sequence: AtomicU64,
    #[cfg(all(up_rust_native32_model = "host", target_has_atomic = "64"))]
    millis: AtomicU64,
    #[cfg(not(all(up_rust_native32_model = "host", target_has_atomic = "64")))]
    data: Leaf<(u64, u64)>,
}
impl core::fmt::Debug for Generator {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Generator").finish_non_exhaustive()
    }
}
impl Generator {
    pub(super) const fn new(sequence: u64, millis: u64) -> Self {
        Self {
            #[cfg(all(up_rust_native32_model = "host", target_has_atomic = "64"))]
            sequence: AtomicU64::new(sequence),
            #[cfg(all(up_rust_native32_model = "host", target_has_atomic = "64"))]
            millis: AtomicU64::new(millis),
            #[cfg(not(all(up_rust_native32_model = "host", target_has_atomic = "64")))]
            data: Leaf::new((sequence, millis)),
        }
    }
    pub(super) fn clock(&self, sampled: Option<u64>) -> u64 {
        #[cfg(all(up_rust_native32_model = "host", target_has_atomic = "64"))]
        {
            // clock and advance were already separate metadata operations.
            // SeqCst retains one order across both words without an owned gate.
            // A successful sample returns that sample, even if another caller
            // updates the fallback before this call returns. Rollback is valid.
            match sampled {
                Some(millis) => {
                    self.millis.store(millis, Ordering::SeqCst);
                    millis
                }
                None => self.millis.load(Ordering::SeqCst),
            }
        }
        #[cfg(not(all(up_rust_native32_model = "host", target_has_atomic = "64")))]
        // SAFETY: this nested closure uses only initialized scalar/Copy metadata,
        // bounds-safe slice iteration and crate-controlled phase helpers; no
        // callback, nested leaf, destructor, reference escape or handoff occurs.
        unsafe {
            self.data.apply(|v| {
                if let Some(millis) = sampled {
                    v.1 = millis;
                }
                v.1
            })
        }
    }
    pub(super) fn advance(&self, amount: u64) -> u64 {
        #[cfg(all(up_rust_native32_model = "host", target_has_atomic = "64"))]
        {
            // fetch_add returns the prior value and wraps exactly like the
            // original scalar transition; no caller retains exclusive access.
            self.sequence.fetch_add(amount, Ordering::SeqCst)
        }
        #[cfg(not(all(up_rust_native32_model = "host", target_has_atomic = "64")))]
        // SAFETY: this nested closure uses only initialized scalar/Copy metadata,
        // bounds-safe slice iteration and crate-controlled phase helpers; no
        // callback, nested leaf, destructor, reference escape or handoff occurs.
        unsafe {
            self.data.apply(|v| {
                let old = v.0;
                v.0 = old.wrapping_add(amount);
                old
            })
        }
    }
}

#[derive(Clone, Copy)]
struct Peer {
    id: u64,
    incarnation: u64,
}
struct Peers {
    // Safety invariant: this slice is the sole owner of initialized permanent
    // peer entries in one StartupMemory reservation. All access uses its Leaf;
    // no independently accessible mutable or shared reference aliases entries.
    entries: &'static mut [Peer],
}
pub(super) struct PeerMetadata {
    data: Leaf<Peers>,
}
impl PeerMetadata {
    pub(super) fn new_in(memory: &StartupMemory, count: usize) -> Result<Self, StartupError> {
        let storage = memory.slice::<Peer>(count.max(1))?;
        for slot in storage.iter_mut() {
            slot.write(Peer {
                id: 0,
                incarnation: 0,
            });
        }
        // SAFETY: every MaybeUninit<Peer> above now contains an initialized Peer;
        // both representations have identical layout/alignment. StartupMemory
        // transfers this disjoint permanent region exclusively; the original
        // slice is consumed here and all later mutation is under this Leaf gate.
        let entries = unsafe { &mut *(core::ptr::from_mut(storage) as *mut [Peer]) };
        Ok(Self {
            data: Leaf::new(Peers { entries }),
        })
    }
    pub(super) fn admit(&self, peer: PeerRef) -> Result<PeerAdmission, ReceiveError> {
        if peer.peer == 0 {
            return Ok(PeerAdmission::Unchecked);
        }
        // SAFETY: this nested closure uses only initialized scalar/Copy metadata,
        // bounds-safe slice iteration and crate-controlled phase helpers; no
        // callback, nested leaf, destructor, reference escape or handoff occurs.
        unsafe {
            self.data.apply(|table| {
                for entry in table.entries.iter_mut() {
                    if entry.id == peer.peer {
                        let previous = entry.incarnation;
                        entry.incarnation = previous.max(peer.incarnation);
                        return match previous.cmp(&peer.incarnation) {
                            core::cmp::Ordering::Less if previous == 0 => {
                                Ok(PeerAdmission::Current)
                            }
                            core::cmp::Ordering::Less => Ok(PeerAdmission::Restarted),
                            core::cmp::Ordering::Equal => Ok(PeerAdmission::Current),
                            core::cmp::Ordering::Greater => Err(ReceiveError::StalePeer),
                        };
                    }
                }
                for entry in table.entries.iter_mut() {
                    if entry.id == 0 {
                        *entry = Peer {
                            id: peer.peer,
                            incarnation: peer.incarnation,
                        };
                        return Ok(PeerAdmission::Current);
                    }
                }
                Err(ReceiveError::PeerTableFull)
            })
        }
    }
    /// Removes only the exact nonzero identity observed lost by the binding.
    ///
    /// Unknown incarnation zero is never permission to erase an entry. An exact
    /// removal releases the table entry; it retains no tombstone or peer history.
    // [impl->req~ssr-peer-stale-rejected~1]
    pub(super) fn forget(&self, peer: PeerRef) -> bool {
        if peer.peer == 0 || peer.incarnation == 0 {
            return false;
        }
        // SAFETY: this nested closure uses only initialized scalar/Copy metadata,
        // bounds-safe slice iteration and crate-controlled phase helpers; no
        // callback, nested leaf, destructor, reference escape or handoff occurs.
        unsafe {
            self.data.apply(|table| {
                for entry in table.entries.iter_mut() {
                    if entry.id == peer.peer && entry.incarnation == peer.incarnation {
                        *entry = Peer { id: 0, incarnation: 0 };
                        return true;
                    }
                }
                false
            })
        }
    }
    pub(super) fn latest(&self) -> u64 {
        // SAFETY: this closed closure copies initialized scalar metadata only;
        // iteration/comparison invokes no user code or nested Leaf and leaks no
        // reference, guard, destructor or execution-context handoff.
        unsafe {
            self.data.apply(|table| {
                table.entries.iter().map(|v| v.incarnation).max().unwrap_or(0)
            })
        }
    }
    /// Copies the sole identified peer; empty/ambiguous/zero-incarnation tables
    /// return UNKNOWN. No arbitrary winner is chosen between distinct peers.
    /// A copied identity is not a retained ownership or loss-event capability.
    /// In particular it cannot authenticate an outgoing request's destination.
    pub(super) fn single_peer(&self) -> PeerRef {
        // SAFETY: this closed closure only scans initialized scalar metadata and
        // copies scalar fields into an owned result. It calls no user code,
        // nested Leaf or destructor; no reference or handoff escapes.
        unsafe {
            self.data.apply(|table| {
                let mut observed = PeerRef::UNKNOWN;
                for entry in table.entries.iter() {
                    if entry.id == 0 { continue; }
                    if observed.peer != 0 || entry.incarnation == 0 {
                        return PeerRef::UNKNOWN;
                    }
                    observed = PeerRef { peer: entry.id, incarnation: entry.incarnation };
                }
                observed
            })
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct CallSnapshot {
    pub(super) state: u64,
    pub(super) msb: u64,
    pub(super) lsb: u64,
    pub(super) deadline: u64,
    pub(super) peer: u64,
    pub(super) incarnation: u64,
}
// Safety invariant CALL-METADATA: data's LEAF owns one coherent snapshot for
// this exact call slot. State includes the full phase/generation word; request
// identity, deadline and full peer identity belong to that snapshot. Only private
// RPC protocol operations change it. A copied snapshot alone grants no payload or
// pool-return authority; that additionally requires the caller's actual tenure.
pub(super) struct CallMetadata {
    data: Leaf<CallSnapshot>,
}
impl CallMetadata {
    /// # Safety-usable invariant
    /// Creates initialized FREE metadata in a fresh domain; grants no call reservation.
    pub(super) const fn new() -> Self {
        Self {
            data: Leaf::new(CallSnapshot {
                state: FREE,
                msb: 0,
                lsb: 0,
                deadline: 0,
                peer: 0,
                incarnation: 0,
            }),
        }
    }
    /// # Safety-usable invariant
    /// In one region stores the full request identity/deadline/peer and changes
    /// only the phase to RESERVED, preserving generation. Any zero identity word
    /// stores canonical UNKNOWN. This is scalar metadata, not evidence that the
    /// supplied peer was the send destination. This initializes metadata;
    /// the RPC caller separately owns the reserved pool slot and payload duties.
    pub(super) fn reserve(&self, msb: u64, lsb: u64, deadline: u64, peer: PeerRef) {
        // SAFETY: this nested closure uses only initialized scalar/Copy metadata,
        // bounds-safe slice iteration and crate-controlled phase helpers; no
        // callback, nested leaf, destructor, reference escape or handoff occurs.
        unsafe {
            self.data.apply(|m| {
                m.msb = msb;
                m.lsb = lsb;
                m.deadline = deadline;
                // Partial identities convey no destination authority. Keep a
                // canonical UNKNOWN pair so response matching cannot accidentally
                // treat just one known word as a bound destination.
                let identified = peer.peer != 0 && peer.incarnation != 0;
                m.peer = if identified { peer.peer } else { 0 };
                m.incarnation = if identified { peer.incarnation } else { 0 };
                m.state = with_phase(m.state, RESERVED);
            })
        };
    }
    /// # Safety-usable invariant
    /// Changes the current phase to ARMED, preserving generation and request fields.
    /// Releasing its gate publishes preceding slot initialization to later metadata
    /// acquisitions. The RPC caller owns setup of this reserved slot; no caller
    /// callback executes here and this helper alone grants no payload access.
    pub(super) fn arm(&self) {
        // SAFETY: this nested closure uses only initialized scalar/Copy metadata,
        // bounds-safe slice iteration and crate-controlled phase helpers; no
        // callback, nested leaf, destructor, reference escape or handoff occurs.
        unsafe { self.data.apply(|m| m.state = with_phase(m.state, ARMED)) };
    }
    /// # Safety-usable invariant
    /// Returns this slot's full current state under an acquired gate, observing
    /// preceding publication. The scalar is a snapshot, not a lasting ownership
    /// grant; payload access also needs the RPC caller's live tenure/phase contract.
    pub(super) fn state(&self) -> u64 {
        // SAFETY: this nested closure uses only initialized scalar/Copy metadata,
        // bounds-safe slice iteration and crate-controlled phase helpers; no
        // callback, nested leaf, destructor, reference escape or handoff occurs.
        unsafe { self.data.apply(|m| m.state) }
    }
    /// # Safety-usable invariant
    /// Copies one coherent complete snapshot under the acquired gate. After return
    /// it may be stale; the copy cannot protect storage or authorize payload access.
    pub(super) fn snapshot(&self) -> CallSnapshot {
        // SAFETY: this nested closure uses only initialized scalar/Copy metadata,
        // bounds-safe slice iteration and crate-controlled phase helpers; no
        // callback, nested leaf, destructor, reference escape or handoff occurs.
        unsafe { self.data.apply(|m| *m) }
    }
    /// # Safety-usable invariant
    /// Atomically with respect to this metadata domain replaces the full state
    /// word and publishes preceding writes on release. It neither validates a
    /// protocol transition nor creates payload/return authority; private RPC
    /// callers establish their phase/generation/tenure premises separately.
    pub(super) fn set_state(&self, state: u64) {
        // SAFETY: this nested closure uses only initialized scalar/Copy metadata,
        // bounds-safe slice iteration and crate-controlled phase helpers; no
        // callback, nested leaf, destructor, reference escape or handoff occurs.
        unsafe { self.data.apply(|m| m.state = state) };
    }
    /// # Safety-usable invariant
    /// Compares the entire state word (including generation) and replaces it with
    /// next iff equal, within one acquired region. Ok returns the exact replaced
    /// word; Err returns the observed word without mutation. Region release
    /// publishes preceding writes. The winner result distinguishes competing
    /// state transitions; payload/cleanup authority follows only with the RPC
    /// protocol's exact slot, legal transition and retained-tenure premises.
    pub(super) fn compare_state(&self, expected: u64, next: u64) -> Result<u64, u64> {
        // SAFETY: this nested closure uses only initialized scalar/Copy metadata,
        // bounds-safe slice iteration and crate-controlled phase helpers; no
        // callback, nested leaf, destructor, reference escape or handoff occurs.
        unsafe {
            self.data.apply(|m| {
                let actual = m.state;
                if actual == expected {
                    m.state = next;
                    Ok(actual)
                } else {
                    Err(actual)
                }
            })
        }
    }
    // Condition and phase change share one region; no stale scanner observation
    // is carried across reuse. Phase definitions are shared with rpc.rs.
    pub(super) fn fail_due(&self, now: u64) -> bool {
        self.fail_if(Some(now), TIMED_OUT)
    }
    /// # Safety-usable invariant
    /// Tests current ARMED phase and both stored peer words in one region. A
    /// nonzero event fails only a nonzero older incarnation of that same peer.
    /// True changes only phase to PEER_RESTARTED, retaining generation/request
    /// fields; false changes nothing. UNKNOWN and different peers never match.
    /// The result grants no payload or return authority.
    pub(super) fn fail_older(&self, peer: PeerRef) -> bool {
        if peer.peer == 0 || peer.incarnation == 0 {
            return false;
        }
        // SAFETY: this closed closure compares initialized primitive metadata
        // and uses phase/with_phase's documented callback-free scalar contract.
        // It performs no nested Leaf operation, callback, destructor or handoff,
        // and returns only an owned bool. PEER_RESTARTED fits PHASE_MASK; only
        // the phase changes, while generation and payload obligations remain.
        unsafe {
            self.data.apply(|m| {
                if phase(m.state) != ARMED || m.peer != peer.peer
                    || m.incarnation == 0 || m.incarnation >= peer.incarnation
                {
                    return false;
                }
                m.state = with_phase(m.state, PEER_RESTARTED);
                true
            })
        }
    }
    #[cfg(test)]
    pub(super) fn fail_all(&self) -> bool {
        self.fail_if(None, PEER_LOST)
    }
    /// Fails only a current ARMED call bound to this exact nonzero peer identity.
    ///
    /// # Safety-usable invariant
    /// The full peer predicate and phase transition occur inside one Leaf region.
    /// True means this slot's current ARMED tenure changed only its low phase
    /// bits to PEER_LOST, preserving generation, request fields and peer identity.
    /// False changes nothing. Unknown identities never match. No copied predicate
    /// survives across reuse, and the result grants no payload or return authority.
    // [impl->req~ssr-peer-failure-normal~1]
    pub(super) fn fail_peer(&self, peer: PeerRef) -> bool {
        if peer.peer == 0 || peer.incarnation == 0 {
            return false;
        }
        // SAFETY: the closed closure reads/writes initialized scalar metadata
        // only. Primitive comparisons and the documented phase/with_phase helpers
        // call no user code and cannot reenter a Leaf; no reference, destructor,
        // callback or handoff escapes. PEER_LOST fits PHASE_MASK, so with_phase
        // preserves the generation word. No payload ownership is transferred.
        unsafe {
            self.data.apply(|m| {
                if phase(m.state) != ARMED || m.peer != peer.peer
                    || m.incarnation != peer.incarnation
                {
                    return false;
                }
                m.state = with_phase(m.state, PEER_LOST);
                true
            })
        }
    }
    /// # Safety-usable invariant
    /// Tests phase and optional deadline predicate and changes the phase within
    /// one acquired region. True means this exact current ARMED tenure changed
    /// to next with generation/request fields preserved; false performs no write.
    /// It never uses a stale copied predicate or grants payload access. The
    /// private callers supply TIMED_OUT or test-only PEER_LOST.
    fn fail_if(&self, now: Option<u64>, next: u64) -> bool {
        // SAFETY: this nested closure uses only initialized scalar/Copy metadata,
        // bounds-safe slice iteration and crate-controlled phase helpers; no
        // callback, nested leaf, destructor, reference escape or handoff occurs.
        unsafe {
            self.data.apply(|m| {
                if phase(m.state) != ARMED
                    || now.is_some_and(|n| m.deadline > n)
                {
                    return false;
                }
                m.state = with_phase(m.state, next);
                true
            })
        }
    }
    /// # Safety-usable invariant
    /// In one acquired region tests ARMED, both request-ID words and the stored
    /// destination. A fully identified reservation requires the response's exact
    /// full peer. Canonical UNKNOWN retains route/request-ID correlation only;
    /// accepting such a response does not authenticate a destination or create
    /// recovery authority. Then changes only phase to DELIVERING.
    /// Some returns the exact prior full state;
    /// for that ARMED tenure only one caller can win. None changes nothing.
    /// The enclosing RPC protocol binds the result to this exact slot and grants
    /// its unique response-write obligation; a copied result is not a new claim.
    pub(super) fn begin_delivery(&self, msb: u64, lsb: u64, peer: PeerRef) -> Option<u64> {
        // SAFETY: this nested closure uses only initialized scalar/Copy metadata,
        // bounds-safe slice iteration and crate-controlled phase helpers; no
        // callback, nested leaf, destructor, reference escape or handoff occurs.
        unsafe {
            self.data.apply(|m| {
                let unbound = m.peer == 0 && m.incarnation == 0;
                let same_peer = m.peer == peer.peer && m.incarnation == peer.incarnation;
                if phase(m.state) != ARMED || m.msb != msb || m.lsb != lsb
                    || (!unbound && !same_peer)
                {
                    return None;
                }
                let state = m.state;
                m.state = with_phase(state, DELIVERING);
                Some(state)
            })
        }
    }
}

#[cfg(all(test, not(target_os = "none")))]
mod scoped_exclusion_tests {
    extern crate std;
    use super::*;
    use crate::memory::StartupRegion;
    use std::{
        sync::{mpsc, Barrier},
        thread,
        time::Duration,
    };

    #[test]
    fn full_peer_max_returns_actual_previous_classification_and_reuses_storage() {
        static REGION: StartupRegion<4096> = StartupRegion::new();
        let memory = REGION.claim().require_fixture();
        let peers = PeerMetadata::new_in(&memory, 1).require_fixture();
        let id = u64::MAX - 1;
        let old = (1_u64 << 61) + 13;
        assert_eq!(
            peers.admit(PeerRef {
                peer: id,
                incarnation: 0
            }),
            Ok(PeerAdmission::Current)
        );
        assert_eq!(
            peers.admit(PeerRef {
                peer: id,
                incarnation: old
            }),
            Ok(PeerAdmission::Current)
        );
        let barrier = Barrier::new(3);
        thread::scope(|s| {
            let run = || {
                barrier.wait();
                peers.admit(PeerRef {
                    peer: id,
                    incarnation: u64::MAX,
                })
            };
            let a = s.spawn(run);
            let b = s.spawn(run);
            barrier.wait();
            let answers = [a.join().require_fixture(), b.join().require_fixture()];
            assert_eq!(
                answers
                    .iter()
                    .filter(|x| **x == Ok(PeerAdmission::Restarted))
                    .count(),
                1
            );
            assert_eq!(
                answers
                    .iter()
                    .filter(|x| **x == Ok(PeerAdmission::Current))
                    .count(),
                1
            );
        });
        assert_eq!(peers.latest(), u64::MAX);
        assert_eq!(
            peers.admit(PeerRef {
                peer: id,
                incarnation: old
            }),
            Err(ReceiveError::StalePeer)
        );
        assert_eq!(
            peers.admit(PeerRef {
                peer: 7,
                incarnation: 0
            }),
            Err(ReceiveError::PeerTableFull)
        );
        assert!(peers.forget(PeerRef {
            peer: id,
            incarnation: u64::MAX,
        }));
        assert_eq!(
            peers.admit(PeerRef {
                peer: 7,
                incarnation: 1
            }),
            Ok(PeerAdmission::Current)
        );
        assert_eq!(peers.latest(), 1);
    }

    // [utest->req~ssr-peer-stale-rejected~1]
    #[test]
    fn late_loss_of_old_incarnation_cannot_clear_current_peer() {
        static REGION: StartupRegion<4096> = StartupRegion::new();
        let memory = REGION.claim().require_fixture();
        let peers = PeerMetadata::new_in(&memory, 1).require_fixture();
        let peer = 0x55_u64;
        peers.admit(PeerRef { peer, incarnation: 7 }).require_fixture();
        assert_eq!(peers.admit(PeerRef { peer, incarnation: 8 }), Ok(PeerAdmission::Restarted));
        assert!(!peers.forget(PeerRef { peer, incarnation: 7 }));
        // These observations precede re-admission and fail under ID-only removal.
        assert_eq!(peers.single_peer(), PeerRef { peer, incarnation: 8 });
        assert_eq!(peers.admit(PeerRef { peer, incarnation: 7 }), Err(ReceiveError::StalePeer));
        assert!(peers.forget(PeerRef { peer, incarnation: 8 }));
        assert_eq!(peers.admit(PeerRef { peer: 9, incarnation: 1 }), Ok(PeerAdmission::Current));
    }

    // [utest->req~ssr-peer-stale-rejected~1]
    #[test]
    fn unidentified_loss_is_not_a_wildcard() {
        static REGION: StartupRegion<4096> = StartupRegion::new();
        let memory = REGION.claim().require_fixture();
        let peers = PeerMetadata::new_in(&memory, 1).require_fixture();
        let current = PeerRef { peer: u64::MAX, incarnation: u64::MAX };
        peers.admit(current).require_fixture();
        assert!(!peers.forget(PeerRef::UNKNOWN));
        assert!(!peers.forget(PeerRef { incarnation: 0, ..current }));
        assert!(!peers.forget(PeerRef { peer: 0, ..current }));
        assert_eq!(peers.single_peer(), current);
        assert!(peers.forget(current));
        assert!(!peers.forget(current));
        assert_eq!(peers.single_peer(), PeerRef::UNKNOWN);
    }

    // [utest->req~ssr-peer-rpc-ends~1]
    #[test]
    fn ambiguous_peers_do_not_invent_a_call_destination() {
        static REGION: StartupRegion<4096> = StartupRegion::new();
        let memory = REGION.claim().require_fixture();
        let peers = PeerMetadata::new_in(&memory, 2).require_fixture();
        let a = PeerRef { peer: 1, incarnation: 7 };
        let b = PeerRef { peer: 2, incarnation: 9 };
        assert_eq!(peers.single_peer(), PeerRef::UNKNOWN);
        peers.admit(a).require_fixture();
        assert_eq!(peers.single_peer(), a);
        peers.admit(b).require_fixture();
        assert_eq!(peers.latest(), 9);
        assert_eq!(peers.single_peer(), PeerRef::UNKNOWN);
        assert!(peers.forget(b));
        assert_eq!(peers.single_peer(), a);
    }

    // [utest->req~ssr-peer-failure-normal~1]
    // [utest->req~ssr-peer-rpc-ends~1]
    #[test]
    fn loss_matches_full_peer_and_current_armed_tenure() {
        let call = CallMetadata::new();
        let current = PeerRef { peer: 42, incarnation: 8 };
        let armed = with_phase(19 << 8, ARMED);
        call.reserve(1, 2, 100, current);
        call.set_state(armed);
        for loss in [PeerRef::UNKNOWN,
            PeerRef { incarnation: 0, ..current },
            PeerRef { incarnation: 7, ..current },
            PeerRef { peer: 43, ..current }]
        {
            assert!(!call.fail_peer(loss));
            assert_eq!(call.state(), armed);
        }
        assert!(call.fail_peer(current));
        assert_eq!(call.state(), with_phase(armed, PEER_LOST));
        assert!(!call.fail_peer(current));
        // Reuse the metadata for a new tenure before a delayed older report.
        let newer = PeerRef { incarnation: 9, ..current };
        call.reserve(3, 4, 200, newer);
        let next = with_phase(20 << 8, ARMED);
        call.set_state(next);
        assert!(!call.fail_peer(current));
        assert_eq!(call.state(), next);
        assert!(call.fail_due(200));
        assert!(!call.fail_peer(newer));
        assert_eq!(phase(call.state()), TIMED_OUT);
    }

    // [utest->req~ssr-peer-rpc-ends~1]
    #[test]
    fn unknown_call_identity_remains_deadline_bound() {
        let call = CallMetadata::new();
        call.reserve(1, 2, 100, PeerRef::UNKNOWN);
        call.arm();
        assert!(!call.fail_peer(PeerRef { peer: 42, incarnation: 8 }));
        assert_eq!(phase(call.state()), ARMED);
        assert!(!call.fail_due(99));
        assert!(call.fail_due(100));
        assert_eq!(phase(call.state()), TIMED_OUT);
    }

    // [utest->req~ssr-peer-failure-normal~1]
    // [utest->req~ssr-peer-rpc-ends~1]
    #[test]
    fn g2_restart_matches_full_peer_and_leaves_unknown_calls_pending() {
        let a = PeerRef { peer: u64::MAX - 1, incarnation: (1_u64 << 62) + 7 };
        let newer = PeerRef { incarnation: a.incarnation + 1, ..a };
        for reserved in [a, PeerRef::UNKNOWN, PeerRef { peer: 17, ..a },
            PeerRef { incarnation: 0, ..a }, PeerRef { peer: 0, ..a }]
        {
            let call = CallMetadata::new();
            let armed = with_phase(27 << 8, ARMED);
            call.reserve(u64::MAX, 1 << 63, u64::MAX, reserved);
            call.set_state(armed);
            for event in [PeerRef::UNKNOWN, PeerRef { incarnation: 0, ..a },
                PeerRef { peer: 0, ..a }, a,
                PeerRef { incarnation: a.incarnation - 1, ..a }]
            {
                assert!(!call.fail_older(event));
                assert_eq!(call.state(), armed);
            }
            assert_eq!(call.fail_older(newer), reserved == a);
            assert_eq!(call.state(), with_phase(armed,
                if reserved == a { PEER_RESTARTED } else { ARMED }));
            assert!(!call.fail_older(newer));
            if reserved != a {
                assert!(!call.fail_due(u64::MAX - 1));
                assert!(call.fail_due(u64::MAX));
            }
        }
    }

    // [utest->req~ssr-peer-stale-rejected~1]
    // [utest->req~ssr-peer-rpc-ends~1]
    #[test]
    fn g2_response_claim_checks_full_peer_before_changing_phase() {
        let call = CallMetadata::new();
        let a = PeerRef { peer: u64::MAX - 1, incarnation: 1 << 61 };
        let armed = with_phase(37 << 8, ARMED);
        call.reserve(u64::MAX, 1 << 63, 100, a);
        call.set_state(armed);
        for response in [PeerRef::UNKNOWN, PeerRef { peer: 0, ..a },
            PeerRef { incarnation: 0, ..a }, PeerRef { peer: 5, ..a },
            PeerRef { incarnation: a.incarnation - 1, ..a },
            PeerRef { incarnation: a.incarnation + 1, ..a }]
        {
            assert_eq!(call.begin_delivery(u64::MAX, 1 << 63, response), None);
            assert_eq!(call.state(), armed);
        }
        assert_eq!(call.begin_delivery(u64::MAX - 1, 1 << 63, a), None);
        assert_eq!(call.begin_delivery(u64::MAX, (1 << 63) + 1, a), None);
        assert_eq!(call.begin_delivery(u64::MAX, 1 << 63, a), Some(armed));
        assert_eq!(call.state(), with_phase(armed, DELIVERING));
        assert!(!call.fail_older(PeerRef { incarnation: a.incarnation + 1, ..a }));
        assert_eq!(call.begin_delivery(u64::MAX, 1 << 63, a), None);
    }

    // [utest->req~ssr-peer-rpc-ends~1]
    #[test]
    fn g2_unbound_response_uses_correlation_without_creating_recovery_authority() {
        for reserved in [PeerRef::UNKNOWN, PeerRef { peer: 7, incarnation: 0 },
            PeerRef { peer: 0, incarnation: 9 }]
        {
            let call = CallMetadata::new();
            call.reserve(1, 2, 100, reserved);
            call.arm();
            let m = call.snapshot();
            assert_eq!((m.peer, m.incarnation), (0, 0));
            let response = PeerRef { peer: 7, incarnation: 9 };
            assert!(!call.fail_peer(response));
            assert!(!call.fail_older(response));
            assert_eq!(call.begin_delivery(1, 3, response), None);
            assert!(call.begin_delivery(1, 2, response).is_some());
            let m = call.snapshot();
            assert_eq!((m.peer, m.incarnation), (0, 0));
        }
    }

    // [utest->req~ssr-peer-stale-rejected~1]
    // Required acceptance control, deliberately expected to fail on this candidate.
    // This ignored test is NOT a pass or a substitute for retained-history design.
    #[test]
    #[ignore = "G2-TOMBSTONE-1 unresolved: exact history after slot reuse is absent"]
    fn g2_required_stale_history_survives_loss_and_unrelated_slot_reuse() {
        static REGION: StartupRegion<4096> = StartupRegion::new();
        let memory = REGION.claim().require_fixture();
        let peers = PeerMetadata::new_in(&memory, 1).require_fixture();
        let current = PeerRef { peer: 41, incarnation: 8 };
        let unrelated = PeerRef { peer: 42, incarnation: 1 };
        assert_eq!(peers.admit(current), Ok(PeerAdmission::Current));
        assert!(peers.forget(current));
        assert_eq!(peers.admit(unrelated), Ok(PeerAdmission::Current));
        assert!(!peers.forget(current)); // delayed loss must not erase unrelated row
        assert_eq!(peers.single_peer(), unrelated);
        assert!(peers.forget(unrelated));
        assert_eq!(peers.admit(PeerRef { incarnation: 7, ..current }),
            Err(ReceiveError::StalePeer));
        assert_eq!(peers.admit(current), Err(ReceiveError::StalePeer));
    }

    #[test]
    fn stopped_observer_after_copy_cannot_pin_unrelated_peer_storage() {
        static REGION: StartupRegion<4096> = StartupRegion::new();
        let memory = REGION.claim().require_fixture();
        let peers = PeerMetadata::new_in(&memory, 1).require_fixture();
        let a = PeerRef {
            peer: u64::MAX,
            incarnation: 42,
        };
        peers.admit(a).require_fixture();
        let (copied, copy) = mpsc::channel();
        let (resume, wait) = mpsc::channel();
        thread::scope(|s| {
            let peers = &peers;
            let observer = s.spawn(move || {
                let v = peers.latest();
                copied.send(v).require_fixture();
                wait.recv().require_fixture();
            });
            assert_eq!(copy.recv_timeout(Duration::from_secs(2)).require_fixture(), 42);
            peers.forget(a);
            assert_eq!(
                peers.admit(PeerRef {
                    peer: 7,
                    incarnation: u64::MAX
                }),
                Ok(PeerAdmission::Current)
            );
            resume.send(()).require_fixture();
            observer.join().require_fixture();
        });
    }

    #[test]
    fn stopped_holder_blocks_its_domain_but_not_an_independent_domain() {
        let held = Leaf::new(0_u64);
        let other = Generator::new(0, 0);
        let (entered, e) = mpsc::channel();
        let (release, r) = mpsc::channel();
        let (finished, f) = mpsc::channel();
        thread::scope(|s| {
            // Test-only controlled stall inside the private leaf. No product leaf
            // has a callback/channel or a caller-forgettable guard.
            let held = &held;
            let owner = s.spawn(move || {
                // SAFETY: Leaf::apply permits stalls in this host test. This local
                // closure uses only concrete unit-message channels and a u64 write;
                // it invokes no caller callback, reenters no Leaf and exposes no
                // protected alias. Its result is (). The private guard encloses
                // the mutable borrow through normal return or channel-error unwind.
                unsafe {
                    held.apply(|value| {
                        entered.send(()).require_fixture();
                        r.recv().require_fixture();
                        *value = 99;
                    })
                }
            });
            e.recv_timeout(Duration::from_secs(2)).require_fixture();
            let waiter = s.spawn(|| {
                // SAFETY: this local closure only copies the initialized u64.
                // It calls no code, cannot reenter a Leaf and returns no alias.
                // Leaf::apply acquires the same host gate before the borrow and
                // ends that borrow before releasing it; send occurs afterward.
                let v = unsafe { held.apply(|v| *v) };
                finished.send(v).require_fixture();
            });
            assert_eq!(other.clock(Some(u64::MAX)), u64::MAX);
            let observation = f.recv_timeout(Duration::from_millis(20));
            release.send(()).require_fixture(); // always release before any assertion can unwind
            owner.join().require_fixture();
            waiter.join().require_fixture();
            assert!(observation.is_err());
            assert_eq!(f.recv_timeout(Duration::from_secs(2)).require_fixture(), 99);
        });
    }

    #[test]
    fn clock_and_sequence_preserve_full_values_and_wrapping_recipe() {
        let g = Generator::new(u64::MAX - 2, 7);
        assert_eq!(g.clock(Some(u64::MAX)), u64::MAX);
        assert_eq!(g.clock(None), u64::MAX);
        assert_eq!(g.advance(5), u64::MAX - 2);
        assert_eq!(g.advance(0), 2);
        assert_eq!(g.clock(Some(1)), 1); // wall-clock rollback is preserved
        assert_eq!(g.clock(None), 1);
    }

    #[test]
    fn full_call_snapshot_and_predicates_remain_coherent_during_publication() {
        let call = CallMetadata::new();
        thread::scope(|s| {
            let writer = s.spawn(|| {
                for n in 1..1000_u64 {
                    call.reserve(n, !n, n.rotate_left(37), PeerRef { peer: n, incarnation: n ^ u64::MAX });
                    call.arm();
                }
            });
            for _ in 0..2000 {
                let m = call.snapshot();
                if m.state != FREE {
                    assert_eq!(
                        (m.lsb, m.deadline, m.peer, m.incarnation),
                        (!m.msb, m.msb.rotate_left(37), m.msb, m.msb ^ u64::MAX)
                    );
                }
            }
            writer.join().require_fixture();
        });
        call.reserve(u64::MAX, 1 << 62, u64::MAX, PeerRef { peer: 9, incarnation: 1 << 60 });
        call.arm();
        assert!(!call.fail_due(u64::MAX - 1));
        assert!(!call.fail_older(PeerRef { peer: 9, incarnation: 1 << 60 }));
        assert!(call.begin_delivery(u64::MAX, 0, PeerRef { peer: 9, incarnation: 1 << 60 }).is_none());
        assert!(call.begin_delivery(u64::MAX, 1 << 62, PeerRef { peer: 9, incarnation: 1 << 60 }).is_some());
        assert!(!call.fail_due(u64::MAX));
        assert!(!call.fail_all());
        assert_eq!(phase(call.snapshot().state), DELIVERING);
    }
}

#[cfg(all(test, up_rust_native32_model = "host", target_has_atomic = "64"))]
mod generator_atomic_tests {
    extern crate std;

    use super::Generator;
    use std::sync::Barrier;
    use std::thread;

    // This is a contention/semantic control, not heap instrumentation: test
    // threads allocate outside Generator. No allocation claim follows from it.
    // Partial req~ssr-ids-unique~2 evidence: counter credits only; UUID
    // collision bounds and platform clock qualification are outside this test.
    #[test]
    fn concurrent_generator_keeps_sequence_credits_and_whole_clock_samples() {
        const PER_THREAD: usize = 1024;
        const SEED: u64 = u64::MAX - 511;
        const SAMPLES: [u64; 4] = [
            0xffff_ffff_0000_0000,
            0x0000_0000_ffff_ffff,
            0xaaaa_aaaa_5555_5555,
            0x5555_5555_aaaa_aaaa,
        ];
        let generator = Generator::new(SEED, 0);
        let start = Barrier::new(SAMPLES.len());
        let mut credits = thread::scope(|scope| {
            let handles = SAMPLES.map(|sample| {
                let generator = &generator;
                let start = &start;
                scope.spawn(move || {
                    let mut credits = [0; PER_THREAD];
                    start.wait();
                    for credit in &mut credits {
                        assert_eq!(generator.clock(Some(sample)), sample);
                        assert!(SAMPLES.contains(&generator.clock(None)));
                        *credit = generator.advance(1).wrapping_sub(SEED);
                    }
                    credits
                })
            });
            handles.map(|handle| handle.join().expect("generator worker failed"))
        });
        credits.as_flattened_mut().sort_unstable();
        for (expected, &credit) in credits.as_flattened().iter().enumerate() {
            assert_eq!(credit, expected as u64);
        }
        assert_eq!(generator.advance(0), SEED.wrapping_add(4096));
    }
}
