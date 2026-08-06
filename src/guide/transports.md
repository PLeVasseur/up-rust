# Implementing a transport

A transport moves message bytes over one technology (a broker, a bus,
shared memory). Capability currently comes in two families, each with its own
tutorial — start with the simplest contract and add capability only if
your technology earns it:

* **[The `UTransport` family](crate::guide::utransport)** —
  implement [`UTransport`](crate::UTransport): send messages, register
  listeners. This is the uP-L1 standard surface; every transport has it,
  and many stop here. The tutorial builds a complete working transport
  in ~40 lines. The wire encoding on this path is whatever your
  technology does internally.
* **[Owned frames](crate::guide::owned)**
  (`owned-frame-transport`, experimental) — exchange whole validated
  frames as owned values and ride the selected-wire system: external or
  deployment-specific wire encodings, registered identities, and
  native-prefix metadata, none of which the `UTransport` path offers.

Whatever the family, the shared obligations below apply.

## Declare families honestly

A transport declares `UTransport` and owned-frame support independently, and
downstream routers and tests dispatch on those declarations. Declare only what
the technology honestly provides. Unsupported families fail explicitly rather
than hanging or degrading to a different family.

## Payload and routing integrity

For `UTransport`-family and owned carriage, preserve three payload
states: absent, present with zero bytes, and present with nonempty
bytes. Presence is not inferred from length.

Encoded metadata and payload regions are opaque to the encoded-core
[`UOwnedTransportCore`](crate::UOwnedTransportCore): a core moves those bytes and never interprets
them. If the underlying protocol or broker mirrors routing fields in
its own native headers, validate those native fields against the
decoded metadata before invoking a callback or forwarding; route from
the validated source/sink metadata, never by decoding
transport-specific meaning out of payload bytes.

## Listener lifecycle and readiness

Getting registration right matters more than it looks: a listener
registered twice double-delivers, and one dropped early loses messages
silently. The UTransport tutorial's loopback shows the registry pattern;
these are the rules it encodes:

Registration completes only when the binding can state what readiness
means for that technology. Discovery-backed transports should expose a
bounded peer or subscription readiness result, not use duplicate
application sends as a readiness protocol. An absent peer returns a
bounded, observable status.

Listener dispatch preserves the binding's documented ordering and is
bounded by an explicit queue, worker, or backpressure policy.
Successful unregister is a quiescence boundary: a callback already
running may complete if the binding says so, but no later callback may
begin. Cancellation wakes blocking receive/poll operations, workers
expose health, shutdown joins them, and native entities are deleted
deterministically so a transport can be recreated in the same domain.

Polling is an acceptable native fallback when its wait is wakeable or
bounded, each iteration bounds take and dispatch work, failures become
health/status signals, and drop cannot leave an unjoined worker.

## Definition of done

"It compiles and my demo works" is not done for a transport — other
people's applications run on your claims. Done means the proof suites
for each family you declare pass. This crate covers owned lifecycle,
validation, filtering, and selected-wire identity behavior. Cross-transport
routing conformance is proved by your ecosystem's integration matrix against
the same declarations.

For which trait is which, see the [trait map](crate::guide::trait_map).
