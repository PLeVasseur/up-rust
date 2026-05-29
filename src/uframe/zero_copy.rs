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

use std::{
    io::{Cursor, Read},
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
};

use super::{
    frame::{UFrameMetadata, UOwnedFrame},
    payload::{
        BorrowPayload, DecodePayload, PayloadCodec, PayloadFormat, PayloadLayout,
        ReadDecodePayload, StableContainerPayload, StablePayload, UDeserializer, UReadDeserializer,
        UWireError,
    },
};

impl UFrameView for UOwnedFrame {
    type PayloadReader<'a>
        = Cursor<&'a [u8]>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = std::iter::Once<&'a [u8]>
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata {
        self.metadata()
    }

    fn payload_len(&self) -> usize {
        self.payload_bytes().len()
    }

    fn has_payload(&self) -> bool {
        UOwnedFrame::has_payload(self)
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        Cursor::new(self.payload_bytes())
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        std::iter::once(self.payload_bytes())
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        Some(self.payload_bytes())
    }
}

/// Mutable transmit storage reserved from a zero-copy transport.
///
/// A transmit buffer is owned by the transport until it is committed with
/// [`UZeroCopyTransport::send_zero_copy`](crate::zero_copy::UZeroCopyTransport::send_zero_copy).
/// Frame metadata is fixed when the loan is reserved. Transports may use that
/// metadata to choose routes, encode native headers, compute payload offsets, or
/// allocate backing storage before handing the loan to the caller.
/// Serializers should write directly into [`Self::payload_mut`] so the payload
/// does not first have to be materialized as an owned [`Vec<u8>`] or
/// [`bytes::Bytes`].
pub trait UTxBuffer {
    /// Returns the immutable frame metadata associated with this transmit loan.
    fn metadata(&self) -> &UFrameMetadata;

    /// Returns the current payload bytes in the transmit loan.
    ///
    /// This view is borrowed from the loan and is valid only while `self` is
    /// borrowed.
    fn payload(&self) -> &[u8];

    /// Returns mutable payload storage for direct serialization into the loan.
    ///
    /// Implementations must expose exactly the payload range requested from the
    /// transport, excluding any transport metadata prefix, padding, or trailer.
    fn payload_mut(&mut self) -> &mut [u8];

    /// Returns diagnostic provenance for this transmit loan.
    fn payload_loan_provenance(&self) -> PayloadLoanProvenance {
        PayloadLoanProvenance::OpaqueTransportLoan
    }

    /// Returns mutable payload bytes with explicit loan provenance.
    fn loaned_payload_mut(&mut self) -> LoanedPayloadMut<'_> {
        let provenance = self.payload_loan_provenance();
        // SAFETY:
        // - `UTxBuffer::payload_mut` is the transport contract for the exact
        //   visible initialized application payload range, excluding metadata,
        //   padding, and trailers.
        // - `&mut self` gives exclusive access to that payload range while the
        //   returned loaned view exists.
        unsafe { LoanedPayloadMut::new_unchecked(self.payload_mut(), provenance) }
    }
}

/// Mutable transmit storage whose application payload bytes are not yet initialized.
///
/// This is a type-state sibling of [`UTxBuffer`]. It must not expose payload
/// bytes as `&[u8]` or `&mut [u8]`; callers initialize the payload through
/// [`LoanedPayloadUninitMut`] and only then convert the loan into its initialized
/// [`UTxBuffer`] form.
pub trait UUninitTxBuffer {
    /// Initialized transmit loan type produced after payload initialization.
    type Initialized: UTxBuffer;

    /// Returns the immutable frame metadata associated with this transmit loan.
    fn metadata(&self) -> &UFrameMetadata;

    /// Returns the visible application payload length.
    fn payload_len(&self) -> usize;

    /// Returns diagnostic provenance for this transmit loan.
    fn payload_loan_provenance(&self) -> PayloadLoanProvenance {
        PayloadLoanProvenance::OpaqueTransportLoan
    }

    /// Returns mutable uninitialized application payload storage.
    fn payload_uninit_mut(&mut self) -> LoanedPayloadUninitMut<'_>;

    /// Converts this uninitialized loan into its initialized TX buffer form.
    ///
    /// # Safety
    ///
    /// The caller must guarantee every visible application payload byte has been
    /// initialized, and that any transport-owned bytes required for send were
    /// initialized by the transport before conversion. Implementations that
    /// reinterpret or commit storage must preserve the original allocation,
    /// length, capacity, alignment, and ownership requirements of the relevant
    /// standard-library operation, such as `Vec::from_raw_parts` or an external
    /// SHM commit API.
    unsafe fn assume_payload_init(self) -> Self::Initialized;
}

