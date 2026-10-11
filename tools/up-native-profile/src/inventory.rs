// SPDX-License-Identifier: Apache-2.0
//! Target-side export and checked import of linked native descriptors.
use crate::{check_hash, content_hash, digest, invalid, ProfileToolError, FORMAT_VERSION};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use up_rust::{
    native_inventory::registered_native_types, NativeByteOrder, NativeRepresentation, NativeTarget,
    MAX_NATIVE_DESCRIPTOR_BYTES, NATIVE_DESCRIPTOR_VERSION, NATIVE_TOKEN_DERIVATION_VERSION,
};

/// Complete target qualification used in artifact headers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Actual compiler target triple.
    pub triple: String,
    /// Scalar byte order, `little` or `big`.
    pub endianness: String,
    /// Pointer width in bits.
    pub pointer_width: u8,
}

impl Target {
    /// Captures this executable's compiled target rather than a command-line label.
    pub fn current() -> Self {
        let target = NativeTarget::current();
        Self {
            triple: target.triple().to_owned(),
            pointer_width: target.pointer_width(),
            endianness: order_name(NativeByteOrder::native()).to_owned(),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ProfileToolError> {
        NativeTarget::new(self.triple.clone(), self.pointer_width)?;
        if !matches!(self.endianness.as_str(), "little" | "big") {
            return Err(invalid("unknown target byte order"));
        }
        Ok(())
    }

    pub(crate) fn matches(&self, value: &NativeRepresentation) -> bool {
        value.target().triple() == self.triple
            && value.target().pointer_width() == self.pointer_width
            && order_name(value.byte_order()) == self.endianness
    }
}

fn order_name(order: NativeByteOrder) -> &'static str {
    match order {
        NativeByteOrder::LittleEndian => "little",
        NativeByteOrder::BigEndian => "big",
    }
}

/// Stable application name and explicit representation revision.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypeKey {
    /// Deployment-stable application type name.
    pub name: String,
    /// Explicit representation revision (zero is valid).
    pub revision: u32,
}

impl std::fmt::Display for TypeKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}@{}", self.name, self.revision)
    }
}

/// Canonical descriptor and independent full/short identities for one type.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeDescriptor {
    /// Stable type identity selected by the application.
    pub key: TypeKey,
    /// Standard padded Base64 of the SDK's complete canonical descriptor.
    pub canonical_descriptor_base64: String,
    /// Full SHA-256 digest of canonical descriptor bytes.
    pub descriptor_sha256: String,
    /// Independent 32-bit structural token, not a payload encoding allocation.
    pub native_type_token: u32,
}

impl NativeDescriptor {
    /// Exports one complete checked SDK representation.
    pub fn from_representation(value: &NativeRepresentation) -> Self {
        let bytes = value.canonical_bytes();
        Self {
            key: TypeKey {
                name: value.name().to_owned(),
                revision: value.revision(),
            },
            canonical_descriptor_base64: STANDARD.encode(&bytes),
            descriptor_sha256: digest(&bytes),
            native_type_token: value.token().as_u32(),
        }
    }

    /// Decodes and verifies bytes, type identity, full digest and short token.
    ///
    /// # Errors
    /// Rejects oversized/malformed/noncanonical Base64 and any descriptor/hash/token drift.
    pub fn representation(&self) -> Result<NativeRepresentation, ProfileToolError> {
        if self.canonical_descriptor_base64.len() > MAX_NATIVE_DESCRIPTOR_BYTES.div_ceil(3) * 4 {
            return Err(invalid("encoded descriptor exceeds input limit"));
        }
        let bytes = STANDARD
            .decode(&self.canonical_descriptor_base64)
            .map_err(|_| invalid("invalid descriptor Base64"))?;
        if STANDARD.encode(&bytes) != self.canonical_descriptor_base64
            || digest(&bytes) != self.descriptor_sha256
        {
            return Err(invalid("noncanonical descriptor or full digest mismatch"));
        }
        let value = NativeRepresentation::from_canonical_bytes(&bytes)?;
        if value.canonical_bytes() != bytes {
            return Err(invalid("descriptor round-trip is not canonical"));
        }
        if value.name() != self.key.name
            || value.revision() != self.key.revision
            || value.token().as_u32() != self.native_type_token
        {
            return Err(invalid("descriptor type identity or short token mismatch"));
        }
        Ok(value)
    }
}

