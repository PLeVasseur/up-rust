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

//! Differential tests of the hand-written parsers in `up-rust-core`.
//!
//! `reference_uuri` and `reference_authority` are the text parser that `up-rust`
//! used before the move, reduced to its outcome. Every generated input, including
//! fuzz-style mutated inputs, must give the same outcome on both sides: the same
//! value on success, and the same error class and text on failure.

// Indexes come from `Xorshift::below`, which is always below the length.
#![allow(clippy::indexing_slicing)]

use std::collections::BTreeSet;
use std::str::FromStr;

use fluent_uri::{component::Host, UriRef};
use up_rust::{UUri, UUriPartsError, UUriRefusal, UUID};
use uuid_simd::{AsciiCase, Out};

const AUTHORITY_NAME_MAX_LENGTH: usize = 128;
const WILDCARD_AUTHORITY: &str = "*";

#[derive(Clone, Debug, Eq, PartialEq)]
enum ReferenceError {
    /// Text of a `UUriError::SerializationError`.
    Serialization(String),
    /// Text of a `UUriError::ValidationError`.
    Validation(String),
}

#[derive(Debug, Eq, PartialEq)]
struct ReferenceUuri {
    authority: String,
    entity_type: u16,
    entity_instance: u16,
    version: u8,
    resource: u16,
}

fn is_valid_authority_name(name: &str) -> bool {
    name.len() <= AUTHORITY_NAME_MAX_LENGTH
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.' | b'_' | b'~')
        })
}

fn parts_message(error: &UUriPartsError) -> String {
    error.to_string()
}

fn reference_check_parsed_authority(
    auth: &fluent_uri::component::Authority<'_>,
) -> Result<String, UUriPartsError> {
    if auth.has_port() {
        Err(UUriPartsError::AuthorityHasPort)
    } else if auth.has_userinfo() {
        Err(UUriPartsError::AuthorityHasUserinfo)
    } else {
        let verified_name = match auth.host_parsed() {
            Host::Ipv4(_) | Host::Ipv6(_) => auth.host(),
            Host::RegName(name) => {
                if !WILDCARD_AUTHORITY.eq(name.as_str()) && !is_valid_authority_name(name.as_str())
                {
                    return Err(UUriPartsError::InvalidAuthorityCharacters);
                }
                name.as_str()
            }
            Host::IpvFuture { .. } => return Err(UUriPartsError::UnsupportedHost),
        };
        if verified_name.len() > AUTHORITY_NAME_MAX_LENGTH {
            return Err(UUriPartsError::AuthorityTooLong);
        }
        Ok(verified_name.to_string())
    }
}

/// The text parser `up-rust` used before `UUri` moved into `up-rust-core`.
fn reference_uuri(uri: &str) -> Result<ReferenceUuri, ReferenceError> {
    use ReferenceError::Serialization;
    const SCHEME_UP: &fluent_uri::component::Scheme =
        fluent_uri::component::Scheme::new_or_panic("up");

    if uri.is_empty() {
        return Err(Serialization("URI is empty".into()));
    }
    let parsed_uri = UriRef::parse(uri).map_err(|e| Serialization(e.to_string()))?;

    if parsed_uri.scheme().is_some_and(|s| s.ne(SCHEME_UP)) {
        return Err(Serialization("uProtocol URI must use 'up' scheme".into()));
    }
    if parsed_uri.has_query() {
        return Err(Serialization("uProtocol URI must not contain query".into()));
    }
    if parsed_uri.has_fragment() {
        return Err(Serialization(
            "uProtocol URI must not contain fragment".into(),
        ));
    }
    let authority = match parsed_uri.authority() {
        Some(auth) => reference_check_parsed_authority(&auth)
            .map_err(|e| ReferenceError::Validation(parts_message(&e)))?,
        None => String::new(),
    };

    let Some(mut path_segments) = parsed_uri.path().segments_if_absolute() else {
        return Err(Serialization(
            "uProtocol URI must contain entity type, instance, version and resource IDs".into(),
        ));
    };
    let Some(entity_type) = path_segments.next() else {
        return Err(Serialization(
            "uProtocol URI must contain entity type ID".into(),
        ));
    };
    let Some(entity_instance) = path_segments.next() else {
        return Err(Serialization(
            "uProtocol URI must contain entity instance ID".into(),
        ));
    };
    let Some(version) = path_segments.next() else {
        return Err(Serialization(
            "uProtocol URI must contain entity version".into(),
        ));
    };
    let Some(resource) = path_segments.next() else {
        return Err(Serialization(
            "uProtocol URI must contain resource ID".into(),
        ));
    };
    if path_segments.next().is_some() {
        return Err(Serialization(
            "uProtocol URI path must contain exactly four segments".into(),
        ));
    }
    if entity_type.is_empty() {
        return Err(Serialization(
            "URI must contain non-empty entity type ID".into(),
        ));
    }
    let entity_type = u16::from_str_radix(entity_type.as_str(), 16)
        .map_err(|e| Serialization(format!("Cannot parse entity type ID: {e}")))?;
    if entity_instance.is_empty() {
        return Err(Serialization(
            "URI must contain non-empty entity instance ID".into(),
        ));
    }
    let entity_instance = u16::from_str_radix(entity_instance.as_str(), 16)
        .map_err(|e| Serialization(format!("Cannot parse entity instance ID: {e}")))?;
    if entity_instance == 0 {
        return Err(Serialization(
            "uProtocol URI's entity instance ID must not be zero".into(),
        ));
    }
    if version.is_empty() {
        return Err(Serialization(
            "URI must contain non-empty entity version".into(),
        ));
    }
    let version = u8::from_str_radix(version.as_str(), 16)
        .map_err(|e| Serialization(format!("Cannot parse entity version: {e}")))?;
    if resource.is_empty() {
        return Err(Serialization(
            "URI must contain non-empty resource ID".into(),
        ));
    }
    let resource = u16::from_str_radix(resource.as_str(), 16)
        .map_err(|e| Serialization(format!("Cannot parse resource ID: {e}")))?;

    Ok(ReferenceUuri {
        authority,
        entity_type,
        entity_instance,
        version,
        resource,
    })
}

