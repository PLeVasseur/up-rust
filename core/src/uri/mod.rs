/********************************************************************************
 * Copyright (c) 2023 Contributors to the Eclipse Foundation
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

/*!
Implementation of the [uProtocol URI (UUri) data model](https://github.com/eclipse-uprotocol/up-spec/blob/v1.6.0-alpha.7/basics/uri.adoc).
A UUri represents a uProtocol resource identifier and is used in various places across the uProtocol specification, e.g., to identify the source and destination of messages or to identify resources that are being accessed via RPC calls.

The authority is stored inline, unconditionally, so construction from validated
parts and cloning never touch a heap. The text parser and `Display` are written
by hand on `core`. This crate has only fixed-error names (`try_from_wire_parts`,
the `check_*` methods, [`UUriRefusal`]). The classic names, which use owned
strings (`UUri::try_from_parts`, the `verify_*` methods, `to_uri`), are extension
traits in `up_rust::prelude`, and `UUriError` stays in `up-rust`.
*/

// [impl->dsn~uri-data-model-naming~2]
// [impl->req~uri-data-model-proto~1]

use core::fmt;
use core::hash::{Hash, Hasher};
use core::num::{IntErrorKind, NonZeroU16};
use core::ops::Deref;
use core::str::FromStr;

mod parser;
use parser::{Authority, Host, ParseError, ParseErrorKind, UriRef};

#[doc(hidden)]
pub const WILDCARD_AUTHORITY: &str = "*";
#[doc(hidden)]
pub const WILDCARD_ENTITY_INSTANCE: u32 = 0xFFFF_0000;
#[doc(hidden)]
pub const WILDCARD_ENTITY_TYPE: u32 = 0x0000_FFFF;
#[doc(hidden)]
pub const WILDCARD_ENTITY_VERSION: u8 = 0xFF;
#[doc(hidden)]
pub const WILDCARD_RESOURCE_ID: u16 = 0xFFFF;

#[doc(hidden)]
pub const RESOURCE_ID_RESPONSE: u16 = 0x0000;
#[doc(hidden)]
pub const RESOURCE_ID_MIN_EVENT: u16 = 0x8000;

const AUTHORITY_NAME_MAX_LENGTH: usize = 128;

/// Validated authority bytes stored without a heap allocation.
#[derive(Clone, Eq, PartialEq)]
#[repr(C)]
#[doc(hidden)]
pub struct AuthorityNameString {
    len: u8,
    bytes: [u8; AUTHORITY_NAME_MAX_LENGTH],
}

impl AuthorityNameString {
    fn new(value: &str) -> Result<Self, UUriPartsError> {
        if value.len() > AUTHORITY_NAME_MAX_LENGTH {
            return Err(UUriPartsError::AuthorityTooLong);
        }
        let mut bytes = [0; AUTHORITY_NAME_MAX_LENGTH];
        for (slot, byte) in bytes.iter_mut().zip(value.bytes()) {
            *slot = byte;
        }
        Ok(Self {
            len: value.len() as u8,
            bytes,
        })
    }

    fn as_str(&self) -> &str {
        // Only `new` and `default` construct the private, immutable valid prefix,
        // so `len` is in bounds and the prefix is valid UTF-8.
        self.bytes
            .get(..usize::from(self.len))
            .and_then(|prefix| core::str::from_utf8(prefix).ok())
            .unwrap_or("")
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    const fn wildcard() -> Self {
        let mut bytes = [0; AUTHORITY_NAME_MAX_LENGTH];
        bytes[0] = b'*';
        Self { len: 1, bytes }
    }
}

impl Default for AuthorityNameString {
    fn default() -> Self {
        Self {
            len: 0,
            bytes: [0; AUTHORITY_NAME_MAX_LENGTH],
        }
    }
}

impl fmt::Debug for AuthorityNameString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), formatter)
    }
}

impl PartialEq<&str> for AuthorityNameString {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl Hash for AuthorityNameString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

/// A rejection of URI parts that carries no heap-allocated diagnostic data.
///
/// [`UUri::try_from_wire_parts`] returns this error in every configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum UUriPartsError {
    /// The authority exceeds the specification's 128-byte bound.
    AuthorityTooLong,
    /// The input cannot be parsed as a URI authority.
    MalformedAuthority,
    /// A port is not part of a uProtocol authority.
    AuthorityHasPort,
    /// User information is not part of a uProtocol authority.
    AuthorityHasUserinfo,
    /// IPvFuture literals are not supported by the uProtocol authority model.
    UnsupportedHost,
    /// An authority argument includes a path, query, or fragment.
    AuthorityHasTrailingComponent,
    /// A registered name contains a character outside the permitted set.
    InvalidAuthorityCharacters,
    /// The entity instance is outside the specification's nonzero range.
    ///
    /// A packed entity ID keeps the instance in its high 16 bits, so a bare 16-bit
    /// type ID such as `0x1234` silently names instance 0. [`UEntityId`] takes the
    /// type and instance separately, so the mistake cannot be spelled.
    ZeroEntityInstance,
}

