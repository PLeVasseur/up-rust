# The trait map: how the traits fit together

The crate's traits look numerous but form a small system: **two transport
families, five communication roles, and three standalone utilities**.
Learn the naming grammar once and every trait's job is readable from its
name:

* **Bare name** (`UTransport`, `UOwnedTransport`) — *you call it.*
  The user-facing contract a configured transport hands you.
* **`...Impl` / `...Core`** (`UOwnedTransportImpl`,
  `UOwnedTransportCore`) — *you implement it,* and only if you are
  writing a transport. `Impl` speaks the library's semantic frame types;
  `Core` receives already-encoded bytes and stays a dumb pipe while
  [`UWireTransport`](crate::UWireTransport) composes wires above it.
* **`...Listener`** (`UListener`, `UOwnedListener`) — *you receive
  through it.* Implement one and register it to be called back.

## The stack in five lines

```text
application            roles (up-L2)  or  UMessage + UTransport (up-L1)
                                 |
transport family       UTransport / owned-frame capability
                                 |
wire composition       UWireTransport = encoded core + wire + codec
                                 |
technology             broker, bus, shared memory
```

## The families

| Family | You implement | You call | You receive with | What flows |
| --- | --- | --- | --- | --- |
| Classic (always available) | [`UTransport`](crate::UTransport) | [`UTransport`](crate::UTransport) | [`UListener`](crate::UListener) | [`UMessage`](crate::UMessage) |
| Owned frames (`owned-frame-transport`) | [`UOwnedTransportImpl`](crate::UOwnedTransportImpl) | [`UOwnedTransport`](crate::UOwnedTransport) | [`UOwnedListener`](crate::UOwnedListener) | the [`Validated`](crate::Validated) frame state |

The UTransport family is the floor every transport provides. The owned family
is an additive capability a transport offers only when its technology honestly
supports complete-frame carriage and validation.

## What flows, in one sentence each

* [`UMessage`](crate::UMessage) — attributes plus optional owned payload bytes
  on the classic transport family.
* [`UOwnedFrame`](crate::UOwnedFrame) — validated frame metadata plus optional
  owned payload bytes on the owned family.
* [`UFrameView`](crate::UFrameView) — the neutral read vocabulary every
  received frame speaks: metadata plus ordered payload bytes. Owned frames
  implement it so decoding code can target the frame view contract.

## The Communication Layer roles (up-L2)

Applications mostly never touch the tables above directly — they use the
role traits. The ready-made implementations below drive a `UTransport`-family
transport. [`communication::owned::Endpoint`](crate::communication::owned::Endpoint)
provides the corresponding owned role facades (feature
`communication-api` for the traits, `communication` for ready-made
implementations):

| You want to… | Role trait | Ready-made implementation |
| --- | --- | --- |
| Publish to whoever listens | [`Publisher`](crate::communication::Publisher) | [`SimplePublisher`](crate::communication::SimplePublisher) |
| Target one uEntity | [`Notifier`](crate::communication::Notifier) | [`SimpleNotifier`](crate::communication::SimpleNotifier) |
| Consume a topic | [`Subscriber`](crate::communication::Subscriber) | [`InMemorySubscriber`](crate::communication::InMemorySubscriber) (needs a uSubscription client — see [the application tutorial](crate::guide::applications)) |
| Call a service | [`RpcClient`](crate::communication::RpcClient) | [`InMemoryRpcClient`](crate::communication::InMemoryRpcClient) |
| Serve requests | [`RpcServer`](crate::communication::RpcServer) + [`RequestHandler`](crate::communication::RequestHandler) | [`InMemoryRpcServer`](crate::communication::InMemoryRpcServer) |

## The wire author's traits

A wire format is a marker plus a codec (feature `wire-implementer-api`):

| Trait | Job |
| --- | --- |
| [`UWire`](crate::UWire) | The marker: the wire's identity constants |
| [`PayloadCodec`](crate::PayloadCodec) | Names the codec and its payload encoding |
| [`EncodePayload`](crate::EncodePayload)`<T>` | Measure (`payload_layout`) then write `T` in place |
| [`DecodePayload`](crate::DecodePayload)`<T>` | Read `T` from borrowed payload bytes |
| [`ReadDecodePayload`](crate::ReadDecodePayload)`<T>` | Decode from an ordered reader — works on segmented storage without coalescing |
| [`UWirePayload`](crate::UWirePayload)`<T>` | Associates a payload type (and its codec) with the wire |

## The composition structs (not traits, but you'll look for them here)

* [`UWireTransport`](crate::UWireTransport) — wraps an encoded core
  selected wire and metadata codec above it. Concretely: the core moves
  two opaque byte regions; `UWireTransport` owns everything typed above
  them — it encodes metadata through the codec, stamps and checks wire
  identity, sizes loans from `payload_layout`, runs the payload codec in
  place, and rejects foreign bytes before any decoder sees them. This is
  where the families and the wires meet, and it is why the transport
  crate underneath never mentions a codec. Its owned receive path returns a
  validated owned frame after metadata decoding and identity checks.

## The standalone utilities

* [`LocalUriProvider`](crate::LocalUriProvider) — answers "what is *my*
  address?"; roles use it to build source URIs.
* [`ProtobufMappable`](crate::ProtobufMappable) — lets any
  protobuf-generated type ride as a payload implicitly
  (`protobuf-support`).
* [`UAttributesValidator`](crate::UAttributesValidator) — validates
  message attributes per message kind; transports use it at boundaries.

## Where to go

Applications: [the application tutorial](crate::guide::applications). Transport
users: [the Transport Layer tutorial](crate::guide::applications::transport).
Transport authors: [the transport implementation tutorial](crate::guide::utransport).
Wire and payload authors: [the wire tutorial](crate::guide::wires).
