//! Deterministic owned-RPC join fixtures: the actual serialized `Deployment`
//! client and server roles over a binding-owned correlation double.
//!
//! The double stands in for middleware that owns correlation (SOME/IP sessions).
//! These are SDK contract fixtures only: no middleware, network, SOME/IP
//! envelope or installed-binding acceptance claim.
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use tokio::sync::Notify;
use up_rust::communication::application::{
    CallError, Decoder, Delivery, Deployment, Encoder, RequestHandler, RpcClient, RpcOptions,
    RpcServer, SerializedError, ServerOptions,
};
use up_rust::inline_dispatch::SendFuture;
use up_rust::owned_rpc::{
    ClientRoute, OwnedReply, OwnedRequest, OwnedRequestSink, OwnedRpc, OwnedRpcCall,
    OwnedRpcClient, ServerRoute,
};
use up_rust::{
    DecodePayload, EncodePayload, OverflowPolicy, OwnedAttachment, PayloadCodecIdentity,
    PayloadDecodeLimit, PayloadEncoding, PayloadIdentity, PayloadLayout, RegistrationConfig,
    StaticUriProvider, UCode, UFrameMetadata, UMessageType, UOwnedFrame, UOwnedListener,
    UOwnedTransportImpl, UPriority, UStatus, UUri, UWireError, WireIdentity, UUID,
};

const SERVER_ENTITY: u32 = 0x0001_0100;
const METHOD: u16 = 0x0042;

/// Schema mapping for the fixtures: one big-endian `u32`.
struct Number;
impl PayloadCodecIdentity for Number {
    fn name() -> &'static str {
        "owned-rpc-join-number"
    }
    fn encoding() -> PayloadEncoding {
        PayloadEncoding::RAW
    }
}
impl EncodePayload<u32> for Number {
    fn payload_layout(_: &u32) -> Result<PayloadLayout, UWireError> {
        PayloadLayout::new(4, 1)
    }
    fn encode_payload(value: &u32, dst: &mut [u8]) -> Result<(), UWireError> {
        dst.get_mut(..4)
            .ok_or_else(|| UWireError::invalid_payload("short number buffer"))?
            .copy_from_slice(&value.to_be_bytes());
        Ok(())
    }
}
impl<'a> DecodePayload<'a, u32> for Number {
    fn decode_payload(src: &'a [u8]) -> Result<u32, UWireError> {
        let bytes: [u8; 4] = src
            .try_into()
            .map_err(|_| UWireError::invalid_payload("number payload length"))?;
        Ok(u32::from_be_bytes(bytes))
    }
}

fn status(code: UCode, message: &str) -> UStatus {
    UStatus::fail_with_code(code, message)
}

#[derive(Default)]
struct Pending {
    client: u64,
    waker: Option<Waker>,
    outcome: Option<Result<UOwnedFrame, UStatus>>,
}

/// Binding-owned correlation double shared by independent deployments.
#[derive(Default)]
struct Net {
    servers: Mutex<Vec<(UUri, Arc<dyn OwnedRequestSink>)>>,
    calls: Mutex<HashMap<(u64, u64), Pending>>,
    next_client: AtomicU64,
    refuse_client: AtomicBool,
    refuse_server: AtomicBool,
    fail_submit: AtomicBool,
    corrupt_reply: AtomicBool,
    retired: AtomicUsize,
    late: AtomicUsize,
    generic_attach: AtomicUsize,
    generic_send: AtomicUsize,
}
impl Net {
    fn complete(&self, key: (u64, u64), frame: UOwnedFrame) -> Result<(), UStatus> {
        let mut calls = self.calls.lock().unwrap();
        if let Some(entry) = calls.get_mut(&key).filter(|entry| entry.outcome.is_none()) {
            entry.outcome = Some(Ok(frame));
            if let Some(waker) = entry.waker.take() {
                waker.wake();
            }
            return Ok(());
        }
        self.late.fetch_add(1, Ordering::SeqCst);
        Err(status(
            UCode::DeadlineExceeded,
            "late reply: call already retired",
        ))
    }
    fn sink(&self, method: Option<&UUri>) -> Option<Arc<dyn OwnedRequestSink>> {
        let servers = self.servers.lock().unwrap();
        servers
            .iter()
            .find(|(candidate, _)| Some(candidate) == method)
            .map(|(_, sink)| sink.clone())
    }
}

