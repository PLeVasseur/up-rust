use up_rust::{StablePayload, StablePayloadInit};

mod local_names {
    pub enum Set {}
    pub enum Unset {}
    pub struct LoanedInitPayload;
    pub mod up_rust {}
}

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "example.init.Hygiene")]
struct Hygiene {
    value: u32,
}

fn build<'a>(
    init: <Hygiene as up_rust::payload::StablePayloadInit>::Init<'a>,
) -> Result<up_rust::payload::InitializedStablePayload<Hygiene>, up_rust::UWireError> {
    let _ = core::mem::size_of::<local_names::LoanedInitPayload>();
    init.value(1).finish()
}

fn main() {}
