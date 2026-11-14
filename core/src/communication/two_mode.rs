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

//! L2 roles in runtime mode: `Publisher`, `Notifier`, `Subscriber`,
//! `NotificationListener`, `RpcClient`, `RpcServer`.
//!
//! Each role is built during initialization on a [`TransportSetup`], producing a
//! setup value with no way to send, and is then activated on a
//! [`TransportRuntime`]. Stored handles name only domain types; they hold
//! `&'static` data and indices, are `Copy`, and dropping one does nothing
//! (R28, R38, R39). Defaults are named once, in the design's table (R29): QoS
//! is [`Qos::DEFAULT`]; send roles get 4 slots; receive roles hold up to 4
//! messages; RPC clients allow 4 calls in flight with a 1 s TTL, and responses as
//! large as the wire's bound, or 1024 bytes when the wire gives none. A wire with
//! no size bound needs `.max_payload(n)`.

#![allow(clippy::result_large_err)]

use crate::transport::inbound::InboundDispatch;
use core::any::TypeId;
use core::fmt;
use core::marker::PhantomData;
use core::mem::{self, MaybeUninit};
use core::time::Duration;

use super::LocalUriProvider;
use crate::frame::header::{HeaderLimits, HEADER_TRACEPARENT_CAPACITY};
use crate::memory::{FixedVec, StartupMemory};
use crate::payload::loan::BorrowError;
use crate::payload::stable::{InitializedStablePayload, StablePayload, StablePayloadInit};
use crate::transport::binding::{RouteKind, StaticHeader};
use crate::transport::envelope::AdmittedWire;
use crate::transport::inbound::{ttl_millis, PeerTable, RxCommon, RxKind, TypedInbound, TypedWire};
use crate::transport::mode::{
    deadline_after, route_kind, route_name, EncodedFrame, Family, InPlaceFrame, MessageFields,
    TransportShared, TxRoute, ValueFrame,
};
use crate::transport::qos::check_qos;
use crate::transport::report::RouteId;
use crate::transport::rpc::{Call, CallTable, ClientResponses};
use crate::transport::{
    CallError, Envelope, PayloadBytes, PreparedRoute, Qos, ReceiveError, Received, ReplyToken,
    RouteCounters, RouteDeclaration, RouteDirection, RouteName, SendError, SendReport,
    StartupError, TransportRuntime, TransportSetup, TransportSource,
};
use crate::uattributes::{PayloadEncoding, UMessageType, UPriority};
use crate::ucode::UCode;
use crate::uri::UUri;
use crate::uuid::UUID;
use crate::wire::{
    Encoded, InPlace, RoleWire, RoleWireKind, RpcRoleWire, RpcWireKind, RuntimeHeap, Wire,
    WireInfo, WireMode, WirePayloadIdentity,
};

pub use crate::transport::{ReceiveError as TwoModeReceiveError, Reply};

const MAX_ALSO_WIRES: usize = 7;

/// Peers a route tracks unless told otherwise: the design's table (R29).
const DEFAULT_MAX_PEERS: usize = 8;

/// A typed publish-subscribe topic of this entity (R11, R13). Optional: a plain
/// resource ID works everywhere a topic does.
#[derive(Debug)]
pub struct PublishTopic<T> {
    resource: u16,
    _marker: PhantomData<fn() -> T>,
}

impl<T> PublishTopic<T> {
    /// Names a topic by its resource ID.
    #[must_use]
    pub const fn new(resource: u16) -> Self {
        Self {
            resource,
            _marker: PhantomData,
        }
    }
}

impl<T> Clone for PublishTopic<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for PublishTopic<T> {}

/// A typed notification topic of this entity (R11, R13).
#[derive(Debug)]
pub struct NotificationTopic<T> {
    resource: u16,
    _marker: PhantomData<fn() -> T>,
}

impl<T> NotificationTopic<T> {
    /// Names a topic by its resource ID.
    #[must_use]
    pub const fn new(resource: u16) -> Self {
        Self {
            resource,
            _marker: PhantomData,
        }
    }
}

impl<T> Clone for NotificationTopic<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for NotificationTopic<T> {}

/// A typed RPC method of this entity (R11, R13).
#[derive(Debug)]
pub struct Method<Req, Resp> {
    resource: u16,
    _marker: PhantomData<fn() -> (Req, Resp)>,
}

impl<Req, Resp> Method<Req, Resp> {
    /// Names a method by its resource ID.
    #[must_use]
    pub const fn new(resource: u16) -> Self {
        Self {
            resource,
            _marker: PhantomData,
        }
    }
}

impl<Req, Resp> Clone for Method<Req, Resp> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Req, Resp> Copy for Method<Req, Resp> {}

/// Anything that names this entity's publish topic resource.
pub trait PublishResource<T> {
    /// The resource ID.
    fn resource_id(self) -> u16;
}
impl<T> PublishResource<T> for u16 {
    fn resource_id(self) -> u16 {
        self
    }
}
impl<T> PublishResource<T> for PublishTopic<T> {
    fn resource_id(self) -> u16 {
        self.resource
    }
}

/// Anything that names this entity's notification resource.
pub trait NotificationResource<T> {
    /// The resource ID.
    fn resource_id(self) -> u16;
}
impl<T> NotificationResource<T> for u16 {
    fn resource_id(self) -> u16 {
        self
    }
}
impl<T> NotificationResource<T> for NotificationTopic<T> {
    fn resource_id(self) -> u16 {
        self.resource
    }
}

/// Anything that names this entity's RPC method resource.
pub trait MethodResource<Req, Resp> {
    /// The resource ID.
    fn resource_id(self) -> u16;
}
impl<Req, Resp> MethodResource<Req, Resp> for u16 {
    fn resource_id(self) -> u16 {
        self
    }
}
impl<Req, Resp> MethodResource<Req, Resp> for Method<Req, Resp> {
    fn resource_id(self) -> u16 {
        self.resource
    }
}

/// The reservations and QoS for one route. Sized by knob, refused when exceeded (R29).
#[derive(Clone, Copy, Debug)]
struct RouteOptions {
    slots: usize,
    max_payload: Option<usize>,
    max_token: usize,
    allow_traceparent: bool,
    max_peers: usize,
    /// Whether `.max_peers(n)` was called, as opposed to the default.
    max_peers_set: bool,
    max_held: usize,
    priority: Option<UPriority>,
    ttl: Option<Duration>,
    envelope: Option<Envelope>,
    qos: Qos,
    require_lending: bool,
}

impl Default for RouteOptions {
    fn default() -> Self {
        Self {
            slots: 4,
            max_payload: None,
            max_token: 0,
            allow_traceparent: false,
            max_peers: DEFAULT_MAX_PEERS,
            max_peers_set: false,
            max_held: 4,
            priority: None,
            ttl: None,
            envelope: None,
            qos: Qos::DEFAULT,
            require_lending: false,
        }
    }
}

macro_rules! route_options {
    () => {
        /// Sets this route's QoS. Refused at setup if the transport cannot honor it.
        /// The default is [`Qos::DEFAULT`].
        #[must_use]
        pub fn qos(mut self, qos: Qos) -> Self {
            self.options.qos = qos;
            self
        }

        /// Sets the largest payload in bytes. By default it comes from the
        /// wire; if the wire gives no bound, this setter is required.
        #[must_use]
        pub fn max_payload(mut self, bytes: usize) -> Self {
            self.options.max_payload = Some(bytes);
            self
        }

        /// Sets the longest access token on this route. The default of 0 refuses tokens.
        #[must_use]
        pub fn max_token_len(mut self, bytes: usize) -> Self {
            self.options.max_token = bytes;
            self
        }

        /// Allows a traceparent. The default refuses it.
        #[must_use]
        pub fn allow_traceparent(mut self, allow: bool) -> Self {
            self.options.allow_traceparent = allow;
            self
        }

        /// Overrides the transport's default envelope, if the transport carries it.
        #[must_use]
        pub fn envelope(mut self, envelope: Envelope) -> Self {
            self.options.envelope = Some(envelope);
            self
        }
    };
}

macro_rules! send_options {
    () => {
        /// Sets the number of startup slots. The default is 4.
        #[must_use]
        pub fn slots(mut self, slots: usize) -> Self {
            self.options.slots = slots;
            self
        }

        /// Sets this route's priority. The default is carried-only.
        #[must_use]
        pub fn priority(mut self, priority: UPriority) -> Self {
            self.options.priority = Some(priority);
            self
        }

        /// Refuses this route at setup unless the transport lends storage (R16).
        ///
        /// By default a route takes whichever the transport offers, and
        /// [`RouteReport::send_mode`](crate::transport::RouteReport::send_mode)
        /// says which it got.
        #[must_use]
        pub fn require_lending(mut self) -> Self {
            self.options.require_lending = true;
            self
        }
    };
}

macro_rules! receive_options {
    () => {
        /// Sets how many received messages the application may hold at once.
        /// The default is 4.
        #[must_use]
        pub fn max_held(mut self, held: usize) -> Self {
            self.options.max_held = held;
            self
        }

        /// Sets the most peers this route can track. The default is 8.
        #[must_use]
        pub fn max_peers(mut self, peers: usize) -> Self {
            self.options.max_peers = peers;
            self
        }
    };
}

/// The wires of one route, erased so far as the role can be.
struct WireSet<T: 'static> {
    infos: [Option<WireInfo>; MAX_ALSO_WIRES + 1],
    typed: [Option<TypedWire<T>>; MAX_ALSO_WIRES + 1],
    count: usize,
    error: Option<&'static str>,
}

impl<T: 'static> WireSet<T> {
    fn new() -> Self {
        Self {
            infos: [None; MAX_ALSO_WIRES + 1],
            typed: [None; MAX_ALSO_WIRES + 1],
            count: 0,
            error: None,
        }
    }

    fn push(&mut self, info: WireInfo, typed: Option<TypedWire<T>>) {
        if let (Some(info_slot), Some(typed_slot)) = (
            self.infos.get_mut(self.count),
            self.typed.get_mut(self.count),
        ) {
            *info_slot = Some(info);
            *typed_slot = typed;
            self.count = self.count.saturating_add(1);
        } else {
            self.error = Some("more than 8 wires on one route");
        }
    }

    /// This set with `info` first: the wire `.wire(..)` chose is the route's
    /// default, ahead of any `.also_wire(..)`, whatever order they were written in.
    fn with_primary(&self, info: WireInfo, typed: Option<TypedWire<T>>) -> Self {
        let mut ordered = Self::new();
        ordered.error = self.error;
        ordered.push(info, typed);
        for (info, typed) in self.infos.iter().zip(&self.typed).take(self.count) {
            if let Some(info) = info {
                ordered.push(*info, *typed);
            }
        }
        ordered
    }

    fn infos(&self) -> impl Iterator<Item = &WireInfo> {
        self.infos.iter().take(self.count).flatten()
    }

    fn typed_slice(&self, memory: &StartupMemory) -> Result<&'static [TypedWire<T>], StartupError> {
        let mut table: FixedVec<TypedWire<T>> = memory.fixed_vec(self.count)?;
        for typed in self.typed.iter().take(self.count).flatten() {
            table.push(*typed)?;
        }
        Ok(table.into_static_slice())
    }

    fn first_wire(&self) -> Option<&'static dyn Wire<T>> {
        self.typed
            .first()
            .copied()
            .flatten()
            .map(|typed| typed.wire)
    }

    /// Two wires of one Rust type would make `publish_using::<W>` ambiguous.
    fn has_duplicate_type(&self) -> bool {
        let mut rest = self.typed.iter().take(self.count).flatten();
        while let Some(wire) = rest.next() {
            if rest.clone().any(|other| other.type_id == wire.type_id) {
                return true;
            }
        }
        false
    }
}

fn map_config(reason: &'static str) -> StartupError {
    StartupError::InvalidConfig(reason)
}

/// Checks everything a route needs that does not need the binding's say-so.
struct AdmittedRoute {
    infos: &'static [WireInfo],
    envelope: Envelope,
    admitted: &'static [AdmittedWire],
    head: usize,
    max_payload: usize,
    alignment: usize,
    encodings: FixedVec<PayloadEncoding>,
    families: FixedVec<crate::wire::WireIdentity>,
}