struct LoopRpc(Arc<Net>);
#[async_trait::async_trait]
impl OwnedRpc for LoopRpc {
    async fn attach_client(
        &self,
        route: ClientRoute<'_>,
    ) -> Result<Box<dyn OwnedRpcClient>, UStatus> {
        if self.0.refuse_client.load(Ordering::SeqCst) {
            return Err(status(UCode::Unimplemented, "client route refused"));
        }
        Ok(Box::new(LoopClient {
            net: self.0.clone(),
            id: self.0.next_client.fetch_add(1, Ordering::SeqCst),
            method: route.method.clone(),
            capacity: route.max_in_flight,
            closed: AtomicBool::new(false),
        }))
    }
    async fn attach_server(
        &self,
        route: ServerRoute<'_>,
        sink: Arc<dyn OwnedRequestSink>,
    ) -> Result<OwnedAttachment, UStatus> {
        if self.0.refuse_server.load(Ordering::SeqCst) {
            return Err(status(UCode::Unimplemented, "server route refused"));
        }
        self.0
            .servers
            .lock()
            .unwrap()
            .push((route.method.clone(), sink));
        let net = self.0.clone();
        let method = route.method.clone();
        Ok(OwnedAttachment::new(move || {
            net.servers
                .lock()
                .unwrap()
                .retain(|(candidate, _)| candidate != &method);
            Ok(())
        }))
    }
}

struct LoopClient {
    net: Arc<Net>,
    id: u64,
    method: UUri,
    capacity: usize,
    closed: AtomicBool,
}
impl OwnedRpcClient for LoopClient {
    fn prepare(&self, metadata: &UFrameMetadata) -> Result<Box<dyn OwnedRpcCall>, UStatus> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(status(UCode::Unavailable, "client role closed"));
        }
        if metadata.kind() != UMessageType::Request || metadata.sink() != Some(&self.method) {
            return Err(status(UCode::InvalidArgument, "not this client's method"));
        }
        let key = metadata.id().as_u64_pair();
        let mut calls = self.net.calls.lock().unwrap();
        if calls
            .values()
            .filter(|entry| entry.client == self.id)
            .count()
            >= self.capacity
        {
            return Err(status(UCode::ResourceExhausted, "binding client capacity"));
        }
        calls.insert(
            key,
            Pending {
                client: self.id,
                ..Pending::default()
            },
        );
        Ok(Box::new(LoopCall {
            net: self.net.clone(),
            key,
            submitted: false,
        }))
    }
    fn in_flight(&self) -> usize {
        let calls = self.net.calls.lock().unwrap();
        calls
            .values()
            .filter(|entry| entry.client == self.id)
            .count()
    }
    fn close(&self) -> Result<(), UStatus> {
        self.closed.store(true, Ordering::SeqCst);
        let mut calls = self.net.calls.lock().unwrap();
        for entry in calls.values_mut().filter(|entry| entry.client == self.id) {
            if entry.outcome.is_none() {
                entry.outcome = Some(Err(status(UCode::Unavailable, "client role closed")));
            }
            if let Some(waker) = entry.waker.take() {
                waker.wake();
            }
        }
        Ok(())
    }
}

