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

//! Scoped native representation agreement.
//!
//! A structural type token is independent of the payload encoding.
//! A deployment agrees one native encoding and its complete representations.
//! Encoding alone never determines the application type; tokens distinguish it.
//! Immutable route agreements retain their complete generation while frames are
//! in flight. Load matching peer configurations before activating a new route;
//! changing the profile requires a new version, with old routes drained separately.

use std::any::TypeId;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, RwLock};

use sha2::{Digest, Sha256};

pub use crate::frame::metadata::NativeTypeToken;
use crate::PayloadEncoding;

mod canonical;

/// Current provisional canonical native-descriptor format version.
pub const NATIVE_DESCRIPTOR_VERSION: u32 = 2;
/// Current provisional token derivation version, including target-qualified input.
pub const NATIVE_TOKEN_DERIVATION_VERSION: u32 = 2;
/// Maximum canonical descriptor size accepted by the checked decoder (1 MiB).
pub const MAX_NATIVE_DESCRIPTOR_BYTES: usize = 1024 * 1024;

/// Compiler target identity for a native representation.
///
/// The target triple and pointer width are part of canonical identity. Equal
/// aggregate layouts on different targets do not erase this qualification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeTarget {
    triple: String,
    pointer_width: u8,
}

impl NativeTarget {
    /// Describes an explicit target for imported representation metadata.
    ///
    /// # Errors
    ///
    /// Rejects empty/oversized or malformed target names and pointer widths other
    /// than 32 or 64. This validates metadata, not a claim of remote attestation.
    pub fn new(triple: impl Into<String>, pointer_width: u8) -> Result<Self, NativeProfileError> {
        let triple = triple.into();
        if triple.is_empty()
            || triple.len() > 256
            || !triple
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            || !matches!(pointer_width, 32 | 64)
        {
            return Err(NativeProfileError::InvalidDescriptor(
                "invalid target identity",
            ));
        }
        Ok(Self {
            triple,
            pointer_width,
        })
    }

    /// Returns the compiler target built into this SDK, rather than a runtime override.
    pub fn current() -> Self {
        Self::new(env!("UP_RUST_TARGET_TRIPLE"), usize::BITS as u8)
            .expect("supported compiler target")
    }

    /// Returns the compiler target triple.
    pub fn triple(&self) -> &str {
        &self.triple
    }

    /// Returns the target pointer width in bits.
    pub fn pointer_width(&self) -> u8 {
        self.pointer_width
    }
}

/// Byte order of a native representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeByteOrder {
    /// Little-endian scalars.
    LittleEndian,
    /// Big-endian scalars.
    BigEndian,
}

impl NativeByteOrder {
    /// Returns the current target's scalar byte order.
    pub const fn native() -> Self {
        if cfg!(target_endian = "little") {
            Self::LittleEndian
        } else {
            Self::BigEndian
        }
    }
}

/// One named field (or repeated array element) in a native representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeField {
    name: String,
    offset: u64,
    count: u64,
    representation: NativeRepresentation,
}

impl NativeField {
    /// Describes a field at a byte offset. Its enclosing representation checks
    /// bounds, alignment, overlap and duplicate names.
    pub fn new(name: impl Into<String>, offset: u64, representation: NativeRepresentation) -> Self {
        Self {
            name: name.into(),
            offset,
            count: 1,
            representation,
        }
    }

    /// Describes repeated elements, including a zero-length array.
    pub fn repeated(
        name: impl Into<String>,
        offset: u64,
        count: u64,
        representation: NativeRepresentation,
    ) -> Self {
        Self {
            name: name.into(),
            offset,
            count,
            representation,
        }
    }
}

/// Complete structural description of a native value in one target ABI.
///
/// Names are deployment-stable cross-language names, not Rust compiler type
/// names. Nested fields include offsets, counts and their own representations;
/// equal aggregate size/alignment is insufficient to establish type identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeRepresentation {
    target: NativeTarget,
    name: String,
    revision: u32,
    size: u64,
    alignment: u64,
    byte_order: NativeByteOrder,
    fields: Vec<NativeField>,
}

impl NativeRepresentation {
    /// Checks and constructs a scalar, struct or array representation for the
    /// current compiler target.
    ///
    /// Scalar representations have no fields and use distinct names for distinct
    /// semantics, for example `i32`, `u32`, `f32`, `bool` and `char`.
    ///
    /// # Errors
    ///
    /// Rejects empty/duplicate names, invalid alignment, overflowing/out-of-bounds
    /// fields, overlapping nonempty fields and mixed-target children. This describes layout; it does
    /// not prove that arbitrary bytes are a valid value of that type.
    pub fn new(
        name: impl Into<String>,
        revision: u32,
        size: u64,
        alignment: u64,
        byte_order: NativeByteOrder,
        fields: Vec<NativeField>,
    ) -> Result<Self, NativeProfileError> {
        Self::for_target(
            NativeTarget::current(),
            name,
            revision,
            size,
            alignment,
            byte_order,
            fields,
        )
    }

