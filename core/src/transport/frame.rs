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

//! Type-erased ownership of one received frame with no per-message heap.
//!
//! A binding's receive owner, which keeps the middleware sample and its ports
//! alive, is moved into a [`RetainedFrame`], and queues and leases move this
//! value in turn. Dropping it runs the owner's destructor, which returns the
//! sample. Storage is `UnsafeCell`: the owner may contain a `Cell`, and a frame
//! lives in a slot reached through `&` from several places, but only one owner
//! ever touches it. `&self` accessors of the SDK read the pointers copied to the
//! slot metadata at delivery and never reach back into the owner.

use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::mem::{self, MaybeUninit};
use core::ptr;

use super::binding::PeerRef;
use super::{Frame, LoanedPayload, PayloadLoanProvenance};

/// Bytes available inline for a binding's retained receive owner.
///
/// A binding whose owner does not fit is refused at compile time by
/// [`RetainedFrame::new`].
pub const RETAINED_FRAME_CAPACITY: usize = 240;
/// Maximum alignment of a binding's retained receive owner.
pub const RETAINED_FRAME_ALIGN: usize = 16;

#[repr(C, align(16))]
struct InlineFrame(
    // Safety invariant: storage may be uninitialized; when owned by RetainedFrame
    // its first size_of::<F>() bytes contain exactly one F matching that owner's
    // FrameOps. The enclosing repr(C,align16) aligns this first field to16.
    UnsafeCell<[MaybeUninit<u8>; RETAINED_FRAME_CAPACITY]>,
);

// SAFETY: `InlineFrame` is sent between threads only inside `RetainedFrame`;
// the only `Frame: Send` values it holds are moved with it, and a slot's owner
// is accessed through exactly one index-holder at a time.
unsafe impl Send for InlineFrame {}

/// A borrowed payload view, as raw parts carried through the operations table.
type RawPayload = Option<(*const [u8], PayloadLoanProvenance)>;

/// Monomorphized operations on the owner stored in one [`RetainedFrame`].
struct FrameOps {
    // Safety invariant: F-ops: all operations are for the same concrete
    // F:Frame (hence Send+'static). Shared operations require one live aligned F,
    // a compatible shared borrow, and no concurrent callback on a !Sync F.
    // Returned pointers retain the exact slice provenance/extent; their validity
    // lasts only while F stays immobile/alive and that shared borrow remains
    // compatible. drop_owner alone consumes the sole live F after all views end.
    metadata: unsafe fn(*const u8) -> *const [u8],
    // Safety invariant: same concrete F and F-ops shared/view contract as metadata.
    payload: unsafe fn(*const u8) -> RawPayload,
    // Safety invariant: same F; F-ops shared-call contract, by-value result only.
    peer: unsafe fn(*const u8) -> PeerRef,
    // Safety invariant: same F, consuming F-ops destructor with no remaining views.
    drop_owner: unsafe fn(*mut u8),
}

/// # Safety
/// The caller must provide a nonnull, aligned pointer to one initialized live
/// F in a single allocation, valid for a compatible shared borrow throughout
/// this call. No conflicting mutation/drop/move or concurrent !Sync F callback
/// may occur. Safe Frame code may unwind/reenter; this function transfers no
/// ownership and does not assume its semantic correctness.
///
/// The result preserves the pointer/metadata of a genuine shared slice returned
/// by F. It remains readable/immutable only for the continued compatible borrow
/// of F, while F and its retained storage stay alive at the same address. The
/// caller must establish that lifetime separately before dereferencing the raw
/// result; moving an inline F can invalidate it.
unsafe fn metadata_of<F: Frame>(frame: *const u8) -> *const [u8] {
    // SAFETY: metadata_of's caller provides an initialized aligned live F and
    // compatible shared access for this exact call. Reference creation does not
    // outlive that access. Frame's safe &[u8] return supplies initialized extent,
    // nonnull/alignment1/provenance; converting it to raw does not extend lifetime.
    ptr::from_ref(unsafe { &*frame.cast::<F>() }.encoded_metadata())
}

