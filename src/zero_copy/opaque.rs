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
//! In-place metadata admission for opaque selected-wire carriage.

// Only some feature sets call the extension traits here.
#[allow(unused_imports)]
use crate::prelude::*;
use crate::wire::{UFrameFieldsMetadataCodec, UWire, UWireMetadataContext};
use crate::{AdmissionError, Frame, HeaderLimits, LoanedPayload, RxHeader, UWireError};
use std::{
    future::Future,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

/// Setup-owned exact selected-wire admission without interpreting the payload.
///
/// Payload encoding and native type token remain independent opaque values. This
/// type does not form typed payload references and therefore does not require a
/// native agreement merely to carry bytes. Use `RxAdmission<P>` for typed admission.
/// Exact identities accept registered compact or literal spelling and require a
/// literal for private identities; this API does not apply custom alias callbacks.
#[derive(Clone)]
pub struct HeaderAdmission {
    pub(crate) context: UWireMetadataContext,
    pub(crate) prefix: Arc<[u8]>,
    limits: HeaderLimits,
    stats: Arc<HeaderAdmissionStats>,
}

/// Counters for opaque metadata admission and optional decoded filtering.
#[derive(Debug, Default)]
pub struct HeaderAdmissionStats {
    /// Frames whose metadata and payload presence were accepted.
    pub admitted: AtomicU64,
    /// Frames rejected during metadata/presence validation.
    pub rejected: AtomicU64,
    /// Admitted frames rejected by decoded source/sink filtering.
    pub filtered: AtomicU64,
}
impl std::fmt::Debug for HeaderAdmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeaderAdmission")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}
impl HeaderAdmission {
    /// Resolves exact wire/family/layout identities and reserves their prefix once.
    ///
    /// # Errors
    /// Rejects an invalid prefix or a token limit beyond the wire's length field.
    pub fn for_wire<W: UWire>(limits: HeaderLimits) -> Result<Self, UWireError> {
        if limits.max_token_len > u16::MAX as usize {
            return Err(UWireError::invalid_payload(
                "token capacity exceeds wire length field",
            ));
        }
        let context = UWireMetadataContext::from_wire::<W, UFrameFieldsMetadataCodec>();
        let prefix = UFrameFieldsMetadataCodec
            .native_prefix(context)
            .map_err(|_| UWireError::invalid_payload("invalid native prefix"))?
            .into();
        Ok(Self {
            context,
            prefix,
            limits,
            stats: Arc::new(HeaderAdmissionStats::default()),
        })
    }

    fn header<'a, F: Frame>(&self, frame: &'a F) -> Result<RxHeader<'a>, AdmissionError> {
        let bytes = frame.encoded_metadata();
        let prefix_len = UFrameFieldsMetadataCodec
            .exact_prefix_len(self.context, bytes)
            .map_err(AdmissionError::Header)?;
        let prefix = bytes
            .get(..prefix_len)
            .ok_or(AdmissionError::Header(crate::UFrameHeaderError::Truncated))?;
        let header =
            crate::parse_header(prefix, bytes, &self.limits).map_err(AdmissionError::Header)?;
        if header.payload_encoding.is_some() != frame.payload().is_some() {
            return Err(AdmissionError::PayloadPresence);
        }
        Ok(header)
    }

    /// Validates header and carrier presence, retaining the original opaque storage.
    ///
    /// # Errors
    /// Rejects wrong identities, malformed/invalid metadata, token overflow or
    /// inconsistent payload presence. Payload bytes and unknown identifiers are preserved.
    pub fn admit<F: Frame>(&self, frame: F) -> Result<HeaderLease<F>, AdmissionError> {
        if let Err(error) = self.header(&frame) {
            self.stats.rejected.fetch_add(1, Ordering::Relaxed);
            return Err(error);
        }
        self.stats.admitted.fetch_add(1, Ordering::Relaxed);
        Ok(HeaderLease {
            frame,
            admission: self.clone(),
        })
    }

    /// Counters shared by clones of this admission configuration.
    #[must_use]
    pub fn stats(&self) -> &HeaderAdmissionStats {
        &self.stats
    }

    /// Borrows the canonical prefix reserved at setup for this exact context.
    ///
    /// Receive matching also accepts valid registered literal spellings; this
    /// canonical output is not the only valid encoding of such an identity.
    #[must_use]
    pub fn prefix(&self) -> &[u8] {
        &self.prefix
    }

    /// Composes opaque admission with a generic worker and a reserved inbox.
    ///
    /// # Errors
    /// Rejects invalid registration capacity/policy or unavailable runtime state.
    #[cfg(feature = "transport-implementer-api")]
    pub fn register<F, H, D>(
        &self,
        config: crate::RegistrationConfig,
        handler: H,
        detach: D,
    ) -> Result<crate::Registration<F>, crate::RegistrationSetupError>
    where
        F: Frame,
        H: HeaderHandler<F>,
        D: FnOnce() + Send + 'static,
    {
        crate::Registration::new(
            config,
            Admitted {
                admission: self.clone(),
                filter: None,
                handler,
            },
            detach,
        )
    }

    /// Registers an opaque worker with decoded source/sink filtering.
    ///
    /// `sink=None` admits sinkless messages only. Partial URI filters are retained
    /// as supplied; a broad physical delivery hint cannot bypass these checks.
    ///
    /// # Errors
    /// Reports registration capacity/policy or runtime setup failures.
    #[cfg(feature = "transport-implementer-api")]
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
        H: HeaderHandler<F>,
        D: FnOnce() + Send + 'static,
    {
        crate::Registration::new(
            config,
            Admitted {
                admission: self.clone(),
                filter: Some((source, sink)),
                handler,
            },
            detach,
        )
    }
}

