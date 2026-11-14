//! Domain-only encoded publishers, prepared before traffic becomes possible.
//!
//! A schema implements [`PublisherMapping`] once. Deployment selects that schema
//! profile and a prepared transport; stored [`Publisher<T>`] contains neither.
//! This is the encoded path only. It never falls back from native ownership.
#![allow(clippy::result_large_err)] // Setup errors preserve the core's inline route diagnosis.

use core::{fmt, time::Duration};
use up_rust_core::communication::{two_mode, LocalUriProvider};
use up_rust_core::transport::{
    Envelope, Qos, RouteCounters, RouteId, SendError, SendReport, StartupError,
    TransportRuntime, TransportSetup,
};
use up_rust_core::uattributes::{PayloadEncoding, UPriority};
use up_rust_core::wire::{EncodedSize, PayloadWriter, Ready, StartupSlot, Wire, WireError, WireInfo};

/// Runtime failure shared by host and embedded publishers. Contains no heap data.
/// A refusal is not a promise that no peer observed a send; retry policy belongs
/// to the application, particularly for partial delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PublishError {
    /// The exact bounded core refusal, including clock/ID/retirement failures.
    Send(SendError),
    /// The binding accepted the operation but reported queue-full discards.
    /// Some peers may already have received it. No automatic retry is performed.
    Dropped(SendReport),
}

impl fmt::Display for PublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Send(error) => write!(f, "{error}"),
            Self::Dropped(report) if report.counts_exact => write!(
                f, "publication accepted by {} routes; {} queue-full discards",
                report.delivered, report.dropped_full,
            ),
            Self::Dropped(report) => write!(
                f, "publication accepted by at least {} routes; at least {} queue-full discards (incomplete counts)",
                report.delivered, report.dropped_full,
            ),
        }
    }
}
impl core::error::Error for PublishError {}
impl From<SendError> for PublishError {
    fn from(error: SendError) -> Self { Self::Send(error) }
}

/// Reservations and behavior fixed during initialization, on every target.
/// Defaults are the core's defaults, including four send slots and `Qos::DEFAULT`.
#[derive(Clone, Copy, Debug)]
pub struct PublisherOptions {
    /// Preallocated send slots; zero is refused by core setup.
    pub slots: usize,
    /// Explicit TX payload bound, or the selected wire's declared bound.
    pub max_payload: Option<usize>,
    /// Optional per-route priority, admitted by the binding.
    pub priority: Option<UPriority>,
    /// Message lifetime, using the core's common clock/error contract.
    pub ttl: Option<Duration>,
    /// Envelope selection independent of payload encoding.
    pub envelope: Option<Envelope>,
    /// Unchanged core QoS, including any synchronous wait policy.
    pub qos: Qos,
    /// Refuse a binding that cannot lend its TX storage.
    pub require_lending: bool,
}
impl Default for PublisherOptions {
    fn default() -> Self {
        Self { slots: 4, max_payload: None, priority: None, ttl: None,
               envelope: None, qos: Qos::DEFAULT, require_lending: false }
    }
}

/// Schema-owner extension point: resolve a real mapping from deployment data.
/// An unsupported profile must refuse; it must not silently select another wire.
/// This safe trait is not a memory-safety contract. Core checks writer ownership,
/// initialization and capacities even when a safe implementation lies.
/// Heap/panic/progress qualification of an admitted implementation is additional
/// product evidence; the SDK cannot enforce those semantic promises by this type.
pub trait PublisherMapping<T: 'static> {
    /// Select the schema's persistent wire or return an explicit setup refusal.
    fn publisher_wire(&self) -> Result<&'static dyn Wire<T>, StartupError>;
}

/// Initialization-only borrowing of transport, address and schema selection.
/// `M` is deployment/schema data; it never appears in stored application handles.
pub struct PublisherDeployment<'a, M> {
    setup: &'a TransportSetup,
    address: &'a dyn LocalUriProvider,
    mapping: M,
}
impl<'a, M> PublisherDeployment<'a, M> {
    /// Bind actual initialization state and address to a meaningful schema profile.
    /// This does not enter runtime or copy/own the transport, address or payload.
    pub fn new(setup: &'a TransportSetup, address: &'a dyn LocalUriProvider, mapping: M) -> Self {
        Self { setup, address, mapping }
    }

