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
//! Prepared stable-native transmit routes and checked in-place payload filling.

use std::{marker::PhantomData, mem, sync::Arc, time::Duration};

use crate::wire::{UFrameFieldsMetadataCodec, UWireMetadataContext};
use crate::{
    header_len, write_header_uninit, FrameMessageKind, FramePriority, InitializedStablePayload,
    InitializedTx, NativePayloadIdentity, NativeProfileAgreement, PreparedTransportCore,
    PreparedTxStorage, RouteLimits, SendError, SetupError, StableContainerPayload,
    StableContainerWire, StablePayload, StablePayloadInit, TxFrameLayout, TxHeaderParts, UCode,
    UFrameHeaderError, UUri, HEADER_TRACEPARENT_CAPACITY, UUID,
};

/// Per-route capacities reserved before any frame is sent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteConfig {
    /// Maximum token length; zero disables token presence on this route.
    pub max_token_len: usize,
    /// Whether the route accepts traceparent fields, up to the profile cap.
    pub allow_traceparent: bool,
}

impl Default for RouteConfig {
    fn default() -> Self {
        Self {
            max_token_len: 2048,
            allow_traceparent: false,
        }
    }
}

/// Fixed semantic addressing and defaults for one prepared route.
#[derive(Clone, Debug)]
pub struct RouteDefinition {
    /// Message kind.
    pub kind: FrameMessageKind,
    /// Source address.
    pub source: UUri,
    /// Optional sink address.
    pub sink: Option<UUri>,
    /// Default priority; RPC routes require CS4 or higher.
    pub priority: Option<FramePriority>,
    /// Fixed time to live; requests require a positive value.
    pub ttl: Option<Duration>,
    /// Fixed permission level.
    pub permission_level: Option<u32>,
}

/// Borrowed per-message overrides; URI and capacity choices remain at setup.
#[derive(Clone, Copy, Default)]
pub struct MessageFields<'a> {
    /// Explicit message ID, or None to generate a fresh UUIDv7 on this thread.
    pub id: Option<&'a UUID>,
    /// Correlated request ID, required for responses.
    pub reqid: Option<&'a UUID>,
    /// Optional priority override.
    pub priority: Option<FramePriority>,
    /// Optional communication status.
    pub comm_status: Option<UCode>,
    /// Optional token borrowed only for the synchronous send.
    pub token: Option<&'a str>,
    /// Optional traceparent.
    pub traceparent: Option<&'a str>,
}

impl std::fmt::Debug for MessageFields<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageFields")
            .field("id", &self.id)
            .field("reqid", &self.reqid)
            .field("token_len", &self.token.map(str::len))
            .finish_non_exhaustive()
    }
}

struct Template<P> {
    prefix: Box<[u8]>,
    identity: NativePayloadIdentity,
    agreement: NativeProfileAgreement,
    proof: PhantomData<fn() -> P>,
}

/// Setup-prepared identity and prefix reusable by preallocated dynamic reply routes.
pub struct TxTemplate<P> {
    inner: Arc<Template<P>>,
}

impl<P> Clone for TxTemplate<P> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<P> std::fmt::Debug for TxTemplate<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TxTemplate")
            .field("identity", &self.inner.identity)
            .finish_non_exhaustive()
    }
}

impl<P: StablePayload> TxTemplate<P> {
    /// Resolves the representation, identity and native prefix once at setup.
    ///
    /// # Errors
    /// Rejects an invalid/unagreed payload type or unsupported prefix.
    pub fn new(agreement: NativeProfileAgreement) -> Result<Self, SetupError> {
        StableContainerPayload::<P>::representation_proven()
            .map_err(|_| SetupError::UnknownPayloadType)?;
        let identity = StableContainerPayload::<P>::identity(&agreement)
            .map_err(|_| SetupError::UnknownPayloadType)?;
        let prefix = UFrameFieldsMetadataCodec
            .native_prefix(UWireMetadataContext::from_wire::<
                StableContainerWire,
                UFrameFieldsMetadataCodec,
            >())
            .map_err(|_| SetupError::Prefix)?;
        Ok(Self {
            inner: Arc::new(Template {
                prefix: prefix.into_boxed_slice(),
                identity,
                agreement,
                proof: PhantomData,
            }),
        })
    }

