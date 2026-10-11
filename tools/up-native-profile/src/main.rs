// SPDX-License-Identifier: Apache-2.0
//! Command-line generation and verification of provisional deployment artifacts.
#![forbid(unsafe_code)]
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use up_native_profile::{
    read_artifact, write_artifact, AgreementArtifact, AllocationBand, DeploymentEndpoint,
    DeploymentManifest, DeploymentRoute, IdentityDeclarations, InventoryArtifact, LoadedDeployment,
    ProfileToolError, SeedAllocation, Target, TypeKey,
};

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Provisional native deployment agreement tooling (not a registry standard)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate an append-only private-ID table from a target inventory.
    Generate {
        /// Target-side inventory JSON to allocate.
        #[arg(long)]
        inventory: PathBuf,
        /// Deployment domain shared by the participating peers.
        #[arg(long)]
        domain: String,
        /// Immutable generation; a successor advances the previous value by one.
        #[arg(long, default_value_t = 1)]
        generation: u32,
        /// Previous agreement whose IDs and retired reservations must be retained.
        #[arg(long)]
        previous: Option<PathBuf>,
        /// Optional genesis-only JSON seed assignments; incompatible with --previous.
        #[arg(long, conflicts_with = "previous")]
        seeds: Option<PathBuf>,
        /// JSON declarations for separate private-payload and wire namespaces.
        #[arg(long)]
        identities: Option<PathBuf>,
        /// First private payload ID in the allocation band, decimal or hexadecimal.
        #[arg(long, default_value = "0xF100", value_parser = parse_id)]
        first: u16,
        /// Last private payload ID in the allocation band, decimal or hexadecimal.
        #[arg(long, default_value = "0xF7FF", value_parser = parse_id)]
        last: u16,
        /// New output file; existing generations are never overwritten.
        #[arg(long)]
        output: PathBuf,
    },
    /// Generate an explicitly selected operation contract using payload ID 0.
    Contract {
        /// Target-side inventory containing the selected type.
        #[arg(long)]
        inventory: PathBuf,
        /// Deployment domain shared by the participating peers.
        #[arg(long)]
        domain: String,
        /// Explicit operation label for this provisional ID-zero agreement.
        #[arg(long)]
        operation: String,
        /// Exact deployment-stable name from the inventory.
        #[arg(long)]
        type_name: String,
        /// Exact representation revision from the inventory.
        #[arg(long, default_value_t = 0)]
        type_revision: u32,
        /// Optional JSON declarations of private payload and wire identities.
        #[arg(long)]
        identities: Option<PathBuf>,
        /// New agreement file; an existing output is rejected.
        #[arg(long)]
        output: PathBuf,
    },
    /// Verify artifact integrity and, for a successor, its supplied prior ledger.
    Verify {
        /// Agreement file to verify.
        #[arg(long)]
        agreement: PathBuf,
        /// Required prior ledger when verifying a successor generation.
        #[arg(long)]
        previous: Option<PathBuf>,
    },
    /// Verify a target inventory, optionally requiring a specific compiler triple.
    VerifyInventory {
        /// Target-side inventory file to verify.
        #[arg(long)]
        inventory: PathBuf,
        /// Required compiler target triple, checked against exported metadata.
        #[arg(long)]
        expect_target: Option<String>,
    },
    /// Create an independently provisioned endpoint/route manifest from explicit inputs.
    Manifest {
        /// JSON array of independently provisioned endpoint expectations.
        #[arg(long)]
        endpoints: PathBuf,
        /// JSON array declaring complete route membership.
        #[arg(long)]
        routes: PathBuf,
        /// New deployment manifest file; existing outputs are rejected.
        #[arg(long)]
        output: PathBuf,
    },
    /// Load independently provisioned route members before admitting a peer pair.
    Admit {
        /// Root directory beneath which agreement files must resolve.
        #[arg(long)]
        profile_root: PathBuf,
        /// Independently provisioned deployment manifest file.
        #[arg(long)]
        manifest: PathBuf,
        /// Declared route identifier to admit.
        #[arg(long)]
        route: String,
        /// Local endpoint identifier in the route.
        #[arg(long)]
        local: String,
        /// Distinct peer endpoint identifier in the same route.
        #[arg(long)]
        peer: String,
    },
}

