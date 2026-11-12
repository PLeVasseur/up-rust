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

fn agreement(identities: IdentityDeclarations) -> AgreementArtifact {
    AgreementArtifact::generate(&inventory(), "example".into(), 1, 0xF100, None, identities)
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
fn successor_cannot_change_its_shared_encoding() {
    let first = agreement(IdentityDeclarations::default());
    assert!(matches!(
        AgreementArtifact::generate(
            &inventory(),
            "example".into(),
            2,
            0,
            Some(&first),
            IdentityDeclarations::default()
        ),
        Err(ProfileToolError::Invalid(
            "successor must preserve domain/target/encoding and advance exactly one generation"
        ))
    ));
    let mut next = AgreementArtifact::generate(
        &inventory(),
        "example".into(),
        2,
        0xF100,
        Some(&first),
        IdentityDeclarations::default(),
    )
    .unwrap();
    next.native.encoding_id = 0xF101;
    next.content_sha256 = content_hash(&next).unwrap();
    assert!(matches!(
        next.validate_successor(&first),
        Err(ProfileToolError::Invalid(
            "invalid previous-agreement hash/generation/domain/target/encoding chain"
        ))
    ));
}

#[test]
fn explicit_new_revision_adds_a_descriptor_without_changing_the_encoding() {
    let first = agreement(IdentityDeclarations::default());
    let revised =
        NativeRepresentation::new("example.Signal", 1, 8, 8, NativeByteOrder::native(), vec![])
            .unwrap();
    let inv =
        InventoryArtifact::from_representations(Target::current(), [revised.clone()]).unwrap();
    let next = AgreementArtifact::generate(
        &inv,
        "example".into(),
        2,
        0xF100,
        Some(&first),
        IdentityDeclarations::default(),
    )
    .unwrap();
    next.validate_successor(&first).unwrap();
    assert_eq!(next.native.types.len(), 2);
    assert_eq!(
        next.native.types.first().unwrap().state,
        NativeTypeState::Retired
    );
    let profile = next.profile().unwrap();
    assert_eq!(profile.encoding().id(), 0xF100);
    assert_eq!(profile.representations().collect::<Vec<_>>(), [&revised]);
}

#[test]
fn duplicate_shared_encoding_rejects_in_the_json_reader() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("duplicate-encoding.json");
    std::fs::write(
        &path,
        br#"{"encoding_id":61696,"types":[],"encoding_id":0}"#,
    )
    .unwrap();
    assert!(
        matches!(read_artifact::<NativeCatalogue>(&path), Err(ProfileToolError::Json(error))
        if error.to_string().contains("duplicate field `encoding_id`"))
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
    let mut value = manifest(&agreement(IdentityDeclarations::default()));
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
    let artifact = agreement(IdentityDeclarations::default());
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
    let first = agreement(declarations.clone());
    let mut removed = declarations;
    if remove_payload {
        removed.private_payloads.clear();
    } else {
        removed.wires.clear();
    }
    assert!(matches!(
        AgreementArtifact::generate(
            &inventory(),
            "example".into(),
            2,
            0xF100,
            Some(&first),
            removed
        ),
        Err(ProfileToolError::Invalid(
            "existing identity declarations cannot be removed or redefined"
        ))
    ));
}

#[test]
fn successor_cannot_switch_to_another_valid_target_inventory() {
    let first = agreement(IdentityDeclarations::default());
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
        AgreementArtifact::generate(
            &foreign,
            "example".into(),
            2,
            0xF100,
            Some(&first),
            IdentityDeclarations::default()
        ),
        Err(ProfileToolError::Invalid(
            "successor must preserve domain/target/encoding and advance exactly one generation"
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
    let artifact = agreement(IdentityDeclarations::default());
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
    let artifact = AgreementArtifact::generate(
        &inventory,
        "local".into(),
        1,
        0,
        None,
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
        "../schemas/deployment-agreement-v2.schema.json"
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
    let artifact = agreement(IdentityDeclarations::default());
    let mut json = serde_json::to_value(&artifact).unwrap();
    json.as_object_mut()
        .unwrap()
        .remove("previous_content_sha256");
    let restored: AgreementArtifact = serde_json::from_value(json).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored, artifact);
}

#[test_case(1; "registered range")]
#[test_case(0xEFFF; "below private range")]
#[test_case(0xFFFF; "future reserved")]
fn unsupported_native_encoding_is_rejected_before_profile_construction(encoding_id: u16) {
    let result = AgreementArtifact::generate(
        &inventory(),
        "example".into(),
        1,
        encoding_id,
        None,
        IdentityDeclarations::default(),
    );
    assert!(matches!(result, Err(ProfileToolError::Invalid(_))));
    let mut artifact = agreement(IdentityDeclarations::default());
    artifact.native.encoding_id = encoding_id;
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

#[test_case(0; "zero")]
#[test_case(0xF000; "first private")]
#[test_case(0xFFFE; "last private")]
fn explicit_encoding_boundaries_are_valid(encoding_id: u16) {
    let artifact = AgreementArtifact::generate(
        &inventory(),
        "example".into(),
        1,
        encoding_id,
        None,
        IdentityDeclarations::default(),
    )
    .unwrap();
    assert_eq!(
        artifact.profile().unwrap().encoding().id(),
        u32::from(encoding_id)
    );
}

#[test_case(0; "old format")]
#[test_case(1; "superseded agreement format")]
#[test_case(99; "future format")]
#[test_case(u64::MAX; "hostile version never truncates")]
fn agreement_versions_are_distinct_from_inventory_and_manifest(version: u64) {
    let artifact = agreement(IdentityDeclarations::default());
    assert_eq!(artifact.format_version, 2);
    assert_eq!(inventory().format_version, 1);
    assert_eq!(manifest(&artifact).format_version, 1);
    let mut json = serde_json::to_value(&artifact).unwrap();
    *json.get_mut("format_version").unwrap() = serde_json::json!(version);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("old-agreement.json");
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(
        matches!(read_artifact::<AgreementArtifact>(&path), Err(ProfileToolError::UnsupportedFormatVersion { found, supported }) if found == version && supported == AGREEMENT_FORMAT_VERSION)
    );
}

#[test]
fn active_inventory_binding_and_catalogue_bounds_cannot_be_rehashed_away() {
    let artifact = agreement(IdentityDeclarations::default());
    let mut changed = artifact.clone();
    changed.inventory_sha256 = "0".repeat(64);
    changed.content_sha256 = content_hash(&changed).unwrap();
    assert!(matches!(
        changed.validate(),
        Err(ProfileToolError::Invalid(
            "active catalogue does not match the inventory hash"
        ))
    ));
    let mut retired = artifact.clone();
    retired.native.types.first_mut().unwrap().state = NativeTypeState::Retired;
    retired.content_sha256 = content_hash(&retired).unwrap();
    assert!(matches!(
        retired.validate(),
        Err(ProfileToolError::Invalid(
            "active catalogue does not match the inventory hash"
        ))
    ));
    let mut oversized = artifact;
    oversized.native.types = vec![oversized.native.types.first().unwrap().clone(); 4097];
    oversized.content_sha256 = content_hash(&oversized).unwrap();
    assert!(matches!(
        oversized.validate(),
        Err(ProfileToolError::Invalid(
            "native catalogue type limit exceeded"
        ))
    ));
    let mut inv = inventory();
    inv.types = vec![inv.types.first().unwrap().clone(); 4097];
    inv.content_sha256 = content_hash(&inv).unwrap();
    assert!(matches!(
        inv.validate(),
        Err(ProfileToolError::Invalid("inventory type limit exceeded"))
    ));
}

#[test_case("base64"; "noncanonical descriptor")]
#[test_case("digest"; "wrong full descriptor hash")]
#[test_case("token"; "wrong short token")]
#[test_case("key"; "wrong name")]
#[test_case("target"; "wrong catalogue target")]
fn retired_descriptors_receive_full_content_checks(field: &str) {
    let first = agreement(IdentityDeclarations::default());
    let empty = InventoryArtifact::from_representations(Target::current(), []).unwrap();
    let mut retired = AgreementArtifact::generate(
        &empty,
        "example".into(),
        2,
        0xF100,
        Some(&first),
        IdentityDeclarations::default(),
    )
    .unwrap();
    let descriptor = &mut retired.native.types.first_mut().unwrap().descriptor;
    match field {
        "base64" => descriptor.canonical_descriptor_base64 = "AA==".into(),
        "digest" => descriptor.descriptor_sha256 = "0".repeat(64),
        "token" => descriptor.native_type_token ^= 1,
        "key" => descriptor.key.name = "other.Signal".into(),
        "target" => retired.target.triple = "other-target".into(),
        _ => unreachable!(),
    }
    retired.content_sha256 = content_hash(&retired).unwrap();
    assert!(retired.validate().is_err());
}

#[test]
fn duplicate_keys_and_noncanonical_catalogue_order_fail() {
    let mut artifact = agreement(IdentityDeclarations::default());
    artifact
        .native
        .types
        .push(artifact.native.types.first().unwrap().clone());
    artifact.content_sha256 = content_hash(&artifact).unwrap();
    assert!(matches!(
        artifact.validate(),
        Err(ProfileToolError::Invalid(
            "duplicate native key or noncanonical catalogue order"
        ))
    ));
    let inv = InventoryArtifact::from_representations(
        Target::current(),
        [
            NativeRepresentation::new("A", 0, 4, 4, NativeByteOrder::native(), vec![]).unwrap(),
            NativeRepresentation::new("B", 0, 4, 4, NativeByteOrder::native(), vec![]).unwrap(),
        ],
    )
    .unwrap();
    let mut artifact = AgreementArtifact::generate(
        &inv,
        "example".into(),
        1,
        0xF100,
        None,
        IdentityDeclarations::default(),
    )
    .unwrap();
    artifact.native.types.reverse();
    artifact.content_sha256 = content_hash(&artifact).unwrap();
    assert!(matches!(
        artifact.validate(),
        Err(ProfileToolError::Invalid(
            "duplicate native key or noncanonical catalogue order"
        ))
    ));
}

#[test]
fn generation_checks_reject_jumps_wrong_domain_and_overflow() {
    let first = agreement(IdentityDeclarations::default());
    assert!(AgreementArtifact::generate(
        &inventory(),
        "example".into(),
        2,
        0xF100,
        None,
        IdentityDeclarations::default()
    )
    .is_err());
    assert!(AgreementArtifact::generate(
        &inventory(),
        "example".into(),
        3,
        0xF100,
        Some(&first),
        IdentityDeclarations::default()
    )
    .is_err());
    assert!(AgreementArtifact::generate(
        &inventory(),
        "other".into(),
        2,
        0xF100,
        Some(&first),
        IdentityDeclarations::default()
    )
    .is_err());
    let mut last = first;
    last.generation = u32::MAX;
    last.previous_content_sha256 = Some("1".repeat(64));
    last.content_sha256 = content_hash(&last).unwrap();
    last.validate().unwrap();
    assert!(AgreementArtifact::generate(
        &inventory(),
        "example".into(),
        1,
        0xF100,
        Some(&last),
        IdentityDeclarations::default()
    )
    .is_err());
}

#[test]
fn private_payload_conflicts_fail_with_all_native_types_retired() {
    let first = agreement(IdentityDeclarations::default());
    let empty = InventoryArtifact::from_representations(Target::current(), []).unwrap();
    let declarations = IdentityDeclarations {
        private_payloads: vec![PrivatePayloadDeclaration {
            encoding_id: 0xF100,
            meaning: "other.bytes".into(),
            owner: "example".into(),
            version: 1,
        }],
        ..Default::default()
    };
    assert!(matches!(
        AgreementArtifact::generate(
            &empty,
            "example".into(),
            2,
            0xF100,
            Some(&first),
            declarations
        ),
        Err(ProfileToolError::Invalid(
            "native encoding conflicts with a private payload declaration"
        ))
    ));
}

#[test_case("encoding_id", serde_json::json!(61696); "no per-type encoding")]
#[test_case("operation", serde_json::json!("signal.event"); "no operation binding")]
fn native_type_entries_reject_removed_fields(field: &str, value: serde_json::Value) {
    let entry = agreement(IdentityDeclarations::default())
        .native
        .types
        .remove(0);
    let mut json = serde_json::to_value(entry).unwrap();
    json.as_object_mut().unwrap().insert(field.into(), value);
    assert!(serde_json::from_value::<NativeTypeEntry>(json).is_err());
}
