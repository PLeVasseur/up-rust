// SPDX-License-Identifier: Apache-2.0
//! Shared native encoding, immutable type history and separate identity declarations.
use crate::{
    check_hash, content_hash, invalid, valid_digest, InventoryArtifact, NativeDescriptor,
    ProfileToolError, Target, AGREEMENT_FORMAT_VERSION, FORMAT_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use up_rust::wire::WireIdentityNamespace;
use up_rust::{
    NativeProfile, PayloadEncoding, NATIVE_DESCRIPTOR_VERSION, NATIVE_TOKEN_DERIVATION_VERSION,
};

fn native_encoding(encoding_id: u16) -> Result<PayloadEncoding, ProfileToolError> {
    match encoding_id {
        0 => Ok(PayloadEncoding::CONTRACT_DEFINED),
        0xF000..=0xFFFE => Ok(PayloadEncoding::private_use(u32::from(encoding_id))),
        _ => Err(invalid(
            "native encoding must be private F000..FFFE or explicit zero",
        )),
    }
}

/// Whether a permanently reserved native identity is currently selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeTypeState {
    /// Available for this generation's native profile.
    Active,
    /// Retained for immutable identity and collision checks, not typed access.
    Retired,
}

/// Immutable native descriptor and current selection state, without a per-type encoding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeTypeEntry {
    /// Active or retired representation.
    pub state: NativeTypeState,
    /// Complete immutable native identity.
    pub descriptor: NativeDescriptor,
}

/// Non-native private payload reservation in the same payload-ID namespace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivatePayloadDeclaration {
    /// Private payload ID, distinct from the shared native encoding.
    pub encoding_id: u16,
    /// Deployment-stable representation meaning.
    pub meaning: String,
    /// Owner of that representation agreement.
    pub owner: String,
    /// Explicit nonzero declaration version.
    pub version: u32,
}

/// Namespace of a wire identity; none is a payload-encoding namespace.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireNamespace {
    /// Complete selected wire.
    Wire,
    /// Payload representation family.
    PayloadFamily,
    /// Frame metadata layout.
    MetadataLayout,
}

impl WireNamespace {
    fn sdk(self) -> WireIdentityNamespace {
        match self {
            Self::Wire => WireIdentityNamespace::Wire,
            Self::PayloadFamily => WireIdentityNamespace::PayloadFamily,
            Self::MetadataLayout => WireIdentityNamespace::MetadataLayout,
        }
    }
}

/// Permitted matching form for a declared wire identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireReference {
    /// Compact form, permitted only for an actual SDK registered pair.
    RegisteredCompact,
    /// Literal form required for every private or unregistered identity.
    Literal,
}

/// Separately namespaced literal/compact wire ownership declaration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireDeclaration {
    /// Selected-wire, family or metadata-layout namespace.
    pub namespace: WireNamespace,
    /// Full literal identity, required even for recorded registered pairs.
    pub literal: String,
    /// Compact reference or private alias; equality alone is not private identity.
    pub compact_id: u16,
    /// Owner of the agreed meaning.
    pub owner: String,
    /// Explicit nonzero declaration version.
    pub version: u32,
    /// Matching form allowed by this declaration and the SDK policy.
    pub reference: WireReference,
}

/// Optional declarations sharing the artifact without merging their namespaces.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityDeclarations {
    /// Additional reservations in the private payload encoding namespace.
    #[serde(default)]
    pub private_payloads: Vec<PrivatePayloadDeclaration>,
    /// Independent selected-wire/family/layout declarations.
    #[serde(default)]
    pub wires: Vec<WireDeclaration>,
}

