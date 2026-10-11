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

//! Golden byte vectors for the UFrame metadata envelope and metadata profiles.
//!
//! These vectors are the cross-language conformance seed: a non-Rust
//! implementation of the canonical UFrame field-block profile (and, where
//! applicable, the legacy protobuf profile) should reproduce these bytes
//! exactly for the documented fixtures, and decode them back to the documented
//! semantics. Fixtures are deterministic; message ids are fixed valid
//! v7 UUIDs, never generated.
//!
//! Default mode COMPARES against the checked-in vectors under
//! `tests/golden/wire-metadata/` and fails on any drift (an intentional wire
//! change must regenerate the vectors in the same commit and say so).
//! Normal tests never rewrite fixtures. Regeneration is an explicitly ignored
//! test requiring `UP_GOLDEN_UPDATE=1` and deliberate `--ignored --exact` invocation.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bytes::Bytes;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use up_rust::NativeTypeToken;
use up_rust::{
    PayloadEncoding, ProtobufMappable, UAttributes, UFrameFieldsMetadataCodec, UFrameMetadata,
    UMessageBuilder, UProtocolNativeWire, UUri, UWireMetadataCodec, UWireMetadataContext, UUID,
};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/wire-metadata")
}

fn require_normal_mode() {
    assert!(
        std::env::var_os("UP_GOLDEN_UPDATE").is_none(),
        "ordinary golden tests never update files; use the explicitly ignored updater"
    );
}

/// Fixed, valid v7/RFC4122 UUIDs (version nibble 0x7 in msb bits 12..=15,
/// variant 0b10 in lsb bits 62..=63). Deterministic by construction.
fn fixed_uuid(seq: u64) -> UUID {
    UUID::from_u64_pair(
        0x0000_018d_0000_7000 | (seq << 16),
        0x8000_0000_0000_0100 | seq,
    )
    .expect("fixed v7 uuid")
}

fn topic() -> UUri {
    UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x9000).expect("topic")
}

fn method() -> UUri {
    UUri::try_from_parts("vehicle", 0x4210, 0x01, 0x0007).expect("method")
}

fn reply_to() -> UUri {
    UUri::try_from_parts("cloud", 0x0042, 0x01, 0x0000).expect("reply-to")
}

fn project(message: &up_rust::UMessage) -> UFrameMetadata {
    up_rust::frame::metadata::try_project_umessage_to_frame_metadata(message).expect("metadata")
}

/// The deterministic fixture set. Names are stable file names; extend by
/// appending, never by renaming.
fn fixtures() -> Vec<(&'static str, &'static str, UFrameMetadata)> {
    let publish_no_payload = {
        let m = UMessageBuilder::publish(topic())
            .with_message_id(fixed_uuid(1))
            .build()
            .expect("message");
        project(&m)
    };
    let publish_raw_payload = {
        let m = UMessageBuilder::publish(topic())
            .with_message_id(fixed_uuid(2))
            .build_with_payload(Bytes::from_static(b"payload"), PayloadEncoding::RAW)
            .expect("message");
        project(&m)
    };
    let publish_private_use_payload = {
        let m = UMessageBuilder::publish(topic())
            .with_message_id(fixed_uuid(3))
            .build_with_payload(
                Bytes::new(),
                PayloadEncoding::from_id(0xFF0F).expect("private-use id"),
            )
            .expect("message");
        project(&m)
    };
    let notification = {
        let m = UMessageBuilder::notification(topic(), reply_to())
            .with_message_id(fixed_uuid(4))
            .build()
            .expect("message");
        project(&m)
    };
    let request = {
        let m = UMessageBuilder::request(method(), reply_to(), 5000)
            .with_message_id(fixed_uuid(5))
            .build()
            .expect("message");
        project(&m)
    };
    let response = {
        let m = UMessageBuilder::response(reply_to(), fixed_uuid(5), method())
            .with_message_id(fixed_uuid(6))
            .build()
            .expect("message");
        project(&m)
    };
    vec![
        (
            "publish-no-payload",
            "publish; no payload encoding; source only",
            publish_no_payload,
        ),
        (
            "publish-raw-payload",
            "publish; standard RAW payload encoding",
            publish_raw_payload,
        ),
        (
            "publish-private-use-payload",
            "publish; private-use payload encoding identifier",
            publish_private_use_payload,
        ),
        (
            "notification",
            "notification; source + entity sink (resource 0)",
            notification,
        ),
        (
            "request",
            "rpc request; method sink, reply-to source, ttl=5000",
            request,
        ),
        (
            "response",
            "rpc response; reqid = request fixture id",
            response,
        ),
        (
            "publish-native-private-token",
            "publish; fixture-private encoding plus independent full-width token",
            UFrameMetadata::publish(topic())
                .with_id(fixed_uuid(9))
                .with_payload_encoding(PayloadEncoding::from_id(0xF001).unwrap())
                .with_native_type_token(NativeTypeToken::from_u32(0x89AB_CDEF))
                .build()
                .unwrap(),
        ),
        (
            "publish-native-contract-token-zero",
            "publish; present contract-defined zero encoding and present zero token",
            UFrameMetadata::publish(topic())
                .with_id(fixed_uuid(10))
                .with_payload_encoding(PayloadEncoding::CONTRACT_DEFINED)
                .with_native_type_token(NativeTypeToken::from_u32(0))
                .build()
                .unwrap(),
        ),
        (
            "publish-explicit-zero-ttl",
            "publish; native TTL presence retained with zero nanoseconds",
            UFrameMetadata::publish(topic())
                .with_id(fixed_uuid(11))
                .with_ttl(Duration::ZERO)
                .build()
                .unwrap(),
        ),
    ]
}