    /// Fills a reserved core route without building another descriptor or prefix.
    ///
    /// This is also used by bounded RPC caches on first client contact: inline
    /// URIs and already-reserved core table entries avoid a late allocation.
    ///
    /// # Errors
    /// Rejects invalid route metadata, capacities or an exhausted route table.
    pub fn route<C: PreparedTransportCore>(
        &self,
        core: Arc<C>,
        definition: RouteDefinition,
        config: RouteConfig,
    ) -> Result<TxRoute<C, P>, SetupError> {
        if u16::try_from(config.max_token_len).is_err() {
            return Err(SetupError::InvalidConfig("token capacity"));
        }
        let id =
            UUID::from_u64_pair(0x7000, 0x8000_0000_0000_0000).expect("fixed valid validation ID");
        let parts = TxHeaderParts {
            kind: definition.kind,
            priority: definition.priority,
            id: &id,
            reqid: (definition.kind == FrameMessageKind::Response).then_some(&id),
            ttl: definition.ttl,
            comm_status: None,
            permission_level: definition.permission_level,
            source: &definition.source,
            sink: definition.sink.as_ref(),
            token: None,
            traceparent: None,
            payload_encoding: Some(self.inner.identity.encoding()),
            native_type_token: Some(self.inner.identity.token()),
        };
        crate::frame::header::validate_parts(&parts)
            .map_err(|_| SetupError::InvalidRoute("metadata invariants"))?;
        let mut maximum = header_len(&self.inner.prefix, &parts)
            .map_err(|_| SetupError::InvalidRoute("header length"))?;
        let mut add = |amount: usize| -> Result<(), SetupError> {
            maximum = maximum
                .checked_add(amount)
                .ok_or(SetupError::InvalidConfig("header capacity overflow"))?;
            Ok(())
        };
        if definition.kind != FrameMessageKind::Response {
            add(16)?;
        }
        add(4)?; // A per-message communication status may be present.
        if config.max_token_len != 0 {
            add(2 + config.max_token_len)?;
        }
        if config.allow_traceparent {
            add(1 + HEADER_TRACEPARENT_CAPACITY)?;
        }
        let limits = RouteLimits {
            max_metadata_len: maximum,
            max_payload_len: mem::size_of::<P>(),
            payload_alignment: mem::align_of::<P>(),
        };
        let route = core.prepare_route(&definition.source, definition.sink.as_ref(), limits)?;
        Ok(TxRoute {
            core,
            route,
            template: self.clone(),
            definition,
            config,
        })
    }
}

/// A concrete derive initializer tied to one selected transmit loan.
pub struct PayloadInit<'a, P: StablePayloadInit> {
    initializer: P::Initializer<'a>,
}

impl<P: StablePayloadInit> std::fmt::Debug for PayloadInit<'_, P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PayloadInit").finish_non_exhaustive()
    }
}

impl<'a, P: StablePayloadInit> PayloadInit<'a, P> {
    /// Returns the derive-generated write-once initializer for this storage.
    #[must_use]
    pub fn into_initializer(self) -> P::Initializer<'a> {
        self.initializer
    }
}

/// One fixed route over a physical core, with all variable capacities prepared.
pub struct TxRoute<C: PreparedTransportCore, P> {
    core: Arc<C>,
    route: C::Route,
    template: TxTemplate<P>,
    definition: RouteDefinition,
    config: RouteConfig,
}