impl UUriPartsError {
    /// The fixed explanation of this rejection.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::AuthorityTooLong => "uProtocol URI's authority is too long",
            Self::MalformedAuthority => "invalid authority",
            Self::AuthorityHasPort => "uProtocol URI's authority must not contain port",
            Self::AuthorityHasUserinfo => "uProtocol URI's authority must not contain userinfo",
            Self::UnsupportedHost => "uProtocol URI's authority must not be an IPvFuture literal",
            Self::AuthorityHasTrailingComponent => {
                "uProtocol URI's authority must not contain a path, query or fragment"
            }
            Self::InvalidAuthorityCharacters => "uProtocol URI's authority contains invalid characters",
            Self::ZeroEntityInstance => {
                "uProtocol URI's entity instance ID must not be zero: the high 16 bits of a packed entity ID hold the instance, so a bare 16-bit type ID has instance 0; name the entity with UEntityId::first_instance(type_id) or UEntityId::new(type_id, instance)"
            }
        }
    }
}

impl fmt::Display for UUriPartsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl core::error::Error for UUriPartsError {}

/// A fixed-size explanation carried by [`UUriRefusal`].
///
/// Formatting produces the same text the owned diagnostic string used to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UUriRefusalDetail {
    text: &'static str,
    suffix: DetailSuffix,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DetailSuffix {
    None,
    Index(usize),
    Int(IntProblem),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IntProblem {
    Empty,
    InvalidDigit,
    PosOverflow,
    NegOverflow,
    Zero,
}

impl IntProblem {
    const fn message(self) -> &'static str {
        match self {
            Self::Empty => "cannot parse integer from empty string",
            Self::InvalidDigit => "invalid digit found in string",
            Self::PosOverflow => "number too large to fit in target type",
            Self::NegOverflow => "number too small to fit in target type",
            Self::Zero => "number would be zero for non-zero type",
        }
    }

    fn from_kind(kind: &IntErrorKind) -> Self {
        // `IntErrorKind` is non-exhaustive and grows new variants, so this cannot
        // be a `match` under the workspace's `wildcard_enum_match_arm` lint.
        if matches!(kind, IntErrorKind::Empty) {
            Self::Empty
        } else if matches!(kind, IntErrorKind::PosOverflow) {
            Self::PosOverflow
        } else if matches!(kind, IntErrorKind::NegOverflow) {
            Self::NegOverflow
        } else if matches!(kind, IntErrorKind::Zero) {
            Self::Zero
        } else {
            Self::InvalidDigit
        }
    }
}

impl UUriRefusalDetail {
    const fn text(text: &'static str) -> Self {
        Self {
            text,
            suffix: DetailSuffix::None,
        }
    }

    const fn indexed(text: &'static str, index: usize) -> Self {
        Self {
            text,
            suffix: DetailSuffix::Index(index),
        }
    }

    const fn int(prefix: &'static str, problem: IntProblem) -> Self {
        Self {
            text: prefix,
            suffix: DetailSuffix::Int(problem),
        }
    }
}

impl fmt::Display for UUriRefusalDetail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.text)?;
        match self.suffix {
            DetailSuffix::None => Ok(()),
            DetailSuffix::Index(index) => write!(f, "{index}"),
            DetailSuffix::Int(problem) => write!(f, ": {}", problem.message()),
        }
    }
}

/// A fixed-size refusal of a URI string or a URI check.
///
/// `up-rust` keeps the classic `UUriError`, with owned strings, and converts
/// from this type. Formatting produces the same text as the classic error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum UUriRefusal {
    /// A given URI string cannot be parsed into a UUri due to invalid formatting or content.
    Serialization(UUriRefusalDetail),
    /// A given URI does not comply with the UUri specification.
    Validation(UUriRefusalDetail),
}

impl UUriRefusal {
    const fn serialization_text(message: &'static str) -> UUriRefusal {
        Self::Serialization(UUriRefusalDetail::text(message))
    }

    const fn validation_text(message: &'static str) -> UUriRefusal {
        Self::Validation(UUriRefusalDetail::text(message))
    }

