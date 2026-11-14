//! One application with real Protobuf/JSON mappings and unchanged business logic.
use protobuf::well_known_types::wrappers::BoolValue;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use up_rust::{
    communication::application::*, OverflowPolicy, ProtobufPayload, RegistrationConfig,
    StaticUriProvider, UCode, UEntityId, UPriority,
};

#[repr(C)]
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    derive(up_rust::StablePayload, up_rust::StablePayloadInit)
)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    stable_payload(type_name = "vehicle.DoorState", revision = 1)
)]
pub(crate) struct DoorState {
    pub(crate) open: bool,
}
#[repr(C)]
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    derive(up_rust::StablePayload, up_rust::StablePayloadInit)
)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    stable_payload(type_name = "vehicle.SetDoor", revision = 1)
)]
pub(crate) struct SetDoor {
    pub(crate) open: bool,
}

fn bool_value(value: bool) -> BoolValue {
    let mut wire = BoolValue::new();
    wire.value = value;
    wire
}

// Both domain types share one wire schema; the route selects which is expected.
up_rust::mapped_codec! {
    pub(crate) struct DoorProtobuf = "vehicle.door.protobuf.BoolValue.v1" via ProtobufPayload, wire BoolValue;
    DoorState => |state| bool_value(state.open), |wire| DoorState { open: wire.value };
    SetDoor => |request| bool_value(request.open), |wire| SetDoor { open: wire.value };
}

// Deployment-private family declared in examples/declarations/door-json.json.
// The alias is not a registered assignment; peers must match the full literal.
up_rust::json_codec!(pub(crate) struct DoorJson = ("example.vehicle.door.json-v1", 0xF101) for DoorState, SetDoor);

#[derive(Debug)]
pub(crate) struct DoorObserver(pub(crate) Arc<Mutex<Vec<bool>>>);
impl Handler<DoorState> for DoorObserver {
    async fn on_receive<D: Delivery<DoorState>>(&mut self, delivery: D) {
        // A native delivery re-checks on every borrow; report a failed check, never panic.
        match delivery.payload() {
            Ok(state) => {
                // The borrow remains bound to its retained delivery across this await.
                tokio::task::yield_now().await;
                if let Ok(mut observed) = self.0.lock() {
                    observed.push(state.open);
                }
            }
            Err(error) => eprintln!("door observer rejected a delivery: {error}"),
        }
    }
}
#[derive(Debug)]
pub(crate) struct DoorController;
impl RequestHandler<SetDoor> for DoorController {
    type Reply = bool;
    async fn handle<D: Delivery<SetDoor>>(&mut self, request: D) -> Result<bool, UCode> {
        if request.context().map_err(|_| UCode::InvalidArgument)?.token != Some("door-control") {
            return Err(UCode::PermissionDenied);
        }
        let open = request.payload().map_err(|_| UCode::InvalidArgument)?.open;
        tokio::task::yield_now().await;
        Ok(open)
    }
}

pub(crate) fn registration() -> RegistrationConfig {
    RegistrationConfig {
        capacity: 8,
        overflow: OverflowPolicy::DropNewest,
    }
}
pub(crate) fn rpc_options() -> RpcOptions {
    RpcOptions {
        max_in_flight: 4,
        ttl: Duration::from_secs(2),
        max_token_len: 32,
        priority: UPriority::CS4,
    }
}
pub(crate) fn uris() -> (Arc<StaticUriProvider>, Arc<StaticUriProvider>) {
    let address = |type_id| {
        StaticUriProvider::for_entity("vehicle", UEntityId::first_instance(type_id), 1)
            .expect("valid Door address")
    };
    (Arc::new(address(1)), Arc::new(address(2)))
}