/// A retained frame with validated metadata and an uninterpreted payload.
pub struct HeaderLease<F: Frame> {
    frame: F,
    admission: HeaderAdmission,
}
impl<F: Frame> std::fmt::Debug for HeaderLease<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeaderLease").finish_non_exhaustive()
    }
}
impl<F: Frame> HeaderLease<F> {
    /// Borrows the validated header, including token and traceparent input bytes.
    ///
    /// # Errors
    /// Reports a carrier that no longer satisfies its immutable-view contract.
    pub fn header(&self) -> Result<RxHeader<'_>, AdmissionError> {
        self.admission.header(&self.frame)
    }
    /// Borrows opaque payload bytes without interpreting identity or field bits.
    #[must_use]
    pub fn payload(&self) -> Option<LoanedPayload<'_>> {
        self.frame.payload()
    }
    /// Releases this metadata wrapper while preserving the underlying storage owner.
    #[must_use]
    pub fn into_frame(self) -> F {
        self.frame
    }
}

/// Generic asynchronous handler of an admitted opaque frame.
pub trait HeaderHandler<F: Frame>: Send + 'static {
    /// Handles one retained frame in registration order without a boxed future.
    fn on_receive(&mut self, frame: HeaderLease<F>) -> impl Future<Output = ()> + Send;
}

