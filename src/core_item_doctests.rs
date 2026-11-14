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

//! Doctests of items that moved to `up-rust-core`.
//!
//! `up-rust-core` cannot depend on `up-rust`, so an example that uses `up_rust::` paths
//! cannot run as a doctest of the moved item. These run here instead, with the text
//! they had before the move. Only `cargo test --doc` compiles this module.

/// ```
/// use up_rust::UPriority;
///
/// assert_eq!(UPriority::CS2.to_priority_code(), "CS2");
/// ```
pub fn upriority_to_priority_code() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::{LocalUriProvider, StaticUriProvider, UEntityId, UUri};
///
/// // The usual case: the first instance of a service type.
/// let door = UEntityId::first_instance(0x1234);
/// let topic = UUri::for_entity("vehicle", door, 1, 0x8001)?;
/// assert_eq!(topic.ue_type_id(), 0x1234);
/// assert_eq!(topic.ue_instance_id(), 1);
///
/// // Instance numbers from configuration are checked at the boundary.
/// let second = UEntityId::new(0x1234, 2)?;
/// let local = StaticUriProvider::for_entity("vehicle", second, 1)?;
/// assert_eq!(local.get_source_uri().ue_instance_id(), 2);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn uentity_id_usual_case() {}

/// ```rust
/// use up_rust::{UEntityId, UUriPartsError};
///
/// assert_eq!(UEntityId::new(0x1234, 0), Err(UUriPartsError::ZeroEntityInstance));
/// ```
pub fn uentity_id_zero_instance() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uuri = UUri::try_from_parts("vin.vehicles", 0x0001_800A, 0x02, 0x0000_1a50).unwrap();
/// let uri_string = uuri.to_uri(false);
/// assert_eq!(uri_string, "//vin.vehicles/800A/1/2/1A50");
/// ```
pub fn uuri_into_string() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use std::str::FromStr;
/// use up_rust::UUri;
///
/// let uri = UUri::try_from_parts("vin.vehicles", 0x000A_8000, 0x02, 0x0000_1a50).unwrap();
/// let uri_from = UUri::from_str("//vin.vehicles/8000/A/2/1A50").unwrap();
/// assert_eq!(uri, uri_from);
/// ```
pub fn uuri_from_str() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uri = UUri::try_from_parts("", 0x001A_8000, 0x02, 0x1a50).unwrap();
/// let uri_from = UUri::try_from("/8000/1A/2/1A50".to_string().as_str()).unwrap();
/// assert_eq!(uri, uri_from);
/// ```
pub fn uuri_try_from_string() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uri = UUri::try_from_parts("", 0x001A_8000, 0x02, 0x1a50).unwrap();
/// let uri_from = UUri::try_from("/8000/1A/2/1A50").unwrap();
/// assert_eq!(uri, uri_from);
/// ```
pub fn uuri_try_from_str() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uri = UUri::try_from_parts("my-vehicle", 0x10101234, 0x01, 0x9a10).unwrap();
/// assert_eq!(uri.authority_name(), "my-vehicle");
/// ```
pub fn uuri_authority_name() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uri = UUri::try_from_parts("my-vehicle", 0x10101234, 0x01, 0x9a10).unwrap();
/// assert_eq!(uri.ue_type_id(), 0x1234);
/// ```
pub fn uuri_ue_type_id() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uri = UUri::try_from_parts("my-vehicle", 0x10101234, 0x01, 0x9a10).unwrap();
/// assert_eq!(uri.ue_instance_id(), 0x1010);
/// ```
pub fn uuri_ue_instance_id() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uri = UUri::try_from_parts("my-vehicle", 0x10101234, 0x01, 0x9a10).unwrap();
/// assert_eq!(uri.uentity_major_version(), 0x01);
/// ```
pub fn uuri_uentity_major_version() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uri = UUri::try_from_parts("my-vehicle", 0x10101234, 0x01, 0x9a10).unwrap();
/// assert_eq!(uri.resource_id(), 0x9a10);
/// ```
pub fn uuri_resource_id() {}

/// ```rust
/// use std::str::FromStr;
/// use up_rust::UUri;
///
/// let authority_a = UUri::from_str("up://authority.a/100A/1/1/0").unwrap();
/// let authority_b = UUri::from_str("up://authority.b/200B/1/2/20").unwrap();
/// assert!(authority_a.is_remote(&authority_b));
///
/// let authority_local = UUri::from_str("up:///100A/1/1/0").unwrap();
/// assert!(!authority_local.is_remote(&authority_a));
///
/// let authority_wildcard = UUri::from_str("up://*/100A/1/1/0").unwrap();
/// assert!(!authority_wildcard.is_remote(&authority_a));
/// assert!(!authority_a.is_remote(&authority_wildcard));
/// assert!(!authority_wildcard.is_remote(&authority_wildcard));
/// ```
pub fn uuri_is_remote() {}

