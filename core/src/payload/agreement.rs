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

//! The build-time native agreement: a constant table, checked at startup.
//!
//! `up-rust` builds its `NativeProfileAgreement` from `NativeRepresentation`
//! values on the heap. Core carries the same agreement as a constant table
//! generated at build time. The token a table entry states is derived by the
//! same algorithm: the first four little-endian bytes of the SHA-256 of the
//! canonical descriptor. Core recomputes that digest from `T`'s const descriptor
//! at startup, so a stale table is refused before any native route opens.
//!
//! The one input a `no_std` crate cannot obtain by itself is the target triple,
//! because this crate has no build script. The table states it. That is the
//! build-time part of a build-time agreement: the table is generated for one
//! target, and core confirms the pointer width and byte order it was given.

use core::fmt;

use super::sha256::Sha256;
use super::stable::{StableDescriptor, StablePayload, StablePayloadField};
use crate::frame::native::NativeTypeToken;
use crate::uattributes::PayloadEncoding;
use crate::wire::WirePayloadIdentity;

/// Domain string that opens the canonical descriptor bytes.
pub const CANONICAL_REPRESENTATION_HEADER: &[u8] = b"uprotocol.native-representation.v2\0";

/// One type in a [`NativeAgreement`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeAgreementEntry {
    /// Deployment-stable type name.
    pub type_name: &'static str,
    /// Layout revision.
    pub revision: u32,
    /// Full SHA-256 of the canonical descriptor. The token is its first four
    /// little-endian bytes.
    pub digest: [u8; 32],
}

impl NativeAgreementEntry {
    /// The native type token this entry states.
    #[must_use]
    pub const fn token(&self) -> NativeTypeToken {
        NativeTypeToken::from_u32(token_from_digest(&self.digest))
    }
}

const fn token_from_digest(digest: &[u8; 32]) -> u32 {
    let [a, b, c, d, ..] = *digest;
    u32::from_le_bytes([a, b, c, d])
}

/// One deployment generation's agreed native representations, as constant data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeAgreement {
    /// Deployment domain.
    pub domain: &'static str,
    /// Generation number.
    pub version: u32,
    /// The native payload encoding shared by these representations.
    pub encoding: PayloadEncoding,
    /// Compiler target triple the table was generated for.
    pub target_triple: &'static str,
    /// Target pointer width, in bits.
    pub pointer_width: u8,
    /// `true` when the agreed scalars are little-endian.
    pub little_endian: bool,
    /// The agreed representations.
    pub entries: &'static [NativeAgreementEntry],
}

/// Why a type could not be resolved under a [`NativeAgreement`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AgreementError {
    /// The table was generated for a different pointer width or byte order.
    WrongTarget,
    /// The type is not in the table.
    NotAgreed,
    /// The type is in the table, but its descriptor disagrees with the table.
    DescriptorMismatch,
}

impl fmt::Display for AgreementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongTarget => {
                f.write_str("native agreement was generated for a different target")
            }
            Self::NotAgreed => f.write_str("payload type is not in the native agreement"),
            Self::DescriptorMismatch => {
                f.write_str("payload descriptor disagrees with the native agreement")
            }
        }
    }
}

impl core::error::Error for AgreementError {}

fn feed_u64(digest: &mut Sha256, value: u64) {
    digest.update(&value.to_le_bytes());
}

fn feed_string(digest: &mut Sha256, value: &str) {
    feed_u64(digest, u64::try_from(value.len()).unwrap_or(u64::MAX));
    digest.update(value.as_bytes());
}

fn feed_representation(digest: &mut Sha256, agreement: &NativeAgreement, node: &StableDescriptor) {
    feed_string(digest, agreement.target_triple);
    digest.update(&[agreement.pointer_width]);
    feed_string(digest, node.name);
    digest.update(&node.revision.to_le_bytes());
    feed_u64(digest, node.size);
    feed_u64(digest, node.alignment);
    digest.update(&[if agreement.little_endian { 1 } else { 2 }]);
    feed_u64(digest, u64::try_from(node.fields.len()).unwrap_or(u64::MAX));
    for field in node.fields {
        feed_string(digest, field.name);
        feed_u64(digest, field.offset);
        feed_u64(digest, field.count);
        feed_representation(digest, agreement, field.descriptor);
    }
}