    /// Checks a representation exported by an explicitly identified target.
    ///
    /// Field representations must have the same target and byte order. Importing
    /// a descriptor does not establish that a value can be borrowed on this host.
    ///
    /// # Errors
    ///
    /// Applies the layout checks of [`Self::new`] and rejects mixed-target fields.
    pub fn for_target(
        target: NativeTarget,
        name: impl Into<String>,
        revision: u32,
        size: u64,
        alignment: u64,
        byte_order: NativeByteOrder,
        fields: Vec<NativeField>,
    ) -> Result<Self, NativeProfileError> {
        let name = name.into();
        if name.is_empty() || !alignment.is_power_of_two() || !size.is_multiple_of(alignment) {
            return Err(NativeProfileError::InvalidDescriptor(
                "invalid name, size or alignment",
            ));
        }
        let mut names = BTreeSet::new();
        let mut ranges = Vec::new();
        for field in &fields {
            if field.name.is_empty() || !names.insert(&field.name) {
                return Err(NativeProfileError::InvalidDescriptor(
                    "empty or duplicate field name",
                ));
            }
            let layout = &field.representation;
            if layout.target != target || layout.byte_order != byte_order {
                return Err(NativeProfileError::InvalidDescriptor(
                    "mixed-target representation fields",
                ));
            }
            let end = layout
                .size
                .checked_mul(field.count)
                .and_then(|len| field.offset.checked_add(len))
                .ok_or(NativeProfileError::InvalidDescriptor("field size overflow"))?;
            if end > size
                || layout.alignment > alignment
                || !field.offset.is_multiple_of(layout.alignment)
            {
                return Err(NativeProfileError::InvalidDescriptor(
                    "field is out of bounds or misaligned",
                ));
            }
            if end != field.offset {
                ranges.push((field.offset, end));
            }
        }
        ranges.sort_unstable();
        let mut previous_end = 0;
        for (start, end) in ranges {
            if start < previous_end {
                return Err(NativeProfileError::InvalidDescriptor("overlapping fields"));
            }
            previous_end = end;
        }
        Ok(Self {
            target,
            name,
            revision,
            size,
            alignment,
            byte_order,
            fields,
        })
    }

    /// Returns the target qualification retained with the complete layout.
    pub fn target(&self) -> &NativeTarget {
        &self.target
    }

    /// Returns the deployment-stable type name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the declared representation revision.
    pub fn revision(&self) -> u32 {
        self.revision
    }

    /// Returns the scalar byte order of this target representation.
    pub fn byte_order(&self) -> NativeByteOrder {
        self.byte_order
    }

    /// Returns the complete object size in bytes, including padding.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Returns the required address alignment in bytes.
    pub fn alignment(&self) -> u64 {
        self.alignment
    }

    /// Returns canonical structural bytes, versioned independently of registry IDs.
    ///
    /// Version 2 uses the ASCII domain below. Each representation begins with a
    /// u64-length-prefixed target triple and u8 pointer width, then a length-prefixed
    /// UTF-8 name, little-endian u32 revision, u64 size/alignment, byte order (1/2),
    /// u64 field count, then each field's name, u64 offset/count and recursively
    /// encoded representation. String lengths are u64; fields retain declaration
    /// order. Array elements use one repeated field rather than expanding values.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = canonical::HEADER.to_vec();
        self.append_canonical(&mut bytes);
        bytes
    }

    fn append_canonical(&self, bytes: &mut Vec<u8>) {
        append_string(bytes, self.target.triple());
        bytes.push(self.target.pointer_width());
        append_string(bytes, &self.name);
        bytes.extend_from_slice(&self.revision.to_le_bytes());
        bytes.extend_from_slice(&self.size.to_le_bytes());
        bytes.extend_from_slice(&self.alignment.to_le_bytes());
        bytes.push(match self.byte_order {
            NativeByteOrder::LittleEndian => 1,
            NativeByteOrder::BigEndian => 2,
        });
        bytes.extend_from_slice(&(self.fields.len() as u64).to_le_bytes());
        for field in &self.fields {
            append_string(bytes, &field.name);
            bytes.extend_from_slice(&field.offset.to_le_bytes());
            bytes.extend_from_slice(&field.count.to_le_bytes());
            field.representation.append_canonical(bytes);
        }
    }

    /// Validates and decodes one complete provisional version-2 descriptor.
    ///
    /// # Errors
    ///
    /// Rejects other versions, malformed/trailing bytes, invalid layouts or
    /// target disagreements, input larger than [`MAX_NATIVE_DESCRIPTOR_BYTES`],
    /// and nesting deeper than 128. Counts and extents are checked before reserve.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, NativeProfileError> {
        canonical::decode(bytes)
    }

    /// Derives the token from the first four SHA-256 digest bytes, interpreted
    /// little-endian, over the version-2 target-qualified canonical descriptor.
    /// Profiles reject collisions and retain full descriptors. This derivation is
    /// provisional and must be agreed by version before interpreting a token.
    pub fn token(&self) -> NativeTypeToken {
        let digest = Sha256::digest(self.canonical_bytes());
        let first: [u8; 4] = digest
            .as_slice()
            .split_at(4)
            .0
            .try_into()
            .expect("SHA-256 prefix");
        NativeTypeToken::from_u32(u32::from_le_bytes(first))
    }
}

