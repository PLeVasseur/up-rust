//! Typed resource declarations and deployment-selected schema mappings.

use super::*;
use std::marker::PhantomData;

/// A real schema declaration for one domain type, not an encoding-ID lookup.
/// A resource declaration alone cannot invent a schema mapping:
///
/// ```compile_fail
/// use up_rust::communication::application::{BoundDeployment, Deployment, Event, PublishOptions};
/// struct Reading;
/// enum Profile { Json }
/// fn missing_mapping(deployment: Deployment) {
///     let selected = BoundDeployment::new(deployment, Profile::Json);
///     let _ = Event::<Reading>::new(0x8001).publisher(&selected, PublishOptions::default());
/// }
/// ```
pub trait MappingProfile<P> {
    /// Returns the declared encoder. No codec instance is constructed.
    fn encoder(&self) -> Result<Encoder<P>, crate::UWireError>;
    /// Returns the independently declared decoder.
    fn decoder(&self) -> Result<Decoder<P>, crate::UWireError>;
}

/// Setup retains a meaningful deployment profile; role handles do not retain its type.
#[derive(Clone, Debug)]
pub struct BoundDeployment<M> {
    deployment: Deployment,
    profile: M,
}
impl<M> BoundDeployment<M> {
    /// Selects an actual schema profile independently of carriage and envelope.
    pub fn new(deployment: Deployment, profile: M) -> Self {
        Self {
            deployment,
            profile,
        }
    }
    /// Advanced setup access, retaining the same admission boundary.
    pub fn deployment(&self) -> &Deployment {
        &self.deployment
    }
    /// Semantic addressing for independently configured peers.
    pub fn address(&self) -> &dyn LocalUriProvider {
        self.deployment.uri_provider()
    }
}

/// Shared role limits, separate from resource and payload declarations.
#[derive(Clone, Copy, Debug)]
pub struct OperationOptions {
    /// Publication and notification options.
    pub publish: PublishOptions,
    /// Client limits and request deadline.
    pub rpc: RpcOptions,
    /// Server client-route limit.
    pub server: ServerOptions,
}