/// Computes the full canonical descriptor digest of `T` under `agreement`'s target.
#[must_use]
pub fn descriptor_digest<T: StablePayloadField>(agreement: &NativeAgreement) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(CANONICAL_REPRESENTATION_HEADER);
    feed_representation(&mut digest, agreement, T::DESCRIPTOR);
    digest.finish()
}

impl NativeAgreement {
    /// Resolves `T` under this agreement, recomputing its digest from `T`'s
    /// const descriptor, so a table that is not current for this binary is refused.
    ///
    /// # Errors
    /// See [`AgreementError`].
    pub fn resolve<T: StablePayload>(&self) -> Result<WirePayloadIdentity, AgreementError> {
        if self.pointer_width != u8::try_from(usize::BITS).unwrap_or(0)
            || self.little_endian != cfg!(target_endian = "little")
        {
            return Err(AgreementError::WrongTarget);
        }
        let entry = self
            .entries
            .iter()
            .find(|entry| entry.type_name == T::TYPE_NAME && entry.revision == T::REVISION)
            .ok_or(AgreementError::NotAgreed)?;
        if !super::stable::descriptor_matches_layout::<T>()
            || T::DESCRIPTOR.name != T::TYPE_NAME
            || T::DESCRIPTOR.revision != T::REVISION
        {
            return Err(AgreementError::DescriptorMismatch);
        }
        let digest = descriptor_digest::<T>(self);
        if digest != entry.digest {
            return Err(AgreementError::DescriptorMismatch);
        }
        Ok(WirePayloadIdentity {
            encoding: self.encoding,
            native_type_token: Some(NativeTypeToken::from_u32(token_from_digest(&digest))),
        })
    }
}

/// The native ABI of this build, independent of the compiler target's name.
///
/// Version 1 is eight bytes: version, byte order (1 little, 2 big), pointer
/// width in bits, then base-2 scalar alignment logarithms in low/high nibbles:
/// `(u16, u32)`, `(u64, u128)`, `(i128, f32)`, `(f64, reserved)`, and one
/// reserved byte. Both reserved fields are zero. Encoding is byte-oriented,
/// with the same meaning on either byte order; it is not a hash.
///
/// Equality is necessary, not sufficient, for native interoperability. Native
/// descriptors/tokens still check type layout and target identity. This value
/// does not establish coherence or protect against a peer lying about its ABI.
/// Route/binding setup exchange is separate work; this value alone does not
/// refuse a deployed route. Serialized wires do not require ABI equality.
///
/// Preparatory mapping: req~ssr-native-abi-agreed~1 at neutral spec 733b006.
/// It lacks SafetyGateUpRust there: no OFT coverage/closure is claimed here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct AbiFingerprint([u8; 8]);

const fn alignment_log<T>() -> u8 {
    // Compile-time use only. Rust alignments are nonzero powers of two.
    let log = core::mem::align_of::<T>().trailing_zeros();
    assert!(log <= 15);
    log as u8
}

/// This build's ABI, computed entirely in a const context, without a target table.
pub const ABI_FINGERPRINT: AbiFingerprint = {
    assert!(usize::BITS <= u8::MAX as u32);
    AbiFingerprint([
        1,
        if cfg!(target_endian = "little") { 1 } else { 2 },
        usize::BITS as u8,
        alignment_log::<u16>() | (alignment_log::<u32>() << 4),
        alignment_log::<u64>() | (alignment_log::<u128>() << 4),
        alignment_log::<i128>() | (alignment_log::<f32>() << 4),
        alignment_log::<f64>(),
        0,
    ])
};

impl AbiFingerprint {
    /// The canonical eight-byte encoding, suitable for a setup attribute.
    #[must_use]
    pub const fn to_bytes(self) -> [u8; 8] {
        self.0
    }

