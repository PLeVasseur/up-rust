//! Small reusable machinery for retained setup and independently bounded cleanup,
//! and the SDK-owned [`role_set!`](crate::role_set) built from those pieces.

use super::{
    Notifier, Publisher, Receiver, RpcClient, RpcServer, SerializedError, SerializedStopError,
    Subscription, SubscriptionCloseError,
};
use crate::{Completion, Failure, UStatus};
use std::{convert::Infallible, future::Future, sync::Arc, time::Duration};

/// Stores a successfully acquired owner before another setup await can occur.
/// Existing slots are not replaced. Cancellation preserves previously filled
/// slots, but cannot strengthen the underlying transport's attach-cancellation
/// contract for an attachment which has not yet been returned to this caller.
///
/// # Errors
/// Returns the original setup error without consuming any acquired slot.
pub async fn acquire<T, E>(
    slot: &mut Option<T>,
    create: impl Future<Output = Result<T, E>>,
) -> Result<(), E> {
    if slot.is_none() {
        *slot = Some(create.await?);
    }
    Ok(())
}

/// Bounded diagnostic history for one independently progressed cleanup leg.
#[derive(Debug)]
pub struct CleanupLeg<E> {
    /// Current attempt identifier, also retained when the outer future is dropped.
    pub attempt: u64,
    /// Latest completed result; None means pending or timed out, never success.
    pub current: Option<Result<(), Arc<E>>>,
    /// First actual error and its original attempt, never replaced by success.
    pub first_error: Option<Failure<Arc<E>>>,
    /// Total actual error observations; later errors may be compacted.
    pub failures: u64,
    /// True if this attempt's own deadline expired.
    pub timed_out: bool,
    /// Resource completion for the current attempt.
    pub completion: Completion,
}
impl<E> Default for CleanupLeg<E> {
    fn default() -> Self {
        Self {
            attempt: 0,
            current: None,
            first_error: None,
            failures: 0,
            timed_out: false,
            completion: Completion::Pending,
        }
    }
}
impl<E> CleanupLeg<E> {
    /// Starts an attempt before scheduling any asynchronous leg.
    pub fn begin(&mut self) {
        self.attempt = self.attempt.saturating_add(1);
        self.current = None;
        self.timed_out = false;
        self.completion = Completion::Pending;
    }
    /// Stores an actual result before another await; errors are not stringified.
    pub fn record(
        &mut self,
        result: Result<(), E>,
        failure_completion: impl FnOnce(&E) -> Completion,
    ) {
        self.completion = match &result {
            Ok(()) => Completion::Succeeded,
            Err(error) => failure_completion(error),
        };
        self.current = Some(result.map_err(|error| {
            self.failures = self.failures.saturating_add(1);
            let error = Arc::new(error);
            if self.first_error.is_none() {
                self.first_error = Some(Failure {
                    attempt: self.attempt,
                    error: error.clone(),
                });
            }
            error
        }));
    }
    /// Bounds this leg independently, without consuming its role owner.
    pub async fn run(
        &mut self,
        deadline: Duration,
        future: impl Future<Output = Result<(), E>>,
        classify: impl FnOnce(&E) -> Completion,
    ) {
        match tokio::time::timeout(deadline, future).await {
            Ok(result) => self.record(result, classify),
            Err(_) => self.timed_out = true,
        }
    }
}