    /// Reserves an encoded role through the actual core admission path.
    /// The returned setup has no publish method. A runtime has no setup method.
    /// Errors are the original core `StartupError`, without heap conversion.
    pub fn publisher<T: 'static>(
        &self, resource: u16, options: PublisherOptions,
    ) -> Result<PublisherSetup<T>, StartupError>
    where M: PublisherMapping<T> {
        let wire = SelectedWire(self.mapping.publisher_wire()?);
        let mut builder = two_mode::Publisher::<T>::on(self.setup, self.address, resource)
            .wire(wire).slots(options.slots).qos(options.qos);
        if let Some(max) = options.max_payload { builder = builder.max_payload(max); }
        if let Some(priority) = options.priority { builder = builder.priority(priority); }
        if let Some(ttl) = options.ttl { builder = builder.ttl(ttl); }
        if let Some(envelope) = options.envelope { builder = builder.envelope(envelope); }
        if options.require_lending { builder = builder.require_lending(); }
        Ok(PublisherSetup { inner: builder.build()? })
    }
}
impl<M> fmt::Debug for PublisherDeployment<'_, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PublisherDeployment").finish_non_exhaustive()
    }
}

/// Prepared route with no traffic capability. Activation consumes this value.
pub struct PublisherSetup<T: 'static> { inner: two_mode::PublisherSetup<T> }
impl<T: 'static> PublisherSetup<T> {
    /// Checks the exact transport identity in core; no heap allocation or free.
    /// # Safety-usable invariant
    /// Success contains only the core handle returned by its checked activation.
    /// No lifetime extension, new ownership or raw callback is introduced here.
    pub fn activate(self, runtime: &TransportRuntime) -> Result<Publisher<T>, StartupError> {
        Ok(Publisher { inner: self.inner.activate(runtime)? })
    }
}

/// The common SDK-owned stored role: the domain type is its only type argument.
/// Dropping it does not free startup storage. It adds no `Send`/`Sync` assertions;
/// all auto traits follow the contained core handle.
pub struct Publisher<T: 'static> { inner: two_mode::Publisher<T> }
impl<T: 'static> Publisher<T> {
    /// Encode once into core-reserved transport storage and send once.
    ///
    /// The returned future is inline and performs no work until first polled.
    /// That poll runs the existing synchronous core operation to completion;
    /// there is no suspension or retained loan across an await. In particular,
    /// `Overflow::Wait` and a blocking binding can block the executor thread.
    /// This API does not turn them into asynchronous readiness or promise a
    /// fixed poll duration. Use an appropriately qualified execution context.
    ///
    /// Cancellation before polling has no effects. Once polling starts, this
    /// operation cannot be cancelled mid-send; completion/partial delivery may
    /// already have occurred when the future is dropped. The future only borrows
    /// the handle and value; it never owns or drops `T` and never boxes a future.
    /// No runtime destructor is required to end a borrowed background operation.
    ///
    /// Success means the binding reported no known full-receiver discards; it does
    /// not promise a subscriber existed or that peers consumed the message.
    /// Use [`Self::publish_report`] for the available counts and `counts_exact`.
    /// Unknown counts do not promise no loss. A known discard is always an error.
    /// A later refusal can carry its original cause plus a prior delivery report.
    pub async fn publish(&self, value: &T) -> Result<(), PublishError> {
        let report = self.inner.publish(value)?;
        if report.dropped_full == 0 { Ok(()) } else { Err(PublishError::Dropped(report)) }
    }

    /// The same single-send operation, exposing counts and their completeness.
    /// Has the same blocking, borrowing and cancellation behavior as `publish`.
    pub async fn publish_report(&self, value: &T) -> Result<SendReport, SendError> {
        self.inner.publish(value)
    }

    /// Snapshot of the underlying admitted route's counters.
    pub fn counters(&self) -> RouteCounters { self.inner.counters() }
    /// Route identity for the transport runtime's diagnostics.
    pub fn route_id(&self) -> RouteId { self.inner.route_id() }
}
impl<T: 'static> fmt::Debug for Publisher<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Publisher").finish_non_exhaustive()
    }
}
impl<T: 'static> fmt::Debug for PublisherSetup<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PublisherSetup").finish_non_exhaustive()
    }
}

// SDK-owned dispatch. The reference is static and Wire requires Send + Sync;
// it stores no value, owner, destructor, borrowed frame, or allocator.
struct SelectedWire<T: 'static>(&'static dyn Wire<T>);
impl<T: 'static> Wire<T> for SelectedWire<T> {
    fn describe(&self) -> WireInfo { self.0.describe() }
    fn encoded_size(&self, value: &T) -> EncodedSize { self.0.encoded_size(value) }
    fn encode(&self, value: &T, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        self.0.encode(value, out)
    }
    fn init_slot<'s>(&self, slot: StartupSlot<'s, T>) -> Ready<'s, T> { self.0.init_slot(slot) }
    fn decode(&self, encoding: PayloadEncoding, bytes: &[u8], into: &mut T) -> Result<(), WireError> {
        self.0.decode(encoding, bytes, into)
    }
}