#[cfg(feature = "transport-implementer-api")]
struct Admitted<H> {
    admission: HeaderAdmission,
    filter: Option<(crate::UUri, Option<crate::UUri>)>,
    handler: H,
}
#[cfg(feature = "transport-implementer-api")]
impl<F: Frame, H: HeaderHandler<F>> crate::RegistrationHandler<F> for Admitted<H> {
    async fn on_receive(&mut self, frame: F) {
        if let Ok(frame) = self.admission.admit(frame) {
            let accepted = self.filter.as_ref().is_none_or(|(source, sink)| {
                frame.header().is_ok_and(|header| {
                    source.matches(&header.source)
                        && match (sink, &header.sink) {
                            (None, None) => true,
                            (Some(filter), Some(actual)) => filter.matches(actual),
                            _ => false,
                        }
                })
            });
            if accepted {
                self.handler.on_receive(frame).await;
            } else {
                self.admission
                    .stats
                    .filtered
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(feature = "transport-implementer-api")]
mod transmit {
    use super::*;
    use crate::{
        InitializedTx, PreparedTransportCore, PreparedTxStorage, RouteConfig, RouteLimits,
        SendError, SetupError, TxFrameLayout, TxHeaderParts, UFrameHeaderError, UUri,
    };

    /// Setup-owned wire prefix for bounded opaque routes.
    ///
    /// This prepares metadata identity, not an application payload descriptor.
    #[derive(Clone, Debug)]
    pub struct OpaqueTemplate {
        admission: HeaderAdmission,
    }
    impl OpaqueTemplate {
        /// Selects exact wire/family/layout identities once before traffic.
        ///
        /// # Errors
        /// Rejects an invalid selected-wire prefix.
        pub fn for_wire<W: UWire>() -> Result<Self, SetupError> {
            Ok(Self {
                admission: HeaderAdmission::for_wire::<W>(HeaderLimits::default())
                    .map_err(|_| SetupError::Prefix)?,
            })
        }

        /// Reserves an exact-address route and logical storage bounds.
        ///
        /// # Errors
        /// Rejects wildcard addresses, invalid capacities or unavailable core state.
        pub fn route<C: PreparedTransportCore>(
            &self,
            core: Arc<C>,
            source: UUri,
            sink: Option<UUri>,
            limits: RouteLimits,
            config: RouteConfig,
        ) -> Result<OpaqueRoute<C>, SetupError> {
            source
                .verify_no_wildcards()
                .map_err(|_| SetupError::InvalidRoute("exact source required"))?;
            if let Some(sink) = &sink {
                sink.verify_no_wildcards()
                    .map_err(|_| SetupError::InvalidRoute("exact sink required"))?;
            }
            if !limits.payload_alignment.is_power_of_two()
                || config.max_token_len > u16::MAX as usize
            {
                return Err(SetupError::InvalidConfig("opaque route limits"));
            }
            let route = core.prepare_route(&source, sink.as_ref(), limits)?;
            Ok(OpaqueRoute {
                core,
                route,
                source,
                sink,
                limits,
                config,
                template: self.clone(),
            })
        }
    }

    /// Prepared carriage of serialized/opaque bytes, with one explicit payload copy.
    ///
    /// Payload interpretation belongs to its codec/consumer. This path preserves
    /// identities without forming a typed reference or claiming end-to-end zero-copy.
    pub struct OpaqueRoute<C: PreparedTransportCore> {
        core: Arc<C>,
        route: C::Route,
        source: UUri,
        sink: Option<UUri>,
        limits: RouteLimits,
        config: RouteConfig,
        template: OpaqueTemplate,
    }
    impl<C: PreparedTransportCore> std::fmt::Debug for OpaqueRoute<C> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("OpaqueRoute")
                .field("source", &self.source)
                .field("sink", &self.sink)
                .finish_non_exhaustive()
        }
    }
    impl<C: PreparedTransportCore> OpaqueRoute<C> {
        /// Originates bytes with validated metadata, refusing reserved encoding IDs.
        ///
        /// # Errors
        /// Rejects wrong route fields, reserved origination, capacity/presence
        /// mismatches, invalid metadata, unavailable storage or delivery failure.
        pub fn send_bytes(
            &self,
            header: TxHeaderParts<'_>,
            payload: Option<&[u8]>,
        ) -> Result<(), SendError> {
            if header
                .payload_encoding
                .is_some_and(|encoding| encoding.is_reserved())
            {
                return Err(SendError::Header(UFrameHeaderError::Invalid(
                    "reserved payload origination",
                )));
            }
            self.write_bytes(header, payload)
        }

        /// Forwards admitted bytes under the same exact wire/family/layout identity.
        ///
        /// Reserved/unknown incoming payload IDs remain opaque and are preserved.
        /// The payload is copied once to this route's egress loan. No native
        /// agreement is invented and no typed payload reference is formed.
        ///
        /// # Errors
        /// Rejects identity relabeling, changed metadata/presence, wrong addresses,
        /// capacity or storage errors, and failed delivery.
        pub fn forward<F: Frame>(&self, frame: &HeaderLease<F>) -> Result<(), SendError> {
            let incoming = frame.admission.context;
            let outgoing = self.template.admission.context;
            if incoming.wire_id.literal_id() != outgoing.wire_id.literal_id()
                || incoming.payload_family_id.literal_id()
                    != outgoing.payload_family_id.literal_id()
                || incoming.metadata_layout_id.literal_id()
                    != outgoing.metadata_layout_id.literal_id()
                || incoming.format_version != outgoing.format_version
            {
                return Err(SendError::Header(UFrameHeaderError::WrongPrefix));
            }
            let header = frame.header().map_err(|error| match error {
                AdmissionError::Header(error) => SendError::Header(error),
                AdmissionError::PayloadPresence
                | AdmissionError::WrongPayloadType
                | AdmissionError::PayloadLength
                | AdmissionError::PayloadAlignment
                | AdmissionError::PayloadFieldBits
                | _ => SendError::PayloadStorage,
            })?;
            let payload = frame.payload();
            self.write_bytes(
                TxHeaderParts::from(&header),
                payload.map(LoanedPayload::bytes),
            )
        }

        fn write_bytes(
            &self,
            header: TxHeaderParts<'_>,
            payload: Option<&[u8]>,
        ) -> Result<(), SendError> {
            if header.source != &self.source || header.sink != self.sink.as_ref() {
                return Err(SendError::Header(UFrameHeaderError::Invalid(
                    "opaque route addressing",
                )));
            }
            if header.payload_encoding.is_some() != payload.is_some() {
                return Err(SendError::PayloadStorage);
            }
            if let Some(token) = header.token {
                if self.config.max_token_len == 0 {
                    return Err(SendError::TokenNotAllowed);
                }
                if token.len() > self.config.max_token_len {
                    return Err(SendError::TokenTooLong);
                }
            }
            if header.traceparent.is_some_and(|trace| {
                !self.config.allow_traceparent || trace.len() > crate::HEADER_TRACEPARENT_CAPACITY
            }) {
                return Err(SendError::TraceparentRejected);
            }
            let prefix = &self.template.admission.prefix;
            let metadata_len = crate::header_len(prefix, &header).map_err(SendError::Header)?;
            if metadata_len > self.limits.max_metadata_len
                || payload.is_some_and(|bytes| bytes.len() > self.limits.max_payload_len)
            {
                return Err(SendError::Loan(crate::LoanError::SlotTooSmall));
            }
            let mut storage = self
                .core
                .loan(
                    &self.route,
                    TxFrameLayout {
                        metadata_len,
                        payload_len: payload.map(<[u8]>::len),
                        payload_alignment: self.limits.payload_alignment,
                    },
                )
                .map_err(SendError::Loan)?;
            if storage.metadata_uninit_mut().len() != metadata_len {
                return Err(SendError::PayloadStorage);
            }
            let written =
                crate::write_header_uninit(prefix, &header, storage.metadata_uninit_mut())
                    .map_err(SendError::Header)?;
            storage.record_header(written)?;
            match (storage.payload_uninit_mut(), payload) {
                (None, None) => {}
                (Some(out), Some(bytes))
                    if out.len() == bytes.len()
                        && (out.as_ptr() as usize)
                            .is_multiple_of(self.limits.payload_alignment) =>
                {
                    for (out, byte) in out.iter_mut().zip(bytes) {
                        out.write(*byte);
                    }
                }
                _ => return Err(SendError::PayloadStorage),
            }
            // SAFETY: the checked initializer above wrote every SDK-visible metadata
            // and present payload byte of this loan and recorded its header extent.
            self.core.publish(&self.route, unsafe {
                InitializedTx::from_initialized(storage)
            })
        }
    }
}

#[cfg(feature = "transport-implementer-api")]
pub use transmit::{OpaqueRoute, OpaqueTemplate};

#[cfg(all(test, feature = "transport-implementer-api"))]
mod tests {
    use super::*;
    use crate::wire::{UProtocolNativeWire, WireIdentity};
    use crate::{
        NativeTypeToken, OverflowPolicy, PayloadEncoding, PayloadLoanProvenance, PoolCore,
        PoolCoreConfig, PreparedTransportCore, RegistrationConfig, RegistrationInbox, RouteConfig,
        RouteLimits, SendError, TxHeaderParts, UFrameHeaderError, UMessageType, UUri, UUID,
    };