/// Neutral view of frame metadata plus ordered payload bytes.
///
/// This trait is shared by owned frames and transport receive leases. It does
/// not by itself imply that payload bytes are backed by transport-loaned storage.
/// The base capability is intentionally segmented-friendly: callers should
/// prefer [`Self::payload_reader`] or [`Self::payload_slices`] unless they
/// explicitly need a contiguous borrowed view.
pub trait UFrameView {
    type PayloadReader<'a>: Read + 'a
    where
        Self: 'a;
    type PayloadSlices<'a>: Iterator<Item = &'a [u8]> + 'a
    where
        Self: 'a;

    /// Returns the native frame metadata.
    fn metadata(&self) -> &UFrameMetadata;

    /// Returns the number of application payload bytes visible through this view.
    ///
    /// Transport metadata prefixes, alignment padding, and protocol trailers
    /// must not be included in this length.
    fn payload_len(&self) -> usize;

    /// Returns whether this view carries a payload, including a present empty
    /// payload.
    ///
    /// The default preserves legacy transport implementations by treating any
    /// non-empty payload byte range or any payload encoding metadata as payload
    /// presence. Frame views that can distinguish an absent payload from a
    /// present empty payload without relying on encoding metadata should override
    /// this method.
    fn has_payload(&self) -> bool {
        self.payload_len() > 0 || self.metadata().encoding().is_some()
    }

    /// Returns an ordered reader over the application payload bytes.
    ///
    /// This is the preferred generic decode path because it works for both
    /// contiguous and segmented transport storage without forcing a coalescing
    /// copy.
    fn payload_reader(&self) -> Self::PayloadReader<'_>;

    /// Returns ordered borrowed payload slices.
    ///
    /// The iterator must yield the same byte sequence that was sent. It may
    /// yield more than one slice when the underlying transport stores payloads in
    /// segmented buffers.
    fn payload_slices(&self) -> Self::PayloadSlices<'_>;

    /// Returns a contiguous borrowed payload view when this view can provide
    /// one without copying.
    ///
    /// A return value of `None` means callers should use [`Self::payload_reader`]
    /// or [`Self::payload_slices`]. Implementations must not allocate or coalesce
    /// segmented storage to satisfy this method.
    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        None
    }

    /// Deserializes this frame view from its ordered payload reader.
    ///
    /// The method verifies the frame's [`PayloadEncoding`](crate::PayloadEncoding) against
    /// the selected [`PayloadFormat`] before invoking the reader-based deserializer.
    /// Use [`UContiguousZeroCopyRxFrame::deserialize_borrowed`] when the decoded
    /// value needs to borrow directly from a guaranteed contiguous payload.
    fn deserialize_from_reader<F, T>(&self) -> Result<T, UWireError>
    where
        F: PayloadFormat,
        T: UReadDeserializer<F>,
    {
        let expected = F::encoding();
        let actual = self
            .metadata()
            .encoding()
            .ok_or(UWireError::MissingEncoding)?;
        if !actual.is_compatible_with(&expected) {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(actual.clone()),
            });
        }
        T::deserialize_from_reader(self.payload_reader(), self.payload_len())
    }

    /// Decodes this frame view from its ordered payload reader with codec `C`.
    ///
    /// This path avoids coalescing segmented receive storage when the selected
    /// codec can decode from a stream.
    fn decode_payload_from_reader_as<C, T>(&self) -> Result<T, UWireError>
    where
        C: PayloadCodec + ReadDecodePayload<T>,
    {
        C::verify_encoding(self.metadata().encoding())?;
        C::decode_payload_from_reader(self.payload_reader(), self.payload_len())
    }
}

/// Receive-side zero-copy frame lease returned by a transport.
///
/// Dropping a receive lease releases the underlying transport storage. Borrowed
/// payload views returned through [`UFrameView`] must not outlive the lease.
/// Implementations must not allocate or coalesce payload storage to satisfy view
/// accessors. Owned frames intentionally implement only [`UFrameView`], not this
/// lease trait.
pub trait UZeroCopyRxLease: UFrameView {}

/// Receive-side frame lease with a guaranteed contiguous payload view.
///
/// Implement this trait only for receive leases whose application payload is
/// always available as one borrowed byte slice without copying. Segmented
/// transports should implement only [`UZeroCopyRxLease`] and rely on reader or
/// slice iteration based decoding.
pub trait UContiguousZeroCopyRxFrame: UZeroCopyRxLease {
    /// Returns the application payload as one contiguous borrowed byte slice.
    ///
    /// The returned slice must exclude transport metadata, padding, and trailers
    /// and must remain valid only for the lifetime of the receive lease.
    fn contiguous_payload(&self) -> &[u8];

