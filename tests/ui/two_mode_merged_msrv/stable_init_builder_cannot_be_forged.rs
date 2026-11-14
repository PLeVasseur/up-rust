//! A builder's fields are visible in the module that derives it, but a finished
//! builder still cannot be made there in safe code.
use up_rust::payload::stable::{
    InitializedStablePayload, StablePayloadInitSet as Set, StablePayloadInitUnset as Unset,
};

#[derive(up_rust::StablePayload, up_rust::StablePayloadInit)]
#[repr(C)]
#[stable_payload(type_name = "uprotocol.test.Forged")]
struct Forged {
    id: u32,
    open: u32,
}

fn forge(
    start: __UpRustStablePayloadInitForForged<'_, Unset, Unset>,
) -> InitializedStablePayload<'_, Forged> {
    let finished: __UpRustStablePayloadInitForForged<'_, Set, Set> =
        __UpRustStablePayloadInitForForged {
            payload: start.payload,
            marker: core::marker::PhantomData,
        };
    finished.finish()
}

fn main() {}
