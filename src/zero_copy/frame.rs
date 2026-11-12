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
//! Prepared receive admission and checked typed leases over binding-owned frames.

use std::{
    future::Future,
    marker::PhantomData,
    mem,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use crate::wire::{UFrameFieldsMetadataCodec, UWireMetadataContext};
use crate::{
    parse_header, HeaderLimits, LoanedPayload, NativePayloadIdentity, NativeProfileAgreement,
    PayloadLoanProvenance, RxHeader, StableContainerPayload, StableContainerWire, StablePayload,
    UFrameHeaderError, UWireError,
};

/// Binding-owned immutable metadata and optional contiguous payload storage.
///
/// The frame retains every resource required by its byte views, including after
/// listener detachment or transport destruction. An absent payload differs from
/// a present empty payload. Implementations report provenance truthfully and do
/// not allocate, coalesce or copy storage to produce these views.
pub trait Frame: Send + 'static {
    /// Selected-wire metadata bytes, immutable while the frame is retained.
    fn encoded_metadata(&self) -> &[u8];
    /// Optional payload loan, including a present zero-length payload.
    fn payload(&self) -> Option<LoanedPayload<'_>>;
}

/// Fixed-size rejection before a received payload reaches its typed handler.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum AdmissionError {
    /// Header structure, metadata invariants or token admission failed.
    #[error("header admission failed: {0}")]
    Header(UFrameHeaderError),
    /// Encoded identity and carrier payload presence disagree.
    #[error("payload presence disagrees with metadata")]
    PayloadPresence,
    /// The encoded identity is not the registered payload type.
    #[error("wrong payload identity")]
    WrongPayloadType,
    /// The payload is not exactly the expected size.
    #[error("wrong payload length")]
    PayloadLength,
    /// The payload address does not satisfy the declared type's alignment.
    #[error("wrong payload alignment")]
    PayloadAlignment,
    /// At least one payload field has an invalid bit pattern.
    #[error("invalid payload field bits")]
    PayloadFieldBits,
}

struct ReceiveContext<P> {
    prefix: Box<[u8]>,
    limits: HeaderLimits,
    identity: NativePayloadIdentity,
    agreement: NativeProfileAgreement,
    representation_proof: PhantomData<fn() -> P>,
    stats: RxStats,
}

/// Admission counters shared by a prepared receiver and its leases.
#[derive(Debug, Default)]
pub struct RxStats {
    /// Frames accepted by typed representation admission.
    pub admitted: AtomicU64,
    /// Frames rejected before any typed handler runs.
    pub rejected: AtomicU64,
    /// Typed frames rejected by decoded source/sink filtering.
    pub filtered: AtomicU64,
}

/// A receive capability prepared once for a stable payload and confirmed agreement.
///
/// Preparation may allocate; admission and typed borrowing do not create a new
/// descriptor, context, string or allocation. The immutable agreement generation
/// is retained with every admitted lease.
pub struct RxAdmission<P> {
    context: Arc<ReceiveContext<P>>,
}

impl<P> Clone for RxAdmission<P> {
    fn clone(&self) -> Self {
        Self {
            context: Arc::clone(&self.context),
        }
    }
}

impl<P> std::fmt::Debug for RxAdmission<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RxAdmission")
            .field("identity", &self.context.identity)
            .field("limits", &self.context.limits)
            .finish_non_exhaustive()
    }
}

impl<P: StablePayload> RxAdmission<P> {
    /// Prepares exact representation, identity, prefix and receiver limits.
    ///
    /// # Errors
    /// Rejects an invalid representation, an unagreed payload type or a prefix
    /// the selected metadata codec cannot produce. Setup errors may own diagnostics.
    pub fn new(
        agreement: NativeProfileAgreement,
        limits: HeaderLimits,
    ) -> Result<Self, UWireError> {
        StableContainerPayload::<P>::representation_proven()?;
        let identity = StableContainerPayload::<P>::identity(&agreement)?;
        let prefix = UFrameFieldsMetadataCodec
            .native_prefix(UWireMetadataContext::from_wire::<
                StableContainerWire,
                UFrameFieldsMetadataCodec,
            >())
            .map_err(|error| UWireError::invalid_payload(error.to_string()))?;
        Ok(Self {
            context: Arc::new(ReceiveContext {
                prefix: prefix.into_boxed_slice(),
                limits,
                identity,
                agreement,
                representation_proof: PhantomData,
                stats: RxStats::default(),
            }),
        })
    }