struct LoopCall {
    net: Arc<Net>,
    key: (u64, u64),
    submitted: bool,
}
impl LoopCall {
    fn deliver(&mut self, frame: UOwnedFrame) -> Result<(), UStatus> {
        if std::mem::replace(&mut self.submitted, true) {
            return Err(status(UCode::FailedPrecondition, "already submitted"));
        }
        if frame.metadata().id().as_u64_pair() != self.key {
            return Err(status(UCode::InvalidArgument, "not the prepared request"));
        }
        if self.net.fail_submit.load(Ordering::SeqCst) {
            return Err(status(UCode::Unavailable, "injected submission refusal"));
        }
        let sink = self
            .net
            .sink(frame.metadata().sink())
            .ok_or_else(|| status(UCode::Unavailable, "no server role attached"))?;
        let reply = Box::new(LoopReply {
            net: self.net.clone(),
            key: self.key,
        });
        sink.push(OwnedRequest::new(frame, reply))
            .map_err(|_| status(UCode::ResourceExhausted, "server inbox refused"))
    }
}
impl OwnedRpcCall for LoopCall {
    fn submit(&mut self, frame: UOwnedFrame) -> SendFuture<'_> {
        let result = self.deliver(frame);
        SendFuture::new(std::future::ready(result))
    }
    fn poll_response(&mut self, context: &mut Context<'_>) -> Poll<Result<UOwnedFrame, UStatus>> {
        let mut calls = self.net.calls.lock().unwrap();
        let Some(entry) = calls.get_mut(&self.key) else {
            return Poll::Ready(Err(status(UCode::Unavailable, "call retired")));
        };
        if let Some(outcome) = entry.outcome.take() {
            return Poll::Ready(outcome);
        }
        entry.waker = Some(context.waker().clone());
        Poll::Pending
    }
}
impl Drop for LoopCall {
    fn drop(&mut self) {
        if self.net.calls.lock().unwrap().remove(&self.key).is_some() {
            self.net.retired.fetch_add(1, Ordering::SeqCst);
        }
    }
}

struct LoopReply {
    net: Arc<Net>,
    key: (u64, u64),
}
impl OwnedReply for LoopReply {
    fn respond(self: Box<Self>, frame: UOwnedFrame) -> SendFuture<'static> {
        let frame = if self.net.corrupt_reply.load(Ordering::SeqCst) {
            uncorrelated(&frame)
        } else {
            frame
        };
        let result = self.net.complete(self.key, frame);
        SendFuture::new(std::future::ready(result))
    }
}

/// A well-formed response for the same addresses but another request ID.
fn uncorrelated(frame: &UOwnedFrame) -> UOwnedFrame {
    let original = frame.metadata();
    let metadata = UFrameMetadata::response(
        original.source().clone(),
        original.sink().cloned().unwrap(),
        UUID::build(),
    )
    .build()
    .unwrap();
    let metadata = PayloadIdentity::Fixed(PayloadEncoding::RAW)
        .apply_to(metadata)
        .unwrap();
    UOwnedFrame::with_payload(metadata, frame.payload().cloned().unwrap()).unwrap()
}

/// Owned-frame carriage whose RPC is binding-owned; generic paths only count misuse.
struct Carriage(LoopRpc);
#[async_trait::async_trait]
impl UOwnedTransportImpl for Carriage {
    fn admit_payload_contract(
        &self,
        identity: PayloadIdentity,
        _family: Option<WireIdentity>,
    ) -> Result<(), UWireError> {
        if matches!(identity, PayloadIdentity::Fixed(_)) {
            Ok(())
        } else {
            Err(UWireError::invalid_payload("serialized fixture carriage"))
        }
    }
    fn owned_rpc(&self) -> Option<&dyn OwnedRpc> {
        Some(&self.0)
    }
    async fn send_validated_owned(&self, _frame: UOwnedFrame) -> Result<(), UStatus> {
        self.0 .0.generic_send.fetch_add(1, Ordering::SeqCst);
        Err(status(
            UCode::FailedPrecondition,
            "generic send used for owned RPC",
        ))
    }
    async fn attach_validated_owned_listener(
        &self,
        _source: &UUri,
        _sink: Option<&UUri>,
        _listener: Arc<dyn UOwnedListener>,
    ) -> Result<OwnedAttachment, UStatus> {
        self.0 .0.generic_attach.fetch_add(1, Ordering::SeqCst);
        Err(status(
            UCode::FailedPrecondition,
            "generic listener used for owned RPC",
        ))
    }
}

fn deployment(net: &Arc<Net>, authority: &str, entity: u32) -> Deployment {
    Deployment::new(
        Arc::new(Carriage(LoopRpc(net.clone()))),
        Arc::new(StaticUriProvider::new(authority, entity, 1).unwrap()),
        PayloadDecodeLimit::new(1024),
        RegistrationConfig {
            capacity: 8,
            overflow: OverflowPolicy::DropNewest,
        },
        32,
    )
    .unwrap()
}

fn method() -> UUri {
    UUri::try_from_parts("server", SERVER_ENTITY, 1, METHOD).unwrap()
}

fn options(max_in_flight: usize, ttl: Duration) -> RpcOptions {
    RpcOptions {
        max_in_flight,
        ttl,
        max_token_len: 32,
        priority: UPriority::CS4,
    }
}