/// The authority check `up-rust` used before the move.
fn reference_authority(authority: &str) -> Result<String, UUriPartsError> {
    if authority.len() > AUTHORITY_NAME_MAX_LENGTH {
        return Err(UUriPartsError::AuthorityTooLong);
    }
    let prefixed = format!("//{authority}");
    let parsed =
        UriRef::parse(prefixed.as_str()).map_err(|_| UUriPartsError::MalformedAuthority)?;
    let auth = parsed
        .authority()
        .ok_or(UUriPartsError::MalformedAuthority)?;
    if auth.as_str() != authority {
        return Err(UUriPartsError::AuthorityHasTrailingComponent);
    }
    reference_check_parsed_authority(&auth)
}

#[derive(Debug, Eq, PartialEq)]
enum Outcome {
    Parsed(ReferenceUuri),
    Serialization(String),
    Validation(String),
}

fn reference_outcome(text: &str) -> Outcome {
    match reference_uuri(text) {
        Ok(value) => Outcome::Parsed(value),
        Err(ReferenceError::Serialization(message)) => Outcome::Serialization(message),
        Err(ReferenceError::Validation(message)) => Outcome::Validation(message),
    }
}

fn core_outcome(text: &str) -> Outcome {
    match UUri::from_str(text) {
        Ok(uri) => Outcome::Parsed(ReferenceUuri {
            authority: uri.authority_name().to_string(),
            entity_type: uri.ue_type_id(),
            entity_instance: uri.ue_instance_id(),
            version: uri.uentity_major_version(),
            resource: uri.resource_id(),
        }),
        Err(UUriRefusal::Serialization(detail)) => Outcome::Serialization(detail.to_string()),
        Err(UUriRefusal::Validation(detail)) => Outcome::Validation(detail.to_string()),
        Err(other) => panic!("unexpected refusal {other:?}"),
    }
}

#[derive(Default)]
struct Tally {
    compared: u64,
    parsed: u64,
    refused: u64,
    distinct: BTreeSet<String>,
}

fn compare_uri(text: &str, tally: &mut Tally) {
    let reference = reference_outcome(text);
    let core = core_outcome(text);
    assert_eq!(reference, core, "input: {text:?}");
    tally.compared += 1;
    match &reference {
        Outcome::Parsed(_) => tally.parsed += 1,
        Outcome::Serialization(_) | Outcome::Validation(_) => tally.refused += 1,
    }
    if tally.distinct.len() < 2_000 {
        tally.distinct.insert(text.to_string());
    }
    // Successful parses also round trip through the classic text forms.
    if let Ok(uri) = UUri::from_str(text) {
        assert_eq!(
            UUri::from_str(&uri.to_string()).as_ref(),
            Ok(&uri),
            "input: {text:?}"
        );
        assert_eq!(
            UUri::from_str(&uri.display_with_scheme(false).to_string()).as_ref(),
            Ok(&uri),
            "input: {text:?}"
        );
    }
}

