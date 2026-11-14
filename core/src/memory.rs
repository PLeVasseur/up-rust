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

//! Startup memory: a claim-once region, checked placement and fixed-capacity containers.
//!
//! Objects placed here are never dropped or freed, which is R38 by construction.
//! Every placement goes through one function, `StartupMemory::reserve`, that
//! checks bounds, alignment and exclusivity (R42).
//!
//! Memory comes from a [`RegionSource`]. The core sources are a [`StartupRegion`]
//! in a `static` and caller-provided bytes ([`StartupMemory::new`]). `up-rust`
//! supplies one that leaks a fresh boxed slice per request (R15), so a `std` path
//! needs no size up front and a 48 MB route just works.

use core::cell::UnsafeCell;
use core::fmt;
use core::mem::{align_of, size_of, MaybeUninit};
use core::ptr;
use crate::platform::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};

/// A refusal in initialization mode. Every variant is a small `Copy` value,
/// so a refusal cannot allocate (R3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StartupError {
    /// The startup memory cannot satisfy the request.
    MemoryExhausted {
        /// Bytes requested, not counting alignment padding.
        requested: usize,
        /// Bytes left in the current region.
        remaining: usize,
    },
    /// An internal reservation requested a non-power-of-two alignment.
    InvalidAlignment {
        /// Alignment reported by the platform reservation.
        alignment: usize,
    },
    /// A second [`StartupRegion::claim`].
    RegionAlreadyClaimed,
    /// A placement or a new setup after [`StartupMemory::seal`], after the last
    /// setup sharing the memory ended once a transport had started, or through the
    /// caller's own handle once any transport using it has entered runtime.
    RegionSealed,
    /// A fixed-capacity table is full.
    TableFull,
}

impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MemoryExhausted {
                requested,
                remaining,
            } => write!(
                f,
                "startup memory exhausted: requested {requested} bytes, {remaining} bytes remain"
            ),
            Self::InvalidAlignment { alignment } => write!(f, "invalid startup alignment: {alignment}"),
            Self::RegionAlreadyClaimed => f.write_str("startup region is already claimed"),
            Self::RegionSealed => f.write_str("startup memory is sealed"),
            Self::TableFull => f.write_str("fixed-capacity table is full"),
        }
    }
}

impl core::error::Error for StartupError {}

/// Supplies startup memory on request.
///
/// A region stays in use for the rest of the process. Each call must return
/// memory that was not handed out before. Returning `None` means "no more
/// memory", which [`StartupMemory`] reports as [`StartupError::MemoryExhausted`].
/// The `&'static mut` return type is the exclusivity proof, so implementing this
/// trait needs no `unsafe`.
pub trait RegionSource: Sync {
    /// Returns a region of at least `min_len` bytes, or `None` if it has none.
    // The result is not reborrowed from `self`: each region is handed out once.
    #[allow(clippy::mut_from_ref)]
    fn region(&'static self, min_len: usize) -> Option<&'static mut [MaybeUninit<u8>]>;
}

/// Region meant to live in a `static`. Sized at build time, claimed once.
///
/// It can back a [`StartupMemory`] directly ([`Self::claim`]) or act as a
/// one-shot [`RegionSource`].
#[repr(C, align(16))]
pub struct StartupRegion<const N: usize> {
    // Safety invariant: false permits exactly one future successful CAS to true;
    // true never becomes false and denies all new accesses to bytes through self.
    claimed: AtomicBool,
    // Safety invariant: live fixed allocation; once claimed its exclusive access
    // belongs only to the returned static mutable slice, never to this wrapper.
    bytes: UnsafeCell<[MaybeUninit<u8>; N]>,
}

// SAFETY: `bytes` is reached only through the single `StartupMemory` that the
// claim flag can return, and `claim`/`region` are guarded by the atomic flag.
unsafe impl<const N: usize> Sync for StartupRegion<N> {}

impl<const N: usize> fmt::Debug for StartupRegion<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StartupRegion")
            .field("len", &N)
            .field("claimed", &self.claimed.load(Ordering::Relaxed))
            .finish()
    }
}