impl IdentityDeclarations {
    /// Checks uniqueness and literal/registered matching using the SDK's own pairs.
    ///
    /// Private compact aliases may coincide when their literals differ; that never
    /// permits compact-only matching. Their literals remain separate identities.
    /// # Errors
    /// Rejects duplicates, missing ownership/literals, invalid IDs or false registration claims.
    pub fn validate(&self) -> Result<(), ProfileToolError> {
        if self.private_payloads.len() > 4096 || self.wires.len() > 4096 {
            return Err(invalid(
                "identity declaration count exceeds artifact limits",
            ));
        }
        let mut payload_ids = BTreeSet::new();
        for value in &self.private_payloads {
            if !(0xF000..=0xFFFE).contains(&value.encoding_id)
                || value.meaning.is_empty()
                || value.owner.is_empty()
                || value.version == 0
                || !payload_ids.insert(value.encoding_id)
            {
                return Err(invalid("invalid or duplicate private payload declaration"));
            }
        }
        let mut literals = BTreeSet::new();
        for value in &self.wires {
            if value.literal.is_empty()
                || value.literal.chars().count() > u16::MAX as usize
                || value.owner.is_empty()
                || value.version == 0
                || !literals.insert((value.namespace, &value.literal))
            {
                return Err(invalid("invalid or duplicate wire declaration"));
            }
            let registered = value
                .namespace
                .sdk()
                .registered_identities()
                .iter()
                .any(|known| {
                    known.literal_id() == value.literal && known.compact_id() == value.compact_id
                });
            if value.reference == WireReference::RegisteredCompact && !registered {
                return Err(invalid(
                    "private/unregistered wire identity requires a literal reference",
                ));
            }
        }
        Ok(())
    }

    fn canonicalize(&mut self) {
        self.private_payloads.sort_by_key(|value| value.encoding_id);
        self.wires.sort_by(|left, right| {
            (left.namespace, &left.literal).cmp(&(right.namespace, &right.literal))
        });
    }

    fn extends(&self, old: &Self) -> Result<(), ProfileToolError> {
        let payloads: BTreeMap<_, _> = self
            .private_payloads
            .iter()
            .map(|value| (value.encoding_id, value))
            .collect();
        let wires: BTreeMap<_, _> = self
            .wires
            .iter()
            .map(|value| ((value.namespace, &value.literal), value))
            .collect();
        if old
            .private_payloads
            .iter()
            .any(|value| payloads.get(&value.encoding_id).copied() != Some(value))
            || old
                .wires
                .iter()
                .any(|value| wires.get(&(value.namespace, &value.literal)).copied() != Some(value))
        {
            return Err(invalid(
                "existing identity declarations cannot be removed or redefined",
            ));
        }
        Ok(())
    }
}

/// One explicit encoding and the complete active/retired representation catalogue.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCatalogue {
    /// Encoding shared by every active type: private F000..FFFE or explicit zero.
    pub encoding_id: u16,
    /// Immutable descriptors, including retired identities, ordered by name/revision.
    pub types: Vec<NativeTypeEntry>,
}

/// Provisional immutable generation containing both identity namespaces.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgreementArtifact {
    /// Exact agreement schema identifier.
    pub schema: String,
    /// Provisional JSON format version.
    pub format_version: u32,
    /// Must remain true for this reference implementation's file format.
    pub provisional: bool,
    /// Canonical descriptor version.
    pub descriptor_version: u32,
    /// Structural token derivation version.
    pub token_derivation_version: u32,
    /// Target qualification shared by all native entries.
    pub target: Target,
    /// Deployment domain.
    pub domain: String,
    /// Nonzero immutable generation.
    pub generation: u32,
    /// Content identity of the immediately preceding agreement, if any.
    pub previous_content_sha256: Option<String>,
    /// Content identity of the selected actual inventory.
    pub inventory_sha256: String,
    /// Shared native encoding and immutable descriptor catalogue.
    pub native: NativeCatalogue,
    /// Separate private payload and wire/family/layout declarations.
    #[serde(default)]
    pub identities: IdentityDeclarations,
    /// Full canonical content hash excluding this field.
    pub content_sha256: String,
}

impl AgreementArtifact {
    /// Checks the selected wire's identity pairs against the provisioned declarations.
    ///
    /// The wire `W` supplies the wire and payload-family identities and the
    /// metadata codec `C` supplies the metadata-layout identity, mirroring how
    /// the SDK builds a metadata context. Registered first-wave pairs may be
    /// omitted from the optional declaration section. Every private/unregistered
    /// pair must have an exact literal entry. Wire-format compatibility remains
    /// the SDK metadata decoder's responsibility.
    /// # Errors
    /// Rejects undeclared private identities and mismatched literal/compact pairs.
    pub fn require_wire<W: up_rust::UWire, C: up_rust::UWireMetadataCodec>(
        &self,
    ) -> Result<(), ProfileToolError> {
        self.require_context(up_rust::UWireMetadataContext::from_wire::<W, C>())
    }

