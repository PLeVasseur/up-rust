# The trait map: how the traits fit together

The crate's behavioral traits form a small system: one transport contract, one
delivery callback, five Communication Layer roles with one request handler, one
subscription callback, and three supporting utilities. This page is the
one-screen overview.

## The stack in five lines

```text
application            roles (up-L2)  or  UMessage + UTransport (up-L1)
                                  |
transport contract     UTransport
                                  |
technology             broker, bus, shared memory
```

## The Transport Layer (up-L1)

| Trait | Implemented by | Called by |
| --- | --- | --- |
| [`UTransport`](crate::UTransport) | transport providers (broker/bus bindings) | applications and the Communication Layer |
| [`UListener`](crate::UListener) | message consumers | the transport, on delivery |

[`UTransport`](crate::UTransport) is the floor: `send` a
[`UMessage`](crate::UMessage), or `register_listener` for a source/sink filter
pair. Everything above is built from those operations.

## The Communication Layer roles (up-L2)

Applications usually use the role traits rather than building messages by
hand. The ready-made implementations below drive a [`UTransport`](crate::UTransport)
(feature `up-l2-api` for the traits, `communication` for every implementation):

| You want to... | Role trait | Ready-made implementation |
| --- | --- | --- |
| Publish to whoever listens | [`Publisher`](crate::communication::Publisher) | [`SimplePublisher`](crate::communication::SimplePublisher) |
| Target one uEntity | [`Notifier`](crate::communication::Notifier) | [`SimpleNotifier`](crate::communication::SimpleNotifier) |
| Consume a topic | [`Subscriber`](crate::communication::Subscriber) | [`InMemorySubscriber`](crate::communication::InMemorySubscriber) (needs uSubscription; see [the application tutorial](crate::guide::applications)) |
| Call a service | [`RpcClient`](crate::communication::RpcClient) | [`InMemoryRpcClient`](crate::communication::InMemoryRpcClient) |
| Serve requests | [`RpcServer`](crate::communication::RpcServer) + [`RequestHandler`](crate::communication::RequestHandler) | [`InMemoryRpcServer`](crate::communication::InMemoryRpcServer) |

Each role owns message construction for its message type. Use the roles and you
do not touch [`UMessageBuilder`](crate::UMessageBuilder); use the Transport Layer
directly and message construction is yours.

## Supporting hooks and utilities

* [`SubscriptionChangeHandler`](crate::communication::SubscriptionChangeHandler)
  receives subscription-state callbacks.
* [`LocalUriProvider`](crate::LocalUriProvider) answers "what is *my* address?";
  the role implementations use it to build source URIs.
* [`ProtobufMappable`](crate::ProtobufMappable) lets protobuf-generated types
  ride as payloads implicitly (feature `protobuf-support`).
* [`UAttributesValidator`](crate::UAttributesValidator) validates message
  attributes by message kind; transports use it at their boundaries.

## Where to go

Applications: [the application tutorial](crate::guide::applications).
Transport users: [the Transport Layer tutorial](crate::guide::applications::transport).
Transport authors: [the transport implementation tutorial](crate::guide::utransport).
