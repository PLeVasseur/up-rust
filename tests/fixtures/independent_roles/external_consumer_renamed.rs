//! Explicit legacy owned-carriage compatibility control. Publisher annotations
//! use LegacyPublisher; this is not a common bounded-core migration.
//! Compile-pass control: an external consumer with a RENAMED SDK dependency.
//!
//! Compile as a library with `--extern sdk=<pinned up_rust rlib>` ONLY, so the
//! name `up_rust` does not exist in this crate: every SDK macro used here must
//! expand through `$crate`. Nested modules, interleaved and repeated
//! resources, separately deployed serialized and native programs, mixed stored
//! roles and two separately named native response owners are composed with no
//! all-role app, opposite peer, dummy resource or codec marker. Native payloads
//! are the SDK's published stable fixture types because the stable-payload
//! derives cannot yet target a renamed crate (see the defect witness).
//! Compile-only: nothing runs, and `ConsumerCore` is a heap-backed witness,
//! not middleware.
//!
//! Second gate form: compiled with `--extern up_rust=<rlib> --cfg up_rust_alias`,
//! the in-source rename below supplies `sdk` instead of a Cargo-level rename.
//! That form keeps `up_rust` in the extern prelude, so only the first form
//! detects hard-coded `::up_rust` paths.

#[cfg(up_rust_alias)]
extern crate up_rust as sdk;

pub mod schema {
    /// Handwritten domain value with a real, explicit schema mapping.
    #[derive(Debug, PartialEq)]
    pub struct Level {
        pub value: u32,
    }

    /// Fixed-width little-endian mapping for `Level`.
    #[derive(Debug)]
    pub struct LevelLe;

    impl sdk::PayloadCodecIdentity for LevelLe {
        fn name() -> &'static str {
            "external.level.le.v1"
        }
        fn encoding() -> sdk::PayloadEncoding {
            sdk::PayloadEncoding::RAW
        }
        fn payload_family() -> Option<sdk::WireIdentity> {
            Some(sdk::WireIdentity::new(
                "example.external.level-le-v1",
                0xF1A0,
            ))
        }
    }

    impl sdk::EncodePayload<Level> for LevelLe {
        fn payload_layout(_: &Level) -> Result<sdk::PayloadLayout, sdk::UWireError> {
            sdk::PayloadLayout::new(4, 1)
        }
        fn encode_payload(value: &Level, target: &mut [u8]) -> Result<(), sdk::UWireError> {
            let bytes = value.value.to_le_bytes();
            if target.len() != bytes.len() {
                return Err(sdk::UWireError::invalid_payload(
                    "exact level extent required",
                ));
            }
            target.copy_from_slice(&bytes);
            Ok(())
        }
    }

    impl sdk::DecodePayload<'_, Level> for LevelLe {
        fn decode_payload(bytes: &[u8]) -> Result<Level, sdk::UWireError> {
            let bytes: [u8; 4] = bytes
                .try_into()
                .map_err(|_| sdk::UWireError::invalid_payload("exact level extent required"))?;
            Ok(Level {
                value: u32::from_le_bytes(bytes),
            })
        }
    }

    /// Deployment-selected schema profile.
    #[derive(Clone, Copy, Debug)]
    pub enum Profile {
        LittleEndian,
    }

    sdk::schema_mapping!(Profile, Level {
        Profile::LittleEndian => (LevelLe, LevelLe),
    });

    pub mod operations {
        // Interleaved kinds and repeated kinds across two independent invocations.
        sdk::declare_operations! {
            pub event LEVEL: super::Level = 0x8001;
            pub rpc SET: super::Level => super::Level = 1;
            pub notification ALERT: super::Level = 0x8002;
            pub event SPARE: super::Level = 0x8003;
            pub rpc RESET: super::Level => super::Level = 2;
        }
        sdk::declare_operations! {
            pub rpc LOCATE: sdk::bench_fixtures::payload_contract::RoiV1
                => sdk::bench_fixtures::payload_contract::SignalSampleV1 = 3;
            pub event POSE: sdk::bench_fixtures::payload_contract::Vector3fV1 = 0x8010;
            pub rpc SAMPLE: sdk::bench_fixtures::payload_contract::RoiV1
                => sdk::bench_fixtures::payload_contract::SignalSampleV1 = 4;
            pub notification MOVED: sdk::bench_fixtures::payload_contract::Vector3fV1 = 0x8011;
        }
    }
}

