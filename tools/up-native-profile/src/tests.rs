// SPDX-License-Identifier: Apache-2.0
use super::*;
use test_case::test_case;
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
    AgreementArtifact::generate_table(
        inv,
        "example.vehicle".to_owned(),
        previous.map_or(1, |value| value.generation + 1),
        AllocationBand::default(),
        previous,
        &[],
        IdentityDeclarations::default(),
    )
}

fn assignments(value: &AgreementArtifact) -> Vec<(String, u16, AllocationState)> {
    match &value.native {
        NativeMode::Table { allocations, .. } => allocations
            .iter()
            .map(|value| {
                (
                    value.descriptor.key.name.clone(),
                    value.encoding_id,
                    value.state,
                )
            })
            .collect(),
        NativeMode::ContractDefined { .. } => panic!("expected table"),
    }
}

fn rehash(value: &mut AgreementArtifact) {
    value.content_sha256 = content_hash(value).unwrap();
}

#[test]
fn append_remove_feature_toggle_and_readd_preserve_identity_reservations() {
    let first_inv = inventory(&["Door", "Seat"]);
    let seeds = [
        SeedAllocation {
            key: TypeKey {
                name: "Door".into(),
                revision: 0,
            },
            encoding_id: 0xF101,
        },
        SeedAllocation {
            key: TypeKey {
                name: "Seat".into(),
                revision: 0,
            },
            encoding_id: 0xF102,
        },
    ];
    let first = AgreementArtifact::generate_table(
        &first_inv,
        "example.vehicle".into(),
        1,
        AllocationBand::default(),
        None,
        &seeds,
        IdentityDeclarations::default(),
    )
    .unwrap();
    let added = generate(&inventory(&["Seat", "Air", "Door"]), Some(&first)).unwrap();
    added.validate_successor(&first).unwrap();
    assert_eq!(
        assignments(&added),
        [
            ("Door".into(), 0xF101, AllocationState::Active),
            ("Seat".into(), 0xF102, AllocationState::Active),
            ("Air".into(), 0xF103, AllocationState::Active)
        ]
    );
    let removed = generate(&inventory(&["Air", "Door"]), Some(&added)).unwrap();
    let toggled = generate(&inventory(&["Door"]), Some(&removed)).unwrap();
    assert_eq!(
        assignments(&toggled).get(1).unwrap().2,
        AllocationState::Retired
    );
    assert_eq!(
        assignments(&toggled).get(2).unwrap().2,
        AllocationState::Retired
    );
    let readded = generate(&inventory(&["Window", "Seat", "Door"]), Some(&toggled)).unwrap();
    readded.validate_successor(&toggled).unwrap();
    assert_eq!(
        assignments(&readded),
        [
            ("Door".into(), 0xF101, AllocationState::Active),
            ("Seat".into(), 0xF102, AllocationState::Active),
            ("Air".into(), 0xF103, AllocationState::Retired),
            ("Window".into(), 0xF104, AllocationState::Active)
        ]
    );
    let changed =
        NativeRepresentation::new("Door", 0, 8, 8, NativeByteOrder::native(), vec![]).unwrap();
    let changed_inv =
        InventoryArtifact::from_representations(Target::current(), [changed]).unwrap();
    assert!(matches!(
        generate(&changed_inv, Some(&readded)),
        Err(ProfileToolError::Invalid(
            "descriptor changed under an existing assignment"
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
fn retired_reservations_cannot_be_removed_reassigned_or_filled_below_high_water() {
    let first = AgreementArtifact::generate_table(
        &inventory(&["Door"]),
        "example.vehicle".into(),
        1,
        AllocationBand::default(),
        None,
        &[SeedAllocation {
            key: TypeKey {
                name: "Door".into(),
                revision: 0,
            },
            encoding_id: 0xF102,
        }],
        IdentityDeclarations::default(),
    )
    .unwrap();
    let retired = generate(&inventory(&[]), Some(&first)).unwrap();
    let mut lost = retired.clone();
    if let NativeMode::Table { allocations, .. } = &mut lost.native {
        allocations.clear();
    }
    rehash(&mut lost);
    assert!(matches!(
        lost.validate_successor(&first),
        Err(ProfileToolError::Invalid(
            "old reservation removed or reassigned"
        ))
    ));
    let mut reassigned = retired.clone();
    if let NativeMode::Table { allocations, .. } = &mut reassigned.native {
        allocations.first_mut().unwrap().descriptor =
            NativeDescriptor::from_representation(&scalar("Seat"));
    }
    rehash(&mut reassigned);
    assert!(matches!(
        reassigned.validate_successor(&first),
        Err(ProfileToolError::Invalid(
            "old reservation removed or reassigned"
        ))
    ));
    let mut gap = retired;
    if let NativeMode::Table { allocations, .. } = &mut gap.native {
        allocations.insert(
            0,
            NativeAllocation {
                encoding_id: 0xF100,
                state: AllocationState::Active,
                descriptor: NativeDescriptor::from_representation(&scalar("Seat")),
            },
        );
    }
    rehash(&mut gap);
    assert!(matches!(
        gap.validate_successor(&first),
        Err(ProfileToolError::Invalid(
            "new assignments must append after the prior high-water mark"
        ))
    ));
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
    assert!(matches!(
        value.validate(),
        Err(ProfileToolError::Invalid(
            "unsupported inventory/descriptor/token version"
        ))
    ));
}

#[test]
fn duplicate_inventory_exhaustion_bad_chain_and_mislabeled_target_fail() {
    assert!(matches!(
        InventoryArtifact::from_representations(
            Target::current(),
            [scalar("Door"), scalar("Door")]
        ),
        Err(ProfileToolError::Invalid("duplicate inventory key"))
    ));
    let inv = inventory(&["Door", "Seat"]);
    assert!(matches!(
        AgreementArtifact::generate_table(
            &inv,
            "d".into(),
            1,
            AllocationBand {
                first: 0xF101,
                last: 0xF101
            },
            None,
            &[],
            IdentityDeclarations::default()
        ),
        Err(ProfileToolError::Invalid(
            "native allocation band exhausted"
        ))
    ));
    let first = generate(&inv, None).unwrap();
    let mut next = generate(&inv, Some(&first)).unwrap();
    next.previous_content_sha256 = Some("0".repeat(64));
    rehash(&mut next);
    assert!(matches!(
        next.validate_successor(&first),
        Err(ProfileToolError::Invalid(
            "invalid previous-ledger hash/generation chain"
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
    let table = AgreementArtifact::generate_table(
        &inventory(&["Door"]),
        "d".into(),
        1,
        AllocationBand::default(),
        None,
        &[],
        declarations,
    )
    .unwrap();
    // Payload F100 is reserved; the separate wire alias F101 consumes no payload ID.
    assert_eq!(assignments(&table).first().unwrap().1, 0xF101);
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
    let declared = AgreementArtifact::generate_table(
        &inventory(&["Door"]),
        "d".into(),
        1,
        AllocationBand::default(),
        None,
        &[],
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

#[test]
fn explicit_contract_is_not_a_missing_table_fallback() {
    let inv = inventory(&["Door", "Seat"]);
    let contract = AgreementArtifact::generate_contract(
        &inv,
        "example.vehicle".into(),
        "door.event".into(),
        &TypeKey {
            name: "Door".into(),
            revision: 0,
        },
        IdentityDeclarations::default(),
    )
    .unwrap();
    let profile = std::sync::Arc::new(contract.profile().unwrap());
    let peer = contract.profile().unwrap();
    let agreement = up_rust::NativeProfileAgreement::new(profile, &peer).unwrap();
    assert_eq!(
        agreement.identity_for(&scalar("Door")).unwrap().encoding(),
        up_rust::PayloadEncoding::CONTRACT_DEFINED
    );
    assert_eq!(
        agreement.identity_for(&scalar("Seat")),
        Err(up_rust::NativeProfileError::UnknownRepresentation)
    );
    assert!(matches!(
        generate(&inventory(&["Seat"]), Some(&contract)),
        Err(ProfileToolError::Invalid(
            "a table successor requires a table predecessor"
        ))
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
