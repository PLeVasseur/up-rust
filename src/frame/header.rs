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
//! In-place native headers using the canonical metadata field block.
//!
//! The caller supplies its agreed selected-wire prefix. Headers encode that
//! prefix, a little-endian field-block length and the existing versioned fields.
//! Payload placement and storage ownership remain the binding's responsibility.
//! Receive token/traceparent values borrow the input; errors carry no heap data.

use crate::UFrameMetadata;

pub use up_rust_core::frame::header::*;

impl<'a> From<&'a UFrameMetadata> for TxHeaderParts<'a> {
    fn from(metadata: &'a UFrameMetadata) -> Self {
        Self {
            kind: metadata.kind(),
            priority: metadata.priority(),
            id: metadata.id(),
            reqid: metadata.reqid(),
            ttl: metadata.ttl(),
            comm_status: metadata.comm_status(),
            permission_level: metadata.permission_level(),
            source: metadata.source(),
            sink: metadata.sink(),
            token: metadata.token(),
            traceparent: metadata.traceparent(),
            payload_encoding: metadata.payload_encoding().copied(),
            native_type_token: metadata.native_type_token(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::codec::*;
    use crate::prelude::*;
    use crate::{NativeTypeToken, PayloadEncoding, UCode, UPriority, UUri, UUID};
    use std::time::Duration;

    const PREFIX: &[u8] = b"agreed-prefix";

    fn metadata(seed: usize) -> UFrameMetadata {
        let topic = UUri::try_from_wire_parts("sensor", 0x0001_1234, 1, 0x8001).unwrap();
        let method = UUri::try_from_wire_parts("service", 0x0001_4567, 2, 0x42).unwrap();
        let reply = UUri::try_from_wire_parts("client", 0x0001_6789, 3, 0).unwrap();
        let mut builder = match seed % 4 {
            0 => UFrameMetadata::publish(topic),
            1 => UFrameMetadata::notification(topic, reply),
            2 => UFrameMetadata::request(method, reply, Duration::from_millis(250)),
            _ => UFrameMetadata::response(method, reply, UUID::build()),
        };
        if seed & 4 != 0 {
            builder = builder.with_priority(UPriority::CS5);
        }
        if seed & 8 != 0 {
            builder = builder.with_ttl(Duration::from_nanos(999));
        }
        if seed & 16 != 0 {
            builder = builder.with_comm_status(UCode::Unavailable);
        }
        if seed & 32 != 0 {
            builder = builder.with_permission_level(seed as u32);
        }
        if seed & 64 != 0 {
            builder = builder.with_token(if seed & 512 == 0 { "private-token" } else { "" });
        }
        if seed & 128 != 0 {
            builder =
                builder.with_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01");
        }
        if seed & 256 != 0 {
            builder = builder.with_payload_encoding(PayloadEncoding::CONTRACT_DEFINED);
        }
        if seed & 512 != 0 && seed & 256 != 0 {
            builder = builder.with_native_type_token(NativeTypeToken::from_u32(seed as u32));
        }
        builder.build().unwrap()
    }

    fn encode(metadata: &UFrameMetadata) -> Vec<u8> {
        let parts = TxHeaderParts::from(metadata);
        let mut bytes = vec![0; header_len(PREFIX, &parts).unwrap()];
        let written = write_header(PREFIX, &parts, &mut bytes).unwrap();
        assert_eq!(written.len, bytes.len());
        bytes
    }

    fn assert_received_fields(received: &RxHeader<'_>, metadata: &UFrameMetadata) {
        assert_eq!(received.kind, metadata.kind());
        assert_eq!(&received.id, metadata.id());
        assert_eq!(received.reqid.as_ref(), metadata.reqid());
        assert_eq!(&received.source, metadata.source());
        assert_eq!(received.sink.as_ref(), metadata.sink());
        assert_eq!(received.ttl, metadata.ttl());
        assert_eq!(received.priority, metadata.priority());
        assert_eq!(received.comm_status, metadata.comm_status());
        assert_eq!(received.permission_level, metadata.permission_level());
        assert_eq!(received.token(), metadata.token());
        assert_eq!(received.traceparent(), metadata.traceparent());
        assert_eq!(
            received.payload_encoding,
            metadata.payload_encoding().copied()
        );
        assert_eq!(received.native_type_token, metadata.native_type_token());
    }

    #[test]
    fn differential_field_blocks_and_receive_views() {
        let limits = HeaderLimits {
            max_token_len: 2048,
        };
        // Test-only fault controls demonstrate that the conformance gate fails
        // for encoding drift and lost decoded identity. No library path reads it.
        let fault = std::env::var("UP_RUST_HEADER_CONFORMANCE_FAULT").ok();
        for seed in 0..1024 {
            let metadata = metadata(seed);
            let fields = encode_frame_metadata_fields(&metadata).unwrap();
            let mut bytes = encode(&metadata);
            if fault.as_deref() == Some("token-byte-order")
                && metadata.native_type_token().is_some()
            {
                let length = bytes.len();
                bytes.get_mut(length - 4..).unwrap().reverse();
            }
            assert_eq!(bytes.get(PREFIX.len() + 4..).unwrap(), fields);
            assert_eq!(
                decode_frame_metadata_fields(bytes.get(PREFIX.len() + 4..).unwrap()).unwrap(),
                metadata
            );
            let mut received = parse_header(PREFIX, &bytes, &limits).unwrap();
            if fault.as_deref() == Some("lost-token") {
                received.native_type_token = None;
            }
            assert_received_fields(&received, &metadata);
            if let Some(token) = received.token() {
                let offset = token.as_ptr() as usize - bytes.as_ptr() as usize;
                assert_eq!(
                    bytes.get(offset..offset + token.len()).unwrap(),
                    token.as_bytes()
                );
            }
        }
        println!("header_field_differential: metadata_cases=1024 receive_comparisons=1024");
    }

    #[test]
    fn native_prefix_and_in_place_header_match_the_whole_codec() {
        use crate::wire::{
            StableContainerWire, UFrameFieldsMetadataCodec, UWireMetadataCodec,
            UWireMetadataContext,
        };
        let codec = UFrameFieldsMetadataCodec;
        let context =
            UWireMetadataContext::from_wire::<StableContainerWire, UFrameFieldsMetadataCodec>();
        let prefix = codec.native_prefix(context).unwrap();
        for seed in 0..1024 {
            let metadata = metadata(seed);
            let parts = TxHeaderParts::from(&metadata);
            let mut output = vec![0; header_len(&prefix, &parts).unwrap()];
            write_header(&prefix, &parts, &mut output).unwrap();
            assert_eq!(
                output,
                codec.encode_frame_metadata(context, &metadata).unwrap()
            );
            assert_eq!(
                codec.decode_frame_metadata(context, &output).unwrap(),
                metadata
            );
            let received = parse_header(
                &prefix,
                &output,
                &HeaderLimits {
                    max_token_len: 2048,
                },
            )
            .unwrap();
            assert_received_fields(&received, &metadata);
        }
        println!("header_whole_codec_differential: metadata_cases=1024 receive_comparisons=1024");
    }

    #[test]
    fn mutated_fields_agree_with_the_canonical_decoder() {
        let limits = HeaderLimits {
            max_token_len: 2048,
        };
        let mut accepted = 0;
        let mut rejected = 0;
        for seed in 0..1024 {
            let original = encode(&metadata(seed));
            for mutation in 0..48 {
                let mut bytes = original.clone();
                let offset = PREFIX.len()
                    + 4
                    + (seed * 13 + mutation * 7) % (bytes.len() - PREFIX.len() - 4);
                *bytes.get_mut(offset).unwrap() ^= 1 << ((seed + mutation) % 8);
                let reference =
                    decode_frame_metadata_fields(bytes.get(PREFIX.len() + 4..).unwrap());
                let expected = reference.as_ref().is_ok_and(|value| {
                    value
                        .traceparent()
                        .is_none_or(|value| value.len() <= HEADER_TRACEPARENT_CAPACITY)
                });
                let received = parse_header(PREFIX, &bytes, &limits);
                assert_eq!(
                    received.is_ok(),
                    expected,
                    "seed {seed}, mutation {mutation}, offset {offset}"
                );
                if expected {
                    assert_received_fields(&received.unwrap(), &reference.unwrap());
                    accepted += 1;
                } else {
                    rejected += 1;
                }
            }
        }
        assert_eq!(accepted + rejected, 49_152);
        assert!(accepted > 0 && rejected > 0);
        println!("header_mutation_differential: base_cases=1024 mutations=49152 accepted={accepted} rejected={rejected} accepted_fields_equal=true");
    }

    #[test]
    fn short_output_and_invalid_metadata_leave_bytes_unchanged() {
        let metadata = metadata(511);
        let mut parts = TxHeaderParts::from(&metadata);
        let mut output = [0xA5; 8];
        assert_eq!(
            write_header(PREFIX, &parts, &mut output),
            Err(UFrameHeaderError::OutputTooShort)
        );
        assert_eq!(output, [0xA5; 8]);
        parts.priority = Some(UPriority::CS0);
        assert_eq!(
            write_header(PREFIX, &parts, &mut output),
            Err(UFrameHeaderError::Invalid("RPC priority"))
        );
        assert_eq!(output, [0xA5; 8]);
    }

    #[test]
    fn token_limit_follows_structure_and_debug_redacts_secret() {
        let metadata = metadata(64);
        let parts = TxHeaderParts::from(&metadata);
        let mut bytes = encode(&metadata);
        let limits = HeaderLimits { max_token_len: 0 };
        assert!(matches!(
            parse_header(PREFIX, &bytes, &limits),
            Err(UFrameHeaderError::TokenTooLong)
        ));
        assert!(!format!("{parts:?}").contains("private-token"));
        let received = parse_header(
            PREFIX,
            &bytes,
            &HeaderLimits {
                max_token_len: 2048,
            },
        )
        .unwrap();
        assert!(!format!("{received:?}").contains("private-token"));
        let last = bytes.len() - 1;
        *bytes.get_mut(last).unwrap() = 0xFF;
        assert!(matches!(
            parse_header(PREFIX, &bytes, &limits),
            Err(UFrameHeaderError::Malformed("invalid UTF-8"))
        ));
    }
}
