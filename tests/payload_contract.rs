#![cfg(feature = "payload-contract-fixtures")]

/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

use std::mem;

use up_rust::bench_fixtures::payload_contract::{self as fixtures, *};
use up_rust::{
    ByteBackedStablePayload, LoanUninitPayload, StableContainerWireFormat, UWirePayload,
};

#[cfg(feature = "test-util")]
use up_rust::{
    InMemoryZeroCopyTransport, UFrameMetadata, ULoanedContiguousZeroCopyRxFrame, UMessageBuilder,
    UUri, UZeroCopyTransport, UZeroCopyUninitTransportExt,
};

#[cfg(feature = "test-util")]
fn topic(resource_id: u16) -> UUri {
    UUri::try_from_parts("payload-contract-test", 0x4210, 1, resource_id).expect("valid test URI")
}

#[cfg(feature = "test-util")]
fn metadata(resource_id: u16) -> UFrameMetadata {
    let message = UMessageBuilder::publish(topic(resource_id))
        .build()
        .expect("valid publish metadata");
    UFrameMetadata::new_unchecked(message.attributes().clone(), None)
}

#[test]
fn payload_contract_manifest_is_canonical_and_hashable() {
    let manifest = fixtures::fixture_manifest();
    assert_eq!(manifest.suite_family, "payload-contract-representative");
    assert_eq!(manifest.fixture_version, PAYLOAD_CONTRACT_FIXTURE_VERSION);
    assert_eq!(manifest.cases.len(), fixtures::all_cases().count());

    let json = fixtures::fixture_manifest_json_canonical();
    assert!(json.contains("\"suite_family\":\"payload-contract-representative\""));
    assert_eq!(fixtures::fixture_manifest_sha256_hex().len(), 64);
}

#[test]
fn payload_contract_representative_constants_and_sizes_match_contract() {
    assert_eq!(SOMEIP_IPV4_UDP_SINGLE_MTU_BUDGET, 1_456);
    assert_eq!(SOMEIP_MTU_SIGNAL_SAMPLE_COUNT, 59);
    assert_eq!(mem::size_of::<SomeIpSignalBatchMtuV1>(), 1_472);
    assert_eq!(RADAR_ARS548_MAX_DETECTIONS, 800);
    assert_eq!(
        mem::size_of::<Ars548DetectionV1>(),
        RADAR_ARS548_DETECTION_RECORD_BYTES
    );
    assert_eq!(RADAR_ARS548_DETECTION_ARRAY_BYTES, 35_200);
    assert_eq!(RADAR_ARS548_DETECTION_MESSAGE_BYTES, 35_336);
    assert_eq!(RADAR_ARS548_MAX_TRACKED_OBJECTS, 50);

    #[cfg(feature = "payload-contract-large-fixtures")]
    {
        assert_eq!(
            mem::size_of::<LidarPointXyzircaedtV1>(),
            LIDAR_XYZIRCAEDT_POINT_BYTES
        );
        assert_eq!(LIDAR_HESAI_AT128_WIDTH, 1_200);
        assert_eq!(LIDAR_HESAI_AT128_HEIGHT, 128);
        assert_eq!(LIDAR_HESAI_AT128_POINT_COUNT, 153_600);
        assert_eq!(LIDAR_HESAI_AT128_POINTS_BYTES, 4_915_200);
        assert_eq!(LIDAR_HESAI_AT128_ROW_STEP_BYTES, 38_400);
        assert_eq!(mem::size_of::<LidarPointCloudHesaiAt128V1>(), 4_915_328);
        assert_eq!(CAMERA_BAYER_RGGB12P_BYTES, 12_441_600);
        assert_eq!(CAMERA_BAYER_RGGB12P_STRIDE_BYTES, 5_760);
        assert_eq!(BITS_PER_SAMPLE_12, 12);
        assert_eq!(CAMERA_CARLA_BGRA32_BYTES, 33_177_600);
        assert_eq!(mem::size_of::<CameraBayerRggb12pFrame8mpV1>(), 12_441_784);
    }
}

