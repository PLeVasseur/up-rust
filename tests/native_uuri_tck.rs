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

use up_rust::UUri;

#[test]
fn uuri_round_trips_between_native_parts_and_uri_string() {
    let uri = UUri::try_from_parts("vin", 0x8000, 1, 2).expect("valid UUri parts");

    assert_eq!(uri.to_uri(false), "//vin/8000/1/2");
    assert_eq!(uri.to_uri(true), "up://vin/8000/1/2");

    let decoded = UUri::try_from(uri.to_uri(true).as_str()).expect("valid serialized UUri");

    assert_eq!(decoded, uri);
}

#[test]
fn uuri_pattern_matching_uses_native_wildcards_without_protobuf() {
    let pattern = UUri::try_from("//vin/A14F/3/FFFF").expect("valid UUri pattern");
    let candidate = UUri::try_from("//vin/A14F/3/B1D4").expect("valid UUri candidate");
    let different_authority = UUri::try_from("//other/A14F/3/B1D4").expect("valid UUri");

    assert!(pattern.matches(&candidate));
    assert!(!pattern.matches(&different_authority));
}

#[test]
fn uuri_rejects_invalid_native_values() {
    let authority_too_long = format!("//{}/A100/1/6501", "a".repeat(129));

    assert!(UUri::try_from(authority_too_long.as_str()).is_err());
    assert!(UUri::try_from("//vin/A100/100/6501").is_err());
    assert!(UUri::try_from("//vin/A100/1/10000").is_err());
}
