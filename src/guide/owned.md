# Owned-frame carriage

Applications use the roles in `communication::application`; see
[building an application](crate::guide::applications). Classic message-oriented
compatibility traits remain available. This guide is for binding implementers;
[implementing a transport](crate::guide::transports) says which trait to implement
and shows `attach_validated_owned_listener` returning an `OwnedAttachment`.

Bindings implement `UOwnedTransportImpl` with scoped `OwnedAttachment` receive and
setup payload-contract admission (`admit_payload_contract`). For inline send
dispatch, implement `serialized_dispatch::StaticOwnedTransport` with plain
`async fn` methods, without `#[async_trait]`, and wrap it in
`InlineOwnedTransport`; existing `#[async_trait]` implementations keep their boxed
send. Encoded bindings use `UOwnedTransportCore`, or `StaticOwnedCore` wrapped in
`InlineOwnedCore`. A semantic-frame binding whose middleware owns RPC correlation
returns it from `UOwnedTransportImpl::owned_rpc` (or
`StaticOwnedTransport::owned_rpc`). `OwnedWireTransport` admits a provisioned full context;
`UWireTransport` keeps existing W presets with their exact original bytes and
family contracts.