/// Cleanup for complete or partially constructed serialized applications.
/// First error plus current result is bounded per role, not an unbounded log.
#[derive(Debug, Default)]
pub struct SerializedCleanup {
    /// Remote removal and local subscription resources.
    pub subscription: CleanupLeg<SubscriptionCloseError<UStatus>>,
    /// Notification receiver.
    pub notifications: CleanupLeg<SerializedStopError>,
    /// Request server.
    pub server: CleanupLeg<SerializedStopError>,
    /// Response correlation client.
    pub client: CleanupLeg<SerializedError>,
}
/// Folds independent role outcomes: anything unresolved is Pending; otherwise any
/// terminal failure is Failed.
#[doc(hidden)]
pub fn aggregate_completion(outcomes: &[Completion]) -> Completion {
    if outcomes.contains(&Completion::Pending) {
        Completion::Pending
    } else if outcomes.contains(&Completion::Failed) {
        Completion::Failed
    } else {
        Completion::Succeeded
    }
}
impl SerializedCleanup {
    /// Aggregate resource completion, never treating a terminal failure as success.
    pub fn completion(&self) -> Completion {
        aggregate_completion(&[
            self.subscription.completion,
            self.notifications.completion,
            self.server.completion,
            self.client.completion,
        ])
    }
    /// Closes every local admission synchronously and progresses bounded drains
    /// concurrently. Cancellation leaves per-leg reports here and partial exact
    /// detach/service observations on the role itself. None means never acquired.
    pub fn close<'a, Req, Resp>(
        &'a mut self,
        mut subscription: Option<&'a mut Subscription>,
        notifications: Option<&'a mut Receiver>,
        server: Option<&'a mut RpcServer>,
        client: Option<&'a RpcClient<Req, Resp>>,
        deadline: Duration,
    ) -> impl Future<Output = Completion> + 'a {
        self.subscription.begin();
        self.notifications.begin();
        self.server.begin();
        self.client.begin();
        if let Some(owner) = subscription.as_mut() {
            owner.close_admission();
        }
        if let Some(owner) = &notifications {
            owner.close_admission();
        }
        if let Some(owner) = &server {
            owner.close_admission();
        }
        self.client
            .record(client.map_or(Ok(()), RpcClient::close), |_| {
                Completion::Pending
            });
        let subscription = subscription.map(Subscription::unsubscribe);
        let notifications = notifications.map(Receiver::stop);
        let server = server.map(RpcServer::stop);
        async move {
            tokio::join!(
                self.subscription.run(
                    deadline,
                    async {
                        match subscription {
                            Some(future) => future.await,
                            None => Ok(()),
                        }
                    },
                    SubscriptionCloseError::completion
                ),
                self.notifications.run(
                    deadline,
                    async {
                        match notifications {
                            Some(future) => future.await,
                            None => Ok(()),
                        }
                    },
                    SerializedStopError::completion
                ),
                self.server.run(
                    deadline,
                    async {
                        match server {
                            Some(future) => future.await,
                            None => Ok(()),
                        }
                    },
                    SerializedStopError::completion
                ),
            );
            self.completion()
        }
    }
}

/// Why a role set refused an operation. Refusals never touch retained owners.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RoleSetError {
    /// `begin` already ran: this setup is cleanup-only.
    #[error("setup already started; clean up retained owners and create a new setup")]
    AlreadyStarted,
    /// Cleanup has begun, so retained owners can no longer become an application.
    #[error("cleanup already started; this setup can only finish closing")]
    CleanupStarted,
    /// A role was never acquired, so the set is not complete.
    #[error("role `{role}` was not acquired")]
    Missing {
        /// The field name of the absent role.
        role: &'static str,
    },
}
impl RoleSetError {
    fn message(self) -> &'static str {
        match self {
            Self::AlreadyStarted => {
                "setup already started; clean up retained owners and create a new setup"
            }
            Self::CleanupStarted => "cleanup already started; this setup can only finish closing",
            Self::Missing { .. } => "a required role was not acquired",
        }
    }
}
impl From<RoleSetError> for SerializedError {
    fn from(error: RoleSetError) -> Self {
        Self::Config(error.message())
    }
}
#[cfg(feature = "up-l2-zero-copy")]
impl From<RoleSetError> for crate::SetupError {
    fn from(error: RoleSetError) -> Self {
        Self::InvalidConfig(error.message())
    }
}

/// A role owner that the SDK can retain and clean up without knowing its type.
///
/// Implemented for every serialized and native application role. A reusable L2
/// library can implement it for its own owners so they can join a
/// [`role_set!`](crate::role_set). The contract is the same as the primitives':
/// [`fence`](Self::fence) closes admission synchronously and
/// [`begin_close`](Self::begin_close) performs every eager effect (detach, correlation
/// close, recording a synchronous outcome) before it returns. Cancelling the returned
/// future keeps the owner, its failed attachment and any pending drain for retry.
pub trait ManagedRole {
    /// The exact close failure, retained un-stringified by the role's leg.
    type CloseError: std::fmt::Debug + Send + Sync + 'static;

    /// Closes local admission without waiting.
    fn fence(&mut self);

    /// Performs the eager close, then returns a future resolving this attempt.
    fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<Self::CloseError>,
        deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a;
}

fn no_cleanup(leg: &mut CleanupLeg<Infallible>) -> std::future::Ready<Completion> {
    leg.record(Ok(()), |never| match *never {});
    std::future::ready(leg.completion)
}

