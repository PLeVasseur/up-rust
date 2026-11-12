//! Schema declarations shared by independently deployed programs.
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
    stable_payload(type_name = "independent.Reading", revision = 1)
)]
pub struct Reading {
    pub value: u32,
}

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

#[derive(Clone, Copy, Debug, Default)]
pub enum Format {
    #[default]
    Protobuf,
    Json,
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

up_rust::mapped_codec! {
    pub struct ReadingProtobuf = "independent.Reading.protobuf.v1" via ProtobufPayload, wire UInt32Value;
    Reading => |reading| uint32(reading.value), |wire| Reading { value: wire.value };
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
