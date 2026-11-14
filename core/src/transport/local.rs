// SPDX-License-Identifier: Apache-2.0
//! Local retained-owner mechanism, separate from public route admission.
//!
//! This EventOnly probe family has no deadline, RPC, carrier or shared-worker
//! conversion. It does not certify queue/QoS semantics or enable a binding's
//! refused native routes. Application handles erase the concrete binding/owner.
//! Startup backing is permanent; only individual retained owners are dropped.

use core::{cell::{Cell, UnsafeCell}, marker::PhantomData, mem::{self, MaybeUninit}, ptr};
use crate::{memory::{SetupClaim, StartupMemory}, payload::{loan::{borrow_checked, BorrowError}, stable::StablePayload},
    frame::header::HeaderLimits, uri::UUri, uattributes::UMessageType,
    wire::{InPlace, NativeWire, RoleWire, WirePayloadIdentity}};
use super::{LoanedPayload, PeerRef, ReceiveBudget, PayloadBytes,
    envelope::{AdmittedWire, Envelope}, frame::{RETAINED_FRAME_ALIGN, RETAINED_FRAME_CAPACITY}};

/// A fixed-size mechanism refusal; no owned error allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalError {
    /// Startup storage could not be reserved.
    Memory(crate::memory::StartupError),
    /// Configuration or frame identity is unsupported.
    Refused(&'static str),
    /// The configured held-owner budget is full; no raw take occurred.
    Full,
    /// Native representation validation failed.
    Payload(BorrowError),
    /// Underlying mechanism operation failed.
    Binding,
}
impl From<crate::memory::StartupError> for LocalError {
    fn from(value: crate::memory::StartupError) -> Self { Self::Memory(value) }
}

/// Local equivalent of Frame: byte borrows retain their owner, without Send.
/// Safe implementations are not trusted for semantic correctness or no-panic.
/// Native resource qualification remains a separate binding obligation.
pub trait LocalFrame: 'static {
    /// Borrow encoded metadata from this owner.
    fn encoded_metadata(&self) -> &[u8];
    /// Borrow original payload storage, including present-empty payloads.
    fn payload(&self) -> Option<LoanedPayload<'_>>;
    /// Peer identity observation.
    fn peer(&self) -> PeerRef;
}
impl<F: super::Frame> LocalFrame for F {
    fn encoded_metadata(&self) -> &[u8] { super::Frame::encoded_metadata(self) }
    fn payload(&self) -> Option<LoanedPayload<'_>> { super::Frame::payload(self) }
    fn peer(&self) -> PeerRef { super::Frame::peer(self) }
}

#[repr(C, align(16))]
struct Inline(UnsafeCell<[MaybeUninit<u8>; RETAINED_FRAME_CAPACITY]>);
struct Ops {
    // Safety invariant: each operation is for the same concrete F, whose sole
    // initialized value occupies a live aligned Inline. Shared calls require a
    // compatible borrow; drop consumes it only after every derived borrow ends.
    metadata: unsafe fn(*const u8) -> *const [u8],
    payload: unsafe fn(*const u8) -> Option<LoanedRaw>,
    peer: unsafe fn(*const u8) -> PeerRef,
    drop: unsafe fn(*mut u8),
}
type LoanedRaw = (*const [u8], super::PayloadLoanProvenance);

