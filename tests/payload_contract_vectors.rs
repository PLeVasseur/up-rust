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
//! Independently stored, hash-bound representative stable payload bytes.
#![cfg(feature = "payload-contract-fixtures")]

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, sync::Arc};
use up_rust::bench_fixtures::payload_contract::{self as fixtures, PayloadContractCase};
use up_rust::prelude::*;
use up_rust::{NativeProfile, NativeProfileAgreement, PayloadEncoding, UFrameMetadata, UUri};

const SEQUENCE: u32 = 17;
const _: () = assert!(
    cfg!(target_endian = "little"),
    "stored native fixture corpus requires its declared little-endian target"
);

fn directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/payload-contract")
}
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn agreement(case: &PayloadContractCase) -> NativeProfileAgreement {
    let profile = NativeProfile::new(
        "stored-fixture",
        1,
        PayloadEncoding::private_use(0xF001),
        [fixtures::stable_payload_representation(case).unwrap()],
    )
    .unwrap();
    NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap()
}
fn normal_mode() {
    assert!(
        std::env::var_os("UP_PAYLOAD_VECTOR_UPDATE").is_none(),
        "normal tests never update vectors; select the explicit ignored updater"
    );
}

#[test]
fn stored_vectors_are_complete_hash_bound_and_semantically_valid() {
    normal_mode();
    let directory = directory();
    let encoded = fs::read(directory.join("vectors.json")).expect("stored vector manifest");
    let manifest: Value = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(manifest.get("format"), Some(&json!(1)));
    assert_eq!(manifest.get("sequence"), Some(&json!(SEQUENCE)));
    assert_eq!(manifest.get("byte_order"), Some(&json!("little")));
    assert_eq!(
        fixtures::fixture_manifest().stored_vectors_manifest_sha256,
        hash(&encoded)
    );
    let rows = manifest.get("vectors").unwrap().as_array().unwrap();
    assert_eq!(rows.len(), 8);
    let mut names = std::collections::BTreeSet::new();
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(
            row.get("case_id").unwrap().as_u64().unwrap(),
            u64::try_from(index + 1).unwrap()
        );
        let name = row.get("name").unwrap().as_str().unwrap();
        let file = row.get("file").unwrap().as_str().unwrap();
        assert_eq!(file, format!("{name}.bin"));
        assert!(!file.contains('/') && !file.contains('\\'));
        assert!(names.insert(file.to_owned()));
        let stored = fs::read(directory.join(file)).expect("stored fixture bytes");
        assert_eq!(
            stored.len(),
            usize::try_from(row.get("bytes").unwrap().as_u64().unwrap()).unwrap()
        );
        assert_eq!(hash(&stored), row.get("sha256").unwrap().as_str().unwrap());
        if let Some(case) = fixtures::all_cases().find(|case| case.name() == name) {
            assert_eq!(
                row.get("type_name").unwrap().as_str().unwrap(),
                fixtures::stable_payload_type_name(case)
            );
            assert_eq!(
                row.get("alignment").unwrap().as_u64().unwrap(),
                u64::try_from(fixtures::stable_payload_align(case)).unwrap()
            );
            assert_eq!(stored.len(), fixtures::stable_payload_len(case));
            let agreement = agreement(case);
            let identity = agreement
                .identity_for(&fixtures::stable_payload_representation(case).unwrap())
                .unwrap();
            let metadata = UFrameMetadata::publish(
                UUri::try_from_parts("fixture", 0x0001_4210, 1, 0x9000).unwrap(),
            )
            .with_native_payload_identity(identity)
            .build()
            .unwrap();
            // Validate the bytes read from disk, then compare the whole stored
            // representation with independently generated current fixture bytes.
            fixtures::validate_stable_owned_bytes(case, SEQUENCE, &metadata, &agreement, &stored)
                .unwrap();
            fixtures::validate_stable_owned_bytes_exact(
                case, SEQUENCE, &metadata, &agreement, &stored,
            )
            .unwrap();
        }
    }
    let actual: std::collections::BTreeSet<_> = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(".bin"))
        .collect();
    assert_eq!(actual, names);
}