    /// Checks header, payload presence, complete identity and representation before
    /// returning a typed lease. Rejections release the incoming frame.
    ///
    /// # Errors
    /// Returns a Copy admission reason without formatting an owned error string.
    pub fn admit<F: Frame>(&self, frame: F) -> Result<RxLease<P, F>, AdmissionError> {
        if let Err(error) = self.check_frame(&frame) {
            self.context.stats.rejected.fetch_add(1, Ordering::Relaxed);
            return Err(error);
        }
        self.context.stats.admitted.fetch_add(1, Ordering::Relaxed);
        Ok(RxLease {
            frame,
            admission: self.clone(),
        })
    }

    pub(crate) fn check_frame<'a, F: Frame>(&self, frame: &'a F) -> Result<&'a P, AdmissionError> {
        let header = self.parse(frame.encoded_metadata())?;
        let payload = frame.payload();
        if header.payload_encoding.is_some() != payload.is_some() {
            return Err(AdmissionError::PayloadPresence);
        }
        if header.payload_encoding != Some(self.context.identity.encoding())
            || header.native_type_token != Some(self.context.identity.token())
        {
            return Err(AdmissionError::WrongPayloadType);
        }
        let bytes = payload.ok_or(AdmissionError::PayloadPresence)?.bytes();
        if bytes.len() != mem::size_of::<P>() {
            return Err(AdmissionError::PayloadLength);
        }
        if !(bytes.as_ptr() as usize).is_multiple_of(mem::align_of::<P>()) {
            return Err(AdmissionError::PayloadAlignment);
        }
        if !P::FIELD_BITS_ALWAYS_VALID && !P::validate_field_bytes(bytes) {
            return Err(AdmissionError::PayloadFieldBits);
        }
        // SAFETY: ReceiveContext is created only after P's descriptor matches the
        // Rust size/alignment/target. This call checked exact payload identity,
        // size, address alignment and all field bits. StablePayload forbids
        // pointers, drop glue and interior mutability. The immutable input borrow
        // pins the bytes for the returned reference's lifetime.
        Ok(unsafe { &*bytes.as_ptr().cast::<P>() })
    }

    /// Expected complete payload identity, frozen at setup.
    #[must_use]
    pub fn identity(&self) -> NativePayloadIdentity {
        self.context.identity
    }

    /// Exact selected-wire prefix, prepared through the metadata codec.
    #[must_use]
    pub fn prefix(&self) -> &[u8] {
        &self.context.prefix
    }

    /// Counters for all admissions through this prepared receiver.
    #[must_use]
    pub fn stats(&self) -> &RxStats {
        &self.context.stats
    }

    pub(crate) fn parse<'a>(&self, bytes: &'a [u8]) -> Result<RxHeader<'a>, AdmissionError> {
        let prefix_len = UFrameFieldsMetadataCodec
            .stable_prefix_len(bytes)
            .map_err(AdmissionError::Header)?;
        let prefix = bytes
            .get(..prefix_len)
            .ok_or(AdmissionError::Header(UFrameHeaderError::Truncated))?;
        parse_header(prefix, bytes, &self.context.limits).map_err(AdmissionError::Header)
    }
}

/// An admitted native lease that retains its frame and agreement independently.
pub struct RxLease<P, F: Frame> {
    frame: F,
    admission: RxAdmission<P>,
}

impl<P, F: Frame> std::fmt::Debug for RxLease<P, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RxLease")
            .field("identity", &self.admission.context.identity)
            .finish_non_exhaustive()
    }
}

impl<P: StablePayload, F: Frame> RxLease<P, F> {
    /// Borrows a typed payload after rechecking the retained immutable frame.
    ///
    /// # Errors
    /// Reports a changed or invalid frame with a fixed-size admission reason.
    pub fn payload(&self) -> Result<&P, AdmissionError> {
        self.admission.check_frame(&self.frame)
    }