impl<const N: usize> Default for StartupRegion<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> StartupRegion<N> {
    /// Creates an unclaimed region. Usable in a `static` initializer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            claimed: AtomicBool::new(false),
            bytes: UnsafeCell::new([MaybeUninit::uninit(); N]),
        }
    }

    // The `claimed` flag lets exactly one caller have this borrow.
    #[allow(clippy::mut_from_ref)]
    /// # Safety-usable invariant
    /// Some is the only exclusive static borrow ever issued for this region,
    /// covering N initialized MaybeUninit elements. None grants no byte access.
    fn take(&'static self) -> Option<&'static mut [MaybeUninit<u8>]> {
        if self
            .claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return None;
        }
        // SAFETY: the flag above lets exactly one caller reach this point, the
        // region is `'static`, and the pointer covers the whole `N`-element array.
        Some(unsafe {
            core::slice::from_raw_parts_mut(self.bytes.get().cast::<MaybeUninit<u8>>(), N)
        })
    }

    /// Claims the region. Only the first claim succeeds (exclusivity check at creation).
    ///
    /// # Errors
    /// Returns [`StartupError::RegionAlreadyClaimed`] on any later claim, and
    /// [`StartupError::MemoryExhausted`] if the region cannot hold its header.
    pub fn claim(&'static self) -> Result<StartupMemory, StartupError> {
        match self.take() {
            Some(bytes) => StartupMemory::new(bytes),
            None => Err(StartupError::RegionAlreadyClaimed),
        }
    }
}