fn admit_route(
    setup: &TransportSetup,
    options: &RouteOptions,
    name: &RouteName,
    infos: &mut dyn Iterator<Item = &WireInfo>,
    count: usize,
    kind: RouteKind,
    native: bool,
) -> Result<AdmittedRoute, StartupError> {
    let memory = setup.memory();
    let capabilities = setup.capabilities();
    let envelope = options.envelope.unwrap_or(capabilities.default_envelope);
    if !capabilities.envelopes.contains(&envelope) {
        return Err(StartupError::EnvelopeNotAccepted {
            route: name.clone(),
        });
    }
    let support = setup.qos_support(kind);
    if let Err(refused) = check_qos(&options.qos, &support) {
        return Err(StartupError::QosNotHonored {
            route: name.clone(),
            setting: refused.setting,
            requested: refused.requested,
            available: refused.available,
        });
    }
    if count == 0 {
        return Err(map_config("a route needs at least one wire"));
    }
    if !capabilities.per_message_encoding && count > 1 {
        return Err(StartupError::SingleEncodingOnly {
            route: name.clone(),
        });
    }
    let mut saved_infos: FixedVec<WireInfo> = memory.fixed_vec(count)?;
    let mut admitted: FixedVec<AdmittedWire> = memory.fixed_vec(count)?;
    let mut encodings: FixedVec<PayloadEncoding> = memory.fixed_vec(count)?;
    let mut families: FixedVec<crate::wire::WireIdentity> = memory.fixed_vec(count)?;
    let mut prefix_len = 0_usize;
    let mut max_payload = options.max_payload;
    let mut alignment = 1_usize;
    let mut exact_only = true;
    for info in infos {
        saved_infos.push(*info)?;
        let wire = AdmittedWire::admit(
            memory,
            envelope,
            info.wire_id,
            info.payload_family,
            info.identity,
        )
        .map_err(|_| StartupError::EnvelopeNotAccepted {
            route: name.clone(),
        })?;
        prefix_len = prefix_len.max(wire.prefix.len());
        admitted.push(wire)?;
        encodings.push(info.identity.encoding)?;
        families.push(info.payload_family)?;
        if options.max_payload.is_none() {
            max_payload = match (max_payload, info.max_encoded_len) {
                (Some(current), Some(next)) => Some(current.max(next)),
                (None, next @ Some(_)) => next,
                (current, None) => current,
            };
        }
        alignment = alignment.max(info.alignment);
        exact_only &= info.exact_size;
    }
    let max_payload = max_payload.ok_or(StartupError::PayloadUnbounded {
        route: name.clone(),
    })?;
    if native && setup.family() != Family::Lending {
        return Err(StartupError::NativeNeedsLending {
            route: name.clone(),
        });
    }
    if setup.family() == Family::Lending && !exact_only && !capabilities.truncate_payload {
        return Err(StartupError::TruncationNotSupported {
            route: name.clone(),
        });
    }
    let head = envelope.max_head_len(prefix_len, options.max_token);
    let requested = head.saturating_add(max_payload);
    if requested > capabilities.max_frame_len {
        return Err(StartupError::FrameTooLarge {
            route: name.clone(),
            requested,
            maximum: capabilities.max_frame_len,
        });
    }
    Ok(AdmittedRoute {
        infos: saved_infos.into_static_slice(),
        envelope,
        admitted: admitted.into_static_slice(),
        head,
        max_payload,
        alignment,
        encodings,
        families,
    })
}

struct TxPlan<'a> {
    setup: &'a TransportSetup,
    kind: UMessageType,
    source: UUri,
    sink: Option<UUri>,
    options: RouteOptions,
    native: bool,
}

fn build_tx(
    plan: TxPlan<'_>,
    infos: &mut dyn Iterator<Item = &WireInfo>,
    count: usize,
    wire_heap: &[(RuntimeHeap, &'static str)],
) -> Result<&'static TxRoute, StartupError> {
    let TxPlan {
        setup,
        kind,
        source,
        sink,
        options,
        native,
    } = plan;
    let memory = setup.memory();
    let lending = setup.family() == Family::Lending;
    let name = route_name(RouteDirection::Send, Some(kind), &source, sink.as_ref());
    if options.require_lending && !lending {
        return Err(StartupError::LendingRequired { route: name });
    }
    let route_kind_value = route_kind(RouteDirection::Send, Some(kind));
    let admission = admit_route(
        setup,
        &options,
        &name,
        infos,
        count,
        route_kind_value,
        native,
    )?;
    let id = setup.next_route_id()?;
    let prepared = setup.prepare_route(&RouteDeclaration {
            lifetime_policy: setup.shared().lifetime_policy(),
        direction: RouteDirection::Send,
        kind: Some(kind),
        source: &source,
        sink: sink.as_ref(),
        inbound: None,
        envelope: admission.envelope,
        encodings: admission.encodings.as_slice(),
        families: admission.families.as_slice(),
        max_metadata_len: admission.head,
        max_payload_len: admission.max_payload,
        payload_alignment: admission.alignment,
        slots: options.slots,
        qos: options.qos,
        native,
        max_peers: 0,
    })?;
    let (value_slots, scratch) = if lending {
        (None, None)
    } else {
        let slot_len = admission.head.saturating_add(admission.max_payload);
        (
            Some(crate::transport::slots::SlotPool::new_in(
                memory,
                options.slots,
                slot_len,
            )?.with_policy(setup.shared().lifetime_policy())),
            Some(crate::transport::slots::SlotPool::new_in(
                memory,
                options.slots.max(1),
                admission.head,
            )?.with_policy(setup.shared().lifetime_policy())),
        )
    };
    let mut wire_heaps: FixedVec<(RuntimeHeap, &'static str)> =
        memory.fixed_vec(wire_heap.len())?;
    for item in wire_heap {
        wire_heaps.push(*item)?;
    }
    let name_ref: &'static RouteName = memory.place(name)?;
    let route: &'static TxRoute = memory.place(TxRoute {
        name: name_ref,
        id,
        binding_route: prepared.route,
        kind,
        source,
        sink,
        priority: options.priority,
        ttl: options.ttl,
        permission_level: None,
        envelope: admission.envelope,
        admitted: admission.admitted,
        wire_heap: wire_heaps.into_static_slice(),
        value_slots,
        scratch,
        max_payload_len: admission.max_payload,
        max_head: admission.head,
        max_token_len: options.max_token,
        allow_traceparent: options.allow_traceparent,
        alignment: admission.alignment,
        ids: crate::transport::ids::IdGenerator::seeded(
            read_seed(setup)?,
            setup.platform().unix_time_ms().unwrap_or(0),
        ),
        qos: options.qos,
        counters: crate::transport::report::Counters::default(),
        heap: prepared.heap,
        deviations: prepared.deviations,
        lending,
        native,
        shared: setup.shared(),
        binding: setup.binding(),
    })?;
    setup.check_route(crate::transport::RouteView {
        report: route.report(),
        wires: admission.infos,
        max_peers: 0,
        clients: &[],
        ttl: options.ttl,
        max_in_flight: 0,
    })?;
    setup.add_tx(route)?;
    Ok(route)
}

fn read_seed(setup: &TransportSetup) -> Result<[u8; 16], StartupError> {
    let mut seed = [0_u8; 16];
    setup
        .platform_init()
        .entropy(&mut seed)
        .map_err(|_| map_config("the platform cannot provide entropy to seed message IDs"))?;
    Ok(seed)
}

/// Shared activation of a send role: checks the runtime, then copies the pointers.
fn check_transport(owner: u64, runtime: &TransportRuntime) -> Result<(), StartupError> {
    if owner == runtime.core.id {
        Ok(())
    } else {
        Err(StartupError::WrongTransport)
    }
}

/// The body of `publish` and `notify`: fills the identity, then sends.
fn send_with<T: 'static>(
    route: &'static TxRoute,
    wire: &TypedWire<T>,
    admitted: &'static AdmittedWire,
    fields: &MessageFields<'_>,
    value: &T,
) -> Result<SendReport, SendError> {
    let id;
    let parts_id = match fields.id {
        Some(id) => id,
        None => {
            id = route.next_id()?;
            &id
        }
    };
    let mut parts = route.parts(parts_id, fields)?;
    parts.payload_encoding = Some(wire.info.identity.encoding);
    parts.native_type_token = wire.info.identity.native_type_token;
    route.binding.send_value(
        route,
        ValueFrame {
            parts,
            wire: admitted,
            size: wire.wire.encoded_size(value),
            encode: &mut |out| wire.wire.encode(value, out),
            reply: ReplyToken::default(),
        },
    )
}

/// The send path shared by [`Publisher`] and [`Notifier`].
struct SendCore<T: 'static> {
    route: &'static TxRoute,
    wires: &'static [TypedWire<T>],
}

impl<T> Clone for SendCore<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for SendCore<T> {}

impl<T: 'static> SendCore<T> {
    fn try_send(&self, fields: &MessageFields<'_>, value: &T) -> Result<SendReport, SendError> {
        let (Some(wire), Some(admitted)) = (self.wires.first(), self.route.admitted.first()) else {
            return Err(SendError::WireNotAdmitted);
        };
        send_with(self.route, wire, admitted, fields, value)
    }

    fn send_using<W: 'static>(
        &self,
        fields: &MessageFields<'_>,
        value: &T,
    ) -> Result<SendReport, SendError> {
        let id = TypeId::of::<W>();
        for (wire, admitted) in self.wires.iter().zip(self.route.admitted) {
            if wire.type_id == id {
                return send_with(self.route, wire, admitted, fields, value);
            }
        }
        Err(SendError::WireNotAdmitted)
    }

    fn send_encoded(
        &self,
        fields: &MessageFields<'_>,
        encoding: PayloadEncoding,
        payload: PayloadBytes<'_>,
    ) -> Result<SendReport, SendError> {
        let route = self.route;
        let admitted = route
            .admitted
            .iter()
            .find(|admitted| admitted.encoding == encoding)
            .ok_or(SendError::EncodingNotAdmitted(encoding.id()))?;
        let generated;
        let id = match fields.id {
            Some(id) => id,
            None => {
                generated = route.next_id()?;
                &generated
            }
        };
        let mut parts = route.parts(id, fields)?;
        parts.payload_encoding = payload.is_present().then_some(encoding);
        parts.native_type_token = admitted.native_type_token;
        route.binding.send_encoded(
            route,
            EncodedFrame {
                parts,
                wire: Some(admitted),
                payload,
                reply: ReplyToken::default(),
            },
        )
    }
}

impl<T: 'static> SendCore<T> {
    fn counters(&self) -> RouteCounters {
        self.route.counters.snapshot()
    }

    fn route_id(&self) -> RouteId {
        self.route.id
    }
}

macro_rules! send_role {
    (
        $(#[$meta:meta])*
        $name:ident, $setup:ident, $builder:ident, $send:ident, $using:ident, $encoded:ident, $with:ident, $kind:expr
    ) => {
        $(#[$meta])*
        pub struct $name<T: 'static, M: WireMode = Encoded> {
            core: SendCore<T>,
            _mode: PhantomData<fn() -> M>,
        }

        impl<T: 'static, M: WireMode> Clone for $name<T, M> {
            fn clone(&self) -> Self {
                *self
            }
        }
        impl<T: 'static, M: WireMode> Copy for $name<T, M> {}
        impl<T: 'static, M: WireMode> fmt::Debug for $name<T, M> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }

        /// A role reserved during initialization. It has no way to send.
        pub struct $setup<T: 'static, M: WireMode = Encoded> {
            transport: u64,
            route: &'static TxRoute,
            wires: &'static [TypedWire<T>],
            _mode: PhantomData<fn() -> M>,
        }

        impl<T: 'static, M: WireMode> fmt::Debug for $setup<T, M> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($setup)).finish_non_exhaustive()
            }
        }

        /// Chooses this role's wire, QoS and reservations during initialization.
        pub struct $builder<'a, T: 'static, W> {
            setup: &'a TransportSetup,
            source: UUri,
            sink: Option<UUri>,
            wire: W,
            set: WireSet<T>,
            options: RouteOptions,
            _message: PhantomData<fn() -> T>,
        }

        impl<'a, T: 'static, W> fmt::Debug for $builder<'a, T, W> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($builder)).finish_non_exhaustive()
            }
        }

        impl<'a, T: 'static, W> $builder<'a, T, W> {
            /// Chooses this role's wire. Every wire has this spelling (R21).
            #[must_use]
            pub fn wire<W2>(self, wire: W2) -> $builder<'a, T, W2> {
                $builder {
                    setup: self.setup,
                    source: self.source,
                    sink: self.sink,
                    wire,
                    set: self.set,
                    options: self.options,
                    _message: PhantomData,
                }
            }

            /// Admits one more wire into this route's closed set (R9).
            #[must_use]
            pub fn also_wire<W2: Wire<T>>(mut self, wire: W2) -> Self {
                let info = wire.describe();
                match self.setup.memory().place(wire) {
                    Ok(placed) => {
                        let placed: &'static W2 = placed;
                        self.set.push(
                            info,
                            Some(TypedWire {
                                type_id: TypeId::of::<W2>(),
                                wire: placed,
                                info,
                            }),
                        );
                    }
                    Err(_) => {
                        self.set.error = Some("no startup memory for a wire");
                    }
                }
                self
            }

            /// Sets this route's message time-to-live.
            #[must_use]
            pub fn ttl(mut self, ttl: Duration) -> Self {
                self.options.ttl = Some(ttl);
                self
            }

            route_options!();
            send_options!();

            /// Reserves everything this role needs. `M` is inferred from the wire.
            ///
            /// # Errors
            /// Refuses the route for capability, naming it.
            pub fn build<M: WireMode>(self) -> Result<$setup<T, M>, StartupError>
            where
                W: RoleWire<T, M>,
            {
                let Self {
                    setup,
                    source,
                    sink,
                    wire,
                    mut set,
                    options,
                    _message,
                } = self;
                if let Some(reason) = set.error {
                    return Err(map_config(reason));
                }
                let kind: UMessageType = $kind;
                let name = route_name(RouteDirection::Send, Some(kind), &source, sink.as_ref());
                let memory = setup.memory();
                let info = RoleWire::__describe(&wire).map_err(map_config)?;
                let native;
                match RoleWire::__place(wire, memory)? {
                    RoleWireKind::Encoded(placed) => {
                        native = false;
                        set = set.with_primary(
                            info,
                            Some(TypedWire {
                                type_id: TypeId::of::<W>(),
                                wire: placed,
                                info,
                            }),
                        );
                    }
                    RoleWireKind::InPlace => {
                        native = true;
                        if set.count > 0 {
                            return Err(map_config(
                                "a native route carries one wire; drop the also_wire",
                            ));
                        }
                        set = set.with_primary(info, None);
                    }
                }
                if let Some(reason) = set.error {
                    return Err(map_config(reason));
                }
                if set.has_duplicate_type() {
                    return Err(StartupError::DuplicateWireType { route: name });
                }
                let wires = set.typed_slice(memory)?;
                let mut heaps: [(RuntimeHeap, &'static str); MAX_ALSO_WIRES + 1] =
                    [(RuntimeHeap::None, ""); MAX_ALSO_WIRES + 1];
                for (slot, info) in heaps.iter_mut().zip(set.infos()) {
                    *slot = (info.encode_heap(), info.name);
                }
                let count = set.count;
                let heap_slice = heaps.get(..count).unwrap_or(&[]);
                let mut infos = set.infos();
                let route = build_tx(
                    TxPlan {
                        setup,
                        kind,
                        source,
                        sink,
                        options,
                        native,
                    },
                    &mut infos,
                    count,
                    heap_slice,
                )?;
                Ok($setup {
                    transport: setup.id(),
                    route,
                    wires,
                    _mode: PhantomData,
                })
            }
        }

        impl<T: 'static, M: WireMode> $setup<T, M> {
            /// Binds this role to the running transport. Allocates nothing.
            ///
            /// # Errors
            /// Refuses a runtime other than the one this role was set up on.
            pub fn activate(self, runtime: &TransportRuntime) -> Result<$name<T, M>, StartupError> {
                const { assert!(!mem::needs_drop::<$name<T, M>>()) };
                check_transport(self.transport, runtime)?;
                Ok($name {
                    core: SendCore {
                        route: self.route,
                        wires: self.wires,
                    },
                    _mode: PhantomData,
                })
            }
        }

        impl<T: 'static> $name<T, Encoded> {
            /// Sends `value` through the route's first wire.
            ///
            /// # Errors
            /// Refuses with a fixed-size reason. No heap on any path.
            pub fn $send(&self, value: &T) -> Result<SendReport, SendError> {
                self.core.try_send(&MessageFields::default(), value)
            }

            /// As the plain send above, with per-message fields such as a token.
            ///
            /// # Errors
            /// As the plain send; also refuses a field the route was not set up for.
            pub fn $with(&self, fields: &MessageFields<'_>, value: &T) -> Result<SendReport, SendError> {
                self.core.try_send(fields, value)
            }

            /// Sends `value` through the admitted wire of type `W` (R9).
            ///
            /// The SDK runs the stored wire and never calls caller-supplied code at send.
            ///
            /// # Errors
            /// `WireNotAdmitted` if no admitted wire has this type.
            pub fn $using<W: Wire<T>>(&self, value: &T) -> Result<SendReport, SendError> {
                self.core.send_using::<W>(&MessageFields::default(), value)
            }

            /// Sends bytes already encoded in an admitted encoding (R14, R24).
            ///
            /// # Errors
            /// `EncodingNotAdmitted` if the encoding is outside the route's set.
            pub fn $encoded(
                &self,
                encoding: PayloadEncoding,
                payload: PayloadBytes<'_>,
            ) -> Result<SendReport, SendError> {
                self.core.send_encoded(&MessageFields::default(), encoding, payload)
            }
        }

        impl<T: 'static, M: WireMode> $name<T, M> {
            /// This route's counters.
            #[must_use]
            pub fn counters(&self) -> RouteCounters {
                self.core.counters()
            }

            /// This route's ID, for `TransportRuntime::counters`.
            #[must_use]
            pub fn route_id(&self) -> RouteId {
                self.core.route_id()
            }
        }

        impl<T: StablePayloadInit> $name<T, InPlace> {
            /// Initializes the payload directly in the transport's loan (R18).
            ///
            /// ```compile_fail
            /// # use up_rust_core::communication::two_mode::Publisher;
            /// # use up_rust_core::wire::InPlace;
            /// // No value-based send on an in-place handle.
            /// fn no_value_send(publisher: Publisher<u32, InPlace>) {
            ///     publisher.publish(&1);
            /// }
            /// ```
            ///
            /// # Errors
            /// Refuses with a fixed-size reason, including a witness that does
            /// not belong to this loan.
            pub fn publish_in_place<F>(&self, fill: F) -> Result<SendReport, SendError>
            where
                F: for<'a> FnOnce(InPlaceInit<'a, T>) -> InitializedStablePayload<'a, T>,
            {
                publish_in_place_on(self.core.route, fill)
            }
        }
    };
}