/// # Safety
/// The caller must provide a nonnull, aligned pointer to one initialized live
/// F in a single allocation, valid for a compatible shared borrow throughout
/// this call. No conflicting mutation/drop/move or concurrent !Sync F callback
/// may occur. Safe Frame code may unwind/reenter; this function transfers no
/// ownership and does not assume its semantic correctness.
///
/// The result preserves the pointer/metadata of a genuine shared slice returned
/// by F. It remains readable/immutable only for the continued compatible borrow
/// of F, while F and its retained storage stay alive at the same address. The
/// caller must establish that lifetime separately before dereferencing the raw
/// result; moving an inline F can invalidate it.
unsafe fn payload_of<F: Frame>(frame: *const u8) -> RawPayload {
    // SAFETY: payload_of's caller contract supplies live aligned initialized F
    // and shared access for this call. Frame's safe borrowed payload and bytes()
    // preserve their slice's extent/provenance; raw conversion extends no lifetime.
    let owner = unsafe { &*frame.cast::<F>() };
    owner
        .payload()
        .map(|payload| (ptr::from_ref(payload.bytes()), payload.provenance()))
}

/// # Safety
/// Caller must provide nonnull aligned storage containing a valid live F,
/// permitting shared access for this entire call and excluding concurrent
/// callback access to a !Sync F. No move/drop or conflicting borrow may occur.
/// Returns a by-value PeerRef from the safe Frame method, with no new pointer or
/// ownership guarantee. Unwind/reentrancy do not transfer the F ownership.
unsafe fn peer_of<F: Frame>(frame: *const u8) -> PeerRef {
    // SAFETY: peer_of's contract gives initialized aligned F and compatible
    // shared access until the by-value PeerRef returns; no reference escapes.
    unsafe { &*frame.cast::<F>() }.peer()
}

/// # Safety
/// Caller must transfer sole ownership of one live initialized F at nonnull,
/// aligned, read/write-valid storage. All borrows/views of F or its retained
/// bytes must have ended and no concurrent access may occur. During destruction
/// only the destructor's &mut F and its derived accesses are permitted. The caller
/// must never read/drop/use the old F afterwards, even if its destructor unwinds.
/// This consumes F in place without deallocating the inline storage.
unsafe fn drop_owner_of<F: Frame>(frame: *mut u8) {
    // SAFETY: drop_owner_of's contract supplies drop_in_place's live initialized
    // F, nonnull/alignment, read/write-valid extent and exclusive destructor
    // access. This consumes F once; callers may not access it on return or unwind.
    unsafe { ptr::drop_in_place(frame.cast::<F>()) };
}

struct OpsOf<F>(PhantomData<F>);
impl<F: Frame> OpsOf<F> {
    const OPS: &'static FrameOps = &FrameOps {
        metadata: metadata_of::<F>,
        payload: payload_of::<F>,
        peer: peer_of::<F>,
        drop_owner: drop_owner_of::<F>,
    };
}

/// One binding-owned received frame, retained with no heap and no transport type.
pub struct RetainedFrame {
    // Safety invariant: F-storage: one initialized F:Frame resides at this
    // aligned first field, size<=capacity/align<=16, with no independent owner.
    // Moving Self moves F and invalidates any pointer into its inline storage.
    // No external byte borrow may be used across such a move or this Drop.
    storage: InlineFrame,
    // Safety invariant: exact OpsOf<F> for F-storage's concrete F, satisfying
    // F-ops. It is immutable and remains paired with storage through every move.
    ops: &'static FrameOps,
    // The owner is Send by the `Frame` contract but need not be Sync, so this
    // value is not `Sync`. Slots hold it behind `UnsafeCell` under one index.
    _not_sync: PhantomData<core::cell::Cell<()>>,
}

impl RetainedFrame {
    /// Moves a binding's receive owner into inline storage.
    ///
    /// Refuses to compile when `F` does not fit [`RETAINED_FRAME_CAPACITY`] or
    /// [`RETAINED_FRAME_ALIGN`], so an unsupported owner is a build error rather
    /// than a runtime refusal. No heap is used.
    #[must_use]
    pub fn new<F: Frame>(frame: F) -> Self {
        const {
            assert!(
                mem::size_of::<F>() <= RETAINED_FRAME_CAPACITY,
                "binding receive owner exceeds RETAINED_FRAME_CAPACITY"
            );
            assert!(
                mem::align_of::<F>() <= RETAINED_FRAME_ALIGN,
                "binding receive owner exceeds RETAINED_FRAME_ALIGN"
            );
        }
        let storage = InlineFrame(UnsafeCell::new(
            [MaybeUninit::uninit(); RETAINED_FRAME_CAPACITY],
        ));
        // SAFETY: the storage is uninitialized, large enough and aligned for F,
        // both checked at compile time above, and is written exactly once.
        unsafe { ptr::write(storage.0.get().cast::<F>(), frame) };
        Self {
            storage,
            ops: OpsOf::<F>::OPS,
            _not_sync: PhantomData,
        }
    }

