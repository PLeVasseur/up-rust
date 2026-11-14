# Direct L1 user

Read `direct_l1_publish` in `examples/support/independent_layers.rs`. It performs
actual schema admission, builds semantic publication metadata, applies the exact
payload identity, encodes a `Reading`, and sends a validated `UOwnedFrame`.
No L2 publisher/subscriber, opposite peer, or all-role object is constructed.
Addresses in the examples use `UEntityId`; the error returned for a packed
instance-0 ID names the fix.

```rust,ignore
direct_l1_publish(carriage.as_ref(), local_address.as_ref(), &Reading { value: 21 }).await?;
```

L1 deliberately leaves message kind, source/sink, identity and protocol behavior
to the caller. A source provider is routing metadata, not a deployed endpoint.
Using a configured `OwnedWireTransport` selects an admitted full-wire envelope;
changing `ReadingProtobuf` changes payload serialization, not that envelope.

The example returns `LayerError::Payload` versus `LayerError::Transport`, not an
erased error. A successful send is not proof of delivery to a remote subscriber.
For receive, keep the returned `OwnedAttachment` and the listener/storage owners;
detach failure is not successful cleanup. For RPC, do not invent a UUID and call
it correlation: use SDK L2 unless implementing full pending/deadline/route logic.

Advanced: supply custom `LocalUriProvider`, exact peer wire declarations and
transport capabilities. For native L1, use a native route with real synchronous
loan initialization and a retained RX lease; never reinterpret encoded owned
bytes as a native layout or erase a loan lifetime to obtain a storable handle.