#[test]
fn payload_contract_stable_fixtures_satisfy_selected_wire_no_zero_bounds() {
    fn assert_selected_wire_no_zero<T>()
    where
        T: ByteBackedStablePayload,
        StableContainerWireFormat: UWirePayload<T>,
        <StableContainerWireFormat as UWirePayload<T>>::Codec: LoanUninitPayload<T>,
    {
    }

    assert_selected_wire_no_zero::<CanClassicFrameV1>();
    assert_selected_wire_no_zero::<CanFdFrameV1>();
    assert_selected_wire_no_zero::<SomeIpSignalBatchMtuV1>();
    assert_selected_wire_no_zero::<StreamChunk4kV1>();
    assert_selected_wire_no_zero::<RadarDetectionListArs548V1>();
    assert_selected_wire_no_zero::<StreamChunk64kV1>();

    #[cfg(feature = "payload-contract-large-fixtures")]
    {
        assert_selected_wire_no_zero::<LidarPointCloudHesaiAt128V1>();
        assert_selected_wire_no_zero::<CameraBayerRggb12pFrame8mpV1>();
    }
}

#[test]
fn payload_contract_protobuf_fixtures_round_trip_and_validate() {
    for case in fixtures::all_cases() {
        let bytes = fixtures::protobuf_encoded_bytes_for(case, 11).expect("protobuf encodes");
        assert_eq!(bytes.len(), fixtures::protobuf_encoded_len(case, 11));
        fixtures::validate_protobuf_bytes(case, 11, &bytes)
            .unwrap_or_else(|_| panic!("{}", case.name()));
    }
}

#[test]
fn payload_contract_stable_owned_fixtures_validate_representative_and_exact_bytes() {
    for case in fixtures::all_cases() {
        let fixture = fixtures::stable_owned_fixture_for(case, 17)
            .unwrap_or_else(|_| panic!("{}", case.name()));
        assert_eq!(
            fixture.stable_type_name,
            fixtures::stable_payload_type_name(case)
        );
        assert_eq!(
            fixture.stable_transport_len,
            fixtures::stable_payload_len(case)
        );
        assert_eq!(fixture.stable_align, fixtures::stable_payload_align(case));
        fixtures::validate_stable_owned_bytes(case, 17, Some(&fixture.encoding), &fixture.bytes)
            .unwrap_or_else(|_| panic!("{}", case.name()));
        fixtures::validate_stable_owned_bytes_exact(
            case,
            17,
            Some(&fixture.encoding),
            &fixture.bytes,
        )
        .unwrap_or_else(|_| panic!("{}", case.name()));
    }
}

#[tokio::test]
#[cfg(feature = "test-util")]
async fn payload_contract_stable_fixtures_round_trip_through_in_memory_zero_copy_transport() {
    for case in fixtures::all_cases() {
        let transport = InMemoryZeroCopyTransport::default();
        send_case(&transport, case, 23).await;
        let resource_id = 0x9000 + case.case_id() as u16;
        let rx = transport
            .receive_zero_copy(&topic(resource_id), None)
            .await
            .expect("receive succeeds");
        validate_rx_case(&rx, case, 23);
    }
}