fn append_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

/// One checked native payload identity within a deployment profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativePayloadIdentity {
    encoding: PayloadEncoding,
    token: NativeTypeToken,
}

impl NativePayloadIdentity {
    /// Returns the deployment-assigned 16-bit encoding, or explicit ID zero.
    pub fn encoding(self) -> PayloadEncoding {
        self.encoding
    }
    /// Returns the independent structural type token.
    pub fn token(self) -> NativeTypeToken {
        self.token
    }
}

/// One agreed native encoding and its admitted representations.
///
/// The encoding describes the native format, not a Rust type. Independent tokens
/// distinguish complete representations within this immutable deployment profile.
/// Explicit [`PayloadEncoding::CONTRACT_DEFINED`] uses the same rules as a private
/// encoding; it is never selected as a fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeProfile {
    domain: String,
    version: u32,
    encoding: PayloadEncoding,
    representations: BTreeMap<NativeTypeToken, NativeRepresentation>,
}

impl NativeProfile {
    /// Checks the native encoding and representations for a deployment generation.
    ///
    /// # Errors
    ///
    /// Rejects an empty domain, version zero, IDs outside the private range other
    /// than explicit zero, mixed targets/byte orders, duplicate representations and
    /// short-token collisions. Encoding identity alone never selects a type.
    pub fn new(
        domain: impl Into<String>,
        version: u32,
        encoding: PayloadEncoding,
        representations: impl IntoIterator<Item = NativeRepresentation>,
    ) -> Result<Self, NativeProfileError> {
        let domain = domain.into();
        if domain.is_empty() || version == 0 {
            return Err(NativeProfileError::InvalidProfile(
                "domain and nonzero version are required",
            ));
        }
        if !encoding.is_private_use() && encoding != PayloadEncoding::CONTRACT_DEFINED {
            return Err(NativeProfileError::UnsupportedEncoding(encoding));
        }
        let mut admitted = BTreeMap::new();
        let mut target = None;
        for representation in representations {
            let actual = (representation.target.clone(), representation.byte_order);
            if target.as_ref().is_some_and(|expected| expected != &actual) {
                return Err(NativeProfileError::InvalidDescriptor(
                    "mixed-target native profile",
                ));
            }
            target = Some(actual);
            let token = representation.token();
            if admitted.insert(token, representation).is_some() {
                return Err(NativeProfileError::TokenCollision(token));
            }
        }
        Ok(Self {
            domain,
            version,
            encoding,
            representations: admitted,
        })
    }

    /// Returns the encoding shared by all representations in this profile.
    pub fn encoding(&self) -> PayloadEncoding {
        self.encoding
    }
    /// Returns admitted representations in ascending token order.
    pub fn representations(&self) -> impl Iterator<Item = &NativeRepresentation> {
        self.representations.values()
    }
    /// Returns the deployment domain.
    pub fn domain(&self) -> &str {
        &self.domain
    }
    /// Returns the immutable generation version.
    pub fn version(&self) -> u32 {
        self.version
    }
    /// Returns canonical agreement bytes including the complete representation set.
    ///
    /// Version 3 uses the ASCII domain below: u64-length-prefixed UTF-8 domain,
    /// u32 LE version, u16 LE encoding, then u64 LE representation count,
    /// followed by ascending u32 LE tokens and target-qualified representations.
    /// There is no mode tag. Descriptor/token-derivation version 2 is unchanged.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = b"uprotocol.native-profile.v3\0".to_vec();
        append_string(&mut bytes, &self.domain);
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&(self.encoding.id() as u16).to_le_bytes());
        bytes.extend_from_slice(&(self.representations.len() as u64).to_le_bytes());
        for (token, representation) in &self.representations {
            bytes.extend_from_slice(&token.as_u32().to_le_bytes());
            representation.append_canonical(&mut bytes);
        }
        bytes
    }

    /// Returns the full SHA-256 content identity for deployment evidence.
    pub fn content_digest(&self) -> [u8; 32] {
        Sha256::digest(self.canonical_bytes()).into()
    }
}

/// Construction-time agreement between loaded peer configurations.
///
/// This is not a network handshake. Deployment tooling supplies the peer's
/// configuration; a route must not substitute its own configuration for that
/// evidence. Clones retain the same immutable generation for in-flight work,
/// and share one per-generation cache of resolved type identities so that the
/// per-message path neither rebuilds descriptors nor hashes them.
#[derive(Clone, Debug)]
pub struct NativeProfileAgreement {
    profile: Arc<NativeProfile>,
    digest: [u8; 32],
    // Read only by the stable-payload resolver, which is feature-gated; the
    // field is kept unconditionally so the type has one shape per build.
    #[cfg_attr(not(feature = "zero-copy-transport"), allow(dead_code))]
    identities: Arc<RwLock<HashMap<TypeId, NativePayloadIdentity>>>,
}

