// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Artifact boundaries, immutable successors and provisioned path/type admission.
use super::*;
use test_case::test_case;
use up_rust::{NativeByteOrder, NativeRepresentation, NativeTarget, StablePayload};

fn inventory() -> InventoryArtifact {
    InventoryArtifact::from_representations(
        Target::current(),
        [
            NativeRepresentation::new("example.Signal", 0, 4, 4, NativeByteOrder::native(), vec![])
                .unwrap(),
        ],
    )
    .unwrap()
}

fn table(identities: IdentityDeclarations) -> AgreementArtifact {
    AgreementArtifact::generate_table(
        &inventory(),
        "example".into(),
        1,
        AllocationBand::default(),
        None,
        &[],
        identities,
    )
    .unwrap()
}

fn manifest(artifact: &AgreementArtifact) -> DeploymentManifest {
    DeploymentManifest::new(
        ["sender", "receiver"]
            .into_iter()
            .map(|id| DeploymentEndpoint {
                id: id.into(),
                agreement_path: format!("{id}.json"),
                content_sha256: artifact.content_sha256.clone(),
                target: artifact.target.clone(),
                domain: artifact.domain.clone(),
                generation: artifact.generation,
            })
            .collect(),
        vec![DeploymentRoute {
            id: "direct".into(),
            endpoints: vec!["sender".into(), "receiver".into()],
        }],
    )
    .unwrap()
}

#[test]
fn successor_cannot_change_its_allocation_band() {
    let first = table(IdentityDeclarations::default());
    let band = AllocationBand {
        last: 0xF800,
        ..AllocationBand::default()
    };
    let reason =
        "the successor must keep the table band; a band change needs a new deployment profile";
    assert!(
        matches!(AgreementArtifact::generate_table(&inventory(), "example".into(), 2,
        band, Some(&first), &[], IdentityDeclarations::default()),
        Err(ProfileToolError::Invalid(message)) if message == reason)
    );
    let mut next = AgreementArtifact::generate_table(
        &inventory(),
        "example".into(),
        2,
        AllocationBand::default(),
        Some(&first),
        &[],
        IdentityDeclarations::default(),
    )
    .unwrap();
    if let NativeMode::Table {
        allocation_band, ..
    } = &mut next.native
    {
        *allocation_band = band;
    }
    next.content_sha256 = content_hash(&next).unwrap();
    assert!(
        matches!(next.validate_successor(&first), Err(ProfileToolError::Invalid(message)) if message == reason)
    );
}

#[test]
fn successor_seed_rejection_names_the_conflicting_input() {
    let first = table(IdentityDeclarations::default());
    let seeds = [SeedAllocation {
        key: TypeKey {
            name: "example.Signal".into(),
            revision: 0,
        },
        encoding_id: 0xF101,
    }];
    assert!(matches!(
        AgreementArtifact::generate_table(
            &inventory(),
            "example".into(),
            2,
            AllocationBand::default(),
            Some(&first),
            &seeds,
            IdentityDeclarations::default()
        ),
        Err(ProfileToolError::Invalid(
            "seeds cannot be supplied with a previous ledger"
        ))
    ));
}

#[test]
fn differing_duplicate_mode_tags_reject_in_the_json_reader() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("duplicate-mode.json");
    std::fs::write(&path, br#"{"mode":"table","allocation_band":{"first":61696,"last":63487},"allocations":[],"mode":"contract_defined"}"#).unwrap();
    assert!(
        matches!(read_artifact::<NativeMode>(&path), Err(ProfileToolError::Json(error))
        if error.to_string().contains("duplicate field `mode`"))
    );
}

#[test]
fn oversized_artifact_input_and_output_reject_without_creating_output() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("oversized-input.json");
    std::fs::File::create(&input)
        .unwrap()
        .set_len(MAX_ARTIFACT_BYTES as u64 + 1)
        .unwrap();
    assert!(matches!(
        read_artifact::<serde_json::Value>(&input),
        Err(ProfileToolError::Invalid("artifact exceeds input limit"))
    ));
    let output = directory.path().join("oversized-output.json");
    assert!(matches!(
        write_artifact(&output, &"x".repeat(MAX_ARTIFACT_BYTES)),
        Err(ProfileToolError::Invalid("artifact exceeds output limit"))
    ));
    assert!(!output.exists());
}

#[test_case("../escape.json"; "parent traversal")]
#[test_case("/escape.json"; "absolute locator")]
fn manifest_rejects_escaping_agreement_locators(locator: &str) {
    let mut value = manifest(&table(IdentityDeclarations::default()));
    value.endpoints.first_mut().unwrap().agreement_path = locator.into();
    value.content_sha256 = content_hash(&value).unwrap();
    assert!(matches!(
        value.validate(),
        Err(ProfileToolError::Invalid(
            "agreement locator must stay beneath the deployment root"
        ))
    ));
}