    /// Decodes version 1, rejecting unknown versions and noncanonical values.
    /// Alignment logarithms occupy four bits, so each alignment is in 1..=32768.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 8]) -> Option<Self> {
        let [version, order, width, _, _, _, last, reserved] = bytes;
        if version != 1
            || (order != 1 && order != 2)
            || !matches!(width, 16 | 32 | 64 | 128)
            || last & 0xf0 != 0
            || reserved != 0
        {
            None
        } else {
            Some(Self(bytes))
        }
    }

    /// Whether scalars in this ABI are little-endian.
    #[must_use]
    pub const fn little_endian(self) -> bool {
        let [_, order, ..] = self.0;
        order == 1
    }

    /// Pointer width in bits.
    #[must_use]
    pub const fn pointer_width(self) -> u8 {
        let [_, _, width, ..] = self.0;
        width
    }

    /// Scalar alignments in bytes, in `u16, u32, u64, u128, i128, f32, f64` order.
    #[must_use]
    pub const fn scalar_alignments(self) -> [usize; 7] {
        let [_, _, _, a, b, c, d, _] = self.0;
        [
            1 << (a & 15),
            1 << (a >> 4),
            1 << (b & 15),
            1 << (b >> 4),
            1 << (c & 15),
            1 << (c >> 4),
            1 << d,
        ]
    }

    /// Compares two decoded ABIs without allocating. Setup callers must still
    /// enforce the result before admitting a native route.
    ///
    /// # Errors
    /// Returns both ABIs when any field differs.
    pub fn check(self, peer: Self) -> Result<(), AbiMismatch> {
        if self == peer {
            Ok(())
        } else {
            Err(AbiMismatch { local: self, peer })
        }
    }
}

impl fmt::Display for AbiFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [version, ..] = self.0;
        write!(
            f,
            "v{version} {}-endian {}-bit align(u16,u32,u64,u128,i128,f32,f64)={:?}",
            if self.little_endian() {
                "little"
            } else {
                "big"
            },
            self.pointer_width(),
            self.scalar_alignments()
        )
    }
}

/// A heap-free ABI comparison failure; it changes no native agreement layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AbiMismatch {
    /// The local build's ABI.
    pub local: AbiFingerprint,
    /// The peer's declared ABI.
    pub peer: AbiFingerprint,
}

impl AbiMismatch {
    /// Names the first differing ABI field, or `none` for equal inputs.
    #[must_use]
    pub fn differing_field(&self) -> &'static str {
        let [local_version, ..] = self.local.0;
        let [peer_version, ..] = self.peer.0;
        if local_version != peer_version {
            return "format version";
        }
        if self.local.little_endian() != self.peer.little_endian() {
            return "byte order";
        }
        if self.local.pointer_width() != self.peer.pointer_width() {
            return "pointer width";
        }
        let names = [
            "u16 alignment",
            "u32 alignment",
            "u64 alignment",
            "u128 alignment",
            "i128 alignment",
            "f32 alignment",
            "f64 alignment",
        ];
        for ((local, peer), name) in self
            .local
            .scalar_alignments()
            .into_iter()
            .zip(self.peer.scalar_alignments())
            .zip(names)
        {
            if local != peer {
                return name;
            }
        }
        "none"
    }
}

impl fmt::Display for AbiMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native ABI mismatch ({}): local [{}], peer [{}]; use a serialized wire between these peers",
            self.differing_field(), self.local, self.peer)
    }
}

impl core::error::Error for AbiMismatch {}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod abi_fingerprint_tests {
    use super::*;
    use core::fmt::Write;
    use core::mem::{align_of, size_of};

    // Preparatory test mapping: req~ssr-native-abi-agreed~1 (G3 not yet gated).
    // method: host ABI observation, including the two 128-bit scalars (R62/P7).
    #[test]
    fn abi_fingerprint_host_const_matches_layout() {
        const ENCODED: [u8; 8] = ABI_FINGERPRINT.to_bytes();
        const DECODED: Option<AbiFingerprint> = AbiFingerprint::from_bytes(ENCODED);
        assert_eq!(DECODED, Some(ABI_FINGERPRINT));
        assert_eq!(size_of::<AbiFingerprint>(), 8);
        assert_eq!(align_of::<AbiFingerprint>(), 1);
        assert!(!core::mem::needs_drop::<AbiFingerprint>());
        assert!(!core::mem::needs_drop::<AbiMismatch>());
        assert_eq!(
            ABI_FINGERPRINT.little_endian(),
            cfg!(target_endian = "little")
        );
        assert_eq!(u32::from(ABI_FINGERPRINT.pointer_width()), usize::BITS);
        assert_eq!(
            ABI_FINGERPRINT.scalar_alignments(),
            [
                align_of::<u16>(),
                align_of::<u32>(),
                align_of::<u64>(),
                align_of::<u128>(),
                align_of::<i128>(),
                align_of::<f32>(),
                align_of::<f64>()
            ]
        );
        assert_eq!(ABI_FINGERPRINT.check(ABI_FINGERPRINT), Ok(()));
    }