/// # Safety
/// p must address one live aligned F with a compatible shared borrow throughout
/// this call. Returned raw slice parts preserve that borrow, not a new lifetime;
/// F must remain immobile/alive for every later use. No ownership is transferred.
unsafe fn metadata<F: LocalFrame>(p: *const u8) -> *const [u8] {
    // SAFETY: caller supplies aligned initialized F and shared access. F's safe
    // return supplies a genuine slice; pointer conversion extends no lifetime.
    ptr::from_ref(unsafe { &*p.cast::<F>() }.encoded_metadata())
}
/// # Safety
/// Same live-F/shared-borrow preconditions and raw-result lifetime as metadata.
unsafe fn payload<F: LocalFrame>(p: *const u8) -> Option<LoanedRaw> {
    // SAFETY: live aligned F/shared access is the caller contract; no F reference
    // escapes except the raw parts of its safe returned slice.
    unsafe { &*p.cast::<F>() }.payload().map(|v| (ptr::from_ref(v.bytes()), v.provenance()))
}
/// # Safety
/// p must address one initialized aligned F under compatible shared access for
/// this call. The by-value result transfers no ownership or pointer guarantee.
unsafe fn peer<F: LocalFrame>(p: *const u8) -> PeerRef {
    // SAFETY: caller contract establishes the exact shared F borrow.
    unsafe { &*p.cast::<F>() }.peer()
}
/// # Safety
/// p must address the sole live initialized F in aligned writable storage. All
/// borrows must have ended. F is consumed even if Drop unwinds; never use it again.
unsafe fn destroy<F: LocalFrame>(p: *mut u8) {
    // SAFETY: the sole initialized F and ended borrows discharge drop_in_place;
    // inline storage stays allocated, while the F ownership ends here.
    unsafe { ptr::drop_in_place(p.cast::<F>()) };
}
struct OpsFor<F>(PhantomData<F>);
impl<F: LocalFrame> OpsFor<F> {
    const OPS: &'static Ops = &Ops { metadata: metadata::<F>, payload: payload::<F>, peer: peer::<F>, drop: destroy::<F> };
}

/// Inline local owner. Cannot become shared Frame or cross a thread boundary.
#[allow(missing_debug_implementations)]
pub struct LocalRetainedFrame {
    // Safety invariant: one initialized F matching ops, fitting/aligned within
    // this Inline; only this value owns F. Moving Self invalidates inline views.
    storage: Inline,
    // Safety invariant: exact operations for storage's F, unchanged through moves.
    ops: &'static Ops,
    _local: PhantomData<*mut ()>,
}
impl LocalRetainedFrame {
    /// Erases one local owner without heap allocation. Oversized owners fail at
    /// compile time. No view is captured until the owner reaches its final slot.
    pub fn new<F: LocalFrame>(frame: F) -> Self {
        const { assert!(mem::size_of::<F>() <= RETAINED_FRAME_CAPACITY); assert!(mem::align_of::<F>() <= RETAINED_FRAME_ALIGN); }
        let storage = Inline(UnsafeCell::new([MaybeUninit::uninit(); RETAINED_FRAME_CAPACITY]));
        // SAFETY: repr alignment and const bounds give one aligned writable F
        // extent, with no old F to drop. write transfers the sole F into storage.
        unsafe { ptr::write(storage.0.get().cast::<F>(), frame) };
        Self { storage, ops: OpsFor::<F>::OPS, _local: PhantomData }
    }
    /// # Safety-usable invariant
    /// The pointer addresses storage's initialized aligned F while self remains
    /// alive/immobile; it conveys no access beyond a compatible self borrow.
    fn owner(&self) -> *const u8 { self.storage.0.get().cast::<u8>() }
    fn metadata(&self) -> &[u8] {
        // SAFETY: paired ops/storage and this local shared borrow discharge the
        // F operation. Self is !Send/!Sync; raw result remains borrowed from self.
        let p = unsafe { (self.ops.metadata)(self.owner()) };
        // SAFETY: the op returns genuine slice parts from the still-live unmoved
        // F. The output lifetime is exactly this self borrow, with no mutation.
        unsafe { &*p }
    }
    fn body(&self) -> Option<LoanedPayload<'_>> {
        // SAFETY: identical live-F/shared local borrow contract to metadata.
        let raw = unsafe { (self.ops.payload)(self.owner()) };
        raw.map(|(p, provenance)| {
            // SAFETY: op returned a genuine slice borrowing immobile live self;
            // zero-length pointers retain their original nonnull provenance.
            LoanedPayload::new(unsafe { &*p }, provenance)
        })
    }
    fn sender(&self) -> PeerRef {
        // SAFETY: paired F/ops and local shared self borrow remain valid.
        unsafe { (self.ops.peer)(self.owner()) }
    }
}
impl Drop for LocalRetainedFrame {
    fn drop(&mut self) {
        // SAFETY: this sole owner has exclusive Drop access after all safe views
        // ended. The op consumes F once; no old F access follows on unwind.
        unsafe { (self.ops.drop)(self.storage.0.get().cast::<u8>()) };
    }
}

