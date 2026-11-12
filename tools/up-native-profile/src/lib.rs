// SPDX-License-Identifier: Apache-2.0
//! Provisional native inventory, deployment agreement and manifest tooling.
//!
//! The SDK owns native representation and identity contracts. This crate owns
//! versioned artifact serialization, immutable type history and deployment-file
//! admission. See the [deployment guide](guide) for format and distribution policy.
#![forbid(unsafe_code)]

#[cfg(test)]
mod additional_tests;
mod agreement;
mod inventory;
mod manifest;
mod owned_context;
pub use owned_context::OwnedContextDeclaration;
#[cfg(test)]
mod tests;

pub use agreement::{
    AgreementArtifact, IdentityDeclarations, NativeCatalogue, NativeTypeEntry, NativeTypeState,
    PrivatePayloadDeclaration, WireDeclaration, WireNamespace, WireReference,
};
pub use inventory::{InventoryArtifact, NativeDescriptor, Target, TypeKey};
pub use manifest::{DeploymentEndpoint, DeploymentManifest, DeploymentRoute, LoadedDeployment};

/// Artifact format, catalogue history and deployment admission guide.
#[doc = include_str!("../docs/deployment-agreement.md")]
pub mod guide {}

use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Version of the unchanged provisional inventory and deployment manifest formats.
pub const FORMAT_VERSION: u32 = 1;
/// Version of the shared-encoding provisional deployment agreement format.
pub const AGREEMENT_FORMAT_VERSION: u32 = 2;
/// Maximum accepted inventory/agreement/manifest file size (16 MiB).
pub const MAX_ARTIFACT_BYTES: usize = 16 * 1024 * 1024;

/// Typed artifact, catalogue and deployment errors.
#[derive(Debug, thiserror::Error)]
pub enum ProfileToolError {
    /// A file could not be read or written.
    #[error("profile file I/O at {location}: {source}", location = .path.display())]
    Io {
        /// File or directory whose operation failed.
        path: PathBuf,
        /// Underlying filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// JSON did not match the declared artifact shape.
    #[error("profile JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// A canonical native descriptor or profile was invalid.
    #[error(transparent)]
    Native(#[from] up_rust::NativeProfileError),
    /// An artifact violated a version, identity, catalogue or admission rule.
    #[error("invalid deployment artifact: {0}")]
    Invalid(&'static str),
    /// Two distinct type identities have the same short structural token.
    #[error("native token {token:#010x} collides between {first} and {second}")]
    TokenCollision {
        /// Colliding token, independent of the shared payload encoding.
        token: u32,
        /// First retained or inventoried type identity.
        first: TypeKey,
        /// Other type identity with the same token.
        second: TypeKey,
    },
    /// The provisional format is unsupported and must be regenerated.
    #[error("unsupported artifact format_version {found}; expected {supported}; regenerate deployment artifacts")]
    UnsupportedFormatVersion {
        /// Actual unsigned version from the artifact.
        found: u64,
        /// This tool's only supported provisional version.
        supported: u32,
    },
}

pub(crate) fn io_error(path: &Path, source: std::io::Error) -> ProfileToolError {
    ProfileToolError::Io {
        path: path.to_path_buf(),
        source,
    }
}

pub(crate) fn invalid(reason: &'static str) -> ProfileToolError {
    ProfileToolError::Invalid(reason)
}

pub(crate) fn check_format_version(found: u64) -> Result<(), ProfileToolError> {
    check_version(found, FORMAT_VERSION)
}

pub(crate) fn check_version(found: u64, supported: u32) -> Result<(), ProfileToolError> {
    if found != u64::from(supported) {
        return Err(ProfileToolError::UnsupportedFormatVersion { found, supported });
    }
    Ok(())
}

pub(crate) fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(crate) fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn canonical_value(
    value: &serde_json::Value,
    output: &mut Vec<u8>,
) -> Result<(), ProfileToolError> {
    match value {
        serde_json::Value::Object(map) => {
            output.push(b'{');
            let mut entries = map.iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            for (index, (key, value)) in entries.into_iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                serde_json::to_writer(&mut *output, key)?;
                output.push(b':');
                canonical_value(value, output)?;
            }
            output.push(b'}');
        }
        serde_json::Value::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(b',');
                }
                canonical_value(value, output)?;
            }
            output.push(b']');
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => serde_json::to_writer(output, value)?,
    }
    Ok(())
}

pub(crate) fn content_hash(value: &impl Serialize) -> Result<String, ProfileToolError> {
    let mut value = serde_json::to_value(value)?;
    value
        .as_object_mut()
        .ok_or_else(|| invalid("artifact must be an object"))?
        .remove("content_sha256");
    let mut bytes = Vec::new();
    canonical_value(&value, &mut bytes)?;
    Ok(digest(&bytes))
}

pub(crate) fn check_hash(value: &impl Serialize, expected: &str) -> Result<(), ProfileToolError> {
    if expected.len() != 64 || content_hash(value)? != expected {
        return Err(invalid("artifact content hash mismatch"));
    }
    Ok(())
}

/// Reads JSON only after enforcing a finite file/input bound.
///
/// # Errors
/// Rejects oversized files, I/O failure, malformed/trailing JSON and unsupported
/// artifact versions or removed allocation/mode fields.
pub fn read_artifact<T: DeserializeOwned>(path: &Path) -> Result<T, ProfileToolError> {
    let file = std::fs::File::open(path).map_err(|error| io_error(path, error))?;
    if file
        .metadata()
        .map_err(|error| io_error(path, error))?
        .len()
        > MAX_ARTIFACT_BYTES as u64
    {
        return Err(invalid("artifact exceeds input limit"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_ARTIFACT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error(path, error))?;
    if bytes.len() > MAX_ARTIFACT_BYTES {
        return Err(invalid("artifact grew beyond input limit"));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    if let Some(version) = value
        .get("format_version")
        .and_then(serde_json::Value::as_u64)
    {
        let supported = if value.get("schema").and_then(serde_json::Value::as_str)
            == Some("uprotocol.deployment-agreement.provisional")
        {
            AGREEMENT_FORMAT_VERSION
        } else {
            FORMAT_VERSION
        };
        check_version(version, supported)?;
    }
    // Deserialize the original bytes so duplicate fields still fail instead of
    // being silently collapsed by the bounded version inspection above.
    Ok(serde_json::from_slice(&bytes)?)
}

/// Writes a new artifact without overwriting an existing deployment generation.
///
/// # Errors
/// Rejects an existing output, an oversized encoding, serialization or I/O failure.
pub fn write_artifact(path: &Path, value: &impl Serialize) -> Result<(), ProfileToolError> {
    use std::io::Write;
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_ARTIFACT_BYTES {
        return Err(invalid("artifact exceeds output limit"));
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io_error(path, error))?;
    file.write_all(&bytes)
        .map_err(|error| io_error(path, error))?;
    file.sync_all().map_err(|error| io_error(path, error))?;
    Ok(())
}