impl<const N: usize> RegionSource for StartupRegion<N> {
    fn region(&'static self, min_len: usize) -> Option<&'static mut [MaybeUninit<u8>]> {
        if min_len > N {
            return None;
        }
        self.take()
    }
}

/// Bump state of one region, stored at the start of that region.
#[repr(C)]
struct RegionHeader {
    // Safety invariant: M-region: nonnull provenance into one permanent
    // allocation containing capacity bytes, capacity<=isize::MAX; header bytes
    // are initialized/aligned and excluded from all issued ranges.
    base: *mut MaybeUninit<u8>,
    // Safety invariant: immutable extent of the same M-region allocation.
    capacity: usize,
    // Safety invariant: <=capacity, monotone; all nonempty issued ranges are
    // disjoint and below cursor. Reserved memory is never reclaimed or reissued.
    cursor: AtomicUsize,
    // Safety invariant: null or a fully initialized immutable permanent header;
    // nonnull publication uses Release, readers use Acquire before dereference.
    next: AtomicPtr<RegionHeader>,
}

// SAFETY: `base` is never written through this reference; `reserve` hands out
// disjoint ranges claimed by compare-exchange on `cursor`, and the other fields
// are atomics or immutable.
unsafe impl Sync for RegionHeader {}

impl RegionHeader {
    /// Writes a header at the start of `bytes` and returns it.
    /// # Safety-usable invariant
    /// Success permanently consumes the input's exclusive storage, initializes
    /// an aligned header within it and establishes M-region. Its cursor starts
    /// after that header; future bump ranges cannot overlap it. No mutable input
    /// borrow is retained or exposed independently of the header's allocator.
    fn place(bytes: &'static mut [MaybeUninit<u8>]) -> Result<&'static RegionHeader, StartupError> {
        let capacity = bytes.len();
        let base = bytes.as_mut_ptr();
        let header_size = size_of::<RegionHeader>();
        let exhausted = StartupError::MemoryExhausted {
            requested: header_size,
            remaining: capacity,
        };
        let mask = align_of::<RegionHeader>().wrapping_sub(1);
        let offset = (base as usize).wrapping_neg() & mask;
        let cursor = offset.checked_add(header_size).ok_or(exhausted)?;
        if cursor > capacity {
            return Err(exhausted);
        }
        // SAFETY: `offset..cursor` is inside `bytes`, aligned for the header, and
        // exclusively ours because `bytes` was `&'static mut`.
        let slot = unsafe { &mut *base.add(offset).cast::<MaybeUninit<RegionHeader>>() };
        Ok(slot.write(RegionHeader {
            base,
            capacity,
            cursor: AtomicUsize::new(cursor),
            next: AtomicPtr::new(ptr::null_mut()),
        }))
    }

    /// Bumps the cursor for `size` bytes at `align`. `Err` carries the bytes left.
    /// # Safety-usable invariant
    /// Ok(p) uniquely reserves size bytes in this M-region allocation, disjoint
    /// from every previous nonempty reservation and header. It preserves original
    /// provenance, is nonnull, and the whole range fits isize without wrapping.
    /// With nonzero power-of-two align, p has that alignment. This conditional
    /// alignment guarantee is not a requirement for safely calling the function.
    /// Bytes may be uninitialized; storage remains live forever. Zero-size
    /// reservations can share an aligned address but grant no overlapping bytes.
    fn bump(&self, size: usize, align: usize) -> Result<*mut MaybeUninit<u8>, usize> {
        let mask = align.wrapping_sub(1);
        loop {
            let used = self.cursor.load(Ordering::Acquire);
            let remaining = self.capacity.saturating_sub(used);
            let addr = (self.base as usize).wrapping_add(used);
            let padding = addr.wrapping_neg() & mask;
            let Some(start) = used.checked_add(padding) else {
                return Err(remaining);
            };
            let Some(end) = start.checked_add(size) else {
                return Err(remaining);
            };
            if end > self.capacity {
                return Err(remaining);
            }
            if self
                .cursor
                .compare_exchange(used, end, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                // SAFETY: `start..end` is inside the region and the compare-exchange
                // gave this caller exclusive ownership of it, because the cursor
                // never moves backward.
                return Ok(unsafe { self.base.add(start) });
            }
        }
    }
}

/// The region list, stored in the first region.
struct Chain {
    // Safety invariant: nonnull initialized permanent M-region header. Links
    // reachable from first satisfy RegionHeader.next; Acquire precedes raw access.
    first: AtomicPtr<RegionHeader>,
    // Safety invariant: nonnull initialized permanent M-region header published
    // with Release; never points into storage issued as an application range.
    current: AtomicPtr<RegionHeader>,
    source: Option<&'static dyn RegionSource>,
    sealed: AtomicBool,
    /// Setups sharing this memory that have not yet entered runtime.
    setups: AtomicUsize,
    /// Whether any setup sharing this memory has entered runtime.
    runtime_started: AtomicBool,
}

// SAFETY: every field is an atomic or a shared reference to a `Sync` value.
unsafe impl Sync for Chain {}

/// Handle for placing startup objects. Not `Copy`, so only `seal` can end the caller's use.
///
/// The caller's handle serves initialization only: once any transport using this
/// memory has entered runtime it places nothing and starts no setup
/// (`RegionSealed`), whatever other setups are still alive. A setup started before
/// then keeps placing through its own handle until it enters runtime.
pub struct StartupMemory {
    chain: &'static Chain,
    /// A setup's own handle, from `share_for_setup`; the caller's handle otherwise.
    setup: bool,
}

/// One setup's use of a [`StartupMemory`] it shares with others.
///
/// Once some setup has entered runtime and the last claim ends, the memory is
/// sealed for every handle. The caller's handle stops earlier, at the first
/// `enter_runtime` (R2, R3): no placement and no new setup after that, so a setup
/// kept alive, or forgotten, cannot hold initialization open for it. Every setup
/// started before then is served until it enters runtime, and a setup dropped
/// before any transport started leaves the memory open.
pub(crate) struct SetupClaim {
    chain: &'static Chain,
}

impl fmt::Debug for SetupClaim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetupClaim").finish_non_exhaustive()
    }
}

impl Drop for SetupClaim {
    fn drop(&mut self) {
        let last = self.chain.setups.fetch_sub(1, Ordering::AcqRel) == 1;
        if last && self.chain.runtime_started.load(Ordering::Acquire) {
            self.chain.sealed.store(true, Ordering::Release);
        }
    }
}

impl fmt::Debug for StartupMemory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let report = self.report();
        f.debug_struct("StartupMemory")
            .field("regions", &report.regions)
            .field("capacity", &report.capacity)
            .field("used", &report.used)
            .field("sealed", &self.chain.sealed.load(Ordering::Relaxed))
            .finish()
    }
}