/// Serialized programs, each with one local deployment and peer addresses.
pub mod serialized {
    /// Publishers and clients of repeated kinds; the controller is an address.
    pub mod telemetry {
        use crate::schema::{operations, Level, Profile};
        use sdk::communication::application::{
            BoundDeployment, OperationOptions, LegacyPublisher, RpcClient, SerializedError,
        };

        pub struct Telemetry {
            pub level: LegacyPublisher<Level>,
            pub set: RpcClient<Level, Level>,
            pub spare: LegacyPublisher<Level>,
            pub reset: RpcClient<Level, Level>,
        }

        impl Telemetry {
            pub async fn prepare(
                local: &BoundDeployment<Profile>,
                controller: &dyn sdk::LocalUriProvider,
            ) -> Result<Self, SerializedError> {
                let options = OperationOptions::default();
                Ok(Self {
                    level: operations::LEVEL.publisher(local, options.publish)?,
                    set: operations::SET
                        .client(local, controller, options.rpc)
                        .await?,
                    spare: operations::SPARE.publisher(local, options.publish)?,
                    reset: operations::RESET
                        .client(local, controller, options.rpc)
                        .await?,
                })
            }

            pub async fn report(&self, value: u32) -> Result<(), SerializedError> {
                self.level.publish(&Level { value }).await?;
                self.spare.publish(&Level { value }).await
            }
        }
    }

    /// A receiver and two servers; telemetry is only an address.
    pub mod controller {
        use crate::schema::{operations, Level, Profile};
        use sdk::communication::application::{
            BoundDeployment, Delivery, Handler, OperationOptions, Receiver, RequestHandler,
            RpcServer, SerializedError,
        };

        pub struct Clamp;

        impl RequestHandler<Level> for Clamp {
            type Reply = u32;
            async fn handle<D: Delivery<Level>>(&mut self, request: D) -> Result<u32, sdk::UCode> {
                Ok(request
                    .payload()
                    .map_err(|_| sdk::UCode::InvalidArgument)?
                    .value
                    .min(100))
            }
        }

        pub struct Count(pub std::sync::Arc<std::sync::atomic::AtomicU32>);

        impl Handler<Level> for Count {
            async fn on_receive<D: Delivery<Level>>(&mut self, delivery: D) {
                if let Ok(level) = delivery.payload() {
                    self.0
                        .store(level.value, std::sync::atomic::Ordering::Release);
                }
            }
        }

        pub struct Controller {
            pub alerts: Receiver,
            pub set: RpcServer,
            pub reset: RpcServer,
        }

        impl Controller {
            pub async fn prepare(
                local: &BoundDeployment<Profile>,
                telemetry: &dyn sdk::LocalUriProvider,
                latest: Count,
            ) -> Result<Self, SerializedError> {
                let options = OperationOptions::default();
                Ok(Self {
                    alerts: operations::ALERT.receiver(local, telemetry, latest).await?,
                    set: operations::SET
                        .server(local, Clamp, |value| Level { value }, options.server)
                        .await?,
                    reset: operations::RESET
                        .server(local, Clamp, |value| Level { value }, options.server)
                        .await?,
                })
            }
        }
    }
}

/// Compile-only consumer witness over the SDK's heap-backed pool.
pub mod witness {
    use sdk::{
        FrameSink, InitializedTx, LoanError, NativeLoanCore, PoolCore, PreparedTransportCore,
        RouteLimits, SendError, SetupError, TxFrameLayout, UUri,
    };
    use std::sync::Arc;