/// An event's resource and domain type, declared once by the schema owner.
#[derive(Debug)]
pub struct Event<P> {
    resource: u16,
    payload: PhantomData<fn() -> P>,
}
/// A notification's resource and domain type.
#[derive(Debug)]
pub struct Notification<P> {
    resource: u16,
    payload: PhantomData<fn() -> P>,
}
/// An RPC method with distinct request and response declarations.
#[derive(Debug)]
pub struct Rpc<Req, Resp> {
    resource: u16,
    payload: PhantomData<fn(Req) -> Resp>,
}
impl<P> Event<P> {
    /// Declares a schema event; role construction validates the resource category.
    pub const fn new(resource: u16) -> Self {
        Self {
            resource,
            payload: PhantomData,
        }
    }
    /// Resource identity, also used by the static native binding.
    pub const fn resource(&self) -> u16 {
        self.resource
    }
    /// Checks both peers' declared event directions without attaching workers.
    pub fn admit<S: EncodeProfile<P>, R: DecodeProfile<P>>(
        &self,
        sender: &BoundDeployment<S>,
        receiver: &BoundDeployment<R>,
    ) -> Result<(), SerializedError> {
        sender
            .profile
            .encoder()?
            .admit_carriage(sender.deployment.transport().as_ref())?;
        receiver
            .profile
            .decoder()?
            .admit_carriage(receiver.deployment.transport().as_ref())?;
        Ok(())
    }
    /// Legacy owned-carriage factory, returning LegacyPublisher explicitly.
    /// Common bounded setup uses PublisherDeployment::publisher(resource(), options).
    pub fn publisher<M: EncodeProfile<P>>(
        &self,
        sender: &BoundDeployment<M>,
        options: PublishOptions,
    ) -> Result<LegacyPublisher<P>, SerializedError> {
        sender
            .deployment
            .publisher(self.resource, options, sender.profile.encoder()?)
    }
    /// Prepares a retained subscription ticket, without activating it.
    pub async fn subscription<M: DecodeProfile<P>, H: Handler<P>>(
        &self,
        receiver: &BoundDeployment<M>,
        source: &dyn LocalUriProvider,
        service: Arc<dyn crate::core::usubscription::USubscription>,
        handler: H,
    ) -> Result<Subscription, SerializedError>
    where
        P: Send + 'static,
    {
        receiver
            .deployment
            .subscriber(service)
            .subscription(
                source.get_resource_uri(self.resource),
                handler,
                receiver.profile.decoder()?,
            )
            .await
    }
}
impl<P> Notification<P> {
    /// Declares a schema notification.
    pub const fn new(resource: u16) -> Self {
        Self {
            resource,
            payload: PhantomData,
        }
    }
    /// Resource identity for the static native binding.
    pub const fn resource(&self) -> u16 {
        self.resource
    }
    /// Checks independently declared sending and receiving profiles.
    pub fn admit<S: EncodeProfile<P>, R: DecodeProfile<P>>(
        &self,
        sender: &BoundDeployment<S>,
        receiver: &BoundDeployment<R>,
    ) -> Result<(), SerializedError> {
        sender
            .profile
            .encoder()?
            .admit_carriage(sender.deployment.transport().as_ref())?;
        receiver
            .profile
            .decoder()?
            .admit_carriage(receiver.deployment.transport().as_ref())?;
        Ok(())
    }
    /// Resolves an exact destination and the sender's real schema mapping.
    pub fn notifier<M: EncodeProfile<P>>(
        &self,
        sender: &BoundDeployment<M>,
        destination: UUri,
        options: PublishOptions,
    ) -> Result<Notifier<P>, SerializedError> {
        sender.deployment.notifier(
            self.resource,
            destination,
            options,
            sender.profile.encoder()?,
        )
    }
    /// Resolves notification direction and exact source before attaching.
    pub async fn receiver<M: DecodeProfile<P>, H: Handler<P>>(
        &self,
        receiver: &BoundDeployment<M>,
        source: &dyn LocalUriProvider,
        handler: H,
    ) -> Result<Receiver, SerializedError>
    where
        P: Send + 'static,
    {
        receiver
            .deployment
            .listen(
                source.get_resource_uri(self.resource),
                handler,
                receiver.profile.decoder()?,
            )
            .await
    }
}
impl<Req, Resp> Rpc<Req, Resp> {
    /// Declares one method and two independently mapped domain types.
    pub const fn new(resource: u16) -> Self {
        Self {
            resource,
            payload: PhantomData,
        }
    }
    /// Resource identity for the static native binding.
    pub const fn resource(&self) -> u16 {
        self.resource
    }
    /// Checks all four RPC directions, without assuming matching codec types.
    pub fn admit<C, S>(
        &self,
        client: &BoundDeployment<C>,
        server: &BoundDeployment<S>,
    ) -> Result<(), SerializedError>
    where
        C: EncodeProfile<Req> + DecodeProfile<Resp>,
        S: DecodeProfile<Req> + EncodeProfile<Resp>,
    {
        <C as EncodeProfile<Req>>::encoder(&client.profile)?
            .admit(client.deployment.transport().as_ref())?;
        <C as DecodeProfile<Resp>>::decoder(&client.profile)?
            .admit(client.deployment.transport().as_ref())?;
        <S as DecodeProfile<Req>>::decoder(&server.profile)?
            .admit(server.deployment.transport().as_ref())?;
        <S as EncodeProfile<Resp>>::encoder(&server.profile)?
            .admit(server.deployment.transport().as_ref())?;
        Ok(())
    }
    /// Reserves correlation using the client's selected request/response mappings.
    pub async fn client<M: EncodeProfile<Req> + DecodeProfile<Resp>>(
        &self,
        client: &BoundDeployment<M>,
        server: &dyn LocalUriProvider,
        options: RpcOptions,
    ) -> Result<RpcClient<Req, Resp>, SerializedError> {
        client
            .deployment
            .rpc_client(
                server.get_resource_uri(self.resource),
                options,
                <M as EncodeProfile<Req>>::encoder(&client.profile)?,
                <M as DecodeProfile<Resp>>::decoder(&client.profile)?,
            )
            .await
    }
    /// Uses the server's independent request decoder and response encoder.
    pub async fn server<M, H, W>(
        &self,
        server: &BoundDeployment<M>,
        handler: H,
        write: W,
        options: ServerOptions,
    ) -> Result<RpcServer, SerializedError>
    where
        M: DecodeProfile<Req> + EncodeProfile<Resp>,
        Req: Send + 'static,
        Resp: Send + 'static,
        H: RequestHandler<Req>,
        W: FnMut(H::Reply) -> Resp + Send + 'static,
    {
        server
            .deployment
            .rpc_server(
                self.resource,
                handler,
                write,
                options,
                <M as DecodeProfile<Req>>::decoder(&server.profile)?,
                <M as EncodeProfile<Resp>>::encoder(&server.profile)?,
            )
            .await
    }
}