#[cfg(unix)]
#[test]
fn load_rejects_a_valid_artifact_behind_an_escaping_symlink() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let artifact = table(IdentityDeclarations::default());
    let external = outside.path().join("agreement.json");
    write_artifact(&external, &artifact).unwrap();
    std::os::unix::fs::symlink(&external, directory.path().join("sender.json")).unwrap();
    write_artifact(&directory.path().join("receiver.json"), &artifact).unwrap();
    assert!(matches!(
        LoadedDeployment::load(directory.path(), manifest(&artifact)),
        Err(ProfileToolError::Invalid(
            "agreement symlink escapes deployment root"
        ))
    ));
}

#[test_case(true; "private payload declaration")]
#[test_case(false; "wire declaration")]
fn successor_cannot_remove_identity_declarations(remove_payload: bool) {
    let declarations = IdentityDeclarations {
        private_payloads: vec![PrivatePayloadDeclaration {
            encoding_id: 0xF000,
            meaning: "example.bytes".into(),
            owner: "example".into(),
            version: 1,
        }],
        wires: vec![WireDeclaration {
            namespace: WireNamespace::Wire,
            literal: "example.wire".into(),
            compact_id: 0x9000,
            owner: "example".into(),
            version: 1,
            reference: WireReference::Literal,
        }],
    };
    let first = table(declarations.clone());
    let mut removed = declarations;
    if remove_payload {
        removed.private_payloads.clear();
    } else {
        removed.wires.clear();
    }
    assert!(matches!(
        AgreementArtifact::generate_table(
            &inventory(),
            "example".into(),
            2,
            AllocationBand::default(),
            Some(&first),
            &[],
            removed
        ),
        Err(ProfileToolError::Invalid(
            "existing identity declarations cannot be removed or redefined"
        ))
    ));
}

#[test]
fn successor_cannot_switch_to_another_valid_target_inventory() {
    let first = table(IdentityDeclarations::default());
    let mut target = Target::current();
    target.triple = "foreign-unknown-target".into();
    let representation = NativeRepresentation::for_target(
        NativeTarget::new(target.triple.clone(), target.pointer_width).unwrap(),
        "example.Signal",
        0,
        4,
        4,
        NativeByteOrder::native(),
        vec![],
    )
    .unwrap();
    let foreign = InventoryArtifact::from_representations(target, [representation]).unwrap();
    assert!(matches!(
        AgreementArtifact::generate_table(
            &foreign,
            "example".into(),
            2,
            AllocationBand::default(),
            Some(&first),
            &[],
            IdentityDeclarations::default()
        ),
        Err(ProfileToolError::Invalid(
            "table successor must preserve domain/target and advance exactly one generation"
        ))
    ));
}

#[test]
fn all_artifact_families_reject_tampered_content_hashes() {
    let mut inv = inventory();
    inv.content_sha256 = "0".repeat(64);
    assert!(matches!(
        inv.validate(),
        Err(ProfileToolError::Invalid("artifact content hash mismatch"))
    ));
    let artifact = table(IdentityDeclarations::default());
    let mut changed = artifact.clone();
    changed.content_sha256 = "0".repeat(64);
    assert!(matches!(
        changed.validate(),
        Err(ProfileToolError::Invalid("artifact content hash mismatch"))
    ));
    let mut deployment = manifest(&artifact);
    deployment.content_sha256 = "0".repeat(64);
    assert!(matches!(
        deployment.validate(),
        Err(ProfileToolError::Invalid("artifact content hash mismatch"))
    ));
}

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "test.tool.LocalDoor", inventory)]
struct LocalDoor {
    value: u32,
}