    /// Heap-backed test witness; it is not installed middleware.
    #[derive(Debug)]
    pub struct ConsumerCore(pub Arc<PoolCore>);

    impl NativeLoanCore for ConsumerCore {}

    impl PreparedTransportCore for ConsumerCore {
        type Route = <PoolCore as PreparedTransportCore>::Route;
        type Tx<'a>
            = <PoolCore as PreparedTransportCore>::Tx<'a>
        where
            Self: 'a;
        type Rx = <PoolCore as PreparedTransportCore>::Rx;
        type Subscription = <PoolCore as PreparedTransportCore>::Subscription;

        fn prepare_route(
            &self,
            source: &UUri,
            sink: Option<&UUri>,
            limits: RouteLimits,
        ) -> Result<Self::Route, SetupError> {
            self.0.prepare_route(source, sink, limits)
        }

        fn loan<'a>(
            &'a self,
            route: &'a Self::Route,
            layout: TxFrameLayout,
        ) -> Result<Self::Tx<'a>, LoanError> {
            self.0.loan(route, layout)
        }

        fn publish<'a>(
            &'a self,
            route: &'a Self::Route,
            frame: InitializedTx<Self::Tx<'a>>,
        ) -> Result<(), SendError> {
            self.0.publish(route, frame)
        }

        fn attach(
            &self,
            source: &UUri,
            sink: Option<&UUri>,
            inbox: Arc<dyn FrameSink<Self::Rx>>,
        ) -> impl std::future::Future<Output = Result<Self::Subscription, SetupError>> + Send
        {
            self.0.attach(source, sink, inbox)
        }

        fn detach(&self, subscription: &mut Self::Subscription) -> Result<(), SetupError> {
            self.0.detach(subscription)
        }
    }
}

/// Native programs with SDK-generated concrete role names.
pub mod native {
    /// Mixed roles with two separately named retained response owners.
    pub mod locator {
        use sdk::bench_fixtures::payload_contract::Vector3fV1;

        sdk::native_roles! {
            pub mod roles {
                core: crate::witness::ConsumerCore;
                client Locate, locate, LocateResponse: crate::schema::operations::LOCATE
                    => sdk::bench_fixtures::payload_contract::RoiV1,
                    sdk::bench_fixtures::payload_contract::SignalSampleV1;
                publisher Pose, pose: crate::schema::operations::POSE
                    => sdk::bench_fixtures::payload_contract::Vector3fV1;
                client Sample, sample, SampleResponse: crate::schema::operations::SAMPLE
                    => sdk::bench_fixtures::payload_contract::RoiV1,
                    sdk::bench_fixtures::payload_contract::SignalSampleV1;
                receiver Moves, moves: crate::schema::operations::MOVED
                    => sdk::bench_fixtures::payload_contract::Vector3fV1;
            }
        }

        pub struct Locator {
            pub locate: roles::Locate,
            pub pose: roles::Pose,
            pub sample: roles::Sample,
            pub moves: roles::Moves,
            pub last_location: Option<roles::LocateResponse>,
            pub last_sample: Option<roles::SampleResponse>,
        }

        impl Locator {
            pub async fn prepare<H: sdk::communication::application::Handler<Vector3fV1>>(
                local: &roles::Deployment,
                sensor: &dyn sdk::LocalUriProvider,
                moves: H,
            ) -> Result<Self, sdk::SetupError> {
                Ok(Self {
                    locate: roles::locate(local, sensor).await?,
                    pose: roles::pose(local)?,
                    sample: roles::sample(local, sensor).await?,
                    moves: roles::moves(local, sensor, moves).await?,
                    last_location: None,
                    last_sample: None,
                })
            }

            pub fn publish_pose(&self, x: f32) -> Result<(), sdk::SendError> {
                self.pose
                    .publish_in_place(|loan| loan.into_initializer().x(x).y(0.0).z(0.0).finish())
            }