/// Declares payload mappings for domain types.
///
/// When each domain type has one codec, generate the profile directly; no enum and
/// no per-deployment choice is needed:
///
/// ```ignore
/// up_rust::schema_mapping!(pub struct Schema { Reading => ReadingCodec, Query => QueryCodec });
/// let local = Deployment::with_defaults(transport, address)?.bind(Schema);
/// ```
///
/// When a deployment selects between codecs, implement one domain mapping across a
/// meaningful profile enum's variants, repeating the invocation per domain type.
/// Request and response codecs need not be equal:
///
/// ```ignore
/// up_rust::schema_mapping!(Format, Reading {
///     Format::Protobuf => (ReadingProtobuf, ReadingProtobuf),
///     Format::Json => (ReadingJson, ReadingJson),
/// });
/// ```
///
/// A generated protobuf type needs neither form: see [`ProtobufProfile`].
/// Mapping functions remain the schema owner's `EncodePayload`/`DecodePayload`.
///
/// [`ProtobufProfile`]: crate::communication::application::ProtobufProfile
#[macro_export]
macro_rules! schema_mapping {
    ($(#[$meta:meta])* $vis:vis struct $name:ident { $($payload:ty => $codec:ty),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default)]
        $vis struct $name;
        $(
            impl $crate::communication::application::MappingProfile<$payload> for $name {
                fn encoder(&self) -> ::core::result::Result<$crate::communication::application::Encoder<$payload>, $crate::UWireError> {
                    $crate::communication::application::Encoder::from_schema_mapping::<$codec>()
                }
                fn decoder(&self) -> ::core::result::Result<$crate::communication::application::Decoder<$payload>, $crate::UWireError> {
                    $crate::communication::application::Decoder::from_schema_mapping::<$codec>()
                }
            }
        )+
    };
    ($profile:ty, $payload:ty { $($variant:path => ($encode:ty, $decode:ty)),+ $(,)? }) => {
        impl $crate::communication::application::MappingProfile<$payload> for $profile {
            fn encoder(&self) -> ::core::result::Result<$crate::communication::application::Encoder<$payload>, $crate::UWireError> {
                match self { $($variant => $crate::communication::application::Encoder::from_schema_mapping::<$encode>()),+ }
            }
            fn decoder(&self) -> ::core::result::Result<$crate::communication::application::Decoder<$payload>, $crate::UWireError> {
                match self { $($variant => $crate::communication::application::Decoder::from_schema_mapping::<$decode>()),+ }
            }
        }
    };
}

/// Schema profile for generated protobuf types: no enum and no mapping code.
///
/// Select it once with [`Deployment::protobuf`]; every operation over a generated
/// type then uses the SDK's fixed-identity `ProtobufPayload` codec.
///
/// ```no_run
/// # use up_rust::prelude::*;
/// use protobuf::well_known_types::wrappers::StringValue;
/// use std::sync::Arc;
/// use up_rust::communication::application::{Deployment, PublishOptions};
/// use up_rust::{StaticUriProvider, UEntityId, UOwnedTransport};
///
/// up_rust::declare_operations! { pub event STATUS: StringValue = 0x8001; }
///
/// # async fn publish(transport: Arc<dyn UOwnedTransport>) -> Result<(), Box<dyn std::error::Error>> {
/// let address = StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(0x1234), 1)?;
/// let local = Deployment::with_defaults(transport, Arc::new(address))?.protobuf();
/// let status = STATUS.publisher(&local, PublishOptions::default())?;
/// status.publish(&StringValue::new()).await?;
/// # Ok(())
/// # }
/// # fn main() {}
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct ProtobufProfile;

impl<P: crate::ProtobufMappable> MappingProfile<P> for ProtobufProfile {
    fn encoder(&self) -> Result<Encoder<P>, crate::UWireError> {
        Encoder::from_schema_mapping::<crate::ProtobufPayload>()
    }
    fn decoder(&self) -> Result<Decoder<P>, crate::UWireError> {
        Decoder::from_schema_mapping::<crate::ProtobufPayload>()
    }
}

/// Declares a stateless JSON codec for serde domain types.
///
/// The SDK does not depend on serde: the expansion calls `serde_json` in the calling
/// crate, which must depend on it and implement `Serialize` and `Deserialize` for each
/// listed type. Encoding is measured once and written once, without staging. The
/// family literal and compact ID are the deployment's private declaration, and every
/// peer must declare exactly the same pair. Use `#[serde(deny_unknown_fields)]` for
/// exact-field schemas.
///
/// ```ignore
/// up_rust::json_codec!(pub struct DoorJson = ("example.vehicle.door.json-v1", 0xF101) for DoorState, SetDoor);
/// ```
#[macro_export]
macro_rules! json_codec {
    ($(#[$meta:meta])* $vis:vis struct $name:ident = ($family:literal, $id:expr) for $($payload:ty),+ $(,)?) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default)]
        $vis struct $name;
        impl $crate::PayloadCodecIdentity for $name {
            fn name() -> &'static str {
                $family
            }
            fn encoding() -> $crate::PayloadEncoding {
                $crate::PayloadEncoding::JSON
            }
            fn payload_family() -> ::core::option::Option<$crate::WireIdentity> {
                ::core::option::Option::Some($crate::WireIdentity::new($family, $id))
            }
        }
        $(
            impl $crate::EncodePayload<$payload> for $name {
                fn payload_layout(
                    value: &$payload,
                ) -> ::core::result::Result<$crate::PayloadLayout, $crate::UWireError> {
                    let mut measured = $crate::__private::ByteCounter::default();
                    ::serde_json::to_writer(&mut measured, value).map_err(|error| {
                        $crate::UWireError::invalid_payload(::std::string::ToString::to_string(&error))
                    })?;
                    $crate::PayloadLayout::new(measured.len(), 1)
                }
                fn encode_payload(
                    value: &$payload,
                    target: &mut [u8],
                ) -> ::core::result::Result<(), $crate::UWireError> {
                    let extent = target.len();
                    let mut written = ::std::io::Cursor::new(target);
                    ::serde_json::to_writer(&mut written, value).map_err(|error| {
                        $crate::UWireError::invalid_payload(::std::string::ToString::to_string(&error))
                    })?;
                    if written.position() != extent as u64 {
                        return ::core::result::Result::Err($crate::UWireError::invalid_payload(
                            "exact JSON payload extent required",
                        ));
                    }
                    ::core::result::Result::Ok(())
                }
                fn encode_payload_owned(
                    value: &$payload,
                ) -> ::core::result::Result<$crate::__private::Bytes, $crate::UWireError> {
                    ::serde_json::to_vec(value)
                        .map($crate::__private::Bytes::from)
                        .map_err(|error| {
                            $crate::UWireError::invalid_payload(::std::string::ToString::to_string(&error))
                        })
                }
            }
            impl $crate::DecodePayload<'_, $payload> for $name {
                fn decode_payload(
                    bytes: &[u8],
                ) -> ::core::result::Result<$payload, $crate::UWireError> {
                    ::serde_json::from_slice(bytes).map_err(|error| {
                        $crate::UWireError::invalid_payload(::std::string::ToString::to_string(&error))
                    })
                }
            }
        )+
    };
}

