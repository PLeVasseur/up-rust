# Component implementation guidance

These pages explain how the Rust SDK intends to meet the up-rust SEooC requirements for the optional memory layers, external memory providers, the application layer and the executor-independent asynchronous layer.

- They are implementation guidance and an evidence plan.
- They are not normative, not assumptions of use, not safety-manual coverage entries and not qualification evidence.

The normative statements live in up-spec's `safety/` area, and the SDK carries a copied snapshot of them under `up-spec/`.
That area is laid out by protocol layer: the element's own rules are in `safety/seooc-up-rust/`, and its transport-layer and communication-layer items are in `safety/up-l1/` and `safety/up-l2/`.

- Where a page cites an item, it cites the revision in that snapshot.
- Where a page names a Rust trait, type, lifetime rule, crate or scheduler, that is the Rust mechanism chosen to meet a behavioural requirement; the requirement itself does not depend on it.

| Page | Components | Requirement anchors |
| --- | --- | --- |
| [Memory layers and providers](memory-and-providers.md), including ordinary ownership, guarantee admission and pending cleanup | `up-rust-alloc`, `up-rust-std`, external `up-alloc-*-rust` providers, owning serialized types | `req~ssr-memory-static~2`, `req~ssr-memory-sealed~3`, `req~ssr-memory-computable~3`, `req~ssr-mode-init-failure~4`, `req~ssr-heap-none~3`, `req~ssr-heap-declared~3`, `req~ssr-heap-guarantees~1`, `req~ssr-heap-slot-types~3`, `req~ssr-release-lease-drop~4`, `req~ssr-mode-bounded-startup~3`, `req~ssr-crate-core-only-binary~2`, `req~ssr-unsafe-sound~1` |
| [Application roles](application-roles.md) | the SDK application layer | product R27 to R30; `req~ssr-mode-no-traffic-before-runtime~1`, `req~ssr-qos-no-downgrade~2`, `req~ssr-error-no-silent-loss~3` |
| [Asynchronous integration](async-integration.md) | the planned general async layer (working name `up-rust-async`) and its scheduler integrations | `req~ssr-async-seam~3`, `req~ssr-bound-wait~3`, `req~ssr-release-lease-drop~4`, `req~ssr-heap-none~3` |
| [Implementation traceability](implementation-traceability.md) | SDK source locations, internal names and test names that implement specification items | Non-normative map from item IDs to SDK locations; the specification does not depend on it |

Evidence is reported by its state: design feasibility, implementation present, compiled, linked, exercised, source-reviewed, composition-verified, and qualified or assessed.

- The feasibility experiments cited here ran on exact Rust 1.100.0 on an aarch64 macOS host.
- They show that a Rust design is possible at their stated scope.
- They do not show that the SDK implements the design, that a target or MCU image works, that any timing bound holds, or that any configuration is qualified.

Unsafe Rust written to meet these pages, and safe APIs that establish or keep an
unsafe invariant, are authored and reviewed with the project's pinned
`unsafe-rust-review` skill. Tests and Miri support the source argument; they do
not replace it.

FreeRTOS and TockOS are later RTOS evaluations, after the MCU foundations are
ready. Nothing here assumes one RTOS's API, allocation scheme or execution model.
