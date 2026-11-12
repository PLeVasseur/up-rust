//! Typed entity addressing: a packed instance-0 ID cannot be spelled by accident,
//! and the error for one that is still spelled by hand names the fix.
use std::num::NonZeroU16;
use up_rust::{LocalUriProvider, StaticUriProvider, UEntityId, UUri, UUriError, UUriPartsError};

#[test]
fn first_instance_never_names_instance_zero() {
    let entity = UEntityId::first_instance(0x1234);
    assert_eq!(entity.type_id(), 0x1234);
    assert_eq!(entity.instance(), 1);
    assert_eq!(entity.packed(), 0x0001_1234);
}

#[test]
fn separate_parts_round_trip_through_the_packed_wire_form() {
    let entity = UEntityId::new(0xABCD, 0x0042).expect("nonzero instance");
    assert_eq!(entity.packed(), 0x0042_ABCD);
    assert_eq!(UEntityId::from_packed(entity.packed()), Ok(entity));
}

#[test]
fn zero_instance_is_refused_at_every_entry_point() {
    assert_eq!(
        UEntityId::new(0x1234, 0),
        Err(UUriPartsError::ZeroEntityInstance)
    );
    assert_eq!(
        UEntityId::from_packed(0x0000_1234),
        Err(UUriPartsError::ZeroEntityInstance)
    );
}

#[test]
fn a_bare_type_id_fails_with_an_error_naming_the_fix() {
    // The legacy packed constructors keep working and keep refusing instance 0.
    let packed = UUri::try_from_parts("vehicle", 0x1234, 1, 0x8001).unwrap_err();
    let provider = StaticUriProvider::new("vehicle", 0x1234, 1).unwrap_err();
    for error in [&packed, &provider] {
        assert!(matches!(error, UUriError::ValidationError(_)));
        let message = error.to_string();
        assert!(message.contains("UEntityId::first_instance"), "{message}");
        assert!(message.contains("instance"), "{message}");
    }
    assert!(UUriPartsError::ZeroEntityInstance
        .to_string()
        .contains("UEntityId::new(type_id, instance)"));
}

#[test]
fn typed_constructors_agree_with_packed_ones() {
    let entity = UEntityId::first_instance(0x1234);
    assert_eq!(
        UUri::for_entity("vehicle", entity, 1, 0x8001).unwrap(),
        UUri::try_from_parts("vehicle", 0x0001_1234, 1, 0x8001).unwrap()
    );
    let provider = StaticUriProvider::for_entity("vehicle", entity, 1).unwrap();
    assert_eq!(provider.get_source_uri().uentity_type_id(), 0x1234);
    assert_eq!(provider.get_source_uri().uentity_instance_id(), 1);
    assert_eq!(
        provider.get_resource_uri(0x8001),
        UUri::for_entity("vehicle", entity, 1, 0x8001).unwrap()
    );
}

#[test]
fn entity_ids_work_in_constants() {
    const DOOR: UEntityId = UEntityId::first_instance(0x0100);
    const LOCK: UEntityId = UEntityId::with_instance(0x0100, NonZeroU16::MIN);
    assert_eq!(DOOR, LOCK);
    assert_eq!(DOOR.packed(), 0x0001_0100);
}

#[test]
fn invalid_authority_still_rejects_through_the_typed_constructors() {
    let entity = UEntityId::first_instance(0x1234);
    assert!(UUri::for_entity("vehicle/x", entity, 1, 0x8001).is_err());
    assert!(StaticUriProvider::for_entity("vehicle/x", entity, 1).is_err());
}