/// Declares a codec that maps domain types through an existing wire type's codec.
///
/// Each mapping is a stateless pair of functions or non-capturing closures, one
/// constructing the wire value from a borrowed domain value and one rebuilding the
/// domain value. Payload identity and family come from the base codec.
///
/// ```ignore
/// up_rust::mapped_codec! {
///     pub struct LevelCodec = "meter.level.v1" via ProtobufPayload, wire UInt32Value;
///     Reading => |reading| uint32(reading.value), |wire| Reading { value: wire.value };
/// }
/// ```
#[macro_export]
macro_rules! mapped_codec {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident = $label:literal via $base:ty, wire $wire:ty;
        $($payload:ty => $encode:expr, $decode:expr;)+
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default)]
        $vis struct $name;
        impl $crate::PayloadCodecIdentity for $name {
            fn name() -> &'static str {
                $label
            }
            fn encoding() -> $crate::PayloadEncoding {
                <$base as $crate::PayloadCodecIdentity>::encoding()
            }
            fn payload_family() -> ::core::option::Option<$crate::WireIdentity> {
                <$base as $crate::PayloadCodecIdentity>::payload_family()
            }
        }
        $(
            impl $crate::EncodePayload<$payload> for $name {
                fn payload_layout(
                    value: &$payload,
                ) -> ::core::result::Result<$crate::PayloadLayout, $crate::UWireError> {
                    let to_wire: fn(&$payload) -> $wire = $encode;
                    <$base as $crate::EncodePayload<$wire>>::payload_layout(&to_wire(value))
                }
                fn encode_payload(
                    value: &$payload,
                    target: &mut [u8],
                ) -> ::core::result::Result<(), $crate::UWireError> {
                    let to_wire: fn(&$payload) -> $wire = $encode;
                    <$base as $crate::EncodePayload<$wire>>::encode_payload(&to_wire(value), target)
                }
                fn encode_payload_owned(
                    value: &$payload,
                ) -> ::core::result::Result<$crate::__private::Bytes, $crate::UWireError> {
                    let to_wire: fn(&$payload) -> $wire = $encode;
                    <$base as $crate::EncodePayload<$wire>>::encode_payload_owned(&to_wire(value))
                }
            }
            impl $crate::DecodePayload<'_, $payload> for $name {
                fn decode_payload(
                    bytes: &[u8],
                ) -> ::core::result::Result<$payload, $crate::UWireError> {
                    let from_wire: fn($wire) -> $payload = $decode;
                    let wire = <$base as $crate::DecodePayload<'_, $wire>>::decode_payload(bytes)?;
                    ::core::result::Result::Ok(from_wire(wire))
                }
            }
        )+
    };
}

