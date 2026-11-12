// SPDX-License-Identifier: Apache-2.0
//! Actual-target exporter that deliberately controls defining-crate linkage.
use clap::Parser;
use std::path::PathBuf;
use up_native_profile::{write_artifact, InventoryArtifact, ProfileToolError, TypeKey};

// A manifest dependency alone is not evidence that its registrations are linked.
#[cfg(feature = "linked-types")]
use native_inventory_fixture_types as _;

#[derive(Debug, Parser)]
#[command(
    about = "Export linked actual-target fixtures in the provisional native inventory format"
)]
struct Args {
    /// New output path; existing inventory generations are never overwritten.
    #[arg(long)]
    output: PathBuf,
    /// Require this actual compiler target; never relabel an export.
    #[arg(long)]
    expect_target: Option<String>,
    /// Require a linked fixture type name at revision zero.
    #[arg(long)]
    require_type: Vec<String>,
}

fn main() -> Result<(), ProfileToolError> {
    let args = Args::parse();
    let required = args
        .require_type
        .into_iter()
        .map(|name| TypeKey { name, revision: 0 })
        .collect::<Vec<_>>();
    let inventory = InventoryArtifact::from_linked(&required)?;
    if args
        .expect_target
        .is_some_and(|target| target != inventory.target.triple)
    {
        return Err(ProfileToolError::Invalid(
            "exporter was not compiled for the requested target",
        ));
    }
    write_artifact(&args.output, &inventory)?;
    println!("{}", inventory.content_sha256);
    Ok(())
}
