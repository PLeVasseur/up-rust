//! UInt32Value mapping for Reading, using the existing checked Wire interfaces.
//! Compatibility baseline: rust-protobuf 3.7.2 `Message::parse_from_bytes` over
//! a complete slice, not streaming Read's differently buffered slow path.
//! See PROTOBUF-COMPATIBILITY.md in the implementation handoff for pinned cuts.
#![forbid(unsafe_code)]

use super::{Reading, ReadingProtobuf};
use up_rust_core::uattributes::PayloadEncoding;
use up_rust_core::wire::{
    EncodedSize, PayloadWriter, Ready, RuntimeHeap, StartupSlot, Wire, WireError,
    WireInfo, WirePayloadIdentity, PROTOBUF_PAYLOAD_FAMILY_ID, PROTOBUF_WIRE_ID,
};

fn payload_size(value: u32) -> usize {
    match value {
        0 => 0,
        1..=0x7f => 2,
        0x80..=0x3fff => 3,
        0x4000..=0x1f_ffff => 4,
        0x20_0000..=0x0fff_ffff => 5,
        _ => 6,
    }
}

impl Wire<Reading> for ReadingProtobuf {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "independent.Reading.protobuf.v1", PROTOBUF_WIRE_ID,
            PROTOBUF_PAYLOAD_FAMILY_ID, WirePayloadIdentity::serialized(PayloadEncoding::PROTOBUF),
            RuntimeHeap::None, RuntimeHeap::None,
        ).max_encoded_len(6).exact()
        // Six bounds canonical TX only. A receive route must explicitly choose
        // its admitted max_payload; valid unknown/duplicate fields can exceed six.
    }

    fn encoded_size(&self, value: &Reading) -> EncodedSize {
        EncodedSize::Exact(payload_size(value.value))
    }

    /// # Safety-usable invariant
    /// Uses only checked writer operations and never replaces the writer. Every
    /// emitted byte is initialized; all writes preserve earlier initialized data.
    /// Capacity is checked for the entire value before any write. No callback,
    /// raw access, heap storage, domain destructor or external serializer runs.
    fn encode(&self, value: &Reading, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        let size = payload_size(value.value);
        if out.remaining() < size {
            return Err(WireError::BufferTooSmall { needed: size, available: out.remaining() });
        }
        if value.value == 0 { return Ok(()); }
        out.put(&[0x08])?;
        let mut remaining = value.value;
        // u32 loses seven bits each iteration; at most five single-byte writes.
        loop {
            let low = (remaining & 0x7f) as u8;
            remaining >>= 7;
            out.put(&[if remaining == 0 { low } else { low | 0x80 }])?;
            if remaining == 0 { return Ok(()); }
        }
    }

    /// # Safety-usable invariant
    /// Initializes exactly the supplied startup slot with a valid Reading and
    /// returns Ready for that same slot/lifetime via StartupSlot::write.
    fn init_slot<'s>(&self, slot: StartupSlot<'s, Reading>) -> Ready<'s, Reading> {
        slot.write(Reading { value: 0 })
    }

    /// # Safety-usable invariant
    /// Reads only the supplied initialized slice. The destination remains its
    /// original valid value on refusal; success replaces its scalar only after
    /// the whole message has parsed. No references to input escape this call.
    fn decode(&self, encoding: PayloadEncoding, bytes: &[u8], into: &mut Reading) -> Result<(), WireError> {
        if encoding != PayloadEncoding::PROTOBUF { return Err(WireError::UnknownEncoding(encoding)); }
        let mut input = Input(bytes);
        let mut value = 0;
        // Generated top-level unknown-group skip does not consume recursion
        // budget; nested CodedInputStream::skip_group consumes up to 100 levels.
        // Field numbers on EndGroup are deliberately not matched in 3.7.2.
        let mut groups = 0_u8;
        while !input.0.is_empty() {
            let tag = input.varint32()?;
            if tag == 8 && groups == 0 {
                value = input.varint32()?;
                continue;
            }
            let kind = tag & 7;
            if tag >> 3 == 0 || kind > 5 { return Err(WireError::Malformed("incorrect protobuf tag")); }
            match kind {
                0 => { let _ = input.varint64()?; }
                1 => input.skip(8)?,
                2 => {
                    let size = usize::try_from(input.varint32()?)
                        .map_err(|_| WireError::Malformed("protobuf length exceeds address space"))?;
                    input.skip(size)?;
                }
                3 => {
                    if groups == 101 { return Err(WireError::Malformed("protobuf recursion limit")); }
                    groups += 1; // checked against 101, so no u8 overflow
                }
                4 => {
                    if groups == 0 { return Err(WireError::Malformed("unexpected protobuf end group")); }
                    groups -= 1;
                }
                5 => input.skip(4)?,
                _ => return Err(WireError::Malformed("incorrect protobuf wire type")),
            }
        }
        // Even though the dependency's nested skip accepts EOF, the outer
        // generated-code skip still requires an EndGroup before returning.
        if groups != 0 { return Err(WireError::Malformed("unterminated protobuf group")); }
        into.value = value;
        Ok(())
    }
}

/// Cursor is an ordinary subslice. Each successful byte/tag/skip consumes input;
/// malformed input returns a static error. There is no pointer arithmetic or
/// recursive stack usage, and parsing is linear in the admitted input length.
struct Input<'a>(&'a [u8]);
impl Input<'_> {
    fn byte(&mut self) -> Result<u8, WireError> {
        let (byte, rest) = self.0.split_first().ok_or(WireError::Malformed("truncated protobuf input"))?;
        self.0 = rest;
        Ok(*byte)
    }
    fn skip(&mut self, count: usize) -> Result<(), WireError> {
        self.0 = self.0.get(count..).ok_or(WireError::Malformed("truncated protobuf field"))?;
        Ok(())
    }
    fn varint32(&mut self) -> Result<u32, WireError> {
        let mut value = 0_u32;
        for shift in [0, 7, 14, 21, 28] {
            let byte = self.byte()?;
            // Matches decode_varint32 on a complete slice: non-minimal forms
            // through five bytes accepted; sixth byte or high bits refused.
            if shift == 28 && byte > 0x0f { return Err(WireError::Malformed("invalid protobuf uint32 varint")); }
            value |= u32::from(byte & 0x7f) << shift;
            if byte < 0x80 { return Ok(value); }
        }
        Err(WireError::Malformed("invalid protobuf uint32 varint"))
    }
    fn varint64(&mut self) -> Result<u64, WireError> {
        let mut value = 0_u64;
        for shift in [0, 7, 14, 21, 28, 35, 42, 49, 56, 63] {
            let byte = self.byte()?;
            if shift == 63 && byte > 1 { return Err(WireError::Malformed("invalid protobuf uint64 varint")); }
            value |= u64::from(byte & 0x7f) << shift;
            if byte < 0x80 { return Ok(value); }
        }
        Err(WireError::Malformed("invalid protobuf uint64 varint"))
    }
}