    /// # Safety-usable invariant
    /// Returns the live initialized F-storage address, nonnull and aligned for
    /// the F paired with ops. It is valid only for compatible borrows of this
    /// self at its current address; the raw pointer does not forbid moving self.
    fn owner(&self) -> *const u8 {
        self.storage.0.get().cast_const().cast::<u8>()
    }

    /// The frame's encoded metadata, as a raw pointer to bytes the owner retains.
    #[must_use]
    /// # Safety-usable invariant
    /// Returns raw parts of a genuine immutable slice borrowed from the live
    /// F-storage. Extent/provenance/non-nullness remain valid only while self
    /// stays immobile/alive with a compatible shared borrow. No static lifetime
    /// is granted. The caller must establish those facts at later dereference.
    pub(crate) fn metadata_ptr(&self) -> *const [u8] {
        // SAFETY: F-storage pairs owner()'s initialized aligned F with F-ops.
        // &self keeps it unmoved/alive for this call; RetainedFrame is !Sync, so
        // safe clients cannot run these !Sync owner callbacks concurrently.
        unsafe { (self.ops.metadata)(self.owner()) }
    }

    /// The payload as a raw pointer to bytes the owner retains, and its provenance.
    #[must_use]
    /// # Safety-usable invariant
    /// Some contains the genuine payload slice's pointer/metadata and by-value
    /// provenance. The same immobile, live, compatible-shared-borrow requirements
    /// as metadata_ptr apply. Absence/presence is semantic, not a safety premise.
    pub(crate) fn payload_parts(&self) -> RawPayload {
        // SAFETY: F-storage pairs owner()'s initialized aligned F with F-ops.
        // &self keeps it unmoved/alive for this call; RetainedFrame is !Sync, so
        // safe clients cannot run these !Sync owner callbacks concurrently.
        unsafe { (self.ops.payload)(self.owner()) }
    }

    /// Who sent the frame.
    #[must_use]
    pub(crate) fn peer(&self) -> PeerRef {
        // SAFETY: F-storage pairs owner()'s initialized aligned F with F-ops.
        // &self keeps it unmoved/alive for this call; RetainedFrame is !Sync, so
        // safe clients cannot run these !Sync owner callbacks concurrently.
        unsafe { (self.ops.peer)(self.owner()) }
    }
}

impl Drop for RetainedFrame {
    fn drop(&mut self) {
        // SAFETY: F-storage and F-ops pair the one live F with its destructor.
        // &mut self at Drop excludes all safe views; raw users must already have
        // ended theirs. Inline storage is nonnull/aligned/read-write-valid. This
        // is the sole consuming call, and no old F access follows, even on unwind.
        unsafe { (self.ops.drop_owner)(self.storage.0.get().cast::<u8>()) };
    }
}

impl Frame for RetainedFrame {
    fn encoded_metadata(&self) -> &[u8] {
        // SAFETY: F-storage/owner() supply the F-ops shared-call contract. Its
        // returned raw slice has initialized single-allocation extent and original
        // nonnull pointer. The returned lifetime is tied to &self, which prevents
        // moving/dropping F; no mutable access is exposed during that borrow.
        unsafe { &*(self.ops.metadata)(self.owner()) }
    }

    fn payload(&self) -> Option<LoanedPayload<'_>> {
        // SAFETY: F-storage/owner() give the matching F-ops shared-call contract;
        // &self holds F at this address and !Sync prevents concurrent callbacks.
        let (bytes, provenance) = unsafe { (self.ops.payload)(self.owner()) }?;
        // SAFETY: F-ops payload postcondition gives a real initialized immutable
        // slice with original provenance/extent/non-nullness. &self keeps F and
        // its retained bytes alive/immobile for exactly this returned borrow.
        Some(LoanedPayload::new(unsafe { &*bytes }, provenance))
    }

    fn peer(&self) -> PeerRef {
        // SAFETY: F-storage and owner() supply the matching F-ops shared-call
        // requirements; this by-value result retains no borrow of a moved owner.
        unsafe { (self.ops.peer)(self.owner()) }
    }
}

impl core::fmt::Debug for RetainedFrame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RetainedFrame").finish_non_exhaustive()
    }
}