/// Initialization side of an explicitly scoped EventOnly owner mechanism.
/// This is not an alternate route-admission capability or QoS declaration.
pub trait LocalBindingSetup: 'static {
    /// Actual runtime form, with no setup/teardown surface.
    type Running: LocalReceive;
    /// Reserved foreign retained-owner capacity.
    fn held_capacity(&self) -> usize;
    /// Completes lazy initialization and freezes permanent owners. Failure is
    /// still setup. Implementations must finish their own fallible reservations
    /// before publishing permanent foreign state. The SDK reserves placement for
    /// Running and its RuntimeState before calling this method. Resource/native
    /// qualification remains separate for each concrete binding; this safe trait
    /// is not trusted for memory-safety or no-panic proof of arbitrary user code.
    /// No local runtime may later destroy its foreign backing.
    fn activate(self) -> Result<Self::Running, LocalError>;
}
/// Runtime mechanism, retaining no caller borrows. No queue-semantics claim.
pub trait LocalReceive: 'static {
    /// Debit this same SDK budget before each raw attempt. Native qualified
    /// implementations must not allocate/free/panic or block on release.
    fn receive(&self, budget: &ReceiveBudget) -> Result<Option<LocalRetainedFrame>, LocalError>;
}
trait ErasedSetup {
    fn activate(&self, memory: &StartupMemory) -> Result<&'static dyn LocalReceive, LocalError>;
    fn cancel(&self);
}
struct SetupHolder<S>(Cell<Option<S>>);
impl<S: LocalBindingSetup> ErasedSetup for SetupHolder<S> {
    fn activate(&self, memory: &StartupMemory) -> Result<&'static dyn LocalReceive, LocalError> {
        // [impl->req~ssr-mode-entry~4] partial: Running is reserved before the setup is consumed
        // [impl->req~ssr-memory-static~1] partial: checked placement from caller startup memory
        // Complete the last generic Running reservation before consuming setup
        // or allowing its activation to publish permanent foreign state. write
        // below only initializes this exclusive permanent reservation.
        let slot = memory.place(MaybeUninit::<S::Running>::uninit())?;
        let setup = self.0.take().ok_or(LocalError::Refused("setup already consumed"))?;
        let running = setup.activate()?;
        Ok(slot.write(running))
    }
    fn cancel(&self) { drop(self.0.take()); }
}
struct Identity(u8);
struct Slot {
    // Safety invariant: claimed excludes all other producers/leases. True
    // initialized means owner contains one live final-address LocalRetainedFrame.
    claimed: Cell<bool>,
    initialized: Cell<bool>,
    owner: UnsafeCell<MaybeUninit<LocalRetainedFrame>>,
}
struct Claim { slot: &'static Slot }
impl Claim {
    fn install(&mut self, owner: LocalRetainedFrame) {
        // SAFETY: an unpublished Claim exclusively owns this vacant slot; no
        // initialized owner exists. write moves F without dropping old bytes.
        unsafe { (*self.slot.owner.get()).write(owner) };
        self.slot.initialized.set(true);
    }
    fn owner(&self) -> Option<&LocalRetainedFrame> {
        if !self.slot.initialized.get() { return None; }
        // SAFETY: initialized establishes one live final owner; the sole Claim
        // keeps it unmoved until Drop. No mutable owner API is exposed.
        Some(unsafe { (*self.slot.owner.get()).assume_init_ref() })
    }
}
impl Drop for Claim {
    fn drop(&mut self) {
        if self.slot.initialized.replace(false) {
            // SAFETY: this sole Claim owns the initialized final owner and all
            // safe views ended before its Drop. Flag is cleared before call; if
            // foreign/caller Drop unwinds, claimed stays true and cannot be reused.
            unsafe { (*self.slot.owner.get()).assume_init_drop() };
        }
        // Release credit only AFTER complete owner/foreign destruction.
        self.slot.claimed.set(false);
    }
}
struct RuntimeState {
    identity: &'static Identity,
    binding: &'static dyn LocalReceive,
    slots: &'static [Slot],
}

/// Non-generic setup-only local handle. No traffic method exists on this type.
#[allow(missing_debug_implementations)]
pub struct LocalTransportSetup {
    memory: StartupMemory,
    _claim: SetupClaim,
    identity: &'static Identity,
    setup: &'static dyn ErasedSetup,
    slots: &'static [Slot],
    activated: bool,
    _local: PhantomData<*mut ()>,
}
impl LocalTransportSetup {
    /// Reserves fixed owner slots in normal SDK startup memory. Does not activate
    /// traffic or certify any public native route. Held count is not unread depth.
    pub fn new_in<S: LocalBindingSetup>(memory: &StartupMemory, binding: S) -> Result<Self, LocalError> {
        let capacity = binding.held_capacity();
        if capacity == 0 { return Err(LocalError::Refused("zero held-owner capacity")); }
        let (memory, claim) = memory.share_for_setup()?;
        let identity = memory.place(Identity(0))?;
        let mut slots = memory.fixed_vec(capacity)?;
        for _ in 0..capacity { slots.push(Slot { claimed: Cell::new(false), initialized: Cell::new(false), owner: UnsafeCell::new(MaybeUninit::uninit()) })?; }
        let setup = memory.place(SetupHolder(Cell::new(Some(binding))))?;
        Ok(Self { memory, _claim: claim, identity, setup, slots: slots.into_static_slice(), activated: false, _local: PhantomData })
    }
    /// Prepares one checked native EventOnly role. No try_receive until explicit
    /// activation; monitored/QoS routes are not admitted by this mechanism API.
    pub fn native<T: StablePayload>(&self, wire: NativeWire, source: UUri) -> Result<LocalNativeSetup<T>, LocalError> {
        if !T::FIELD_BITS_ALWAYS_VALID || mem::needs_drop::<T>() { return Err(LocalError::Refused("native all-bit-patterns-valid profile required")); }
        let info = <NativeWire as RoleWire<T, InPlace>>::__describe(&wire).map_err(LocalError::Refused)?;
        let admitted = AdmittedWire::admit(&self.memory, Envelope::UFrameFields, info.wire_id, info.payload_family, info.identity)
            .map_err(|_| LocalError::Refused("native envelope preparation"))?;
        Ok(LocalNativeSetup { identity: self.identity, admitted, expected: info.identity, source, _value: PhantomData, _local: PhantomData })
    }
    /// Consumes initialization, reserves permanent runtime state and seals SDK
    /// setup memory when the last setup ends. Runtime-handle Drop owns no backing.
    pub fn enter_runtime(mut self) -> Result<LocalTransportRuntime, LocalError> {
        // Partial mappings for this local mechanism only: it checks no clock, QoS, waiver or
        // strict-mode entry condition, and its errors name no route or setter.
        // [impl->req~ssr-mode-entry~4] partial: fallible reservations precede binding activation
        // [impl->req~ssr-mode-init-failure~2] partial: each refusal returns with no runtime value
        // [impl->req~ssr-memory-static~1] partial: checked placement from caller startup memory
        // Reserve RuntimeState before the erased holder reserves Running and
        // calls the binding. Either reservation may refuse while setup is still
        // cancellable. After binding activation, only infallible writes/atomics
        // remain; there is no further startup allocator call.
        let state_slot = self.memory.place(MaybeUninit::<RuntimeState>::uninit())?;
        let binding = self.setup.activate(&self.memory)?;
        let state = state_slot.write(RuntimeState { identity: self.identity, binding, slots: self.slots });
        self.activated = true;
        self.memory.note_runtime_started();
        Ok(LocalTransportRuntime { state, _local: PhantomData })
    }
}
impl Drop for LocalTransportSetup {
    fn drop(&mut self) { if !self.activated { self.setup.cancel(); } }
}
/// Permanent local runtime handle; no resource-owning destructor or setup API.
#[allow(missing_debug_implementations)]
pub struct LocalTransportRuntime { state: &'static RuntimeState, _local: PhantomData<*mut ()> }
/// Setup-only role with no traffic methods and no binding/wire type parameter.
#[allow(missing_debug_implementations)]
pub struct LocalNativeSetup<T: StablePayload> {
    identity: &'static Identity, admitted: AdmittedWire, expected: WirePayloadIdentity,
    source: UUri, _value: PhantomData<fn() -> T>, _local: PhantomData<*mut ()>,
}
impl<T: StablePayload> LocalNativeSetup<T> {
    /// Activates only against the matching local runtime, before traffic escapes.
    pub fn activate(self, runtime: &LocalTransportRuntime) -> Result<LocalNativeRole<T>, LocalError> {
        if !ptr::eq(self.identity, runtime.state.identity) { return Err(LocalError::Refused("wrong local runtime")); }
        Ok(LocalNativeRole { state: runtime.state, admitted: self.admitted, expected: self.expected, source: self.source, _value: PhantomData, _local: PhantomData })
    }
}
/// Active EventOnly native owner probe. Not a public subscriber/QoS admission.
#[allow(missing_debug_implementations)]
pub struct LocalNativeRole<T: StablePayload> {
    state: &'static RuntimeState, admitted: AdmittedWire, expected: WirePayloadIdentity,
    source: UUri, _value: PhantomData<fn() -> T>, _local: PhantomData<*mut ()>,
}
impl<T: StablePayload> LocalNativeRole<T> {
    /// Claims a final SDK slot before one bounded underlying attempt. Full makes
    /// zero raw attempts. Validation failure releases only this unpublished owner.
    pub fn try_receive(&self) -> Result<Option<LocalReceived<T>>, LocalError> {
        let slot = self.state.slots.iter().find(|s| !s.claimed.replace(true)).ok_or(LocalError::Full)?;
        let mut claim = Claim { slot };
        let budget = ReceiveBudget::new(1);
        let Some(owner) = self.state.binding.receive(&budget)? else { return Ok(None); };
        claim.install(owner);
        {
            let owner = claim.owner().ok_or(LocalError::Binding)?;
            let payload = owner.body().ok_or(LocalError::Refused("native payload absent"))?;
            let parsed = Envelope::UFrameFields.parse(core::slice::from_ref(&self.admitted), owner.metadata(), PayloadBytes::one(payload.bytes()), &HeaderLimits::default())
                .map_err(|_| LocalError::Refused("malformed native metadata"))?;
            if parsed.header.kind != UMessageType::Publish || parsed.header.source != self.source || parsed.header.sink.is_some()
                || parsed.header.payload_encoding != Some(self.expected.encoding) || parsed.header.native_type_token != self.expected.native_type_token {
                return Err(LocalError::Refused("native route/type identity mismatch"));
            }
            borrow_checked::<T>(payload.bytes()).map_err(LocalError::Payload)?;
        }
        Ok(Some(LocalReceived { claim, _value: PhantomData, _local: PhantomData }))
    }
}
/// One original local native owner, independent of the role/runtime handle.
/// Forget withholds its slot; Drop completes the foreign release before reuse.
#[allow(missing_debug_implementations)]
pub struct LocalReceived<T: StablePayload> { claim: Claim, _value: PhantomData<fn() -> T>, _local: PhantomData<*mut ()> }
impl<T: StablePayload> LocalReceived<T> {
    /// Borrows directly from the retained original payload, checking representation.
    pub fn payload(&self) -> Result<&T, LocalError> {
        let owner = self.claim.owner().ok_or(LocalError::Binding)?;
        let bytes = owner.body().ok_or(LocalError::Refused("native payload absent"))?;
        borrow_checked(bytes.bytes()).map_err(LocalError::Payload)
    }
    /// Original payload slice for address/extent evidence, never a copied buffer.
    pub fn payload_bytes(&self) -> Option<&[u8]> { self.claim.owner()?.body().map(|p| p.bytes()) }
    /// Reported peer observation; this does not qualify restart identity.
    pub fn peer(&self) -> Option<PeerRef> { self.claim.owner().map(LocalRetainedFrame::sender) }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
