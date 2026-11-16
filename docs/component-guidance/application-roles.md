# Application roles

Implementation guidance; see the [index](README.md) for its status. It separates
what the SDK's application layer owes from what the integrator's application
owes.

## Two different responsibilities

| Responsibility | Owner | Anchors |
| --- | --- | --- |
| The application layer's role operations: publish, subscribe, notify, listen, invoke and serve, owned and native | SDK application-layer maintainers | Product R27 to R30; `req~ssr-mode-no-traffic-before-runtime~1`, `req~ssr-mode-no-setup-at-runtime~1`, `req~ssr-mode-entry-admission~1`, `req~ssr-qos-no-downgrade~2`, `req~ssr-error-fixed-size~1`, `req~ssr-error-no-silent-loss~3`, `req~ssr-error-route-report~2`, `req~ssr-async-seam~3` |
| Reacting to what the element reports: errors, counters, silence | The integrator's application | `aou~seooc-application-reacts~4` |

The second is an assumption of use; it cannot stand in for the first. An
application-layer role must keep the admitted route, wire, ownership, QoS and
failure semantics of the core operations it wraps:

- Setup has no traffic operation, and runtime has no setup operation, also
  through the wrapper.
- Unsupported policy is refused at setup by name, never weakened.
- Partial or unknown delivery is never reported as confirmed consumption, a wire
  is never changed silently, and an operation that may have committed is never
  retried silently.
- A native role uses the underlying transport's real loans and samples; it never
  falls back to a serialized copy.
- Domain-facing handles carry no middleware, wire-engine or scheduler type in
  their ordinary signatures (R28), with progressive disclosure for advanced use
  (R30).

## What the frozen revision shows

At `dc7f22da2bf90d98d94d468621df26caadf853ca`, the application crate implements
encoded publishing only; its publisher's setup delegates activation to core's
checked transition. Subscribers, notifications, client and server and the native
role family remain to be built.

## Acceptance checks still to run

- Complete publish/subscribe, notification and request/response examples through
  both wire kinds (native, Protobuf) and both uP-L1 transports (iceoryx2, LoLa).
- Inject genuine Full, post-claim allocation refusal, partial delivery, an
  unreadable clock and unknown counts; compare the direct-core and
  application-layer outcomes.
- Wrong-runtime activation refused without traffic; compile-fail checks for
  setup-only and runtime-only operations through every role.
- Native roles: instrument loan identity and copy count; no serialized fallback.
- Assess the five ergonomic deductions against complete migration examples; this
  guidance closes none of them.
