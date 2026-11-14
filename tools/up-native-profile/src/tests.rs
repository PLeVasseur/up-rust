// SPDX-License-Identifier: Apache-2.0
use super::*;
use test_case::test_case;
use up_rust::StablePayloadExt as _;
use up_rust::{NativeByteOrder, NativeRepresentation, NativeTarget, StablePayload};

fn scalar(name: &str) -> NativeRepresentation {
    NativeRepresentation::new(name, 0, 4, 4, NativeByteOrder::native(), vec![]).unwrap()
}

fn inventory(names: &[&str]) -> InventoryArtifact {
    InventoryArtifact::from_representations(
        Target::current(),
        names.iter().map(|name| scalar(name)),
    )
    .unwrap()
}

fn generate(
    inv: &InventoryArtifact,
    previous: Option<&AgreementArtifact>,
) -> Result<AgreementArtifact, ProfileToolError> {
    AgreementArtifact::generate(
        inv,
        "example.vehicle".to_owned(),
        previous.map_or(1, |value| value.generation + 1),
        0xF100,
        previous,
        IdentityDeclarations::default(),
    )
}

fn catalogue(value: &AgreementArtifact) -> Vec<(String, NativeTypeState)> {
    value
        .native
        .types
        .iter()
        .map(|value| (value.descriptor.key.name.clone(), value.state))
        .collect()
}

fn rehash(value: &mut AgreementArtifact) {
    value.content_sha256 = content_hash(value).unwrap();
}

#[test]
fn append_remove_feature_toggle_and_readd_preserve_descriptor_history() {
    let first_inv = inventory(&["Door", "Seat"]);
    let first = generate(&first_inv, None).unwrap();
    let added = generate(&inventory(&["Seat", "Air", "Door"]), Some(&first)).unwrap();
    added.validate_successor(&first).unwrap();
    assert_eq!(
        catalogue(&added),
        [
            ("Air".into(), NativeTypeState::Active),
            ("Door".into(), NativeTypeState::Active),
            ("Seat".into(), NativeTypeState::Active)
        ]
    );
    let removed = generate(&inventory(&["Air", "Door"]), Some(&added)).unwrap();
    let toggled = generate(&inventory(&["Door"]), Some(&removed)).unwrap();
    assert_eq!(
        catalogue(&toggled).first().unwrap().1,
        NativeTypeState::Retired
    );
    assert_eq!(
        catalogue(&toggled).get(2).unwrap().1,
        NativeTypeState::Retired
    );
    let readded = generate(&inventory(&["Window", "Seat", "Door"]), Some(&toggled)).unwrap();
    readded.validate_successor(&toggled).unwrap();
    assert_eq!(
        catalogue(&readded),
        [
            ("Air".into(), NativeTypeState::Retired),
            ("Door".into(), NativeTypeState::Active),
            ("Seat".into(), NativeTypeState::Active),
            ("Window".into(), NativeTypeState::Active)
        ]
    );
    assert_eq!(readded.native.encoding_id, 0xF100);
    for retained in &first.native.types {
        assert_eq!(
            readded
                .native
                .types
                .iter()
                .find(|entry| entry.descriptor.key == retained.descriptor.key)
                .unwrap()
                .descriptor,
            retained.descriptor
        );
    }
    let profile = std::sync::Arc::new(readded.profile().unwrap());
    let agreement =
        up_rust::NativeProfileAgreement::new(std::sync::Arc::clone(&profile), &profile).unwrap();
    assert_eq!(
        agreement.identity_for(&scalar("Air")),
        Err(up_rust::NativeProfileError::UnknownRepresentation)
    );
    let changed =
        NativeRepresentation::new("Door", 0, 8, 8, NativeByteOrder::native(), vec![]).unwrap();
    let changed_inv =
        InventoryArtifact::from_representations(Target::current(), [changed]).unwrap();
    assert!(matches!(
        generate(&changed_inv, Some(&readded)),
        Err(ProfileToolError::Invalid(
            "descriptor changed under an existing name/revision"
        ))
    ));
}

#[test]
fn generation_is_deterministic_under_inventory_input_reordering() {
    let first = generate(&inventory(&["Seat", "Door"]), None).unwrap();
    let second = generate(&inventory(&["Door", "Seat"]), None).unwrap();
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
    assert_eq!(first.content_sha256, second.content_sha256);
}