/// A sending-only schema declaration; no decoder is required.
pub trait EncodeProfile<P> {
    /// Resolves the real schema encoder, independently of the envelope.
    fn encoder(&self) -> Result<Encoder<P>, crate::UWireError>;
}

/// A receiving-only schema declaration; no encoder is required.
pub trait DecodeProfile<P> {
    /// Resolves the real schema decoder, independently of the envelope.
    fn decoder(&self) -> Result<Decoder<P>, crate::UWireError>;
}

impl<P, M: MappingProfile<P>> EncodeProfile<P> for M {
    fn encoder(&self) -> Result<Encoder<P>, crate::UWireError> {
        MappingProfile::encoder(self)
    }
}

impl<P, M: MappingProfile<P>> DecodeProfile<P> for M {
    fn decoder(&self) -> Result<Decoder<P>, crate::UWireError> {
        MappingProfile::decoder(self)
    }
}

/// Named owned-carriage limits. Defaults are bounded, not throughput promises.
#[derive(Clone, Copy, Debug)]
pub struct DeploymentOptions {
    /// Encoded input bound; decoded allocations can be larger.
    pub payload_limit: crate::PayloadDecodeLimit,
    /// Bounded receive queue and overflow policy.
    pub registration: crate::RegistrationConfig,
    /// Reserved credential bytes.
    pub max_token_len: usize,
}