    struct Raw {
        metadata: Vec<u8>,
        payload: Option<Vec<u8>>,
    }
    impl Frame for Raw {
        fn peer(&self) -> crate::transport::PeerRef {
            crate::transport::PeerRef::UNKNOWN
        }

        fn encoded_metadata(&self) -> &[u8] {
            &self.metadata
        }
        fn payload(&self) -> Option<LoanedPayload<'_>> {
            self.payload
                .as_ref()
                .map(|bytes| LoanedPayload::new(bytes, PayloadLoanProvenance::OwnedReceiveLease))
        }
    }
    fn source(resource: u16) -> UUri {
        UUri::try_from_wire_parts("sender", 0x0001_0001, 1, resource).unwrap()
    }
    fn raw(encoding: Option<u32>, payload: Option<&[u8]>, resource: u16, sink: bool) -> Raw {
        let admission =
            HeaderAdmission::for_wire::<UProtocolNativeWire>(HeaderLimits::default()).unwrap();
        let id = UUID::build();
        let source = source(resource);
        let sink = sink.then(|| UUri::try_from_wire_parts("receiver", 0x0001_0002, 1, 0).unwrap());
        let fields = TxHeaderParts {
            kind: if sink.is_some() {
                UMessageType::Notification
            } else {
                UMessageType::Publish
            },
            priority: None,
            id: &id,
            reqid: None,
            ttl: None,
            comm_status: None,
            permission_level: None,
            source: &source,
            sink: sink.as_ref(),
            token: Some("opaque-token"),
            traceparent: None,
            payload_encoding: encoding.map(|id| PayloadEncoding::from_id(id).unwrap()),
            native_type_token: encoding.map(|_| NativeTypeToken::from_u32(0xDEAD_BEEF)),
        };
        let mut metadata = vec![0; crate::header_len(&admission.prefix, &fields).unwrap()];
        crate::write_header(&admission.prefix, &fields, &mut metadata).unwrap();
        Raw {
            metadata,
            payload: payload.map(<[u8]>::to_vec),
        }
    }

