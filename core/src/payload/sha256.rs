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

//! SHA-256, streaming and allocation-free.
//!
//! Hand-written because this crate has no dependencies (R48). Used only during
//! initialization, to derive native type tokens from a const descriptor. Proven
//! against the `sha2` crate by a differential test in `up-rust`.

const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// A SHA-256 computation in progress.
#[derive(Clone, Debug)]
pub struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    fill: usize,
    length_bits: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

/// The schedule word for round `round`, kept in a 16-entry circular buffer.
fn schedule_word(words: &mut [u32; 16], round: usize) -> u32 {
    let at = |offset: usize, words: &[u32; 16]| -> u32 {
        words
            .get(round.wrapping_add(offset) & 15)
            .copied()
            .unwrap_or(0)
    };
    let current = at(0, words);
    if round < 16 {
        return current;
    }
    let w1 = at(1, words);
    let w9 = at(9, words);
    let w14 = at(14, words);
    let s0 = w1.rotate_right(7) ^ w1.rotate_right(18) ^ (w1 >> 3);
    let s1 = w14.rotate_right(17) ^ w14.rotate_right(19) ^ (w14 >> 10);
    let next = current.wrapping_add(s0).wrapping_add(w9).wrapping_add(s1);
    if let Some(slot) = words.get_mut(round & 15) {
        *slot = next;
    }
    next
}

impl Sha256 {
    /// Starts a new digest.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: [
                0x6a09_e667,
                0xbb67_ae85,
                0x3c6e_f372,
                0xa54f_f53a,
                0x510e_527f,
                0x9b05_688c,
                0x1f83_d9ab,
                0x5be0_cd19,
            ],
            block: [0; 64],
            fill: 0,
            length_bits: 0,
        }
    }

    fn compress(&mut self) {
        let mut words = [0_u32; 16];
        for (slot, chunk) in words.iter_mut().zip(self.block.as_chunks::<4>().0) {
            *slot = u32::from_be_bytes(*chunk);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for (round, constant) in K.iter().enumerate() {
            let w = schedule_word(&mut words, round);
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*constant)
                .wrapping_add(w);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }

    /// Feeds more input.
    pub fn update(&mut self, mut bytes: &[u8]) {
        let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        self.length_bits = self.length_bits.wrapping_add(length.wrapping_mul(8));
        while !bytes.is_empty() {
            let free = 64_usize.saturating_sub(self.fill);
            let take = free.min(bytes.len());
            let (now, rest) = bytes.split_at(take);
            bytes = rest;
            if let Some(target) = self
                .block
                .get_mut(self.fill..self.fill.saturating_add(take))
            {
                for (slot, byte) in target.iter_mut().zip(now) {
                    *slot = *byte;
                }
            }
            self.fill = self.fill.saturating_add(take);
            if self.fill == 64 {
                self.compress();
                self.fill = 0;
            }
        }
    }

    /// Finishes the digest.
    #[must_use]
    pub fn finish(mut self) -> [u8; 32] {
        let bits = self.length_bits.to_be_bytes();
        self.update(&[0x80]);
        let zero = [0_u8; 64];
        let padding = (120_usize.wrapping_sub(self.fill)) % 64;
        // The padding length is the distance to the length word; it never spills a block.
        let (zeros, _) = zero.split_at(padding.min(64));
        self.update(zeros);
        self.update(&bits);
        let mut out = [0_u8; 32];
        for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(self.state) {
            *chunk = word.to_be_bytes();
        }
        out
    }
}

/// Convenience digest of one slice.
#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest.finish()
}
