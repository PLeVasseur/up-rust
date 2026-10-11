# Guide

* [Migrating from 0.9 to 0.10](crate::guide::migration): source-level changes and examples.
* [URI storage and validation](crate::guide::uris): bounded wire-part parsing and feature-selected storage.
* [Bounded receive registrations](crate::guide::registrations): setup capacity, generic workers and lifecycle.
* [Frame metadata](crate::guide::frame_metadata): validated metadata, representations and codec errors.
* [Native identity](crate::guide::native_identity): target-qualified descriptors and
  explicit deployment agreement (`native-identity`).

* [Writing an application](crate::guide::applications): overview, with
  runnable Communication Layer role examples and examples that use
  [the Transport Layer](crate::guide::applications::transport) directly.
* [Implementing a transport](crate::guide::utransport): the
  relationship between [`UTransport`](crate::UTransport), L1 requirements, and
  protocol bindings, with links to published transport implementations.
* [The trait map](crate::guide::trait_map): how the public traits fit together.

Start with [applications](crate::guide::applications) to use uProtocol, or the
[trait map](crate::guide::trait_map) for an API overview.