    /// Deserializes a value that may borrow directly from the contiguous payload.
    ///
    /// The method verifies the frame's [`PayloadEncoding`](crate::PayloadEncoding) against
    /// the selected [`PayloadFormat`] before invoking the borrowed deserializer.
    fn deserialize_borrowed<'a, F, T>(&'a self) -> Result<T, UWireError>
    where
        F: PayloadFormat,
        T: UDeserializer<'a, F>,
    {
        let expected = F::encoding();
        let actual = self
            .metadata()
            .encoding()
            .ok_or(UWireError::MissingEncoding)?;
        if !actual.is_compatible_with(&expected) {
            return Err(UWireError::UnsupportedEncoding {
                expected: Box::new(expected),
                actual: Box::new(actual.clone()),
            });
        }
        T::deserialize_from(self.contiguous_payload())
    }

    /// Decodes a value from the contiguous payload with codec `C`.
    fn decode_payload_as<'a, C, T>(&'a self) -> Result<T, UWireError>
    where
        C: PayloadCodec + DecodePayload<'a, T>,
    {
        C::verify_encoding(self.metadata().encoding())?;
        C::decode_payload(self.contiguous_payload())
    }

    /// Borrows a typed view directly from the contiguous payload with codec `C`.
    fn borrow_payload_as<C, T>(&self) -> Result<&T, UWireError>
    where
        C: PayloadCodec + BorrowPayload<T>,
        T: ?Sized,
    {
        C::verify_encoding(self.metadata().encoding())?;
        C::borrow_payload(self.contiguous_payload())
    }
}

/// Diagnostic provenance for loan-backed payload storage.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PayloadLoanProvenance {
    /// Payload bytes are backed by a transport loan whose domain is opaque.
    OpaqueTransportLoan,
    /// Payload bytes are backed by a proven shared-memory region.
    SharedMemory,
}

/// Immutable payload bytes with explicit transport-loan provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoanedPayload<'a> {
    bytes: &'a [u8],
    provenance: PayloadLoanProvenance,
}

impl<'a> LoanedPayload<'a> {
    /// Creates a loaned payload view from transport-owned storage.
    ///
    /// This constructor is intended for transport implementations that can
    /// prove payload provenance. Application code should obtain loaned payloads
    /// from receive leases.
    ///
    /// # Safety
    ///
    /// Callers must guarantee `bytes` is a valid shared slice for `'a`: it must
    /// stay within one allocation, be valid for reads, and not be mutated through
    /// an alias for the returned lifetime. The storage must be described by
    /// `kind` and must not have been allocated or coalesced solely to satisfy a
    /// zero-copy borrow. These obligations are the safety preconditions of
    /// `slice::from_raw_parts` plus the transport's external provenance contract.
    pub unsafe fn new_unchecked(bytes: &'a [u8], provenance: PayloadLoanProvenance) -> Self {
        Self { bytes, provenance }
    }

    /// Returns diagnostic storage provenance.
    pub fn provenance(self) -> PayloadLoanProvenance {
        self.provenance
    }

    /// Returns the payload bytes.
    pub fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Returns the payload length in bytes.
    pub fn len(self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the payload is empty.
    pub fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }
}

impl AsRef<[u8]> for LoanedPayload<'_> {
    fn as_ref(&self) -> &[u8] {
        self.bytes
    }
}

impl Deref for LoanedPayload<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.bytes
    }
}

/// Mutable payload bytes with explicit transport-loan provenance.
#[derive(Debug, Eq, PartialEq)]
pub struct LoanedPayloadMut<'a> {
    bytes: &'a mut [u8],
    provenance: PayloadLoanProvenance,
}

impl<'a> LoanedPayloadMut<'a> {
    /// Creates a mutable loaned payload view from transport-owned storage.
    ///
    /// This constructor is intended for transport implementations that can
    /// prove the exposed slice is the exact visible application payload range.
    /// Application code should use transmit loan APIs instead.
    ///
    /// # Safety
    ///
    /// Callers must guarantee `bytes` is a valid mutable slice for `'a`: it must
    /// stay within one allocation, be valid for reads and writes, and have no
    /// other active access path for the returned lifetime. The storage must be
    /// described by `kind` and be the exact visible application payload range for
    /// the loan. These obligations are the safety preconditions of
    /// `slice::from_raw_parts_mut` plus the transport's external provenance
    /// contract.
    pub unsafe fn new_unchecked(bytes: &'a mut [u8], provenance: PayloadLoanProvenance) -> Self {
        Self { bytes, provenance }
    }

    /// Returns diagnostic storage provenance.
    pub fn provenance(&self) -> PayloadLoanProvenance {
        self.provenance
    }