#[test_case(false; "matching linked descriptor")]
#[test_case(true; "same identity different descriptor")]
fn id_zero_loaded_deployment_checks_the_linked_descriptor(mismatch: bool) {
    let representation = if mismatch {
        NativeRepresentation::new(
            LocalDoor::TYPE_NAME,
            LocalDoor::REVISION,
            8,
            8,
            NativeByteOrder::native(),
            vec![],
        )
        .unwrap()
    } else {
        LocalDoor::native_representation()
    };
    let inventory =
        InventoryArtifact::from_representations(Target::current(), [representation]).unwrap();
    let key = TypeKey {
        name: LocalDoor::TYPE_NAME.into(),
        revision: LocalDoor::REVISION,
    };
    let artifact = AgreementArtifact::generate_single_type(
        &inventory,
        "local".into(),
        &key,
        IdentityDeclarations::default(),
    )
    .unwrap();
    let directory = tempfile::tempdir().unwrap();
    for id in ["sender", "receiver"] {
        write_artifact(&directory.path().join(format!("{id}.json")), &artifact).unwrap();
    }
    let loaded = LoadedDeployment::load(directory.path(), manifest(&artifact)).unwrap();
    let result = loaded.require_local_types("sender", &[key]);
    if mismatch {
        assert!(matches!(
            result,
            Err(ProfileToolError::Invalid(
                "compiled application descriptor differs from the provisioned type"
            ))
        ));
    } else {
        result.unwrap();
        let agreement = loaded.agreement("direct", "sender", "receiver").unwrap();
        assert_eq!(
            agreement
                .identity_for(&LocalDoor::native_representation())
                .unwrap()
                .encoding(),
            up_rust::PayloadEncoding::CONTRACT_DEFINED
        );
    }
}

#[test_case("a"; "ASCII")]
#[test_case("💡"; "multibyte Unicode")]
fn literal_length_matches_the_schema_character_limit(character: &str) {
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../schemas/deployment-agreement-v1.schema.json"
    ))
    .unwrap();
    let limit = usize::try_from(
        schema
            .pointer("/$defs/wire/properties/literal/maxLength")
            .unwrap()
            .as_u64()
            .unwrap(),
    )
    .unwrap();
    let mut identities = IdentityDeclarations {
        wires: vec![WireDeclaration {
            namespace: WireNamespace::Wire,
            literal: character.repeat(limit),
            compact_id: 0x9000,
            owner: "example".into(),
            version: 1,
            reference: WireReference::Literal,
        }],
        ..Default::default()
    };
    identities.validate().unwrap();
    identities.wires.first_mut().unwrap().literal.push('x');
    assert!(matches!(
        identities.validate(),
        Err(ProfileToolError::Invalid(
            "invalid or duplicate wire declaration"
        ))
    ));
}

#[test]
fn genesis_allows_an_absent_optional_previous_hash() {
    let artifact = table(IdentityDeclarations::default());
    let mut json = serde_json::to_value(&artifact).unwrap();
    json.as_object_mut()
        .unwrap()
        .remove("previous_content_sha256");
    let restored: AgreementArtifact = serde_json::from_value(json).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, artifact);
}

#[test]
fn future_reserved_id_is_rejected_before_profile_construction() {
    let inventory = inventory();
    let key = inventory
        .types
        .first()
        .expect("one inventoried type")
        .key
        .clone();
    for last in [0xFFFE, 0xFFFF] {
        let band = AllocationBand {
            first: 0xF000,
            last,
        };
        let result = AgreementArtifact::generate_table(
            &inventory,
            "example".into(),
            1,
            band,
            None,
            &[SeedAllocation {
                key: key.clone(),
                encoding_id: 0xFFFF,
            }],
            IdentityDeclarations::default(),
        );
        assert!(matches!(result, Err(ProfileToolError::Invalid(_))));
    }
    let mut artifact = table(IdentityDeclarations::default());
    if let NativeMode::Table {
        allocation_band,
        allocations,
    } = &mut artifact.native
    {
        allocation_band.last = 0xFFFF;
        // An old full band is invalid even without an allocation at its last ID.
        assert!(allocations.iter().all(|entry| entry.encoding_id != 0xFFFF));
    }
    artifact.content_sha256 = content_hash(&artifact).unwrap();
    assert!(matches!(
        artifact.profile(),
        Err(ProfileToolError::Invalid(_))
    ));
    let declarations = IdentityDeclarations {
        private_payloads: vec![PrivatePayloadDeclaration {
            encoding_id: 0xFFFF,
            meaning: "old.private".into(),
            owner: "example".into(),
            version: 1,
        }],
        ..Default::default()
    };
    assert!(matches!(
        declarations.validate(),
        Err(ProfileToolError::Invalid(_))
    ));
}

#[test]
fn last_private_id_is_valid_and_default_band_is_unchanged() {
    let inventory = inventory();
    assert_eq!(
        AllocationBand::default(),
        AllocationBand {
            first: 0xF100,
            last: 0xF7FF
        }
    );
    let artifact = AgreementArtifact::generate_table(
        &inventory,
        "example".into(),
        1,
        AllocationBand {
            first: 0xF000,
            last: 0xFFFE,
        },
        None,
        &[SeedAllocation {
            key: inventory
                .types
                .first()
                .expect("one inventoried type")
                .key
                .clone(),
            encoding_id: 0xFFFE,
        }],
        IdentityDeclarations::default(),
    )
    .unwrap();
    assert!(artifact.profile().is_ok());
}