            pub async fn refresh(&mut self, x: u32) -> Result<(), sdk::NativeCallError> {
                let location = self.locate.invoke_in_place(|loan| {
                    loan.into_initializer()
                        .x(x)
                        .y(0)
                        .width(1)
                        .height(1)
                        .finish()
                })?;
                let sample = self.sample.invoke_in_place(|loan| {
                    loan.into_initializer()
                        .x(x)
                        .y(1)
                        .width(1)
                        .height(1)
                        .finish()
                })?;
                // Awaited out of order; each owner keeps its own name and storage.
                self.last_sample = Some(sample.response().await?);
                self.last_location = Some(location.response().await?);
                Ok(())
            }

            pub fn location_signal(&self) -> Result<Option<u32>, sdk::AdmissionError> {
                self.last_location
                    .as_ref()
                    .map(|owner| owner.payload().map(|signal| signal.signal_id))
                    .transpose()
            }
        }
    }

    /// Server-only and notifier-only roles; the locator is only an address.
    pub mod sensor {
        use sdk::bench_fixtures::payload_contract::RoiV1;
        use sdk::communication::application::{Delivery, RequestHandler};

        sdk::native_roles! {
            pub mod roles {
                core: crate::witness::ConsumerCore;
                server Locate, locate: crate::schema::operations::LOCATE
                    => sdk::bench_fixtures::payload_contract::RoiV1,
                    sdk::bench_fixtures::payload_contract::SignalSampleV1;
                reply u32 => |value, loan| loan
                    .into_initializer()
                    .signal_id(value)
                    .status(0)
                    .timestamp_ns(0)
                    .value(0.0)
                    .finish();
                notifier Moved, moved: crate::schema::operations::MOVED
                    => sdk::bench_fixtures::payload_contract::Vector3fV1;
                server Sample, sample: crate::schema::operations::SAMPLE
                    => sdk::bench_fixtures::payload_contract::RoiV1,
                    sdk::bench_fixtures::payload_contract::SignalSampleV1;
                reply u32 => |value, loan| loan
                    .into_initializer()
                    .signal_id(value)
                    .status(1)
                    .timestamp_ns(0)
                    .value(0.0)
                    .finish();
            }
        }

        pub struct Region;

        impl RequestHandler<RoiV1> for Region {
            type Reply = u32;
            async fn handle<D: Delivery<RoiV1>>(&mut self, request: D) -> Result<u32, sdk::UCode> {
                Ok(request
                    .payload()
                    .map_err(|_| sdk::UCode::InvalidArgument)?
                    .width)
            }
        }

        pub struct Sensor {
            pub locate: roles::Locate,
            pub moved: roles::Moved,
            pub sample: roles::Sample,
        }

        impl Sensor {
            pub async fn prepare(
                local: &roles::Deployment,
                locator: sdk::UUri,
            ) -> Result<Self, sdk::SetupError> {
                Ok(Self {
                    locate: roles::locate(local, Region).await?,
                    moved: roles::moved(local, locator)?,
                    sample: roles::sample(local, Region).await?,
                })
            }
        }
    }
}

/// The deployment owner's one native selection point for this consumer.
pub fn native_deployment(
    pool: std::sync::Arc<sdk::PoolCore>,
    agreement: sdk::NativeProfileAgreement,
    config: sdk::communication::typed::NativeConfig,
    address: std::sync::Arc<dyn sdk::LocalUriProvider>,
) -> Result<native::locator::roles::Deployment, sdk::SetupError> {
    let transport = std::sync::Arc::new(sdk::UWireTransport::with_native_agreement(
        std::sync::Arc::new(witness::ConsumerCore(pool)),
        sdk::StableContainerWire,
        sdk::UFrameFieldsMetadataCodec,
        agreement,
    ));
    native::locator::roles::Deployment::new(transport, config, address)
}
