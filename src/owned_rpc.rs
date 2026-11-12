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
//! Binding-owned RPC correlation and reply authority for owned-frame transports.
//!
//! Most bindings use the SDK's generic RPC machinery: a bounded correlation table
//! plus an exact response listener. A binding whose middleware owns request and
//! response correlation itself, such as SOME/IP sessions, implements [`OwnedRpc`]
//! and returns it from
//! [`UOwnedTransportImpl::owned_rpc`](crate::UOwnedTransportImpl::owned_rpc).
//! The serialized application roles then use that authority exclusively:
//!
//! * a client call stores the binding's [`OwnedRpcCall`] as its only completion
//!   authority. The SDK reserves no generic table entry and attaches no response
//!   listener for that client;
//! * a server worker receives each [`OwnedRequest`] with an affine
//!   [`OwnedReply`], the only permission to respond. The SDK keeps no request-ID to
//!   reply map and never answers through `send_owned`;
//! * every refusal is final. Setup, preparation, submission, completion and reply
//!   failures are never retried through the generic listener/send path or through
//!   another binding.
//!
//! Application handles are unchanged: `RpcClient<Req, Resp>` and `RpcServer`
//! store no binding type, and client-only, server-only or mixed deployments need
//! no opposite role. Implementations must be cancellation-safe: dropping a setup
//! or submission future, a call, an unconsumed reply or the server attachment
//! retires exactly the binding record it owns.
//!
//! Costs are explicit. The SDK boxes one [`OwnedRpcCall`] per invocation and one
//! [`OwnedReply`] per admitted request, in addition to the binding's own
//! correlation, envelope and copy costs. Submission and reply futures use the
//! bounded inline [`SendFuture`], without a per-send box.

use crate::inline_dispatch::SendFuture;
use crate::{
    OwnedAttachment, PayloadIdentity, UFrameMetadata, UOwnedFrame, UStatus, UUri, WireIdentity,
};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

/// Setup declaration of one serialized client role, constructed by the SDK.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct ClientRoute<'a> {
    /// Exact remote method.
    pub method: &'a UUri,
    /// Exact local response address.
    pub reply_to: &'a UUri,
    /// Declared request payload identity.
    pub request: PayloadIdentity,
    /// Declared request payload family, when the schema mapping has one.
    pub request_family: Option<WireIdentity>,
    /// Declared response payload identity.
    pub response: PayloadIdentity,
    /// Declared response payload family, when the schema mapping has one.
    pub response_family: Option<WireIdentity>,
    /// Bound on simultaneously reserved calls of this role.
    pub max_in_flight: usize,
    /// Time to live of every request of this role.
    pub ttl: Duration,
}

/// Setup declaration of one serialized server role, constructed by the SDK.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct ServerRoute<'a> {
    /// Exact local method.
    pub method: &'a UUri,
    /// Declared request payload identity.
    pub request: PayloadIdentity,
    /// Declared request payload family, when the schema mapping has one.
    pub request_family: Option<WireIdentity>,
    /// Declared response payload identity.
    pub response: PayloadIdentity,
    /// Declared response payload family, when the schema mapping has one.
    pub response_family: Option<WireIdentity>,
    /// Bound on distinct response destinations this server retains.
    pub max_clients: usize,
    /// Whether a full server inbox blocks the delivering thread until a deadline
    /// instead of refusing the request at once.
    ///
    /// A binding that cannot slow its senders should refuse this at setup rather
    /// than stall its ingress thread.
    pub blocks_when_full: bool,
}

/// Binding-supplied RPC authority; see the [module documentation](self).
#[async_trait::async_trait]
pub trait OwnedRpc: Send + Sync {
    /// Installs one client role before any of its requests can be sent.
    ///
    /// # Errors
    /// Refuses unsupported routes, payload identities, limits or lifetimes before
    /// any side effect. Failure or cancellation must leave no registration.
    async fn attach_client(
        &self,
        route: ClientRoute<'_>,
    ) -> Result<Box<dyn OwnedRpcClient>, UStatus>;

