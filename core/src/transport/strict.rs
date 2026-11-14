// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Initialization-only inspection. Rule evaluation belongs to `up-rust-strict`.

use super::{Capabilities, RouteReport, RuntimeWaiver, StartupError};
use crate::{platform::Platform, signal::Signal, uri::UUri, wire::WireInfo};
use core::time::Duration;

/// A snapshot of declarations; changing a copy cannot change an admitted route.
/// Available to a policy only during initialization. Later stages add integrity
/// levels and detection bounds when their implementations exist.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct RouteView {
    /// Route identity, kind, QoS, send mode, heap declaration and deviations.
    pub report: RouteReport,
    /// Typed wires, or an empty slice on a byte-preserving carrier route.
    pub wires: &'static [WireInfo],
    /// Reserved peer capacity; zero for sends.
    pub max_peers: usize,
    /// Explicit RPC clients, when declared.
    pub clients: &'static [UUri],
    /// RPC/message TTL, when this route declares one.
    pub ttl: Option<Duration>,
    /// Reserved outstanding calls; zero outside an RPC response route.
    pub max_in_flight: usize,
}

/// Borrowed initialization state. No concrete transport type enters this API.
#[non_exhaustive]
#[derive(Debug)]
pub struct TransportView<'a> {
    /// Binding identity, as declared by `BindingSetup::name`.
    pub binding: &'static str,
    /// Binding version, or explicitly unknown when the binding has not declared it.
    pub binding_version: Option<&'static str>,
    /// Binding capabilities.
    pub capabilities: Capabilities,
    /// Routes already admitted at this observation.
    pub routes: &'a [RouteView],
    /// Explicit middleware waivers.
    pub waivers: &'a [&'static RuntimeWaiver],
    /// Shared signal identity.
    pub signal: *const Signal,
    /// Shared startup region-chain identity, without a placement handle.
    pub memory: *const (),
    /// Platform contract; no additional platform claim is inferred here.
    pub platform: *const dyn Platform,
    /// Whether this build aborts on panic.
    pub panic_abort: bool,
}

/// A policy installed at setup. Checks run at each route's build and again at
/// the transition, before binding startup. No policy is stored in runtime state.
/// A policy reports a refusal as `StartupError::Strict`, naming the route, rule,
/// requirement and fix. It must not re-enter the setup it is inspecting.
// [impl->req~ssr-strict-init-only~1]
pub trait SetupCheck: Sync {
    /// True only for an enforcing policy, never a report-only policy.
    fn is_enforcing(&self) -> bool {
        false
    }
    /// Checks one route. `transport.routes` contains previously admitted routes
    /// at build time, and all admitted routes at transition time.
    fn route(&self, transport: &TransportView<'_>, route: &RouteView) -> Result<(), StartupError>;
    /// Checks the whole transport before entering runtime.
    fn transport(&self, transport: &TransportView<'_>) -> Result<(), StartupError>;
}
