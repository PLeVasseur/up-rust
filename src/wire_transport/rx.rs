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
//! Validated receive views over retained encoded storage.

use super::*;

/// Raw encoded receive object returned by a transport core.
///
/// This is an implementation-boundary trait. Raw encoded receive objects should
/// not implement public frame or lease traits directly; public receive paths
/// expose [`UWireRx<Rx, W, C>`] after selected-wire metadata decode and validation.
///
/// The object must retain all resources backing both metadata and payload views
/// for its lifetime, including after listener unregistration or core destruction.
/// Retaining a sample-handle allocation without its native proxy/mapping owner
/// does not satisfy this contract.
pub trait UEncodedRxFrame {
    /// Ordered payload reader type.
    type PayloadReader<'a>: Read + 'a
    where
        Self: 'a;
    /// Ordered payload slices iterator type.
    type PayloadSlices<'a>: Iterator<Item = &'a [u8]> + 'a
    where
        Self: 'a;

    /// Returns selected-wire encoded metadata bytes.
    fn encoded_metadata(&self) -> &[u8];

    /// Returns the visible application payload length.
    fn payload_len(&self) -> usize;

    /// Returns an ordered reader over the application payload bytes.
    fn payload_reader(&self) -> Self::PayloadReader<'_>;

    /// Returns ordered borrowed payload slices.
    fn payload_slices(&self) -> Self::PayloadSlices<'_>;

    /// Returns a contiguous borrowed payload view when available without copying.
    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        None
    }
}

#[cfg(feature = "zero-copy-transport")]
/// Raw encoded receive object that can prove its contiguous payload is loan-backed.
pub trait UEncodedLoanedRxFrame: UEncodedRxFrame {
    /// Returns one contiguous loan-backed application payload view.
    ///
    /// Implementations must not allocate, copy, or coalesce payload bytes to
    /// satisfy this method.
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError>;
}

/// Public zero-copy receive lease after selected-wire metadata validation.
pub struct UWireRx<Rx, W, C>
where
    W: UWire,
{
    metadata: UFrameMetadata,
    native_agreement: Option<crate::NativeProfileAgreement>,
    raw: Rx,
    _wire: PhantomData<W>,
    _metadata_codec: PhantomData<C>,
}

impl<Rx, W, C> core::fmt::Debug for UWireRx<Rx, W, C>
where
    W: UWire,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UWireRx").finish_non_exhaustive()
    }
}

impl<Rx, W, C> UWireRx<Rx, W, C>
where
    Rx: UEncodedRxFrame,
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
    /// Decodes metadata from a raw encoded receive object and validates the public frame view.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata bytes are malformed, selected-wire checks
    /// fail, or the resulting public frame view violates transport invariants.
    pub fn try_from_encoded(raw: Rx, codec: &C) -> Result<Self, UStatus> {
        Self::from_encoded_in_context(raw, codec, None)
    }

    /// Decodes a view while retaining the confirmed agreement for its route.
    /// Payload bytes remain opaque until an explicit typed operation is requested.
    ///
    /// # Errors
    ///
    /// Returns the same framing/metadata errors as [`Self::try_from_encoded`].
    pub fn try_from_encoded_with_native_agreement(
        raw: Rx,
        codec: &C,
        agreement: crate::NativeProfileAgreement,
    ) -> Result<Self, UStatus> {
        Self::from_encoded_in_context(raw, codec, Some(agreement))
    }

    fn from_encoded_in_context(
        raw: Rx,
        codec: &C,
        native_agreement: Option<crate::NativeProfileAgreement>,
    ) -> Result<Self, UStatus> {
        let metadata = codec.decode_frame_metadata(
            UWireMetadataContext::from_wire::<W, C>(),
            raw.encoded_metadata(),
        )?;
        let frame = Self {
            metadata,
            native_agreement,
            raw,
            _wire: PhantomData,
            _metadata_codec: PhantomData,
        };
        validate_frame_view_for_transport(&frame)?;
        Ok(frame)
    }

    /// Returns the raw encoded receive object behind this public wrapper.
    #[must_use]
    pub fn raw(&self) -> &Rx {
        &self.raw
    }

    /// Consumes the public wrapper and returns the raw encoded receive object.
    #[must_use]
    pub fn into_raw(self) -> Rx {
        self.raw
    }

    /// Decodes this frame's payload using the selected wire `W`.
    ///
    /// Prefer this selected-wire helper on receive values produced by
    /// explicit selected-wire adapter construction. Use
    /// [`UFrameView::decode_payload_from_reader_as`] only for
    /// explicit low-level codec selection.
    ///
    /// # Errors
    ///
    /// Returns an error if the frame has missing or incompatible payload encoding,
    /// has no payload, or if the selected wire cannot decode the payload bytes.
    pub fn decode_payload<T>(&self, limit: PayloadDecodeLimit) -> Result<T, UWireError>
    where
        W: UWirePayload<T>,
        <W as UWirePayload<T>>::Codec: ReadDecodePayload<T>,
    {
        <<W as UWirePayload<T>>::Codec as crate::payload::codec::PayloadCodec>::verify_metadata(
            &self.metadata,
            self.native_agreement.as_ref(),
        )?;
        if !self.has_payload() {
            return Err(UWireError::MissingPayload);
        }
        <W as UWirePayload<T>>::Codec::decode_payload_from_reader(
            self.payload_reader(),
            self.payload_len(),
            limit,
        )
    }
}

