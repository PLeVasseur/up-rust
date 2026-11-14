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

//! Fixed-size wires for contract tests.

use up_rust_core::uattributes::PayloadEncoding;
use up_rust_core::wire::{
    EncodedSize, PayloadWriter, Ready, RuntimeHeap, StartupSlot, Wire, WireError, WireIdentity,
    WireInfo, WirePayloadIdentity,
};

/// A small fixed-size value for contract tests.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DoorState {
    /// Door identifier.
    pub id: u32,
    /// Whether it is open.
    pub open: u8,
}

/// A private payload encoding for [`DoorWire`].
pub const DOOR_ENCODING: PayloadEncoding = PayloadEncoding::private_use(0xF001);
/// A private payload encoding for [`LittleDoorWire`].
pub const LITTLE_DOOR_ENCODING: PayloadEncoding = PayloadEncoding::private_use(0xF002);
/// Payload family of [`DoorWire`].
pub const DOOR_FAMILY_ID: WireIdentity = WireIdentity::new("test.door-family", 0x8A02);
const DOOR_WIRE_ID: WireIdentity = WireIdentity::new("test.door-wire", 0x8A01);
const LITTLE_DOOR_WIRE_ID: WireIdentity = WireIdentity::new("test.little-door-wire", 0x8A03);
const LITTLE_DOOR_FAMILY_ID: WireIdentity = WireIdentity::new("test.little-door-family", 0x8A04);

/// A fixed-size, heap-free wire for [`DoorState`]: big-endian ID, then the state byte.
#[derive(Clone, Copy, Debug, Default)]
pub struct DoorWire;

impl Wire<DoorState> for DoorWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "door-wire",
            DOOR_WIRE_ID,
            DOOR_FAMILY_ID,
            WirePayloadIdentity::serialized(DOOR_ENCODING),
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
        .max_encoded_len(5)
        .exact()
    }

    fn encoded_size(&self, _value: &DoorState) -> EncodedSize {
        EncodedSize::Exact(5)
    }

    fn encode(&self, value: &DoorState, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        out.put(&value.id.to_be_bytes())?;
        out.put(&[value.open])
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, DoorState>) -> Ready<'s, DoorState> {
        slot.write(DoorState::default())
    }

    fn decode(
        &self,
        encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut DoorState,
    ) -> Result<(), WireError> {
        if encoding != DOOR_ENCODING {
            return Err(WireError::UnknownEncoding(encoding));
        }
        let [a, b, c, d, open] = *bytes else {
            return Err(WireError::InvalidLength {
                expected: 5,
                actual: bytes.len(),
            });
        };
        *into = DoorState {
            id: u32::from_be_bytes([a, b, c, d]),
            open,
        };
        Ok(())
    }
}

/// A second fixed-size wire with its own encoding: little-endian ID.
#[derive(Clone, Copy, Debug, Default)]
pub struct LittleDoorWire;

impl Wire<DoorState> for LittleDoorWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "little-door-wire",
            LITTLE_DOOR_WIRE_ID,
            LITTLE_DOOR_FAMILY_ID,
            WirePayloadIdentity::serialized(LITTLE_DOOR_ENCODING),
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
        .max_encoded_len(5)
    }

    fn encode(&self, value: &DoorState, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        out.put(&value.id.to_le_bytes())?;
        out.put(&[value.open])
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, DoorState>) -> Ready<'s, DoorState> {
        slot.write(DoorState::default())
    }

    fn decode(
        &self,
        encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut DoorState,
    ) -> Result<(), WireError> {
        if encoding != LITTLE_DOOR_ENCODING {
            return Err(WireError::UnknownEncoding(encoding));
        }
        let [a, b, c, d, open] = *bytes else {
            return Err(WireError::InvalidLength {
                expected: 5,
                actual: bytes.len(),
            });
        };
        *into = DoorState {
            id: u32::from_le_bytes([a, b, c, d]),
            open,
        };
        Ok(())
    }
}

/// A wire that declares it needs the heap, to prove admission refuses it.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllocatingWire;

impl Wire<DoorState> for AllocatingWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "allocating-wire",
            DOOR_WIRE_ID,
            DOOR_FAMILY_ID,
            WirePayloadIdentity::serialized(DOOR_ENCODING),
            RuntimeHeap::Allocates("decoder builds an owned value"),
            RuntimeHeap::Allocates("decoder builds an owned value"),
        )
        .max_encoded_len(5)
    }

    fn encode(&self, _value: &DoorState, _out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        Err(WireError::Unencodable("allocating wire is not run"))
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, DoorState>) -> Ready<'s, DoorState> {
        slot.write(DoorState::default())
    }

    fn decode(
        &self,
        _encoding: PayloadEncoding,
        _bytes: &[u8],
        _into: &mut DoorState,
    ) -> Result<(), WireError> {
        Err(WireError::Malformed("allocating wire is not run"))
    }
}

/// A wire whose encoded size varies with the value: the state byte, then `id` in
/// decimal ASCII preceded by its digit count, so truncation can be detected.
#[derive(Clone, Copy, Debug, Default)]
pub struct VariableDoorWire;

impl Wire<DoorState> for VariableDoorWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "variable-door-wire",
            WireIdentity::new("test.variable-door", 0x8A05),
            DOOR_FAMILY_ID,
            WirePayloadIdentity::serialized(PayloadEncoding::private_use(0xF003)),
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
        .max_encoded_len(12)
    }

    // A loose bound on purpose: the loan is larger than what a short value uses.
    fn encoded_size(&self, _value: &DoorState) -> EncodedSize {
        EncodedSize::AtMost(12)
    }

    fn encode(&self, value: &DoorState, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        let mut text = [0_u8; 10];
        let mut rest = value.id;
        let mut start = 10;
        loop {
            start -= 1;
            if let Some(slot) = text.get_mut(start) {
                *slot = b'0' + (rest % 10) as u8;
            }
            rest /= 10;
            if rest == 0 {
                break;
            }
        }
        let digits = text.get(start..).unwrap_or_default();
        out.put(&[value.open, digits.len() as u8])?;
        out.put(digits)
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, DoorState>) -> Ready<'s, DoorState> {
        slot.write(DoorState::default())
    }

    fn decode(
        &self,
        _encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut DoorState,
    ) -> Result<(), WireError> {
        let [open, count, digits @ ..] = bytes else {
            return Err(WireError::Malformed("too short"));
        };
        let open = *open;
        if usize::from(*count) != digits.len() || digits.is_empty() {
            return Err(WireError::Malformed("wrong digit count"));
        }
        let mut id = 0_u32;
        for &byte in digits {
            if !byte.is_ascii_digit() {
                return Err(WireError::Malformed("not a number"));
            }
            id = id.saturating_mul(10).saturating_add(u32::from(byte - b'0'));
        }
        *into = DoorState { id, open };
        Ok(())
    }
}
