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

use up_rust::UUID;

#[test]
fn uuid_round_trips_between_native_string_and_bytes() {
    let uuid = "00000000-0001-7000-8010-101010101a1A"
        .parse::<UUID>()
        .expect("valid uProtocol UUID");

    assert!(uuid.is_uprotocol_uuid());
    assert_eq!(uuid.get_time(), Some(1));
    assert_eq!(
        uuid.to_hyphenated_string(),
        "00000000-0001-7000-8010-101010101a1a"
    );

    let bytes = Vec::<u8>::from(&uuid);
    let decoded = UUID::try_from(bytes).expect("native UUID bytes should decode");

    assert_eq!(decoded, uuid);
}

#[test]
fn uuid_rejects_invalid_version_and_variant() {
    assert!(UUID::from_u64_pair(0x0000_0000_0001_C000, 0x8000_0000_0000_0000).is_err());
    assert!(UUID::from_u64_pair(0x0000_0000_0001_7000, 0x4000_0000_0000_0000).is_err());
}
