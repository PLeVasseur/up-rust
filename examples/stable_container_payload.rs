/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

use std::{collections::VecDeque, error::Error, io::Cursor, mem::MaybeUninit};

use tokio::sync::Mutex;
use up_rust::{
    payload::StableContainerPayload,
    zero_copy::{
        LoanedPayload, LoanedPayloadUninitMut, PayloadLoanProvenance, UContiguousZeroCopyRxFrame,
        UFrameView, ULoanedContiguousZeroCopyRxFrame, UTxBuffer, UUninitTxBuffer, UZeroCopyRxLease,
        UZeroCopyTransport, UZeroCopyTransportImpl, UZeroCopyUninitTransportExt,
        UZeroCopyUninitTransportImpl, ValidatedTxLoanSpec,
    },
    UCode, UFrameMetadata, UStatus, UUri,
};

#[repr(C)]
#[derive(
    Clone, Copy, Debug, Default, PartialEq, up_rust::StablePayload, up_rust::ByteBackedStablePayload,
)]
#[stable_payload(type_name = "example.vehicle.VehiclePose")]
struct VehiclePose {
    x_m: f32,
    y_m: f32,
    yaw_rad: f32,
}

#[repr(C, align(4))]
struct VehiclePoseStorage([u8; std::mem::size_of::<VehiclePose>()]);

#[repr(C, align(4))]
struct VehiclePoseUninitStorage([MaybeUninit<u8>; std::mem::size_of::<VehiclePose>()]);

struct VehiclePoseFrame {
    metadata: UFrameMetadata,
    storage: VehiclePoseStorage,
}

struct VehiclePoseUninitFrame {
    metadata: UFrameMetadata,
    storage: VehiclePoseUninitStorage,
}

impl UTxBuffer for VehiclePoseFrame {
    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload(&self) -> &[u8] {
        self.storage.0.as_slice()
    }

    fn payload_mut(&mut self) -> &mut [u8] {
        self.storage.0.as_mut_slice()
    }
}

impl UUninitTxBuffer for VehiclePoseUninitFrame {
    type Initialized = VehiclePoseFrame;

    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload_len(&self) -> usize {
        self.storage.0.len()
    }

    fn payload_uninit_mut(&mut self) -> LoanedPayloadUninitMut<'_> {
        // SAFETY:
        // - `self.storage.0` is the exact visible payload range for this example
        //   frame and is exclusively borrowed through `&mut self`.
        unsafe {
            LoanedPayloadUninitMut::new_unchecked(
                self.storage.0.as_mut_slice(),
                PayloadLoanProvenance::OpaqueTransportLoan,
            )
        }
    }

    /// # Safety
    ///
    /// The caller must guarantee every byte in this example's visible payload
    /// range has been initialized before conversion.
    unsafe fn assume_payload_init(self) -> Self::Initialized {
        let mut bytes = [0_u8; std::mem::size_of::<VehiclePose>()];
        for (dst, src) in bytes.iter_mut().zip(self.storage.0) {
            // SAFETY: The trait method caller guarantees each visible payload
            // byte was initialized before conversion.
            *dst = unsafe { src.assume_init() };
        }
        VehiclePoseFrame {
            metadata: self.metadata,
            storage: VehiclePoseStorage(bytes),
        }
    }
}

impl UFrameView for VehiclePoseFrame {
    type PayloadReader<'a>
        = Cursor<&'a [u8]>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = std::iter::Once<&'a [u8]>
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload_len(&self) -> usize {
        self.storage.0.len()
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        Cursor::new(self.storage.0.as_slice())
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        std::iter::once(self.storage.0.as_slice())
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        Some(self.storage.0.as_slice())
    }
}

impl UZeroCopyRxLease for VehiclePoseFrame {}

impl UContiguousZeroCopyRxFrame for VehiclePoseFrame {
    fn contiguous_payload(&self) -> &[u8] {
        self.storage.0.as_slice()
    }
}

impl ULoanedContiguousZeroCopyRxFrame for VehiclePoseFrame {
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, up_rust::payload::UWireError> {
        // SAFETY:
        // - This example frame owns the backing storage and returns a payload
        //   slice borrowed from `&self`, so the slice cannot outlive the frame.
        Ok(unsafe {
            LoanedPayload::new_unchecked(
                self.storage.0.as_slice(),
                PayloadLoanProvenance::OpaqueTransportLoan,
            )
        })
    }
}

#[derive(Default)]
struct StableLoopbackTransport {
    queue: Mutex<VecDeque<VehiclePoseFrame>>,
}

#[async_trait::async_trait]
impl UZeroCopyTransportImpl for StableLoopbackTransport {
    type Tx = VehiclePoseFrame;
    type Rx = VehiclePoseFrame;

    async fn loan_validated_tx(&self, spec: ValidatedTxLoanSpec) -> Result<Self::Tx, UStatus> {
        if spec.payload_len() != std::mem::size_of::<VehiclePose>() {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "unsupported payload length",
            ));
        }
        if spec.payload_alignment() > std::mem::align_of::<VehiclePoseStorage>() {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "unsupported payload alignment",
            ));
        }
        Ok(VehiclePoseFrame {
            metadata: spec.metadata().clone(),
            storage: VehiclePoseStorage([0; std::mem::size_of::<VehiclePose>()]),
        })
    }

    async fn send_validated_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
        self.queue.lock().await.push_back(buffer);
        Ok(())
    }

    async fn receive_validated_zero_copy(
        &self,
        _source_filter: &UUri,
        _sink_filter: Option<&UUri>,
    ) -> Result<Self::Rx, UStatus> {
        self.queue
            .lock()
            .await
            .pop_front()
            .ok_or_else(|| UStatus::fail_with_code(UCode::NOT_FOUND, "no frame available"))
    }
}

#[async_trait::async_trait]
impl UZeroCopyUninitTransportImpl for StableLoopbackTransport {
    type UninitTx = VehiclePoseUninitFrame;

    async fn loan_validated_uninit_tx(
        &self,
        spec: ValidatedTxLoanSpec,
    ) -> Result<Self::UninitTx, UStatus> {
        if spec.payload_len() != std::mem::size_of::<VehiclePose>() {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "unsupported payload length",
            ));
        }
        if spec.payload_alignment() > std::mem::align_of::<VehiclePoseUninitStorage>() {
            return Err(UStatus::fail_with_code(
                UCode::INVALID_ARGUMENT,
                "unsupported payload alignment",
            ));
        }
        Ok(VehiclePoseUninitFrame {
            metadata: spec.metadata().clone(),
            storage: VehiclePoseUninitStorage(
                [const { MaybeUninit::uninit() }; std::mem::size_of::<VehiclePose>()],
            ),
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let topic = UUri::try_from("//my-vehicle/4210/1/9001")?;
    let transport = StableLoopbackTransport::default();

    transport
        .send_uninit_loaned_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>(
            UFrameMetadata::try_publish(topic.clone())?,
            |slot| {
                Ok(slot.write(VehiclePose {
                    x_m: 1.25,
                    y_m: -2.5,
                    yaw_rad: 0.75,
                }))
            },
        )
        .await?;

    let rx = transport.receive_zero_copy(&topic, None).await?;
    let pose = rx.borrow_stable_payload::<VehiclePose>()?;

    assert_eq!(pose.x_m, 1.25);
    println!(
        "stable container pose x={} y={} yaw={}",
        pose.x_m, pose.y_m, pose.yaw_rad
    );
    Ok(())
}