    /// Returns the payload bytes.
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes
    }

    /// Returns mutable payload bytes.
    pub fn as_mut_bytes(&mut self) -> &mut [u8] {
        self.bytes
    }

    /// Returns the payload length in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the payload is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

impl AsRef<[u8]> for LoanedPayloadMut<'_> {
    fn as_ref(&self) -> &[u8] {
        self.bytes
    }
}

impl AsMut<[u8]> for LoanedPayloadMut<'_> {
    fn as_mut(&mut self) -> &mut [u8] {
        self.bytes
    }
}

impl Deref for LoanedPayloadMut<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.bytes
    }
}

impl DerefMut for LoanedPayloadMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.bytes
    }
}

/// Mutable uninitialized payload bytes with explicit transport-loan provenance.
#[derive(Debug)]
pub struct LoanedPayloadUninitMut<'a> {
    bytes: &'a mut [MaybeUninit<u8>],
    provenance: PayloadLoanProvenance,
}

impl<'a> LoanedPayloadUninitMut<'a> {
    /// Creates a mutable uninitialized loaned payload view from transport-owned storage.
    ///
    /// This constructor is intended for transport implementations that can
    /// prove the exposed slice is the exact visible application payload range.
    /// Application code should use uninitialized transmit helpers instead.
    ///
    /// # Safety
    ///
    /// Callers must guarantee `bytes` is a valid mutable uninitialized byte slice
    /// for `'a`: it must stay within one allocation, be valid for writes, and
    /// have no other active access path for the returned lifetime. The storage
    /// must be described by `kind` and be the exact visible application payload
    /// range for the loan. `MaybeUninit<u8>` has the same layout as `u8`; the
    /// initialization obligation is tracked by the returned loan marker.
    pub unsafe fn new_unchecked(
        bytes: &'a mut [MaybeUninit<u8>],
        provenance: PayloadLoanProvenance,
    ) -> Self {
        Self { bytes, provenance }
    }

    /// Returns diagnostic storage provenance.
    pub fn provenance(&self) -> PayloadLoanProvenance {
        self.provenance
    }

    pub(crate) fn as_uninit_bytes_mut_internal(&mut self) -> &mut [MaybeUninit<u8>] {
        self.bytes
    }

    /// Returns mutable uninitialized payload bytes.
    ///
    /// # Safety
    ///
    /// Callers must initialize every byte they later expose as initialized and
    /// must not let raw writes get out of sync with any higher-level initialization
    /// proof. Prefer [`Self::into_writer`] for safe direct byte generation.
    #[cfg(any(
        feature = "unsafe-uninit-payload-bytes",
        feature = "expert-unsafe-payloads"
    ))]
    pub unsafe fn as_uninit_bytes_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        self.as_uninit_bytes_mut_internal()
    }

    /// Returns the payload length in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the payload is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Creates a safe cursor that initializes bytes from the start of the payload.
    pub fn into_writer(self) -> LoanedUninitByteWriter<'a> {
        LoanedUninitByteWriter {
            payload: self,
            written: 0,
        }
    }

    /// Converts this payload view into initialized mutable bytes.
    ///
    /// # Safety
    ///
    /// The caller must guarantee every byte in this payload view is initialized.
    pub unsafe fn assume_init(self) -> LoanedPayloadMut<'a> {
        let len = self.bytes.len();
        let ptr = self.bytes.as_mut_ptr().cast::<u8>();
        // SAFETY:
        // - The caller guarantees every `MaybeUninit<u8>` element is initialized.
        // - Per stable `MaybeUninit<T>` layout docs, `MaybeUninit<u8>` has the
        //   same size, alignment, and ABI as `u8`.
        // - Per stable `slice::from_raw_parts_mut` docs, the pointer must be
        //   non-null, aligned, valid for `len` initialized `u8` elements, and
        //   contained in one allocation; those properties come from the original
        //   mutable slice plus the caller's initialization guarantee.
        let bytes = unsafe { std::slice::from_raw_parts_mut(ptr, len) };
        // SAFETY: The initialized slice is the same exact loan-backed payload
        // range and retains the original `PayloadLoanProvenance` diagnostic value.
        unsafe { LoanedPayloadMut::new_unchecked(bytes, self.provenance) }
    }
}

/// Safe cursor for initializing an uninitialized payload byte range exactly once.
#[derive(Debug)]
pub struct LoanedUninitByteWriter<'a> {
    payload: LoanedPayloadUninitMut<'a>,
    written: usize,
}

impl<'a> LoanedUninitByteWriter<'a> {
    /// Returns the total payload length.
    pub fn len(&self) -> usize {
        self.payload.len()
    }

