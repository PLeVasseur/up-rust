// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Execute the linked exporter and all deployment-tool subcommands.
use std::path::{Path, PathBuf};
use std::process::Command;
use up_native_profile::{
    read_artifact, write_artifact, AgreementArtifact, DeploymentEndpoint, DeploymentRoute,
    IdentityDeclarations, InventoryArtifact, Target, WireDeclaration, WireNamespace, WireReference,
};
use up_rust::{StableContainerWire, UFrameFieldsMetadataCodec, UWire};

fn run(executable: &Path, args: &[&str], expected_code: i32) -> String {
    let output = Command::new(executable).args(args).output().unwrap();
    eprintln!(
        "CLI {:?} {:?}: exit={:?}\nstdout={}\nstderr={}",
        executable,
        args,
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(expected_code));
    if expected_code == 0 {
        String::from_utf8(output.stdout).unwrap()
    } else {
        String::from_utf8(output.stderr).unwrap()
    }
}

fn exporter() -> (PathBuf, Option<tempfile::TempDir>) {
    if let Some(path) = std::env::var_os("UP_NATIVE_PROFILE_EXPORTER") {
        let path = PathBuf::from(path);
        assert!(
            path.is_file(),
            "the supplied exporter must be a built executable"
        );
        return (path, None);
    }
    // A direct cargo-test invocation gets a real build in an isolated target
    // directory, never a nested build waiting for the parent target's lock.
    let target = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(env!("CARGO"))
        .args([
            "build",
            "--locked",
            "-p",
            "native-inventory-fixture-export",
            "--bin",
            "native-inventory-fixture-export",
            "--message-format=json",
        ])
        .current_dir(root)
        .env("CARGO_TARGET_DIR", target.path())
        .env_remove("CARGO_MAKEFLAGS")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let executable = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find_map(|line| {
            let value: serde_json::Value = serde_json::from_str(line).ok()?;
            if value.get("reason")?.as_str()? != "compiler-artifact"
                || value.get("target")?.get("name")?.as_str()? != "native-inventory-fixture-export"
            {
                return None;
            }
            value.get("executable")?.as_str().map(PathBuf::from)
        })
        .expect("Cargo must identify the actual exporter executable");
    (executable, Some(target))
}

