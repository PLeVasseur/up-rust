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
//! Reserved in-process slot storage for allocation and ownership conformance.
//! Native bindings supply their own storage and lifetime implementations.

use std::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::{
        atomic::{compiler_fence, AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use crate::WrittenHeader;
#[cfg(test)]
use crate::{write_header_uninit, TxHeaderParts, UFrameHeaderError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PoolError {
    InvalidCapacity,
    Allocation,
    Exhausted,
    InvalidLayout,
    ReferenceLimit,
    #[cfg(test)]
    Header(UFrameHeaderError),
}

#[derive(Clone, Copy, Debug, Default)]
struct Layout {
    header_len: usize,
    payload_offset: usize,
    payload_len: usize,
    token: Option<(usize, usize)>,
}

struct Slot {
    bytes: UnsafeCell<Box<[MaybeUninit<u8>]>>,
    start: usize,
    readers: AtomicUsize,
    layout: UnsafeCell<Layout>,
}

// SAFETY: an index leaves the free list before its single SlotLoan is constructed.
// Only that non-cloneable loan mutates storage/layout. Publication ends the writer
// and releases the initialized layout before readers acquire it. The last reader
// wipes the owned token range before putting the index back under the free-list
// mutex. No slot storage moves or resizes after creation.
unsafe impl Sync for Slot {}

pub(crate) struct SlotPool {
    slots: Box<[Slot]>,
    free: Mutex<Vec<usize>>,
    capacity: usize,
    alignment: usize,
    exhausted: AtomicUsize,
    wiped: AtomicUsize,
}

impl SlotPool {
    pub(crate) fn new(
        count: usize,
        capacity: usize,
        alignment: usize,
    ) -> Result<Arc<Self>, PoolError> {
        if count == 0 || capacity == 0 || !alignment.is_power_of_two() {
            return Err(PoolError::InvalidCapacity);
        }
        let storage_len = capacity
            .checked_add(alignment - 1)
            .ok_or(PoolError::InvalidCapacity)?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(count)
            .map_err(|_| PoolError::Allocation)?;
        for _ in 0..count {
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(storage_len)
                .map_err(|_| PoolError::Allocation)?;
            bytes.resize_with(storage_len, MaybeUninit::uninit);
            let bytes = bytes.into_boxed_slice();
            let start = (bytes.as_ptr() as usize).wrapping_neg() & (alignment - 1);
            slots.push(Slot {
                bytes: UnsafeCell::new(bytes),
                start,
                readers: AtomicUsize::new(0),
                layout: UnsafeCell::new(Layout::default()),
            });
        }
        let mut free = Vec::new();
        free.try_reserve_exact(count)
            .map_err(|_| PoolError::Allocation)?;
        free.extend((0..count).rev());
        Ok(Arc::new(Self {
            slots: slots.into_boxed_slice(),
            free: Mutex::new(free),
            capacity,
            alignment,
            exhausted: AtomicUsize::new(0),
            wiped: AtomicUsize::new(0),
        }))
    }

    pub(crate) fn loan(self: &Arc<Self>) -> Result<SlotLoan, PoolError> {
        let index = self
            .free
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop();
        match index {
            Some(index) => Ok(SlotLoan {
                pool: Arc::clone(self),
                index,
                header: None,
                published: false,
            }),
            None => {
                self.exhausted.fetch_add(1, Ordering::Relaxed);
                Err(PoolError::Exhausted)
            }
        }
    }

    fn reclaim(&self, index: usize, token: Option<(usize, usize)>) {
        let slot = self.slot(index);
        if let Some((offset, len)) = token {
            // SAFETY: reclaim runs with the sole unpublished loan, or after the
            // final reader released its handle. The index is not yet available
            // to another writer; the checked token range belongs to this slot.
            let bytes = unsafe { &mut *slot.bytes.get() };
            let range = bytes
                .get_mut(slot.start + offset..slot.start + offset + len)
                .expect("checked token range");
            for byte in range {
                // SAFETY: each pointer is a valid, uniquely owned u8 destination.
                // A volatile zero store initializes it and cannot be elided.
                unsafe { byte.as_mut_ptr().write_volatile(0) };
            }
            compiler_fence(Ordering::SeqCst);
            self.wiped.fetch_add(1, Ordering::Relaxed);
        }
        self.free
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(index);
    }

    pub(crate) fn free_slots(&self) -> usize {
        self.free
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    fn slot(&self, index: usize) -> &Slot {
        self.slots
            .get(index)
            .expect("index is owned by a live pool handle")
    }
}

pub(crate) struct SlotLoan {
    pool: Arc<SlotPool>,
    index: usize,
    header: Option<WrittenHeader>,
    published: bool,
}

impl SlotLoan {
    #[cfg(feature = "transport-implementer-api")]
    pub(crate) fn index(&self) -> usize {
        self.index
    }

    #[cfg(feature = "transport-implementer-api")]
    pub(crate) fn record_header(&mut self, header: WrittenHeader) -> Result<(), PoolError> {
        if header.len > self.pool.capacity
            || header.token.is_some_and(|(offset, len)| {
                offset.checked_add(len).is_none_or(|end| end > header.len)
            })
        {
            return Err(PoolError::InvalidLayout);
        }
        self.header = Some(header);
        Ok(())
    }

    pub(crate) fn capacity(&self) -> usize {
        self.pool.capacity
    }

    pub(crate) fn bytes_uninit(&mut self) -> &mut [MaybeUninit<u8>] {
        let slot = self.pool.slot(self.index);
        // SAFETY: this is the only loan for its removed index; readers cannot
        // exist until this loan is consumed by publish. The borrow prevents
        // publishing or dropping the loan while a mutable slice remains live.
        let bytes = unsafe { &mut *slot.bytes.get() };
        bytes
            .get_mut(slot.start..slot.start + self.pool.capacity)
            .expect("reserved aligned extent")
    }

    #[cfg(test)]
    pub(crate) fn write_header(
        &mut self,
        prefix: &[u8],
        parts: &TxHeaderParts<'_>,
    ) -> Result<WrittenHeader, PoolError> {
        if self.header.is_some() {
            return Err(PoolError::InvalidLayout);
        }
        let header =
            write_header_uninit(prefix, parts, self.bytes_uninit()).map_err(PoolError::Header)?;
        // Keep the range immediately, so cancelling later payload initialization
        // still wipes the access token written by this operation.
        self.header = Some(header);
        Ok(header)
    }

    /// Publishes a successfully initialized payload without copying it.
    ///
    /// # Safety
    /// Every byte in the checked payload range must be initialized. Header bytes
    /// were initialized by write_header; the uninitialized gap is never exposed.
    /// The caller must separately validate any typed payload witness/field bits.
    pub(crate) unsafe fn publish(
        mut self,
        payload_offset: usize,
        payload_len: usize,
    ) -> Result<SlotRef, PoolError> {
        let header = self.header.ok_or(PoolError::InvalidLayout)?;
        let end = payload_offset
            .checked_add(payload_len)
            .ok_or(PoolError::InvalidLayout)?;
        if payload_offset < header.len
            || end > self.pool.capacity
            || !payload_offset.is_multiple_of(self.pool.alignment)
        {
            return Err(PoolError::InvalidLayout);
        }
        let slot = self.pool.slot(self.index);
        let layout = Layout {
            header_len: header.len,
            payload_offset,
            payload_len,
            token: header.token,
        };
        // SAFETY: the unpublished loan exclusively owns this slot and layout.
        unsafe { *slot.layout.get() = layout };
        slot.readers.store(1, Ordering::Release);
        self.published = true;
        Ok(SlotRef {
            pool: Arc::clone(&self.pool),
            index: self.index,
        })
    }
}

impl Drop for SlotLoan {
    fn drop(&mut self) {
        if !self.published {
            self.pool
                .reclaim(self.index, self.header.and_then(|header| header.token));
        }
    }
}

pub(crate) struct SlotRef {
    pool: Arc<SlotPool>,
    index: usize,
}

impl SlotRef {
    pub(crate) fn try_retain(&self) -> Result<Self, PoolError> {
        let readers = &self.pool.slot(self.index).readers;
        let mut value = readers.load(Ordering::Acquire);
        loop {
            let next = value
                .checked_add(1)
                .filter(|next| value != 0 && *next <= isize::MAX as usize)
                .ok_or(PoolError::ReferenceLimit)?;
            match readers.compare_exchange_weak(value, next, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break,
                Err(actual) => value = actual,
            }
        }
        Ok(Self {
            pool: Arc::clone(&self.pool),
            index: self.index,
        })
    }

    fn layout(&self) -> Layout {
        let slot = self.pool.slot(self.index);
        assert_ne!(
            slot.readers.load(Ordering::Acquire),
            0,
            "live slot reference"
        );
        // SAFETY: publication initialized the layout before its Release store;
        // the live reader prevents reuse/mutation until this handle is released.
        unsafe { *slot.layout.get() }
    }

    fn initialized(&self, offset: usize, len: usize) -> &[u8] {
        let slot = self.pool.slot(self.index);
        // SAFETY: the live reader pins this allocation and prevents a writer.
        let storage = unsafe { &*slot.bytes.get() };
        let bytes = storage
            .get(slot.start + offset..slot.start + offset + len)
            .expect("published initialized extent");
        // SAFETY: callers use only the checked header/payload ranges initialized
        // before publication. No reference to the uninitialized gap is formed.
        unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<u8>(), bytes.len()) }
    }

    pub(crate) fn header(&self) -> &[u8] {
        let layout = self.layout();
        self.initialized(0, layout.header_len)
    }
    pub(crate) fn payload(&self) -> &[u8] {
        let layout = self.layout();
        self.initialized(layout.payload_offset, layout.payload_len)
    }
}

impl std::fmt::Debug for SlotRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlotRef")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

impl Drop for SlotRef {
    fn drop(&mut self) {
        let slot = self.pool.slot(self.index);
        if slot.readers.fetch_sub(1, Ordering::AcqRel) == 1 {
            // SAFETY: this was the last reference and no writer can acquire the
            // index until reclaim publishes it to the mutex-protected free list.
            let token = unsafe { (*slot.layout.get()).token };
            self.pool.reclaim(self.index, token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_header, HeaderLimits, PayloadEncoding, UFrameMetadata, UUri};

    fn metadata() -> UFrameMetadata {
        UFrameMetadata::publish(UUri::try_from_wire_parts("pool", 0x0001_0001, 1, 0x8001).unwrap())
            .with_token("secret-token")
            .with_payload_encoding(PayloadEncoding::RAW)
            .build()
            .unwrap()
    }

    fn count_only_frame(pool: &Arc<SlotPool>) -> SlotRef {
        let mut loan = pool.loan().unwrap();
        pool.slot(loan.index).readers.store(1, Ordering::Release);
        loan.published = true;
        SlotRef {
            pool: Arc::clone(pool),
            index: loan.index,
        }
    }

    #[test]
    fn retain_rejects_zero_and_reference_limits_without_mutating_count() {
        let pool = SlotPool::new(1, 128, 8).unwrap();
        let frame = count_only_frame(&pool);
        let readers = &pool.slot(frame.index).readers;
        for value in [0, isize::MAX as usize, isize::MAX as usize + 1, usize::MAX] {
            readers.store(value, Ordering::Release);
            let result = frame.try_retain();
            let observed = readers.load(Ordering::Acquire);
            readers.store(1 + usize::from(result.is_ok()), Ordering::Release);
            assert!(matches!(&result, Err(PoolError::ReferenceLimit)));
            assert_eq!(observed, value);
            drop(result);
        }

        readers.store(isize::MAX as usize - 1, Ordering::Release);
        let result = frame.try_retain();
        let observed = readers.load(Ordering::Acquire);
        readers.store(1 + usize::from(result.is_ok()), Ordering::Release);
        assert_eq!(observed, isize::MAX as usize);
        drop(result.expect("the last allowed retain must succeed"));
        assert_eq!(readers.load(Ordering::Acquire), 1);
        assert_eq!(pool.free_slots(), 0);
        drop(frame);
        assert_eq!(readers.load(Ordering::Acquire), 0);
        assert_eq!(pool.free_slots(), 1);
    }

    #[test]
    fn concurrent_retain_release_preserves_the_root_until_last_drop() {
        const WORKERS: usize = 8;
        const PER_WORKER: usize = 16;
        let pool = SlotPool::new(1, 128, 8).unwrap();
        let frame = count_only_frame(&pool);
        let readers = &pool.slot(frame.index).readers;
        let start = std::sync::Barrier::new(WORKERS);
        let held_count = std::thread::scope(|scope| {
            let (ready, received) = std::sync::mpsc::channel();
            let mut releases = Vec::new();
            let mut workers = Vec::new();
            for _ in 0..WORKERS {
                let frame = &frame;
                let start = &start;
                let ready = ready.clone();
                let (release, released) = std::sync::mpsc::channel::<()>();
                releases.push(release);
                workers.push(scope.spawn(move || {
                    start.wait();
                    let retained: Vec<_> = (0..PER_WORKER)
                        .map(|_| frame.try_retain().unwrap())
                        .collect();
                    ready.send(()).unwrap();
                    drop(ready);
                    let _ = released.recv();
                    drop(retained);
                    for _ in 0..64 {
                        let retained = frame.try_retain().unwrap();
                        std::thread::yield_now();
                        drop(retained);
                    }
                }));
            }
            drop(ready);
            for _ in 0..WORKERS {
                received.recv().unwrap();
            }
            let held_count = readers.load(Ordering::Acquire);
            drop(releases);
            for worker in workers {
                worker.join().unwrap();
            }
            held_count
        });
        assert_eq!(held_count, 1 + WORKERS * PER_WORKER);
        assert_eq!(readers.load(Ordering::Acquire), 1);
        assert_eq!(pool.free_slots(), 0);
        drop(frame);
        assert_eq!(readers.load(Ordering::Acquire), 0);
        assert_eq!(pool.free_slots(), 1);
    }

    #[test]
    fn last_reader_reclaims_and_wipes_token() {
        let pool = SlotPool::new(1, 512, 64).unwrap();
        let mut loan = pool.loan().unwrap();
        assert_eq!(loan.capacity(), 512);
        assert!(matches!(pool.loan(), Err(PoolError::Exhausted)));
        let metadata = metadata();
        let written = loan
            .write_header(b"prefix", &TxHeaderParts::from(&metadata))
            .unwrap();
        for (slot, byte) in loan
            .bytes_uninit()
            .get_mut(256..260)
            .unwrap()
            .iter_mut()
            .zip(b"data")
        {
            slot.write(*byte);
        }
        // SAFETY: the four payload bytes were initialized above; write_header
        // initialized the metadata. The gap is deliberately left uninitialized.
        let frame = unsafe { loan.publish(256, 4) }.unwrap();
        assert_eq!(frame.payload(), b"data");
        assert_eq!((frame.payload().as_ptr() as usize) % 64, 0);
        assert_eq!(
            parse_header(
                b"prefix",
                frame.header(),
                &HeaderLimits {
                    max_token_len: 2048
                }
            )
            .unwrap()
            .token(),
            Some("secret-token")
        );
        let retained = frame.try_retain().unwrap();
        drop(frame);
        assert_eq!(pool.free_slots(), 0);
        assert_eq!(retained.payload(), b"data");
        drop(retained);
        assert_eq!(pool.free_slots(), 1);
        assert_eq!(pool.wiped.load(Ordering::Relaxed), 1);
        let mut reused = pool.loan().unwrap();
        let (offset, len) = written.token.unwrap();
        for byte in reused.bytes_uninit().get(offset..offset + len).unwrap() {
            // SAFETY: reclaim initialized the recorded token range with zeros.
            assert_eq!(unsafe { byte.assume_init() }, 0);
        }
    }

    #[test]
    fn cancellation_wipes_header_without_reading_uninitialized_payload() {
        let pool = SlotPool::new(1, 512, 8).unwrap();
        let mut loan = pool.loan().unwrap();
        let metadata = metadata();
        loan.write_header(b"prefix", &TxHeaderParts::from(&metadata))
            .unwrap();
        drop(loan);
        assert_eq!(pool.free_slots(), 1);
        assert_eq!(pool.wiped.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn hostile_capacities_and_layouts_are_rejected() {
        assert!(matches!(
            SlotPool::new(0, 128, 8),
            Err(PoolError::InvalidCapacity)
        ));
        assert!(matches!(
            SlotPool::new(1, usize::MAX, 8),
            Err(PoolError::InvalidCapacity)
        ));
        assert!(matches!(
            SlotPool::new(1, 128, 3),
            Err(PoolError::InvalidCapacity)
        ));
        let pool = SlotPool::new(1, 512, 8).unwrap();
        let mut loan = pool.loan().unwrap();
        loan.write_header(b"prefix", &TxHeaderParts::from(&metadata()))
            .unwrap();
        // SAFETY: the zero-length payload has no initialization obligation;
        // publication must reject its overlapping/misaligned position first.
        assert!(matches!(
            // SAFETY: zero payload bytes require no initialization; the invalid
            // position must be rejected before any receive view can be formed.
            unsafe { loan.publish(1, 0) },
            Err(PoolError::InvalidLayout)
        ));
        assert_eq!(pool.free_slots(), 1);
    }
}