impl NativeProfileAgreement {
    /// Confirms exact domain, version and complete representation assignments.
    ///
    /// # Errors
    ///
    /// Rejects any disagreement, including equal domain/version with different
    /// assignments. Full equality is checked, not only the short type tokens.
    pub fn new(
        local: Arc<NativeProfile>,
        peer: &NativeProfile,
    ) -> Result<Self, NativeProfileError> {
        if local.as_ref() != peer {
            return Err(NativeProfileError::ProfileMismatch);
        }
        let digest = local.content_digest();
        Ok(Self {
            profile: local,
            digest,
            identities: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Returns the retained profile generation.
    pub fn profile(&self) -> &NativeProfile {
        &self.profile
    }

    /// Returns the SHA-256 content identity computed once at construction.
    pub fn content_digest(&self) -> [u8; 32] {
        self.digest
    }

    /// Whether both agreements retain the same immutable generation.
    ///
    /// Clones of one agreement share their `Arc`; independently loaded copies
    /// of the same artifact compare by the cached content digest. Neither path
    /// walks the representations, so this is safe to call per message.
    #[must_use]
    pub fn is_same_generation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.profile, &other.profile) || self.digest == other.digest
    }

    /// Resolves the identity of the stable payload type `T` under this
    /// generation, retaining a successful result for every later call.
    ///
    /// This is the only writer of the per-generation cache. The key is the
    /// `TypeId` of `T` and the value is the identity that
    /// [`Self::identity_for`] derives from the representation `represent`
    /// builds for that same `T`; both come from crate code, so no caller can
    /// associate one type's identity with another type. The cache is keyed by
    /// `TypeId`, which is sound because a stable payload's descriptor is a
    /// property of the type and the generation is immutable.
    ///
    /// Only successful resolutions are cached; an error is returned unchanged,
    /// and the next call builds the representation again, so a misconfigured
    /// type keeps reporting its reason until it is fixed.
    ///
    /// # Errors
    ///
    /// Returns the error from `represent` or from [`Self::identity_for`].
    #[cfg(feature = "zero-copy-transport")]
    pub(crate) fn resolve_type_identity<T: 'static, E>(
        &self,
        represent: impl FnOnce() -> Result<NativeRepresentation, E>,
    ) -> Result<NativePayloadIdentity, E>
    where
        E: From<NativeProfileError>,
    {
        let key = TypeId::of::<T>();
        if let Some(identity) = self
            .identities
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
        {
            return Ok(*identity);
        }
        let representation = represent()?;
        let identity = self.identity_for(&representation)?;
        self.identities
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key, identity);
        Ok(identity)
    }

    /// Test hook: resolves through the same cache, so a test can count how
    /// often its validator ran. Not a stable API.
    #[cfg(feature = "zero-copy-transport")]
    #[doc(hidden)]
    #[cfg(up_rust_test_hooks)]
    pub fn __resolve_type_identity_for_test<T: 'static, E>(
        &self,
        represent: impl FnOnce() -> Result<NativeRepresentation, E>,
    ) -> Result<NativePayloadIdentity, E>
    where
        E: From<NativeProfileError>,
    {
        self.resolve_type_identity::<T, E>(represent)
    }

    /// Prepares a matching, newer generation before the route owner activates it.
    /// Existing agreements retain the old generation for in-flight frames.
    ///
    /// # Errors
    ///
    /// Rejects domain changes, non-increasing versions and peer disagreement.
    /// A domain change requires constructing a separate route.
    pub fn successor(
        &self,
        local: Arc<NativeProfile>,
        peer: &NativeProfile,
    ) -> Result<Self, NativeProfileError> {
        if local.domain != self.profile.domain || local.version <= self.profile.version {
            return Err(NativeProfileError::InvalidProfile(
                "successor must retain the domain and advance the version",
            ));
        }
        Self::new(local, peer)
    }

    /// Resolves a complete representation under this route's agreed profile.
    ///
    /// # Errors
    ///
    /// Rejects representations absent from the profile.
    pub fn identity_for(
        &self,
        representation: &NativeRepresentation,
    ) -> Result<NativePayloadIdentity, NativeProfileError> {
        let token = self
            .profile
            .representations
            .iter()
            .find_map(|(token, actual)| (actual == representation).then_some(*token))
            .ok_or(NativeProfileError::UnknownRepresentation)?;
        Ok(NativePayloadIdentity {
            encoding: self.profile.encoding,
            token,
        })
    }

    /// Checks a carried encoding/token pair against the complete profile.
    ///
    /// # Errors
    ///
    /// Rejects an unknown ID or a mismatched token. Payload validity is separate.
    pub fn verify_carried(
        &self,
        encoding: PayloadEncoding,
        token: NativeTypeToken,
    ) -> Result<(), NativeProfileError> {
        if encoding != self.profile.encoding {
            return Err(NativeProfileError::UnknownEncoding(encoding));
        }
        if !self.profile.representations.contains_key(&token) {
            return Err(NativeProfileError::IdentityMismatch);
        }
        Ok(())
    }

    /// Checks the complete expected representation as well as the carried pair.
    ///
    /// # Errors
    ///
    /// Rejects missing membership, foreign encoding IDs and mismatched tokens.
    pub fn verify_representation(
        &self,
        encoding: PayloadEncoding,
        token: NativeTypeToken,
        representation: &NativeRepresentation,
    ) -> Result<(), NativeProfileError> {
        let expected = self.identity_for(representation)?;
        if expected.encoding != encoding || expected.token != token {
            return Err(NativeProfileError::IdentityMismatch);
        }
        Ok(())
    }
}