    /// The detail of this refusal.
    #[must_use]
    pub const fn detail(&self) -> &UUriRefusalDetail {
        match self {
            Self::Serialization(detail) | Self::Validation(detail) => detail,
        }
    }

    fn cannot_parse(prefix: &'static str, error: &core::num::ParseIntError) -> UUriRefusal {
        Self::Serialization(UUriRefusalDetail::int(
            prefix,
            IntProblem::from_kind(error.kind()),
        ))
    }

    fn from_parse_error(error: ParseError) -> UUriRefusal {
        let text = match error.kind {
            ParseErrorKind::InvalidPctEncodedOctet => "invalid percent-encoded octet at index ",
            ParseErrorKind::UnexpectedChar => "unexpected character at index ",
            ParseErrorKind::InvalidIpv6Addr => "invalid IPv6 address at index ",
        };
        Self::Serialization(UUriRefusalDetail::indexed(text, error.index))
    }
}

impl fmt::Display for UUriRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serialization(detail) => write!(f, "Serialization error: {detail}"),
            Self::Validation(detail) => write!(f, "Validation error: {detail}"),
        }
    }
}

impl core::error::Error for UUriRefusal {}

impl From<UUriPartsError> for UUriRefusal {
    fn from(error: UUriPartsError) -> Self {
        Self::Validation(UUriRefusalDetail::text(error.message()))
    }
}

/// A uEntity named by its separate 16-bit type ID and nonzero 16-bit instance.
///
/// A packed `u32` entity ID keeps the instance in its high 16 bits, so writing a
/// bare service ID such as `0x1234` silently names instance 0, which no URI
/// accepts. Naming the two parts separately makes that mistake unrepresentable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct UEntityId {
    type_id: u16,
    instance: NonZeroU16,
}

impl UEntityId {
    /// Names the first instance of a uEntity type, the usual case for a service
    /// running once per authority. This cannot fail.
    #[must_use]
    pub const fn first_instance(type_id: u16) -> Self {
        Self {
            type_id,
            instance: NonZeroU16::MIN,
        }
    }

    /// Names one instance of a type from runtime or configuration values.
    ///
    /// # Errors
    ///
    /// Returns [`UUriPartsError::ZeroEntityInstance`] when `instance` is zero.
    pub const fn new(type_id: u16, instance: u16) -> Result<Self, UUriPartsError> {
        match NonZeroU16::new(instance) {
            Some(instance) => Ok(Self { type_id, instance }),
            None => Err(UUriPartsError::ZeroEntityInstance),
        }
    }

    /// Names a statically known instance, for use in constants.
    #[must_use]
    pub const fn with_instance(type_id: u16, instance: NonZeroU16) -> Self {
        Self { type_id, instance }
    }

    /// Splits a packed entity ID, whose instance is in the high 16 bits.
    ///
    /// # Errors
    ///
    /// Returns [`UUriPartsError::ZeroEntityInstance`] when the high 16 bits are zero.
    pub const fn from_packed(entity_id: u32) -> Result<Self, UUriPartsError> {
        Self::new(
            (entity_id & WILDCARD_ENTITY_TYPE) as u16,
            (entity_id >> 16) as u16,
        )
    }

    /// Gets the 16-bit type ID.
    #[must_use]
    pub const fn type_id(self) -> u16 {
        self.type_id
    }

    /// Gets the nonzero 16-bit instance.
    #[must_use]
    pub const fn instance(self) -> u16 {
        self.instance.get()
    }

    /// Gets the packed `u32` wire form, with the instance in the high 16 bits.
    #[must_use]
    pub const fn packed(self) -> u32 {
        ((self.instance.get() as u32) << 16) | self.type_id as u32
    }
}

// Equivalent to matching against `^[a-z0-9\-._~]{0,128}$`.
fn is_valid_authority_name(name: &str) -> bool {
    name.len() <= AUTHORITY_NAME_MAX_LENGTH
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.' | b'_' | b'~')
        })
}

/// A URI that represents a uProtocol resource identifier.
///
/// The authority is stored inline, so construction from validated parts and
/// cloning do not allocate.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct UUri {
    ue_id: u32,
    resource_id: u16,
    ue_version_major: u8,
    // The scalar prefix occupies seven bytes; the inline authority occupies 129.
    authority_name: AuthorityNameString,
}

const _: () = assert!(core::mem::size_of::<UUri>() == 136);

impl fmt::Display for UUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.display_with_scheme(true).fmt(f)
    }
}

/// A [`Display`](fmt::Display) adapter that renders a URI with or without the `up` scheme.
#[derive(Clone, Copy, Debug)]
pub struct UUriDisplay<'a> {
    uri: &'a UUri,
    include_scheme: bool,
}