fn compare_authority(authority: &str) {
    let reference = reference_authority(authority);
    let core = UUri::try_from_wire_parts(authority, 0x0001_0001, 1, 1)
        .map(|uri| uri.authority_name().to_string());
    assert_eq!(reference, core, "authority: {authority:?}");
    assert_eq!(
        reference_authority(authority).map(drop),
        UUri::check_authority(authority),
        "authority check: {authority:?}"
    );
}

/// A small deterministic generator, so a failure names a reproducible input.
struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

const SEEDS: &[&str] = &[
    "up://vehicle/10AB/3/1/8001",
    "//vehicle.example/ABCD/1/A5/1234",
    "up:/1234/5/1/7FFF",
    "/8000/1A/2/1A50",
    "up://192.168.1.10/1/1/1/1",
    "up://[2001:db8::8]/100/2/3/4",
    "//[::1]/1/1/1/1",
    "up://[v1.fe]/1/1/1/1",
    "up://user@vin/1/1/1/1",
    "up://vin:80/1/1/1/1",
    "up://*/A100/1/1/1",
    "up://vin/FFFF/FFFF/FF/FFFF",
    "UP://VIN/1/1/1/1",
    "foo://vin/1/1/1/1",
    "up://vin/1/1/1/1?q=1",
    "up://vin/1/1/1/1#frag",
    "up://vin/1/1/1",
    "up://vin/1/1/1/1/1",
    "//vin/1//1/1",
    "up://vin/+1/+1/+1/+1",
    "up://vin/%31/1/1/1",
    "up://vi%6E/1/1/1/1",
    "up://my_vin-1.a~b/1/1/1/1",
    "//1.2.3/1/1/1/1",
    "//1.2.3.4.5/1/1/1/1",
    "//256.1.1.1/1/1/1/1",
    "//[1:2:3:4:5:6:7:8]/1/1/1/1",
    "//[1:2:3:4:5:6:1.2.3.4]/1/1/1/1",
    "//[::ffff:1.2.3.4]/1/1/1/1",
    "//[1::2::3]/1/1/1/1",
    "//[:::]/1/1/1/1",
    "//[v.1]/1/1/1/1",
    "//user:pass@[::1]:99/1/1/1/1",
    "//a@b@c/1/1/1/1",
    "/1/1/1/1?x#y",
    "a/b/c",
    ":bad",
    "1up://vin/1/1/1/1",
];

const ALPHABET: &[u8] = b"ABCabc019.:/?#[]@!$&'()*+,;=-_~%vVuUpP gG\xc3\xa9";

fn mutate(rng: &mut Xorshift, seed: &[u8]) -> Vec<u8> {
    let mut bytes = seed.to_vec();
    let steps = 1 + rng.below(4);
    for _ in 0..steps {
        let at = if bytes.is_empty() {
            0
        } else {
            rng.below(bytes.len())
        };
        match rng.below(6) {
            0 if !bytes.is_empty() => {
                bytes.remove(at);
            }
            1 if !bytes.is_empty() => {
                bytes[at] = ALPHABET[rng.below(ALPHABET.len())];
            }
            2 => {
                bytes.insert(at, ALPHABET[rng.below(ALPHABET.len())]);
            }
            3 if !bytes.is_empty() => {
                // duplicate a range
                let end = (at + rng.below(8) + 1).min(bytes.len());
                let copy: Vec<u8> = bytes[at..end].to_vec();
                let index = rng.below(bytes.len() + 1);
                for (offset, byte) in copy.into_iter().enumerate() {
                    bytes.insert(index + offset, byte);
                }
            }
            4 if !bytes.is_empty() => {
                // splice from another seed
                let other = SEEDS[rng.below(SEEDS.len())].as_bytes();
                let cut = rng.below(other.len());
                bytes.truncate(at);
                bytes.extend_from_slice(&other[cut..]);
            }
            _ => {}
        }
    }
    bytes
}

#[test]
fn uri_parser_agrees_with_fluent_uri_on_seeds_and_mutations() {
    let mut tally = Tally::default();
    for seed in SEEDS {
        compare_uri(seed, &mut tally);
    }
    // Mutation-based fuzzing of real-looking URIs, deterministic and reproducible.
    let mut rng = Xorshift(0x0123_4567_89AB_CDEF);
    for round in 0..600_000_u64 {
        let seed = SEEDS[(round as usize) % SEEDS.len()].as_bytes();
        let bytes = mutate(&mut rng, seed);
        // The parser takes `&str`; invalid UTF-8 cannot reach it. Lossy conversion
        // keeps the mutation stream deterministic and still exercises non-ASCII.
        let text = String::from_utf8_lossy(&bytes);
        compare_uri(&text, &mut tally);
    }
    println!(
        "uri differential: {} compared, {} parsed, {} refused",
        tally.compared, tally.parsed, tally.refused
    );
    assert!(
        tally.parsed > 10_000,
        "too few successful parses: {}",
        tally.parsed
    );
    assert!(
        tally.refused > 10_000,
        "too few refusals: {}",
        tally.refused
    );
}

