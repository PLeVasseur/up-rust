# Private selected-wire identity

Payload encodings and selected-wire, payload-family and metadata-layout identities
are separate namespaces. A private payload allocation does not assign a wire alias.
Production-private representations can be long-lived contracts; their meaning is
established by explicit deployment agreement, not by a globally unique number.

Only registered first-wave literal/compact pairs may match compact-only references.
The registration list is namespace-specific and available through
`WireIdentityNamespace::registered_identities` at the crate root. Choosing a registered compact
number for a different literal, or reusing a family literal as a wire, does not
register that identity.

```rust
# #[cfg(any(feature = "selected-wire-user-api", feature = "wire-implementer-api"))]
# {
use up_rust::{WireIdentity, WireIdentityRef};
let expected = WireIdentity::new("vendor.example.wire.v1", 0x8000);
assert!(!WireIdentityRef::Compact(0x8000).matches(expected));
assert!(WireIdentityRef::Literal("vendor.example.wire.v1".into()).matches(expected));
assert!(!WireIdentityRef::Literal("other.vendor.wire.v1".into()).matches(expected));
# }
```

Private/unregistered wire, family and layout references carry their literals.
Metadata admission checks the compact-only restriction before invoking custom
compatibility callbacks and before exposing the frame. Any accepted literal alias
must itself be part of the configured deployment agreement. Full identity checking
is not peer discovery or remote producer attestation.

The README provisional-interface policy applies to these SDK conventions.
Metadata-layout compact value 1 is reserved and has no decoder; only the defined
field-block layout is registered for compact matching. This metadata reservation
does not alter payload-registry policy or reserve values in the wire/family
namespaces. Changes to that set require an explicit versioned format decision.