impl fmt::Display for UUriDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.include_scheme {
            f.write_str("up:")?;
        }
        if !self.uri.authority_name.is_empty() {
            f.write_str("//")?;
            f.write_str(self.uri.authority_name.as_str())?;
        }
        write!(
            f,
            "/{:X}/{:X}/{:X}/{:X}",
            self.uri.ue_type_id(),
            self.uri.ue_instance_id(),
            self.uri.ue_version_major,
            self.uri.resource_id
        )
    }
}

impl FromStr for UUri {
    type Err = UUriRefusal;

    /// Attempts to parse a `str` into a `UUri`.
    ///
    /// Note that the _authority_ of the URI is not normalized: upper case
    /// characters in a registered name are rejected.
    // [impl->dsn~uri-authority-name-length~1]
    // [impl->dsn~uri-scheme~1]
    // [impl->dsn~uri-host-only~2]
    // [impl->dsn~uri-authority-mapping~1]
    // [impl->dsn~uri-path-mapping~3]
    // [impl->req~uri-serialization~1]
    // [impl->dsn~uri-entity-instance-id~1]
    fn from_str(uri: &str) -> Result<Self, Self::Err> {
        if uri.is_empty() {
            return Err(UUriRefusal::serialization_text("URI is empty"));
        }
        let parsed: UriRef<'_> = parser::parse(uri).map_err(UUriRefusal::from_parse_error)?;

        if parsed
            .scheme
            .is_some_and(|scheme| !scheme.eq_ignore_ascii_case("up"))
        {
            return Err(UUriRefusal::serialization_text(
                "uProtocol URI must use 'up' scheme",
            ));
        }
        if parsed.has_query {
            return Err(UUriRefusal::serialization_text(
                "uProtocol URI must not contain query",
            ));
        }
        if parsed.has_fragment {
            return Err(UUriRefusal::serialization_text(
                "uProtocol URI must not contain fragment",
            ));
        }
        let authority_name = match &parsed.authority {
            Some(authority) => {
                UUri::check_parsed_authority(authority).map_err(UUriRefusal::from)?
            }
            None => AuthorityNameString::default(),
        };

        let Some(path) = parsed.path.strip_prefix('/') else {
            return Err(UUriRefusal::serialization_text(
                "uProtocol URI must contain entity type, instance, version and resource IDs",
            ));
        };
        // A one-element array, not `char`: see `PayloadEncoding::from_media_type_str`.
        let mut path_segments = path.split(['/']);
        let Some(entity_type) = path_segments.next() else {
            return Err(UUriRefusal::serialization_text(
                "uProtocol URI must contain entity type ID",
            ));
        };
        let Some(entity_instance) = path_segments.next() else {
            return Err(UUriRefusal::serialization_text(
                "uProtocol URI must contain entity instance ID",
            ));
        };
        let Some(version) = path_segments.next() else {
            return Err(UUriRefusal::serialization_text(
                "uProtocol URI must contain entity version",
            ));
        };
        let Some(resource) = path_segments.next() else {
            return Err(UUriRefusal::serialization_text(
                "uProtocol URI must contain resource ID",
            ));
        };
        if path_segments.next().is_some() {
            return Err(UUriRefusal::serialization_text(
                "uProtocol URI path must contain exactly four segments",
            ));
        }

        if entity_type.is_empty() {
            return Err(UUriRefusal::serialization_text(
                "URI must contain non-empty entity type ID",
            ));
        }
        let entity_type = u16::from_str_radix(entity_type, 16)
            .map_err(|e| UUriRefusal::cannot_parse("Cannot parse entity type ID", &e))?;
        if entity_instance.is_empty() {
            return Err(UUriRefusal::serialization_text(
                "URI must contain non-empty entity instance ID",
            ));
        }
        let entity_instance = u16::from_str_radix(entity_instance, 16)
            .map_err(|e| UUriRefusal::cannot_parse("Cannot parse entity instance ID", &e))?;
        if entity_instance == 0 {
            return Err(UUriRefusal::serialization_text(
                "uProtocol URI's entity instance ID must not be zero",
            ));
        }
        if version.is_empty() {
            return Err(UUriRefusal::serialization_text(
                "URI must contain non-empty entity version",
            ));
        }
        let ue_version_major = u8::from_str_radix(version, 16)
            .map_err(|e| UUriRefusal::cannot_parse("Cannot parse entity version", &e))?;
        if resource.is_empty() {
            return Err(UUriRefusal::serialization_text(
                "URI must contain non-empty resource ID",
            ));
        }
        let resource_id = u16::from_str_radix(resource, 16)
            .map_err(|e| UUriRefusal::cannot_parse("Cannot parse resource ID", &e))?;

        Ok(UUri {
            authority_name,
            ue_id: (u32::from(entity_instance) << 16) | u32::from(entity_type),
            ue_version_major,
            resource_id,
        })
    }
}