    /// Returns whether the payload is empty.
    pub fn is_empty(&self) -> bool {
        self.payload.is_empty()
    }

    /// Returns the number of initialized bytes written so far.
    pub fn written(&self) -> usize {
        self.written
    }

    /// Returns the number of bytes that still must be written before finish.
    pub fn remaining(&self) -> usize {
        self.len().saturating_sub(self.written)
    }

    /// Appends bytes to the initialized prefix of the payload.
    pub fn write_all(&mut self, src: &[u8]) -> Result<(), UWireError> {
        let end = self
            .written
            .checked_add(src.len())
            .ok_or_else(|| UWireError::invalid_payload("payload writer length overflow"))?;
        let total_len = self.len();
        let dst = self
            .payload
            .bytes
            .get_mut(self.written..end)
            .ok_or_else(|| UWireError::buffer_too_small(end, total_len))?;
        for (byte, slot) in src.iter().copied().zip(dst.iter_mut()) {
            slot.write(byte);
        }
        self.written = end;
        Ok(())
    }

    /// Finishes initialization and returns initialized mutable payload bytes.
    pub fn finish(self) -> Result<LoanedPayloadMut<'a>, UWireError> {
        let len = self.len();
        if self.written != len {
            return Err(UWireError::invalid_payload_length(len, self.written));
        }
        // SAFETY: `written == len` proves that every byte in the payload range
        // was initialized through `write_all` before conversion.
        Ok(unsafe { self.payload.assume_init() })
    }

    /// Returns raw mutable uninitialized bytes for custom initialization.
    ///
    /// # Safety
    ///
    /// Callers must ensure the writer's `written` cursor remains consistent with
    /// the initialized prefix before calling [`Self::finish`]. Prefer
    /// [`Self::write_all`] for safe byte generation.
    #[cfg(any(
        feature = "unsafe-uninit-payload-bytes",
        feature = "expert-unsafe-payloads"
    ))]
    pub unsafe fn as_uninit_bytes_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        self.payload.as_uninit_bytes_mut_internal()
    }
}

/// Receive lease that can expose a contiguous payload from loan-backed storage.
///
/// This is stricter than [`UContiguousZeroCopyRxFrame`]. A contiguous payload may
/// simply be owned memory, while this trait represents the payload-level
/// zero-copy receive contract. Implementations must not allocate or coalesce to
/// satisfy [`Self::try_loaned_contiguous_payload`].
pub trait ULoanedContiguousZeroCopyRxFrame: UZeroCopyRxLease {
    /// Returns one contiguous loan-backed application payload view.
    ///
    /// Implementations return an error when the current frame is not backed by a
    /// suitable loan, is segmented, or otherwise cannot provide the slice without
    /// copying.
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError>;

    /// Returns diagnostic provenance for successful loaned payload borrows.
    fn payload_loan_provenance(&self) -> Result<PayloadLoanProvenance, UWireError> {
        Ok(self.loaned_contiguous_payload()?.provenance())
    }

    /// Compatibility helper returning only loan-backed payload bytes.
    fn try_loaned_contiguous_payload(&self) -> Result<&[u8], UWireError> {
        Ok(self.loaned_contiguous_payload()?.as_bytes())
    }

    /// Borrows one stable-container value from loan-backed contiguous storage.
    ///
    /// This is the safe stable-container typed receive boundary. It validates the
    /// stable-container encoding, exact payload size, local alignment, and loaned
    /// lifetime before constructing `&T`. The diagnostic provenance value is not
    /// used as the safety gate.
    fn borrow_stable_payload<T>(&self) -> Result<&T, UWireError>
    where
        T: StablePayload,
    {
        StableContainerPayload::<T>::verify_encoding(self.metadata().encoding())?;
        let payload = self.loaned_contiguous_payload()?;
        StableContainerPayload::<T>::borrow_checked_payload(payload.as_bytes())
    }
}

/// Verifies the visible transmit payload layout exposed by a zero-copy loan.
pub fn verify_tx_buffer_payload_layout(
    buffer: &mut impl UTxBuffer,
    payload_len: usize,
    alignment: usize,
) -> Result<(), UWireError> {
    let layout = PayloadLayout::new(payload_len, alignment)?;
    let payload = buffer.payload_mut();
    if payload.len() != layout.len() {
        return Err(UWireError::invalid_payload_length(
            layout.len(),
            payload.len(),
        ));
    }
    if !payload.is_empty() && !(payload.as_ptr() as usize).is_multiple_of(layout.align()) {
        return Err(UWireError::invalid_payload_alignment(
            layout.align(),
            payload.as_ptr() as usize,
        ));
    }
    Ok(())
}