/// ```rust
/// use std::str::FromStr;
/// use up_rust::UUri;
///
/// let authority_a = UUri::from_str("up://authority.a/100A/1/1/0").unwrap();
/// let authority_b = "authority.b";
/// assert!(authority_a.is_remote_authority(&authority_b));
///
/// let authority_local = "";
/// assert!(!authority_a.is_remote_authority(&authority_local));
///
/// let authority_wildcard = "*";
/// assert!(!authority_a.is_remote_authority(&authority_wildcard));
/// ```
pub fn uuri_is_remote_authority() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uuri = UUri::try_from_parts("", 0x0001_9B3A, 0x01, 0x145b).unwrap();
/// assert!(uuri.has_empty_authority());
/// ```
pub fn uuri_has_empty_authority() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uuri = UUri::try_from_parts("*", 0x0001_9B3A, 0x01, 0x145b).unwrap();
/// assert!(uuri.has_wildcard_authority());
/// ```
pub fn uuri_has_wildcard_authority() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uuri = UUri::try_from_parts("vin", 0xFFFF_0123, 0x01, 0x145b).unwrap();
/// assert!(uuri.has_wildcard_entity_instance());
/// ```
pub fn uuri_has_wildcard_entity_instance() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uuri = UUri::try_from_parts("vin", 0x00C0_FFFF, 0x01, 0x145b).unwrap();
/// assert!(uuri.has_wildcard_entity_type());
/// ```
pub fn uuri_has_wildcard_entity_type() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uuri = UUri::try_from_parts("vin", 0x0001_9B3A, 0xFF, 0x145b).unwrap();
/// assert!(uuri.has_wildcard_version());
/// ```
pub fn uuri_has_wildcard_version() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uuri = UUri::try_from_parts("vin", 0x0001_9B3A, 0x01, 0xFFFF).unwrap();
/// assert!(uuri.has_wildcard_resource_id());
/// ```
pub fn uuri_has_wildcard_resource_id() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uri = UUri::try_from_parts("", 0x0001_ABCD, 0x01, 0x7FFF).unwrap();
/// assert!(uri.is_rpc_method());
/// ```
pub fn uuri_is_rpc_method() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uri = UUri::try_from_parts("", 0x0001_ABCD, 0x01, 0x0000).unwrap();
/// assert!(uri.is_notification_destination());
/// ```
pub fn uuri_is_notification_destination() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uri = UUri::try_from_parts("", 0x0001_ABCD, 0x01, 0x0000).unwrap();
/// assert!(uri.is_rpc_response());
/// ```
pub fn uuri_is_rpc_response() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUri;
///
/// let uri = UUri::try_from_parts("", 0x0001_ABCD, 0x01, 0x8000).unwrap();
/// assert!(uri.is_event());
/// ```
pub fn uuri_is_event() {}

/// ```rust
/// use up_rust::UUri;
///
/// let pattern = UUri::try_from("//vin/A14F/1/3/FFFF").unwrap();
/// let candidate = UUri::try_from("//vin/A14F/1/3/B1D4").unwrap();
/// assert!(pattern.matches(&candidate));
/// ```
pub fn uuri_matches() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUID;
///
/// // timestamp = 0x018D548EA8E0 (Monday, 29 January 2024, 9:30:52 AM GMT)
/// // ver = 0b0111
/// let msb = 0x018D548EA8E07000u64;
/// // variant = 0b10
/// let lsb = 0x8000000000000000u64;
/// let creation_time = UUID::from_u64_pair(msb, lsb).unwrap().time();
/// assert_eq!(creation_time, 0x018D548EA8E0_u64);
/// ```
pub fn uuid_time() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUID;
///
/// // timestamp = 1, ver = 0b0111, variant = 0b10
/// let bytes: Vec<u8> = vec![
///     0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x70, 0x00,
///     0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
/// ];
/// let conversion_attempt = UUID::from_byte_vec(bytes);
/// assert!(conversion_attempt.is_ok());
/// let uuid = conversion_attempt.unwrap();
/// assert_eq!(uuid.time(), 0x1_u64);
/// ```
pub fn uuid_try_from_bytes() {}

/// ```rust
/// # use up_rust::prelude::*;
/// use up_rust::UUID;
///
/// // timestamp = 1, ver = 0b0111
/// let msb = 0x0000000000017000_u64;
/// // variant = 0b10, random = 0x0010101010101a1a
/// let lsb = 0x8010101010101a1a_u64;
/// let uuid = UUID::from_u64_pair(msb, lsb).unwrap();
/// let bytes: Vec<u8> = uuid.to_byte_vec();
/// assert_eq!(bytes, vec![
///     0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x70, 0x00,
///     0x80, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1a, 0x1a,
/// ]);
/// ```
pub fn uuid_into_bytes() {}

/// ```rust
/// use up_rust::UUID;
///
/// // parsing a valid uProtocol UUID succeeds
/// let parsing_attempt = "00000000-0001-7000-8010-101010101a1A".parse::<UUID>();
/// assert!(parsing_attempt.is_ok());
///
/// // parsing an invalid UUID fails
/// assert!("a1a2a3a4-b1b2-c1c2-d1d2-d3d4d5d6d7d8"
///     .parse::<UUID>()
///     .is_err());
///
/// // parsing a string that is not a UUID fails
/// assert!("this-is-not-a-UUID"
///     .parse::<UUID>()
///     .is_err());
/// ```
pub fn uuid_from_str() {}