    // Preparatory test mapping: req~ssr-native-abi-agreed~1. method: encoding vectors.
    #[test]
    fn abi_fingerprint_encoding_and_roundtrip() {
        // ARMv7-like vs RISC-V32-like ABI: u128/i128 alignments must discriminate.
        let arm = AbiFingerprint::from_bytes([1, 1, 32, 0x21, 0x33, 0x23, 3, 0]).unwrap();
        let risc = AbiFingerprint::from_bytes([1, 1, 32, 0x21, 0x43, 0x24, 3, 0]).unwrap();
        assert_eq!(arm.scalar_alignments(), [2, 4, 8, 8, 8, 4, 8]);
        assert_eq!(risc.scalar_alignments(), [2, 4, 8, 16, 16, 4, 8]);
        assert_ne!(arm, risc);
        // Exhaust each nibble at each position; ensure one field never spills into another.
        for field in 0..7 {
            for log in 0..16 {
                let mut bytes = arm.to_bytes();
                let shift = (field % 2) * 4;
                bytes[3 + field / 2] = (bytes[3 + field / 2] & !(15 << shift)) | (log << shift);
                let decoded = AbiFingerprint::from_bytes(bytes).unwrap();
                assert_eq!(decoded.to_bytes(), bytes);
                let mut expected = arm.scalar_alignments();
                expected[field] = 1 << log;
                assert_eq!(decoded.scalar_alignments(), expected);
            }
        }
    }

    struct Text {
        bytes: [u8; 512],
        len: usize,
    }
    impl fmt::Write for Text {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            let end = self.len.checked_add(text.len()).ok_or(fmt::Error)?;
            self.bytes
                .get_mut(self.len..end)
                .ok_or(fmt::Error)?
                .copy_from_slice(text.as_bytes());
            self.len = end;
            Ok(())
        }
    }

    // Preparatory test mapping: req~ssr-native-abi-agreed~1. method: one-field mutations.
    #[test]
    fn abi_fingerprint_each_field_and_refusal_text() {
        let base = AbiFingerprint::from_bytes([1, 1, 32, 0x21, 0x33, 0x23, 3, 0]).unwrap();
        let changes = [
            (0, 2, "format version"),
            (1, 2, "byte order"),
            (2, 64, "pointer width"),
            (3, 0x22, "u16 alignment"),
            (3, 0x31, "u32 alignment"),
            (4, 0x32, "u64 alignment"),
            (4, 0x43, "u128 alignment"),
            (5, 0x24, "i128 alignment"),
            (5, 0x33, "f32 alignment"),
            (6, 2, "f64 alignment"),
        ];
        for (index, value, name) in changes {
            let mut bytes = base.to_bytes();
            bytes[index] = value;
            // Unknown-version values are private synthetic data, never accepted by decoding.
            let peer = AbiFingerprint(bytes);
            assert_ne!(base, peer);
            for (local, peer) in [(base, peer), (peer, base)] {
                let mismatch = local.check(peer).unwrap_err();
                assert_eq!(mismatch, AbiMismatch { local, peer });
                assert_eq!(mismatch.differing_field(), name);
                let mut text = Text {
                    bytes: [0; 512],
                    len: 0,
                };
                write!(&mut text, "{mismatch}").unwrap();
                let rendered = core::str::from_utf8(&text.bytes[..text.len]).unwrap();
                assert!(rendered.contains(name));
                assert!(rendered.contains("local [v") && rendered.contains("peer [v"));
                assert!(rendered.contains("-endian") && rendered.contains("-bit"));
                assert!(rendered.ends_with("use a serialized wire between these peers"));
            }
        }
    }

    // Preparatory test mapping: req~ssr-native-abi-agreed~1. method: malformed encodings.
    #[test]
    fn abi_fingerprint_rejects_noncanonical_encoding() {
        for (index, value) in [
            (0, 0),
            (0, 2),
            (1, 0),
            (1, 3),
            (2, 0),
            (2, 33),
            (6, 0x13),
            (7, 1),
        ] {
            let mut bytes = ABI_FINGERPRINT.to_bytes();
            bytes[index] = value;
            assert_eq!(AbiFingerprint::from_bytes(bytes), None);
        }
        for width in [16, 32, 64, 128] {
            for order in [1, 2] {
                let bytes = [1, order, width, 0x21, 0x43, 0x24, 3, 0];
                assert_eq!(AbiFingerprint::from_bytes(bytes).unwrap().to_bytes(), bytes);
            }
        }
    }
}
