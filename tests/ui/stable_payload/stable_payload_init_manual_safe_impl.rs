#[repr(C)]
#[derive(up_rust::StablePayload)]
#[stable_payload(type_name = "example.init.fail.ManualSafe")]
struct ManualSafe;

impl up_rust::payload::StablePayloadInit for ManualSafe {
    type Init<'a> = ();

    fn init_from_uninit_payload<'a>(
        _payload: up_rust::zero_copy::LoanedPayloadUninitMut<'a>,
    ) -> Result<Self::Init<'a>, up_rust::UWireError> {
        Ok(())
    }

    fn __init_from_slot<'a>(
        _slot: up_rust::__derive_support::StablePayloadInitSlot<'a, Self>,
    ) -> Result<Self::Init<'a>, up_rust::UWireError> {
        Ok(())
    }
}

fn main() {}