/// A handle to a stable payload's in-place initializer for one send.
pub struct InPlaceInit<'a, T: StablePayloadInit + 'a> {
    initializer: T::Initializer<'a>,
}

impl<T: StablePayloadInit> fmt::Debug for InPlaceInit<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InPlaceInit").finish_non_exhaustive()
    }
}

impl<'a, T: StablePayloadInit + 'a> InPlaceInit<'a, T> {
    /// Starts field-by-field initialization of `T` directly in the loan.
    pub fn into_initializer(self) -> T::Initializer<'a> {
        self.initializer
    }
}

fn publish_in_place_on<T, F>(route: &'static TxRoute, fill: F) -> Result<SendReport, SendError>
where
    T: StablePayloadInit,
    F: for<'a> FnOnce(InPlaceInit<'a, T>) -> InitializedStablePayload<'a, T>,
{
    let id = route.next_id()?;
    send_in_place_with::<T, F>(route, &id, &MessageFields::default(), None, fill)
}

/// The body of every in-place send: `sink` overrides the route's for a response.
fn send_in_place_with<T, F>(
    route: &'static TxRoute,
    id: &UUID,
    fields: &MessageFields<'_>,
    sink: Option<&UUri>,
    fill: F,
) -> Result<SendReport, SendError>
where
    T: StablePayloadInit,
    F: for<'a> FnOnce(InPlaceInit<'a, T>) -> InitializedStablePayload<'a, T>,
{
    let (Some(admitted), true) = (route.admitted.first(), route.native) else {
        return Err(SendError::WireNotAdmitted);
    };
    let mut parts = route.parts(id, fields)?;
    if sink.is_some() {
        parts.sink = sink;
    }
    parts.payload_encoding = Some(admitted.encoding);
    parts.native_type_token = admitted.native_type_token;
    let mut fill = Some(fill);
    let payload_len = mem::size_of::<T>();
    let mut checked_fill = |storage: &mut [MaybeUninit<u8>]| -> Result<(), SendError> {
        let Some(fill) = fill.take() else {
            return Err(SendError::WitnessMismatch);
        };
        let start = storage.as_ptr();
        // The SDK checked the loan's size and alignment for exactly this type;
        // a refusal here is a binding defect, reported rather than a panic.
        let initializer = T::init(storage).map_err(|_| SendError::PayloadStorage)?;
        let witness = fill(InPlaceInit { initializer });
        if core::ptr::eq(witness.as_bytes().as_ptr(), start.cast::<u8>())
            && witness.as_bytes().len() == payload_len
        {
            Ok(())
        } else {
            Err(SendError::WitnessMismatch)
        }
    };
    // SAFETY: InitializedStablePayload has private fields; its unsafe
    // finish_stable_payload_init constructor requires initialized valid T bytes
    // in live exclusive storage. T's unsafe StablePayload/StablePayloadField
    // contracts preserve byte validity and exclude interior mutation. The safe
    // StablePayloadInit trait itself supplies no unsafe premise. checked_fill
    // returns Ok only for this exact loan base/full extent;
    // the higher-ranked initializer cannot retain its borrow, and this closure
    // neither replaces storage nor schedules deferred mutation. A second call
    // refuses, and refusal/unwind never reports successful initialization.
    let frame = unsafe {
        InPlaceFrame::from_checked_fill(parts, admitted, payload_len, &mut checked_fill)
    };
    route.binding.send_in_place(route, frame)
}

send_role! {
    /// Publishes events to subscribers of this entity's topic.
    ///
    /// Stored handles name domain types only (R28): `Publisher<DoorState>`.
    Publisher, PublisherSetup, PublisherBuilder, publish, publish_using, publish_encoded,
    publish_with, UMessageType::Publish
}

send_role! {
    /// Sends notifications to one destination fixed at setup.
    Notifier, NotifierSetup, NotifierBuilder, notify, notify_using, notify_encoded,
    notify_with, UMessageType::Notification
}

impl<T: 'static> Publisher<T> {
    /// Starts declaring a publisher on this transport. `resource` is a plain
    /// resource ID or a typed [`PublishTopic`] (R13).
    pub fn on<'a, S: TransportSource>(
        source: &'a S,
        uri: &dyn LocalUriProvider,
        resource: impl PublishResource<T>,
    ) -> PublisherBuilder<'a, T, S::DefaultWire> {
        PublisherBuilder {
            setup: source.setup(),
            source: uri.get_resource_uri(resource.resource_id()),
            sink: None,
            wire: source.default_wire(),
            set: WireSet::new(),
            options: RouteOptions::default(),
            _message: PhantomData,
        }
    }
}

impl<T: 'static, M: WireMode> Publisher<T, M> {
    /// Test hook: lends `f` the bytes of a free value slot of this handoff
    /// publisher, for checking what a send left there (L4, R3-4.1). `None` on a
    /// lending route, which has no value slots, or while every slot is taken.
    #[doc(hidden)]
    #[cfg(up_rust_test_hooks)]
    pub fn __free_value_slot<R>(&self, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let slot = self.core.route.value_slots.as_ref()?.acquire().ok()?;
        Some(f(slot.bytes()))
    }
}

impl<T: 'static> Notifier<T> {
    /// Starts declaring a notifier with its destination fixed at setup (design S3).
    pub fn on<'a, S: TransportSource>(
        source: &'a S,
        uri: &dyn LocalUriProvider,
        resource: impl NotificationResource<T>,
        destination: UUri,
    ) -> NotifierBuilder<'a, T, S::DefaultWire> {
        NotifierBuilder {
            setup: source.setup(),
            source: uri.get_resource_uri(resource.resource_id()),
            sink: Some(destination),
            wire: source.default_wire(),
            set: WireSet::new(),
            options: RouteOptions::default(),
            _message: PhantomData,
        }
    }
}

macro_rules! receive_role {
    (
        $(#[$meta:meta])*
        $name:ident, $setup:ident, $builder:ident, $kind:expr
    ) => {
        $(#[$meta])*
        pub struct $name<T: 'static, M: WireMode = Encoded> {
            route: &'static TypedInbound<T, M>,
        }

        impl<T: 'static, M: WireMode> Clone for $name<T, M> {
            fn clone(&self) -> Self {
                *self
            }
        }
        impl<T: 'static, M: WireMode> Copy for $name<T, M> {}
        impl<T: 'static, M: WireMode> fmt::Debug for $name<T, M> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }

        /// A role reserved during initialization. It has no way to receive.
        pub struct $setup<T: 'static, M: WireMode = Encoded> {
            transport: u64,
            route: &'static TypedInbound<T, M>,
        }

        impl<T: 'static, M: WireMode> fmt::Debug for $setup<T, M> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($setup)).finish_non_exhaustive()
            }
        }

        /// Chooses this role's wire, QoS and reservations during initialization.
        pub struct $builder<'a, T: 'static, W> {
            setup: &'a TransportSetup,
            source_filter: UUri,
            sink_filter: Option<UUri>,
            wire: W,
            set: WireSet<T>,
            options: RouteOptions,
            _message: PhantomData<fn() -> T>,
        }

        impl<'a, T: 'static, W> fmt::Debug for $builder<'a, T, W> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct(stringify!($builder)).finish_non_exhaustive()
            }
        }

        impl<'a, T: 'static, W> $builder<'a, T, W> {
            /// Chooses this role's wire. Every wire has this spelling (R21).
            #[must_use]
            pub fn wire<W2>(self, wire: W2) -> $builder<'a, T, W2> {
                $builder {
                    setup: self.setup,
                    source_filter: self.source_filter,
                    sink_filter: self.sink_filter,
                    wire,
                    set: self.set,
                    options: self.options,
                    _message: PhantomData,
                }
            }

            /// Admits one more wire into this route's closed set (R9). Frames
            /// select among admitted wires by their encoding.
            #[must_use]
            pub fn also_wire<W2: Wire<T>>(mut self, wire: W2) -> Self {
                let info = wire.describe();
                match self.setup.memory().place(wire) {
                    Ok(placed) => {
                        let placed: &'static W2 = placed;
                        self.set.push(
                            info,
                            Some(TypedWire {
                                type_id: TypeId::of::<W2>(),
                                wire: placed,
                                info,
                            }),
                        );
                    }
                    Err(_) => {
                        self.set.error = Some("no startup memory for a wire");
                    }
                }
                self
            }

            route_options!();
            receive_options!();

            /// Reserves everything this role needs. `M` is inferred from the wire.
            ///
            /// # Errors
            /// Refuses the route for capability, naming it.
            pub fn build<M: WireMode>(self) -> Result<$setup<T, M>, StartupError>
            where
                W: RoleWire<T, M>,
                T: Send + Sync,
            {
                build_receive::<T, W, M>(
                    self.setup,
                    self.source_filter,
                    self.sink_filter,
                    self.wire,
                    self.set,
                    self.options,
                    $kind,
                )
                .map(|route| $setup {
                    transport: self.setup.id(),
                    route,
                })
            }
        }

        impl<T: Send + Sync + 'static, M: WireMode> $setup<T, M> {
            /// Binds this role to the running transport. Allocates nothing.
            ///
            /// # Errors
            /// Refuses a runtime other than the one this role was set up on.
            pub fn activate(self, runtime: &TransportRuntime) -> Result<$name<T, M>, StartupError> {
                const { assert!(!mem::needs_drop::<$name<T, M>>()) };
                check_transport(self.transport, runtime)?;
                Ok($name { route: self.route })
            }
        }

        impl<T: Send + Sync + 'static, M: WireMode> $name<T, M> {
            /// Takes the oldest received frame, if any. Non-blocking.
            ///
            /// # Errors
            /// `MaxHeldExceeded` when every lease is held.
            pub fn try_recv(&self) -> Result<Option<Received<T, M>>, ReceiveError> {
                self.route.try_receive()
            }

            /// This route's counters.
            #[must_use]
            pub fn counters(&self) -> RouteCounters {
                self.route.common.counters.snapshot()
            }

            /// This route's ID, for `TransportRuntime::counters`.
            #[must_use]
            pub fn route_id(&self) -> RouteId {
                self.route.common.id
            }

            /// The route's waker storage, for the async layer.
            #[doc(hidden)]
            /// Returns the receive route’s process-lifetime wake storage. Only the owning receive
            /// adapter registers or clears it; binding threads may wake it. Register before waiting,
            /// poll again after registration and every wake (a hint), and clear on Recv drop. This
            /// accessor neither blocks nor uses the heap.
            // [impl->req~ssr-async-seam~1]
            pub fn __route_wake(&self) -> &'static crate::transport::wake::RouteWake {
                self.route.__wake()
            }


        }
    };
}