#[test]
#[ignore = "explicit source regeneration; requires UP_PAYLOAD_VECTOR_UPDATE=1 and all eight fixture types"]
fn regenerate_payload_contract_vectors() {
    assert_eq!(
        std::env::var("UP_PAYLOAD_VECTOR_UPDATE").as_deref(),
        Ok("1")
    );
    assert_eq!(
        fixtures::all_cases().count(),
        8,
        "enable payload-contract-large-fixtures"
    );
    let directory = directory();
    assert!(directory.is_dir(), "use the checked-in corpus directory");
    let mut rows = Vec::new();
    for case in fixtures::all_cases() {
        let fixture = fixtures::stable_owned_fixture_for(case, SEQUENCE, &agreement(case)).unwrap();
        let name = format!("{}.bin", case.name());
        fs::write(directory.join(&name), &fixture.bytes).unwrap();
        rows.push(
            json!({"case_id": case.case_id(), "name": case.name(), "file": name,
            "type_name": fixture.stable_type_name, "bytes": fixture.bytes.len(),
            "alignment": fixture.stable_align, "sha256": hash(&fixture.bytes)}),
        );
    }
    let mut encoded = serde_json::to_vec_pretty(&json!({"format": 1, "sequence": SEQUENCE,
        "byte_order": "little", "vectors": rows}))
    .unwrap();
    encoded.push(b'\n');
    fs::write(directory.join("vectors.json"), &encoded).unwrap();
    let mut catalog = fixtures::fixture_manifest();
    // The updater binary was compiled before this manifest was replaced. Bind
    // the just-written bytes rather than its embedded prior manifest digest.
    catalog.stored_vectors_manifest_sha256 = hash(&encoded);
    let full = serde_json::to_vec(&catalog).unwrap();
    fs::write(directory.join("fixture-manifest.full.json"), &full).unwrap();
    catalog.cases.retain(|case| case.case_id <= 6);
    let core = serde_json::to_vec(&catalog).unwrap();
    fs::write(directory.join("fixture-manifest.core.json"), &core).unwrap();
    println!("full_fixture_manifest_sha256={}", hash(&full));
    println!("core_fixture_manifest_sha256={}", hash(&core));
    println!("vectors_manifest_sha256={}", hash(&encoded));
}

#[test]
#[ignore = "explicit native-host coefficient generation; requires UP_PAYLOAD_VECTOR_UPDATE=1"]
fn regenerate_lidar_geometry_coefficients() {
    use std::fmt::Write as _;
    assert_eq!(
        std::env::var("UP_PAYLOAD_VECTOR_UPDATE").as_deref(),
        Ok("1")
    );
    let mut source = String::from("// SPDX-License-Identifier: Apache-2.0\n\
        // Frozen representative LiDAR coefficients from the original native fixture arithmetic.\n\
        // Regenerate only with the explicit ignored coefficient updater, then verify stored vectors.\n");
    let mut table = |name: &str, values: Vec<u32>| {
        writeln!(
            source,
            "pub(super) const {name}: [u32; {}] = [",
            values.len()
        )
        .unwrap();
        for chunk in values.chunks(8) {
            source.push_str("    ");
            for bits in chunk {
                write!(source, "0x{bits:08x}, ").unwrap();
            }
            source.push('\n');
        }
        source.push_str("];\n");
    };
    table(
        "AZIMUTH_COS_BITS",
        (0..1200)
            .map(|column| {
                (-60.0_f32 + column as f32 * 0.1)
                    .to_radians()
                    .cos()
                    .to_bits()
            })
            .collect(),
    );
    table(
        "AZIMUTH_SIN_BITS",
        (0..1200)
            .map(|column| {
                (-60.0_f32 + column as f32 * 0.1)
                    .to_radians()
                    .sin()
                    .to_bits()
            })
            .collect(),
    );
    table(
        "ELEVATION_SIN_BITS",
        (0..128)
            .map(|row| (-12.7_f32 + row as f32 * 0.2).to_radians().sin().to_bits())
            .collect(),
    );
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/bench_fixtures/payload_contract/lidar_geometry.rs");
    assert!(
        path.is_file(),
        "replace the existing explicit coefficient source only"
    );
    fs::write(&path, source).unwrap();
    println!("updated {}", path.display());
}