fn classic_attribute_fixtures() -> Vec<(&'static str, &'static str, UAttributes)> {
    let raw = UMessageBuilder::publish(topic())
        .with_message_id(fixed_uuid(7))
        .build_with_payload(Bytes::from_static(b"payload"), PayloadEncoding::RAW)
        .expect("message")
        .attributes()
        .clone();
    let private_use_xcdr2 = UMessageBuilder::publish(topic())
        .with_message_id(fixed_uuid(8))
        .build_with_payload(
            Bytes::from_static(b"cdr-bytes"),
            PayloadEncoding::from_id(0xFA2D).expect("fixture-private id"),
        )
        .expect("message")
        .attributes()
        .clone();

    vec![
        (
            "classic-publish-raw-payload",
            "UAttributes; registered RAW payload encoding",
            raw,
        ),
        (
            "classic-publish-private-use-xcdr2-payload",
            "UAttributes; private-use XCDRv2 payload encoding identifier",
            private_use_xcdr2,
        ),
    ]
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2 + 1);
    for b in bytes {
        write!(s, "{b:02x}").expect("hex write");
    }
    s.push('\n');
    s
}

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn vector_entry(name: &str, profile: &str, bytes: &[u8], encoded: &[u8]) -> Value {
    json!({"fixture": name, "profile": profile, "file": format!("{name}.{profile}.hex"),
           "len": bytes.len(), "sha256": hash(bytes), "hex_sha256": hash(encoded)})
}

fn load_vectors(dir: &Path, profile: &str, expected: &[&str]) -> Result<Vec<Vec<u8>>, String> {
    let manifest_path = dir.join(format!("manifest.{profile}.json"));
    let manifest = fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "missing/unreadable manifest {}: {error}",
            manifest_path.display()
        )
    })?;
    let entries: Vec<Value> = serde_json::from_str(&manifest).map_err(|error| error.to_string())?;
    if entries.len() != expected.len() {
        return Err("manifest fixture count mismatch".into());
    }
    let mut result = Vec::new();
    for (entry, name) in entries.iter().zip(expected) {
        let file = format!("{name}.{profile}.hex");
        let encoded =
            fs::read(dir.join(&file)).map_err(|error| format!("vector {file}: {error}"))?;
        let text = std::str::from_utf8(&encoded).map_err(|error| error.to_string())?;
        let bytes = hex::decode(text.trim()).map_err(|error| error.to_string())?;
        if *entry != vector_entry(name, profile, &bytes, &encoded) {
            return Err(format!("manifest/hash mismatch for {file}"));
        }
        result.push(bytes);
    }
    Ok(result)
}

fn context() -> UWireMetadataContext {
    UWireMetadataContext::from_wire::<UProtocolNativeWire, UFrameFieldsMetadataCodec>()
}