receive_role! {
    /// Receives events published on a topic.
    Subscriber, SubscriberSetup, SubscriberBuilder, UMessageType::Publish
}

receive_role! {
    /// Receives notifications addressed to this entity.
    NotificationListener, NotificationListenerSetup, NotificationListenerBuilder, UMessageType::Notification
}

impl<T: 'static> Subscriber<T> {
    /// Starts declaring a subscriber. `topic` is the remote topic URI.
    pub fn on<'a, S: TransportSource>(
        source: &'a S,
        topic: UUri,
    ) -> SubscriberBuilder<'a, T, S::DefaultWire> {
        SubscriberBuilder {
            setup: source.setup(),
            source_filter: topic,
            sink_filter: None,
            wire: source.default_wire(),
            set: WireSet::new(),
            options: RouteOptions::default(),
            _message: PhantomData,
        }
    }
}

impl<T: 'static> NotificationListener<T> {
    /// Starts declaring a listener for notifications from `origin` to this entity.
    pub fn on<'a, S: TransportSource>(
        source: &'a S,
        origin: UUri,
        uri: &dyn LocalUriProvider,
    ) -> NotificationListenerBuilder<'a, T, S::DefaultWire> {
        NotificationListenerBuilder {
            setup: source.setup(),
            source_filter: origin,
            sink_filter: Some(uri.get_source_uri()),
            wire: source.default_wire(),
            set: WireSet::new(),
            options: RouteOptions::default(),
            _message: PhantomData,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_receive<T, W, M>(
    setup: &TransportSetup,
    source_filter: UUri,
    sink_filter: Option<UUri>,
    wire: W,
    mut set: WireSet<T>,
    options: RouteOptions,
    kind: UMessageType,
) -> Result<&'static TypedInbound<T, M>, StartupError>
where
    T: Send + Sync + 'static,
    M: WireMode,
    W: RoleWire<T, M>,
{
    if let Some(reason) = set.error {
        return Err(map_config(reason));
    }
    let memory = setup.memory();
    let info = RoleWire::__describe(&wire).map_err(map_config)?;
    let mut native = None;
    match RoleWire::__place(wire, memory)? {
        RoleWireKind::Encoded(placed) => {
            set = set.with_primary(
                info,
                Some(TypedWire {
                    type_id: TypeId::of::<W>(),
                    wire: placed,
                    info,
                }),
            );
        }
        RoleWireKind::InPlace => {
            if set.count > 0 {
                return Err(map_config(
                    "a native route carries one wire; drop the also_wire",
                ));
            }
            native = Some(native_identity(&info));
            set = set.with_primary(info, None);
        }
    }
    if let Some(reason) = set.error {
        return Err(map_config(reason));
    }
    if set.has_duplicate_type() {
        return Err(StartupError::DuplicateWireType {
            route: route_name(
                RouteDirection::Receive,
                Some(kind),
                &source_filter,
                sink_filter.as_ref(),
            ),
        });
    }
    place_receive(
        setup,
        source_filter,
        sink_filter,
        &set,
        options,
        kind,
        native,
        &[],
    )
}

fn native_identity(info: &WireInfo) -> WirePayloadIdentity {
    WirePayloadIdentity {
        encoding: info.identity.encoding,
        native_type_token: info.identity.native_type_token,
    }
}

/// Places one receive route: `native` holds the agreed identity of an in-place
/// route, and `clients` the deployment's client list, empty on every other route.
#[allow(clippy::too_many_arguments)]
fn place_receive<T, M>(
    setup: &TransportSetup,
    source_filter: UUri,
    sink_filter: Option<UUri>,
    set: &WireSet<T>,
    options: RouteOptions,
    kind: UMessageType,
    native: Option<WirePayloadIdentity>,
    clients: &'static [UUri],
) -> Result<&'static TypedInbound<T, M>, StartupError>
where
    T: Send + Sync + 'static,
    M: WireMode,
{
    let memory = setup.memory();
    let name = route_name(
        RouteDirection::Receive,
        Some(kind),
        &source_filter,
        sink_filter.as_ref(),
    );
    let is_native = native.is_some();
    let mut infos = set.infos();
    let count = set.count;
    let rx_kind = route_kind(RouteDirection::Receive, Some(kind));
    let admission = admit_route(
        setup, &options, &name, &mut infos, count, rx_kind, is_native,
    )?;
    // The binding is told exactly the table the SDK builds: none of zero entries.
    if options.max_peers == 0 {
        return Err(StartupError::InvalidConfig(
            ".max_peers(0) leaves no entry for any identified sender",
        ));
    }
    let id = setup.next_route_id()?;
    let inbound = setup.next_inbound()?;
    let prepared: PreparedRoute = setup.prepare_route(&RouteDeclaration {
            lifetime_policy: setup.shared().lifetime_policy(),
        direction: RouteDirection::Receive,
        kind: Some(kind),
        source: &source_filter,
        sink: sink_filter.as_ref(),
        inbound: Some(inbound),
        envelope: admission.envelope,
        encodings: admission.encodings.as_slice(),
        families: admission.families.as_slice(),
        max_metadata_len: admission.head,
        max_payload_len: admission.max_payload,
        payload_alignment: admission.alignment,
        slots: usize::from(options.qos.depth)
            .saturating_add(options.max_held)
            .saturating_add(1),
        qos: options.qos,
        native: is_native,
        max_peers: options.max_peers,
    })?;
    let peers = PeerTable::new_in(memory, options.max_peers)?;
    let limits = HeaderLimits {
        max_token_len: options.max_token,
    };
    let name_ref: &'static RouteName = memory.place(name)?;
    let common = RxCommon::new(
        name_ref,
        id,
        prepared.route,
        Some(kind),
        source_filter,
        sink_filter,
        clients,
        admission.envelope,
        admission.admitted,
        limits,
        peers,
        options.qos,
        prepared.heap,
        prepared.deviations,
        setup.shared(),
    );
    let lending = setup.family() == Family::Lending;
    let frame_len = if lending {
        0
    } else {
        admission.head.saturating_add(admission.max_payload)
    };
    let text_len = if lending && !is_native {
        options
            .max_token
            .saturating_add(HEADER_TRACEPARENT_CAPACITY)
    } else {
        0
    };
    let (rx_kind_value, init_wire) = match native {
        Some(identity) => (RxKind::InPlace { identity }, None),
        None => (
            RxKind::Encoded {
                wires: set.typed_slice(memory)?,
            },
            set.first_wire(),
        ),
    };
    let prepared_route = TypedInbound::<T, M>::place(
        memory,
        common,
        rx_kind_value,
        usize::from(options.qos.depth),
        options.max_held,
        frame_len,
        text_len,
        is_native,
        init_wire,
    )?;
    setup.check_route(crate::transport::RouteView {
        report: prepared_route.report(),
        wires: admission.infos,
        max_peers: options.max_peers,
        clients,
        ttl: options.ttl,
        max_in_flight: 0,
    })?;
    prepared_route.install(setup)
}

/// A received request with its responder. Dropping it returns the request's slot.
pub struct Request<Req: 'static, Resp: 'static, M: WireMode = Encoded> {
    received: Received<Req, M>,
    server: ServerCore<Resp>,
    source: UUri,
    reqid: UUID,
    reply: ReplyToken,
    ttl: Option<Duration>,
}

impl<Req: 'static, Resp: 'static, M: WireMode> fmt::Debug for Request<Req, Resp, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Request").finish_non_exhaustive()
    }
}

impl<Req: Send + Sync + 'static, Resp: 'static> core::ops::Deref for Request<Req, Resp, Encoded> {
    type Target = Req;

    fn deref(&self) -> &Req {
        &self.received
    }
}

impl<Req: 'static, Resp: 'static, M: WireMode> Request<Req, Resp, M> {
    /// The request's lease, for access to its header.
    #[must_use]
    pub fn received(&self) -> &Received<Req, M> {
        &self.received
    }
}

impl<Req: 'static, Resp: 'static, M: WireMode> Request<Req, Resp, M> {
    /// Whether a response may still go out: the window is the request's own
    /// time-to-live, from its creation, the same rule the receive path applied.
    ///
    /// # Errors
    /// `Expired` after the window; `ClockUnavailable` when the wall clock cannot
    /// be read, so the window cannot be judged (fail closed).
    fn check_reply_window(&self) -> Result<(), SendError> {
        let Some(ttl) = self.ttl else {
            return Ok(());
        };
        if ttl.is_zero() {
            return Ok(());
        }
        let now = self
            .server
            .shared
            .platform
            .unix_time_ms()
            .ok_or(SendError::ClockUnavailable)?;
        if self.reqid.is_expired_at(ttl_millis(ttl), u128::from(now)) {
            return Err(SendError::Expired);
        }
        Ok(())
    }

    fn respond_route(&self) -> Option<&'static TxRoute> {
        self.server.route_for(&self.source)
    }

    fn response_fields(&self) -> MessageFields<'_> {
        MessageFields {
            id: None,
            reqid: Some(&self.reqid),
            priority: None,
            token: None,
            traceparent: None,
            comm_status: None,
        }
    }

    /// Sends a status-only failure response. Consumes the request.
    ///
    /// # Errors
    /// `Expired` after the request's time-to-live, counted from its creation;
    /// otherwise the send's refusal.
    pub fn respond_failure(self, code: UCode) -> Result<SendReport, SendError> {
        self.check_reply_window()?;
        let route = self.respond_route().ok_or(SendError::Rejected {
            code: UCode::NotFound,
            detail: "the requester is not in this server's client list",
        })?;
        let mut fields = self.response_fields();
        fields.comm_status = Some(code);
        let id = route.next_id()?;
        let mut parts = route.parts(&id, &fields)?;
        parts.sink = Some(&self.source);
        route.binding.send_encoded(
            route,
            EncodedFrame {
                parts,
                wire: route.admitted.first(),
                payload: PayloadBytes::Absent,
                reply: self.reply,
            },
        )
    }
}

impl<Req: 'static, Resp: 'static> Request<Req, Resp, Encoded> {
    /// Sends the response. Consumes the request.
    ///
    /// # Errors
    /// `Expired` after the request's time-to-live, counted from its creation;
    /// otherwise the send's refusal.
    pub fn respond(self, value: &Resp) -> Result<SendReport, SendError> {
        self.check_reply_window()?;
        let route = self.respond_route().ok_or(SendError::Rejected {
            code: UCode::NotFound,
            detail: "the requester is not in this server's client list",
        })?;
        let (Some(wire), Some(admitted)) = (self.server.wires.first(), route.admitted.first())
        else {
            return Err(SendError::WireNotAdmitted);
        };
        let fields = self.response_fields();
        let id = route.next_id()?;
        let mut parts = route.parts(&id, &fields)?;
        parts.sink = Some(&self.source);
        parts.payload_encoding = Some(wire.info.identity.encoding);
        parts.native_type_token = wire.info.identity.native_type_token;
        route.binding.send_value(
            route,
            ValueFrame {
                parts,
                wire: admitted,
                size: wire.wire.encoded_size(value),
                encode: &mut |out| wire.wire.encode(value, out),
                reply: self.reply,
            },
        )
    }
}

impl<Req: StablePayload + Send + Sync + 'static, Resp: 'static> Request<Req, Resp, InPlace> {
    /// Borrows the typed request from the retained native storage, checked.
    ///
    /// # Errors
    /// Reports a frame whose bytes are not a valid `Req`, with a fixed-size reason.
    pub fn payload(&self) -> Result<&Req, BorrowError> {
        self.received.payload()
    }
}

impl<Req: 'static, Resp: StablePayloadInit> Request<Req, Resp, InPlace> {
    /// Initializes the response directly in the transport's loan (R18). Consumes
    /// the request.
    ///
    /// # Errors
    /// `Expired` after the request's time-to-live, counted from its creation;
    /// otherwise the send's refusal, including a witness that does not belong to
    /// this loan.
    pub fn respond_in_place<F>(self, fill: F) -> Result<SendReport, SendError>
    where
        F: for<'a> FnOnce(InPlaceInit<'a, Resp>) -> InitializedStablePayload<'a, Resp>,
    {
        self.check_reply_window()?;
        let route = self.respond_route().ok_or(SendError::Rejected {
            code: UCode::NotFound,
            detail: "the requester is not in this server's client list",
        })?;
        let id = route.next_id()?;
        let fields = self.response_fields();
        send_in_place_with::<Resp, F>(route, &id, &fields, Some(&self.source), fill)
    }
}

/// The response side of an RPC server.
struct ServerCore<Resp: 'static> {
    /// Whether this server enforces a client list.
    enforce_clients: bool,
    /// Clients, parallel to `client_routes`. Also the receive route's list.
    clients: &'static [UUri],
    /// One route per declared client, or one shared route.
    client_routes: &'static [&'static TxRoute],
    shared_route: Option<&'static TxRoute>,
    wires: &'static [TypedWire<Resp>],
    shared: &'static TransportShared,
}

impl<Resp> Clone for ServerCore<Resp> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Resp> Copy for ServerCore<Resp> {}

impl<Resp> ServerCore<Resp> {
    fn route_for(&self, client: &UUri) -> Option<&'static TxRoute> {
        if self.enforce_clients {
            let index = self
                .clients
                .iter()
                .position(|known| known.matches(client))?;
            self.client_routes.get(index).copied()
        } else {
            self.shared_route
        }
    }
}

/// A server for one RPC method. Stored handles name domain types only.
pub struct RpcServer<Req: 'static, Resp: 'static, M: WireMode = Encoded> {
    requests: &'static TypedInbound<Req, M>,
    core: ServerCore<Resp>,
    _mode: PhantomData<fn() -> M>,
}