    /// Admits every identity in an explicit serialized envelope/codec context.
    /// New private full-wire identities need real immutable declarations; this
    /// method neither allocates identities nor repurposes an existing preset.
    /// # Errors
    /// Refuses undeclared/mismatched pairs and invalid agreement artifacts.
    pub fn require_context(
        &self,
        context: up_rust::UWireMetadataContext,
    ) -> Result<(), ProfileToolError> {
        self.validate()?;
        for (namespace, expected) in [
            (WireNamespace::Wire, context.wire_id),
            (WireNamespace::PayloadFamily, context.payload_family_id),
            (WireNamespace::MetadataLayout, context.metadata_layout_id),
        ] {
            match self.identities.wires.iter().find(|value| {
                value.namespace == namespace && value.literal == expected.literal_id()
            }) {
                Some(value) if value.compact_id == expected.compact_id() => {}
                Some(_) => {
                    return Err(invalid(
                        "declared wire pair differs from the selected SDK wire",
                    ))
                }
                None if namespace.sdk().registered_identities().contains(&expected) => {}
                None => {
                    return Err(invalid(
                        "selected private wire identity has no provisioned literal declaration",
                    ))
                }
            }
        }
        Ok(())
    }

    fn build(
        inventory: &InventoryArtifact,
        domain: String,
        generation: u32,
        previous: Option<String>,
        native: NativeCatalogue,
        mut identities: IdentityDeclarations,
    ) -> Result<Self, ProfileToolError> {
        identities.canonicalize();
        let mut value = Self {
            schema: "uprotocol.deployment-agreement.provisional".to_owned(),
            format_version: AGREEMENT_FORMAT_VERSION,
            provisional: true,
            descriptor_version: NATIVE_DESCRIPTOR_VERSION,
            token_derivation_version: NATIVE_TOKEN_DERIVATION_VERSION,
            target: inventory.target.clone(),
            domain,
            generation,
            previous_content_sha256: previous,
            inventory_sha256: inventory.content_sha256.clone(),
            native,
            identities,
            content_sha256: String::new(),
        };
        value.content_sha256 = content_hash(&value)?;
        value.validate()?;
        Ok(value)
    }

    /// Generates a shared-encoding catalogue from the actual target inventory.
    ///
    /// Retired entries remain reserved. Reintroducing the identical name/revision/
    /// descriptor reactivates it; a changed descriptor under that key fails.
    /// # Errors
    /// Rejects invalid inventories/chains/encodings, changed identities or token collisions.
    pub fn generate(
        inventory: &InventoryArtifact,
        domain: String,
        generation: u32,
        encoding_id: u16,
        previous: Option<&Self>,
        identities: IdentityDeclarations,
    ) -> Result<Self, ProfileToolError> {
        inventory.validate()?;
        native_encoding(encoding_id)?;
        identities.validate()?;
        let mut types: BTreeMap<_, _> = if let Some(old) = previous {
            old.validate()?;
            if old.domain != domain
                || old.target != inventory.target
                || old.generation.checked_add(1) != Some(generation)
                || old.native.encoding_id != encoding_id
            {
                return Err(invalid("successor must preserve domain/target/encoding and advance exactly one generation"));
            }
            identities.extends(&old.identities)?;
            old.native
                .types
                .iter()
                .map(|entry| (entry.descriptor.key.clone(), entry.clone()))
                .collect()
        } else {
            if generation != 1 {
                return Err(invalid("genesis must use generation 1"));
            }
            BTreeMap::new()
        };
        for value in types.values_mut() {
            value.state = NativeTypeState::Retired;
        }
        for descriptor in &inventory.types {
            if let Some(existing) = types.get_mut(&descriptor.key) {
                if existing.descriptor != *descriptor {
                    return Err(invalid(
                        "descriptor changed under an existing name/revision",
                    ));
                }
                existing.state = NativeTypeState::Active;
                continue;
            }
            types.insert(
                descriptor.key.clone(),
                NativeTypeEntry {
                    state: NativeTypeState::Active,
                    descriptor: descriptor.clone(),
                },
            );
        }
        Self::build(
            inventory,
            domain,
            generation,
            previous.map(|value| value.content_sha256.clone()),
            NativeCatalogue {
                encoding_id,
                types: types.into_values().collect(),
            },
            identities,
        )
    }