/// Request metadata observed by the server handler.
#[derive(Clone, Debug)]
struct Seen {
    id: UUID,
    source: UUri,
    token: Option<String>,
    priority: Option<UPriority>,
    ttl: Option<Duration>,
}

#[derive(Default)]
struct Gate {
    entered: Notify,
    release: Notify,
}

struct Doubler {
    seen: Arc<Mutex<Vec<Seen>>>,
    gate: Option<Arc<Gate>>,
}
impl RequestHandler<u32> for Doubler {
    type Reply = u32;
    async fn handle<D: Delivery<u32>>(&mut self, request: D) -> Result<u32, UCode> {
        let value = *request.payload().map_err(|_| UCode::InvalidArgument)?;
        let seen = {
            let context = request.context().map_err(|_| UCode::InvalidArgument)?;
            Seen {
                id: context.id.clone(),
                source: context.source.clone(),
                token: context.token.map(str::to_owned),
                priority: context.priority,
                ttl: context.ttl,
            }
        };
        self.seen.lock().unwrap().push(seen);
        if let Some(gate) = &self.gate {
            gate.entered.notify_one();
            gate.release.notified().await;
        }
        if value == 0 {
            return Err(UCode::NotFound);
        }
        Ok(value * 2)
    }
}

async fn server(net: &Arc<Net>, gate: Option<Arc<Gate>>) -> (RpcServer, Arc<Mutex<Vec<Seen>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let server = deployment(net, "server", SERVER_ENTITY)
        .rpc_server(
            METHOD,
            Doubler {
                seen: seen.clone(),
                gate,
            },
            |reply: u32| reply,
            ServerOptions { max_clients: 4 },
            Decoder::from_schema_mapping::<Number>().unwrap(),
            Encoder::from_schema_mapping::<Number>().unwrap(),
        )
        .await
        .unwrap();
    (server, seen)
}

async fn client(net: &Arc<Net>, entity: u32, rpc: RpcOptions) -> RpcClient<u32, u32> {
    deployment(net, "client", entity)
        .rpc_client(
            method(),
            rpc,
            Encoder::from_schema_mapping::<Number>().unwrap(),
            Decoder::from_schema_mapping::<Number>().unwrap(),
        )
        .await
        .unwrap()
}

async fn eventually(condition: impl Fn() -> bool) {
    for _ in 0..200 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("condition not reached within the bounded fixture wait");
}

