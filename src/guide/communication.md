# Communication roles

Applications use the role handles in `communication::application`; see
[building an application](crate::guide::applications) for setup and small runnable
programs. The common bounded `Publisher<T>` is prepared by `PublisherDeployment`,
activated after core `enter_runtime`, and sends with fixed-size `PublishError`.
Its implementation is shared by host and no_std targets in `up-rust-application`.
The legacy rows below start from declarations made with `declare_operations!`;
their allocating owned-carriage publisher is explicitly `LegacyPublisher<T>`:

| Role | Declaration | Setup | Use |
| --- | --- | --- | --- |
| LegacyPublisher | `event` | `EVENT.publisher(&local, options)?` | `publisher.publish(&value).await?` |
| Subscription | `event` | `EVENT.subscription(&local, publisher_address, service, handler).await?`, then `activate().await?` | Handler `on_receive` |
| Notifier | `notification` | `NOTE.notifier(&local, destination_uri, options)?` | `notifier.notify(&value).await?` |
| Receiver | `notification` | `NOTE.receiver(&local, sender_address, handler).await?` | Handler `on_receive` |
| RPC client | `rpc` | `METHOD.client(&local, server_address, rpc_options).await?` | `client.invoke(&request).await?`, then `pending.response().await?` |
| RPC server | `rpc` | `METHOD.server(&local, handler, write_reply, server_options).await?` | Handler `handle` |

Handlers implement `Handler<P>` or `RequestHandler<P>` and receive an owning checked
`Delivery`, whose `context()` exposes message metadata. Setup selects the payload
mapping or required native capability; native setups use `native_*` factories and
in-place sends, described in the [typed guide](crate::guide::typed).
`examples/hello_publisher.rs`, `examples/hello_notify.rs` and `examples/hello_rpc.rs`
are the shortest runnable programs. `examples/typed_door.rs` shows all five roles
through the static engines of `communication::advanced::typed`, and
`examples/user_first_door.rs` shows retained cleanup after a lost unsubscribe
acknowledgement.

The message-oriented trait recipes remain in
[classic compatibility](crate::guide::compatibility_communication).
