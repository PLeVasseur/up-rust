// SPDX-License-Identifier: Apache-2.0
//! New local owner controls. No middleware or native qualification is inferred.
extern crate std;
use super::*;
use crate::test_support::FixtureResult;
use std::{boxed::Box, rc::Rc, panic::{catch_unwind, AssertUnwindSafe}};

struct Owner { drops: Rc<Cell<usize>>, panic_drop: bool, bytes: [u8; 8] }
impl LocalFrame for Owner {
    fn encoded_metadata(&self) -> &[u8] { &[] }
    fn payload(&self) -> Option<LoanedPayload<'_>> { Some(LoanedPayload::new(&self.bytes, super::super::PayloadLoanProvenance::OpaqueTransportLoan)) }
    fn peer(&self) -> PeerRef { PeerRef::UNKNOWN }
}
impl Drop for Owner {
    #[expect(clippy::panic, reason = "Intentional adversarial safe Drop checks that unwinding cannot release or reuse a live owner slot")]
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
        if self.panic_drop { std::panic!("intentional hostile safe Drop"); }
    }
}
fn slot() -> &'static Slot {
    Box::leak(Box::new(Slot { claimed: Cell::new(true), initialized: Cell::new(false), owner: UnsafeCell::new(MaybeUninit::uninit()) }))
}
#[test]
fn local_erasure_moves_inline_owner_without_duplicate_drop() {
    let drops = Rc::new(Cell::new(0));
    let erased = LocalRetainedFrame::new(Owner { drops: drops.clone(), panic_drop: false, bytes: [17; 8] });
    let mut claim = Claim { slot: slot() };
    claim.install(erased);
    assert_eq!(claim.owner().require_fixture().body().require_fixture().bytes(), &[17; 8]);
    assert_eq!(drops.get(), 0);
    let where_owned = claim.slot;
    drop(claim);
    assert_eq!(drops.get(), 1);
    assert!(!where_owned.claimed.get());
    assert!(!where_owned.initialized.get());
}
#[test]
fn local_forget_keeps_owner_and_credit_claimed() {
    let drops = Rc::new(Cell::new(0));
    let mut claim = Claim { slot: slot() };
    claim.install(LocalRetainedFrame::new(Owner { drops: drops.clone(), panic_drop: false, bytes: [9; 8] }));
    let where_owned = claim.slot;
    mem::forget(claim);
    assert_eq!(drops.get(), 0);
    assert!(where_owned.claimed.get());
    assert!(where_owned.initialized.get());
}
#[test]
fn hostile_drop_unwind_cannot_reuse_or_drop_old_owner_twice() {
    let drops = Rc::new(Cell::new(0));
    let mut claim = Claim { slot: slot() };
    claim.install(LocalRetainedFrame::new(Owner { drops: drops.clone(), panic_drop: true, bytes: [4; 8] }));
    let where_owned = claim.slot;
    assert!(catch_unwind(AssertUnwindSafe(|| drop(claim))).is_err());
    assert_eq!(drops.get(), 1);
    assert!(where_owned.claimed.get());
    assert!(!where_owned.initialized.get());
}
struct Setup { cancelled: Rc<Cell<usize>>, activated: Rc<Cell<usize>> }
struct Running;
impl Drop for Setup { fn drop(&mut self) { self.cancelled.set(self.cancelled.get() + 1); } }
impl LocalBindingSetup for Setup {
    type Running = Running;
    fn held_capacity(&self) -> usize { 2 }
    fn activate(self) -> Result<Running, LocalError> { self.activated.set(self.activated.get()+1); Ok(Running) }
}
impl LocalReceive for Running {
    fn receive(&self, _: &ReceiveBudget) -> Result<Option<LocalRetainedFrame>, LocalError> { Ok(None) }
}
#[test]
fn setup_cancel_consumes_binding_once_without_activation() {
    let cancelled = Rc::new(Cell::new(0));
    let activated = Rc::new(Cell::new(0));
    let memory = Box::leak(Box::new(crate::memory::StartupRegion::<8192>::new())).claim().require_fixture();
    let setup = LocalTransportSetup::new_in(&memory, Setup { cancelled: cancelled.clone(), activated: activated.clone() }).require_fixture();
    drop(setup);
    assert_eq!(cancelled.get(), 1);
    assert_eq!(activated.get(), 0);
}
#[test]
fn activation_retains_runtime_after_handle_drop() {
    let cancelled = Rc::new(Cell::new(0));
    let activated = Rc::new(Cell::new(0));
    let memory = Box::leak(Box::new(crate::memory::StartupRegion::<8192>::new())).claim().require_fixture();
    let setup = LocalTransportSetup::new_in(&memory, Setup { cancelled, activated: activated.clone() }).require_fixture();
    let runtime = setup.enter_runtime().require_fixture();
    let state = runtime.state;
    assert_eq!(activated.get(), 1);
    drop(runtime);
    assert!(state.binding.receive(&ReceiveBudget::new(1)).require_fixture().is_none());
}