/// Verifies the visible uninitialized transmit payload layout exposed by a loan.
pub fn verify_uninit_tx_buffer_payload_layout(
    buffer: &mut impl UUninitTxBuffer,
    payload_len: usize,
    alignment: usize,
) -> Result<(), UWireError> {
    let layout = PayloadLayout::new(payload_len, alignment)?;
    let mut payload = buffer.payload_uninit_mut();
    let payload = payload.as_uninit_bytes_mut_internal();
    if payload.len() != layout.len() {
        return Err(UWireError::invalid_payload_length(
            layout.len(),
            payload.len(),
        ));
    }
    if !payload.is_empty() && !(payload.as_ptr() as usize).is_multiple_of(layout.align()) {
        return Err(UWireError::invalid_payload_alignment(
            layout.align(),
            payload.as_ptr() as usize,
        ));
    }
    Ok(())
}

/// Verifies the visible contiguous receive payload layout.
pub fn verify_contiguous_rx_payload_layout(
    frame: &(impl UContiguousZeroCopyRxFrame + ?Sized),
    payload_len: usize,
    alignment: usize,
) -> Result<(), UWireError> {
    let layout = PayloadLayout::new(payload_len, alignment)?;
    let payload = frame.contiguous_payload();
    if payload.len() != layout.len() || frame.payload_len() != layout.len() {
        return Err(UWireError::invalid_payload_length(
            layout.len(),
            payload.len(),
        ));
    }
    if !payload.is_empty() && !(payload.as_ptr() as usize).is_multiple_of(layout.align()) {
        return Err(UWireError::invalid_payload_alignment(
            layout.align(),
            payload.as_ptr() as usize,
        ));
    }
    Ok(())
}

/// Verifies the loan-backed contiguous receive payload layout.
pub fn verify_loaned_rx_payload_layout(
    frame: &(impl ULoanedContiguousZeroCopyRxFrame + ?Sized),
    payload_len: usize,
    alignment: usize,
) -> Result<(), UWireError> {
    let layout = PayloadLayout::new(payload_len, alignment)?;
    let payload = frame.try_loaned_contiguous_payload()?;
    if payload.len() != layout.len() || frame.payload_len() != layout.len() {
        return Err(UWireError::invalid_payload_length(
            layout.len(),
            payload.len(),
        ));
    }
    if !payload.is_empty() && !(payload.as_ptr() as usize).is_multiple_of(layout.align()) {
        return Err(UWireError::invalid_payload_alignment(
            layout.align(),
            payload.as_ptr() as usize,
        ));
    }
    Ok(())
}

/// Explicit helpers for crossing from zero-copy receive leases into owned bytes.
///
/// These helpers intentionally copy. They are useful at adapter boundaries such
/// as routers that operate on [`UOwnedFrame`], but should not be used on a path
/// that claims to preserve end-to-end zero-copy delivery.
pub trait UZeroCopyPayloadCopyExt: UFrameView {
    /// Copies the ordered payload bytes into `dst`.
    ///
    /// Returns the number of bytes copied, or an error if `dst` is too small or
    /// if the slice iterator does not produce exactly [`UFrameView::payload_len`]
    /// bytes.
    fn copy_payload_to(&self, dst: &mut [u8]) -> Result<usize, UWireError> {
        let expected = self.payload_len();
        if dst.len() < expected {
            return Err(UWireError::buffer_too_small(expected, dst.len()));
        }

        let mut written = 0_usize;
        let mut copy_result = Ok(());
        for slice in self.payload_slices() {
            if copy_result.is_err() {
                break;
            }
            let Some(end) = written.checked_add(slice.len()) else {
                copy_result = Err(UWireError::invalid_payload("payload length overflow"));
                break;
            };
            let Some(target) = dst.get_mut(written..end) else {
                copy_result = Err(UWireError::buffer_too_small(expected, dst.len()));
                break;
            };
            target.copy_from_slice(slice);
            written = end;
        }
        copy_result?;
        if written != expected {
            return Err(UWireError::invalid_payload(format!(
                "payload slices yielded {written} bytes but payload_len returned {expected} bytes"
            )));
        }
        Ok(written)
    }

    /// Copies the ordered payload bytes into a newly allocated [`Vec<u8>`].
    fn try_payload_to_vec(&self) -> Result<Vec<u8>, UWireError> {
        let expected = self.payload_len();
        let mut payload = Vec::new();
        payload.try_reserve_exact(expected).map_err(|error| {
            UWireError::invalid_payload(format!(
                "failed to allocate {expected} payload bytes: {error}"
            ))
        })?;

        let mut written = 0_usize;
        for slice in self.payload_slices() {
            let end = written
                .checked_add(slice.len())
                .ok_or_else(|| UWireError::invalid_payload("payload length overflow"))?;
            if end > expected {
                return Err(UWireError::buffer_too_small(expected, end));
            }
            payload.extend_from_slice(slice);
            written = end;
        }
        if written != expected {
            return Err(UWireError::invalid_payload(format!(
                "payload slices yielded {written} bytes but payload_len returned {expected} bytes"
            )));
        }
        Ok(payload)
    }
}