#[test]
fn uri_parser_agrees_with_fluent_uri_on_exhaustive_short_inputs() {
    // Every string up to length 4 over an alphabet that covers the grammar's
    // decision points, including `%`, brackets and path, query, fragment and scheme.
    const SMALL: &[u8] = b"a0:/?#[]@%.*";
    let mut tally = Tally::default();
    let mut work: Vec<u8> = Vec::new();
    fn walk(work: &mut Vec<u8>, depth: usize, tally: &mut Tally) {
        if let Ok(text) = std::str::from_utf8(work) {
            compare_uri(text, tally);
        }
        if depth == 0 {
            return;
        }
        for byte in SMALL {
            work.push(*byte);
            walk(work, depth - 1, tally);
            work.pop();
        }
    }
    walk(&mut work, 4, &mut tally);
    println!(
        "uri exhaustive: {} compared, {} parsed, {} refused",
        tally.compared, tally.parsed, tally.refused
    );
}

#[test]
fn authority_check_agrees_with_fluent_uri() {
    let mut rng = Xorshift(0xDEAD_BEEF_CAFE_F00D);
    for authority in [
        "",
        "vehicle",
        "*",
        "192.168.1.10",
        "[2001:db8::8]",
        "[v1.xy]",
        "user@vin",
        "vin:80",
        "vin:",
        "VIN",
        "vin/",
        "vin/x",
        "vin?x",
        "vin#x",
        "a*b",
        "1.2.3",
        "my_vin-1.a~b",
        "%41",
        "[::1]",
        "[1:2:3:4:5:6:7:8]",
    ] {
        compare_authority(authority);
    }
    compare_authority(&"a".repeat(128));
    compare_authority(&"a".repeat(129));
    for round in 0..200_000_u32 {
        let seed = [
            "vehicle",
            "192.168.1.10",
            "[2001:db8::8]",
            "[v1.xy]",
            "a@b:9",
            "*",
        ][(round as usize) % 6];
        let bytes = mutate(&mut rng, seed.as_bytes());
        let text = String::from_utf8_lossy(&bytes);
        compare_authority(&text);
    }
}

#[test]
fn uuid_parser_and_formatter_agree_with_uuid_simd() {
    let mut rng = Xorshift(0xFEED_FACE_0000_0001);
    let mut valid = 0_u64;
    for round in 0..200_000_u64 {
        // A valid v7 UUID, then a mutation of its text.
        let msb = (rng.next() & !0xF000) | 0x7000;
        let lsb = (rng.next() & !(0b11 << 62)) | (0b10 << 62);
        let uuid = UUID::try_from_wire_pair(msb, lsb).unwrap();

        let mut bytes = [0_u8; 16];
        bytes[..8].copy_from_slice(&msb.to_be_bytes());
        bytes[8..].copy_from_slice(&lsb.to_be_bytes());
        let mut buffer = [0_u8; 36];
        let simd_text =
            uuid_simd::format_hyphenated(&bytes, Out::from_mut(&mut buffer), AsciiCase::Lower);
        let simd_text: &[u8] = &simd_text[..];
        assert_eq!(uuid.to_string().as_bytes(), simd_text);
        assert_eq!(&uuid.hyphenated_ascii()[..], simd_text);

        let text = String::from_utf8_lossy(simd_text).into_owned();
        let mutated = if round % 5 == 0 {
            text.clone()
        } else if round % 5 == 1 {
            text.to_uppercase()
        } else {
            String::from_utf8_lossy(&mutate(&mut rng, text.as_bytes())).into_owned()
        };

        let mut parsed = [0_u8; 16];
        let reference = uuid_simd::parse_hyphenated(mutated.as_bytes(), Out::from_mut(&mut parsed))
            .ok()
            .map(|bytes| *bytes)
            .and_then(|bytes| UUID::try_from_wire_bytes(&bytes).ok());
        let core = mutated.parse::<UUID>().ok();
        assert_eq!(reference, core, "input: {mutated:?}");
        if core.is_some() {
            valid += 1;
        }
    }
    println!("uuid differential: {valid} valid of 200000");
    assert!(valid > 10_000);
}