impl<Req: 'static, Resp: 'static, M: WireMode> Clone for RpcServer<Req, Resp, M> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Req: 'static, Resp: 'static, M: WireMode> Copy for RpcServer<Req, Resp, M> {}
impl<Req: 'static, Resp: 'static, M: WireMode> fmt::Debug for RpcServer<Req, Resp, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RpcServer").finish_non_exhaustive()
    }
}

/// An RPC server reserved during initialization.
pub struct RpcServerSetup<Req: 'static, Resp: 'static, M: WireMode = Encoded> {
    transport: u64,
    requests: &'static TypedInbound<Req, M>,
    core: ServerCore<Resp>,
    _mode: PhantomData<fn() -> M>,
}

impl<Req: 'static, Resp: 'static, M: WireMode> fmt::Debug for RpcServerSetup<Req, Resp, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RpcServerSetup").finish_non_exhaustive()
    }
}

impl<'a, Req: 'static, Resp: 'static, W> fmt::Debug for RpcServerBuilder<'a, Req, Resp, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RpcServerBuilder").finish_non_exhaustive()
    }
}

/// Chooses a server's wire, clients and reservations during initialization.
pub struct RpcServerBuilder<'a, Req: 'static, Resp: 'static, W> {
    setup: &'a TransportSetup,
    method: UUri,
    wire: W,
    clients: &'a [UUri],
    options: RouteOptions,
    response_options: RouteOptions,
    _message: PhantomData<fn() -> (Req, Resp)>,
}

impl<Req: 'static, Resp: 'static> RpcServer<Req, Resp> {
    /// Starts declaring a server for this entity's method.
    pub fn on<'a, S: TransportSource>(
        source: &'a S,
        uri: &dyn LocalUriProvider,
        method: impl MethodResource<Req, Resp>,
    ) -> RpcServerBuilder<'a, Req, Resp, S::DefaultWire> {
        RpcServerBuilder {
            setup: source.setup(),
            method: uri.get_resource_uri(method.resource_id()),
            wire: source.default_wire(),
            clients: &[],
            options: RouteOptions::default(),
            response_options: RouteOptions {
                slots: 8,
                ..RouteOptions::default()
            },
            _message: PhantomData,
        }
    }
}

impl<'a, Req: 'static, Resp: 'static, W> RpcServerBuilder<'a, Req, Resp, W> {
    /// Chooses this server's wire. It is used for requests and responses.
    #[must_use]
    pub fn wire<W2>(self, wire: W2) -> RpcServerBuilder<'a, Req, Resp, W2> {
        RpcServerBuilder {
            setup: self.setup,
            method: self.method,
            wire,
            clients: self.clients,
            options: self.options,
            response_options: self.response_options,
            _message: PhantomData,
        }
    }

    /// Names this server's clients, from deployment data.
    ///
    /// Required on a binding that declares `declared_clients`. On any other
    /// binding it is optional and enforced the same way when given. A request
    /// from anyone else is refused on arrival, before it can take a peer-table
    /// entry, and an exact list sizes the table.
    ///
    /// On a table sized exactly to its clients, a client that is reported lost and
    /// sends again at once can cost another client one request: while the first
    /// client's two first requests race, they can briefly hold two entries. That
    /// request is refused `PeerTableFull` and counted in `refused`, and the next
    /// one is admitted (`PeerTable::admit`, R3-1).
    #[must_use]
    pub fn clients(mut self, clients: &'a [UUri]) -> Self {
        self.clients = clients;
        self
    }

    /// Sets how many requests the application may hold at once.
    #[must_use]
    pub fn max_held(mut self, held: usize) -> Self {
        self.options.max_held = held;
        self
    }

    /// Sets the most peers this server can track. Without a client list the
    /// default is 8; with an exact client list it is the list's length, and a
    /// value below that is refused at `build`.
    #[must_use]
    pub fn max_peers(mut self, peers: usize) -> Self {
        self.options.max_peers = peers;
        self.options.max_peers_set = true;
        self
    }

    /// Sets the QoS of the request route.
    #[must_use]
    pub fn qos(mut self, qos: Qos) -> Self {
        self.options.qos = qos;
        self
    }

    /// Sets the largest request payload. By default it comes from the wire.
    #[must_use]
    pub fn max_payload(mut self, bytes: usize) -> Self {
        self.options.max_payload = Some(bytes);
        self
    }

    /// Sets the largest response payload. By default it comes from the wire.
    #[must_use]
    pub fn max_response(mut self, bytes: usize) -> Self {
        self.response_options.max_payload = Some(bytes);
        self
    }

    /// Sets the number of response slots. The default is 8.
    #[must_use]
    pub fn slots(mut self, slots: usize) -> Self {
        self.response_options.slots = slots;
        self
    }

    /// Overrides the transport's default envelope, if the transport carries it.
    #[must_use]
    pub fn envelope(mut self, envelope: Envelope) -> Self {
        self.options.envelope = Some(envelope);
        self.response_options.envelope = Some(envelope);
        self
    }

    /// Reserves everything this server needs.
    ///
    /// # Errors
    /// Refuses the route for capability, naming it.
    pub fn build<M: WireMode>(self) -> Result<RpcServerSetup<Req, Resp, M>, StartupError>
    where
        W: RpcRoleWire<Req, Resp, M>,
        Req: Send + Sync,
        Resp: Send + Sync,
    {
        let Self {
            setup,
            method,
            wire,
            clients,
            mut options,
            response_options,
            _message,
        } = self;
        let memory = setup.memory();
        let capabilities = setup.capabilities();
        // The table holds every declared client. An exact list is the size; a
        // list with wildcards can match more peers, so it keeps the default floor.
        // An explicit size must fit the list.
        if !clients.is_empty() {
            if options.max_peers_set {
                if options.max_peers < clients.len() {
                    return Err(map_config(
                        "more clients are declared than .max_peers(..) allows",
                    ));
                }
            } else if clients
                .iter()
                .all(|client| client.check_no_wildcards().is_ok())
            {
                options.max_peers = clients.len();
            } else {
                options.max_peers = options.max_peers.max(clients.len());
            }
        }
        if capabilities.declared_clients && clients.is_empty() {
            return Err(StartupError::ClientsRequired {
                route: route_name(
                    RouteDirection::Receive,
                    Some(UMessageType::Request),
                    &UUri::any(),
                    Some(&method),
                ),
            });
        }
        let (req_info, resp_info) =
            <W as RpcRoleWire<Req, Resp, M>>::__describe(&wire).map_err(map_config)?;
        // The request side: one route, addressed to this method.
        let mut set: WireSet<Req> = WireSet::new();
        let (native_request, resp_typed) =
            match <W as RpcRoleWire<Req, Resp, M>>::__place(wire, memory)? {
                RpcWireKind::Encoded {
                    type_id,
                    request,
                    response,
                } => {
                    set.push(
                        req_info,
                        Some(TypedWire {
                            type_id,
                            wire: request,
                            info: req_info,
                        }),
                    );
                    let typed = TypedWire {
                        type_id,
                        wire: response,
                        info: resp_info,
                    };
                    (None, Some(typed))
                }
                RpcWireKind::InPlace => {
                    set.push(req_info, None);
                    (Some(native_identity(&req_info)), None)
                }
            };
        let native = native_request.is_some();
        let client_list: &'static [UUri] = {
            let mut list: FixedVec<UUri> = memory.fixed_vec(clients.len())?;
            for client in clients {
                list.push(client.clone())?;
            }
            list.into_static_slice()
        };
        let requests = place_receive(
            setup,
            UUri::any(),
            Some(method.clone()),
            &set,
            options,
            UMessageType::Request,
            native_request,
            client_list,
        )?;
        let preparation = setup.guard_composite_preparation();
        // The response side: one route per declared client, or one shared route.
        let resp_set: &'static [TypedWire<Resp>] = match resp_typed {
            Some(typed) => {
                let mut table: FixedVec<TypedWire<Resp>> = memory.fixed_vec(1)?;
                table.push(typed)?;
                table.into_static_slice()
            }
            None => &[],
        };
        let enforce = !clients.is_empty();
        let mut response_options = response_options;
        if response_options.priority.is_none() {
            response_options.priority = Some(UPriority::CS4);
        }
        let make_route = |client: Option<&UUri>| -> Result<&'static TxRoute, StartupError> {
            let mut infos = core::iter::once(&resp_info);
            let heaps = [(resp_info.encode_heap(), resp_info.name)];
            build_tx(
                TxPlan {
                    setup,
                    kind: UMessageType::Response,
                    source: method.clone(),
                    sink: client.cloned(),
                    options: response_options,
                    native,
                },
                &mut infos,
                1,
                &heaps,
            )
        };
        let mut client_routes: FixedVec<&'static TxRoute> = memory.fixed_vec(clients.len())?;
        let shared_route = if enforce {
            for client in clients {
                client_routes.push(make_route(Some(client))?)?;
            }
            None
        } else {
            Some(make_route(None)?)
        };
        let role = RpcServerSetup {
            transport: setup.id(),
            requests,
            core: ServerCore {
                enforce_clients: enforce,
                clients: client_list,
                client_routes: client_routes.into_static_slice(),
                shared_route,
                wires: resp_set,
                shared: setup.shared(),
            },
            _mode: PhantomData,
        };
        preparation.complete();
        Ok(role)
    }
}

impl<Req: Send + Sync + 'static, Resp: 'static, M: WireMode> RpcServerSetup<Req, Resp, M> {
    /// Binds this role to the running transport. Allocates nothing.
    ///
    /// # Errors
    /// Refuses a runtime other than the one this role was set up on.
    pub fn activate(
        self,
        runtime: &TransportRuntime,
    ) -> Result<RpcServer<Req, Resp, M>, StartupError> {
        const { assert!(!mem::needs_drop::<RpcServer<Req, Resp, M>>()) };
        check_transport(self.transport, runtime)?;
        Ok(RpcServer {
            requests: self.requests,
            core: self.core,
            _mode: PhantomData,
        })
    }
}

impl<Req: Send + Sync + 'static, Resp: 'static, M: WireMode> RpcServer<Req, Resp, M> {
    /// Takes the oldest request, if any. Non-blocking.
    ///
    /// A request from a client outside the deployment's list never reaches this
    /// queue: it is refused and counted (`unknown_client`) on arrival. One call
    /// examines at most the route's bound of frames, refused ones included.
    ///
    /// # Errors
    /// `MaxHeldExceeded` when every lease is held.
    pub fn try_recv(&self) -> Result<Option<Request<Req, Resp, M>>, ReceiveError> {
        let Some(received) = self.requests.try_receive()? else {
            return Ok(None);
        };
        let header: StaticHeader = received.header().with_text(None, None);
        let reply = received.reply();
        Ok(Some(Request {
            received,
            server: self.core,
            source: header.source.clone(),
            reqid: header.id.clone(),
            reply,
            ttl: header.ttl,
        }))
    }

    /// The request route's counters.
    #[must_use]
    pub fn counters(&self) -> RouteCounters {
        self.requests.common.counters.snapshot()
    }

    /// The request route's ID, for `TransportRuntime::counters`.
    #[must_use]
    pub fn route_id(&self) -> RouteId {
        self.requests.common.id
    }

    /// The route's waker storage, for the async layer.
    #[doc(hidden)]
    /// Returns the receive route’s process-lifetime wake storage. Only the owning receive
    /// adapter registers or clears it; binding threads may wake it. Register before waiting,
    /// poll again after registration and every wake (a hint), and clear on Recv drop. This
    /// accessor neither blocks nor uses the heap.
    // [impl->req~ssr-async-seam~1]
    pub fn __route_wake(&self) -> &'static crate::transport::wake::RouteWake {
        self.requests.__wake()
    }
}

/// A client for one RPC method on another entity.
pub struct RpcClient<Req: 'static, Resp: 'static, M: WireMode = Encoded> {
    request: &'static TxRoute,
    wires: &'static [TypedWire<Req>],
    responses: &'static ClientResponses<Resp>,
    ttl: Duration,
    _mode: PhantomData<fn() -> M>,
}

impl<Req: 'static, Resp: 'static, M: WireMode> Clone for RpcClient<Req, Resp, M> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Req: 'static, Resp: 'static, M: WireMode> Copy for RpcClient<Req, Resp, M> {}
impl<Req: 'static, Resp: 'static, M: WireMode> fmt::Debug for RpcClient<Req, Resp, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RpcClient").finish_non_exhaustive()
    }
}

/// An RPC client reserved during initialization.
pub struct RpcClientSetup<Req: 'static, Resp: 'static, M: WireMode = Encoded> {
    transport: u64,
    request: &'static TxRoute,
    wires: &'static [TypedWire<Req>],
    responses: &'static ClientResponses<Resp>,
    ttl: Duration,
    _mode: PhantomData<fn() -> M>,
}

impl<Req: 'static, Resp: 'static, M: WireMode> fmt::Debug for RpcClientSetup<Req, Resp, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RpcClientSetup").finish_non_exhaustive()
    }
}

impl<'a, Req: 'static, Resp: 'static, W> fmt::Debug for RpcClientBuilder<'a, Req, Resp, W> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RpcClientBuilder").finish_non_exhaustive()
    }
}

/// Chooses a client's wire and reservations during initialization.
pub struct RpcClientBuilder<'a, Req: 'static, Resp: 'static, W> {
    setup: &'a TransportSetup,
    client: UUri,
    method: UUri,
    wire: W,
    options: RouteOptions,
    max_in_flight: usize,
    max_response: Option<usize>,
    ttl: Duration,
    _message: PhantomData<fn() -> (Req, Resp)>,
}