// TRACE-1 tags for the four activation-preflight controls below: partial evidence only.
// Host fixtures with heap-leaked startup regions and safe test bindings. Not shown: route
// or setter naming, the other mode-entry conditions, sealing when the last setup enters,
// static-only memory, restart or successor recovery, foreign activation, native resources.
// [utest->req~ssr-mode-entry~4] method: test; partial evidence: refused entry, no runtime value
// [utest->req~ssr-mode-init-failure~2] method: test; partial evidence: error, no activation, one cancel
// [utest->req~ssr-memory-sealed~3] method: test; partial evidence: no placement after an early seal
#[test]
fn preflight_sealed_runtime_state_never_activates_binding() {
    let cancelled = Rc::new(Cell::new(0));
    let activated = Rc::new(Cell::new(0));
    let memory = Box::leak(Box::new(crate::memory::StartupRegion::<8192>::new())).claim().require_fixture();
    let setup = LocalTransportSetup::new_in(&memory, Setup { cancelled: cancelled.clone(), activated: activated.clone() }).require_fixture();
    let _ = memory.seal();
    assert!(matches!(setup.enter_runtime(), Err(LocalError::Memory(crate::memory::StartupError::RegionSealed))));
    assert_eq!(activated.get(), 0);
    assert_eq!(cancelled.get(), 1);
}

struct LargeRunning([u8; 16384]);
impl LocalReceive for LargeRunning {
    fn receive(&self, _: &ReceiveBudget) -> Result<Option<LocalRetainedFrame>, LocalError> {
        let _ = &self.0;
        Ok(None)
    }
}
struct LargeSetup { cancelled: Rc<Cell<usize>>, activated: Rc<Cell<usize>> }
impl Drop for LargeSetup { fn drop(&mut self) { self.cancelled.set(self.cancelled.get() + 1); } }
impl LocalBindingSetup for LargeSetup {
    type Running = LargeRunning;
    fn held_capacity(&self) -> usize { 1 }
    fn activate(self) -> Result<LargeRunning, LocalError> {
        self.activated.set(self.activated.get() + 1);
        Ok(LargeRunning([0; 16384]))
    }
}
// [utest->req~ssr-mode-entry~4] method: test; partial evidence: Running exhaustion refuses entry
// [utest->req~ssr-mode-init-failure~2] method: test; partial evidence: error before activation, one cancel
#[test]
fn preflight_running_exhaustion_cancels_before_foreign_commit() {
    let cancelled = Rc::new(Cell::new(0));
    let activated = Rc::new(Cell::new(0));
    let memory = Box::leak(Box::new(crate::memory::StartupRegion::<4096>::new())).claim().require_fixture();
    let setup = LocalTransportSetup::new_in(&memory, LargeSetup { cancelled: cancelled.clone(), activated: activated.clone() }).require_fixture();
    assert!(matches!(setup.enter_runtime(), Err(LocalError::Memory(crate::memory::StartupError::MemoryExhausted { .. }))));
    assert_eq!(activated.get(), 0);
    assert_eq!(cancelled.get(), 1);
}

struct RefusingSetup { cancelled: Rc<Cell<usize>>, attempted: Rc<Cell<usize>> }
impl Drop for RefusingSetup { fn drop(&mut self) { self.cancelled.set(self.cancelled.get() + 1); } }
impl LocalBindingSetup for RefusingSetup {
    type Running = Running;
    fn held_capacity(&self) -> usize { 1 }
    fn activate(self) -> Result<Running, LocalError> {
        self.attempted.set(self.attempted.get() + 1);
        Err(LocalError::Binding)
    }
}
// [utest->req~ssr-mode-entry~4] method: test; partial evidence: binding refusal, no runtime value
// [utest->req~ssr-mode-init-failure~2] method: test; partial evidence: error, consumed setup dropped once
#[test]
fn preflight_binding_refusal_cancels_once_without_runtime_publication() {
    let cancelled = Rc::new(Cell::new(0));
    let attempted = Rc::new(Cell::new(0));
    let memory = Box::leak(Box::new(crate::memory::StartupRegion::<8192>::new())).claim().require_fixture();
    let setup = LocalTransportSetup::new_in(&memory, RefusingSetup { cancelled: cancelled.clone(), attempted: attempted.clone() }).require_fixture();
    assert!(matches!(setup.enter_runtime(), Err(LocalError::Binding)));
    assert_eq!(attempted.get(), 1);
    assert_eq!(cancelled.get(), 1);
}

struct SealingSetup { memory: StartupMemory }
impl LocalBindingSetup for SealingSetup {
    type Running = Running;
    fn held_capacity(&self) -> usize { 1 }
    fn activate(self) -> Result<Running, LocalError> {
        let _ = self.memory.seal();
        Ok(Running)
    }
}
// [utest->req~ssr-mode-entry~4] method: test; partial evidence: no placement after binding activation
// [utest->req~ssr-memory-sealed~3] method: test; partial evidence: placement after the seal is refused
#[test]
fn preflight_commit_does_not_reserve_after_binding_activation() {
    let memory = Box::leak(Box::new(crate::memory::StartupRegion::<8192>::new())).claim().require_fixture();
    let (memory_to_seal, _extra_setup_claim) = memory.share_for_setup().require_fixture();
    let setup = LocalTransportSetup::new_in(&memory, SealingSetup { memory: memory_to_seal }).require_fixture();
    // activate seals all new placements. Returning a runtime proves the SDK
    // commit uses only the previously reserved Running/RuntimeState storage.
    let runtime = setup.enter_runtime().require_fixture();
    assert!(runtime.state.binding.receive(&ReceiveBudget::new(1)).require_fixture().is_none());
    assert!(matches!(memory.place(1u8), Err(crate::memory::StartupError::RegionSealed)));
}
