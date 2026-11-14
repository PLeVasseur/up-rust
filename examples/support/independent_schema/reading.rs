//! Single domain declaration and schema selection, shared by host and embedded.

#[repr(C)]
#[derive(Debug, PartialEq)]
#[cfg_attr(feature = "typed-communication", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "up-l2-zero-copy", derive(up_rust::StablePayload, up_rust::StablePayloadInit))]
#[cfg_attr(feature = "up-l2-zero-copy", stable_payload(type_name = "independent.Reading", revision = 1))]
pub struct Reading { pub value: u32 }

/// The original schema marker; host legacy codec impls and the bounded Wire impl
/// attach to this same type, rather than declaring a second Reading or marker.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReadingProtobuf;

#[derive(Clone, Copy, Debug, Default)]
pub enum Format {
    #[default]
    Protobuf,
    Json,
}

pub const TEMPERATURE_RESOURCE: u16 = 0x8001;
pub const PRESSURE_RESOURCE: u16 = 0x8002;

#[path = "reading_protobuf_core.rs"]
mod reading_protobuf_core;

static READING_PROTOBUF: ReadingProtobuf = ReadingProtobuf;

impl up_rust_application::application::PublisherMapping<Reading> for Format {
    fn publisher_wire(&self) -> Result<&'static dyn up_rust_core::wire::Wire<Reading>,
                                      up_rust_core::transport::StartupError> {
        match self {
            Self::Protobuf => Ok(&READING_PROTOBUF),
            Self::Json => Err(up_rust_core::transport::StartupError::InvalidConfig(
                "independent.Reading JSON has no bounded wire; choose a qualified profile")),
        }
    }
}
