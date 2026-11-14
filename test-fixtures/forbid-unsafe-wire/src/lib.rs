//! A hand-written wire and a wrapper around it, in a crate that forbids `unsafe`.
//!
//! Writing or composing a wire needs no `unsafe` (R46). The forbid below, and the
//! fact that this crate builds, are the proof; the tests show the wire works.

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicUsize, Ordering};
use up_rust::two_mode::{
    EncodedSize, PayloadWriter, Ready, RuntimeHeap, StartupSlot, Wire, WireError, WireIdentity,
    WireInfo, WirePayloadIdentity, ZeroSlot,
};

use up_rust::PayloadEncoding;

const ENCODING: PayloadEncoding = PayloadEncoding::private_use(0xF500);

/// A plain message whose all-zero form is valid, derived with no `unsafe` here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ZeroSlot)]
#[repr(C)]
pub struct Reading {
    /// Sensor reading.
    pub value: u32,
    /// Sequence number.
    pub sequence: u32,
}

/// Big-endian `value` then `sequence`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReadingWire;

impl Wire<Reading> for ReadingWire {
    fn describe(&self) -> WireInfo {
        WireInfo::new(
            "reading-wire",
            WireIdentity::new("example.vendor/reading-wire", 0),
            WireIdentity::new("example.vendor/reading", 0),
            WirePayloadIdentity::serialized(ENCODING),
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
        .max_encoded_len(8)
        .exact()
    }

    fn encoded_size(&self, _value: &Reading) -> EncodedSize {
        EncodedSize::Exact(8)
    }

    fn encode(&self, value: &Reading, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        out.put(&value.value.to_be_bytes())?;
        out.put(&value.sequence.to_be_bytes())
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, Reading>) -> Ready<'s, Reading> {
        slot.zeroed()
    }

    fn decode(
        &self,
        encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut Reading,
    ) -> Result<(), WireError> {
        if encoding != ENCODING {
            return Err(WireError::UnknownEncoding(encoding));
        }
        let [a, b, c, d, e, f, g, h] = *bytes else {
            return Err(WireError::InvalidLength {
                expected: 8,
                actual: bytes.len(),
            });
        };
        *into = Reading {
            value: u32::from_be_bytes([a, b, c, d]),
            sequence: u32::from_be_bytes([e, f, g, h]),
        };
        Ok(())
    }
}

/// A wrapper that counts the bytes it encodes. It states its own heap class.
#[derive(Clone, Copy, Debug)]
pub struct Counted<W> {
    inner: W,
    bytes: &'static AtomicUsize,
}

impl<W> Counted<W> {
    /// Wraps a wire; bytes encoded through it are added to `bytes`.
    pub const fn new(inner: W, bytes: &'static AtomicUsize) -> Self {
        Self { inner, bytes }
    }
}

impl<W: Wire<Reading>> Wire<Reading> for Counted<W> {
    fn describe(&self) -> WireInfo {
        WireInfo::wrapping(
            &self.inner.describe(),
            "counted-wire",
            WireIdentity::new("example.vendor/counted-wire", 0),
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
    }

    fn encoded_size(&self, value: &Reading) -> EncodedSize {
        self.inner.encoded_size(value)
    }

    fn encode(&self, value: &Reading, out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        let before = out.written();
        self.inner.encode(value, out)?;
        let used = out.written().saturating_sub(before);
        self.bytes.fetch_add(used, Ordering::Relaxed);
        Ok(())
    }

    fn init_slot<'s>(&self, slot: StartupSlot<'s, Reading>) -> Ready<'s, Reading> {
        self.inner.init_slot(slot)
    }

    fn decode(
        &self,
        encoding: PayloadEncoding,
        bytes: &[u8],
        into: &mut Reading,
    ) -> Result<(), WireError> {
        self.inner.decode(encoding, bytes, into)
    }
}