impl<Req: 'static, Resp: 'static> RpcClient<Req, Resp> {
    /// Starts declaring a client for a method on another entity.
    pub fn on<'a, S: TransportSource>(
        source: &'a S,
        uri: &dyn LocalUriProvider,
        method: UUri,
    ) -> RpcClientBuilder<'a, Req, Resp, S::DefaultWire> {
        RpcClientBuilder {
            setup: source.setup(),
            client: uri.get_source_uri(),
            method,
            wire: source.default_wire(),
            options: RouteOptions::default(),
            max_in_flight: 4,
            max_response: None,
            ttl: Duration::from_secs(1),
            _message: PhantomData,
        }
    }
}

impl<'a, Req: 'static, Resp: 'static, W> RpcClientBuilder<'a, Req, Resp, W> {
    /// Chooses this client's wire. It is used for requests and responses.
    #[must_use]
    pub fn wire<W2>(self, wire: W2) -> RpcClientBuilder<'a, Req, Resp, W2> {
        RpcClientBuilder {
            setup: self.setup,
            client: self.client,
            method: self.method,
            wire,
            options: self.options,
            max_in_flight: self.max_in_flight,
            max_response: self.max_response,
            ttl: self.ttl,
            _message: PhantomData,
        }
    }

    /// Sets how many calls can be outstanding. The default is 4.
    #[must_use]
    pub fn max_in_flight(mut self, calls: usize) -> Self {
        self.max_in_flight = calls;
        self
    }

    /// Sets the largest response payload. By default it comes from the wire,
    /// and is 1024 bytes when the wire gives no bound.
    #[must_use]
    pub fn max_response(mut self, bytes: usize) -> Self {
        self.max_response = Some(bytes);
        self
    }

    /// Sets the call's time-to-live. The default is 1 s.
    #[must_use]
    pub fn ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Sets the largest request payload. By default it comes from the wire.
    #[must_use]
    pub fn max_payload(mut self, bytes: usize) -> Self {
        self.options.max_payload = Some(bytes);
        self
    }

    /// Sets the QoS of the request route.
    #[must_use]
    pub fn qos(mut self, qos: Qos) -> Self {
        self.options.qos = qos;
        self
    }

    /// Sets this client's priority. The default is CS4, the lowest RPC class.
    #[must_use]
    pub fn priority(mut self, priority: UPriority) -> Self {
        self.options.priority = Some(priority);
        self
    }

    /// Overrides the transport's default envelope, if the transport carries it.
    #[must_use]
    pub fn envelope(mut self, envelope: Envelope) -> Self {
        self.options.envelope = Some(envelope);
        self
    }

    /// Reserves everything this client needs.
    ///
    /// # Errors
    /// Refuses the route for capability, naming it.
    pub fn build<M: WireMode>(self) -> Result<RpcClientSetup<Req, Resp, M>, StartupError>
    where
        W: RpcRoleWire<Req, Resp, M>,
        Req: Send + Sync,
        Resp: Send + Sync,
    {
        let Self {
            setup,
            client,
            method,
            wire,
            mut options,
            max_in_flight,
            max_response,
            ttl,
            _message,
        } = self;
        let memory = setup.memory();
        let (req_info, resp_info) =
            <W as RpcRoleWire<Req, Resp, M>>::__describe(&wire).map_err(map_config)?;
        let (native, wires, typed, init_wire) =
            match <W as RpcRoleWire<Req, Resp, M>>::__place(wire, memory)? {
                RpcWireKind::Encoded {
                    type_id,
                    request,
                    response,
                } => {
                    let mut requests: FixedVec<TypedWire<Req>> = memory.fixed_vec(1)?;
                    requests.push(TypedWire {
                        type_id,
                        wire: request,
                        info: req_info,
                    })?;
                    let mut responses: FixedVec<TypedWire<Resp>> = memory.fixed_vec(1)?;
                    responses.push(TypedWire {
                        type_id,
                        wire: response,
                        info: resp_info,
                    })?;
                    let requests: &'static [TypedWire<Req>] = requests.into_static_slice();
                    let responses: &'static [TypedWire<Resp>] = responses.into_static_slice();
                    (false, requests, responses, Some(response))
                }
                RpcWireKind::InPlace => (true, &[][..], &[][..], None),
            };
        if options.priority.is_none() {
            options.priority = Some(UPriority::CS4);
        }
        options.ttl = Some(ttl);
        let request = {
            let mut infos = core::iter::once(&req_info);
            let heaps = [(req_info.encode_heap(), req_info.name)];
            build_tx(
                TxPlan {
                    setup,
                    kind: UMessageType::Request,
                    source: client.clone(),
                    sink: Some(method.clone()),
                    options,
                    native,
                },
                &mut infos,
                1,
                &heaps,
            )?
        };
        let preparation = setup.guard_composite_preparation();
        // The response side.
        let response_max = max_response.or(resp_info.max_encoded_len).unwrap_or(1024);
        // Every outstanding call can have a response waiting at once. A shallower
        // queue would evict responses on a lending transport, and their calls
        // would end only at their deadlines.
        let response_depth = u16::try_from(max_in_flight).unwrap_or(u16::MAX).max(1);
        let response_options = RouteOptions {
            max_payload: Some(response_max),
            qos: Qos::DEFAULT.with_depth(response_depth),
            ..RouteOptions::default()
        };
        let name = route_name(
            RouteDirection::Receive,
            Some(UMessageType::Response),
            &method,
            Some(&client),
        );
        let mut infos = core::iter::once(&resp_info);
        let rx_kind = route_kind(RouteDirection::Receive, Some(UMessageType::Response));
        let admission = admit_route(
            setup,
            &response_options,
            &name,
            &mut infos,
            1,
            rx_kind,
            native,
        )?;
        let id = setup.next_route_id()?;
        let inbound = setup.next_inbound()?;
        let prepared = setup.prepare_route(&RouteDeclaration {
            lifetime_policy: setup.shared().lifetime_policy(),
            direction: RouteDirection::Receive,
            kind: Some(UMessageType::Response),
            source: &method,
            sink: Some(&client),
            inbound: Some(inbound),
            envelope: admission.envelope,
            encodings: admission.encodings.as_slice(),
            families: admission.families.as_slice(),
            max_metadata_len: admission.head,
            max_payload_len: admission.max_payload,
            payload_alignment: admission.alignment,
            slots: max_in_flight,
            qos: response_options.qos,
            native,
            max_peers: response_options.max_peers,
        })?;
        let peers = PeerTable::new_in(memory, response_options.max_peers)?;
        let name_ref: &'static RouteName = memory.place(name)?;
        let lending = setup.family() == Family::Lending;
        let prepared_table = CallTable::<Resp>::prepare(
            memory,
            max_in_flight,
            response_max,
            lending,
            typed,
            init_wire,
            setup.shared().lifetime_policy(),
        )?;
        let common = RxCommon::new(
            name_ref,
            id,
            prepared.route,
            Some(UMessageType::Response),
            method.clone(),
            Some(client.clone()),
            &[],
            admission.envelope,
            admission.admitted,
            HeaderLimits { max_token_len: 0 },
            peers,
            response_options.qos,
            prepared.heap,
            prepared.deviations,
            setup.shared(),
        );
        let prepared_responses = prepared_table.with_responses(memory, common)?;
        setup.check_route(crate::transport::RouteView {
            report: prepared_responses.report(),
            wires: admission.infos,
            max_peers: response_options.max_peers,
            clients: &[],
            ttl: Some(ttl),
            max_in_flight,
        })?;
        let responses = prepared_responses.install(setup)?;
        let role = RpcClientSetup {
            transport: setup.id(),
            request,
            wires,
            responses,
            ttl,
            _mode: PhantomData,
        };
        preparation.complete();
        Ok(role)
    }
}

impl<Req: 'static, Resp: Send + Sync + 'static, M: WireMode> RpcClientSetup<Req, Resp, M> {
    /// Binds this client to the running transport. Allocates nothing.
    ///
    /// # Errors
    /// Refuses a runtime other than the one this role was set up on.
    pub fn activate(
        self,
        runtime: &TransportRuntime,
    ) -> Result<RpcClient<Req, Resp, M>, StartupError> {
        const { assert!(!mem::needs_drop::<RpcClient<Req, Resp, M>>()) };
        check_transport(self.transport, runtime)?;
        Ok(RpcClient {
            request: self.request,
            wires: self.wires,
            responses: self.responses,
            ttl: self.ttl,
            _mode: PhantomData,
        })
    }
}

impl<Req: 'static, Resp: Send + Sync + 'static> RpcClient<Req, Resp> {
    /// Sends a request. The call slot is reserved before the request is sent.
    ///
    /// # Errors
    /// `SlotsExhausted` when no call slot is free, `NotSent` on a send refusal.
    pub fn invoke(&self, request: &Req) -> Result<Call<Resp>, CallError> {
        let route = self.request;
        let id = route.next_id().map_err(CallError::NotSent)?;
        let call = self.reserve(&id)?;
        let (Some(wire), Some(admitted)) = (self.wires.first(), route.admitted.first()) else {
            return Err(CallError::NotSent(SendError::WireNotAdmitted));
        };
        let fields = MessageFields {
            id: Some(&id),
            ..MessageFields::default()
        };
        send_with(route, wire, admitted, &fields, request).map_err(CallError::NotSent)?;
        Ok(call)
    }
}

impl<Req: StablePayloadInit, Resp: Send + Sync + 'static> RpcClient<Req, Resp, InPlace> {
    /// Initializes the request directly in the transport's loan and sends it
    /// (R18). The call slot is reserved first.
    ///
    /// # Errors
    /// `SlotsExhausted` when no call slot is free, `NotSent` on a send refusal,
    /// including a witness that does not belong to this loan.
    pub fn invoke_in_place<F>(&self, fill: F) -> Result<Call<Resp, InPlace>, CallError>
    where
        F: for<'a> FnOnce(InPlaceInit<'a, Req>) -> InitializedStablePayload<'a, Req>,
    {
        let route = self.request;
        let id = route.next_id().map_err(CallError::NotSent)?;
        let call = self.reserve(&id)?;
        send_in_place_with::<Req, F>(route, &id, &MessageFields::default(), None, fill)
            .map_err(CallError::NotSent)?;
        Ok(call)
    }
}

impl<Req: 'static, Resp: Send + Sync + 'static, M: WireMode> RpcClient<Req, Resp, M> {
    /// Claims a call slot for `id`, before anything is sent.
    fn reserve(&self, id: &UUID) -> Result<Call<Resp, M>, CallError> {
        let shared = self.request.shared;
        let deadline = deadline_after(shared.platform, self.ttl);
        // A response-table observation does not identify this send's recipient.
        // Neither handoff send nor lending publish binds a destination before
        // publication. Keep the call unbound even when one peer was observed:
        // loss/restart cannot terminate it; response correlation and TTL still can.
        let peer = crate::transport::binding::PeerRef::UNKNOWN;
        let table = self.responses.table;
        let index = table
            .reserve(id, deadline, peer)?;
        // SAFETY: table.reserve just transferred the sole in-range live index
        // obligation. This same RpcClient owns its initialized response/shared
        // context; no other owner adopted or returned the index. Call consumes it.
        Ok(unsafe { Call::from_reserved(table, self.responses, shared, index) })
    }

    /// The request route's counters.
    #[must_use]
    pub fn counters(&self) -> RouteCounters {
        self.request.counters.snapshot()
    }

    /// The response route's counters.
    #[must_use]
    pub fn response_counters(&self) -> RouteCounters {
        self.responses.common.counters.snapshot()
    }

    /// The request route's ID, for `TransportRuntime::counters`.
    #[must_use]
    pub fn route_id(&self) -> RouteId {
        self.request.id
    }