    #[test_case::test_case(0; "contract defined zero")]
    #[test_case::test_case(8; "unassigned registry value")]
    #[test_case::test_case(0xE000; "reserved incoming value")]
    #[test_case::test_case(0xFFFF; "future reserved incoming value")]
    fn opaque_receive_keeps_unknown_identity_bytes_and_storage(encoding: u32) {
        let admission =
            HeaderAdmission::for_wire::<UProtocolNativeWire>(HeaderLimits::default()).unwrap();
        let lease = admission
            .admit(raw(Some(encoding), Some(&[0xFF, 0, 0x80]), 0x8001, false))
            .unwrap();
        drop(admission);
        assert_eq!(
            lease.header().unwrap().payload_encoding.unwrap().id(),
            encoding
        );
        assert_eq!(
            lease.header().unwrap().native_type_token,
            Some(NativeTypeToken::from_u32(0xDEAD_BEEF))
        );
        assert_eq!(lease.header().unwrap().token(), Some("opaque-token"));
        assert_eq!(lease.payload().unwrap().bytes(), &[0xFF, 0, 0x80]);
    }

    #[test]
    fn empty_payload_is_present_and_mismatch_is_rejected() {
        let admission =
            HeaderAdmission::for_wire::<UProtocolNativeWire>(HeaderLimits::default()).unwrap();
        let empty = admission
            .admit(raw(Some(0), Some(&[]), 0x8001, false))
            .unwrap();
        assert!(empty.payload().unwrap().bytes().is_empty());
        let absent = admission.admit(raw(None, None, 0x8001, false)).unwrap();
        assert!(absent.payload().is_none());
        assert!(matches!(
            admission.admit(raw(Some(0), None, 0x8001, false)),
            Err(AdmissionError::PayloadPresence)
        ));
        assert_eq!(admission.stats().admitted.load(Ordering::Relaxed), 2);
        assert_eq!(admission.stats().rejected.load(Ordering::Relaxed), 1);
    }

    struct OtherWire;
    impl UWire for OtherWire {
        const WIRE_ID: WireIdentity = WireIdentity::new("private.other-wire", 0xF100);
        const PAYLOAD_FAMILY_ID: WireIdentity = UProtocolNativeWire::PAYLOAD_FAMILY_ID;
        const FORMAT_VERSION: u16 = 1;
    }

