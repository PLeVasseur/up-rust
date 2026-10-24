// SPDX-License-Identifier: Apache-2.0
//! Append-only native allocation and separate private identity declarations.
use crate::{
    check_hash, content_hash, invalid, valid_digest, InventoryArtifact, NativeDescriptor,
    ProfileToolError, Target, TypeKey, FORMAT_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use up_rust::wire::WireIdentityNamespace;
use up_rust::{
    NativeProfile, NativeProfileMode, NativeProfileTable, NativeSingleType, PayloadEncoding,
    NATIVE_DESCRIPTOR_VERSION, NATIVE_TOKEN_DERIVATION_VERSION,
};

/// Inclusive deployment-private native allocation band.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocationBand {
    /// First allowed private payload ID.
    pub first: u16,
    /// Last allowed private payload ID.
    pub last: u16,
}

impl Default for AllocationBand {
    fn default() -> Self {
        Self {
            first: 0xF100,
            last: 0xF7FF,
        }
    }
}

impl AllocationBand {
    fn validate(self) -> Result<(), ProfileToolError> {
        if self.first < 0xF000 || self.last > 0xFFFE || self.first > self.last {
            return Err(invalid("invalid private allocation band"));
        }
        Ok(())
    }
    fn contains(self, id: u16) -> bool {
        (self.first..=self.last).contains(&id)
    }
}

/// Whether a permanently reserved native identity is currently selected.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationState {
    /// Available for this generation's native profile.
    Active,
    /// Reserved for its original identity, unavailable for reassignment.
    Retired,
}

/// Immutable native payload-ID assignment and current selection state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeAllocation {
    /// Deployment-private payload encoding ID.
    pub encoding_id: u16,
    /// Active or retired reservation.
    pub state: AllocationState,
    /// Complete immutable native identity.
    pub descriptor: NativeDescriptor,
}

/// Explicit genesis assignment for an already coordinated native type.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedAllocation {
    /// Native type selected from the actual inventory.
    pub key: TypeKey,
    /// Existing private payload encoding ID to preserve.
    pub encoding_id: u16,
}

/// Non-native private payload reservation in the same payload-ID namespace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivatePayloadDeclaration {
    /// Private payload ID, distinct from all native assignments including tombstones.
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