    /// The call time-to-live fixed at setup, for the async layer's timer.
    #[doc(hidden)]
    #[must_use]
    /// Returns the configured TTL by value, valid independently of this borrow. May be read on
    /// any thread allowed by the client handle. It neither blocks nor uses the heap; the
    /// executor must still poll the call at expiry.
    // [impl->req~ssr-async-seam~1]
    pub fn __ttl(&self) -> Duration {
        self.ttl
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod whitebox {
    use super::*;
    use crate::memory::StartupRegion;
    use crate::transport::tests::{NullHandoff, TestUri, TestWire, CLOCK};
    use crate::wire::WireError;

    static REGION: StartupRegion<{ 1 << 18 }> = StartupRegion::new();

    /// L4. A token written into a slot is wiped on every way out of a failed send:
    /// a send that dies in `encode` used to leave it in the slot.
    #[test]
    fn a_token_is_wiped_from_the_slot_when_the_send_fails() {
        let memory = REGION.claim().unwrap();
        let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
        let provider = TestUri(1);
        let setup_role = Publisher::<u32>::on(&setup, &provider, 0x8001_u16)
            .wire(TestWire { fail_encode: true })
            .max_token_len(16)
            .build()
            .unwrap();
        let runtime = setup.enter_runtime().unwrap();
        let publisher = setup_role.activate(&runtime).unwrap();
        let fields = MessageFields {
            token: Some("SECRET-TOKEN"),
            ..MessageFields::default()
        };
        assert_eq!(
            publisher.publish_with(&fields, &7),
            Err(SendError::Wire(WireError::Unencodable(
                "test wire refuses to encode"
            )))
        );
        let pool: &'static crate::transport::slots::SlotPool =
            publisher.core.route.value_slots.as_ref().unwrap();
        let slot = pool.acquire().expect("the send gave the slot back");
        assert!(
            !slot.bytes().windows(6).any(|window| window == b"SECRET"),
            "the token must not survive in the slot after a failed send"
        );
    }

    static UNWIND_REGION: StartupRegion<{ 1 << 18 }> = StartupRegion::new();

    /// R2-4, the send side. A handoff send holds its value slot under a guard that
    /// clears the slot unless the send returned, so a wire's `encode` that unwinds
    /// leaves no token in the slot it gives back (L4). `core` is `no_std` and cannot
    /// catch an unwind, so the guard is dropped here as an unwind drops it.
    #[test]
    fn a_value_slot_is_wiped_unless_its_send_returned() {
        use crate::transport::mode::WipeUnlessReturned;
        let memory = UNWIND_REGION.claim().unwrap();
        let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap();
        let setup_role = Publisher::<u32>::on(&setup, &TestUri(1), 0x8001_u16)
            .wire(TestWire { fail_encode: false })
            .build()
            .unwrap();
        let runtime = setup.enter_runtime().unwrap();
        let publisher = setup_role.activate(&runtime).unwrap();
        let pool: &'static crate::transport::slots::SlotPool =
            publisher.core.route.value_slots.as_ref().unwrap();
        let mut slot = pool.acquire().expect("a free value slot");
        slot.bytes_mut()[..6].copy_from_slice(b"SECRET");
        WipeUnlessReturned(&mut slot).returned();
        assert_eq!(
            &slot.bytes()[..6],
            b"SECRET",
            "a send that returned clears its own way"
        );
        drop(WipeUnlessReturned(&mut slot));
        assert!(
            slot.bytes().iter().all(|byte| *byte == 0),
            "an unwound send leaves nothing in the slot"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod retirement_integration_tests {
    extern crate std;
    use std::{alloc::{GlobalAlloc, Layout, System}, cell::Cell, thread_local};
    use crate::transport::{Frame, LoanedPayload, PayloadLoanProvenance, BindingSetup, PeerRef};
    use crate::transport::pool::{IndexPool, IndexPoolSetup, IndexSlot, IndexLease};
    use crate::platform::atomic::{AtomicU32, Ordering::SeqCst};
    use super::*;
    use crate::memory::StartupRegion;
    use crate::transport::tests::{NullHandoff, PushLending, TestUri, TestWire, CLOCK, whole_frame, StaticFrame};
    use crate::transport::{LifetimePolicy, LifetimeState, PoolError, ReceiveError, RetainedFrame};
    // [utest->req~ssr-runtime-no-heap~1]
    // [utest->req~ssr-release-lease-drop~2]
    #[test]
    fn retirement_integration_send_preserves_input_and_diagnostics() {
        static REGION: StartupRegion<{ 1 << 18 }> = StartupRegion::new();
        let memory = REGION.claim().unwrap();
        let setup = TransportSetup::handoff_on(&memory, &CLOCK, NullHandoff).unwrap()
            .with_lifetime_policy(LifetimePolicy::new(2).unwrap()).unwrap();
        let role = Publisher::<u32>::on(&setup, &TestUri(1), 0x8001_u16).wire(TestWire { fail_encode: false }).build().unwrap();
        let id = role.route.id;
        setup.__seed_pool_free_for_test(0, u64::MAX - 5).unwrap();
        let runtime = setup.enter_runtime().unwrap();
        let role = role.activate(&runtime).unwrap();
        let input = 123_u32;
        assert_eq!(runtime.pool_lifetime(id), Some(LifetimeState::Healthy));
        assert!(role.publish(&input).is_ok());
        assert_eq!(runtime.pool_lifetime(id), Some(LifetimeState::LifetimeMarginLow));
        assert!(role.publish(&input).is_ok());
        assert_eq!(runtime.pool_lifetime(id), Some(LifetimeState::Retired));
        assert_eq!(role.publish(&input), Err(SendError::PoolRetired));
        assert_eq!(input, 123);
        assert_eq!(runtime.counters(id).slots_exhausted, 0);
        assert_eq!(runtime.routes()[0].lifetime_policy.warning_remaining(), 2);
    }
    #[test]
    fn retirement_integration_received_owner_drains_after_closure() {
        static REGION: StartupRegion<{ 1 << 18 }> = StartupRegion::new();
        let memory = REGION.claim().unwrap();
        let topic = TestUri(1).get_resource_uri(0x8001);
        let a = whole_frame(&memory, &topic, &17_u32.to_le_bytes());
        let b = whole_frame(&memory, &topic, &29_u32.to_le_bytes());
        let setup = TransportSetup::lending_on(&memory, &CLOCK, PushLending).unwrap();
        let role = Subscriber::<u32>::on(&setup, topic).wire(TestWire { fail_encode: false }).max_held(2).build().unwrap();
        setup.__seed_pool_free_for_test(0, u64::MAX - 3).unwrap();
        let route = role.route;
        let id = route.common.id;
        let runtime = setup.enter_runtime().unwrap();
        let role = role.activate(&runtime).unwrap();
        route.deliver_retained(RetainedFrame::new(StaticFrame { bytes: a, peer: PeerRef::UNKNOWN })).unwrap();
        let first = role.try_recv().unwrap().unwrap();
        route.deliver_retained(RetainedFrame::new(StaticFrame { bytes: b, peer: PeerRef::UNKNOWN })).unwrap();
        let held = role.try_recv().unwrap().unwrap();
        drop(first); // terminal hot slot closes, second decoded owner remains held
        assert_eq!(runtime.pool_lifetime(id), Some(LifetimeState::Retired));
        assert_eq!(*held, 29);
        drop(held);
        assert!(matches!(role.try_recv(), Err(ReceiveError::PoolRetired)));
        assert_eq!(runtime.counters(id).slots_exhausted, 0);
        assert_eq!(runtime.receive_fault(id), Some(ReceiveError::PoolRetired));
    }
    #[test]
    fn retirement_integration_full_does_not_latch_terminal() {
        static REGION: StartupRegion<{ 1 << 18 }> = StartupRegion::new();
        let memory = REGION.claim().unwrap();
        let pool = memory.place(crate::transport::slots::SlotPool::new_in(&memory, 65, 8).unwrap()).unwrap();
        let mut leases = [const { None };65];
        for (i, slot) in leases.iter_mut().enumerate() { let mut lease = pool.acquire().unwrap(); lease.bytes_mut()[0] = i as u8; *slot = Some(lease); }
        assert!(matches!(pool.acquire(), Err(PoolError::Full)));
        drop(leases[32].take());
        let mut next = pool.acquire().unwrap(); assert_eq!(next.index(), 32); next.bytes_mut().fill(7);
        assert_eq!(leases[31].as_ref().unwrap().bytes()[0], 31);
        assert_eq!(leases[33].as_ref().unwrap().bytes()[0], 33);
    }
struct CountHeap;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct HeapCounts {
    active: bool,
    allocs: usize,
    frees: usize,
    saturated: bool,
}
impl HeapCounts {
    /// # Safety-usable invariant
    /// Performs only bounded scalar operations, with no panic, allocation,
    /// destructor, callback or allocator reentry. Counters never wrap; losing
    /// exactness latches saturated. Inactive measurements remain unchanged.
    fn record(&mut self, allocs: usize, frees: usize) {
        if self.active {
            self.saturated |= self.allocs.checked_add(allocs).is_none()
                || self.frees.checked_add(frees).is_none();
            self.allocs = self.allocs.saturating_add(allocs);
            self.frees = self.frees.saturating_add(frees);
        }
    }
    fn is_quiet(self, unavailable: bool) -> bool {
        !unavailable && !self.saturated && self.allocs == 0 && self.frees == 0
    }
}
thread_local! {
    static HEAP_COUNTS: Cell<HeapCounts> = const { Cell::new(HeapCounts {
        active: false, allocs: 0, frees: 0, saturated: false,
    }) };
}
// A failed TLS access may conceal an active window. Conservatively poison all
// later observations; never clear this process-wide latch to manufacture a pass.
static HEAP_UNAVAILABLE: crate::platform::atomic::AtomicBool = crate::platform::atomic::AtomicBool::new(false);

/// # Safety-usable invariant
/// Cannot unwind or invoke caller code: try_with uses a const no-drop Cell
/// initializer, the local closure uses Copy Cell operations and HeapCounts::record,
/// and an unavailable key only performs a valid Relaxed AtomicBool store.
/// No allocation, pointer access or allocator recursion is introduced here.
fn count_heap(allocs: usize, frees: usize) {
    if HEAP_COUNTS.try_with(|cell| {
        let mut counts = cell.get();
        counts.record(allocs, frees);
        cell.set(counts);
    }).is_err() {
        HEAP_UNAVAILABLE.store(true, crate::platform::atomic::Ordering::Relaxed);
    }
}

// SAFETY: GlobalAlloc requires matching layouts/ownership and no unwinding.
// Every allocation operation forwards its original arguments directly to System,
// so blocks retain System identity and its success/failure ownership semantics.
// count_heap's safety-usable invariant establishes nonpanicking instrumentation;
// System implements GlobalAlloc's no-unwind contract. No hook calls user code.
// The inherited alloc_zeroed uses this alloc plus the trait's zeroing default.
// Measurements are host observations, never a premise that an allocation occurs.
unsafe impl GlobalAlloc for CountHeap {
    /// # Safety
    /// The caller must satisfy [`GlobalAlloc::alloc`]. Success returns a new
    /// System-backed block; null transfers no ownership.
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_heap(1, 0);
        // SAFETY: GlobalAlloc::alloc's caller supplies a nonzero valid Layout.
        // The unchanged size/alignment reach System; its result transfers the
        // block to the caller or returns null without creating an owner.
        unsafe { System.alloc(layout) }
    }
    /// # Safety
    /// The caller must satisfy [`GlobalAlloc::dealloc`]. This consumes the
    /// allocation with its exact layout; no owner remains.
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count_heap(0, 1);
        // SAFETY: GlobalAlloc::dealloc's caller supplies a currently allocated
        // block from this allocator and its exact allocation Layout. All blocks
        // originate from System; the hook neither accesses nor changes them.
        // Forwarding consumes that allocation once, with no later pointer use.
        unsafe { System.dealloc(ptr, layout) }
    }
    /// # Safety
    /// The caller must satisfy [`GlobalAlloc::realloc`]. Nonnull success transfers
    /// the old allocation and returns its replacement; null preserves the old one.
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count_heap(1, 1); // operation attempts, even when realloc returns null
        // SAFETY: GlobalAlloc::realloc's caller supplies a live block and exact
        // old Layout, nonzero new_size and the required isize-bounded rounded
        // size. These unchanged arguments satisfy System's identical contract.
        // Nonnull success consumes the old allocation; null leaves it owned by
        // the caller. No pointer arithmetic, reference or extra owner is made.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}
#[global_allocator] static HEAP: CountHeap = CountHeap;
struct HeapWindow;
impl Drop for HeapWindow {
    fn drop(&mut self) {
        if HEAP_COUNTS.try_with(|cell| {
            let mut counts = cell.get();
            counts.active = false;
            cell.set(counts);
        }).is_err() {
            HEAP_UNAVAILABLE.store(true, crate::platform::atomic::Ordering::Relaxed);
        }
    }
}
fn measure_heap(f: impl FnOnce()) -> HeapCounts {
    HEAP_COUNTS.with(|cell| {
        assert!(!cell.get().active, "heap measurement windows must not nest");
        cell.set(HeapCounts { active: true, ..HeapCounts::default() });
    });
    let guard = HeapWindow;
    f();
    drop(guard); // also disables measurement if f unwinds
    HEAP_COUNTS.with(Cell::get)
}
fn heap_window(f: impl FnOnce()) {
    let counts = measure_heap(f);
    let unavailable = HEAP_UNAVAILABLE.load(crate::platform::atomic::Ordering::Relaxed);
    // Diagnostics and assertion failure occur after the window, never inside
    // a GlobalAlloc callback. Unknown/saturated measurements cannot pass.
    assert!(counts.is_quiet(unavailable), "heap activity or invalid measurement: {counts:?}, unavailable={unavailable}");
}

// [utest->req~ssr-runtime-no-heap~1] Instrumentation control, host observations only.
#[test]
fn heap_counter_overflow_and_unavailable_cannot_pass() {
    let mut counts = HeapCounts { active: true, allocs: usize::MAX, frees: usize::MAX, saturated: false };
    counts.record(1, 1);
    assert_eq!((counts.allocs, counts.frees), (usize::MAX, usize::MAX));
    assert!(counts.saturated);
    assert!(!counts.is_quiet(false));
    assert!(!HeapCounts::default().is_quiet(true));
    assert!(HeapCounts::default().is_quiet(false));
    let mut inactive = HeapCounts::default();
    inactive.record(usize::MAX, usize::MAX);
    assert_eq!(inactive, HeapCounts::default());
}

// [utest->req~ssr-runtime-no-heap~1] A fixture unwind must not contaminate later windows.
#[test]
fn heap_window_disables_on_unwind() {
    let result = std::panic::catch_unwind(|| measure_heap(|| { let observed = core::hint::black_box(false); assert!(observed, "fixture unwind"); }));
    assert!(result.is_err());
    assert!(!HEAP_COUNTS.with(Cell::get).active);
    heap_window(|| {});
}

// [utest->req~ssr-runtime-no-heap~1] Exact forwarding/control observation, not a universal allocation claim.
#[test]
fn heap_forwarding_observes_all_three_operations() {
    use crate::test_support::FixtureResult as _;
    let layout = Layout::from_size_align(8, 8).require_fixture();
    let grown = Layout::from_size_align(16, 8).require_fixture();
    let counts = measure_heap(|| {
        // SAFETY: layout is a valid nonzero Layout; this acquires a System-backed
        // block or null. No reference is created before checking success.
        let original = unsafe { HEAP.alloc(layout) };
        assert!(!original.is_null(), "fixture allocation failed");
        // SAFETY: original is the live uniquely owned allocation just returned
        // by HEAP with exact layout; 16 is nonzero and fits the rounded bound.
        let resized = unsafe { HEAP.realloc(original, layout, 16) };
        if resized.is_null() {
            // SAFETY: failed realloc retains original and its unchanged layout;
            // this is its sole deallocation and no borrow or later access exists.
            unsafe { HEAP.dealloc(original, layout) };
        } else {
            // SAFETY: successful realloc transfers ownership to resized with
            // size 16 and original alignment 8. grown is that exact layout;
            // original is no longer used and this consumes the sole owner.
            unsafe { HEAP.dealloc(resized, grown) };
        }
        assert!(!resized.is_null(), "fixture reallocation failed");
    });
    assert_eq!((counts.allocs, counts.frees), (2, 2));
    assert!(!counts.saturated);
    assert!(!HEAP_UNAVAILABLE.load(crate::platform::atomic::Ordering::Relaxed));
}

    // This is an SDK-owned retained frame, not an iceoryx2 Sample. Genuine
    // native destructor evidence is a separate binding qualification obligation.
    #[repr(transparent)]
    struct Scalar(u32);
    // SAFETY: transparent u32, every bit valid, no padding/pointers/drop glue.
    unsafe impl crate::payload::stable::StablePayloadField for Scalar {
        const FIELD_BITS_ALWAYS_VALID: bool = true;
        const DESCRIPTOR: &'static crate::payload::stable::StableDescriptor = &crate::payload::stable::StableDescriptor {
            name: "test.retirement.Scalar", revision: 0, size: 4, alignment: 4, fields: &[],
        };
        fn validate_field_bytes(bytes: &[u8]) -> bool { bytes.len() == 4 }
    }
    // SAFETY: exact layout and bit validator above.
    unsafe impl StablePayload for Scalar { const TYPE_NAME: &'static str = "test.retirement.Scalar"; }

    fn native_header(memory: &crate::memory::StartupMemory, topic: &UUri)
        -> (&'static crate::payload::agreement::NativeAgreement, &'static [u8])
    {
        use crate::payload::agreement::*;
        use crate::frame::header::{TxHeaderParts, header_len, write_header_uninit};
        let mut agreement = NativeAgreement {domain:"test", version:1, encoding:crate::transport::tests::ENCODING,
            target_triple:"test-host", pointer_width:usize::BITS as u8, little_endian:cfg!(target_endian="little"), entries:&[]};
        let digest = descriptor_digest::<Scalar>(&agreement);
        let entries = memory.place([NativeAgreementEntry {type_name:Scalar::TYPE_NAME, revision:0, digest}]).unwrap();
        agreement.entries = entries;
        let agreement = memory.place(agreement).unwrap();
        let identity = agreement.resolve::<Scalar>().unwrap();
        let id=UUID::try_from_wire_pair((crate::transport::tests::WALL_MS<<16)|0x7000,0x8000_0000_0000_0001).unwrap();
        let parts=TxHeaderParts {kind:UMessageType::Publish,priority:None,id:&id,reqid:None,ttl:None,comm_status:None,
            permission_level:None,source:topic,sink:None,token:None,traceparent:None,payload_encoding:Some(identity.encoding),native_type_token:identity.native_type_token};
        let mut prefix=[MaybeUninit::uninit();256];
        let len=crate::wire::render_selected_prefix(crate::wire::STABLE_CONTAINER_WIRE_ID,crate::wire::STABLE_CONTAINER_PAYLOAD_FAMILY_ID,&mut prefix).unwrap();
        // SAFETY: render_selected_prefix initialized exactly len bytes.
        let prefix=unsafe {core::slice::from_raw_parts(prefix.as_ptr().cast::<u8>(),len)};
        let bytes=memory.bytes(header_len(prefix,&parts).unwrap()).unwrap();
        write_header_uninit(prefix,&parts,bytes).unwrap();
        // SAFETY: writer initialized this exact extent in process-lifetime memory.
        let bytes=unsafe {core::slice::from_raw_parts(bytes.as_ptr().cast::<u8>(),bytes.len())};
        (agreement,bytes)
    }
    struct NativePush { fault: bool }
    impl BindingSetup for NativePush {
        type Running=Self;
        fn name(&self)->&'static str {"retirement-native-facade"}
        fn capabilities(&self)->crate::transport::Capabilities {
            crate::transport::Capabilities::new(Envelope::UFrameFields,&[Envelope::UFrameFields],1<<20)
                .with_per_message_encoding(true).with_truncate_payload(true)
        }
        fn qos_support(&self,k:RouteKind)->crate::transport::QosSupport {BindingSetup::qos_support(&PushLending,k)}
        fn prepare_route(&mut self,r:&RouteDeclaration<'_>)->Result<crate::transport::PreparedRoute,crate::transport::BindingRefusal> {
            BindingSetup::prepare_route(&mut PushLending,r)
        }
        fn into_running(self,_:crate::transport::RuntimePort)->Result<Self,crate::transport::BindingRefusal> {Ok(self)}
    }
    impl crate::transport::LendingRuntime for NativePush {
        type Tx<'a>=crate::transport::tests::NoLoan;
        type Rx=StaticFrame;
        fn loan(&self,_:crate::transport::BindingRoute,_:crate::transport::TxFrameLayout)->Result<Self::Tx<'_>,crate::transport::LoanError> {Err(crate::transport::LoanError::PoolRetired)}
        fn publish(&self,_:crate::transport::BindingRoute,_:crate::transport::InitializedTx<Self::Tx<'_>>)->Result<SendReport,SendError> {Err(SendError::PoolRetired)}
        fn receive(&self,_:crate::transport::BindingRoute)->Option<Self::Rx> {None}
        fn receive_bounded(&self,_:crate::transport::BindingRoute,b:&crate::transport::ReceiveBudget)->Option<Self::Rx> {let _=b.try_take();None}
        fn receive_fault(&self,_:crate::transport::BindingRoute)->Option<ReceiveError> {self.fault.then_some(ReceiveError::PoolRetired)}
        fn pool_lifetime(&self,_:crate::transport::BindingRoute)->LifetimeState {if self.fault {LifetimeState::Retired} else {LifetimeState::Healthy}}
    }
    struct Tail { pool:&'static IndexPool<'static>, drops:&'static AtomicU32 }
    impl Drop for Tail { fn drop(&mut self) {
        // Every pool slot is still owned in this probe. Returning credit from
        // the outer destructor (before fields) would make this assertion fail.
        assert!(matches!(self.pool.try_acquire(),Err(PoolError::Full)|Err(PoolError::PoolRetired)));
        self.drops.fetch_add(1,SeqCst);
    }}
    struct NativeOwner {
        metadata:&'static [u8], value:Scalar, not_sync:Cell<u32>,
        _tail:Tail,
        // Rust drops fields in declaration order. This original credit must be
        // last, after the complete outer destructor and nontrivial tail field.
        credit:IndexLease<'static,'static>,
    }
    impl Frame for NativeOwner {
        fn encoded_metadata(&self)->&[u8] {self.not_sync.set(self.not_sync.get()+1);self.metadata}
        fn payload(&self)->Option<LoanedPayload<'_>> {
            // SAFETY: Scalar is fully initialized plain data, borrowed with self.
            let bytes=unsafe {core::slice::from_raw_parts(core::ptr::from_ref(&self.value).cast::<u8>(),4)};
            Some(LoanedPayload::new(bytes,PayloadLoanProvenance::OpaqueTransportLoan))
        }
        fn peer(&self)->PeerRef {PeerRef::UNKNOWN}
    }
    impl Drop for NativeOwner {fn drop(&mut self) {assert!(self.not_sync.get()>0);let _=self.credit.index();}}

    #[test]
    fn retirement_integration_native_owner_and_field_drop_across_closure() {
        static REGION:StartupRegion<{1<<18}>=StartupRegion::new();
        static DROPS:AtomicU32=AtomicU32::new(0);
        let memory=REGION.claim().unwrap();let topic=TestUri(1).get_resource_uri(0x8001);
        let (agreement,metadata)=native_header(&memory,&topic);
        let cells=memory.place([IndexSlot::new()]).unwrap();
        let native=memory.place(IndexPoolSetup::new(cells).unwrap().enter_runtime()).unwrap();
        let cells2=memory.place([IndexSlot::new()]).unwrap();
        let native2=memory.place(IndexPoolSetup::new(cells2).unwrap().enter_runtime()).unwrap();
        let setup=TransportSetup::lending_on(&memory,&CLOCK,NativePush {fault:false}).unwrap();
        let role=Subscriber::<Scalar>::on(&setup,topic).wire(crate::wire::NativeWire::new(agreement)).max_held(2).build().unwrap();
        setup.__seed_pool_free_for_test(0,u64::MAX-3).unwrap();
        let route=role.route;let runtime=setup.enter_runtime().unwrap();let role=role.activate(&runtime).unwrap();
        heap_window(|| {
            let owner=NativeOwner {metadata,value:Scalar(73),not_sync:Cell::new(0),_tail:Tail {pool:native,drops:&DROPS},credit:native.try_acquire().unwrap()};
            route.deliver_retained(RetainedFrame::new(owner)).unwrap();
            let first=role.try_recv().unwrap().unwrap();
            let owner2=NativeOwner {metadata,value:Scalar(91),not_sync:Cell::new(0),_tail:Tail {pool:native2,drops:&DROPS},credit:native2.try_acquire().unwrap()};
            route.deliver_retained(RetainedFrame::new(owner2)).unwrap();
            let held=role.try_recv().unwrap().unwrap();
            assert_eq!(first.payload().unwrap().0,73);
            assert_eq!(DROPS.load(SeqCst),0);
            drop(first); // outer + tail + native credit, then SDK slot closes
            assert_eq!(DROPS.load(SeqCst),1);
            assert_eq!(runtime.pool_lifetime(role.route_id()),Some(LifetimeState::Retired));
            assert_eq!(held.payload().unwrap().0,91);
            assert!(matches!(native2.try_acquire(),Err(PoolError::Full)));
            let successor=native.try_acquire().unwrap();assert_eq!(successor.index(),0);drop(successor);
            drop(held);assert_eq!(DROPS.load(SeqCst),2);drop(native2.try_acquire().unwrap());
        });
    }
    #[test]
    fn retirement_integration_binding_fault_is_not_empty() {
        static REGION:StartupRegion<{1<<18}>=StartupRegion::new();
        let memory=REGION.claim().unwrap();let topic=TestUri(1).get_resource_uri(0x8001);
        let (agreement,_)=native_header(&memory,&topic);
        let setup=TransportSetup::lending_on(&memory,&CLOCK,NativePush {fault:true}).unwrap();
        let role=Subscriber::<Scalar>::on(&setup,topic).wire(crate::wire::NativeWire::new(agreement)).build().unwrap();
        let runtime=setup.enter_runtime().unwrap();let role=role.activate(&runtime).unwrap();
        heap_window(|| {
            assert!(matches!(role.try_recv(),Err(ReceiveError::PoolRetired)));
            assert_eq!(runtime.receive_fault(role.route_id()),Some(ReceiveError::PoolRetired));
            assert_eq!(runtime.pool_lifetime(role.route_id()),Some(LifetimeState::Retired));
            assert_eq!(runtime.counters(role.route_id()).slots_exhausted,0);
        });
    }
    #[test]
    fn retirement_integration_hot_slot_concurrent_warning_is_sticky() {
        let mut cells=[IndexSlot::new()];
        let mut setup=IndexPoolSetup::new(&mut cells).unwrap().with_lifetime_policy(LifetimePolicy::new(16).unwrap());
        setup.__seed_free_for_test(0,u64::MAX-129).unwrap();let pool=setup.enter_runtime();
        let observed=AtomicU32::new(0);
        std::thread::scope(|s| {
            for _ in 0..2 {let p=&pool;let o=&observed;s.spawn(move|| heap_window(|| {
                loop {match p.try_acquire() {
                    Ok(v)=>{if p.lifetime()==LifetimeState::LifetimeMarginLow {o.fetch_add(1,SeqCst);}drop(v);},
                    Err(PoolError::Full)=>{}, Err(PoolError::PoolRetired)=>break,
                }}
            }));}
        });
        assert!(observed.load(SeqCst)>0);assert_eq!(pool.lifetime(),LifetimeState::Retired);
        assert!(matches!(pool.try_acquire(),Err(PoolError::PoolRetired)));
    }
    #[test]
    fn retirement_integration_pull_reservation_is_returned_on_closure_race() {
        static REGIONS:[StartupRegion<{1<<19}>;2]=[const {StartupRegion::new()};2];
        for (region,lending) in REGIONS.iter().zip([false,true]) {
            let memory=region.claim().unwrap();let topic=TestUri(1).get_resource_uri(0x8001);
            let bytes=whole_frame(&memory,&topic,&11_u32.to_le_bytes());
            let setup=if lending {TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap()}
                else {TransportSetup::handoff_on(&memory,&CLOCK,NullHandoff).unwrap()};
            let role=Subscriber::<u32>::on(&setup,topic.clone()).wire(TestWire {fail_encode:false})
                .qos(Qos::DEFAULT.with_depth(1).with_overflow(crate::transport::Overflow::DropNewest)).build().unwrap();
            setup.__seed_pool_free_for_test(0,u64::MAX-3).unwrap();
            let route=role.route;let runtime=setup.enter_runtime().unwrap();let role=role.activate(&runtime).unwrap();
            route.deliver_retained(RetainedFrame::new(StaticFrame {bytes,peer:PeerRef::UNKNOWN})).unwrap();
            let held=role.try_recv().unwrap().unwrap();assert!(route.begin_pull());
            assert_eq!(route.__queued_for_test(),1);drop(held);
            let error=if lending {route.deliver_retained_pulled(RetainedFrame::new(StaticFrame {bytes,peer:PeerRef::UNKNOWN}))}
                else {route.deliver_pulled(crate::transport::InboundFrame {metadata:&[],payload:PayloadBytes::one(bytes),peer:PeerRef::UNKNOWN,reply:ReplyToken::default()})};
            assert_eq!(error,Err(ReceiveError::PoolRetired));assert_eq!(route.__queued_for_test(),0);
        }
    }
    include!("sdk_owned_queue_cases.rs");
    include!("sdk_owned_queue_review_cases.rs");
    include!("rpc_ownership_regressions.rs");
}
