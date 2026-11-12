# Public trait map

Ordinary applications start with `communication::application` (enable
`typed-communication`); see [building an application](crate::guide::applications).
A schema package declares operations and payload mappings once, a `Deployment`
supplies carriage and limits, and the application stores domain-only role handles.
`communication::advanced::typed` holds the static engines underneath for advanced and
reusable code; see the [typed guide](crate::guide::typed).

| Concern | Contract | Owner |
| --- | --- | --- |
| Application roles | `communication::application`: `Event`, `Notification`, `Rpc`, `Deployment`; stored `Publisher<P>`, `Notifier<P>`, `Subscription`, `Receiver`, `RpcClient<Req, Resp>`, `RpcServer` | Schema package declares once; applications store domain-only handles |
| Static engines | `communication::advanced::typed::Endpoint` with `Serialized<T, K>` or `Native<C>` | Advanced and reusable code; native deployment bindings |
| Business reception | `Handler<P>`, `RequestHandler<P>`, owning `Delivery<P>` | Application; metadata is available through borrowed `MessageContext` |
| Serialized mapping | `MappingProfile`, `EncodePayload`, `DecodePayload`, `PayloadCodec` | Schema/codec implementer; family identity belongs here |
| Native construction | `StablePayloadInit`, `PayloadInit`, `InitializedStablePayload` | Stable representation derive and application initializer |
| Native checked reception | `RxAdmission`, `RxLease` | SDK over retained binding storage and complete native agreement |
| Local addressing | `LocalUriProvider` | Application/deployment |
| Remote subscriptions | `USubscription`, borrowed activation/close on `Subscription` | Service client and SDK ticket |
| Lifecycle outcomes | `Completion`, `RegistrationStopError::is_terminal`, `completion()` on stop errors | SDK; owners stay retained while an outcome is `Pending` |
| Owned carriage | `UOwnedTransportImpl` → `UOwnedTransport`; `StaticOwnedTransport` in `InlineOwnedTransport` | Physical binding: payload admission and scoped receive; plain `async fn` for inline send dispatch |
| Binding-owned RPC | `owned_rpc::OwnedRpc`, returned by `UOwnedTransportImpl::owned_rpc` | Binding whose middleware owns correlation; the default `None` keeps SDK correlation |
| Classic migration | `ClassicCarriage<T: UTransport>` | One SDK adapter for all roles; payload-agnostic, native-token refusal |
| Encoded carriage | `UOwnedTransportCore` or `StaticOwnedCore` in `InlineOwnedCore`; `OwnedWireTransport`, `OwnedWireContext` | Binding plus provisioned codec/envelope context |
| Existing selected presets | `UWireTransport<W,C>` | Existing exact W identities, family contract and bytes |
| Physical native capability | `NativeLoanCore: PreparedTransportCore` | Explicit configured middleware binding; no blanket implementation |
| Prepared mechanics | `PreparedTransportCore`, `PreparedTxStorage`, `Frame`, `TxRoute` | Binding/bridge implementer; generic pool conformance is not native proof |
| Bounded workers | `RegistrationInbox`, `RegistrationHandler`, `Registration` | SDK; setup capacities and explicit drain/cancel |
| Scoped listener ownership | `OwnedAttachment` or native instance-bound subscription | Binding; synchronous exact detach, retry on failure |
| Opaque forwarding | `HeaderAdmission`, `HeaderLease`, `OpaqueRoute` | Bridge; retain identities and count each cross-pool copy |

Enable `typed-communication` for serialized roles, plus `up-l2-zero-copy` for native.
Native TX initializes directly in middleware storage and commits synchronously;
only response waits and owning receive handlers may cross awaits. Serialization
supports heap-owning values and cross-host carriage with ordinary copy costs.

Classic `communication` traits and `SimplePublisher`, `SimpleNotifier`,
`InMemorySubscriber`, `InMemoryRpcClient` and `InMemoryRpcServer` remain supported
message compatibility. Their recipes are in
[compatibility communication](crate::guide::compatibility_communication).
UMessage/UTransport and `.proto` functionality remain available independently.
The lower-level `communication::owned` and `communication::zero_copy` engines
(under `communication::advanced` with `typed-communication`) are
not application entry points; native deployment bindings compose the `zero_copy`
engines.

To choose a trait, see [implementing a transport](crate::guide::transports). See
[the `UTransport` guide](crate::guide::utransport),
[wire implementers](crate::guide::wires), [owned carriage](crate::guide::owned)
and [native mechanics](crate::guide::zero_copy) for the lower-level contracts.
