// SPDX-License-Identifier: Apache-2.0
//! Independently provisioned endpoint/route membership and artifact admission.
use crate::{
    check_hash, content_hash, invalid, io_error, read_artifact, valid_digest, AgreementArtifact,
    AllocationState, NativeMode, ProfileToolError, Target, TypeKey, FORMAT_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};
use std::sync::Arc;
use up_rust::{native_inventory::registered_native_types, NativeProfile, NativeProfileAgreement};

/// Independently provisioned expected artifact identity for one endpoint.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentEndpoint {
    /// Stable endpoint identity used in declared routes and startup receipts.
    pub id: String,
    /// Relative locator beneath the configured deployment profile root.
    pub agreement_path: String,
    /// Expected full canonical agreement content hash.
    pub content_sha256: String,
    /// Expected native representation target, not an inferred host layout.
    pub target: Target,
    /// Expected deployment domain.
    pub domain: String,
    /// Expected immutable generation.
    pub generation: u32,
}

/// Complete endpoint membership of one native agreement route, including direct peers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentRoute {
    /// Stable route identity.
    pub id: String,
    /// Every endpoint participating in the declared route.
    pub endpoints: Vec<String>,
}

/// Provisional independently provisioned deployment admission manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentManifest {
    /// Exact manifest schema identifier.
    pub schema: String,
    /// Provisional format version.
    pub format_version: u32,
    /// Must be true for this reference format.
    pub provisional: bool,
    /// Explicit endpoint expectations; no missing peer is synthesized locally.
    pub endpoints: Vec<DeploymentEndpoint>,
    /// Complete route membership.
    pub routes: Vec<DeploymentRoute>,
    /// Canonical content hash excluding this field.
    pub content_sha256: String,
}

impl DeploymentManifest {
    /// Constructs and hashes independently provisioned endpoint/route expectations.
    ///
    /// # Errors
    /// Rejects duplicate/absent endpoints, invalid locators, incomplete routes or identities.
    pub fn new(
        mut endpoints: Vec<DeploymentEndpoint>,
        mut routes: Vec<DeploymentRoute>,
    ) -> Result<Self, ProfileToolError> {
        endpoints.sort_by(|left, right| left.id.cmp(&right.id));
        routes.sort_by(|left, right| left.id.cmp(&right.id));
        let mut value = Self {
            schema: "uprotocol.native-deployment.provisional".to_owned(),
            format_version: FORMAT_VERSION,
            provisional: true,
            endpoints,
            routes,
            content_sha256: String::new(),
        };
        value.content_sha256 = content_hash(&value)?;
        value.validate()?;
        Ok(value)
    }

    /// Checks manifest structure, complete membership and its canonical hash.
    ///
    /// # Errors
    /// Rejects unknown formats, missing or duplicated peers, invalid paths and hash drift.
    pub fn validate(&self) -> Result<(), ProfileToolError> {
        if self.schema != "uprotocol.native-deployment.provisional"
            || self.format_version != FORMAT_VERSION
            || !self.provisional
        {
            return Err(invalid("unsupported deployment manifest version"));
        }
        if self.endpoints.len() < 2
            || self.endpoints.len() > 4096
            || self.routes.is_empty()
            || self.routes.len() > 4096
        {
            return Err(invalid("deployment membership limit exceeded"));
        }
        let mut endpoints = BTreeSet::new();
        for endpoint in &self.endpoints {
            if endpoint.id.is_empty()
                || !endpoints.insert(&endpoint.id)
                || endpoint.domain.is_empty()
                || endpoint.generation == 0
                || !valid_digest(&endpoint.content_sha256)
            {
                return Err(invalid("invalid or duplicate endpoint expectation"));
            }
            endpoint.target.validate()?;
            let path = Path::new(&endpoint.agreement_path);
            if endpoint.agreement_path.is_empty()
                || path.is_absolute()
                || path
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_)))
            {
                return Err(invalid(
                    "agreement locator must stay beneath the deployment root",
                ));
            }
        }
        let mut routes = BTreeSet::new();
        for route in &self.routes {
            if route.id.is_empty()
                || !routes.insert(&route.id)
                || route.endpoints.len() < 2
                || route.endpoints.len() > 4096
            {
                return Err(invalid(
                    "route must declare its complete distinct peer membership",
                ));
            }
            let mut members = BTreeSet::new();
            for member in &route.endpoints {
                if !endpoints.contains(member) || !members.insert(member) {
                    return Err(invalid("missing or duplicate route endpoint"));
                }
            }
        }
        check_hash(self, &self.content_sha256)
    }
}

#[derive(Debug)]
struct LoadedEndpoint {
    artifact: AgreementArtifact,
    profile: Arc<NativeProfile>,
}

/// Immutable loaded deployment generation checked against provisioned expectations.
#[derive(Debug)]
pub struct LoadedDeployment {
    manifest: DeploymentManifest,
    endpoints: BTreeMap<String, LoadedEndpoint>,
}