fn assert_no_generic_path(net: &Net) {
    assert_eq!(net.generic_attach.load(Ordering::SeqCst), 0);
    assert_eq!(net.generic_send.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn independent_client_and_server_use_only_binding_authority() {
    let net = Arc::new(Net::default());
    let (mut rpc_server, seen) = server(&net, None).await;
    let rpc_client = client(&net, 0x0001_1001, options(2, Duration::from_secs(5))).await;
    rpc_client.set_token("credential").unwrap();
    let call = rpc_client.invoke(&21).await.unwrap();
    assert_eq!(rpc_client.in_flight(), 1);
    let reply = call.response().await.unwrap();
    assert_eq!(*reply.payload().unwrap(), 42);
    let request = seen.lock().unwrap().first().cloned().unwrap();
    assert_eq!(request.token.as_deref(), Some("credential"));
    assert_eq!(request.priority, Some(UPriority::CS4));
    assert_eq!(request.ttl, Some(Duration::from_secs(5)));
    let reply_to = UUri::try_from_parts("client", 0x0001_1001, 1, 0).unwrap();
    assert_eq!(request.source, reply_to);
    let context = reply.context().unwrap();
    assert_eq!(context.kind, UMessageType::Response);
    assert_eq!(context.source, method());
    assert_eq!(context.sink, Some(reply_to));
    assert_eq!(context.request_id, Some(request.id));
    assert_eq!(context.priority, Some(UPriority::CS4));
    assert_eq!(rpc_client.in_flight(), 0);
    assert_eq!(rpc_server.stats().responded.load(Ordering::SeqCst), 1);
    assert_no_generic_path(&net);
    rpc_server.stop().await.unwrap();
}

#[tokio::test]
async fn multiple_clients_complete_independently_at_one_server() {
    let net = Arc::new(Net::default());
    let (mut rpc_server, seen) = server(&net, None).await;
    let first = client(&net, 0x0001_1001, options(2, Duration::from_secs(5))).await;
    let second = client(&net, 0x0001_1002, options(2, Duration::from_secs(5))).await;
    let (a, b) = tokio::join!(
        async { first.invoke(&1).await.unwrap().response().await.unwrap() },
        async { second.invoke(&2).await.unwrap().response().await.unwrap() }
    );
    assert_eq!(*a.payload().unwrap(), 2);
    assert_eq!(*b.payload().unwrap(), 4);
    assert_ne!(a.context().unwrap().sink, b.context().unwrap().sink);
    assert_eq!(seen.lock().unwrap().len(), 2);
    assert_eq!(rpc_server.stats().responded.load(Ordering::SeqCst), 2);
    assert_eq!(first.in_flight() + second.in_flight(), 0);
    assert_no_generic_path(&net);
    rpc_server.stop().await.unwrap();
}

#[tokio::test]
async fn dropped_call_retires_and_late_reply_is_refused() {
    let net = Arc::new(Net::default());
    let gate = Arc::new(Gate::default());
    let (mut rpc_server, _) = server(&net, Some(gate.clone())).await;
    let rpc_client = client(&net, 0x0001_1001, options(1, Duration::from_secs(5))).await;
    let call = rpc_client.invoke(&3).await.unwrap();
    gate.entered.notified().await;
    drop(call);
    assert_eq!(net.retired.load(Ordering::SeqCst), 1);
    assert_eq!(rpc_client.in_flight(), 0);
    gate.release.notify_one();
    eventually(|| rpc_server.stats().send_failed.load(Ordering::SeqCst) == 1).await;
    assert_eq!(net.late.load(Ordering::SeqCst), 1);
    assert_eq!(rpc_server.stats().responded.load(Ordering::SeqCst), 0);
    assert_no_generic_path(&net);
    rpc_server.stop().await.unwrap();
}

#[tokio::test]
async fn status_only_error_response_is_mapped_exactly() {
    let net = Arc::new(Net::default());
    let (mut rpc_server, _) = server(&net, None).await;
    let rpc_client = client(&net, 0x0001_1001, options(1, Duration::from_secs(5))).await;
    let reply = rpc_client.invoke(&0).await.unwrap().response().await;
    assert!(matches!(reply, Err(CallError::Status(UCode::NotFound))));
    assert_eq!(rpc_server.stats().responded.load(Ordering::SeqCst), 1);
    assert_eq!(rpc_client.in_flight(), 0);
    assert_no_generic_path(&net);
    rpc_server.stop().await.unwrap();
}

#[tokio::test]
async fn timeout_retires_correlation_before_a_late_reply() {
    let net = Arc::new(Net::default());
    let gate = Arc::new(Gate::default());
    let (mut rpc_server, _) = server(&net, Some(gate.clone())).await;
    let rpc_client = client(&net, 0x0001_1001, options(1, Duration::from_millis(100))).await;
    let call = rpc_client.invoke(&4).await.unwrap();
    gate.entered.notified().await;
    assert!(matches!(call.response().await, Err(CallError::TimedOut)));
    assert_eq!(rpc_client.in_flight(), 0);
    gate.release.notify_one();
    eventually(|| net.late.load(Ordering::SeqCst) == 1).await;
    eventually(|| rpc_server.stats().send_failed.load(Ordering::SeqCst) == 1).await;
    assert_no_generic_path(&net);
    rpc_server.stop().await.unwrap();
}

#[tokio::test]
async fn binding_refusals_are_final_and_never_fall_back() {
    let net = Arc::new(Net::default());
    net.refuse_client.store(true, Ordering::SeqCst);
    let refused = deployment(&net, "client", 0x0001_1001)
        .rpc_client::<u32, u32>(
            method(),
            options(1, Duration::from_secs(5)),
            Encoder::from_schema_mapping::<Number>().unwrap(),
            Decoder::from_schema_mapping::<Number>().unwrap(),
        )
        .await;
    assert!(
        matches!(refused, Err(SerializedError::Transport(status)) if status.code() == UCode::Unimplemented)
    );
    net.refuse_client.store(false, Ordering::SeqCst);
    net.refuse_server.store(true, Ordering::SeqCst);
    let refused = deployment(&net, "server", SERVER_ENTITY)
        .rpc_server(
            METHOD,
            Doubler {
                seen: Arc::default(),
                gate: None,
            },
            |reply: u32| reply,
            ServerOptions { max_clients: 4 },
            Decoder::from_schema_mapping::<Number>().unwrap(),
            Encoder::from_schema_mapping::<Number>().unwrap(),
        )
        .await;
    assert!(
        matches!(refused, Err(SerializedError::Transport(status)) if status.code() == UCode::Unimplemented)
    );
    net.refuse_server.store(false, Ordering::SeqCst);
    let gate = Arc::new(Gate::default());
    let (mut rpc_server, _) = server(&net, Some(gate.clone())).await;
    let rpc_client = client(&net, 0x0001_1001, options(1, Duration::from_secs(5))).await;
    let held = rpc_client.invoke(&5).await.unwrap();
    gate.entered.notified().await;
    assert!(matches!(
        rpc_client.invoke(&6).await,
        Err(CallError::TooManyInFlight)
    ));
    gate.release.notify_one();
    assert_eq!(*held.response().await.unwrap().payload().unwrap(), 10);
    net.fail_submit.store(true, Ordering::SeqCst);
    assert!(matches!(
        rpc_client.invoke(&7).await,
        Err(CallError::Submit(SerializedError::Transport(status))) if status.code() == UCode::Unavailable
    ));
    assert_eq!(rpc_client.in_flight(), 0);
    assert_no_generic_path(&net);
    rpc_server.stop().await.unwrap();
}

#[tokio::test]
async fn client_close_retires_outstanding_calls_and_refuses_new_ones() {
    let net = Arc::new(Net::default());
    let gate = Arc::new(Gate::default());
    let (mut rpc_server, _) = server(&net, Some(gate.clone())).await;
    let rpc_client = client(&net, 0x0001_1001, options(2, Duration::from_secs(5))).await;
    let call = rpc_client.invoke(&8).await.unwrap();
    gate.entered.notified().await;
    rpc_client.close().unwrap();
    assert!(matches!(call.response().await, Err(CallError::Closed)));
    assert!(matches!(
        rpc_client.invoke(&9).await,
        Err(CallError::Closed)
    ));
    gate.release.notify_one();
    eventually(|| rpc_server.stats().send_failed.load(Ordering::SeqCst) == 1).await;
    assert_eq!(net.late.load(Ordering::SeqCst), 1);
    assert_no_generic_path(&net);
    rpc_server.stop().await.unwrap();
}

#[tokio::test]
async fn server_stop_drains_admitted_requests_with_their_reply_authority() {
    let net = Arc::new(Net::default());
    let gate = Arc::new(Gate::default());
    let (mut rpc_server, _) = server(&net, Some(gate.clone())).await;
    let rpc_client = client(&net, 0x0001_1001, options(2, Duration::from_secs(5))).await;
    let call = rpc_client.invoke(&11).await.unwrap();
    gate.entered.notified().await;
    let stopping = rpc_server.stop();
    gate.release.notify_one();
    let (stopped, reply) = tokio::join!(stopping, call.response());
    stopped.unwrap();
    assert_eq!(*reply.unwrap().payload().unwrap(), 22);
    assert!(matches!(
        rpc_client.invoke(&12).await,
        Err(CallError::Submit(SerializedError::Transport(status))) if status.code() == UCode::Unavailable
    ));
    assert_eq!(rpc_client.in_flight(), 0);
    assert_no_generic_path(&net);
}

#[tokio::test]
async fn binding_completion_is_readmitted_by_the_sdk() {
    let net = Arc::new(Net::default());
    let (mut rpc_server, _) = server(&net, None).await;
    let rpc_client = client(&net, 0x0001_1001, options(1, Duration::from_secs(5))).await;
    net.corrupt_reply.store(true, Ordering::SeqCst);
    let reply = rpc_client.invoke(&13).await.unwrap().response().await;
    assert!(matches!(reply, Err(CallError::Response(_))));
    assert_eq!(rpc_client.stats().rejected.load(Ordering::SeqCst), 1);
    assert_eq!(rpc_client.in_flight(), 0);
    assert_no_generic_path(&net);
    rpc_server.stop().await.unwrap();
}
