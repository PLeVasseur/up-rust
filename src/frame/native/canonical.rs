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
//! Checked decoding for the target-qualified native descriptor representation.
use super::{
    NativeByteOrder, NativeField, NativeProfileError, NativeRepresentation, NativeTarget,
    MAX_NATIVE_DESCRIPTOR_BYTES,
};

pub(super) const HEADER: &[u8] = b"uprotocol.native-representation.v2\0";

fn invalid(reason: &'static str) -> NativeProfileError {
    NativeProfileError::InvalidDescriptor(reason)
}

struct Cursor<'a> {
    remaining: &'a [u8],
}

impl<'a> Cursor<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], NativeProfileError> {
        let (value, rest) = self
            .remaining
            .split_at_checked(length)
            .ok_or_else(|| invalid("truncated canonical descriptor"))?;
        self.remaining = rest;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, NativeProfileError> {
        self.take(1)?
            .first()
            .copied()
            .ok_or_else(|| invalid("missing descriptor byte"))
    }

    fn u32(&mut self) -> Result<u32, NativeProfileError> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| invalid("invalid descriptor u32"))?,
        ))
    }

    fn u64(&mut self) -> Result<u64, NativeProfileError> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| invalid("invalid descriptor u64"))?,
        ))
    }

    fn length(&mut self) -> Result<usize, NativeProfileError> {
        usize::try_from(self.u64()?).map_err(|_| invalid("descriptor length exceeds usize"))
    }

    fn string(&mut self) -> Result<String, NativeProfileError> {
        let length = self.length()?;
        let value = std::str::from_utf8(self.take(length)?)
            .map_err(|_| invalid("descriptor string is not UTF-8"))?;
        Ok(value.to_owned())
    }

    fn representation(&mut self, depth: usize) -> Result<NativeRepresentation, NativeProfileError> {
        if depth > 128 {
            return Err(invalid("descriptor nesting limit exceeded"));
        }
        let triple = self.string()?;
        let target = NativeTarget::new(triple, self.byte()?)?;
        let name = self.string()?;
        let revision = self.u32()?;
        let size = self.u64()?;
        let alignment = self.u64()?;
        let byte_order = match self.byte()? {
            1 => NativeByteOrder::LittleEndian,
            2 => NativeByteOrder::BigEndian,
            _ => return Err(invalid("unknown descriptor byte order")),
        };
        let count = self.length()?;
        // A field needs at least its three u64 prefixes plus a child target,
        // name and scalar layout. This lower bound ignores nonempty strings.
        const MIN_FIELD_BYTES: usize = 24 + 8 + 1 + 8 + 4 + 8 + 8 + 1 + 8;
        if count > self.remaining.len() / MIN_FIELD_BYTES {
            return Err(invalid("descriptor field count exceeds remaining bytes"));
        }
        let mut fields = Vec::with_capacity(count);
        for _ in 0..count {
            let name = self.string()?;
            let offset = self.u64()?;
            let repetitions = self.u64()?;
            let representation = self.representation(depth + 1)?;
            fields.push(NativeField::repeated(
                name,
                offset,
                repetitions,
                representation,
            ));
        }
        NativeRepresentation::for_target(
            target, name, revision, size, alignment, byte_order, fields,
        )
    }
}

pub(super) fn decode(bytes: &[u8]) -> Result<NativeRepresentation, NativeProfileError> {
    if bytes.len() > MAX_NATIVE_DESCRIPTOR_BYTES {
        return Err(invalid("canonical descriptor exceeds input limit"));
    }
    let remaining = bytes
        .strip_prefix(HEADER)
        .ok_or_else(|| invalid("unsupported canonical descriptor version"))?;
    let mut cursor = Cursor { remaining };
    let representation = cursor.representation(0)?;
    if !cursor.remaining.is_empty() {
        return Err(invalid("trailing canonical descriptor bytes"));
    }
    Ok(representation)
}
