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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "protobuf-wire")]
    generate_up_core_api()?;

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
            format!("{UPROTOCOL_BASE_URI}uprotocol/v1/uri.proto"),
            format!("{UPROTOCOL_BASE_URI}uprotocol/core/usubscription/v3/usubscription.proto"),
        ])
        .cargo_out_dir("uprotocol")
        .run_from_script();

    Ok(())
}