#[cfg(feature = "test-util")]
async fn send_case(
    transport: &InMemoryZeroCopyTransport,
    case: &PayloadContractCase,
    sequence: u32,
) {
    match case.kind() {
        PayloadContractCaseKind::CanClassicMax => transport
            .send_uninit_stable_payload_as::<CanClassicFrameV1>(
                metadata(0x9000 + case.case_id() as u16),
                |init| fixtures::init_can_classic_max(init, sequence),
            )
            .await
            .expect("send CAN Classic"),
        PayloadContractCaseKind::CanFdMax => transport
            .send_uninit_stable_payload_as::<CanFdFrameV1>(
                metadata(0x9000 + case.case_id() as u16),
                |init| fixtures::init_can_fd_max(init, sequence),
            )
            .await
            .expect("send CAN FD"),
        PayloadContractCaseKind::SomeIpSingleMtu => transport
            .send_uninit_stable_payload_as::<SomeIpSignalBatchMtuV1>(
                metadata(0x9000 + case.case_id() as u16),
                |init| fixtures::init_someip_single_mtu(init, sequence),
            )
            .await
            .expect("send SOME/IP"),
        PayloadContractCaseKind::Streamer4k => transport
            .send_uninit_stable_payload_as::<StreamChunk4kV1>(
                metadata(0x9000 + case.case_id() as u16),
                |init| fixtures::init_streamer_4k(init, sequence),
            )
            .await
            .expect("send stream 4K"),
        PayloadContractCaseKind::RadarArs548DetectionList => transport
            .send_uninit_stable_payload_as::<RadarDetectionListArs548V1>(
                metadata(0x9000 + case.case_id() as u16),
                |init| fixtures::init_radar_ars548_detection_list(init, sequence),
            )
            .await
            .expect("send radar"),
        PayloadContractCaseKind::Streamer64k => transport
            .send_uninit_stable_payload_as::<StreamChunk64kV1>(
                metadata(0x9000 + case.case_id() as u16),
                |init| fixtures::init_streamer_64k(init, sequence),
            )
            .await
            .expect("send stream 64K"),
        #[cfg(feature = "payload-contract-large-fixtures")]
        PayloadContractCaseKind::LidarHesaiAt128PointCloud => transport
            .send_uninit_stable_payload_as::<LidarPointCloudHesaiAt128V1>(
                metadata(0x9000 + case.case_id() as u16),
                |init| fixtures::init_lidar_hesai_at128_point_cloud(init, sequence),
            )
            .await
            .expect("send LiDAR"),
        #[cfg(feature = "payload-contract-large-fixtures")]
        PayloadContractCaseKind::Camera8mpBayerRggb12p => transport
            .send_uninit_stable_payload_as::<CameraBayerRggb12pFrame8mpV1>(
                metadata(0x9000 + case.case_id() as u16),
                |init| fixtures::init_camera_8mp_bayer_rggb12p(init, sequence),
            )
            .await
            .expect("send camera"),
        #[cfg(feature = "payload-contract-simulator-fixtures")]
        PayloadContractCaseKind::Camera8mpCarlaBgra32 => unreachable!(),
    }
}

#[cfg(feature = "test-util")]
fn validate_rx_case(
    rx: &impl ULoanedContiguousZeroCopyRxFrame,
    case: &PayloadContractCase,
    sequence: u32,
) {
    match case.kind() {
        PayloadContractCaseKind::CanClassicMax => fixtures::validate_stable_payload(
            case,
            sequence,
            rx.borrow_stable_payload::<CanClassicFrameV1>()
                .expect("borrow CAN Classic"),
        )
        .expect("validate CAN Classic"),
        PayloadContractCaseKind::CanFdMax => fixtures::validate_stable_payload(
            case,
            sequence,
            rx.borrow_stable_payload::<CanFdFrameV1>()
                .expect("borrow CAN FD"),
        )
        .expect("validate CAN FD"),
        PayloadContractCaseKind::SomeIpSingleMtu => fixtures::validate_stable_payload(
            case,
            sequence,
            rx.borrow_stable_payload::<SomeIpSignalBatchMtuV1>()
                .expect("borrow SOME/IP"),
        )
        .expect("validate SOME/IP"),
        PayloadContractCaseKind::Streamer4k => fixtures::validate_stable_payload(
            case,
            sequence,
            rx.borrow_stable_payload::<StreamChunk4kV1>()
                .expect("borrow stream 4K"),
        )
        .expect("validate stream 4K"),
        PayloadContractCaseKind::RadarArs548DetectionList => fixtures::validate_stable_payload(
            case,
            sequence,
            rx.borrow_stable_payload::<RadarDetectionListArs548V1>()
                .expect("borrow radar"),
        )
        .expect("validate radar"),
        PayloadContractCaseKind::Streamer64k => fixtures::validate_stable_payload(
            case,
            sequence,
            rx.borrow_stable_payload::<StreamChunk64kV1>()
                .expect("borrow stream 64K"),
        )
        .expect("validate stream 64K"),
        #[cfg(feature = "payload-contract-large-fixtures")]
        PayloadContractCaseKind::LidarHesaiAt128PointCloud => fixtures::validate_stable_payload(
            case,
            sequence,
            rx.borrow_stable_payload::<LidarPointCloudHesaiAt128V1>()
                .expect("borrow LiDAR"),
        )
        .expect("validate LiDAR"),
        #[cfg(feature = "payload-contract-large-fixtures")]
        PayloadContractCaseKind::Camera8mpBayerRggb12p => fixtures::validate_stable_payload(
            case,
            sequence,
            rx.borrow_stable_payload::<CameraBayerRggb12pFrame8mpV1>()
                .expect("borrow camera"),
        )
        .expect("validate camera"),
        #[cfg(feature = "payload-contract-simulator-fixtures")]
        PayloadContractCaseKind::Camera8mpCarlaBgra32 => unreachable!(),
    }
}
