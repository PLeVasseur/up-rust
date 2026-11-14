// SPDX-License-Identifier: Apache-2.0
//! Explicit serialized combinations, independent of a native type inventory.
use crate::{IdentityDeclarations, ProfileToolError, WireDeclaration, WireNamespace};
use serde::{Deserialize, Serialize};
use up_rust::{UWireMetadataContext, WireIdentity};

/// Deployment-private declaration of one complete serialized context. The wire
/// literal names this exact family/layout/version combination. This is a local
/// provisioning artifact, not a registry allocation or discovery protocol.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnedContextDeclaration {
    /// Exact provisional schema identifier.
    pub schema: String,
    /// Explicit full-wire ownership and identity.
    pub wire: WireDeclaration,
    /// Actual payload codec operation family, separately namespaced.
    pub payload_family: WireDeclaration,
    /// Metadata codec's format identity, separately namespaced.
    pub metadata_layout: WireDeclaration,
    /// SDK format version used for encoded metadata.
    pub format_version: u16,
}
fn matches(actual: &WireDeclaration, expected: WireIdentity) -> bool {
    actual.literal == expected.literal_id() && actual.compact_id == expected.compact_id()
}
impl OwnedContextDeclaration {
    /// Checks identity policy and rejects redefinition of existing SDK presets.
    /// # Errors
    /// Rejects false registration, invalid namespaces, missing owners, unsupported
    /// format, native contexts and relabelled preset payload families.
    pub fn validate(&self) -> Result<(), ProfileToolError> {
        let invalid = || ProfileToolError::Invalid("invalid serialized context declaration");
        if self.schema != "uprotocol.owned-context.provisional.v1"
            || self.format_version != up_rust::wire::FORMAT_VERSION
            || self.wire.namespace != WireNamespace::Wire
            || self.payload_family.namespace != WireNamespace::PayloadFamily
            || self.metadata_layout.namespace != WireNamespace::MetadataLayout
            || self.wire.literal == "org.eclipse.uprotocol.wire.stable-container"
        {
            return Err(invalid());
        }
        IdentityDeclarations {
            private_payloads: Vec::new(),
            wires: vec![
                self.wire.clone(),
                self.payload_family.clone(),
                self.metadata_layout.clone(),
            ],
        }
        .validate()?;
        for (wire, family) in [
            (
                up_rust::UPROTOCOL_NATIVE_WIRE_ID,
                up_rust::NATIVE_EXPLICIT_PAYLOAD_FAMILY_ID,
            ),
            (
                up_rust::PROTOBUF_WIRE_ID,
                up_rust::PROTOBUF_PAYLOAD_FAMILY_ID,
            ),
            (up_rust::XCDR_V2_WIRE_ID, up_rust::XCDR_V2_PAYLOAD_FAMILY_ID),
        ] {
            if self.wire.literal == wire.literal_id()
                && (!matches(&self.wire, wire) || !matches(&self.payload_family, family))
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
    /// Checks an application's static context against its actual provisioned file.
    /// No dynamic string is leaked to manufacture a static SDK identity.
    /// # Errors
    /// Rejects any context difference or invalid declaration.
    pub fn require_context(&self, context: UWireMetadataContext) -> Result<(), ProfileToolError> {
        self.validate()?;
        if !matches(&self.wire, context.wire_id)
            || !matches(&self.payload_family, context.payload_family_id)
            || !matches(&self.metadata_layout, context.metadata_layout_id)
            || self.format_version != context.format_version
        {
            return Err(ProfileToolError::Invalid(
                "application context differs from provisioned declaration",
            ));
        }
        Ok(())
    }
    /// Admits independently provisioned peer declarations without negotiation.
    /// # Errors
    /// Rejects any difference in identities, owners, versions or byte format.
    pub fn agree(&self, peer: &Self) -> Result<(), ProfileToolError> {
        self.validate()?;
        peer.validate()?;
        if self != peer {
            return Err(ProfileToolError::Invalid("peer serialized context differs"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn json() -> OwnedContextDeclaration {
        serde_json::from_str(include_str!("../tests/fixtures/door-json.json")).unwrap()
    }
    #[test]
    fn actual_fixture_declaration_and_private_literal_policy() {
        let local = json();
        let peer = json();
        local.agree(&peer).unwrap();
        let context = UWireMetadataContext::new_exact(
            WireIdentity::new("example.vehicle.door.fields-json-v1", 0xF102),
            WireIdentity::new("example.vehicle.door.json-v1", 0xF101),
            up_rust::UFRAME_FIELDS_METADATA_LAYOUT_ID,
            up_rust::wire::FORMAT_VERSION,
        );
        local.require_context(context).unwrap();
        let mut wrong = local.clone();
        wrong.wire.reference = crate::WireReference::RegisteredCompact;
        assert!(wrong.validate().is_err());
        let mut wrong = local.clone();
        wrong.payload_family.literal.push_str("-other");
        assert!(local.agree(&wrong).is_err());
        assert!(wrong.require_context(context).is_err());
        let mut wrong = local;
        wrong.wire.literal = up_rust::PROTOBUF_WIRE_ID.literal_id().into();
        wrong.wire.compact_id = up_rust::PROTOBUF_WIRE_ID.compact_id();
        assert!(wrong.validate().is_err());
    }
}