/// What `seal` reports, as input to the storage estimate (ISO 26262-6 7.4.13).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryReport {
    /// Bytes consumed including alignment padding and headers, across all regions.
    pub used: usize,
    /// Total size of all regions.
    pub capacity: usize,
    /// Number of regions in use.
    pub regions: usize,
}

impl StartupMemory {
    pub(crate) fn identity(&self) -> *const () {
        core::ptr::from_ref(self.chain).cast()
    }

    /// Wraps caller-provided memory, such as a linker section. No further memory
    /// can be obtained; a request that does not fit is refused.
    ///
    /// # Errors
    /// Returns [`StartupError::MemoryExhausted`] if the bookkeeping does not fit.
    pub fn new(bytes: &'static mut [MaybeUninit<u8>]) -> Result<Self, StartupError> {
        Self::with_region(bytes, None)
    }

    /// Takes memory from `source`, in as many regions as requests need.
    ///
    /// `first_len` is the size to ask for up front; later regions are requested
    /// as needed, each large enough for its request.
    ///
    /// # Errors
    /// Returns [`StartupError::MemoryExhausted`] if the source cannot supply the
    /// first region or the bookkeeping does not fit in it.
    pub fn from_source(
        source: &'static dyn RegionSource,
        first_len: usize,
    ) -> Result<Self, StartupError> {
        let needed = first_len
            .max(size_of::<RegionHeader>().saturating_add(size_of::<Chain>()))
            .saturating_add(align_of::<RegionHeader>())
            .saturating_add(align_of::<Chain>());
        let bytes = source.region(needed).ok_or(StartupError::MemoryExhausted {
            requested: needed,
            remaining: 0,
        })?;
        Self::with_region(bytes, Some(source))
    }

    fn with_region(
        bytes: &'static mut [MaybeUninit<u8>],
        source: Option<&'static dyn RegionSource>,
    ) -> Result<Self, StartupError> {
        let capacity = bytes.len();
        let region = RegionHeader::place(bytes)?;
        let region_ptr = ptr::from_ref(region).cast_mut();
        let chain_ptr = region
            .bump(size_of::<Chain>(), align_of::<Chain>())
            .map_err(|remaining| StartupError::MemoryExhausted {
                requested: size_of::<Chain>(),
                remaining: remaining.min(capacity),
            })?;
        // SAFETY: `bump` returned exclusive, aligned, in-bounds storage for one
        // `Chain`, valid for `'static` because the region is never reclaimed.
        let chain = unsafe { &mut *chain_ptr.cast::<MaybeUninit<Chain>>() }.write(Chain {
            first: AtomicPtr::new(region_ptr),
            current: AtomicPtr::new(region_ptr),
            source,
            sealed: AtomicBool::new(false),
            setups: AtomicUsize::new(0),
            runtime_started: AtomicBool::new(false),
        });
        Ok(Self {
            chain,
            setup: false,
        })
    }

    /// Whether this handle is past initialization: the memory is sealed, or this is
    /// the caller's handle and a transport using the memory has entered runtime.
    fn past_initialization(&self) -> bool {
        self.chain.sealed.load(Ordering::Acquire)
            || (!self.setup && self.chain.runtime_started.load(Ordering::Acquire))
    }