    #[tokio::test]
    async fn forward_preserves_reserved_ids_and_rejects_origination_and_relabeling() {
        let core = Arc::new(
            PoolCore::new(PoolCoreConfig {
                slots: 2,
                slot_bytes: 1024,
                alignment: 8,
                max_routes: 2,
                max_registrations: 1,
            })
            .unwrap(),
        );
        let admission =
            HeaderAdmission::for_wire::<UProtocolNativeWire>(HeaderLimits::default()).unwrap();
        let original = admission
            .admit(raw(Some(0xE000), Some(b"opaque"), 0x8001, false))
            .unwrap();
        let limits = RouteLimits {
            max_metadata_len: 512,
            max_payload_len: 16,
            payload_alignment: 8,
        };
        let route = OpaqueTemplate::for_wire::<UProtocolNativeWire>()
            .unwrap()
            .route(
                Arc::clone(&core),
                source(0x8001),
                None,
                limits,
                RouteConfig::default(),
            )
            .unwrap();
        let other = OpaqueTemplate::for_wire::<OtherWire>()
            .unwrap()
            .route(
                Arc::clone(&core),
                source(0x8001),
                None,
                limits,
                RouteConfig::default(),
            )
            .unwrap();
        let inbox = Arc::new(
            RegistrationInbox::new(RegistrationConfig {
                capacity: 1,
                overflow: OverflowPolicy::DropNewest,
            })
            .unwrap(),
        );
        let mut subscription = core
            .attach(&source(0x8001), None, inbox.clone())
            .await
            .unwrap();
        assert!(matches!(
            route.send_bytes(
                TxHeaderParts::from(&original.header().unwrap()),
                Some(b"opaque")
            ),
            Err(SendError::Header(UFrameHeaderError::Invalid(_)))
        ));
        assert_eq!(
            other.forward(&original),
            Err(SendError::Header(UFrameHeaderError::WrongPrefix))
        );
        route.forward(&original).unwrap();
        let received = admission.admit(inbox.recv().await.unwrap()).unwrap();
        assert_eq!(
            received.header().unwrap().payload_encoding,
            original.header().unwrap().payload_encoding
        );
        assert_eq!(
            received.header().unwrap().native_type_token,
            original.header().unwrap().native_type_token
        );
        assert_eq!(received.header().unwrap().id, original.header().unwrap().id);
        assert_eq!(
            received.payload().unwrap().bytes(),
            original.payload().unwrap().bytes()
        );
        assert_ne!(
            received.payload().unwrap().bytes().as_ptr(),
            original.payload().unwrap().bytes().as_ptr()
        );
        core.detach(&mut subscription).unwrap();
        drop((core, route, other, admission));
        assert_eq!(received.payload().unwrap().bytes(), b"opaque");
    }

    struct Count(Arc<AtomicU64>);
    impl HeaderHandler<Raw> for Count {
        async fn on_receive(&mut self, _: HeaderLease<Raw>) {
            tokio::task::yield_now().await;
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
    #[tokio::test]
    async fn filtered_worker_rechecks_decoded_fields_and_sinkless_semantics() {
        let admission =
            HeaderAdmission::for_wire::<UProtocolNativeWire>(HeaderLimits::default()).unwrap();
        let count = Arc::new(AtomicU64::new(0));
        let worker = admission
            .register_filtered(
                source(0x8001),
                None,
                RegistrationConfig {
                    capacity: 4,
                    overflow: OverflowPolicy::DropNewest,
                },
                Count(Arc::clone(&count)),
                || {},
            )
            .unwrap();
        worker
            .inbox()
            .push(raw(Some(8), Some(b"keep"), 0x8001, false))
            .unwrap();
        worker
            .inbox()
            .push(raw(Some(8), Some(b"other topic"), 0x8002, false))
            .unwrap();
        worker
            .inbox()
            .push(raw(Some(8), Some(b"notification"), 0x8001, true))
            .unwrap();
        let mut malformed = raw(Some(8), Some(b"bad header"), 0x8001, false);
        *malformed.metadata.first_mut().unwrap() ^= 1;
        worker.inbox().push(malformed).unwrap();
        worker.stop().await.unwrap();
        assert_eq!(count.load(Ordering::Relaxed), 1);
        assert_eq!(admission.stats().admitted.load(Ordering::Relaxed), 3);
        assert_eq!(admission.stats().filtered.load(Ordering::Relaxed), 2);
        assert_eq!(admission.stats().rejected.load(Ordering::Relaxed), 1);
    }
}
