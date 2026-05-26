struct ManualField(u8);

impl up_rust::__derive_support::ByteBackedStablePayloadField for ManualField {
    const SUPPORTS_BYTE_BACKED_STABLE_FIELD: bool = true;

    const BYTE_BACKED_STABLE_FIELD_CHECK: () = ();
}

fn main() {}
