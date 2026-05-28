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

use std::{collections::VecDeque, error::Error};

use tokio::sync::Mutex;
use up_rust::{
    payload::{PayloadFormat, UDeserializer, USerializer, UWireError},
    zero_copy::{
        UContiguousZeroCopyRxFrame, UVecTxBuffer, UZeroCopyTransport, UZeroCopyTransportExt,
    },
    PayloadEncoding, UCode, UFrameMetadata, UOwnedFrame, UStatus, UTxLoanSpec, UUri,
};

#[derive(Debug, PartialEq)]
struct ImageView<'a> {
    width: u16,
    height: u16,
    pixels: &'a [u8],
}

struct ImagePayload;

impl PayloadFormat for ImagePayload {
    fn name() -> &'static str {
        "demo-image-v1"
    }

    fn encoding() -> PayloadEncoding {
        PayloadEncoding::custom(Self::name(), "application/x.demo.image")
    }
}

impl USerializer<ImagePayload> for ImageView<'_> {
    fn encoded_len(&self) -> usize {
        4 + self.pixels.len()
    }

    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        let expected = self.encoded_len();
        let actual = dst.len();
        let out = dst
            .get_mut(..expected)
            .ok_or_else(|| UWireError::buffer_too_small(expected, actual))?;
        let (header, pixel_dst) = out.split_at_mut(4);
        let (width, height) = header.split_at_mut(2);
        width.copy_from_slice(&self.width.to_be_bytes());
        height.copy_from_slice(&self.height.to_be_bytes());
        pixel_dst.copy_from_slice(self.pixels);
        Ok(expected)
    }
}

impl<'a> UDeserializer<'a, ImagePayload> for ImageView<'a> {
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
        let header = src
            .get(..4)
            .ok_or_else(|| UWireError::invalid_payload("image header is missing"))?;
        let width = u16::from_be_bytes(
            header
                .get(..2)
                .ok_or_else(|| UWireError::invalid_payload("width is missing"))?
                .try_into()
                .map_err(|_| UWireError::invalid_payload("width is invalid"))?,
        );
        let height = u16::from_be_bytes(
            header
                .get(2..4)
                .ok_or_else(|| UWireError::invalid_payload("height is missing"))?
                .try_into()
                .map_err(|_| UWireError::invalid_payload("height is invalid"))?,
        );
        let pixels = src
            .get(4..)
            .ok_or_else(|| UWireError::invalid_payload("pixels are missing"))?;
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
}

#[derive(Default)]
struct LoopbackZeroCopyTransport {
    queue: Mutex<VecDeque<UOwnedFrame>>,
}

#[async_trait::async_trait]
impl UZeroCopyTransport for LoopbackZeroCopyTransport {
    type Tx = UVecTxBuffer;
    type Rx = UOwnedFrame;

    async fn loan_tx(&self, spec: UTxLoanSpec) -> Result<Self::Tx, UStatus> {
        UVecTxBuffer::with_alignment(
            spec.metadata().clone(),
            spec.payload_len(),
            spec.payload_alignment(),
        )
        .map_err(UStatus::from)
    }

    async fn send_zero_copy(&self, buffer: Self::Tx) -> Result<(), UStatus> {
        self.queue.lock().await.push_back(buffer.into_frame());
        Ok(())
    }

    async fn receive_zero_copy(
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

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    let topic = UUri::try_from("//my-vehicle/4210/1/9000")?;
    let transport = LoopbackZeroCopyTransport::default();
    let pixels = [10_u8, 20, 30, 40];
    let image = ImageView {
        width: 2,
        height: 2,
        pixels: &pixels,
    };

    transport
        .send_serialized_zero_copy::<ImagePayload, _>(
            UFrameMetadata::publish(topic.clone()),
            &image,
        )
        .await?;

    let rx = transport.receive_zero_copy(&topic, None).await?;
    let decoded: ImageView<'_> = rx.deserialize_borrowed::<ImagePayload, _>()?;
    assert_eq!(decoded, image);
    println!(
        "received {}x{} image with {} borrowed bytes",
        decoded.width,
        decoded.height,
        decoded.pixels.len()
    );

    Ok(())
}