// [impl->req~uri-serialization~1]
impl TryFrom<&str> for UUri {
    type Error = UUriRefusal;

    /// Attempts to serialize a `str` into a `UUri`.
    fn try_from(uri: &str) -> Result<Self, Self::Error> {
        UUri::from_str(uri)
    }
}

impl Hash for UUri {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.authority_name.hash(state);
        self.ue_id.hash(state);
        self.ue_version_major.hash(state);
        self.resource_id.hash(state);
    }
}

impl Eq for UUri {}

/// A [`UUri`] that has been checked to contain no wildcard components.
///
/// This proof type is for APIs where exact-vs-wildcard URI behavior is
/// materially different. It only carries the result of
/// [`UUri::check_no_wildcards`]; it does not imply physical transport
/// prefiltering or no-copy behavior.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct ExactUUri(UUri);

impl ExactUUri {
    /// Creates an exact URI proof from a URI with no wildcard components.
    ///
    /// # Errors
    ///
    /// Returns the same refusal as [`UUri::check_no_wildcards`] when any URI
    /// component contains a wildcard.
    pub fn check(uri: UUri) -> Result<Self, UUriRefusal> {
        uri.check_no_wildcards()?;
        Ok(Self(uri))
    }

    /// Borrows the checked URI.
    #[must_use]
    pub fn as_uuri(&self) -> &UUri {
        &self.0
    }

    /// Consumes this proof and returns the underlying URI.
    #[must_use]
    pub fn into_inner(self) -> UUri {
        self.0
    }
}

impl AsRef<UUri> for ExactUUri {
    fn as_ref(&self) -> &UUri {
        self.as_uuri()
    }
}

impl Deref for ExactUUri {
    type Target = UUri;

    fn deref(&self) -> &Self::Target {
        self.as_uuri()
    }
}

impl From<ExactUUri> for UUri {
    fn from(value: ExactUUri) -> Self {
        value.into_inner()
    }
}

impl TryFrom<UUri> for ExactUUri {
    type Error = UUriRefusal;

    fn try_from(value: UUri) -> Result<Self, Self::Error> {
        Self::check(value)
    }
}

impl TryFrom<&UUri> for ExactUUri {
    type Error = UUriRefusal;

    fn try_from(value: &UUri) -> Result<Self, Self::Error> {
        Self::check(value.clone())
    }
}

impl PartialEq<str> for UUri {
    fn eq(&self, other: &str) -> bool {
        match UUri::from_str(other) {
            Ok(other_uri) => self.eq(&other_uri),
            Err(_) => false,
        }
    }
}

impl PartialEq<&str> for UUri {
    fn eq(&self, other: &&str) -> bool {
        self.eq(*other)
    }
}

