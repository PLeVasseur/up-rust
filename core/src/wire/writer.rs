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

//! The checked writer a wire encodes through.
//!
//! A wire never writes through a raw slice: every byte goes through a method that
//! stays inside the writer's storage and advances the count the SDK reads
//! afterwards. Nothing a safe wire can do lets it claim more initialized bytes
//! than it really wrote (R46).

use core::mem::MaybeUninit;

use super::WireError;

/// Writes one encoded payload into lent storage. Bytes are written once.
#[derive(Debug)]
// [impl->req~ssr-unsafe-wire-untrusted~1]
pub struct PayloadWriter<'a> {
    // Safety invariant: uniquely borrowed output. Every safe operation preserves
    // every byte that was initialized on entry, including error/unwind paths;
    // the raw MaybeUninit slice is never exposed to caller code.
    out: &'a mut [MaybeUninit<u8>],
    // Safety invariant: written<=out.len(); every byte in0..written is initialized.
    written: usize,
}

/// A byte range reserved for back-patching. Patch before the writer goes out of scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[must_use]
pub struct ReservedRange {
    start: usize,
    len: usize,
}

impl<'a> PayloadWriter<'a> {
    /// Wraps lent storage, typically by the SDK. A wrapper wire may also use this
    /// over its own scratch space.
    /// # Safety-usable invariant
    /// All reachable safe writer methods preserve already initialized bytes,
    /// including on errors/unwind. Safe clients can replace/move/forget this
    /// writer, but cannot retrieve or deinitialize its raw storage. `written`
    /// describes only this writer's checked initialized prefix, not an old writer.
    pub fn new(out: &'a mut [MaybeUninit<u8>]) -> Self {
        Self { out, written: 0 }
    }

    /// Bytes written so far.
    #[must_use]
    pub const fn written(&self) -> usize {
        self.written
    }

    /// Bytes still available.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.out.len().saturating_sub(self.written)
    }

    /// Whether this writer still owns the storage the SDK handed it.
    ///
    /// A safe wire can assign a fresh writer through `&mut`, which would make
    /// `written` count bytes in some other storage. The SDK checks this after
    /// `encode` and refuses the frame on a mismatch (design section 4.1).
    #[must_use]
    /// # Safety-usable invariant
    /// True proves exact base pointer and slice length match, not just capacity.
    /// Combined with the field invariant, written then describes that output.
    pub(crate) fn owns(&self, base: *const MaybeUninit<u8>, len: usize) -> bool {
        core::ptr::eq(self.out.as_ptr(), base) && self.out.len() == len
    }

    /// Appends `bytes`.
    ///
    /// # Errors
    /// Reports a destination that cannot hold them, without writing any.
    pub fn put(&mut self, bytes: &[u8]) -> Result<(), WireError> {
        let shortfall = WireError::BufferTooSmall {
            needed: bytes.len(),
            available: self.remaining(),
        };
        let end = self.written.checked_add(bytes.len()).ok_or(shortfall)?;
        let target = self.out.get_mut(self.written..end).ok_or(shortfall)?;
        for (slot, byte) in target.iter_mut().zip(bytes) {
            slot.write(*byte);
        }
        self.written = end;
        Ok(())
    }

    /// Reserves `len` zeroed bytes to be filled later with [`Self::patch`].
    ///
    /// # Errors
    /// Reports a destination that cannot hold them.
    /// # Safety-usable invariant
    /// Success initializes precisely the returned in-bounds range to0 and advances
    /// written only afterwards. Existing initialized bytes are never deinitialized.
    pub fn reserve(&mut self, len: usize) -> Result<ReservedRange, WireError> {
        let start = self.written;
        let shortfall = WireError::BufferTooSmall {
            needed: len,
            available: self.remaining(),
        };
        let end = start.checked_add(len).ok_or(shortfall)?;
        for slot in self.out.get_mut(start..end).ok_or(shortfall)? {
            slot.write(0);
        }
        self.written = end;
        Ok(ReservedRange { start, len })
    }

    /// Fills a range from [`Self::reserve`], which must be exactly `bytes` long.
    ///
    /// # Errors
    /// Reports a range and value of different lengths.
    pub fn patch(&mut self, range: ReservedRange, bytes: &[u8]) -> Result<(), WireError> {
        let shortfall = WireError::BufferTooSmall {
            needed: bytes.len(),
            available: range.len,
        };
        if range.len != bytes.len() {
            return Err(shortfall);
        }
        let end = range.start.saturating_add(range.len);
        if end > self.written {
            return Err(shortfall);
        }
        let target = self.out.get_mut(range.start..end).ok_or(shortfall)?;
        for (slot, byte) in target.iter_mut().zip(bytes) {
            slot.write(*byte);
        }
        Ok(())
    }

    /// Appends `len` zero bytes and returns them for encoders that write through
    /// `&mut [u8]`, such as Protobuf over the size it computed.
    ///
    /// # Errors
    /// Reports a destination that cannot hold them.
    pub fn zeroed(&mut self, len: usize) -> Result<&mut [u8], WireError> {
        let range = self.reserve(len)?;
        let start = range.start;
        let end = start.saturating_add(len);
        let target = self
            .out
            .get_mut(start..end)
            .ok_or(WireError::BufferTooSmall {
                needed: len,
                available: 0,
            })?;
        // SAFETY: `reserve` wrote every byte of this range just above, and
        // `MaybeUninit<u8>` has the layout of `u8`.
        Ok(unsafe { &mut *(core::ptr::from_mut(target) as *mut [u8]) })
    }

    /// The bytes written so far.
    #[must_use]
    pub fn as_written(&self) -> &[u8] {
        let Some(written) = self.out.get(..self.written) else {
            return &[];
        };
        // SAFETY: the writer only exposes bytes it wrote (`written` advances
        // only after initialization), and `MaybeUninit<u8>` has the layout of `u8`.
        unsafe { &*(core::ptr::from_ref(written) as *const [u8]) }
    }
}