#[cfg(feature = "zero-copy-transport")]
impl<Rx, W, C> UWireRx<Rx, W, C>
where
    Rx: UEncodedLoanedRxFrame,
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
    /// Borrows this frame's payload through the selected wire mapping.
    ///
    /// # Errors
    ///
    /// Returns an error for missing or incompatible encoding, absent/non-loaned
    /// payload storage, invalid size/alignment or invalid field bits.
    pub fn borrow_payload<T>(&self) -> Result<&T, UWireError>
    where
        W: UWirePayload<T>,
        <W as UWirePayload<T>>::Codec: BorrowPayload<T>,
    {
        <<W as UWirePayload<T>>::Codec as crate::PayloadCodec>::verify_metadata(
            &self.metadata,
            self.native_agreement.as_ref(),
        )?;
        if !self.has_payload() {
            return Err(UWireError::MissingPayload);
        }
        let payload = self.raw.loaned_contiguous_payload()?;
        <<W as UWirePayload<T>>::Codec as BorrowPayload<T>>::borrow_payload(payload.bytes())
    }

    /// Borrows this frame's payload through the selected wire's expert lane.
    ///
    /// Encoding, payload presence and loan-backed contiguity remain checked.
    /// `unchecked` permits but does not guarantee bit-validation elision.
    ///
    /// # Safety
    ///
    /// The caller must prove closed producer provenance, typed construction of
    /// the exact requested Rust representation and recursive bit validity for
    /// `T`. Wire/profile identity, size, alignment and loan-backed contiguity
    /// remain checked; only the recursive bit validator may be elided.
    pub unsafe fn borrow_payload_unchecked<T>(&self) -> Result<&T, UWireError>
    where
        W: UWirePayload<T>,
        <W as UWirePayload<T>>::Codec: BorrowPayload<T>,
    {
        <<W as UWirePayload<T>>::Codec as crate::PayloadCodec>::verify_metadata(
            &self.metadata,
            self.native_agreement.as_ref(),
        )?;
        if !self.has_payload() {
            return Err(UWireError::MissingPayload);
        }
        let payload = self.raw.loaned_contiguous_payload()?;
        // SAFETY: This function's caller must establish closed producer
        // provenance, exact typed construction and recursive validity for T.
        // Metadata, presence and loan-backed contiguity were checked above;
        // the selected codec retains its size/alignment checks.
        unsafe {
            <<W as UWirePayload<T>>::Codec as BorrowPayload<T>>::borrow_payload_unchecked(
                payload.bytes(),
            )
        }
    }
}

impl<Rx, W, C> UFrameView for UWireRx<Rx, W, C>
where
    Rx: UEncodedRxFrame,
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
    type PayloadReader<'a>
        = Rx::PayloadReader<'a>
    where
        Self: 'a;
    type PayloadSlices<'a>
        = Rx::PayloadSlices<'a>
    where
        Self: 'a;

    fn metadata(&self) -> &UFrameMetadata {
        &self.metadata
    }

    fn native_agreement(&self) -> Option<&crate::NativeProfileAgreement> {
        self.native_agreement.as_ref()
    }

    fn payload_len(&self) -> usize {
        self.raw.payload_len()
    }

    fn has_payload(&self) -> bool {
        self.payload_len() > 0 || self.metadata.payload_encoding().is_some()
    }

    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        self.raw.payload_reader()
    }

    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        self.raw.payload_slices()
    }

    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        self.raw.try_contiguous_payload()
    }
}

#[cfg(feature = "zero-copy-transport")]
impl<Rx, W, C> UZeroCopyRxLease for UWireRx<Rx, W, C>
where
    Rx: UEncodedRxFrame,
    W: UWire,
    C: UWireMetadataCodecFor<W>,
{
}

#[cfg(feature = "zero-copy-transport")]
impl<Rx, C> ULoanedContiguousZeroCopyRxFrame for UWireRx<Rx, StableContainerWire, C>
where
    Rx: UEncodedLoanedRxFrame,
    C: UWireMetadataCodecFor<StableContainerWire>,
{
    fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError> {
        self.raw.loaned_contiguous_payload()
    }
}