impl UUri {
    /// A [`Display`](fmt::Display) adapter for this URI, with or without the `up` scheme.
    ///
    /// `UUri`'s own `Display` always includes the scheme.
    #[must_use]
    pub fn display_with_scheme(&self, include_scheme: bool) -> UUriDisplay<'_> {
        UUriDisplay {
            uri: self,
            include_scheme,
        }
    }

    /// Validates URI parts with an allocation-free error representation.
    ///
    /// The entire authority argument must be consumed; a suffix path, query,
    /// fragment, port, user information, or IPvFuture host is rejected.
    ///
    /// # Errors
    ///
    /// Returns the specific [`UUriPartsError`] for invalid authority input or a
    /// zero entity instance. The high 16 bits of `entity_id` must be nonzero.
    // [impl->dsn~uri-authority-name-length~1]
    // [impl->dsn~uri-host-only~2]
    // [impl->dsn~uri-entity-instance-id~1]
    pub fn try_from_wire_parts(
        authority: &str,
        entity_id: u32,
        entity_version: u8,
        resource_id: u16,
    ) -> Result<Self, UUriPartsError> {
        let authority_name = Self::check_authority_name(authority)?;
        if entity_id >> 16 == 0 {
            return Err(UUriPartsError::ZeroEntityInstance);
        }
        Ok(UUri {
            authority_name,
            ue_id: entity_id,
            ue_version_major: entity_version,
            resource_id,
        })
    }

    /// Gets a pattern URI that [matches](UUri::matches) any given candidate URI.
    #[must_use]
    pub fn any() -> Self {
        Self::any_with_resource_id(WILDCARD_RESOURCE_ID)
    }

    /// Gets a pattern URI that [matches](UUri::matches) any given candidate URI with a specific resource ID.
    #[must_use]
    pub fn any_with_resource_id(resource_id: u16) -> Self {
        UUri {
            authority_name: AuthorityNameString::wildcard(),
            ue_id: WILDCARD_ENTITY_INSTANCE | WILDCARD_ENTITY_TYPE,
            ue_version_major: WILDCARD_ENTITY_VERSION,
            resource_id,
        }
    }

    /// Gets the authority name part from this uProtocol URI.
    #[must_use]
    pub fn authority_name(&self) -> &str {
        self.authority_name.as_str()
    }

    /// Gets the uEntity type identifier part from this uProtocol URI.
    #[must_use]
    pub fn ue_type_id(&self) -> u16 {
        (self.ue_id & WILDCARD_ENTITY_TYPE) as u16
    }

    /// Gets the uEntity instance identifier part from this uProtocol URI.
    #[must_use]
    pub fn ue_instance_id(&self) -> u16 {
        ((self.ue_id & WILDCARD_ENTITY_INSTANCE) >> 16) as u16
    }

    /// Gets the major version using the data model property name.
    #[must_use]
    pub const fn ue_version_major(&self) -> u8 {
        self.ue_version_major
    }

    /// Gets the major version part from this uProtocol URI.
    #[must_use]
    pub fn uentity_major_version(&self) -> u8 {
        self.ue_version_major
    }

    /// Gets the resource identifier part from this uProtocol URI.
    #[must_use]
    pub fn resource_id(&self) -> u16 {
        self.resource_id
    }

    /// Returns a copy of this URI with the resource id replaced.
    #[must_use]
    pub fn clone_with_resource_id(&self, resource_id: u16) -> Self {
        UUri {
            authority_name: self.authority_name.clone(),
            ue_id: self.ue_id,
            ue_version_major: self.ue_version_major,
            resource_id,
        }
    }

    /// Checks that the given authority name complies with the UUri specification.
    ///
    /// # Errors
    ///
    /// Returns the specific [`UUriPartsError`] if the authority name is invalid.
    // [impl->dsn~uri-authority-name-length~1]
    // [impl->dsn~uri-host-only~2]
    pub fn check_authority(authority: &str) -> Result<(), UUriPartsError> {
        Self::check_authority_name(authority).map(drop)
    }

    fn check_authority_name(authority: &str) -> Result<AuthorityNameString, UUriPartsError> {
        if authority.len() > AUTHORITY_NAME_MAX_LENGTH {
            return Err(UUriPartsError::AuthorityTooLong);
        }
        // The parser reads an authority within a URI reference. The length check
        // above bounds both the addition and the stack-storage slice.
        let mut storage = [0_u8; 2 + AUTHORITY_NAME_MAX_LENGTH];
        for (slot, byte) in storage
            .iter_mut()
            .zip(b"//".iter().copied().chain(authority.bytes()))
        {
            *slot = byte;
        }
        let total = authority.len().wrapping_add(2);
        let prefixed = storage
            .get(..total)
            .ok_or(UUriPartsError::AuthorityTooLong)
            .and_then(|bytes| {
                core::str::from_utf8(bytes).map_err(|_| UUriPartsError::MalformedAuthority)
            })?;
        let parsed = parser::parse(prefixed).map_err(|_| UUriPartsError::MalformedAuthority)?;
        let auth = parsed.authority.ok_or(UUriPartsError::MalformedAuthority)?;
        if auth.text != authority {
            return Err(UUriPartsError::AuthorityHasTrailingComponent);
        }
        Self::check_parsed_authority(&auth)
    }

    // [impl->dsn~uri-authority-name-length~1]
    // [impl->dsn~uri-host-only~2]
    fn check_parsed_authority(auth: &Authority<'_>) -> Result<AuthorityNameString, UUriPartsError> {
        if auth.has_port() {
            Err(UUriPartsError::AuthorityHasPort)
        } else if auth.has_userinfo() {
            Err(UUriPartsError::AuthorityHasUserinfo)
        } else {
            let host = auth.host_text();
            let verified_name = match auth.host {
                Host::Ipv4 | Host::Ipv6 => host,
                Host::RegName => {
                    if WILDCARD_AUTHORITY != host && !is_valid_authority_name(host) {
                        return Err(UUriPartsError::InvalidAuthorityCharacters);
                    }
                    host
                }
                Host::IpvFuture => return Err(UUriPartsError::UnsupportedHost),
            };
            if verified_name.len() > AUTHORITY_NAME_MAX_LENGTH {
                return Err(UUriPartsError::AuthorityTooLong);
            }
            AuthorityNameString::new(verified_name)
        }
    }

    /// Check if an `UUri` is remote, by comparing authority fields.
    /// UUris with empty authority are considered to be local.
    ///
    /// # Returns
    ///
    /// 'true' if other_uri has a different authority than `Self`, `false` otherwise.
    #[must_use]
    pub fn is_remote(&self, other_uri: &UUri) -> bool {
        self.is_remote_authority(other_uri.authority_name.as_str())
    }

    /// Check if an authority is remote compared to the authority field of the UUri.
    /// Empty authorities are considered to be local.
    ///
    /// # Returns
    ///
    /// 'true' if authority is a different than `Self.authority_name`, `false` otherwise.
    #[must_use]
    pub fn is_remote_authority(&self, authority: &str) -> bool {
        !authority.is_empty()
            && !self.authority_name.is_empty()
            && !self.has_wildcard_authority()
            && authority != WILDCARD_AUTHORITY
            && self.authority_name.as_str() != authority
    }

    /// Checks if this UUri has an empty authority name.
    #[must_use]
    pub fn has_empty_authority(&self) -> bool {
        self.authority_name.is_empty()
    }

    /// Checks if this UUri has a wildcard authority name.
    #[must_use]
    pub fn has_wildcard_authority(&self) -> bool {
        self.authority_name == WILDCARD_AUTHORITY
    }

    /// Checks if this UUri has an entity identifier matching any instance.
    #[must_use]
    pub fn has_wildcard_entity_instance(&self) -> bool {
        self.ue_id & WILDCARD_ENTITY_INSTANCE == WILDCARD_ENTITY_INSTANCE
    }

    /// Checks if this UUri has an entity identifier matching any type.
    #[must_use]
    pub fn has_wildcard_entity_type(&self) -> bool {
        self.ue_id & WILDCARD_ENTITY_TYPE == WILDCARD_ENTITY_TYPE
    }

    /// Checks if this UUri has a wildcard major version.
    #[must_use]
    pub fn has_wildcard_version(&self) -> bool {
        self.ue_version_major == WILDCARD_ENTITY_VERSION
    }

    /// Checks if this UUri has a wildcard entity identifier.
    #[must_use]
    pub fn has_wildcard_resource_id(&self) -> bool {
        self.resource_id == WILDCARD_RESOURCE_ID
    }

    /// Checks that this UUri does not contain any wildcards.
    ///
    /// # Errors
    ///
    /// Returns a refusal if any of this UUri's properties contain a wildcard value.
    pub fn check_no_wildcards(&self) -> Result<(), UUriRefusal> {
        if self.has_wildcard_authority() {
            Err(UUriRefusal::validation_text(
                "Authority must not contain wildcard character [*]",
            ))
        } else if self.has_wildcard_entity_instance() {
            Err(UUriRefusal::validation_text(
                "Entity instance ID must not be set to wildcard value [0xFFFF0000]",
            ))
        } else if self.has_wildcard_entity_type() {
            Err(UUriRefusal::validation_text(
                "Entity type ID must not be set to wildcard value [0xFFFF]",
            ))
        } else if self.has_wildcard_version() {
            Err(UUriRefusal::validation_text(
                "Entity version must not be set to wildcard value [0xFF]",
            ))
        } else if self.has_wildcard_resource_id() {
            Err(UUriRefusal::validation_text(
                "Resource ID must not be set to wildcard value [0xFFFF]",
            ))
        } else {
            Ok(())
        }
    }

    /// Checks if this UUri refers to a service method.
    ///
    /// Returns `true` if 0 < resource ID < 0x8000.
    #[must_use]
    pub fn is_rpc_method(&self) -> bool {
        self.resource_id > RESOURCE_ID_RESPONSE && self.resource_id < RESOURCE_ID_MIN_EVENT
    }

    /// Checks that this UUri refers to a service method.
    ///
    /// # Errors
    ///
    /// Returns a refusal if [`Self::is_rpc_method`] fails or
    /// the UUri [contains any wildcards](Self::check_no_wildcards).
    pub fn check_rpc_method(&self) -> Result<(), UUriRefusal> {
        if !self.is_rpc_method() {
            Err(UUriRefusal::validation_text(
                "Resource ID must be a value from ]0x0, 0x8000[",
            ))
        } else {
            self.check_no_wildcards()
        }
    }

    /// Checks if this UUri represents a destination for a Notification.
    ///
    /// Returns `true` if resource ID is 0.
    #[must_use]
    pub fn is_notification_destination(&self) -> bool {
        self.resource_id == RESOURCE_ID_RESPONSE
    }

    /// Checks if this UUri represents an RPC response address.
    ///
    /// Returns `true` if resource ID is 0.
    #[must_use]
    pub fn is_rpc_response(&self) -> bool {
        self.resource_id == RESOURCE_ID_RESPONSE
    }

    /// Checks that this UUri represents an RPC response address.
    ///
    /// # Errors
    ///
    /// Returns a refusal if [`Self::is_rpc_response`] fails or
    /// the UUri [contains any wildcards](Self::check_no_wildcards).
    pub fn check_rpc_response(&self) -> Result<(), UUriRefusal> {
        if !self.is_rpc_response() {
            Err(UUriRefusal::validation_text("Resource ID must be 0x0"))
        } else {
            self.check_no_wildcards()
        }
    }

    /// Checks if this UUri can be used as the source of an event or notification.
    ///
    /// Returns `true` if resource ID >= 0x8000.
    #[must_use]
    pub fn is_event(&self) -> bool {
        self.resource_id >= RESOURCE_ID_MIN_EVENT
    }

    /// Checks that this UUri can be used as the source of an event or notification.
    ///
    /// # Errors
    ///
    /// Returns a refusal if [`Self::is_event`] fails or
    /// the UUri [contains any wildcards](Self::check_no_wildcards).
    pub fn check_event(&self) -> Result<(), UUriRefusal> {
        if !self.is_event() {
            Err(UUriRefusal::validation_text(
                "Resource ID must be >= 0x8000",
            ))
        } else {
            self.check_no_wildcards()
        }
    }

    fn matches_authority(&self, candidate: &UUri) -> bool {
        self.has_wildcard_authority() || self.authority_name == candidate.authority_name
    }

    fn matches_entity_type(&self, candidate: &UUri) -> bool {
        self.has_wildcard_entity_type() || self.ue_type_id() == candidate.ue_type_id()
    }

    fn matches_entity_instance(&self, candidate: &UUri) -> bool {
        self.has_wildcard_entity_instance() || self.ue_instance_id() == candidate.ue_instance_id()
    }

    fn matches_entity_version(&self, candidate: &UUri) -> bool {
        self.has_wildcard_version()
            || self.uentity_major_version() == candidate.uentity_major_version()
    }

    fn matches_entity(&self, candidate: &UUri) -> bool {
        self.matches_entity_type(candidate)
            && self.matches_entity_instance(candidate)
            && self.matches_entity_version(candidate)
    }

    fn matches_resource(&self, candidate: &UUri) -> bool {
        self.has_wildcard_resource_id() || self.resource_id == candidate.resource_id
    }

    /// Checks if a given candidate URI matches a pattern.
    ///
    /// # Returns
    ///
    /// `true` if the candiadate matches the pattern represented by this UUri.
    // [impl->dsn~uri-pattern-matching~3]
    #[must_use]
    pub fn matches(&self, candidate: &UUri) -> bool {
        self.matches_authority(candidate)
            && self.matches_entity(candidate)
            && self.matches_resource(candidate)
    }

    /// Compatibility alias, retained for one release. Use [`Self::ue_type_id`].
    #[deprecated(note = "use ue_type_id instead; this alias will be removed next release")]
    #[must_use]
    pub fn uentity_type_id(&self) -> u16 {
        self.ue_type_id()
    }

    /// Compatibility alias, retained for one release. Use [`Self::ue_instance_id`].
    #[deprecated(note = "use ue_instance_id instead; this alias will be removed next release")]
    #[must_use]
    pub fn uentity_instance_id(&self) -> u16 {
        self.ue_instance_id()
    }

    /// The offsets of the fields that make up this type's ABI, for ABI checks.
    #[doc(hidden)]
    #[must_use]
    #[cfg(up_rust_test_hooks)]
    pub const fn __abi_offsets() -> (usize, usize, usize, usize, usize) {
        (
            core::mem::offset_of!(UUri, ue_id),
            core::mem::offset_of!(UUri, resource_id),
            core::mem::offset_of!(UUri, ue_version_major),
            core::mem::offset_of!(UUri, authority_name)
                + core::mem::offset_of!(AuthorityNameString, len),
            core::mem::offset_of!(UUri, authority_name)
                + core::mem::offset_of!(AuthorityNameString, bytes),
        )
    }
}
