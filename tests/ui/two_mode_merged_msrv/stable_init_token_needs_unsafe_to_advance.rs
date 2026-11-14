//! Moving a builder's state needs the token's `unsafe advance`.
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
            token: start.token,
        };
    finished.finish()
}

fn main() {}