/// Errors establishing or using a scoped native agreement.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum NativeProfileError {
    /// The structural description is internally inconsistent.
    #[error("invalid native representation: {0}")]
    InvalidDescriptor(&'static str),
    /// The profile lacks an explicit domain or nonzero version.
    #[error("invalid native profile: {0}")]
    InvalidProfile(&'static str),
    /// An assignment uses neither a deployment-private ID nor explicit zero.
    #[error("native encoding {0} is neither private-use nor contract-defined")]
    UnsupportedEncoding(PayloadEncoding),
    /// Multiple entries share a short structural token.
    #[error("native type token collision: {0:?}")]
    TokenCollision(NativeTypeToken),
    /// The full representation has no agreed assignment.
    #[error("native representation is not in the agreed profile")]
    UnknownRepresentation,
    /// The carried ID has no agreed assignment.
    #[error("payload encoding {0} is not in the agreed native profile")]
    UnknownEncoding(PayloadEncoding),
    /// A carried identity differs from the agreed representation.
    #[error("native encoding/type token mismatch")]
    IdentityMismatch,
    /// Loaded peer configurations differ.
    #[error("native profile domain, version or content differs")]
    ProfileMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use test_case::test_case;

    fn scalar(name: &str) -> NativeRepresentation {
        NativeRepresentation::new(name, 0, 4, 4, NativeByteOrder::native(), vec![]).unwrap()
    }

    fn fixed_target_scalar(name: &str) -> NativeRepresentation {
        NativeRepresentation::for_target(
            NativeTarget::new("x86_64-unknown-linux-gnu", 64).unwrap(),
            name,
            0,
            4,
            4,
            NativeByteOrder::LittleEndian,
            vec![],
        )
        .unwrap()
    }

    fn encoding(id: u32) -> PayloadEncoding {
        PayloadEncoding::from_id(id).unwrap()
    }

    fn profile(id: u32, name: &str) -> NativeProfile {
        NativeProfile::new("vehicle", 1, encoding(id), [scalar(name)]).unwrap()
    }

    #[test_case(8; "unassigned ID is not a native allocation")]
    #[test_case(0xDFFF; "registry range is not private")]
    #[test_case(0xEFFF; "reserved range is not private")]
    #[test_case(0xFFFF; "future reserved value is not a native allocation")]
    fn profile_rejects_unassigned_or_reserved_encoding(id: u32) {
        assert_eq!(
            NativeProfile::new("vehicle", 1, encoding(id), [scalar("u32")]),
            Err(NativeProfileError::UnsupportedEncoding(encoding(id)))
        );
    }

    #[test_case(0xF000; "first private ID")]
    #[test_case(0xFFFE; "last private ID")]
    fn profile_keeps_encoding_separate_from_token(id: u32) {
        let profile = profile(id, "u32");
        let agreement = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
        let identity = agreement.identity_for(&scalar("u32")).unwrap();
        assert_eq!(identity.encoding().id(), id);
        assert_eq!(identity.token(), scalar("u32").token());
        assert_eq!(
            agreement.verify_representation(identity.encoding(), identity.token(), &scalar("i32")),
            Err(NativeProfileError::UnknownRepresentation)
        );
    }

    #[test]
    fn duplicate_representations_fail() {
        let result = NativeProfile::new(
            "vehicle",
            1,
            encoding(0xF000),
            [scalar("u32"), scalar("u32")],
        );
        assert_eq!(
            result,
            Err(NativeProfileError::TokenCollision(scalar("u32").token()))
        );
    }

    #[test]
    fn independently_computed_token_vector_is_frozen() {
        // Independent Python hashlib.sha256 over target-qualified version 2 bytes.
        assert_eq!(fixed_target_scalar("u32").token().as_u32(), 0x3140_5817);
    }

    #[test]
    fn real_short_token_collision_is_rejected() {
        // Independently found for the fixed x86_64 target and version 2 descriptor.
        // Distinct full SHA-256 digests share the same first four bytes.
        let first = fixed_target_scalar("collision.23393");
        let second = fixed_target_scalar("collision.37407");
        assert_ne!(first.canonical_bytes(), second.canonical_bytes());
        assert_eq!(first.token(), NativeTypeToken::from_u32(0xb61b_9d85));
        assert_eq!(first.token(), second.token());
        assert!(matches!(
            NativeProfile::new("vehicle", 1, encoding(0xF000), [first, second]),
            Err(NativeProfileError::TokenCollision(_))
        ));
    }

    #[test_case("vehicle", 1, 0xF001, "u32"; "same domain and version different assignment")]
    #[test_case("vehicle", 1, 0xF000, "i32"; "same size different scalar")]
    #[test_case("other", 1, 0xF000, "u32"; "different domain")]
    #[test_case("vehicle", 2, 0xF000, "u32"; "different generation")]
    fn route_rejects_peer_disagreement(domain: &str, version: u32, id: u32, name: &str) {
        let local = profile(0xF000, "u32");
        let peer = NativeProfile::new(domain, version, encoding(id), [scalar(name)]).unwrap();
        assert!(matches!(
            NativeProfileAgreement::new(Arc::new(local), &peer),
            Err(NativeProfileError::ProfileMismatch)
        ));
    }

    #[test]
    fn canonical_profile_is_independent_of_registration_order() {
        let representations = [scalar("u32"), scalar("i32")];
        let first =
            NativeProfile::new("vehicle", 1, encoding(0xF101), representations.clone()).unwrap();
        let second = NativeProfile::new(
            "vehicle",
            1,
            encoding(0xF101),
            representations.into_iter().rev(),
        )
        .unwrap();
        assert_eq!(first.canonical_bytes(), second.canonical_bytes());
        assert_eq!(first.content_digest(), second.content_digest());
        assert!(NativeProfileAgreement::new(Arc::new(first), &second).is_ok());
    }

    #[test]
    fn explicit_zero_requires_exact_representation_and_peer_content() {
        let representation = scalar("u32");
        let local = NativeProfile::new(
            "vehicle",
            1,
            PayloadEncoding::CONTRACT_DEFINED,
            [representation],
        )
        .unwrap();
        let agreement = NativeProfileAgreement::new(Arc::new(local.clone()), &local).unwrap();
        let identity = agreement.identity_for(&scalar("u32")).unwrap();
        assert_eq!(identity.encoding(), PayloadEncoding::CONTRACT_DEFINED);
        assert_eq!(identity.token(), scalar("u32").token());
        assert_eq!(
            agreement.identity_for(&scalar("i32")),
            Err(NativeProfileError::UnknownRepresentation)
        );
        assert_eq!(
            agreement.verify_carried(identity.encoding(), identity.token()),
            Ok(())
        );
        let wrong_token = NativeTypeToken::from_u32(identity.token().as_u32() ^ 1);
        assert_eq!(
            agreement.verify_carried(PayloadEncoding::CONTRACT_DEFINED, wrong_token),
            Err(NativeProfileError::IdentityMismatch)
        );
        assert_eq!(
            agreement.verify_carried(encoding(0xF000), identity.token()),
            Err(NativeProfileError::UnknownEncoding(encoding(0xF000)))
        );
        let peer = NativeProfile::new(
            "vehicle",
            1,
            PayloadEncoding::CONTRACT_DEFINED,
            [scalar("i32")],
        )
        .unwrap();
        assert!(matches!(
            NativeProfileAgreement::new(Arc::new(local), &peer),
            Err(NativeProfileError::ProfileMismatch)
        ));
    }

    #[test]
    fn explicit_zero_uses_assignment_format_without_changing_descriptor_bytes() {
        let representation = scalar("u32");
        let profile = NativeProfile::new(
            "vehicle",
            1,
            PayloadEncoding::CONTRACT_DEFINED,
            [representation.clone()],
        )
        .unwrap();
        let mut expected = b"uprotocol.native-profile.v3\0".to_vec();
        append_string(&mut expected, "vehicle");
        expected.extend_from_slice(&1u32.to_le_bytes());
        expected.extend_from_slice(&0u16.to_le_bytes());
        expected.extend_from_slice(&1u64.to_le_bytes());
        expected.extend_from_slice(&representation.token().as_u32().to_le_bytes());
        representation.append_canonical(&mut expected);
        assert_eq!(profile.canonical_bytes(), expected);
        assert_eq!(NATIVE_DESCRIPTOR_VERSION, 2);
        assert_eq!(NATIVE_TOKEN_DERIVATION_VERSION, 2);
    }

    #[test_case(0; "explicit contract")]
    #[test_case(0xF100; "private native encoding")]
    fn one_encoding_admits_distinct_representations(id: u32) {
        let profile =
            NativeProfile::new("vehicle", 1, encoding(id), [scalar("u32"), scalar("i32")]).unwrap();
        let agreement = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
        let unsigned = agreement.identity_for(&scalar("u32")).unwrap();
        let signed = agreement.identity_for(&scalar("i32")).unwrap();
        assert_eq!(unsigned.encoding(), signed.encoding());
        assert_eq!(unsigned.encoding(), encoding(id));
        assert_ne!(unsigned.token(), signed.token());
        assert_eq!(
            agreement.verify_carried(unsigned.encoding(), unsigned.token()),
            Ok(())
        );
        assert_eq!(
            agreement.verify_carried(signed.encoding(), signed.token()),
            Ok(())
        );
        assert_eq!(
            agreement.verify_representation(unsigned.encoding(), unsigned.token(), &scalar("i32")),
            Err(NativeProfileError::IdentityMismatch)
        );
    }

    #[test]
    fn explicit_zero_is_not_a_fallback_for_an_empty_profile() {
        let profile = NativeProfile::new("vehicle", 1, encoding(0xF100), []).unwrap();
        assert_eq!(profile.representations().count(), 0);
        let agreement = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
        assert_eq!(
            agreement.verify_carried(PayloadEncoding::CONTRACT_DEFINED, scalar("u32").token()),
            Err(NativeProfileError::UnknownEncoding(
                PayloadEncoding::CONTRACT_DEFINED
            ))
        );
        assert_eq!(
            agreement.identity_for(&scalar("u32")),
            Err(NativeProfileError::UnknownRepresentation)
        );
    }

    #[test_case("", 1; "empty domain")]
    #[test_case("vehicle", 0; "zero generation")]
    fn profile_requires_domain_and_generation(domain: &str, version: u32) {
        assert_eq!(
            NativeProfile::new(domain, version, encoding(0xF100), []),
            Err(NativeProfileError::InvalidProfile(
                "domain and nonzero version are required"
            ))
        );
    }

    #[test_case(0; "explicit zero")]
    #[test_case(0xF101; "private assignment")]
    fn canonical_assignment_includes_encoding_and_complete_representation(id: u32) {
        let representation = fixed_target_scalar("u32");
        let profile =
            NativeProfile::new("vehicle", 1, encoding(id), [representation.clone()]).unwrap();
        let mut expected = b"uprotocol.native-profile.v3\0".to_vec();
        append_string(&mut expected, "vehicle");
        expected.extend_from_slice(&1u32.to_le_bytes());
        expected.extend_from_slice(&(id as u16).to_le_bytes());
        expected.extend_from_slice(&1u64.to_le_bytes());
        expected.extend_from_slice(&representation.token().as_u32().to_le_bytes());
        representation.append_canonical(&mut expected);
        assert_eq!(profile.canonical_bytes(), expected);
        assert_eq!(profile.representations().next(), Some(&representation));
    }

    #[test]
    fn fields_distinguish_equal_sized_representations() {
        let make = |name| {
            NativeRepresentation::new(
                "Packet",
                1,
                4,
                4,
                NativeByteOrder::LittleEndian,
                vec![NativeField::new("value", 0, scalar(name))],
            )
            .unwrap()
        };
        assert_ne!(make("u32").token(), make("i32").token());
        assert_ne!(make("u32").canonical_bytes(), make("i32").canonical_bytes());
    }

    #[test_case("vehicle", 1; "changed table without version advance")]
    #[test_case("other", 2; "domain change on live route")]
    fn successor_rejects_invalid_generation(domain: &str, version: u32) {
        let current = profile(0xF000, "u32");
        let agreement = NativeProfileAgreement::new(Arc::new(current.clone()), &current).unwrap();
        let next = NativeProfile::new(domain, version, encoding(0xF001), [scalar("u32")]).unwrap();
        assert!(matches!(
            agreement.successor(Arc::new(next.clone()), &next),
            Err(NativeProfileError::InvalidProfile(
                "successor must retain the domain and advance the version"
            ))
        ));
    }

    #[test]
    fn successor_preserves_in_flight_generation() {
        let current = profile(0xF000, "u32");
        let old = NativeProfileAgreement::new(Arc::new(current.clone()), &current).unwrap();
        let next = NativeProfile::new("vehicle", 2, encoding(0xF001), [scalar("u32")]).unwrap();
        assert!(matches!(
            old.successor(Arc::new(next.clone()), &current),
            Err(NativeProfileError::ProfileMismatch)
        ));
        let new = old.successor(Arc::new(next.clone()), &next).unwrap();
        assert_eq!(
            old.identity_for(&scalar("u32")).unwrap().encoding(),
            encoding(0xF000)
        );
        assert_eq!(
            new.identity_for(&scalar("u32")).unwrap().encoding(),
            encoding(0xF001)
        );
    }

    #[test_case(0, "overlapping fields"; "overlap")]
    #[test_case(2, "field is out of bounds or misaligned"; "misaligned")]
    #[test_case(8, "field is out of bounds or misaligned"; "out of bounds")]
    #[test_case(u64::MAX, "field size overflow"; "overflow")]
    fn invalid_field_layout_is_rejected(offset: u64, reason: &'static str) {
        assert_eq!(
            NativeRepresentation::new(
                "Packet",
                1,
                8,
                4,
                NativeByteOrder::LittleEndian,
                vec![
                    NativeField::new("a", 0, scalar("u32")),
                    NativeField::new("b", offset, scalar("i32"))
                ]
            ),
            Err(NativeProfileError::InvalidDescriptor(reason))
        );
    }

    #[test]
    fn canonical_round_trip_preserves_complete_target_and_nested_fields() {
        let value = NativeRepresentation::new(
            "pair",
            7,
            8,
            4,
            NativeByteOrder::native(),
            vec![NativeField::repeated("values", 0, 2, scalar("u32"))],
        )
        .unwrap();
        let bytes = value.canonical_bytes();
        let decoded = NativeRepresentation::from_canonical_bytes(&bytes).unwrap();
        assert_eq!(decoded, value);
        assert_eq!(decoded.target(), &NativeTarget::current());
        assert_eq!(decoded.token(), value.token());
    }

    #[test]
    fn equal_layouts_on_different_targets_have_distinct_identity() {
        let host = fixed_target_scalar("u32");
        let arm = NativeRepresentation::for_target(
            NativeTarget::new("aarch64-unknown-linux-gnu", 64).unwrap(),
            "u32",
            0,
            4,
            4,
            NativeByteOrder::LittleEndian,
            vec![],
        )
        .unwrap();
        assert_ne!(host.canonical_bytes(), arm.canonical_bytes());
        assert_ne!(host.token(), arm.token());
        assert_eq!(
            NativeProfile::new("vehicle", 1, encoding(0xF100), [host.clone(), arm.clone()]),
            Err(NativeProfileError::InvalidDescriptor(
                "mixed-target native profile"
            ))
        );
        assert_eq!(
            NativeRepresentation::for_target(
                host.target.clone(),
                "mixed",
                0,
                4,
                4,
                NativeByteOrder::LittleEndian,
                vec![NativeField::new("field", 0, arm)]
            ),
            Err(NativeProfileError::InvalidDescriptor(
                "mixed-target representation fields"
            ))
        );
    }

    #[test]
    fn canonical_decoder_rejects_old_truncated_trailing_and_hostile_inputs() {
        let bytes = fixed_target_scalar("u32").canonical_bytes();
        for end in 0..bytes.len() {
            assert!(
                matches!(
                    NativeRepresentation::from_canonical_bytes(bytes.get(..end).unwrap()),
                    Err(NativeProfileError::InvalidDescriptor(_))
                ),
                "truncation {end}"
            );
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            NativeRepresentation::from_canonical_bytes(&trailing),
            Err(NativeProfileError::InvalidDescriptor(
                "trailing canonical descriptor bytes"
            ))
        );
        assert_eq!(
            NativeRepresentation::from_canonical_bytes(b"uprotocol.native-representation.v1\0"),
            Err(NativeProfileError::InvalidDescriptor(
                "unsupported canonical descriptor version"
            ))
        );
        let mut hostile_count = bytes;
        let start = hostile_count.len() - 8;
        hostile_count
            .get_mut(start..)
            .unwrap()
            .copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(
            NativeRepresentation::from_canonical_bytes(&hostile_count),
            Err(NativeProfileError::InvalidDescriptor(
                "descriptor field count exceeds remaining bytes"
            ))
        );
        let mut hostile_string = canonical::HEADER.to_vec();
        hostile_string.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(
            NativeRepresentation::from_canonical_bytes(&hostile_string),
            Err(NativeProfileError::InvalidDescriptor(
                "truncated canonical descriptor" | "descriptor length exceeds usize"
            ))
        ));
        assert_eq!(
            NativeRepresentation::from_canonical_bytes(&vec![0; MAX_NATIVE_DESCRIPTOR_BYTES + 1]),
            Err(NativeProfileError::InvalidDescriptor(
                "canonical descriptor exceeds input limit"
            ))
        );
    }

    #[test_case("", "a", "b", "invalid name, size or alignment"; "empty representation name")]
    #[test_case("Pair", "", "b", "empty or duplicate field name"; "empty field name")]
    #[test_case("Pair", "value", "value", "empty or duplicate field name"; "duplicate field names")]
    fn invalid_names_are_rejected(name: &str, first: &str, second: &str, reason: &'static str) {
        assert_eq!(
            NativeRepresentation::new(
                name,
                1,
                8,
                4,
                NativeByteOrder::native(),
                vec![
                    NativeField::new(first, 0, scalar("u32")),
                    NativeField::new(second, 4, scalar("u32"))
                ]
            ),
            Err(NativeProfileError::InvalidDescriptor(reason))
        );
    }

    #[test_case(128, true; "maximum nesting accepted")]
    #[test_case(129, false; "excessive nesting rejected")]
    fn canonical_nesting_is_bounded(depth: usize, accepted: bool) {
        let mut representation = scalar("u32");
        for _ in 0..depth {
            representation = NativeRepresentation::new(
                "nested",
                0,
                4,
                4,
                NativeByteOrder::native(),
                vec![NativeField::new("value", 0, representation)],
            )
            .unwrap();
        }
        let decoded = NativeRepresentation::from_canonical_bytes(&representation.canonical_bytes());
        if accepted {
            assert_eq!(decoded, Ok(representation));
        } else {
            assert_eq!(
                decoded,
                Err(NativeProfileError::InvalidDescriptor(
                    "descriptor nesting limit exceeded"
                ))
            );
        }
    }
}