#[test]
fn canonical_field_block_golden_vectors() {
    require_normal_mode();
    let fixtures = fixtures();
    let names: Vec<_> = fixtures.iter().map(|(name, _, _)| *name).collect();
    let stored =
        load_vectors(&golden_dir(), "uframe-fields", &names).expect("complete hash-bound manifest");
    for ((name, _, metadata), bytes) in fixtures.into_iter().zip(stored) {
        let decoded = UFrameFieldsMetadataCodec
            .decode_frame_metadata(context(), &bytes)
            .expect("decode stored bytes");
        assert_eq!(decoded, metadata, "stored fixture semantics for {name}");
        assert_eq!(
            UFrameFieldsMetadataCodec
                .encode_frame_metadata(context(), &metadata)
                .unwrap(),
            bytes,
            "encoded bytes for {name}"
        );
        let codec = UFrameFieldsMetadataCodec;
        assert_eq!(
            codec
                .encoded_frame_metadata_len(context(), &metadata)
                .unwrap(),
            bytes.len()
        );
        let mut slice = vec![0xA5; bytes.len() + 7];
        let written = codec
            .encode_frame_metadata_into(context(), &metadata, &mut slice)
            .unwrap();
        assert_eq!(
            slice.get(..written).unwrap(),
            bytes,
            "caller-storage bytes for {name}"
        );
        assert!(slice
            .get(written..)
            .unwrap()
            .iter()
            .all(|byte| *byte == 0xA5));
        let mut short = vec![0xA5; bytes.len() - 1];
        assert!(matches!(
            codec.encode_frame_metadata_into(context(), &metadata, &mut short),
            Err(up_rust::UWireMetadataError::OutputTooShort { .. })
        ));
        assert!(short.iter().all(|byte| *byte == 0xA5));
        #[cfg(feature = "zero-copy-transport")]
        {
            let prefix = codec.native_prefix(context()).unwrap();
            let mut uninit = vec![std::mem::MaybeUninit::new(0xA5); bytes.len() + 7];
            let header = up_rust::write_header_uninit(
                &prefix,
                &up_rust::TxHeaderParts::from(&metadata),
                &mut uninit,
            )
            .unwrap();
            assert_eq!(header.len, bytes.len());
            // SAFETY: the writer initialized exactly header.len bytes and that
            // range remains borrowed from uninit for this comparison.
            let actual =
                unsafe { std::slice::from_raw_parts(uninit.as_ptr().cast::<u8>(), header.len) };
            assert_eq!(actual, bytes, "in-place bytes for {name}");
            for byte in uninit.get(header.len..).unwrap() {
                // SAFETY: the entire destination was initialized to the sentinel.
                assert_eq!(unsafe { byte.assume_init() }, 0xA5);
            }
        }
    }
}

#[test]
fn classic_uattributes_golden_vectors() {
    require_normal_mode();
    let fixtures = classic_attribute_fixtures();
    let names: Vec<_> = fixtures.iter().map(|(name, _, _)| *name).collect();
    let stored = load_vectors(&golden_dir(), "uattributes-classic", &names)
        .expect("complete hash-bound manifest");
    for ((name, _, attributes), bytes) in fixtures.into_iter().zip(stored) {
        let decoded =
            UAttributes::parse_from_protobuf_bytes(&bytes).expect("decode stored classic bytes");
        assert_eq!(decoded, attributes, "stored classic semantics for {name}");
        assert_eq!(
            attributes.write_to_protobuf_bytes().unwrap(),
            bytes,
            "encoded classic bytes for {name}"
        );
    }
}

#[test]
fn explicit_zero_ttl_is_retained_natively_and_canonicalized_through_classic() {
    let native = UFrameMetadata::publish(topic())
        .with_id(fixed_uuid(11))
        .with_ttl(Duration::ZERO)
        .build()
        .unwrap();
    let bytes = UFrameFieldsMetadataCodec
        .encode_frame_metadata(
            UWireMetadataContext::from_wire::<UProtocolNativeWire, UFrameFieldsMetadataCodec>(),
            &native,
        )
        .unwrap();
    let decoded = UFrameFieldsMetadataCodec
        .decode_frame_metadata(
            UWireMetadataContext::from_wire::<UProtocolNativeWire, UFrameFieldsMetadataCodec>(),
            &bytes,
        )
        .unwrap();
    assert_eq!(decoded.ttl(), Some(Duration::ZERO));
    let attributes = decoded.try_project_to_attributes().unwrap();
    assert_eq!(attributes.ttl(), None);
    let canonical = UFrameMetadata::publish(topic())
        .with_id(fixed_uuid(11))
        .build()
        .unwrap();
    let protobuf = attributes.write_to_protobuf_bytes().unwrap();
    assert_eq!(
        protobuf,
        canonical
            .try_project_to_attributes()
            .unwrap()
            .write_to_protobuf_bytes()
            .unwrap()
    );
    // Protobuf field 6 is scalar TTL: explicitly encoded zero and its omitted
    // default have the same Rust/native no-expiry meaning.
    let mut explicit_protobuf_zero = protobuf.clone();
    explicit_protobuf_zero.extend_from_slice(&[0x30, 0x00]);
    for wire in [protobuf, explicit_protobuf_zero] {
        let parsed = UAttributes::parse_from_protobuf_bytes(&wire).unwrap();
        assert_eq!(parsed.ttl(), None);
        let projected = parsed.try_project_to_frame_metadata().unwrap();
        assert_eq!(projected, canonical);
        assert_ne!(
            projected, native,
            "native explicit-zero presence is intentionally canonicalized"
        );
    }
}

