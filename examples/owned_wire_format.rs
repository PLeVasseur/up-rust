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

use std::{error::Error, sync::Arc};

use up_rust::{
    local_transport::LocalTransport, UDeserializer, UEncoding, UMessageBuilder, UOwnedFrame,
    UOwnedListener, UOwnedTransport, USerializer, UUri, UWireError, WireFormat,
};

#[derive(Debug, PartialEq)]
struct TemperatureReading {
    sensor_id: u16,
    milli_celsius: i32,
}

struct TemperatureWire;

impl WireFormat for TemperatureWire {
    fn name() -> &'static str {
        "demo-temperature-v1"
    }

    fn encoding() -> UEncoding {
        UEncoding::with_schema_ref(
            Self::name(),
            "application/x.demo.temperature",
            "urn:demo:TemperatureReading:v1",
        )
    }
}

impl TemperatureReading {
    const WIRE_LEN: usize = 6;
}

impl USerializer<TemperatureWire> for TemperatureReading {
    fn encoded_len(&self) -> usize {
        Self::WIRE_LEN
    }

    fn serialize_into(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        let actual = dst.len();
        let out = dst
            .get_mut(..Self::WIRE_LEN)
            .ok_or_else(|| UWireError::buffer_too_small(Self::WIRE_LEN, actual))?;
        let (sensor_id, rest) = out.split_at_mut(2);
        let (milli_celsius, _) = rest.split_at_mut(4);
        sensor_id.copy_from_slice(&self.sensor_id.to_be_bytes());
        milli_celsius.copy_from_slice(&self.milli_celsius.to_be_bytes());
        Ok(Self::WIRE_LEN)
    }
}

impl<'a> UDeserializer<'a, TemperatureWire> for TemperatureReading {
    fn deserialize_from(src: &'a [u8]) -> Result<Self, UWireError> {
        if src.len() != Self::WIRE_LEN {
            return Err(UWireError::invalid_payload(format!(
                "expected {} bytes, got {} bytes",
                Self::WIRE_LEN,
                src.len()
            )));
        }
        let sensor_id = u16::from_be_bytes(
            src.get(..2)
                .ok_or_else(|| UWireError::invalid_payload("missing sensor_id"))?
                .try_into()
                .map_err(|_| UWireError::invalid_payload("invalid sensor_id"))?,
        );
        let milli_celsius = i32::from_be_bytes(
            src.get(2..Self::WIRE_LEN)
                .ok_or_else(|| UWireError::invalid_payload("missing milli_celsius"))?
                .try_into()
                .map_err(|_| UWireError::invalid_payload("invalid milli_celsius"))?,
        );
        Ok(Self {
            sensor_id,
            milli_celsius,
        })
    }
}

struct ConsolePrinter;

#[async_trait::async_trait]
impl UOwnedListener for ConsolePrinter {
    async fn on_receive_owned(&self, frame: UOwnedFrame) {
        if let Ok(reading) = frame.deserialize::<TemperatureWire, TemperatureReading>() {
            println!(
                "received sensor {}: {} mC",
                reading.sensor_id, reading.milli_celsius
            );
        }
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn Error>> {
    let topic = UUri::try_from("//my-vehicle/4210/1/B24D")?;
    let transport = LocalTransport::default();
    let listener = Arc::new(ConsolePrinter);

    transport
        .register_owned_listener(&topic, None, listener.clone())
        .await?;

    let reading = TemperatureReading {
        sensor_id: 7,
        milli_celsius: 21_750,
    };
    let frame = UMessageBuilder::publish(topic.clone())
        .build_with_serializable::<TemperatureWire, _>(&reading)?;
    transport.send_owned(frame).await?;

    transport
        .unregister_owned_listener(&topic, None, listener)
        .await?;

    Ok(())
}