impl<P> ManagedRole for Publisher<P> {
    type CloseError = Infallible;
    fn fence(&mut self) {}
    fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<Infallible>,
        _deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a {
        no_cleanup(leg)
    }
}
impl<P> ManagedRole for Notifier<P> {
    type CloseError = Infallible;
    fn fence(&mut self) {}
    fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<Infallible>,
        _deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a {
        no_cleanup(leg)
    }
}
impl<Req, Resp> ManagedRole for RpcClient<Req, Resp> {
    type CloseError = SerializedError;
    fn fence(&mut self) {
        // `close` closes admission and detaches synchronously.
    }
    fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<SerializedError>,
        _deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a {
        leg.record(self.close(), |_| Completion::Pending);
        std::future::ready(leg.completion)
    }
}
impl ManagedRole for Receiver {
    type CloseError = SerializedStopError;
    fn fence(&mut self) {
        self.close_admission();
    }
    fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<SerializedStopError>,
        deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a {
        let stop = self.stop();
        async move {
            leg.run(deadline, stop, SerializedStopError::completion)
                .await;
            leg.completion
        }
    }
}
impl ManagedRole for RpcServer {
    type CloseError = SerializedStopError;
    fn fence(&mut self) {
        self.close_admission();
    }
    fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<SerializedStopError>,
        deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a {
        let stop = self.stop();
        async move {
            leg.run(deadline, stop, SerializedStopError::completion)
                .await;
            leg.completion
        }
    }
}
impl ManagedRole for Subscription {
    type CloseError = SubscriptionCloseError<UStatus>;
    fn fence(&mut self) {
        self.close_admission();
    }
    fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<SubscriptionCloseError<UStatus>>,
        deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a {
        let close = self.unsubscribe();
        async move {
            leg.run(deadline, close, SubscriptionCloseError::completion)
                .await;
            leg.completion
        }
    }
}

/// One role owner that setup may or may not have reached.
///
/// A slot stores its owner before the next setup await can start, so a failed or
/// cancelled attempt keeps every exact owner it already acquired.
pub struct RoleSlot<T> {
    owner: Option<T>,
}
impl<T> Default for RoleSlot<T> {
    fn default() -> Self {
        Self { owner: None }
    }
}
impl<T> std::fmt::Debug for RoleSlot<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RoleSlot")
            .field("held", &self.owner.is_some())
            .finish()
    }
}
impl<T> RoleSlot<T> {
    /// Stores a synchronously created owner; an owner already held is kept.
    pub fn store(&mut self, owner: T) -> &mut T {
        self.owner.get_or_insert(owner)
    }
    /// Awaits creation and stores the owner before any other setup await can run.
    /// Cancellation or failure leaves this slot empty; an owner already held is
    /// kept and `create` is dropped unpolled.
    ///
    /// # Errors
    /// Returns the original setup error without touching any acquired slot.
    pub async fn acquire<E>(
        &mut self,
        create: impl Future<Output = Result<T, E>>,
    ) -> Result<&mut T, E> {
        let owner = match self.owner.take() {
            Some(owner) => owner,
            None => create.await?,
        };
        Ok(self.owner.insert(owner))
    }
    /// Whether this slot holds its owner.
    pub fn is_held(&self) -> bool {
        self.owner.is_some()
    }
    /// The retained owner, if acquired.
    pub fn get(&self) -> Option<&T> {
        self.owner.as_ref()
    }
    /// Exclusive access to the retained owner, if acquired.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.owner.as_mut()
    }
    /// Removes the owner, for callers that manage its lifecycle themselves.
    pub fn take(&mut self) -> Option<T> {
        self.owner.take()
    }
}
impl<T: ManagedRole> RoleSlot<T> {
    #[doc(hidden)]
    pub fn fence(&mut self) {
        if let Some(owner) = &mut self.owner {
            owner.fence();
        }
    }
    #[doc(hidden)]
    pub fn begin_close<'a>(
        &'a mut self,
        leg: &'a mut CleanupLeg<T::CloseError>,
        deadline: Duration,
    ) -> impl Future<Output = Completion> + Send + 'a {
        let closing = match self.owner.as_mut() {
            Some(owner) => Some(owner.begin_close(leg, deadline)),
            None => {
                leg.record(Ok(()), |_| Completion::Succeeded);
                None
            }
        };
        async move {
            match closing {
                Some(closing) => closing.await,
                None => Completion::Succeeded,
            }
        }
    }
}

