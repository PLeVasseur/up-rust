# The trait map

Ten traits carry the crate's behavior: **three at the transport layer,
six at the communication layer, one addressing utility**. Every one has
a role header on its own page saying who implements it and who calls
it; this page is the one-screen overview.

## The transport layer (up-L1)

| Trait | Implemented by | Called by |
|---|---|---|
| [`UTransport`](crate::UTransport) | transport providers (broker/bus bindings) | applications and the communication layer |
| [`UListener`](crate::UListener) | message consumers | the transport, on delivery |
| [`LocalUriProvider`](crate::LocalUriProvider) | deployments (who am I?) | anything that builds addresses |

[`UTransport`](crate::UTransport) is the floor: `send` a
[`UMessage`](crate::UMessage), `register_listener` for a source/sink
filter pair. Everything above is built from those two calls.

## The communication layer (up-L2, feature `communication`)

| Role trait | You are the... | Reference impl |
|---|---|---|
| [`Publisher`](crate::communication::Publisher) | producer of a topic | [`SimplePublisher`](crate::communication::SimplePublisher) |
| [`Subscriber`](crate::communication::Subscriber) | consumer of a topic | [`InMemorySubscriber`](crate::communication::InMemorySubscriber) |
| [`Notifier`](crate::communication::Notifier) | sender of directed notifications | [`SimpleNotifier`](crate::communication::SimpleNotifier) |
| [`RpcClient`](crate::communication::RpcClient) | caller of a method | [`InMemoryRpcClient`](crate::communication::InMemoryRpcClient) |
| [`RpcServer`](crate::communication::RpcServer) | provider of methods | [`InMemoryRpcServer`](crate::communication::InMemoryRpcServer) |
| [`RequestHandler`](crate::communication::RequestHandler) | method body behind an `RpcServer` | yours |

Two supporting hooks complete the layer:
[`SubscriptionChangeHandler`](crate::communication::SubscriptionChangeHandler)
(subscription state callbacks) and the role impls' shared use of
[`LocalUriProvider`](crate::LocalUriProvider) for addressing.

Each role owns message construction for its message type — use the
roles and you never touch [`UMessageBuilder`](crate::UMessageBuilder);
drop to the transport layer and you own it. That split is the whole
crate in one sentence, and [the applications
chapter](crate::guide::applications) walks both sides of it.