#[test]
fn retired_descriptors_cannot_be_removed_or_redefined() {
    let first = generate(&inventory(&["Door"]), None).unwrap();
    let retired = generate(&inventory(&[]), Some(&first)).unwrap();
    let mut lost = retired.clone();
    lost.native.types.clear();
    rehash(&mut lost);
    assert!(matches!(
        lost.validate_successor(&first),
        Err(ProfileToolError::Invalid(
            "old native identity removed or redefined"
        ))
    ));
    let mut reassigned = retired.clone();
    reassigned.native.types.first_mut().unwrap().descriptor =
        NativeDescriptor::from_representation(&scalar("Seat"));
    rehash(&mut reassigned);
    assert!(matches!(
        reassigned.validate_successor(&first),
        Err(ProfileToolError::Invalid(
            "old native identity removed or redefined"
        ))
    ));
    let mut redefined = retired.clone();
    redefined.native.types.first_mut().unwrap().descriptor = NativeDescriptor::from_representation(
        &NativeRepresentation::new("Door", 0, 8, 8, NativeByteOrder::native(), vec![]).unwrap(),
    );
    rehash(&mut redefined);
    assert!(matches!(
        redefined.validate_successor(&first),
        Err(ProfileToolError::Invalid(
            "old native identity removed or redefined"
        ))
    ));
    let added = generate(&inventory(&["Air"]), Some(&retired)).unwrap();
    added.validate_successor(&retired).unwrap();
    assert_eq!(
        catalogue(&added),
        [
            ("Air".into(), NativeTypeState::Active),
            ("Door".into(), NativeTypeState::Retired)
        ]
    );
}

#[test]
fn real_short_token_collision_including_retired_entries_is_rejected() {
    let target = Target {
        triple: "x86_64-unknown-linux-gnu".into(),
        pointer_width: 64,
        endianness: "little".into(),
    };
    let make = |name| {
        NativeRepresentation::for_target(
            NativeTarget::new(target.triple.clone(), 64).unwrap(),
            name,
            0,
            4,
            4,
            NativeByteOrder::LittleEndian,
            vec![],
        )
        .unwrap()
    };
    let first = make("collision.23393");
    let second = make("collision.37407");
    assert_ne!(first.canonical_bytes(), second.canonical_bytes());
    assert_eq!(first.token(), second.token());
    let collision =
        InventoryArtifact::from_representations(target.clone(), [first.clone(), second.clone()])
            .unwrap_err();
    assert!(
        matches!(&collision, ProfileToolError::TokenCollision { first, second, .. }
        if first.name == "collision.23393" && second.name == "collision.37407")
    );
    assert!(collision.to_string().contains("collision.23393@0"));
    assert!(collision.to_string().contains("collision.37407@0"));
    let first_inv = InventoryArtifact::from_representations(target.clone(), [first]).unwrap();
    let table = generate(&first_inv, None).unwrap();
    let empty = InventoryArtifact::from_representations(target.clone(), []).unwrap();
    let retired = generate(&empty, Some(&table)).unwrap();
    let second_inv = InventoryArtifact::from_representations(target, [second]).unwrap();
    assert!(matches!(generate(&second_inv, Some(&retired)),
        Err(ProfileToolError::TokenCollision { first, second, .. })
        if first.name == "collision.23393" && second.name == "collision.37407"));
}

#[test_case(0, 2; "old file format")]
#[test_case(1, 1; "old descriptor format")]
#[test_case(1, 99; "unknown descriptor format")]
fn old_or_unknown_versions_are_not_reinterpreted(format: u32, descriptor: u32) {
    let mut value = inventory(&["Door"]);
    value.format_version = format;
    value.descriptor_version = descriptor;
    value.content_sha256 = content_hash(&value).unwrap();
    if format != FORMAT_VERSION {
        assert!(matches!(value.validate(),
            Err(ProfileToolError::UnsupportedFormatVersion { found, supported })
            if found == u64::from(format) && supported == FORMAT_VERSION));
    } else {
        assert!(matches!(
            value.validate(),
            Err(ProfileToolError::Invalid(
                "unsupported inventory/descriptor/token version"
            ))
        ));
    }
}

#[test]
fn duplicate_inventory_bad_chain_and_mislabeled_target_fail() {
    assert!(matches!(
        InventoryArtifact::from_representations(
            Target::current(),
            [scalar("Door"), scalar("Door")]
        ),
        Err(ProfileToolError::Invalid("duplicate inventory key"))
    ));
    let inv = inventory(&["Door", "Seat"]);
    let first = generate(&inv, None).unwrap();
    let mut next = generate(&inv, Some(&first)).unwrap();
    next.previous_content_sha256 = Some("0".repeat(64));
    rehash(&mut next);
    assert!(matches!(
        next.validate_successor(&first),
        Err(ProfileToolError::Invalid(
            "invalid previous-agreement hash/generation/domain/target/encoding chain"
        ))
    ));
    let mut mislabeled = inv;
    mislabeled.target.triple = "different-target".into();
    mislabeled.content_sha256 = content_hash(&mislabeled).unwrap();
    assert!(matches!(
        mislabeled.validate(),
        Err(ProfileToolError::Invalid(
            "inventory target does not match canonical descriptor"
        ))
    ));
}