    /// The single bounds, alignment and exclusivity check (R42), across regions.
    /// # Safety-usable invariant
    /// Ok uniquely reserves size bytes in one permanent M-region, with original
    /// provenance, nonnullness, full extent<=isize::MAX and no wrap. With a
    /// nonzero power-of-two align it is aligned accordingly. No initialized T is
    /// implied. Every nonempty successful range is disjoint from all earlier
    /// reservations and headers; errors expose no new access. Safe RegionSource
    /// implementations cannot return overlapping live &'static mut slices.
    fn reserve(&self, size: usize, align: usize) -> Result<*mut MaybeUninit<u8>, StartupError> {
        let chain = self.chain;
        if self.past_initialization() {
            return Err(StartupError::RegionSealed);
        }
        loop {
            let current_ptr = chain.current.load(Ordering::Acquire);
            // SAFETY: `current` always points to a region header placed by
            // `RegionHeader::place`, valid for `'static`.
            let current = unsafe { &*current_ptr };
            let remaining = match current.bump(size, align) {
                Ok(ptr) => return Ok(ptr),
                Err(remaining) => remaining,
            };
            let refused = StartupError::MemoryExhausted {
                requested: size,
                remaining,
            };
            let Some(source) = chain.source else {
                return Err(refused);
            };
            // Ask for room for this request, its padding and the new header.
            let needed = size
                .checked_add(align)
                .and_then(|value| value.checked_add(size_of::<RegionHeader>()))
                .and_then(|value| value.checked_add(align_of::<RegionHeader>()))
                .ok_or(refused)?;
            let bytes = source.region(needed).ok_or(refused)?;
            let next = RegionHeader::place(bytes)?;
            let next_ptr = ptr::from_ref(next).cast_mut();
            // Link the region. If another thread linked one first, this region is
            // abandoned (it stays allocated, like everything here) and the loop
            // retries against the winner.
            if current
                .next
                .compare_exchange(
                    ptr::null_mut(),
                    next_ptr,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
            {
                let _ = chain.current.compare_exchange(
                    current_ptr,
                    next_ptr,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
            }
        }
    }

    /// Moves `value` into startup memory for the rest of the process. It is never dropped.
    ///
    /// # Errors
    /// Returns an error when the memory is sealed or cannot hold `T`.
    /// # Safety-usable invariant
    /// Ok returns the sole initialized T ownership in a permanent, aligned,
    /// write-valid reservation. The input value has moved there, never copied;
    /// no other reservation overlaps its nonzero extent. Forgetting any owner
    /// cannot revoke this static allocation.
    pub fn place<T: 'static>(&self, value: T) -> Result<&'static mut T, StartupError> {
        let ptr = self.reserve(size_of::<T>(), align_of::<T>())?;
        // SAFETY: `reserve` returned exclusive, aligned, in-bounds storage for one
        // `T`, valid for `'static` because the region is never reclaimed. `write`
        // initializes it fully.
        let slot = unsafe { &mut *ptr.cast::<MaybeUninit<T>>() };
        Ok(slot.write(value))
    }

    /// # Safety-usable invariant
    /// Ok returns exclusive static storage for exactly count MaybeUninit<T>
    /// elements, correctly aligned even for ZST/count0. Checked multiplication
    /// and M-region bounds keep byte extent<=isize::MAX in one allocation.
    /// T values need not be initialized. The mutable slice is the sole capability
    /// to nonzero bytes; no later reservation reissues them.
    pub(crate) fn slice<T: 'static>(
        &self,
        count: usize,
    ) -> Result<&'static mut [MaybeUninit<T>], StartupError> {
        let exhausted = StartupError::MemoryExhausted {
            requested: usize::MAX,
            remaining: 0,
        };
        let bytes = size_of::<T>().checked_mul(count).ok_or(exhausted)?;
        let ptr = self.reserve(bytes, align_of::<T>())?;
        // SAFETY: `reserve` returned exclusive, aligned, in-bounds storage for
        // `count` elements; `MaybeUninit<T>` needs no initialization and the
        // storage lives for `'static`.
        Ok(unsafe { core::slice::from_raw_parts_mut(ptr.cast::<MaybeUninit<T>>(), count) })
    }

    /// Reserves a permanently owned, explicitly aligned worker stack.
    /// # Safety-usable invariant
    /// Success supplies the sole exclusive static byte range in one allocation,
    /// with the requested power-of-two alignment and extent <= isize::MAX. No
    /// initialized objects occupy it; forgetting the slice does not reclaim it.
    pub(crate) fn aligned_bytes(&self, count: usize, alignment: usize)
        -> Result<&'static mut [MaybeUninit<u8>], StartupError>
    {
        if !alignment.is_power_of_two() { return Err(StartupError::InvalidAlignment { alignment }); }
        let pointer = self.reserve(count, alignment)?;
        // SAFETY: reserve's postcondition gives unique permanent, nonnull aligned
        // storage for count bytes in one allocation, even for count=0. Every
        // byte is a valid MaybeUninit<u8>; no initialized u8 is exposed. The
        // returned mutable slice is the only access capability for this range.
        Ok(unsafe { core::slice::from_raw_parts_mut(pointer, count) })
    }

