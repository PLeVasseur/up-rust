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

//! Async forms of the two-mode runtime handles, on Tokio.
//!
//! ```no_run
//! # use up_rust::two_mode::*;
//! # use up_rust::test_util::wires::DoorState;
//! # async fn run(
//! #     feed: Subscriber<DoorState>,
//! #     set_door: RpcClient<DoorState, DoorState>,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! use up_rust_tokio::{AsyncCall as _, AsyncReceive as _};
//!
//! let state = feed.recv().await?;
//! let reply = set_door.call(&state).await?;
//! # let _ = reply;
//! # Ok(())
//! # }
//! ```
//!
//! This layer is outside the no-heap claim (R45), and its traits exist only on
//! activated handles. Its futures are named and inline, one waker cell per call
//! slot in storage `up-rust-core` reserves at setup, cleared when the future
//! drops. Nothing here spawns a task, opens a channel or boxes a future per
//! operation.
//!
//! [`AsyncReceive::recv`] allows one waiting task per route. A second concurrent
//! `recv` on a route replaces the first task's waker, so the first is not woken.

use std::fmt;
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use pin_project_lite::pin_project;
use tokio::time::{sleep_until, Instant, Sleep};
use up_rust::transport::wake::RouteWake;
use up_rust::two_mode::{
    BorrowError, Call, CallError, InPlace, InPlaceInit, InitializedStablePayload,
    NotificationListener, ReceiveError, Received, Request, RpcClient, RpcServer, StablePayload,
    StablePayloadInit, Subscriber, TransportRuntime, WireMode,
};

mod wake;

/// A value polled by [`Recv`]: a route that can be polled and that has a waker cell.
pub trait AsyncSource {
    /// What one receive yields.
    type Item;
    /// Takes the next item, if one is ready.
    ///
    /// # Errors
    /// The route's receive refusal.
    fn poll_receive(&self) -> Result<Option<Self::Item>, ReceiveError>;
    /// The waker cell of this route.
    fn wake_cell(&self) -> &'static RouteWake;
}

impl<T: Send + Sync + 'static, M: WireMode> AsyncSource for Subscriber<T, M> {
    type Item = Received<T, M>;

    fn poll_receive(&self) -> Result<Option<Self::Item>, ReceiveError> {
        self.try_recv()
    }

    fn wake_cell(&self) -> &'static RouteWake {
        self.__route_wake()
    }
}

impl<T: Send + Sync + 'static, M: WireMode> AsyncSource for NotificationListener<T, M> {
    type Item = Received<T, M>;

    fn poll_receive(&self) -> Result<Option<Self::Item>, ReceiveError> {
        self.try_recv()
    }

    fn wake_cell(&self) -> &'static RouteWake {
        self.__route_wake()
    }
}

impl<Req: Send + Sync + 'static, Resp: 'static, M: WireMode> AsyncSource
    for RpcServer<Req, Resp, M>
{
    type Item = Request<Req, Resp, M>;

    fn poll_receive(&self) -> Result<Option<Self::Item>, ReceiveError> {
        self.try_recv()
    }

    fn wake_cell(&self) -> &'static RouteWake {
        self.__route_wake()
    }
}

/// `recv` on the runtime handles.
pub trait AsyncReceive: AsyncSource + Sized {
    /// Waits for the next item. One waiting task per route.
    fn recv(&self) -> Recv<'_, Self> {
        Recv {
            source: self,
            registered: false,
        }
    }
}

impl<S: AsyncSource> AsyncReceive for S {}

/// The future of a route's next item, stored inline by the caller.
#[must_use = "futures do nothing until polled"]
pub struct Recv<'a, S: AsyncSource> {
    source: &'a S,
    registered: bool,
}

impl<S: AsyncSource> fmt::Debug for Recv<'_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Recv").finish_non_exhaustive()
    }
}

impl<S: AsyncSource> Future for Recv<'_, S> {
    type Output = Result<S::Item, ReceiveError>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(outcome) = self.source.poll_receive().transpose() {
            return Poll::Ready(outcome);
        }
        wake::register(self.source.wake_cell(), context.waker());
        self.registered = true;
        // Registered first, so an arrival in between is not missed.
        match self.source.poll_receive().transpose() {
            Some(outcome) => Poll::Ready(outcome),
            None => Poll::Pending,
        }
    }
}