impl<T> UZeroCopyPayloadCopyExt for T where T: UFrameView + ?Sized {}

/// Owned buffer useful for tests, examples, and adapters that emulate a transmit loan.
///
/// `UVecTxBuffer` implements [`UTxBuffer`] over an owned `Vec<u8>`. It is
/// intended for test fakes and examples; production shared-memory transports
/// should expose their own transport-specific loan type. Receive-side fakes that
/// are used as zero-copy transports should implement [`UZeroCopyRxLease`] rather
/// than using [`UOwnedFrame`] as a lease.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UVecTxBuffer {
    metadata: UFrameMetadata,
    storage: Vec<u8>,
    payload_offset: usize,
    payload_len: usize,
}

/// Owned uninitialized buffer useful for tests, examples, and uninit adapters.
#[derive(Clone, Debug)]
pub struct UVecUninitTxBuffer {
    metadata: UFrameMetadata,
    storage: Vec<MaybeUninit<u8>>,
    payload_offset: usize,
    payload_len: usize,
}

/// In-memory receive lease for tests and examples that need receive-lease shape.
///
/// This type owns its bytes but models the lifetime boundary of a receive lease:
/// borrowed payload views cannot outlive the value. Production transports should
/// expose transport-specific receive lease types backed by their native storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UVecRxLease {
    frame: UOwnedFrame,
}

impl UVecRxLease {
    /// Creates an in-memory receive lease from an owned frame.
    pub fn new(frame: UOwnedFrame) -> Self {
        Self { frame }
    }

    /// Consumes the lease and returns the owned frame used by this test lease.
    pub fn into_frame(self) -> UOwnedFrame {
        self.frame
    }
}

impl UFrameView for UVecRxLease {
    type PayloadReader<'a>
        = Cursor<&'a [u8]>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = std::iter::Once<&'a [u8]>
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata {
        self.frame.metadata()
    }

    fn payload_len(&self) -> usize {
        self.frame.payload_bytes().len()
    }

    fn has_payload(&self) -> bool {
        self.frame.has_payload()
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        Cursor::new(self.frame.payload_bytes())
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        std::iter::once(self.frame.payload_bytes())
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        Some(self.frame.payload_bytes())
    }
}

impl UZeroCopyRxLease for UVecRxLease {}

impl UContiguousZeroCopyRxFrame for UVecRxLease {
    fn contiguous_payload(&self) -> &[u8] {
        self.frame.payload_bytes()
    }
}

impl UVecUninitTxBuffer {
    /// Creates an owned uninitialized transmit buffer.
    pub fn new(metadata: UFrameMetadata, payload_len: usize) -> Self {
        Self {
            metadata,
            storage: vec![MaybeUninit::uninit(); payload_len],
            payload_offset: 0,
            payload_len,
        }
    }

    /// Creates an owned uninitialized transmit buffer whose visible payload starts at `alignment`.
    pub fn with_alignment(
        metadata: UFrameMetadata,
        payload_len: usize,
        alignment: usize,
    ) -> Result<Self, UWireError> {
        let layout = PayloadLayout::new(payload_len, alignment)?;
        if layout.is_empty() {
            return Ok(Self::new(metadata, payload_len));
        }
        let extra = layout.align().saturating_sub(1);
        let storage_len = layout.len().checked_add(extra).ok_or_else(|| {
            UWireError::invalid_payload("payload length plus alignment padding overflows usize")
        })?;
        let storage = vec![MaybeUninit::uninit(); storage_len];
        let address = storage.as_ptr() as usize;
        let payload_offset = if layout.align() == 1 {
            0
        } else {
            (layout.align() - (address % layout.align())) % layout.align()
        };
        Ok(Self {
            metadata,
            storage,
            payload_offset,
            payload_len: layout.len(),
        })
    }

    fn payload_range(&self) -> std::ops::Range<usize> {
        let end = self
            .payload_offset
            .checked_add(self.payload_len)
            .expect("UVecUninitTxBuffer payload range overflow");
        self.payload_offset..end
    }
}

impl UUninitTxBuffer for UVecUninitTxBuffer {
    type Initialized = UVecTxBuffer;

    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload_len(&self) -> usize {
        self.payload_len
    }