    /// Uninitialized typed slots, for `up-rust`'s contract tests only.
    ///
    /// # Errors
    /// Returns an error when the memory is sealed or cannot hold them.
    #[doc(hidden)]
    #[cfg(up_rust_test_hooks)]
    pub fn __slice_for_tests<T: 'static>(
        &self,
        count: usize,
    ) -> Result<&'static mut [MaybeUninit<T>], StartupError> {
        self.slice::<T>(count)
    }

    /// Records that a transport sharing this memory entered runtime, so it is
    /// sealed when the last setup still using it ends.
    pub(crate) fn note_runtime_started(&self) {
        self.chain.runtime_started.store(true, Ordering::Release);
    }

    /// A second handle on the same memory, kept by the setup that places into it,
    /// with the claim that seals the memory after the last such setup ends.
    ///
    /// Starting a setup is initialization, so it is refused wherever a placement
    /// through `self` is. A transport entering runtime between the check and the
    /// claim is ordered after this setup's start; a seal in between refuses this
    /// setup's first placement. The caller's handle can also seal memory early:
    /// `seal` takes `self`.
    ///
    /// # Errors
    /// [`StartupError::RegionSealed`] once the memory is sealed or, on the caller's
    /// handle, once a transport using this memory has entered runtime.
    pub(crate) fn share_for_setup(&self) -> Result<(StartupMemory, SetupClaim), StartupError> {
        if self.past_initialization() {
            return Err(StartupError::RegionSealed);
        }
        self.chain.setups.fetch_add(1, Ordering::AcqRel);
        Ok((
            StartupMemory {
                chain: self.chain,
                setup: true,
            },
            SetupClaim { chain: self.chain },
        ))
    }

    /// Reserves uninitialized bytes, for example one startup slot.
    ///
    /// # Errors
    /// Returns an error when the memory is sealed or cannot hold `count` bytes.
    /// # Safety-usable invariant
    /// Exactly slice::<u8>(count)'s exclusive static one-allocation storage;
    /// elements are valid MaybeUninit<u8>, not necessarily initialized u8.
    pub fn bytes(&self, count: usize) -> Result<&'static mut [MaybeUninit<u8>], StartupError> {
        self.slice::<u8>(count)
    }

    /// Reserves a fixed-capacity vector.
    ///
    /// # Errors
    /// Returns an error if the memory cannot hold `capacity` elements.
    /// # Safety-usable invariant
    /// Success creates exclusive typed static backing with capacity elements,
    /// an empty initialized prefix, and FixedVec's field invariants established.
    pub fn fixed_vec<T: 'static>(&self, capacity: usize) -> Result<FixedVec<T>, StartupError> {
        Ok(FixedVec {
            slots: self.slice::<T>(capacity)?,
            len: 0,
        })
    }

    fn report(&self) -> MemoryReport {
        let mut report = MemoryReport {
            used: 0,
            capacity: 0,
            regions: 0,
        };
        let mut next = self.chain.first.load(Ordering::Acquire);
        // SAFETY: every link was placed by `RegionHeader::place`, valid for `'static`.
        while let Some(region) = unsafe { next.as_ref() } {
            report.used = report
                .used
                .saturating_add(region.cursor.load(Ordering::Acquire));
            report.capacity = report.capacity.saturating_add(region.capacity);
            report.regions = report.regions.saturating_add(1);
            next = region.next.load(Ordering::Acquire);
        }
        report
    }

    /// Ends initialization for this memory and reports consumption. Afterward no
    /// handle the caller can reach exists, and any setup still alive gets
    /// [`StartupError::RegionSealed`].
    ///
    /// A memory shared with setups is sealed anyway once the last of them has
    /// entered runtime; call this for a memory no setup uses, or to end placement
    /// earlier and take the storage report.
    #[must_use]
    pub fn seal(self) -> MemoryReport {
        self.chain.sealed.store(true, Ordering::Release);
        self.report()
    }
}

/// Fixed-capacity vector over startup memory. Never grows, never frees.
pub struct FixedVec<T: 'static> {
    // Safety invariant: exclusive permanent typed allocation; only0..len contains
    // initialized owned T. Tail is not read/dropped as T. No external ownership
    // aliases the prefix; converting/moving the vector consumes this capability.
    slots: &'static mut [MaybeUninit<T>],
    // Safety invariant: len<=slots.len(); prefix0..len is initialized valid T.
    len: usize,
}