fn write_vectors(profile: &str, vectors: Vec<(&str, Vec<u8>)>) {
    let mut manifest = Vec::new();
    for (name, bytes) in vectors {
        let encoded = hex(&bytes);
        fs::write(golden_dir().join(format!("{name}.{profile}.hex")), &encoded).unwrap();
        manifest.push(vector_entry(name, profile, &bytes, encoded.as_bytes()));
    }
    fs::write(
        golden_dir().join(format!("manifest.{profile}.json")),
        serde_json::to_string_pretty(&manifest).unwrap() + "\n",
    )
    .unwrap();
}

#[test]
#[ignore = "explicit golden regeneration; requires UP_GOLDEN_UPDATE=1"]
fn regenerate_wire_metadata_goldens() {
    assert_eq!(std::env::var("UP_GOLDEN_UPDATE").as_deref(), Ok("1"));
    write_vectors(
        "uframe-fields",
        fixtures()
            .into_iter()
            .map(|(name, _, metadata)| {
                (
                    name,
                    UFrameFieldsMetadataCodec
                        .encode_frame_metadata(context(), &metadata)
                        .unwrap(),
                )
            })
            .collect(),
    );
    write_vectors(
        "uattributes-classic",
        classic_attribute_fixtures()
            .into_iter()
            .map(|(name, _, attributes)| (name, attributes.write_to_protobuf_bytes().unwrap()))
            .collect(),
    );
}

#[test]
fn missing_or_tampered_manifests_and_vectors_fail() {
    let temp = std::env::temp_dir().join(format!(
        "up-rust-golden-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&temp).unwrap();
    let name = "publish-no-payload";
    let profile = "uframe-fields";
    assert!(load_vectors(&temp, profile, &[name])
        .unwrap_err()
        .contains("manifest"));
    let encoded = fs::read(golden_dir().join(format!("{name}.{profile}.hex"))).unwrap();
    let bytes = hex::decode(std::str::from_utf8(&encoded).unwrap().trim()).unwrap();
    let entry = vector_entry(name, profile, &bytes, &encoded);
    let manifest = temp.join(format!("manifest.{profile}.json"));
    fs::write(&manifest, serde_json::to_vec(&vec![entry.clone()]).unwrap()).unwrap();
    assert!(load_vectors(&temp, profile, &[name])
        .unwrap_err()
        .contains("vector"));
    let vector = temp.join(format!("{name}.{profile}.hex"));
    fs::write(&vector, &encoded).unwrap();
    assert_eq!(load_vectors(&temp, profile, &[name]).unwrap(), vec![bytes]);
    fs::write(&vector, "00\n").unwrap();
    assert!(load_vectors(&temp, profile, &[name])
        .unwrap_err()
        .contains("hash mismatch"));
    fs::write(&vector, &encoded).unwrap();
    let mut tampered = entry;
    tampered
        .as_object_mut()
        .unwrap()
        .insert("sha256".into(), json!("incorrect"));
    fs::write(&manifest, serde_json::to_vec(&vec![tampered]).unwrap()).unwrap();
    assert!(load_vectors(&temp, profile, &[name])
        .unwrap_err()
        .contains("hash mismatch"));
    fs::write(&manifest, "[]").unwrap();
    assert!(load_vectors(&temp, profile, &[name])
        .unwrap_err()
        .contains("fixture count"));
    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn accidental_update_environment_never_rewrites_normal_tests() {
    let snapshot = || {
        let mut entries: Vec<_> = fs::read_dir(golden_dir())
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                (
                    path.file_name().unwrap().to_owned(),
                    hash(&fs::read(path).unwrap()),
                )
            })
            .collect();
        entries.sort();
        entries
    };
    let before = snapshot();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "canonical_field_block_golden_vectors"])
        .env("UP_GOLDEN_UPDATE", "1")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stdout).contains("ordinary golden tests never update"));
    assert_eq!(snapshot(), before);
}