impl Default for DeploymentOptions {
    fn default() -> Self {
        Self {
            payload_limit: crate::PayloadDecodeLimit::new(1024 * 1024),
            registration: crate::RegistrationConfig {
                capacity: 64,
                overflow: crate::OverflowPolicy::DropNewest,
            },
            max_token_len: 256,
        }
    }
}

impl Default for OperationOptions {
    fn default() -> Self {
        Self {
            publish: PublishOptions::default(),
            rpc: RpcOptions {
                max_in_flight: 16,
                ttl: std::time::Duration::from_secs(5),
                max_token_len: 256,
                priority: crate::UPriority::CS4,
            },
            server: ServerOptions { max_clients: 16 },
        }
    }
}

impl Deployment {
    /// Selects this deployment's schema profile; `BoundDeployment::new` spelled forward.
    pub fn bind<M>(self, profile: M) -> BoundDeployment<M> {
        BoundDeployment::new(self, profile)
    }

    /// Selects [`ProtobufProfile`] for generated protobuf types, with no profile enum.
    pub fn protobuf(self) -> BoundDeployment<ProtobufProfile> {
        self.bind(ProtobufProfile)
    }

    /// Admits one local deployment with bounded ordinary defaults.
    pub fn with_defaults(
        transport: Arc<dyn crate::UOwnedTransport>,
        address: Arc<dyn LocalUriProvider>,
    ) -> Result<Self, SerializedError> {
        Self::configured(transport, address, DeploymentOptions::default())
    }

    /// Named advanced limits use the same admission path as `new`.
    pub fn configured(
        transport: Arc<dyn crate::UOwnedTransport>,
        address: Arc<dyn LocalUriProvider>,
        options: DeploymentOptions,
    ) -> Result<Self, SerializedError> {
        Self::new(
            transport,
            address,
            options.payload_limit,
            options.registration,
            options.max_token_len,
        )
    }
}

/// Declares zero or more independent resources in any order, with no peers.
/// This is the schema declaration every application and library uses: role
/// construction then names only the deployment and handler it needs.
///
/// ```
/// up_rust::declare_operations! {
///     pub event TEMPERATURE: u32 = 0x8001;
///     pub event PRESSURE: u32 = 0x8002;
///     pub rpc READ: u32 => u32 = 1;
/// }
/// ```
/// A declaration alone grants neither a payload mapping nor a native layout.
#[macro_export]
macro_rules! declare_operations {
    () => {};
    ($vis:vis event $name:ident: $payload:ty = $id:expr; $($rest:tt)*) => {
        #[doc = "Schema-owned independent event."]
        $vis const $name: $crate::communication::application::Event<$payload> =
            $crate::communication::application::Event::new($id);
        $crate::declare_operations! { $($rest)* }
    };
    ($vis:vis notification $name:ident: $payload:ty = $id:expr; $($rest:tt)*) => {
        #[doc = "Schema-owned independent notification."]
        $vis const $name: $crate::communication::application::Notification<$payload> =
            $crate::communication::application::Notification::new($id);
        $crate::declare_operations! { $($rest)* }
    };
    ($vis:vis rpc $name:ident: $request:ty => $response:ty = $id:expr; $($rest:tt)*) => {
        #[doc = "Schema-owned independent RPC operation."]
        $vis const $name: $crate::communication::application::Rpc<$request, $response> =
            $crate::communication::application::Rpc::new($id);
        $crate::declare_operations! { $($rest)* }
    };
}
