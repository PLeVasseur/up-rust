//! The same Reading schema for host and no_std publisher services.
#[path = "independent_schema/reading.rs"]
mod reading;
pub use reading::{Format, Reading, ReadingProtobuf, PRESSURE_RESOURCE, TEMPERATURE_RESOURCE};

#[cfg(feature = "typed-communication")]
mod legacy {
use super::{Format, Reading, ReadingProtobuf};
use protobuf::well_known_types::wrappers::UInt32Value;
use up_rust::{
    communication::application::{self, BoundDeployment},
    ProtobufPayload,
};

#[repr(C)]
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    derive(up_rust::StablePayload, up_rust::StablePayloadInit)
)]
#[cfg_attr(
    feature = "up-l2-zero-copy",
    stable_payload(type_name = "independent.Query", revision = 1)
)]
pub struct Query {
    pub slot: u32,
}

up_rust::declare_operations! {
    pub event TEMPERATURE: Reading = 0x8001;
    pub rpc READ: Query => Reading = 1;
    pub event PRESSURE: Reading = 0x8002;
    pub notification ALARM: Reading = 0x8003;
    pub rpc READ_BACKUP: Query => Reading = 2;
    pub notification ALERT: Reading = 0x8004;
}

/// A deployment bound to this schema's selected format.
pub type LocalDeployment = BoundDeployment<Format>;

pub fn bind(local: application::Deployment) -> LocalDeployment {
    local.bind(Format::default())
}

pub fn bind_format(local: application::Deployment, format: Format) -> LocalDeployment {
    local.bind(format)
}

fn uint32(value: u32) -> UInt32Value {
    let mut wire = UInt32Value::new();
    wire.value = value;
    wire
}

// Legacy trait impls attach to the same marker as the bounded Wire adapter.
impl up_rust::PayloadCodecIdentity for ReadingProtobuf {
    fn name() -> &'static str { "independent.Reading.protobuf.v1" }
    fn encoding() -> up_rust::PayloadEncoding {
        <ProtobufPayload as up_rust::PayloadCodecIdentity>::encoding()
    }
    fn payload_family() -> Option<up_rust::WireIdentity> {
        <ProtobufPayload as up_rust::PayloadCodecIdentity>::payload_family()
    }
}
impl up_rust::EncodePayload<Reading> for ReadingProtobuf {
    fn payload_layout(value: &Reading) -> Result<up_rust::PayloadLayout, up_rust::UWireError> {
        <ProtobufPayload as up_rust::EncodePayload<UInt32Value>>::payload_layout(&uint32(value.value))
    }
    fn encode_payload(value: &Reading, target: &mut [u8]) -> Result<(), up_rust::UWireError> {
        <ProtobufPayload as up_rust::EncodePayload<UInt32Value>>::encode_payload(&uint32(value.value), target)
    }
    fn encode_payload_owned(value: &Reading) -> Result<up_rust::__private::Bytes, up_rust::UWireError> {
        <ProtobufPayload as up_rust::EncodePayload<UInt32Value>>::encode_payload_owned(&uint32(value.value))
    }
}
impl up_rust::DecodePayload<'_, Reading> for ReadingProtobuf {
    fn decode_payload(bytes: &[u8]) -> Result<Reading, up_rust::UWireError> {
        let wire = <ProtobufPayload as up_rust::DecodePayload<'_, UInt32Value>>::decode_payload(bytes)?;
        Ok(Reading { value: wire.value })
    }
}
up_rust::mapped_codec! {
    pub struct QueryProtobuf = "independent.Query.protobuf.v1" via ProtobufPayload, wire UInt32Value;
    Query => |query| uint32(query.slot), |wire| Query { slot: wire.value };
}
up_rust::json_codec!(pub struct Json = ("example.independent.json-v1", 0xF103) for Reading, Query);

up_rust::schema_mapping!(Format, Reading {
    Format::Protobuf => (ReadingProtobuf, ReadingProtobuf),
    Format::Json => (Json, Json),
});
up_rust::schema_mapping!(Format, Query {
    Format::Protobuf => (QueryProtobuf, QueryProtobuf),
    Format::Json => (Json, Json),
});

}
#[cfg(feature = "typed-communication")]
pub use legacy::*;