    /// Borrows a validated header, including token and traceparent slices.
    ///
    /// # Errors
    /// Reports header admission failure without allocating diagnostics.
    pub fn header(&self) -> Result<RxHeader<'_>, AdmissionError> {
        self.admission.parse(self.frame.encoded_metadata())
    }

    /// Reports the binding's storage provenance without treating it as type proof.
    #[must_use]
    pub fn provenance(&self) -> Option<PayloadLoanProvenance> {
        self.frame.payload().map(LoanedPayload::provenance)
    }

    /// Borrows the exact immutable agreement retained at admission.
    #[must_use]
    pub fn native_agreement(&self) -> &NativeProfileAgreement {
        &self.admission.context.agreement
    }

    /// Borrows the binding-owned frame without detaching its storage owner.
    #[must_use]
    pub fn frame(&self) -> &F {
        &self.frame
    }
}

/// Generic async application handler for one checked typed lease.
pub trait ZeroCopyHandler<P: StablePayload, F: Frame>: Send + 'static {
    /// Handles one admitted lease; the concrete future lives in its worker.
    fn on_receive(&mut self, lease: RxLease<P, F>) -> impl Future<Output = ()> + Send;
}

#[cfg(feature = "transport-implementer-api")]
struct AdmittedHandler<P, H> {
    admission: RxAdmission<P>,
    filter: Option<(crate::UUri, Option<crate::UUri>)>,
    handler: H,
}