#[test]
fn exported_inventory_drives_all_six_cli_subcommands() {
    let tool = Path::new(env!("CARGO_BIN_EXE_up-native-profile"));
    let (exporter, _isolated_build) = exporter();
    let directory = tempfile::tempdir().unwrap();
    let inventory_path = directory.path().join("inventory.json");
    let identities_path = directory.path().join("identities.json");
    let agreement_path = directory.path().join("agreement.json");
    let contract_path = directory.path().join("contract.json");
    let endpoints_path = directory.path().join("endpoints.json");
    let routes_path = directory.path().join("routes.json");
    let manifest_path = directory.path().join("manifest.json");
    let target = Target::current();

    run(
        &exporter,
        &[
            "--output",
            inventory_path.to_str().unwrap(),
            "--expect-target",
            &target.triple,
            "--require-type",
            "example.DoorState",
        ],
        0,
    );
    let inventory: InventoryArtifact = read_artifact(&inventory_path).unwrap();
    assert_eq!(inventory.target, target);
    assert!(inventory
        .types
        .iter()
        .any(|value| value.key.name == "example.DoorState"));
    run(
        tool,
        &[
            "verify-inventory",
            "--inventory",
            inventory_path.to_str().unwrap(),
            "--expect-target",
            &target.triple,
        ],
        0,
    );
    let identities = IdentityDeclarations {
        wires: [
            (WireNamespace::Wire, StableContainerWire::WIRE_ID),
            (
                WireNamespace::PayloadFamily,
                StableContainerWire::PAYLOAD_FAMILY_ID,
            ),
        ]
        .into_iter()
        .map(|(namespace, identity)| WireDeclaration {
            namespace,
            literal: identity.literal_id().into(),
            compact_id: identity.compact_id(),
            owner: "example".into(),
            version: 1,
            reference: WireReference::Literal,
        })
        .collect(),
        ..Default::default()
    };
    write_artifact(&identities_path, &identities).unwrap();
    run(
        tool,
        &[
            "generate",
            "--inventory",
            inventory_path.to_str().unwrap(),
            "--domain",
            "example.cli",
            "--identities",
            identities_path.to_str().unwrap(),
            "--output",
            agreement_path.to_str().unwrap(),
        ],
        0,
    );
    run(
        tool,
        &["verify", "--agreement", agreement_path.to_str().unwrap()],
        0,
    );
    run(
        tool,
        &[
            "contract",
            "--inventory",
            inventory_path.to_str().unwrap(),
            "--domain",
            "example.cli",
            "--operation",
            "door.event",
            "--type-name",
            "example.DoorState",
            "--output",
            contract_path.to_str().unwrap(),
        ],
        0,
    );
    let agreement: AgreementArtifact = read_artifact(&agreement_path).unwrap();
    agreement
        .require_wire::<StableContainerWire, UFrameFieldsMetadataCodec>()
        .unwrap();
    let endpoints: Vec<_> = ["sender", "receiver"]
        .into_iter()
        .map(|id| DeploymentEndpoint {
            id: id.into(),
            agreement_path: format!("{id}.json"),
            content_sha256: agreement.content_sha256.clone(),
            target: agreement.target.clone(),
            domain: agreement.domain.clone(),
            generation: agreement.generation,
        })
        .collect();
    for endpoint in &endpoints {
        write_artifact(&directory.path().join(&endpoint.agreement_path), &agreement).unwrap();
    }
    write_artifact(&endpoints_path, &endpoints).unwrap();
    write_artifact(
        &routes_path,
        &vec![DeploymentRoute {
            id: "direct".into(),
            endpoints: vec!["sender".into(), "receiver".into()],
        }],
    )
    .unwrap();
    run(
        tool,
        &[
            "manifest",
            "--endpoints",
            endpoints_path.to_str().unwrap(),
            "--routes",
            routes_path.to_str().unwrap(),
            "--output",
            manifest_path.to_str().unwrap(),
        ],
        0,
    );
    let admitted = run(
        tool,
        &[
            "admit",
            "--profile-root",
            directory.path().to_str().unwrap(),
            "--manifest",
            manifest_path.to_str().unwrap(),
            "--route",
            "direct",
            "--local",
            "sender",
            "--peer",
            "receiver",
        ],
        0,
    );
    let admitted: serde_json::Value = serde_json::from_str(&admitted).unwrap();
    assert_eq!(
        admitted
            .get("configuration_admission")
            .and_then(serde_json::Value::as_str),
        Some("passed")
    );

    let error = run(
        tool,
        &[
            "generate",
            "--inventory",
            inventory_path.to_str().unwrap(),
            "--domain",
            "example.cli",
            "--previous",
            agreement_path.to_str().unwrap(),
            "--seeds",
            "absent-seeds.json",
            "--output",
            "unused.json",
        ],
        2,
    );
    assert!(error.contains("--seeds") && error.contains("--previous"));
    let absent = directory.path().join("missing-agreement.json");
    let error = run(
        tool,
        &["verify", "--agreement", absent.to_str().unwrap()],
        1,
    );
    assert!(error.contains(absent.to_str().unwrap()) && error.contains("profile file I/O"));
    assert!(
        !error.contains("Io {"),
        "the CLI must print Display, not Debug"
    );

    if let Some(destination) = std::env::var_os("UP_NATIVE_PROFILE_PROOF_DIR") {
        let destination = PathBuf::from(destination);
        std::fs::create_dir_all(&destination).unwrap();
        for entry in std::fs::read_dir(directory.path()).unwrap() {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            std::fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}