#[test]
fn private_wire_aliases_require_literals_and_namespace_registration_is_exact() {
    let private = |literal: &str| WireDeclaration {
        namespace: WireNamespace::Wire,
        literal: literal.into(),
        compact_id: 0x9001,
        owner: literal.into(),
        version: 1,
        reference: WireReference::Literal,
    };
    let mut declarations = IdentityDeclarations {
        wires: vec![private("vendor.a"), private("vendor.b")],
        ..Default::default()
    };
    declarations.validate().unwrap();
    declarations.wires.get_mut(1).unwrap().reference = WireReference::RegisteredCompact;
    assert!(matches!(
        declarations.validate(),
        Err(ProfileToolError::Invalid(
            "private/unregistered wire identity requires a literal reference"
        ))
    ));
    let identity = up_rust::wire::UPROTOCOL_NATIVE_WIRE_ID;
    let registered = WireDeclaration {
        namespace: WireNamespace::Wire,
        literal: identity.literal_id().into(),
        compact_id: identity.compact_id(),
        owner: "SDK".into(),
        version: 1,
        reference: WireReference::RegisteredCompact,
    };
    declarations.wires = vec![registered];
    declarations.validate().unwrap();
    declarations.wires.first_mut().unwrap().namespace = WireNamespace::MetadataLayout;
    assert!(matches!(
        declarations.validate(),
        Err(ProfileToolError::Invalid(
            "private/unregistered wire identity requires a literal reference"
        ))
    ));
}

#[test]
fn shared_artifact_keeps_payload_and_wire_namespaces_separate() {
    let declarations = IdentityDeclarations {
        private_payloads: vec![PrivatePayloadDeclaration {
            encoding_id: 0xF100,
            meaning: "example.bytes".into(),
            owner: "example".into(),
            version: 1,
        }],
        wires: vec![WireDeclaration {
            namespace: WireNamespace::Wire,
            literal: "example.wire".into(),
            compact_id: 0xF101,
            owner: "example".into(),
            version: 1,
            reference: WireReference::Literal,
        }],
    };
    let artifact = AgreementArtifact::generate(
        &inventory(&["Door"]),
        "d".into(),
        1,
        0xF101,
        None,
        declarations,
    )
    .unwrap();
    assert_eq!(artifact.native.encoding_id, 0xF101);
    let mut conflict = artifact;
    conflict.native.encoding_id = 0xF100;
    rehash(&mut conflict);
    assert!(matches!(
        conflict.validate(),
        Err(ProfileToolError::Invalid(
            "native encoding conflicts with a private payload declaration"
        ))
    ));
}