impl<T: 'static> fmt::Debug for FixedVec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FixedVec")
            .field("len", &self.len)
            .field("capacity", &self.slots.len())
            .finish()
    }
}

impl<T: 'static> FixedVec<T> {
    /// Appends a value.
    ///
    /// # Errors
    /// Returns [`StartupError::TableFull`] at capacity, never panics and never grows.
    /// # Safety-usable invariant
    /// Success moves value into the old len slot before increasing len by1;
    /// failure leaves the existing initialized prefix/length unchanged. All
    /// prior initialized elements keep their ownership, and no T is duplicated.
    pub fn push(&mut self, value: T) -> Result<(), StartupError> {
        let slot = self
            .slots
            .get_mut(self.len)
            .ok_or(StartupError::TableFull)?;
        slot.write(value);
        self.len = self.len.wrapping_add(1);
        Ok(())
    }

    /// Number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the vector is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The initialized elements.
    #[must_use]
    /// # Safety-usable invariant
    /// Borrows exactly the initialized prefix, bounded by self's borrow. Does not
    /// initialize, move, duplicate ownership or expose the MaybeUninit tail.
    pub fn as_slice(&self) -> &[T] {
        // SAFETY: FixedVec's prefix invariant gives len initialized T and no
        // conflicting owner. MaybeUninit<T>'s element size/alignment plus
        // Reference array/slice layout prove identical stride and extent;
        // slots' typed allocation gives nonnull alignment/isize bounds, even
        // for ZST/len0. &self excludes mutation for the resulting borrow.
        unsafe { core::slice::from_raw_parts(self.slots.as_ptr().cast::<T>(), self.len) }
    }

    /// Consumes the vector into a `'static` slice for runtime handles.
    #[must_use]
    /// # Safety-usable invariant
    /// Consumes this vector's only mutable storage capability and exposes only
    /// its initialized prefix as a shared static slice. No destructor reclaims
    /// those elements; ownership cannot be recovered through this consumed vector.
    pub fn into_static_slice(self) -> &'static [T] {
        // SAFETY: FixedVec's prefix invariant gives initialized T and len bounds.
        // MaybeUninit element layout plus Reference array/slice layout preserves
        // extent/alignment/provenance. The backing is static and this consuming
        // method relinquishes the sole mutable capability; no Drop reclaims it.
        unsafe { core::slice::from_raw_parts(self.slots.as_ptr().cast::<T>(), self.len) }
    }
}

impl<T: 'static> IntoIterator for FixedVec<T> {
    type Item = T;
    type IntoIter = FixedVecIter<T>;
    fn into_iter(self) -> FixedVecIter<T> {
        FixedVecIter {
            slots: self.slots,
            next: 0,
            len: self.len,
        }
    }
}

/// Consuming iterator, so a list of setups can each enter runtime mode by value.
pub struct FixedVecIter<T: 'static> {
    // Safety invariant: only next..len remains initialized owned T; earlier
    // elements were moved out and cannot be read/dropped again by this iterator.
    slots: &'static mut [MaybeUninit<T>],
    // Safety invariant: next<=len<=slots.len(); advancing next consumes one item.
    next: usize,
    // Safety invariant: original initialized prefix bound, never increased.
    len: usize,
}

impl<T: 'static> fmt::Debug for FixedVecIter<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FixedVecIter")
            .field("remaining", &self.len.saturating_sub(self.next))
            .finish()
    }
}

impl<T: 'static> Iterator for FixedVecIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        if self.next >= self.len {
            return None;
        }
        let slot = self.slots.get(self.next)?;
        self.next = self.next.wrapping_add(1);
        // SAFETY: FixedVecIter's remaining-prefix invariant and the preceding
        // next<len/get check prove one initialized T. next advances before the
        // move-out without callbacks or wrap (old next<len<=usize::MAX), so the
        // old cell is no longer owned/read/dropped by this iterator. Typed storage
        // remains live/aligned, including ZST; forget can only leak unread items.
        Some(unsafe { slot.assume_init_read() })
    }
}