    /// Verifies complete content, namespaces, descriptors and the active inventory binding.
    ///
    /// # Errors
    /// Rejects old/unknown versions, corrupt content, encoding conflicts or duplicate keys/tokens.
    pub fn validate(&self) -> Result<(), ProfileToolError> {
        crate::check_version(u64::from(self.format_version), AGREEMENT_FORMAT_VERSION)?;
        if self.schema != "uprotocol.deployment-agreement.provisional"
            || !self.provisional
            || self.descriptor_version != NATIVE_DESCRIPTOR_VERSION
            || self.token_derivation_version != NATIVE_TOKEN_DERIVATION_VERSION
        {
            return Err(invalid("unsupported agreement/descriptor/token version"));
        }
        self.target.validate()?;
        if self.domain.is_empty()
            || self.generation == 0
            || !valid_digest(&self.inventory_sha256)
            || self
                .previous_content_sha256
                .as_ref()
                .is_some_and(|hash| !valid_digest(hash))
        {
            return Err(invalid("invalid domain/generation/hash metadata"));
        }
        self.identities.validate()?;
        native_encoding(self.native.encoding_id)?;
        if self
            .identities
            .private_payloads
            .iter()
            .any(|value| value.encoding_id == self.native.encoding_id)
        {
            return Err(invalid(
                "native encoding conflicts with a private payload declaration",
            ));
        }
        if (self.generation == 1) != self.previous_content_sha256.is_none() {
            return Err(invalid(
                "agreement generation does not match genesis/successor hash state",
            ));
        }
        if self.native.types.len() > 4096 {
            return Err(invalid("native catalogue type limit exceeded"));
        }
        let mut tokens = BTreeMap::new();
        let mut previous = None;
        for value in &self.native.types {
            if previous.is_some_and(|key| key >= &value.descriptor.key) {
                return Err(invalid(
                    "duplicate native key or noncanonical catalogue order",
                ));
            }
            if let Some(first) = tokens.insert(
                value.descriptor.native_type_token,
                value.descriptor.key.clone(),
            ) {
                return Err(ProfileToolError::TokenCollision {
                    token: value.descriptor.native_type_token,
                    first,
                    second: value.descriptor.key.clone(),
                });
            }
            if !self.target.matches(&value.descriptor.representation()?) {
                return Err(invalid("native catalogue target mismatch"));
            }
            previous = Some(&value.descriptor.key);
        }
        let inventory = InventoryArtifact {
            schema: "uprotocol.native-inventory.provisional".to_owned(),
            format_version: FORMAT_VERSION,
            provisional: true,
            descriptor_version: self.descriptor_version,
            token_derivation_version: self.token_derivation_version,
            target: self.target.clone(),
            types: self
                .native
                .types
                .iter()
                .filter(|value| value.state == NativeTypeState::Active)
                .map(|value| value.descriptor.clone())
                .collect(),
            content_sha256: String::new(),
        };
        if content_hash(&inventory)? != self.inventory_sha256 {
            return Err(invalid(
                "active catalogue does not match the inventory hash",
            ));
        }
        check_hash(self, &self.content_sha256)
    }

    /// Checks immutable catalogue history against the supplied previous agreement.
    ///
    /// # Errors
    /// Rejects wrong hashes/generations/encodings, lost identities or descriptor redefinition.
    pub fn validate_successor(&self, previous: &Self) -> Result<(), ProfileToolError> {
        self.validate()?;
        previous.validate()?;
        if self.previous_content_sha256.as_deref() != Some(previous.content_sha256.as_str())
            || previous.generation.checked_add(1) != Some(self.generation)
            || self.domain != previous.domain
            || self.target != previous.target
            || self.native.encoding_id != previous.native.encoding_id
        {
            return Err(invalid(
                "invalid previous-agreement hash/generation/domain/target/encoding chain",
            ));
        }
        self.identities.extends(&previous.identities)?;
        let by_key: BTreeMap<_, _> = self
            .native
            .types
            .iter()
            .map(|value| (&value.descriptor.key, &value.descriptor))
            .collect();
        for entry in &previous.native.types {
            if by_key.get(&entry.descriptor.key).copied() != Some(&entry.descriptor) {
                return Err(invalid("old native identity removed or redefined"));
            }
        }
        Ok(())
    }

    /// Builds the SDK profile after validating the complete artifact.
    ///
    /// # Errors
    /// Rejects invalid artifact data or SDK profile invariants.
    pub fn profile(&self) -> Result<NativeProfile, ProfileToolError> {
        self.validate()?;
        let representations = self
            .native
            .types
            .iter()
            .filter(|value| value.state == NativeTypeState::Active)
            .map(|value| value.descriptor.representation())
            .collect::<Result<Vec<_>, ProfileToolError>>()?;
        Ok(NativeProfile::new(
            self.domain.clone(),
            self.generation,
            native_encoding(self.native.encoding_id)?,
            representations,
        )?)
    }
}