/// Explicit native agreement mode; no implicit table-to-ID 0 fallback exists.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum NativeMode {
    /// Private-ID table with permanent reservations.
    Table {
        /// Inclusive allocation policy for this table.
        allocation_band: AllocationBand,
        /// All assignments, including retired identities, ordered by encoding ID.
        allocations: Vec<NativeAllocation>,
    },
    /// One exact native representation explicitly selected for payload ID 0.
    SingleType {
        /// Full representation agreed for this deployment; no operation binding.
        descriptor: NativeDescriptor,
    },
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
    /// Content identity of the immediately preceding table, if any.
    pub previous_content_sha256: Option<String>,
    /// Content identity of the selected actual inventory.
    pub inventory_sha256: String,
    /// Explicit native table or single-type agreement.
    pub native: NativeMode,
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
        self.validate()?;
        for (namespace, expected) in [
            (WireNamespace::Wire, W::WIRE_ID),
            (WireNamespace::PayloadFamily, W::PAYLOAD_FAMILY_ID),
            (WireNamespace::MetadataLayout, C::METADATA_LAYOUT_ID),
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
        native: NativeMode,
        mut identities: IdentityDeclarations,
    ) -> Result<Self, ProfileToolError> {
        identities.canonicalize();
        let mut value = Self {
            schema: "uprotocol.deployment-agreement.provisional".to_owned(),
            format_version: FORMAT_VERSION,
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

    /// Generates an append-only table from actual inventory and an optional prior ledger.
    ///
    /// Retired entries remain reserved. Reintroducing the identical name/revision/
    /// descriptor reactivates its original ID; a changed descriptor under that key fails.
    /// # Errors
    /// Rejects invalid inventories/chains, changed identities, collisions, bad seeds or exhaustion.
    pub fn generate_table(
        inventory: &InventoryArtifact,
        domain: String,
        generation: u32,
        band: AllocationBand,
        previous: Option<&Self>,
        seeds: &[SeedAllocation],
        identities: IdentityDeclarations,
    ) -> Result<Self, ProfileToolError> {
        inventory.validate()?;
        band.validate()?;
        identities.validate()?;
        let mut allocations = if let Some(old) = previous {
            old.validate()?;
            if !seeds.is_empty() {
                return Err(invalid("seeds cannot be supplied with a previous ledger"));
            }
            if old.domain != domain
                || old.target != inventory.target
                || old.generation.checked_add(1) != Some(generation)
            {
                return Err(invalid("table successor must preserve domain/target and advance exactly one generation"));
            }
            identities.extends(&old.identities)?;
            match &old.native {
                NativeMode::Table {
                    allocation_band,
                    allocations,
                } if *allocation_band == band => allocations.clone(),
                NativeMode::Table { .. } => {
                    return Err(invalid(
                        "the successor must keep the table band; a band change needs a new deployment profile",
                    ))
                }
                NativeMode::SingleType { .. } => {
                    return Err(invalid("a table successor requires a table predecessor"));
                }
            }
        } else {
            if generation != 1 {
                return Err(invalid("genesis must use generation 1"));
            }
            let mut keys = BTreeSet::new();
            let mut ids = BTreeSet::new();
            let mut values = Vec::new();
            for seed in seeds {
                if !keys.insert(&seed.key)
                    || !ids.insert(seed.encoding_id)
                    || !band.contains(seed.encoding_id)
                {
                    return Err(invalid("duplicate or out-of-band genesis seed"));
                }
                let descriptor = inventory
                    .types
                    .iter()
                    .find(|value| value.key == seed.key)
                    .ok_or_else(|| invalid("seed type absent from actual inventory"))?
                    .clone();
                values.push(NativeAllocation {
                    encoding_id: seed.encoding_id,
                    state: AllocationState::Retired,
                    descriptor,
                });
            }
            values
        };
        for value in &mut allocations {
            value.state = AllocationState::Retired;
        }
        let reserved: BTreeSet<u16> = identities
            .private_payloads
            .iter()
            .map(|value| value.encoding_id)
            .collect();
        let mut next = allocations
            .iter()
            .map(|value| u32::from(value.encoding_id) + 1)
            .max()
            .unwrap_or(u32::from(band.first));
        for descriptor in &inventory.types {
            if let Some(existing) = allocations
                .iter_mut()
                .find(|value| value.descriptor.key == descriptor.key)
            {
                if existing.descriptor != *descriptor {
                    return Err(invalid("descriptor changed under an existing assignment"));
                }
                existing.state = AllocationState::Active;
                continue;
            }
            if let Some(first) = allocations
                .iter()
                .find(|value| value.descriptor.native_type_token == descriptor.native_type_token)
            {
                return Err(ProfileToolError::TokenCollision {
                    token: descriptor.native_type_token,
                    first: first.descriptor.key.clone(),
                    second: descriptor.key.clone(),
                });
            }
            while next <= u32::from(band.last) && reserved.contains(&(next as u16)) {
                next += 1;
            }
            if next > u32::from(band.last) {
                return Err(invalid("native allocation band exhausted"));
            }
            let encoding_id =
                u16::try_from(next).map_err(|_| invalid("native allocation ID overflow"))?;
            allocations.push(NativeAllocation {
                encoding_id,
                state: AllocationState::Active,
                descriptor: descriptor.clone(),
            });
            next += 1;
        }
        allocations.sort_by_key(|value| value.encoding_id);
        Self::build(
            inventory,
            domain,
            generation,
            previous.map(|value| value.content_sha256.clone()),
            NativeMode::Table {
                allocation_band: band,
                allocations,
            },
            identities,
        )
    }

    /// Generates an explicit ID 0 agreement for one inventoried representation.
    ///
    /// # Errors
    /// Rejects a missing type or invalid inventory/declarations.
    pub fn generate_single_type(
        inventory: &InventoryArtifact,
        domain: String,
        key: &TypeKey,
        identities: IdentityDeclarations,
    ) -> Result<Self, ProfileToolError> {
        inventory.validate()?;
        let descriptor = inventory
            .types
            .iter()
            .find(|value| &value.key == key)
            .ok_or_else(|| invalid("single type absent from inventory"))?
            .clone();
        Self::build(
            inventory,
            domain,
            1,
            None,
            NativeMode::SingleType { descriptor },
            identities,
        )
    }

    /// Verifies complete content, namespace rules, descriptor/target identities and reservations.
    ///
    /// # Errors
    /// Rejects old/unknown versions, corrupt content, duplicate/reused IDs or token collisions.
    pub fn validate(&self) -> Result<(), ProfileToolError> {
        crate::check_format_version(u64::from(self.format_version))?;
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
        match &self.native {
            NativeMode::Table {
                allocation_band,
                allocations,
            } => {
                allocation_band.validate()?;
                if (self.generation == 1) != self.previous_content_sha256.is_none() {
                    return Err(invalid(
                        "table generation does not match genesis/successor hash state",
                    ));
                }
                if allocations.len() > 4096 {
                    return Err(invalid("native allocation count exceeds private namespace"));
                }
                let mut ids: BTreeSet<_> = self
                    .identities
                    .private_payloads
                    .iter()
                    .map(|value| value.encoding_id)
                    .collect();
                let mut keys = BTreeSet::new();
                let mut tokens = BTreeMap::new();
                let mut previous = None;
                for value in allocations {
                    if !allocation_band.contains(value.encoding_id)
                        || !ids.insert(value.encoding_id)
                        || !keys.insert(&value.descriptor.key)
                        || previous.is_some_and(|id| id >= value.encoding_id)
                    {
                        return Err(invalid(
                            "invalid/reused native ID, duplicate key/token or noncanonical order",
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
                        return Err(invalid("native assignment target mismatch"));
                    }
                    previous = Some(value.encoding_id);
                }
            }
            NativeMode::SingleType { descriptor } => {
                if self.previous_content_sha256.is_some()
                    || !self.target.matches(&descriptor.representation()?)
                {
                    return Err(invalid("invalid explicit single-type agreement"));
                }
            }
        }
        check_hash(self, &self.content_sha256)
    }

    /// Checks that this table is an immutable-assignment successor of a supplied ledger.
    ///
    /// # Errors
    /// Rejects wrong hashes/generations, lost reservations or changed prior assignments.
    pub fn validate_successor(&self, previous: &Self) -> Result<(), ProfileToolError> {
        self.validate()?;
        previous.validate()?;
        if self.previous_content_sha256.as_deref() != Some(previous.content_sha256.as_str())
            || previous.generation.checked_add(1) != Some(self.generation)
            || self.domain != previous.domain
            || self.target != previous.target
        {
            return Err(invalid("invalid previous-ledger hash/generation chain"));
        }
        self.identities.extends(&previous.identities)?;
        match (&previous.native, &self.native) {
            (
                NativeMode::Table {
                    allocation_band: old_band,
                    allocations: old,
                },
                NativeMode::Table {
                    allocation_band: new_band,
                    allocations: new,
                },
            ) if old_band == new_band => {
                let by_id: BTreeMap<_, _> =
                    new.iter().map(|value| (value.encoding_id, value)).collect();
                let old_ids: BTreeSet<_> = old.iter().map(|value| value.encoding_id).collect();
                let high_water = old_ids.last().copied();
                if new.iter().any(|value| {
                    !old_ids.contains(&value.encoding_id)
                        && high_water.is_some_and(|last| value.encoding_id <= last)
                }) {
                    return Err(invalid(
                        "new assignments must append after the prior high-water mark",
                    ));
                }
                for entry in old {
                    if !by_id
                        .get(&entry.encoding_id)
                        .is_some_and(|next| next.descriptor == entry.descriptor)
                    {
                        return Err(invalid("old reservation removed or reassigned"));
                    }
                }
            }
            (NativeMode::Table { .. }, NativeMode::Table { .. }) => return Err(invalid(
                "the successor must keep the table band; a band change needs a new deployment profile")),
            _ => return Err(invalid("unsupported agreement mode migration")),
        }
        Ok(())
    }

    /// Builds the SDK profile after validating the complete artifact.
    ///
    /// # Errors
    /// Rejects invalid artifact data or SDK profile invariants.
    pub fn profile(&self) -> Result<NativeProfile, ProfileToolError> {
        self.validate()?;
        let mode = match &self.native {
            NativeMode::Table { allocations, .. } => {
                let entries = allocations
                    .iter()
                    .filter(|value| value.state == AllocationState::Active)
                    .map(|value| {
                        Ok((
                            PayloadEncoding::private_use(u32::from(value.encoding_id)),
                            value.descriptor.representation()?,
                        ))
                    })
                    .collect::<Result<Vec<_>, ProfileToolError>>()?;
                NativeProfileMode::Table(NativeProfileTable::new(entries)?)
            }
            NativeMode::SingleType { descriptor } => {
                NativeProfileMode::SingleType(NativeSingleType::new(descriptor.representation()?))
            }
        };
        Ok(NativeProfile::new(
            self.domain.clone(),
            self.generation,
            mode,
        )?)
    }
}