#[cfg(feature = "transport-implementer-api")]
impl<P, F, H> crate::RegistrationHandler<F> for AdmittedHandler<P, H>
where
    P: StablePayload,
    F: Frame,
    H: ZeroCopyHandler<P, F>,
{
    async fn on_receive(&mut self, frame: F) {
        if let Ok(lease) = self.admission.admit(frame) {
            let accepted = self.filter.as_ref().is_none_or(|(source, sink)| {
                lease.header().is_ok_and(|header| {
                    source.matches(&header.source)
                        && match (sink, &header.sink) {
                            (None, None) => true,
                            (Some(filter), Some(actual)) => filter.matches(actual),
                            _ => false,
                        }
                })
            });
            if accepted {
                self.handler.on_receive(lease).await;
            } else {
                self.admission
                    .context
                    .stats
                    .filtered
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(feature = "transport-implementer-api")]
impl<P: StablePayload> RxAdmission<P> {
    /// Creates one generic worker that admits each frame before invoking the handler.
    ///
    /// # Errors
    /// Returns registration setup failures; rejected frames are counted later in
    /// this receiver's statistics and never delivered to the application handler.
    pub fn register<F, H, D>(
        &self,
        config: crate::RegistrationConfig,
        handler: H,
        detach: D,
    ) -> Result<crate::Registration<F>, crate::RegistrationSetupError>
    where
        F: Frame,
        H: ZeroCopyHandler<P, F>,
        D: FnOnce() + Send + 'static,
    {
        crate::Registration::new(
            config,
            AdmittedHandler {
                admission: self.clone(),
                filter: None,
                handler,
            },
            detach,
        )
    }

    /// Registers a typed worker with decoded source/sink matching after admission.
    ///
    /// Partial source filters are preserved; `sink=None` matches only sinkless
    /// messages. Physical delivery never substitutes for these decoded checks.
    ///
    /// # Errors
    /// Reports registration capacity, policy or runtime setup failures.
    pub fn register_filtered<F, H, D>(
        &self,
        source: crate::UUri,
        sink: Option<crate::UUri>,
        config: crate::RegistrationConfig,
        handler: H,
        detach: D,
    ) -> Result<crate::Registration<F>, crate::RegistrationSetupError>
    where
        F: Frame,
        H: ZeroCopyHandler<P, F>,
        D: FnOnce() + Send + 'static,
    {
        crate::Registration::new(
            config,
            AdmittedHandler {
                admission: self.clone(),
                filter: Some((source, sink)),
                handler,
            },
            detach,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::UWireMetadataCodec;
    use crate::{NativeProfile, NativeTypeToken, PayloadEncoding, UFrameMetadata, UUri};

    #[repr(C)]
    #[derive(Debug, Eq, PartialEq, crate::StablePayload)]
    #[stable_payload(type_name = "uprotocol.test.PreparedAdmission")]
    struct Value {
        flag: bool,
        padding: [u8; 3],
        counter: u32,
    }

    #[repr(align(8))]
    struct Aligned([u8; 16]);

    struct TestFrame {
        metadata: Vec<u8>,
        payload: Aligned,
        offset: usize,
        len: usize,
        present: bool,
    }

    impl Frame for TestFrame {
        fn encoded_metadata(&self) -> &[u8] {
            &self.metadata
        }
        fn payload(&self) -> Option<LoanedPayload<'_>> {
            self.present.then(|| {
                LoanedPayload::new(
                    self.payload
                        .0
                        .get(self.offset..self.offset + self.len)
                        .unwrap(),
                    PayloadLoanProvenance::OwnedReceiveLease,
                )
            })
        }
    }

    fn admission() -> RxAdmission<Value> {
        let profile = NativeProfile::new(
            "prepared-admission",
            1,
            PayloadEncoding::from_id(0xF101).unwrap(),
            [Value::native_representation()],
        )
        .unwrap();
        RxAdmission::new(
            crate::test_support::self_agreement(profile),
            HeaderLimits::default(),
        )
        .unwrap()
    }

    fn frame(admission: &RxAdmission<Value>, identity: NativePayloadIdentity) -> TestFrame {
        let metadata = UFrameMetadata::publish(
            UUri::try_from_wire_parts("source", 0x0001_0001, 1, 0x8001).unwrap(),
        )
        .with_native_payload_identity(identity)
        .with_token("borrowed-token")
        .build()
        .unwrap();
        let context =
            UWireMetadataContext::from_wire::<StableContainerWire, UFrameFieldsMetadataCodec>();
        let metadata = UFrameFieldsMetadataCodec
            .encode_frame_metadata(context, &metadata)
            .unwrap();
        assert!(metadata.starts_with(admission.prefix()));
        let mut payload = Aligned([0; 16]);
        *payload.0.first_mut().unwrap() = 1;
        payload
            .0
            .get_mut(4..8)
            .unwrap()
            .copy_from_slice(&42_u32.to_ne_bytes());
        TestFrame {
            metadata,
            payload,
            offset: 0,
            len: 8,
            present: true,
        }
    }

    #[test]
    fn typed_lease_retains_value_token_and_provenance() {
        let admission = admission();
        let frame = frame(&admission, admission.identity());
        let lease = admission.admit(frame).unwrap();
        assert_eq!(
            lease.payload().unwrap(),
            &Value {
                flag: true,
                padding: [0; 3],
                counter: 42
            }
        );
        assert_eq!(lease.header().unwrap().token(), Some("borrowed-token"));
        assert_eq!(
            lease.provenance(),
            Some(PayloadLoanProvenance::OwnedReceiveLease)
        );
        drop(admission);
        assert_eq!(lease.payload().unwrap().counter, 42);
        assert!(!format!("{lease:?}").contains("borrowed-token"));
    }

    #[test]
    fn rejection_precedes_typed_references() {
        let admission = admission();
        let mut absent = frame(&admission, admission.identity());
        absent.present = false;
        assert!(matches!(
            admission.admit(absent),
            Err(AdmissionError::PayloadPresence)
        ));
        let mut short = frame(&admission, admission.identity());
        short.len = 7;
        assert!(matches!(
            admission.admit(short),
            Err(AdmissionError::PayloadLength)
        ));
        let mut unaligned = frame(&admission, admission.identity());
        unaligned.offset = 1;
        assert!(matches!(
            admission.admit(unaligned),
            Err(AdmissionError::PayloadAlignment)
        ));
        let mut invalid = frame(&admission, admission.identity());
        *invalid.payload.0.first_mut().unwrap() = 2;
        assert!(matches!(
            admission.admit(invalid),
            Err(AdmissionError::PayloadFieldBits)
        ));
        let mut foreign = frame(&admission, admission.identity());
        let end = foreign.metadata.len();
        foreign
            .metadata
            .get_mut(end - 4..)
            .unwrap()
            .copy_from_slice(
                &NativeTypeToken::from_u32(admission.identity().token().as_u32() ^ 1)
                    .as_u32()
                    .to_le_bytes(),
            );
        assert!(matches!(
            admission.admit(foreign),
            Err(AdmissionError::WrongPayloadType)
        ));
        assert_eq!(admission.stats().rejected.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn registered_literal_prefix_is_accepted_without_weakening_private_identity() {
        use crate::wire::{UWire, UFRAME_FIELDS_METADATA_LAYOUT_ID};
        let admission = admission();
        let mut received = frame(&admission, admission.identity());
        assert_eq!(received.metadata.get(4), Some(&0));
        let literal = UFRAME_FIELDS_METADATA_LAYOUT_ID.literal_id().as_bytes();
        let mut expanded = b"UPWM".to_vec();
        expanded.push(1);
        expanded.extend_from_slice(&(literal.len() as u32).to_le_bytes());
        expanded.extend_from_slice(literal);
        expanded.extend_from_slice(received.metadata.get(7..).unwrap());
        received.metadata = expanded;
        let context =
            UWireMetadataContext::from_wire::<StableContainerWire, UFrameFieldsMetadataCodec>();
        assert!(UFrameFieldsMetadataCodec
            .decode_frame_metadata(context, &received.metadata)
            .is_ok());
        assert_eq!(
            admission
                .admit(received)
                .unwrap()
                .payload()
                .unwrap()
                .counter,
            42
        );

        let mut private_alias = frame(&admission, admission.identity());
        let wire_offset = 4 + 3 + 2;
        assert_eq!(private_alias.metadata.get(wire_offset), Some(&1));
        let wire_end = wire_offset + 5 + StableContainerWire::WIRE_ID.literal_id().len();
        let mut shortened = private_alias.metadata.get(..wire_offset).unwrap().to_vec();
        shortened.push(0);
        shortened.extend_from_slice(&StableContainerWire::WIRE_ID.compact_id().to_le_bytes());
        shortened.extend_from_slice(private_alias.metadata.get(wire_end..).unwrap());
        private_alias.metadata = shortened;
        assert!(UFrameFieldsMetadataCodec
            .decode_frame_metadata(context, &private_alias.metadata)
            .is_err());
        assert!(matches!(
            admission.admit(private_alias),
            Err(AdmissionError::Header(UFrameHeaderError::WrongPrefix))
        ));
    }

    #[cfg(feature = "transport-implementer-api")]
    #[tokio::test]
    async fn typed_registration_never_calls_handler_for_rejected_frames() {
        struct Handler(Arc<AtomicU64>);
        impl ZeroCopyHandler<Value, TestFrame> for Handler {
            async fn on_receive(&mut self, lease: RxLease<Value, TestFrame>) {
                self.0.fetch_add(
                    u64::from(lease.payload().unwrap().counter),
                    Ordering::Relaxed,
                );
            }
        }
        let admission = admission();
        let seen = Arc::new(AtomicU64::new(0));
        let registration = admission
            .register(
                crate::RegistrationConfig {
                    capacity: 2,
                    overflow: crate::OverflowPolicy::DropNewest,
                },
                Handler(Arc::clone(&seen)),
                || {},
            )
            .unwrap();
        let inbox = registration.inbox();
        let mut invalid = frame(&admission, admission.identity());
        invalid.len = 1;
        inbox.push(invalid).unwrap();
        inbox.push(frame(&admission, admission.identity())).unwrap();
        registration.stop().await.unwrap();
        assert_eq!(seen.load(Ordering::Relaxed), 42);
        assert_eq!(admission.stats().admitted.load(Ordering::Relaxed), 1);
        assert_eq!(admission.stats().rejected.load(Ordering::Relaxed), 1);
    }
}
