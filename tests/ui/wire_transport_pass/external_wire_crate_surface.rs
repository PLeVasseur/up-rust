/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

#[path = "../../support/fake_external_wire_crate/mod.rs"]
mod fake_external_wire_crate;

use fake_external_wire_crate::{metadata, ExternalFixture, ExternalTxCore, FakeExternalWire};
use up_rust::{
    DecodePayload, EncodePayload, PreparedTxLoanSpec, ProtobufMetadataCodec, UTxLoanSpec,
    UWireDecode, UWireEncode, UWireLoan, UWireMetadata, UWireReadDecode,
    UWithProtobufMetadataWire, UZeroCopyTransportExt, ValidatedTxLoanSpec,
};

fn assert_external_wire<W>()
where
    W: UWireMetadata
        + UWireEncode<ExternalFixture>
        + for<'a> UWireDecode<'a, ExternalFixture>
        + UWireReadDecode<ExternalFixture>
        + UWireLoan<ExternalFixture>,
{
}

async fn selected_wire_zero_copy_tx_compiles() -> Result<(), up_rust::UStatus> {
    let transport = ExternalTxCore.with_protobuf_metadata_wire(FakeExternalWire);
    transport
        .send_loaned_payload::<ExternalFixture>(metadata(), |payload| {
            payload.signal_id = 7;
            payload.value = 42;
        })
        .await
}

fn main() {
    assert_external_wire::<FakeExternalWire>();

    let fixture = ExternalFixture {
        signal_id: 7,
        value: 42,
    };
    let encoded = FakeExternalWire::encode_payload_owned(&fixture).expect("encode fake external");
    let decoded = FakeExternalWire::decode_payload(&encoded).expect("decode fake external");
    assert_eq!(decoded, fixture);

    let spec = UTxLoanSpec::payload(metadata(), encoded.len(), 1).expect("loan spec");
    let prepared = PreparedTxLoanSpec::from_validated::<FakeExternalWire, ProtobufMetadataCodec>(
        ValidatedTxLoanSpec::try_from(spec).expect("validated spec"),
        &ProtobufMetadataCodec,
    )
    .expect("prepared fake external TX");
    assert!(!prepared.encoded_metadata().is_empty());

    let _ = selected_wire_zero_copy_tx_compiles;
}