impl LoadedDeployment {
    /// Loads all independently named endpoint artifacts and checks every route before admission.
    ///
    /// Expected identity comes from the supplied manifest, not the loaded local table.
    /// File locators are confined to `profile_root`, including resolved symlinks.
    /// # Errors
    /// Rejects absent/half-configured peers, target/domain/generation/hash disagreement,
    /// path escapes, incompatible route content and invalid artifact data.
    pub fn load(
        profile_root: &Path,
        manifest: DeploymentManifest,
    ) -> Result<Self, ProfileToolError> {
        manifest.validate()?;
        let root = profile_root
            .canonicalize()
            .map_err(|error| io_error(profile_root, error))?;
        let mut endpoints = BTreeMap::new();
        for expected in &manifest.endpoints {
            let requested_path = root.join(&expected.agreement_path);
            let path = requested_path
                .canonicalize()
                .map_err(|error| io_error(&requested_path, error))?;
            if !path.starts_with(&root) {
                return Err(invalid("agreement symlink escapes deployment root"));
            }
            let artifact: AgreementArtifact = read_artifact(&path)?;
            artifact.validate()?;
            if artifact.content_sha256 != expected.content_sha256
                || artifact.target != expected.target
                || artifact.domain != expected.domain
                || artifact.generation != expected.generation
            {
                return Err(invalid(
                    "loaded agreement differs from independently provisioned expectation",
                ));
            }
            let profile = Arc::new(artifact.profile()?);
            endpoints.insert(expected.id.clone(), LoadedEndpoint { artifact, profile });
        }
        for route in &manifest.routes {
            let mut expected_hash = None;
            for id in &route.endpoints {
                let value = endpoints
                    .get(id)
                    .ok_or_else(|| invalid("route endpoint was not loaded"))?;
                if expected_hash.is_some_and(|hash| hash != value.artifact.content_sha256) {
                    return Err(invalid(
                        "declared route endpoints disagree on full agreement content",
                    ));
                }
                expected_hash = Some(value.artifact.content_sha256.as_str());
            }
        }
        Ok(Self {
            manifest,
            endpoints,
        })
    }

    /// Returns the retained independent manifest for startup identity reporting.
    pub fn manifest(&self) -> &DeploymentManifest {
        &self.manifest
    }

    /// Returns a checked endpoint artifact for startup hash/target/domain/generation reporting.
    ///
    /// # Errors
    /// Rejects an endpoint that was not independently provisioned and loaded.
    pub fn artifact(&self, endpoint: &str) -> Result<&AgreementArtifact, ProfileToolError> {
        Ok(&self
            .endpoints
            .get(endpoint)
            .ok_or_else(|| invalid("unknown deployment endpoint"))?
            .artifact)
    }

    /// Establishes SDK agreement only between independently declared members of a complete route.
    ///
    /// # Errors
    /// Rejects self-substitution, unknown routes/endpoints or SDK profile disagreement.
    pub fn agreement(
        &self,
        route: &str,
        local: &str,
        peer: &str,
    ) -> Result<NativeProfileAgreement, ProfileToolError> {
        let route = self
            .manifest
            .routes
            .iter()
            .find(|value| value.id == route)
            .ok_or_else(|| invalid("unknown deployment route"))?;
        if local == peer
            || !route.endpoints.iter().any(|id| id == local)
            || !route.endpoints.iter().any(|id| id == peer)
        {
            return Err(invalid(
                "local and peer must be distinct declared route members",
            ));
        }
        let local = self
            .endpoints
            .get(local)
            .ok_or_else(|| invalid("local endpoint missing"))?;
        let peer = self
            .endpoints
            .get(peer)
            .ok_or_else(|| invalid("peer endpoint missing"))?;
        Ok(NativeProfileAgreement::new(
            Arc::clone(&local.profile),
            &peer.profile,
        )?)
    }

    /// Checks required application types against actual linked target descriptors before activation.
    ///
    /// Opaque routers need not claim a local typed representation. Typed applications
    /// call this with their required identities; received bytes never choose a type.
    /// # Errors
    /// Rejects unlinked types or a compiled descriptor absent from the active agreed profile.
    pub fn require_local_types(
        &self,
        endpoint: &str,
        required: &[TypeKey],
    ) -> Result<(), ProfileToolError> {
        let endpoint = self
            .endpoints
            .get(endpoint)
            .ok_or_else(|| invalid("unknown deployment endpoint"))?;
        let inventory = registered_native_types()?;
        for key in required {
            let representation = inventory
                .iter()
                .find(|value| value.name() == key.name && value.revision() == key.revision)
                .ok_or_else(|| invalid("required application type is not linked"))?;
            let declared = match &endpoint.artifact.native {
                NativeMode::Table { allocations, .. } => allocations
                    .iter()
                    .find(|value| {
                        value.state == AllocationState::Active && &value.descriptor.key == key
                    })
                    .map(|value| &value.descriptor),
                NativeMode::ContractDefined { descriptor, .. } => {
                    (&descriptor.key == key).then_some(descriptor)
                }
            }
            .ok_or_else(|| invalid("required application type is not active in the agreement"))?;
            if declared.representation()? != *representation {
                return Err(invalid(
                    "compiled application descriptor differs from the provisioned type",
                ));
            }
        }
        Ok(())
    }
}