#[test]
fn selected_private_wire_must_be_declared_before_admission() {
    use up_rust::{StableContainerWire, UWire};
    let table = generate(&inventory(&["Door"]), None).unwrap();
    table
        .require_wire::<up_rust::UProtocolNativeWire, up_rust::UFrameFieldsMetadataCodec>()
        .unwrap();
    assert!(matches!(
        table.require_wire::<StableContainerWire, up_rust::UFrameFieldsMetadataCodec>(),
        Err(ProfileToolError::Invalid(
            "selected private wire identity has no provisioned literal declaration"
        ))
    ));
    let wires = [
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
    .collect();
    let declared = AgreementArtifact::generate(
        &inventory(&["Door"]),
        "d".into(),
        1,
        0xF100,
        None,
        IdentityDeclarations {
            wires,
            ..Default::default()
        },
    )
    .unwrap();
    declared
        .require_wire::<StableContainerWire, up_rust::UFrameFieldsMetadataCodec>()
        .unwrap();
}

#[test_case(0; "explicit zero")]
#[test_case(0xF100; "private encoding")]
fn shared_encoding_accepts_many_types_without_inference(encoding_id: u16) {
    let inv = inventory(&["Door", "Seat"]);
    let artifact = AgreementArtifact::generate(
        &inv,
        "example.vehicle".into(),
        1,
        encoding_id,
        None,
        IdentityDeclarations::default(),
    )
    .unwrap();
    let profile = std::sync::Arc::new(artifact.profile().unwrap());
    assert_eq!(profile.representations().count(), 2);
    assert_eq!(profile.encoding().id(), u32::from(encoding_id));
    let peer = artifact.profile().unwrap();
    let agreement = up_rust::NativeProfileAgreement::new(profile, &peer).unwrap();
    let door = agreement.identity_for(&scalar("Door")).unwrap();
    let seat = agreement.identity_for(&scalar("Seat")).unwrap();
    assert_eq!(door.encoding(), seat.encoding());
    assert_eq!(door.encoding().id(), u32::from(encoding_id));
    assert_ne!(door.token(), seat.token());
    assert_eq!(
        agreement.verify_representation(door.encoding(), seat.token(), &scalar("Door")),
        Err(up_rust::NativeProfileError::IdentityMismatch)
    );
    assert_eq!(
        agreement.verify_carried(door.encoding(), scalar("Unknown").token()),
        Err(up_rust::NativeProfileError::IdentityMismatch)
    );
    let foreign_encoding = up_rust::PayloadEncoding::private_use(0xF101);
    assert_eq!(
        agreement.verify_carried(foreign_encoding, door.token()),
        Err(up_rust::NativeProfileError::UnknownEncoding(
            foreign_encoding
        ))
    );
    assert_eq!(
        agreement.identity_for(&scalar("Unknown")),
        Err(up_rust::NativeProfileError::UnknownRepresentation)
    );
    let encoded = serde_json::to_value(&artifact).unwrap();
    let native = encoded.get("native").unwrap();
    assert!(native.get("mode").is_none());
    assert!(native
        .get("types")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .all(|entry| entry.get("encoding_id").is_none()));
    let decoded: AgreementArtifact = serde_json::from_value(encoded.clone()).unwrap();
    decoded.validate().unwrap();
    assert_eq!(decoded, artifact);
    let mut old_field = encoded;
    old_field
        .get_mut("native")
        .and_then(serde_json::Value::as_object_mut)
        .unwrap()
        .insert("operation".into(), serde_json::json!("door.event"));
    assert!(serde_json::from_value::<AgreementArtifact>(old_field).is_err());
}

#[test_case("mode", serde_json::json!("table"); "old mode")]
#[test_case("allocation_band", serde_json::json!({"first": 61696, "last": 63487}); "old band")]
#[test_case("allocations", serde_json::json!([]); "old allocations")]
fn removed_native_fields_are_rejected_at_the_file_boundary(
    field: &str,
    removed: serde_json::Value,
) {
    let artifact = generate(&inventory(&["Door"]), None).unwrap();
    let mut value = serde_json::to_value(artifact).unwrap();
    value
        .get_mut("native")
        .and_then(serde_json::Value::as_object_mut)
        .unwrap()
        .insert(field.into(), removed);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unsupported.json");
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(
        matches!(read_artifact::<AgreementArtifact>(&path), Err(ProfileToolError::Json(error)) if error.to_string().contains("unknown field"))
    );
}

#[test_case(0; "old format")]
#[test_case(99; "future format")]
#[test_case(u64::MAX; "hostile version never truncates")]
fn unsupported_format_versions_are_named_at_the_file_boundary(version: u64) {
    let mut value = serde_json::to_value(inventory(&["Door"])).unwrap();
    *value.get_mut("format_version").unwrap() = serde_json::json!(version);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unsupported.json");
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(matches!(read_artifact::<InventoryArtifact>(&path),
        Err(ProfileToolError::UnsupportedFormatVersion { found, supported })
        if found == version && supported == FORMAT_VERSION));
}

#[test]
fn bounded_version_inspection_does_not_hide_duplicate_fields() {
    let bytes = serde_json::to_string(&inventory(&["Door"])).unwrap();
    let duplicated = format!(
        "{{\"format_version\":1,{}",
        bytes.strip_prefix('{').unwrap()
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("duplicate.json");
    std::fs::write(&path, duplicated).unwrap();
    assert!(matches!(
        read_artifact::<InventoryArtifact>(&path),
        Err(ProfileToolError::Json(_))
    ));
}

#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "test.tool.Door", inventory)]
struct Door {
    value: u32,
}

fn manifest(artifact: &AgreementArtifact) -> DeploymentManifest {
    let endpoints = ["sender", "receiver"]
        .into_iter()
        .map(|id| DeploymentEndpoint {
            id: id.into(),
            agreement_path: format!("{id}.json"),
            content_sha256: artifact.content_sha256.clone(),
            target: artifact.target.clone(),
            domain: artifact.domain.clone(),
            generation: artifact.generation,
        })
        .collect();
    DeploymentManifest::new(
        endpoints,
        vec![DeploymentRoute {
            id: "direct".into(),
            endpoints: vec!["sender".into(), "receiver".into()],
        }],
    )
    .unwrap()
}

#[test]
fn direct_pair_requires_independent_peer_files_full_identity_and_actual_linked_types() {
    let root = tempfile::tempdir().unwrap();
    let key = TypeKey {
        name: Door::TYPE_NAME.into(),
        revision: Door::REVISION,
    };
    let inventory = InventoryArtifact::from_linked(std::slice::from_ref(&key)).unwrap();
    let table = generate(&inventory, None).unwrap();
    write_artifact(&root.path().join("sender.json"), &table).unwrap();
    write_artifact(&root.path().join("receiver.json"), &table).unwrap();
    assert!(
        matches!(write_artifact(&root.path().join("sender.json"), &table),
        Err(ProfileToolError::Io { path, source })
        if path == root.path().join("sender.json") && source.kind() == std::io::ErrorKind::AlreadyExists)
    );
    let loaded = LoadedDeployment::load(root.path(), manifest(&table)).unwrap();
    loaded.require_local_types("sender", &[key]).unwrap();
    let agreement = loaded.agreement("direct", "sender", "receiver").unwrap();
    assert!(matches!(
        loaded.agreement("direct", "sender", "sender"),
        Err(ProfileToolError::Invalid(
            "local and peer must be distinct declared route members"
        ))
    ));
    assert!(matches!(
        loaded.agreement("unknown", "sender", "receiver"),
        Err(ProfileToolError::Invalid("unknown deployment route"))
    ));
    assert!(matches!(
        loaded.require_local_types(
            "sender",
            &[TypeKey {
                name: "not.linked".into(),
                revision: 0
            }]
        ),
        Err(ProfileToolError::Invalid(
            "required application type is not linked"
        ))
    ));
    std::fs::remove_file(root.path().join("receiver.json")).unwrap();
    assert!(
        matches!(LoadedDeployment::load(root.path(), manifest(&table)),
        Err(ProfileToolError::Io { path, source })
        if path == root.path().join("receiver.json") && source.kind() == std::io::ErrorKind::NotFound)
    );
    // The already admitted immutable generation remains valid after file removal.
    assert!(agreement
        .identity_for(&Door::native_representation())
        .is_ok());
    assert!(matches!(
        DeploymentManifest::new(vec![], vec![]),
        Err(ProfileToolError::Invalid(
            "deployment membership limit exceeded"
        ))
    ));
}

#[test_case("hash"; "wrong full content")]
#[test_case("domain"; "wrong domain")]
#[test_case("generation"; "wrong generation")]
#[test_case("target"; "wrong target")]
fn independent_manifest_expectations_reject_disagreement(field: &str) {
    let root = tempfile::tempdir().unwrap();
    let table = generate(&inventory(&["Door"]), None).unwrap();
    write_artifact(&root.path().join("sender.json"), &table).unwrap();
    write_artifact(&root.path().join("receiver.json"), &table).unwrap();
    let mut expected = manifest(&table);
    let endpoint = expected.endpoints.first_mut().unwrap();
    match field {
        "hash" => endpoint.content_sha256 = "0".repeat(64),
        "domain" => endpoint.domain = "other".into(),
        "generation" => endpoint.generation += 1,
        "target" => endpoint.target.triple = "other-target".into(),
        _ => unreachable!(),
    }
    expected.content_sha256 = content_hash(&expected).unwrap();
    assert!(matches!(
        LoadedDeployment::load(root.path(), expected),
        Err(ProfileToolError::Invalid(
            "loaded agreement differs from independently provisioned expectation"
        ))
    ));
}

#[test]
fn individually_valid_endpoint_artifacts_must_agree_across_the_whole_route() {
    let root = tempfile::tempdir().unwrap();
    let local = generate(&inventory(&["Door"]), None).unwrap();
    let peer = generate(&inventory(&["Seat"]), None).unwrap();
    write_artifact(&root.path().join("sender.json"), &local).unwrap();
    write_artifact(&root.path().join("receiver.json"), &peer).unwrap();
    let mut expected = manifest(&local);
    expected
        .endpoints
        .iter_mut()
        .find(|value| value.id == "receiver")
        .unwrap()
        .content_sha256 = peer.content_sha256;
    expected.content_sha256 = content_hash(&expected).unwrap();
    // Each file matches its own expectation, but route-wide identity differs.
    assert!(matches!(
        LoadedDeployment::load(root.path(), expected),
        Err(ProfileToolError::Invalid(
            "declared route endpoints disagree on full agreement content"
        ))
    ));
}