impl<C: PreparedTransportCore, P> std::fmt::Debug for TxRoute<C, P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TxRoute")
            .field("definition", &self.definition)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<C: PreparedTransportCore, P: StablePayloadInit> TxRoute<C, P> {
    fn parts<'a>(
        &'a self,
        fields: MessageFields<'a>,
        id: &'a UUID,
        payload: bool,
    ) -> Result<TxHeaderParts<'a>, SendError> {
        if let Some(token) = fields.token {
            if self.config.max_token_len == 0 {
                return Err(SendError::TokenNotAllowed);
            }
            if token.len() > self.config.max_token_len {
                return Err(SendError::TokenTooLong);
            }
        }
        if fields.traceparent.is_some_and(|value| {
            !self.config.allow_traceparent || value.len() > HEADER_TRACEPARENT_CAPACITY
        }) {
            return Err(SendError::TraceparentRejected);
        }
        let priority = fields.priority.or(self.definition.priority);
        if matches!(
            self.definition.kind,
            FrameMessageKind::Request | FrameMessageKind::Response
        ) && priority.is_none_or(|value| value < FramePriority::CS4)
        {
            return Err(SendError::PriorityRejected);
        }
        if self.definition.kind == FrameMessageKind::Response && fields.reqid.is_none() {
            return Err(SendError::MissingRequestId);
        }
        Ok(TxHeaderParts {
            kind: self.definition.kind,
            priority,
            id,
            reqid: fields.reqid,
            ttl: self.definition.ttl,
            comm_status: fields.comm_status,
            permission_level: self.definition.permission_level,
            source: &self.definition.source,
            sink: self.definition.sink.as_ref(),
            token: fields.token,
            traceparent: fields.traceparent,
            payload_encoding: payload.then_some(self.template.inner.identity.encoding()),
            native_type_token: payload.then_some(self.template.inner.identity.token()),
        })
    }

    /// Initializes and publishes one stable payload directly in reserved storage.
    ///
    /// All URI/header construction, encoding, payload validation and core publish
    /// occur in this call. Application initializer code must honor its own
    /// allocation policy. No transmit loan is retained across an await.
    ///
    /// # Errors
    /// Returns fixed-size per-message errors, including rejected overrides,
    /// exhausted storage, wrong initialization witness and invalid field bits.
    pub fn send<F>(&self, fields: MessageFields<'_>, fill: F) -> Result<(), SendError>
    where
        F: for<'a> FnOnce(PayloadInit<'a, P>) -> InitializedStablePayload<'a, P>,
    {
        let frame = self.prepare(fields, fill)?;
        self.publish(frame)
    }

    /// Initializes a checked loan so borrowed input guards can be released before
    /// the physical publish operation. The returned loan still borrows this route.
    ///
    /// # Errors
    /// Returns the same validation, storage and witness errors as send.
    pub fn prepare<'a, F>(
        &'a self,
        fields: MessageFields<'_>,
        fill: F,
    ) -> Result<InitializedTx<C::Tx<'a>>, SendError>
    where
        F: for<'b> FnOnce(PayloadInit<'b, P>) -> InitializedStablePayload<'b, P>,
    {
        let generated;
        let id = if let Some(id) = fields.id {
            id
        } else {
            generated = UUID::build();
            &generated
        };
        let parts = self.parts(fields, id, true)?;
        self.prepare_parts(parts, fill)
    }

    /// Writes caller-built semantic metadata into this route's storage and sends it.
    ///
    /// Addressing, kind, TTL, permission and complete payload identity must match
    /// this prepared route. Per-message strings are borrowed from metadata; they
    /// are not cloned. The caller controls any allocation used to build metadata.
    ///
    /// # Errors
    /// Rejects mismatched fixed fields/identity, invalid overrides, storage or
    /// initialization failures, and physical delivery failures.
    pub fn send_metadata<F>(
        &self,
        metadata: &crate::UFrameMetadata,
        fill: F,
    ) -> Result<(), SendError>
    where
        F: for<'a> FnOnce(PayloadInit<'a, P>) -> InitializedStablePayload<'a, P>,
    {
        let frame = self.prepare_metadata(metadata, fill)?;
        self.publish(frame)
    }

    /// Prepares a complete typed loan from caller-built metadata without publishing.
    ///
    /// # Errors
    /// Uses the same fixed-field, identity, capacity and witness checks as
    /// [`Self::send_metadata`]. Rejection occurs before the filler is invoked.
    pub fn prepare_metadata<'a, F>(
        &'a self,
        metadata: &crate::UFrameMetadata,
        fill: F,
    ) -> Result<InitializedTx<C::Tx<'a>>, SendError>
    where
        F: for<'b> FnOnce(PayloadInit<'b, P>) -> InitializedStablePayload<'b, P>,
    {
        if metadata.kind() != self.definition.kind
            || metadata.source() != &self.definition.source
            || metadata.sink() != self.definition.sink.as_ref()
            || metadata.ttl() != self.definition.ttl
            || metadata.permission_level() != self.definition.permission_level
        {
            return Err(SendError::Header(UFrameHeaderError::Invalid(
                "metadata does not match prepared route",
            )));
        }
        if metadata.payload_encoding().copied() != Some(self.template.inner.identity.encoding())
            || metadata.native_type_token() != Some(self.template.inner.identity.token())
        {
            return Err(SendError::Header(UFrameHeaderError::Invalid(
                "metadata does not match prepared payload identity",
            )));
        }
        self.parts(
            MessageFields {
                id: Some(metadata.id()),
                reqid: metadata.reqid(),
                priority: metadata.priority(),
                token: metadata.token(),
                traceparent: metadata.traceparent(),
                comm_status: metadata.comm_status(),
            },
            metadata.id(),
            true,
        )?;
        self.prepare_parts(TxHeaderParts::from(metadata), fill)
    }

    fn prepare_parts<'a, F>(
        &'a self,
        parts: TxHeaderParts<'_>,
        fill: F,
    ) -> Result<InitializedTx<C::Tx<'a>>, SendError>
    where
        F: for<'b> FnOnce(PayloadInit<'b, P>) -> InitializedStablePayload<'b, P>,
    {
        let metadata_len =
            header_len(&self.template.inner.prefix, &parts).map_err(SendError::Header)?;
        let layout = TxFrameLayout {
            metadata_len,
            payload_len: Some(mem::size_of::<P>()),
            payload_alignment: mem::align_of::<P>(),
        };
        let mut storage = self
            .core
            .loan(&self.route, layout)
            .map_err(SendError::Loan)?;
        if storage.metadata_uninit_mut().len() != metadata_len {
            return Err(SendError::PayloadStorage);
        }
        let written = write_header_uninit(
            &self.template.inner.prefix,
            &parts,
            storage.metadata_uninit_mut(),
        )
        .map_err(SendError::Header)?;
        storage.record_header(written)?;
        {
            let bytes = storage
                .payload_uninit_mut()
                .ok_or(SendError::PayloadStorage)?;
            if bytes.len() != mem::size_of::<P>()
                || !(bytes.as_ptr() as usize).is_multiple_of(mem::align_of::<P>())
            {
                return Err(SendError::PayloadStorage);
            }
            let address = bytes.as_ptr().cast::<u8>();
            let initializer = P::init(bytes).map_err(|_| SendError::PayloadStorage)?;
            let witness = fill(PayloadInit { initializer });
            let initialized = witness.as_bytes();
            if initialized.len() != mem::size_of::<P>() || initialized.as_ptr() != address {
                return Err(SendError::WitnessMismatch);
            }
            if !P::FIELD_BITS_ALWAYS_VALID && !P::validate_field_bytes(initialized) {
                return Err(SendError::InvalidFieldBits);
            }
        }
        Ok(InitializedTx { storage })
    }

    /// Publishes a loan prepared for this exact core route.
    ///
    /// # Errors
    /// Returns a fixed-size route or physical delivery failure.
    pub fn publish<'a>(&'a self, frame: InitializedTx<C::Tx<'a>>) -> Result<(), SendError> {
        self.core.publish(&self.route, frame)
    }

    /// Publishes a correlated status-only response without an application payload.
    ///
    /// # Errors
    /// Rejects non-response routes, missing request IDs, invalid overrides and
    /// exhausted storage. Payload absence is checked against the core loan.
    pub fn send_status(
        &self,
        mut fields: MessageFields<'_>,
        status: UCode,
    ) -> Result<(), SendError> {
        if self.definition.kind != FrameMessageKind::Response {
            return Err(SendError::Header(UFrameHeaderError::Invalid(
                "status-only route kind",
            )));
        }
        fields.comm_status = Some(status);
        let generated;
        let id = if let Some(id) = fields.id {
            id
        } else {
            generated = UUID::build();
            &generated
        };
        let parts = self.parts(fields, id, false)?;
        let metadata_len =
            header_len(&self.template.inner.prefix, &parts).map_err(SendError::Header)?;
        let mut storage = self
            .core
            .loan(
                &self.route,
                TxFrameLayout {
                    metadata_len,
                    payload_len: None,
                    payload_alignment: 1,
                },
            )
            .map_err(SendError::Loan)?;
        if storage.metadata_uninit_mut().len() != metadata_len
            || storage.payload_uninit_mut().is_some()
        {
            return Err(SendError::PayloadStorage);
        }
        let written = write_header_uninit(
            &self.template.inner.prefix,
            &parts,
            storage.metadata_uninit_mut(),
        )
        .map_err(SendError::Header)?;
        storage.record_header(written)?;
        self.core.publish(&self.route, InitializedTx { storage })
    }

    /// Immutable agreement retained for this prepared route.
    #[must_use]
    pub fn native_agreement(&self) -> &NativeProfileAgreement {
        &self.template.inner.agreement
    }

    /// Fixed metadata definition for this route.
    #[must_use]
    pub fn definition(&self) -> &RouteDefinition {
        &self.definition
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Frame, HeaderLimits, NativeProfile, NativeProfileMode, NativeProfileTable, OverflowPolicy,
        PayloadEncoding, PoolCore, PoolCoreConfig, RegistrationConfig, RegistrationInbox,
        RxAdmission,
    };

    #[repr(C)]
    #[derive(Debug, Eq, PartialEq, crate::StablePayload, crate::StablePayloadInit)]
    #[stable_payload(type_name = "uprotocol.test.PreparedRoute")]
    struct Value {
        value: u32,
    }

    fn agreement() -> NativeProfileAgreement {
        let table = NativeProfileTable::new([(
            PayloadEncoding::from_id(0xF111).unwrap(),
            Value::native_representation(),
        )])
        .unwrap();
        crate::test_support::self_agreement(
            NativeProfile::new("prepared-route", 1, NativeProfileMode::Table(table)).unwrap(),
        )
    }

    fn core() -> Arc<PoolCore> {
        Arc::new(
            PoolCore::new(PoolCoreConfig {
                slots: 4,
                slot_bytes: 4096,
                alignment: 8,
                max_routes: 4,
                max_registrations: 4,
            })
            .unwrap(),
        )
    }

    fn definition(kind: FrameMessageKind) -> RouteDefinition {
        let source = UUri::try_from_wire_parts(
            "sender",
            1,
            1,
            if kind == FrameMessageKind::Response {
                1
            } else {
                0x8001
            },
        )
        .unwrap();
        let sink = (kind == FrameMessageKind::Response)
            .then(|| UUri::try_from_wire_parts("client", 2, 1, 0).unwrap());
        RouteDefinition {
            kind,
            source,
            sink,
            priority: (kind == FrameMessageKind::Response).then_some(FramePriority::CS4),
            ttl: None,
            permission_level: None,
        }
    }

    #[tokio::test]
    async fn prepared_send_preserves_typed_value_and_default_token_capacity() {
        let core = core();
        let agreement = agreement();
        let definition = definition(FrameMessageKind::Publish);
        let receiver =
            RxAdmission::<Value>::new(agreement.clone(), HeaderLimits::default()).unwrap();
        let template = TxTemplate::<Value>::new(agreement).unwrap();
        let route = template
            .route(
                Arc::clone(&core),
                definition.clone(),
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
            .attach(&definition.source, None, inbox.clone())
            .await
            .unwrap();
        let token = "x".repeat(2048);
        route
            .send(
                MessageFields {
                    token: Some(&token),
                    ..MessageFields::default()
                },
                |payload| payload.into_initializer().value(42).finish(),
            )
            .unwrap();
        let frame = inbox.recv().await.unwrap();
        let lease = receiver.admit(frame).unwrap();
        assert_eq!(lease.payload().unwrap().value, 42);
        assert_eq!(lease.header().unwrap().token(), Some(token.as_str()));
        core.detach(&mut subscription).unwrap();
        assert_eq!(lease.payload().unwrap().value, 42);
        drop(lease);
        assert_eq!(core.free_slots(), 4);
    }

    #[test]
    fn rejected_overrides_do_not_call_the_initializer_or_hold_a_slot() {
        let core = core();
        let template = TxTemplate::<Value>::new(agreement()).unwrap();
        let route = template
            .route(
                Arc::clone(&core),
                definition(FrameMessageKind::Publish),
                RouteConfig {
                    max_token_len: 4,
                    allow_traceparent: false,
                },
            )
            .unwrap();
        assert_eq!(
            route.send(
                MessageFields {
                    token: Some("too-long"),
                    ..MessageFields::default()
                },
                |_| panic!("initializer must not run")
            ),
            Err(SendError::TokenTooLong)
        );
        assert_eq!(
            route.send(
                MessageFields {
                    traceparent: Some("trace"),
                    ..MessageFields::default()
                },
                |_| panic!("initializer must not run")
            ),
            Err(SendError::TraceparentRejected)
        );
        assert_eq!(core.free_slots(), 4);
        let response = template
            .route(
                Arc::clone(&core),
                definition(FrameMessageKind::Response),
                RouteConfig::default(),
            )
            .unwrap();
        assert_eq!(
            response.send(MessageFields::default(), |_| panic!(
                "initializer must not run"
            )),
            Err(SendError::MissingRequestId)
        );
    }

    #[tokio::test]
    async fn caller_metadata_preserves_bytes_and_rejects_identity_before_initialization() {
        use crate::wire::{UWireMetadataCodec, UWireMetadataContext};
        let core = core();
        let agreement = agreement();
        let definition = definition(FrameMessageKind::Publish);
        let admission =
            RxAdmission::<Value>::new(agreement.clone(), HeaderLimits::default()).unwrap();
        let route = TxTemplate::<Value>::new(agreement)
            .unwrap()
            .route(
                Arc::clone(&core),
                definition.clone(),
                RouteConfig::default(),
            )
            .unwrap();
        let metadata = crate::UFrameMetadata::publish(definition.source.clone())
            .with_native_payload_identity(admission.identity())
            .with_token("borrowed credential")
            .with_priority(FramePriority::CS2)
            .build()
            .unwrap();
        let expected = UFrameFieldsMetadataCodec
            .encode_frame_metadata(
                UWireMetadataContext::from_wire::<StableContainerWire, UFrameFieldsMetadataCodec>(),
                &metadata,
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
            .attach(&definition.source, None, inbox.clone())
            .await
            .unwrap();
        route
            .send_metadata(&metadata, |init| init.into_initializer().value(42).finish())
            .unwrap();
        let raw = inbox.recv().await.unwrap();
        assert_eq!(raw.encoded_metadata(), expected);
        assert_eq!(admission.admit(raw).unwrap().payload().unwrap().value, 42);
        let encoding = metadata.payload_encoding().copied().unwrap();
        let missing = metadata
            .clone()
            .without_payload_encoding()
            .with_payload_encoding(encoding)
            .unwrap();
        let wrong = metadata
            .clone()
            .with_native_type_token(crate::NativeTypeToken::from_u32(
                admission.identity().token().as_u32() ^ 1,
            ))
            .unwrap();
        let other_route = crate::UFrameMetadata::publish(
            UUri::try_from_wire_parts("other", 1, 1, 0x8001).unwrap(),
        )
        .with_native_payload_identity(admission.identity())
        .build()
        .unwrap();
        for rejected in [missing, wrong, other_route] {
            assert!(matches!(
                route.send_metadata(&rejected, |_| panic!(
                    "mismatch must reject before initialization"
                )),
                Err(SendError::Header(UFrameHeaderError::Invalid(_)))
            ));
            assert_eq!(core.free_slots(), 4);
        }
        core.detach(&mut subscription).unwrap();
    }

    #[tokio::test]
    async fn status_only_response_preserves_correlation_and_absence() {
        let core = core();
        let definition = definition(FrameMessageKind::Response);
        let template = TxTemplate::<Value>::new(agreement()).unwrap();
        let route = template
            .route(
                Arc::clone(&core),
                definition.clone(),
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
        let _subscription = core
            .attach(&definition.source, definition.sink.as_ref(), inbox.clone())
            .await
            .unwrap();
        let request = UUID::build();
        route
            .send_status(
                MessageFields {
                    reqid: Some(&request),
                    ..MessageFields::default()
                },
                UCode::NotFound,
            )
            .unwrap();
        let frame = inbox.recv().await.unwrap();
        let header = crate::parse_header(
            &template.inner.prefix,
            frame.encoded_metadata(),
            &HeaderLimits::default(),
        )
        .unwrap();
        assert_eq!(header.reqid.as_ref(), Some(&request));
        assert_eq!(header.comm_status, Some(UCode::NotFound));
        assert_eq!(header.payload_encoding, None);
        assert_eq!(header.native_type_token, None);
        assert!(frame.payload().is_none());
    }
}
