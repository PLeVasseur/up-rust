/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

#[cfg(feature = "protobuf-wire")]
const UPROTOCOL_BASE_URI: &str = "up-spec/up-core-api/";

#[cfg(feature = "payload-contract-fixtures")]
const PAYLOAD_CONTRACT_PROTO_BASE_URI: &str = "test-fixtures/proto/";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "protobuf-wire")]
    generate_up_core_api()?;

    #[cfg(feature = "payload-contract-fixtures")]
    generate_payload_contract_fixtures()?;

    Ok(())
}

#[cfg(feature = "protobuf-wire")]
fn generate_up_core_api() -> Result<(), Box<dyn std::error::Error>> {
    use protobuf_codegen::Customize;

    protobuf_codegen::Codegen::new()
        .protoc()
        .protoc_path(&protoc_bin_vendored::protoc_bin_path()?)
        .customize(Customize::default().tokio_bytes(true))
        .include(UPROTOCOL_BASE_URI)
        .inputs([
            format!("{UPROTOCOL_BASE_URI}uprotocol/uoptions.proto"),
            format!("{UPROTOCOL_BASE_URI}uprotocol/v1/ucode.proto"),
            format!("{UPROTOCOL_BASE_URI}uprotocol/v1/uuid.proto"),
            format!("{UPROTOCOL_BASE_URI}uprotocol/v1/uri.proto"),
            format!("{UPROTOCOL_BASE_URI}uprotocol/v1/uattributes.proto"),
            format!("{UPROTOCOL_BASE_URI}uprotocol/v1/umessage.proto"),
            format!("{UPROTOCOL_BASE_URI}uprotocol/core/usubscription/v3/usubscription.proto"),
        ])
        .cargo_out_dir("uprotocol")
        .run_from_script();

    Ok(())
}

#[cfg(feature = "payload-contract-fixtures")]
fn generate_payload_contract_fixtures() -> Result<(), Box<dyn std::error::Error>> {
    use protobuf_codegen::Customize;

    let inputs = [
        format!("{PAYLOAD_CONTRACT_PROTO_BASE_URI}uprotocol/bench/v1/common.proto"),
        format!("{PAYLOAD_CONTRACT_PROTO_BASE_URI}uprotocol/bench/v1/can.proto"),
        format!("{PAYLOAD_CONTRACT_PROTO_BASE_URI}uprotocol/bench/v1/someip_signal_batch.proto"),
        format!("{PAYLOAD_CONTRACT_PROTO_BASE_URI}uprotocol/bench/v1/stream_chunk.proto"),
        format!(
            "{PAYLOAD_CONTRACT_PROTO_BASE_URI}uprotocol/bench/v1/radar_ars548_detection_list.proto"
        ),
        format!("{PAYLOAD_CONTRACT_PROTO_BASE_URI}uprotocol/bench/v1/lidar_point_cloud.proto"),
        format!("{PAYLOAD_CONTRACT_PROTO_BASE_URI}uprotocol/bench/v1/camera_bayer_frame.proto"),
        format!(
            "{PAYLOAD_CONTRACT_PROTO_BASE_URI}uprotocol/bench/v1/camera_carla_bgra_frame.proto"
        ),
    ];

    for input in &inputs {
        println!("cargo:rerun-if-changed={input}");
    }

    protobuf_codegen::Codegen::new()
        .protoc()
        .protoc_path(&protoc_bin_vendored::protoc_bin_path()?)
        .customize(Customize::default())
        .include(PAYLOAD_CONTRACT_PROTO_BASE_URI)
        .inputs(inputs)
        .cargo_out_dir("payload_contract_fixtures")
        .run_from_script();

    Ok(())
}