impl<S: AsyncSource> Drop for Recv<'_, S> {
    fn drop(&mut self) {
        if self.registered {
            self.source.wake_cell().clear();
        }
    }
}

/// `call` on an RPC client handle.
pub trait AsyncCall<Req: 'static, Resp: 'static> {
    /// Sends `request` and waits for the response.
    ///
    /// The call slot is reserved before the request is sent, and the request is
    /// sent when the future is first polled. The future ends by the call's TTL
    /// even if nothing services the transport.
    fn call<'a>(&'a self, request: &'a Req) -> CallFuture<'a, Req, Resp>;
}

impl<Req: 'static, Resp: Send + Sync + 'static> AsyncCall<Req, Resp> for RpcClient<Req, Resp> {
    fn call<'a>(&'a self, request: &'a Req) -> CallFuture<'a, Req, Resp> {
        CallFuture {
            client: self,
            request,
            call: None,
            timer: None,
        }
    }
}

pin_project! {
    /// The future of one RPC call, stored inline by the caller.
    #[must_use = "futures do nothing until polled"]
    pub struct CallFuture<'a, Req: 'static, Resp: 'static> {
        client: &'a RpcClient<Req, Resp>,
        request: &'a Req,
        call: Option<Call<Resp>>,
        #[pin]
        timer: Option<Sleep>,
    }
}

impl<Req: 'static, Resp: 'static> fmt::Debug for CallFuture<'_, Req, Resp> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallFuture").finish_non_exhaustive()
    }
}

impl<Req: 'static, Resp: Send + Sync + 'static> Future for CallFuture<'_, Req, Resp> {
    type Output = Result<CallResponse<Resp>, CallError>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if this.call.is_none() {
            match this.client.invoke(this.request) {
                Ok(call) => *this.call = Some(call),
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
        let Some(call) = this.call.as_mut() else {
            return Poll::Ready(Err(CallError::AlreadyTaken));
        };
        let wake = call.__wake();
        let done = drive(context, wake, this.client.__ttl(), this.timer, || {
            call.try_response().map(|reply| reply.map(drop))
        });
        match done {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(respond(this.call))),
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Polls a call until its response arrives, it fails, or its deadline passes.
fn drive(
    context: &mut Context<'_>,
    cell: Option<&'static RouteWake>,
    ttl: Duration,
    mut timer: Pin<&mut Option<Sleep>>,
    mut poll: impl FnMut() -> Result<Option<()>, CallError>,
) -> Poll<Result<(), CallError>> {
    match poll() {
        Ok(Some(())) => return Poll::Ready(Ok(())),
        Ok(None) => {}
        Err(error) => return Poll::Ready(Err(error)),
    }
    if let Some(cell) = cell {
        wake::register(cell, context.waker());
    }
    // Registered first, so a completion in between is not missed.
    match poll() {
        Ok(Some(())) => return Poll::Ready(Ok(())),
        Ok(None) => {}
        Err(error) => return Poll::Ready(Err(error)),
    }
    // The future ends at the call's time-to-live even with no service tick.
    if timer.is_none() {
        timer.set(Some(sleep_until(Instant::now() + ttl)));
    }
    if let Some(sleeping) = timer.as_mut().as_pin_mut() {
        if sleeping.poll(context).is_ready() {
            return Poll::Ready(match poll() {
                Ok(Some(())) => Ok(()),
                Ok(None) => Err(CallError::TimedOut),
                Err(error) => Err(error),
            });
        }
    }
    Poll::Pending
}

/// A completed call's response. It keeps the call slot until dropped.
pub struct CallResponse<Resp: 'static> {
    call: Call<Resp>,
}

impl<Resp: 'static> fmt::Debug for CallResponse<Resp> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallResponse").finish_non_exhaustive()
    }
}

impl<Resp: Send + Sync + 'static> Deref for CallResponse<Resp> {
    type Target = Resp;

    fn deref(&self) -> &Resp {
        match self.call.__taken_value() {
            Some(value) => value,
            // `respond` is the only constructor, and it runs after `try_response`.
            None => unreachable!("a CallResponse exists only for a decoded response"),
        }
    }
}