fn parse_id(text: &str) -> Result<u16, String> {
    text.strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .map_or_else(|| text.parse(), |hex| u16::from_str_radix(hex, 16))
        .map_err(|_| "expected a 16-bit decimal or hexadecimal identifier".to_owned())
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(command: Command) -> Result<(), ProfileToolError> {
    match command {
        Command::Generate {
            inventory,
            domain,
            generation,
            previous,
            seeds,
            identities,
            first,
            last,
            output,
        } => {
            let inventory: InventoryArtifact = read_artifact(&inventory)?;
            let previous: Option<AgreementArtifact> =
                previous.as_deref().map(read_artifact).transpose()?;
            let seeds: Vec<SeedAllocation> = seeds
                .as_deref()
                .map(read_artifact)
                .transpose()?
                .unwrap_or_default();
            let identities: IdentityDeclarations = identities
                .as_deref()
                .map(read_artifact)
                .transpose()?
                .unwrap_or_else(|| {
                    previous
                        .as_ref()
                        .map(|value| value.identities.clone())
                        .unwrap_or_default()
                });
            let result = AgreementArtifact::generate_table(
                &inventory,
                domain,
                generation,
                AllocationBand { first, last },
                previous.as_ref(),
                &seeds,
                identities,
            )?;
            write_artifact(&output, &result)?;
            println!("{}", result.content_sha256);
        }
        Command::Contract {
            inventory,
            domain,
            operation,
            type_name,
            type_revision,
            identities,
            output,
        } => {
            let inventory: InventoryArtifact = read_artifact(&inventory)?;
            let identities = identities
                .as_deref()
                .map(read_artifact)
                .transpose()?
                .unwrap_or_default();
            let result = AgreementArtifact::generate_contract(
                &inventory,
                domain,
                operation,
                &TypeKey {
                    name: type_name,
                    revision: type_revision,
                },
                identities,
            )?;
            write_artifact(&output, &result)?;
            println!("{}", result.content_sha256);
        }
        Command::Verify {
            agreement,
            previous,
        } => {
            let agreement: AgreementArtifact = read_artifact(&agreement)?;
            agreement.validate()?;
            if let Some(path) = previous {
                agreement.validate_successor(&read_artifact(&path)?)?;
            } else if agreement.previous_content_sha256.is_some() {
                return Err(ProfileToolError::Invalid(
                    "verify a successor with its actual --previous ledger",
                ));
            }
            println!("{}", agreement.content_sha256);
        }
        Command::VerifyInventory {
            inventory,
            expect_target,
        } => {
            let inventory: InventoryArtifact = read_artifact(&inventory)?;
            inventory.validate()?;
            if expect_target.is_some_and(|expected| expected != inventory.target.triple) {
                return Err(ProfileToolError::Invalid(
                    "inventory does not match the requested target",
                ));
            }
            println!("{}", inventory.content_sha256);
        }
        Command::Admit {
            profile_root,
            manifest,
            route,
            local,
            peer,
        } => {
            let manifest: DeploymentManifest = read_artifact(&manifest)?;
            let loaded = LoadedDeployment::load(&profile_root, manifest)?;
            loaded.agreement(&route, &local, &peer)?;
            let artifact = loaded.artifact(&local)?;
            println!(
                "{}",
                serde_json::json!({"configuration_admission": "passed", "route": route,
                "local": local, "peer": peer, "manifest_sha256": loaded.manifest().content_sha256,
                "agreement_sha256": artifact.content_sha256, "domain": artifact.domain,
                "generation": artifact.generation, "representation_target": artifact.target,
                "executable_target": Target::current()})
            );
        }
        Command::Manifest {
            endpoints,
            routes,
            output,
        } => {
            let endpoints: Vec<DeploymentEndpoint> = read_artifact(&endpoints)?;
            let routes: Vec<DeploymentRoute> = read_artifact(&routes)?;
            let manifest = DeploymentManifest::new(endpoints, routes)?;
            write_artifact(&output, &manifest)?;
            println!("{}", manifest.content_sha256);
        }
    }
    Ok(())
}
