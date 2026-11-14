# Guide

Start with [building an application](crate::guide::applications): small runnable
programs, then addressing, schema paths and `role_set!`. If you are writing a
binding, start with [implementing a transport](crate::guide::transports).

## Writing applications

* [Build an application](crate::guide::applications): `hello_*` examples, schema
  paths, `role_set!` for partial setup and cleanup.
* [Communication roles](crate::guide::applications::communication): the role
  handles at a glance.
* [Migrating from 0.9 to 0.10](crate::guide::migration): source-level changes, a
  classic-to-application walkthrough and exact diffs between codecs, envelopes,
  transports and representations.
* [Using the Transport Layer directly](crate::guide::applications::transport)
  and [classic communication recipes](crate::guide::compatibility_communication):
  the message-oriented 0.9 roles, which remain supported.
* [URI storage and received parts](crate::guide::uris): feature-selected storage and typed rejection.

## Implementing bindings and advanced use

* [Implementing a transport](crate::guide::transports): which trait to implement,
  the `transport-implementer` feature, serving receive roles and shared obligations.
* [`UTransport`](crate::guide::utransport): the classic transport interface, with
  L1 requirements and links to published transport implementations.
* [Owned frames](crate::guide::owned), [frame metadata](crate::guide::frame_metadata)
  and [wire formats](crate::guide::wires): validated metadata, representations,
  codecs and selected-wire adapters.
* [Bounded registrations](crate::guide::registrations): generic workers, overflow and lifecycle.
* [Native identity](crate::guide::native_identity): target-qualified descriptors and
  explicit deployment agreement (`native-identity`).
* [Native loan mechanics](crate::guide::zero_copy): prepared engines and receive
  leases for binding implementers (`up-l2-zero-copy`).
* [The trait map](crate::guide::trait_map): primary application roles and their
  underlying carriage/storage contracts.
* [The typed guide](crate::guide::typed): the static engines underneath,
  migration deltas and native ownership.

With `typed-communication`, `guide::independent_l1_implementer`,
`guide::independent_l1_user`, `guide::independent_l2_implementer` and
`guide::independent_l2_user` address wire/transport authors, direct L1 users,
reusable L2 authors and L2 application users separately.

Native paths have only been exercised on test fixtures, SOME/IP is not yet admitted
as a supported binding, and other bindings have not been rebuilt against this API.
Each binding's own documentation states what it supports and has been tested with.