#[cfg(test)]
mod initialization_regressions {
    use super::{PayloadWriter, WireError};
    use crate::transport::slots::as_uninit_mut;

    // [utest->req~ssr-unsafe-wire-untrusted~1]
    #[test]
    fn short_buffer_and_range_errors_preserve_initialized_slot_bytes() -> Result<(), WireError> {
        let mut bytes = [91_u8, 92, 93, 94];
        {
            // SAFETY: as_uninit_mut requires every byte to remain initialized on
            // every exit throughout its returned borrow. bytes is exclusively
            // borrowed here and starts fully initialized. The view goes only to
            // crate-controlled PayloadWriter::new: its documented preservation
            // invariant and put/zeroed bodies write initialized u8 values or
            // return before writing; no raw view or callback escapes. Unwinding
            // an assertion drops only this writer, never deinitializing storage.
            // The writer/view borrow ends before bytes is read below.
            let mut writer = PayloadWriter::new(unsafe { as_uninit_mut(&mut bytes) });
            writer.put(&[11, 12])?;
            assert!(matches!(
                writer.put(&[21, 22, 23]),
                Err(WireError::BufferTooSmall { needed: 3, available: 2 })
            ));
            assert!(matches!(
                writer.zeroed(3),
                Err(WireError::BufferTooSmall { needed: 3, available: 2 })
            ));
            assert_eq!(writer.written(), 2);
            assert_eq!(writer.remaining(), 2);
            assert_eq!(writer.as_written(), &[11, 12]);
        }
        // Read the complete initialized slot, including the unwritten suffix.
        assert_eq!(bytes, [11, 12, 93, 94]);

        {
            // SAFETY: bytes still contains four initialized u8 values, with one
            // exclusive borrow. Only PayloadWriter's crate-controlled put,
            // reserve and patch methods receive the view; their checked error
            // branches preserve storage and successful writes initialize u8.
            // No callback/raw-view escape occurs, including on ? or assertion
            // unwind. The temporary writer relinquishes its borrow before the
            // following complete-slot read; no byte is deinitialized or moved.
            let mut writer = PayloadWriter::new(unsafe { as_uninit_mut(&mut bytes) });
            writer.put(&[21, 22, 23])?;
            assert!(matches!(
                writer.reserve(usize::MAX),
                Err(WireError::BufferTooSmall { needed: usize::MAX, available: 1 })
            ));
            assert_eq!(writer.written(), 3);
            assert_eq!(writer.remaining(), 1);
            assert_eq!(writer.as_written(), &[21, 22, 23]);
        }
        assert_eq!(bytes, [21, 22, 23, 94]);

        {
            // SAFETY: the same all-exit initialized-byte contract holds: bytes
            // is exclusively borrowed, all four values are initialized, and
            // the view is confined to PayloadWriter. Its safe put/reserve/patch
            // operations preserve initialization on success, error and unwind;
            // none invokes caller code or exposes the MaybeUninit slice.
            // The borrow ends here before the complete-slot read below.
            let mut writer = PayloadWriter::new(unsafe { as_uninit_mut(&mut bytes) });
            writer.put(&[31, 32, 33])?;
            let range = writer.reserve(1)?;
            assert_eq!(writer.as_written(), &[31, 32, 33, 0]);
            assert!(matches!(
                writer.patch(range, &[41, 42]),
                Err(WireError::BufferTooSmall { needed: 2, available: 1 })
            ));
            assert_eq!(writer.written(), 4);
            assert_eq!(writer.as_written(), &[31, 32, 33, 0]);
            writer.patch(range, &[34])?;
        }
        assert_eq!(bytes, [31, 32, 33, 34]);

        let mut empty = [];
        {
            // SAFETY: empty is a valid exclusive zero-length initialized-byte
            // slice. as_uninit_mut preserves its extent/lifetime; PayloadWriter
            // cannot expose or deinitialize any bytes, and these checked writes
            // have zero available capacity. The view stays local to this scope.
            let mut writer = PayloadWriter::new(unsafe { as_uninit_mut(&mut empty) });
            writer.put(&[])?;
            assert!(matches!(
                writer.put(&[1]),
                Err(WireError::BufferTooSmall { needed: 1, available: 0 })
            ));
            assert_eq!(writer.written(), 0);
            assert!(writer.as_written().is_empty());
        }
        assert!(empty.is_empty());
        Ok(())
    }
}