    fn payload_uninit_mut(&mut self) -> LoanedPayloadUninitMut<'_> {
        let provenance = self.payload_loan_provenance();
        let range = self.payload_range();
        let payload = self
            .storage
            .get_mut(range)
            .expect("UVecUninitTxBuffer payload range must be in bounds");
        // SAFETY:
        // - `payload_range` is bounds-checked against `storage` above.
        // - The returned range is the exact visible application payload range.
        // - `&mut self` gives exclusive access to the storage for the loan.
        unsafe { LoanedPayloadUninitMut::new_unchecked(payload, provenance) }
    }

    unsafe fn assume_payload_init(self) -> Self::Initialized {
        let Self {
            metadata,
            mut storage,
            payload_offset,
            payload_len,
        } = self;
        let payload_end = payload_offset
            .checked_add(payload_len)
            .expect("UVecUninitTxBuffer payload range overflow");
        let prefix = storage
            .get_mut(..payload_offset)
            .expect("UVecUninitTxBuffer prefix range must be in bounds");
        for slot in prefix {
            slot.write(0);
        }
        let suffix = storage
            .get_mut(payload_end..)
            .expect("UVecUninitTxBuffer suffix range must be in bounds");
        for slot in suffix {
            slot.write(0);
        }
        let len = storage.len();
        let capacity = storage.capacity();
        let ptr = storage.as_mut_ptr().cast::<u8>();
        std::mem::forget(storage);
        // SAFETY:
        // - `MaybeUninit<u8>` has the same size, alignment, and ABI as `u8` per
        //   the stable `MaybeUninit` layout docs.
        // - The caller of `assume_payload_init` guarantees the visible payload
        //   bytes are initialized; this function initializes the prefix and
        //   suffix bytes before conversion.
        // - `ptr`, `len`, and `capacity` come from the original `Vec` allocation,
        //   and `storage` was forgotten so the allocation has one owner.
        let storage = unsafe { Vec::from_raw_parts(ptr, len, capacity) };
        UVecTxBuffer {
            metadata,
            storage,
            payload_offset,
            payload_len,
        }
    }
}

impl UVecTxBuffer {
    /// Creates an owned transmit buffer with `payload_len` zero-initialized bytes.
    pub fn new(metadata: UFrameMetadata, payload_len: usize) -> Self {
        Self {
            metadata,
            storage: vec![0_u8; payload_len],
            payload_offset: 0,
            payload_len,
        }
    }

    /// Creates an owned transmit buffer whose visible payload starts at `alignment`.
    pub fn with_alignment(
        metadata: UFrameMetadata,
        payload_len: usize,
        alignment: usize,
    ) -> Result<Self, UWireError> {
        let layout = PayloadLayout::new(payload_len, alignment)?;
        if layout.is_empty() {
            return Ok(Self::new(metadata, payload_len));
        }
        let extra = layout.align().saturating_sub(1);
        let storage_len = layout.len().checked_add(extra).ok_or_else(|| {
            UWireError::invalid_payload("payload length plus alignment padding overflows usize")
        })?;
        let storage = vec![0_u8; storage_len];
        let address = storage.as_ptr() as usize;
        let payload_offset = if layout.align() == 1 {
            0
        } else {
            (layout.align() - (address % layout.align())) % layout.align()
        };
        Ok(Self {
            metadata,
            storage,
            payload_offset,
            payload_len: layout.len(),
        })
    }

    fn payload_range(&self) -> std::ops::Range<usize> {
        let end = self
            .payload_offset
            .checked_add(self.payload_len)
            .expect("UVecTxBuffer payload range overflow");
        self.payload_offset..end
    }

    /// Converts the buffer into an owned frame, consuming the emulated loan.
    pub fn into_frame(self) -> UOwnedFrame {
        let payload = self
            .storage
            .get(self.payload_range())
            .expect("UVecTxBuffer payload range must be in bounds")
            .to_vec();
        if self.metadata.encoding().is_some() {
            UOwnedFrame::with_payload_unchecked(self.metadata, payload)
        } else {
            UOwnedFrame::without_payload_unchecked(self.metadata)
        }
    }
}

impl AsRef<[u8]> for UVecTxBuffer {
    fn as_ref(&self) -> &[u8] {
        self.payload()
    }
}

impl UTxBuffer for UVecTxBuffer {
    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn payload(&self) -> &[u8] {
        self.storage
            .get(self.payload_range())
            .expect("UVecTxBuffer payload range must be in bounds")
    }

    fn payload_mut(&mut self) -> &mut [u8] {
        let range = self.payload_range();
        self.storage
            .get_mut(range)
            .expect("UVecTxBuffer payload range must be in bounds")
    }
}