fn respond<Resp: 'static>(call: &mut Option<Call<Resp>>) -> CallResponse<Resp> {
    match call.take() {
        Some(call) => CallResponse { call },
        // Reached only right after `try_response` borrowed this same call.
        None => unreachable!("the call was present"),
    }
}

/// `call_in_place` on an RPC client handle of a native route.
pub trait AsyncCallInPlace<Req: StablePayloadInit, Resp: StablePayload + Send + Sync> {
    /// Initializes the request in the transport's loan, sends it and waits for
    /// the response. The future ends by the call's TTL.
    fn call_in_place<F>(&self, fill: F) -> CallInPlaceFuture<'_, Req, Resp, F>
    where
        F: for<'b> FnOnce(InPlaceInit<'b, Req>) -> InitializedStablePayload<'b, Req>;
}

impl<Req: StablePayloadInit, Resp: StablePayload + Send + Sync> AsyncCallInPlace<Req, Resp>
    for RpcClient<Req, Resp, InPlace>
{
    fn call_in_place<F>(&self, fill: F) -> CallInPlaceFuture<'_, Req, Resp, F>
    where
        F: for<'b> FnOnce(InPlaceInit<'b, Req>) -> InitializedStablePayload<'b, Req>,
    {
        CallInPlaceFuture {
            client: self,
            fill: Some(fill),
            call: None,
            timer: None,
        }
    }
}

pin_project! {
    /// The future of one native RPC call, stored inline by the caller.
    #[must_use = "futures do nothing until polled"]
    pub struct CallInPlaceFuture<'a, Req: 'static, Resp: 'static, F> {
        client: &'a RpcClient<Req, Resp, InPlace>,
        fill: Option<F>,
        call: Option<Call<Resp, InPlace>>,
        #[pin]
        timer: Option<Sleep>,
    }
}

impl<Req: 'static, Resp: 'static, F> fmt::Debug for CallInPlaceFuture<'_, Req, Resp, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallInPlaceFuture").finish_non_exhaustive()
    }
}

impl<Req, Resp, F> Future for CallInPlaceFuture<'_, Req, Resp, F>
where
    Req: StablePayloadInit,
    Resp: StablePayload + Send + Sync,
    F: for<'b> FnOnce(InPlaceInit<'b, Req>) -> InitializedStablePayload<'b, Req>,
{
    type Output = Result<CallInPlaceResponse<Resp>, CallError>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        if this.call.is_none() {
            let Some(fill) = this.fill.take() else {
                return Poll::Ready(Err(CallError::AlreadyTaken));
            };
            match this.client.invoke_in_place(fill) {
                Ok(call) => *this.call = Some(call),
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
        let Some(call) = this.call.as_mut() else {
            return Poll::Ready(Err(CallError::AlreadyTaken));
        };
        let wake = call.__wake();
        let done = drive(context, wake, this.client.__ttl(), this.timer, || {
            call.try_response().map(|reply| reply.map(drop))
        });
        match done {
            Poll::Ready(Ok(())) => Poll::Ready(match this.call.take() {
                Some(call) => Ok(CallInPlaceResponse { call }),
                None => Err(CallError::AlreadyTaken),
            }),
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// A completed native call's response. It keeps the call slot, and the transport's
/// storage, until dropped.
pub struct CallInPlaceResponse<Resp: 'static> {
    call: Call<Resp, InPlace>,
}

impl<Resp: 'static> fmt::Debug for CallInPlaceResponse<Resp> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallInPlaceResponse")
            .finish_non_exhaustive()
    }
}

impl<Resp: StablePayload + Send + Sync> CallInPlaceResponse<Resp> {
    /// Borrows the typed response from the retained native storage, checked.
    ///
    /// # Errors
    /// Reports a frame whose bytes are not a valid `Resp`, with a fixed-size reason.
    pub fn payload(&self) -> Result<&Resp, BorrowError> {
        self.call.__taken_payload()
    }
}

/// Applies deadlines forever: the tick a transport needs when nothing polls it.
///
/// Spawn one per transport. Runs [`TransportRuntime::service`] every `period`.
pub async fn service_loop(runtime: TransportRuntime, period: Duration) {
    let mut interval = tokio::time::interval(period);
    loop {
        interval.tick().await;
        let _ = runtime.service();
    }
}