/// Tracks a setup's single attempt so cleanup-only behavior is enforced by the SDK.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default)]
pub struct RoleSetProgress {
    started: bool,
    closing: bool,
}
impl RoleSetProgress {
    pub fn begin(&mut self) -> Result<(), RoleSetError> {
        if self.closing {
            return Err(RoleSetError::CleanupStarted);
        }
        if self.started {
            return Err(RoleSetError::AlreadyStarted);
        }
        self.started = true;
        Ok(())
    }
    pub fn finishing(&self) -> Result<(), RoleSetError> {
        if self.closing {
            Err(RoleSetError::CleanupStarted)
        } else {
            Ok(())
        }
    }
    pub fn closing(&mut self) {
        self.closing = true;
    }
}

/// Generates an SDK-owned set of role owners, in any mix, for one application.
///
/// Name a module and list the roles the program actually needs, any subset and any
/// repeats, serialized or native; each field's type decides how it is closed. The
/// generated module holds:
///
/// * `Setup`: one attempt at acquiring every role. `begin` returns the `Slots`
///   once, so a second attempt is refused; a slot stores each owner before the next
///   await, so failure or cancellation keeps every exact owner it already acquired.
///   `finish` promotes a complete set to `Roles`; `close` cleans up a partial one.
/// * `Roles`: the plain typed role fields, used directly (`app.state.publish(..)`).
///   `close` is bounded per role, retryable, and keeps a `CleanupLeg` per field in
///   `cleanup`, so no error is lost and a hung role cannot starve the others.
///
/// Paths in the list resolve in the invoking module, wherever the macro is called.
/// `cleanup` is reserved: it names the per-field legs on `Setup` and `Roles`.
///
/// ```no_run
/// use protobuf::well_known_types::wrappers::StringValue;
/// use std::{sync::Arc, time::Duration};
/// use up_rust::communication::application::*;
/// use up_rust::core::usubscription::USubscription;
/// use up_rust::{StaticUriProvider, UEntityId, UOwnedTransport};
///
/// up_rust::declare_operations! {
///     pub event STATUS: StringValue = 0x8001;
///     pub event COMMAND: StringValue = 0x8002;
/// }
///
/// struct Recorder;
/// impl Handler<StringValue> for Recorder {
///     async fn on_receive<D: Delivery<StringValue>>(&mut self, delivery: D) {
///         if let Ok(value) = delivery.payload() {
///             println!("{}", value.value);
///         }
///     }
/// }
///
/// up_rust::role_set! {
///     /// The owners this one program needs; roles and repeats are its own choice.
///     pub mod gateway {
///         status: Publisher<StringValue>,
///         commands: Subscription,
///         audit: Subscription,
///     }
/// }
///
/// async fn start(
///     setup: &mut gateway::Setup,
///     local: &BoundDeployment<ProtobufProfile>,
///     peer: &dyn up_rust::LocalUriProvider,
///     service: Arc<dyn USubscription>,
/// ) -> Result<(), SerializedError> {
///     let slots = setup.begin()?;
///     slots.status.store(STATUS.publisher(local, PublishOptions::default())?);
///     slots.commands.acquire(COMMAND.subscription(local, peer, service.clone(), Recorder)).await?;
///     slots.audit.acquire(COMMAND.subscription(local, peer, service, Recorder)).await?;
///     Ok(())
/// }
///
/// # async fn run(transport: Arc<dyn UOwnedTransport>, service: Arc<dyn USubscription>) -> Result<(), Box<dyn std::error::Error>> {
/// let address = StaticUriProvider::for_entity("gateway", UEntityId::first_instance(1), 1)?;
/// let peer = StaticUriProvider::for_entity("device", UEntityId::first_instance(2), 1)?;
/// let local = Deployment::with_defaults(transport, Arc::new(address))?.protobuf();
/// let mut setup = gateway::Setup::default();
/// if let Err(original) = start(&mut setup, &local, &peer, service).await {
///     while setup.close(Duration::from_secs(1)).await == up_rust::Completion::Pending {
///         tokio::time::sleep(Duration::from_millis(100)).await;
///     }
///     return Err(original.into());
/// }
/// let mut app = setup.finish()?;
/// app.status.publish(&StringValue::new()).await?;
/// let _outcome = app.close(Duration::from_secs(1)).await;
/// # Ok(())
/// # }
/// # fn main() {}
/// ```
#[macro_export]
macro_rules! role_set {
    (
        $(#[$meta:meta])*
        $vis:vis mod $name:ident { $($field:ident : $role:ty),+ $(,)? }
    ) => {
        $crate::__role_set! { [$(#[$meta])*] [$vis] $name [] [] [$($field : $role),+] }
    };
    (
        $(#[$meta:meta])*
        $vis:vis mod $name:ident < $param:ident : $bound:path > { $($field:ident : $role:ty),+ $(,)? }
    ) => {
        $crate::__role_set! { [$(#[$meta])*] [$vis] $name [<$param: $bound>] [<$param>] [$($field : $role),+] }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __role_set {
    (
        [$(#[$meta:meta])*] [$vis:vis] $name:ident [$($impl_generics:tt)*] [$($ty_generics:tt)*]
        [$($field:ident : $role:ty),+]
    ) => {
        $(#[$meta])*
        #[allow(dead_code)]
        $vis mod $name {
            #[allow(unused_imports)]
            use super::*;

            #[doc = "Slots for one setup attempt; each keeps the exact owner it acquired."]
            pub struct Slots $($impl_generics)* {
                $(
                    #[doc = concat!("Slot for the `", stringify!($field), "` role.")]
                    pub $field: $crate::communication::application::RoleSlot<$role>,
                )+
            }

            #[doc = "Per-role cleanup diagnostics; the first error and every attempt are kept."]
            pub struct Cleanup $($impl_generics)* {
                $(
                    #[doc = concat!("Cleanup leg of the `", stringify!($field), "` role.")]
                    pub $field: $crate::communication::application::CleanupLeg<
                        <$role as $crate::communication::application::ManagedRole>::CloseError,
                    >,
                )+
            }

            #[doc = "One setup attempt: `begin` once, then `finish` or `close`."]
            #[must_use = "a setup owns any roles it acquired; finish or close it"]
            pub struct Setup $($impl_generics)* {
                slots: Slots $($ty_generics)*,
                #[doc = "Per-role cleanup diagnostics, retained across attempts."]
                pub cleanup: Cleanup $($ty_generics)*,
                progress: $crate::communication::application::RoleSetProgress,
            }

            #[doc = "The complete, stored role owners of one application."]
            pub struct Roles $($impl_generics)* {
                $(
                    #[doc = concat!("The `", stringify!($field), "` role.")]
                    pub $field: $role,
                )+
                #[doc = "Per-role cleanup diagnostics, retained across attempts."]
                pub cleanup: Cleanup $($ty_generics)*,
            }

            impl $($impl_generics)* ::core::default::Default for Slots $($ty_generics)* {
                fn default() -> Self {
                    Self { $($field: ::core::default::Default::default(),)+ }
                }
            }

            impl $($impl_generics)* ::core::default::Default for Cleanup $($ty_generics)* {
                fn default() -> Self {
                    Self { $($field: ::core::default::Default::default(),)+ }
                }
            }

            impl $($impl_generics)* ::core::default::Default for Setup $($ty_generics)* {
                fn default() -> Self {
                    Self {
                        slots: ::core::default::Default::default(),
                        cleanup: ::core::default::Default::default(),
                        progress: ::core::default::Default::default(),
                    }
                }
            }

            impl $($impl_generics)* ::core::fmt::Debug for Slots $($ty_generics)* {
                fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    formatter
                        .debug_struct("Slots")
                        $(.field(stringify!($field), &self.$field))+
                        .finish()
                }
            }

            impl $($impl_generics)* ::core::fmt::Debug for Cleanup $($ty_generics)* {
                fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    formatter
                        .debug_struct("Cleanup")
                        $(.field(stringify!($field), &self.$field))+
                        .finish()
                }
            }

            impl $($impl_generics)* ::core::fmt::Debug for Setup $($ty_generics)* {
                fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    formatter
                        .debug_struct("Setup")
                        .field("slots", &self.slots)
                        .field("cleanup", &self.cleanup)
                        .finish_non_exhaustive()
                }
            }

            impl $($impl_generics)* ::core::fmt::Debug for Roles $($ty_generics)* {
                fn fmt(&self, formatter: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                    formatter
                        .debug_struct("Roles")
                        .field("cleanup", &self.cleanup)
                        .finish_non_exhaustive()
                }
            }

            impl $($impl_generics)* Cleanup $($ty_generics)* {
                #[doc = "Aggregate completion: Pending while anything is unresolved."]
                pub fn completion(&self) -> $crate::Completion {
                    $crate::communication::application::aggregate_completion(&[
                        $(self.$field.completion),+
                    ])
                }
            }

            impl $($impl_generics)* Setup $($ty_generics)* {
                #[doc = "Starts the single setup attempt; any repeat is refused."]
                #[doc = ""]
                #[doc = "# Errors"]
                #[doc = "Refuses a second attempt or an attempt after cleanup began."]
                pub fn begin(
                    &mut self,
                ) -> ::core::result::Result<
                    &mut Slots $($ty_generics)*,
                    $crate::communication::application::RoleSetError,
                > {
                    self.progress.begin()?;
                    ::core::result::Result::Ok(&mut self.slots)
                }

                #[doc = "Retained slots, for inspecting a partial or complete setup."]
                pub fn slots(&self) -> &Slots $($ty_generics)* {
                    &self.slots
                }

                #[doc = "Promotes a complete set. Nothing is taken from an incomplete one."]
                #[doc = ""]
                #[doc = "# Errors"]
                #[doc = "Refuses incomplete sets and sets whose cleanup has begun."]
                pub fn finish(
                    &mut self,
                ) -> ::core::result::Result<
                    Roles $($ty_generics)*,
                    $crate::communication::application::RoleSetError,
                > {
                    self.progress.finishing()?;
                    $(
                        if !self.slots.$field.is_held() {
                            return ::core::result::Result::Err(
                                $crate::communication::application::RoleSetError::Missing {
                                    role: stringify!($field),
                                },
                            );
                        }
                    )+
                    ::core::result::Result::Ok(Roles {
                        $(
                            $field: self.slots.$field.take().ok_or(
                                $crate::communication::application::RoleSetError::Missing {
                                    role: stringify!($field),
                                },
                            )?,
                        )+
                        cleanup: ::core::default::Default::default(),
                    })
                }

                #[doc = "Fences every held role, then closes each within its own deadline."]
                pub fn close(
                    &mut self,
                    deadline: ::core::time::Duration,
                ) -> impl ::core::future::Future<Output = $crate::Completion> + ::core::marker::Send + '_ {
                    self.progress.closing();
                    let Self { slots, cleanup, .. } = self;
                    let Slots { $($field,)+ } = slots;
                    $(cleanup.$field.begin();)+
                    $($crate::communication::application::RoleSlot::fence($field);)+
                    $(
                        let $field = $crate::communication::application::RoleSlot::begin_close(
                            $field,
                            &mut cleanup.$field,
                            deadline,
                        );
                    )+
                    async move {
                        let ($($field,)+) = $crate::__private::tokio::join!($($field),+);
                        $crate::communication::application::aggregate_completion(&[$($field),+])
                    }
                }

                #[doc = "Retries `close` up to `attempts` times while roles remain unresolved."]
                pub async fn close_bounded(
                    &mut self,
                    attempts: usize,
                    deadline: ::core::time::Duration,
                ) -> $crate::Completion {
                    for _attempt in 0..attempts {
                        let completion = self.close(deadline).await;
                        if completion != $crate::Completion::Pending {
                            return completion;
                        }
                    }
                    $crate::Completion::Pending
                }

                #[doc = "Aggregate cleanup completion of this attempt."]
                pub fn completion(&self) -> $crate::Completion {
                    self.cleanup.completion()
                }
            }

            impl $($impl_generics)* Roles $($ty_generics)* {
                #[doc = "Fences every role, then closes each within its own deadline."]
                pub fn close(
                    &mut self,
                    deadline: ::core::time::Duration,
                ) -> impl ::core::future::Future<Output = $crate::Completion> + ::core::marker::Send + '_ {
                    let Self { cleanup, $($field,)+ } = self;
                    $(cleanup.$field.begin();)+
                    $($crate::communication::application::ManagedRole::fence($field);)+
                    $(
                        let $field = $crate::communication::application::ManagedRole::begin_close(
                            $field,
                            &mut cleanup.$field,
                            deadline,
                        );
                    )+
                    async move {
                        let ($($field,)+) = $crate::__private::tokio::join!($($field),+);
                        $crate::communication::application::aggregate_completion(&[$($field),+])
                    }
                }

                #[doc = "Retries `close` up to `attempts` times while roles remain unresolved."]
                pub async fn close_bounded(
                    &mut self,
                    attempts: usize,
                    deadline: ::core::time::Duration,
                ) -> $crate::Completion {
                    for _attempt in 0..attempts {
                        let completion = self.close(deadline).await;
                        if completion != $crate::Completion::Pending {
                            return completion;
                        }
                    }
                    $crate::Completion::Pending
                }

                #[doc = "Aggregate cleanup completion across every role."]
                pub fn completion(&self) -> $crate::Completion {
                    self.cleanup.completion()
                }
            }
        }
    };
}
