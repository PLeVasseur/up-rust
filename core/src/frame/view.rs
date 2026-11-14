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

//! The slice view of a frame payload, with no reader and no owned metadata.
//!
//! `up-rust`'s `UFrameView` exposes `&UFrameMetadata` and a `std::io::Read`
//! associated type, so it cannot live here. This is the part of its contract
//! that does not need `std`: the ordered payload slices and their presence.

/// Ordered, borrowed payload bytes of a received frame.
///
/// A frame with an absent payload reports `has_payload() == false`; a present
/// empty payload reports `true` with `payload_len() == 0`. Implementations must
/// not allocate, coalesce or copy storage to produce these views.
pub trait FramePayloadSlices {
    /// Iterator over the payload's storage segments in order.
    type PayloadSlices<'a>: Iterator<Item = &'a [u8]> + 'a
    where
        Self: 'a;

    /// Returns the number of application payload bytes visible through this view.
    fn payload_len(&self) -> usize;

    /// Returns whether this view carries a payload, including a present empty payload.
    fn has_payload(&self) -> bool;

    /// Returns ordered borrowed payload slices.
    fn payload_slices(&self) -> Self::PayloadSlices<'_>;

    /// Returns a contiguous borrowed payload view when one exists without copying.
    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        None
    }
}

/// Totals the bytes of ordered payload slices, refusing an overflow.
///
/// Returns `None` if the total does not fit in `usize`.
#[must_use]
pub fn total_slice_len<'a>(slices: impl Iterator<Item = &'a [u8]>) -> Option<usize> {
    let mut total = 0_usize;
    for slice in slices {
        total = total.checked_add(slice.len())?;
    }
    Some(total)
}