/// Provisional target-qualified export from actual linked application registrations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryArtifact {
    /// Exact inventory schema identifier.
    pub schema: String,
    /// Provisional artifact format version.
    pub format_version: u32,
    /// Must remain true until a reviewed format specification replaces this profile.
    pub provisional: bool,
    /// Canonical descriptor version.
    pub descriptor_version: u32,
    /// Short-token derivation version.
    pub token_derivation_version: u32,
    /// Actual exporting executable target.
    pub target: Target,
    /// Sorted unique native type descriptors.
    pub types: Vec<NativeDescriptor>,
    /// Canonical content hash excluding this field.
    pub content_sha256: String,
}

impl InventoryArtifact {
    /// Exports linked concrete registrations and rejects missing required types.
    ///
    /// # Errors
    /// Rejects duplicate/bad registrations, omitted required types and invalid descriptors.
    pub fn from_linked(required: &[TypeKey]) -> Result<Self, ProfileToolError> {
        let representations = registered_native_types()?;
        let result = Self::from_representations(Target::current(), representations)?;
        for key in required {
            if !result.types.iter().any(|value| &value.key == key) {
                return Err(invalid(
                    "required application type is not linked/registered",
                ));
            }
        }
        Ok(result)
    }

    /// Builds an inventory from explicitly target-qualified descriptors.
    ///
    /// This imports metadata; it does not attest how a remote producer was built.
    /// # Errors
    /// Rejects mixed targets, duplicate keys/tokens and invalid canonical descriptors.
    pub fn from_representations(
        target: Target,
        values: impl IntoIterator<Item = NativeRepresentation>,
    ) -> Result<Self, ProfileToolError> {
        let mut types = values
            .into_iter()
            .map(|value| NativeDescriptor::from_representation(&value))
            .collect::<Vec<_>>();
        types.sort_by(|left, right| left.key.cmp(&right.key));
        let mut result = Self {
            schema: "uprotocol.native-inventory.provisional".to_owned(),
            format_version: FORMAT_VERSION,
            provisional: true,
            descriptor_version: NATIVE_DESCRIPTOR_VERSION,
            token_derivation_version: NATIVE_TOKEN_DERIVATION_VERSION,
            target,
            types,
            content_sha256: String::new(),
        };
        result.content_sha256 = content_hash(&result)?;
        result.validate()?;
        Ok(result)
    }

    /// Verifies schema/version, complete descriptors, target and deterministic ordering.
    ///
    /// # Errors
    /// Rejects old/unknown formats, mismatches, duplicate type keys or token collisions.
    pub fn validate(&self) -> Result<(), ProfileToolError> {
        if self.schema != "uprotocol.native-inventory.provisional"
            || self.format_version != FORMAT_VERSION
            || !self.provisional
            || self.descriptor_version != NATIVE_DESCRIPTOR_VERSION
            || self.token_derivation_version != NATIVE_TOKEN_DERIVATION_VERSION
        {
            return Err(invalid("unsupported inventory/descriptor/token version"));
        }
        self.target.validate()?;
        if self.types.len() > 4096 {
            return Err(invalid("inventory type limit exceeded"));
        }
        let mut keys = BTreeSet::new();
        let mut tokens = BTreeMap::new();
        let mut previous = None;
        for value in &self.types {
            let descriptor = value.representation()?;
            if !self.target.matches(&descriptor) {
                return Err(invalid(
                    "inventory target does not match canonical descriptor",
                ));
            }
            if !keys.insert(&value.key) {
                return Err(invalid("duplicate inventory key"));
            }
            if let Some(first) = tokens.insert(value.native_type_token, value.key.clone()) {
                return Err(ProfileToolError::TokenCollision {
                    token: value.native_type_token,
                    first,
                    second: value.key.clone(),
                });
            }
            if previous.is_some_and(|key| key >= &value.key) {
                return Err(invalid("inventory types are not in canonical order"));
            }
            previous = Some(&value.key);
        }
        check_hash(self, &self.content_sha256)
    }
}
