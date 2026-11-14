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

//! Compile-time pin: every path whose type moved into `up-rust-core` still
//! resolves at its old `up_rust` location.

#![allow(unused_imports)]

use up_rust::frame::codec::{
    FIELD_COMM_STATUS, FIELD_MASK_V1, FIELD_NATIVE_TYPE_TOKEN, FIELD_PAYLOAD_ENCODING,
    FIELD_PERMISSION_LEVEL, FIELD_REQID, FIELD_SINK, FIELD_TOKEN, FIELD_TRACEPARENT, FIELD_TTL,
    FRAME_FIELDS_VERSION,
};
use up_rust::frame::header::{
    header_fields_len, header_len, parse_header, write_header, write_header_uninit, HeaderLimits,
    RxHeader, TxHeaderParts, UFrameHeaderError, WrittenHeader, HEADER_TRACEPARENT_CAPACITY,
};
use up_rust::frame::metadata::NativeTypeToken as MetadataToken;
use up_rust::{
    header_fields_len as root_fields_len, header_len as root_header_len,
    parse_header as root_parse_header, write_header as root_write_header,
    write_header_uninit as root_write_uninit, AdmissionError, ExactUUri, Frame,
    HeaderLimits as Limits, InitializedTx, LoanError, LoanedPayload, NativeTypeToken,
    PayloadEncoding, PayloadLoanProvenance, PreparedTxStorage, RouteLimits, RxHeader as Rx,
    SendError, SetupError, TxFrameLayout, TxHeaderParts as Tx, UCode, UEntityId,
    UFrameHeaderError as HeaderError, UMessageType, UPriority, UStatus, UUri, UUriError,
    UUriPartsError, WrittenHeader as Written, HEADER_TRACEPARENT_CAPACITY as CAPACITY,
    PAYLOAD_ENCODING_PRIVATE_USE_MAX, PAYLOAD_ENCODING_PRIVATE_USE_MIN,
    PAYLOAD_ENCODING_RESERVED_MIN, UUID,
};

// The classic error types stay as they were (re-check R1).
use up_rust::prelude::*;

fn classic_errors_keep_their_shape(error: UUriError) -> bool {
    matches!(error, UUriError::ValidationError(_))
        || matches!(error, UUriError::SerializationError(_))
}

#[test]
fn every_moved_path_still_resolves() {
    let classic = UUri::try_from_parts("vehicle", 0x1234, 1, 0x8001).unwrap_err();
    assert!(classic_errors_keep_their_shape(classic));
    let token = NativeTypeToken::from_u32(9);
    let same: MetadataToken = token;
    assert_eq!(same.as_u32(), 9);
}

/// Rustdoc does not list these on the inlined pages, so they are pinned by compiling them.
#[test]
fn conversions_rustdoc_cannot_show_still_work() {
    let id: u32 = PayloadEncoding::RAW.into();
    assert_eq!(id, PayloadEncoding::RAW.id());
    let exact = ExactUUri::try_new(UUri::try_from_parts("vin", 0x0001_1234, 1, 0x8001).unwrap());
    let exact = exact.unwrap();
    assert_eq!(exact.to_uri(false), "//vin/1234/1/1/8001");
    assert!(exact.verify_no_wildcards().is_ok());
    assert!(exact.verify_event().is_ok());
}