    /// Installs one server role that delivers admitted requests to `sink`.
    ///
    /// Releasing the returned attachment stops admission. Requests already moved
    /// into `sink` keep their reply authority until consumed, dropped or expired.
    ///
    /// # Errors
    /// The same refusal and rollback contract as [`Self::attach_client`].
    async fn attach_server(
        &self,
        route: ServerRoute<'_>,
        sink: Arc<dyn OwnedRequestSink>,
    ) -> Result<OwnedAttachment, UStatus>;
}

/// One installed client role. Dropping it closes the role.
pub trait OwnedRpcClient: Send + Sync {
    /// Checks every attribute of `metadata` and reserves binding correlation for
    /// its request ID before the SDK encodes the payload.
    ///
    /// # Errors
    /// `UCode::ResourceExhausted` reports occupied capacity. A refusal has no
    /// side effect; an attribute the binding cannot preserve is refused here.
    fn prepare(&self, metadata: &UFrameMetadata) -> Result<Box<dyn OwnedRpcCall>, UStatus>;

    /// Reserved calls, including completed calls not yet consumed or dropped.
    fn in_flight(&self) -> usize;

    /// Closes admission and retires every outstanding call of this role.
    ///
    /// # Errors
    /// A physical release failure. The role stays closed and the call may be retried.
    fn close(&self) -> Result<(), UStatus>;
}

/// Binding-owned completion authority of one request. Dropping it cancels the
/// call and retires its exact record, before or after a committed send.
pub trait OwnedRpcCall: Send + Sync {
    /// Sends the prepared request once; `frame` carries the prepared request ID.
    ///
    /// Dropping the returned future before completion must not leave a
    /// reservation that a later response could complete.
    fn submit(&mut self, frame: UOwnedFrame) -> SendFuture<'_>;

    /// Polls for the correlated response, registering the waker while pending.
    ///
    /// `Ready` is returned once; the SDK never polls a completed call again.
    fn poll_response(&mut self, context: &mut Context<'_>) -> Poll<Result<UOwnedFrame, UStatus>>;
}

/// Affine response permission of one admitted request.
pub trait OwnedReply: Send {
    /// Sends `frame` as the response, consuming the permission.
    ///
    /// Dropping an unconsumed reply, or its future before completion, retires the
    /// request without a response.
    fn respond(self: Box<Self>, frame: UOwnedFrame) -> SendFuture<'static>;
}

/// One admitted server request paired with its affine reply authority.
pub struct OwnedRequest {
    frame: UOwnedFrame,
    reply: Box<dyn OwnedReply>,
}

impl OwnedRequest {
    /// Pairs a validated request frame with the binding's reply permission.
    pub fn new(frame: UOwnedFrame, reply: Box<dyn OwnedReply>) -> Self {
        Self { frame, reply }
    }

    /// The request frame, including its complete original metadata.
    pub fn frame(&self) -> &UOwnedFrame {
        &self.frame
    }

    /// Separates the frame from its reply permission.
    pub fn into_parts(self) -> (UOwnedFrame, Box<dyn OwnedReply>) {
        (self.frame, self.reply)
    }
}

impl std::fmt::Debug for OwnedRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OwnedRequest")
            .field("frame", &self.frame)
            .finish_non_exhaustive()
    }
}

/// Refusal of the SDK's bounded request inbox; the request is already released.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RequestRefused {
    /// The server's admission has closed.
    Closed,
    /// The bounded inbox is full, or its configured blocking wait expired.
    Full,
}

/// SDK-owned bounded request admission handed to [`OwnedRpc::attach_server`].
pub trait OwnedRequestSink: Send + Sync {
    /// Moves one request into reserved inbox storage. A refused request, with its
    /// reply permission, is dropped before this returns.
    ///
    /// The deployment's overflow policy applies; a blocking policy can block the
    /// calling binding thread up to its configured deadline.
    ///
    /// # Errors
    /// Reports closed or exhausted admission.
    fn push(&self, request: OwnedRequest) -> Result<(), RequestRefused>;
}
