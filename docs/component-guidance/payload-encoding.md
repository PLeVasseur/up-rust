# Payload encoding: identity, presence, carriage and migration

**Non-normative.** This page is implementation guidance and an evidence plan for
how the Rust SDK meets uProtocol's payload-encoding contract. It adds no
requirement, and nothing here shows that a configuration conforms.

The rules are up-spec's. Its protocol chapters define them, and the SDK carries a
copied snapshot under `up-spec/`. Item revisions cited here are that snapshot's.
Paths to code are relative to the SDK repository. Use them to find where to look,
then check the coverage tags and the source.

## One registry, and it is up-spec's

| What | Where | Items |
| --- | --- | --- |
| The identifier space and its assignments, and the rules for originating, carrying and reading an identifier | `up-spec/basics/payload_encoding_registry.adoc` | `aou~payload-encoding-opaque-carriage~1`, `aou~payload-encoding-supported-decoding~1`, `aou~payload-encoding-origination~1`, `aou~payload-encoding-private-agreement~1`, `aou~payload-encoding-contract-defined~1`, `aou~payload-encoding-category-independent-carriage~1` |
| The attribute `payloadEncodingId`: its presence and its range | `up-spec/basics/uattributes.adoc` | `dsn~up-attributes-payload-encoding-id~1` |
| When a payload is present | `up-spec/basics/umessage.adoc`, "Data Model" | the paragraph beside `req~umessage-data-model-impl~1` and `req~umessage-data-model-proto~1` |
| The Protobuf fields | `up-spec/up-core-api/uprotocol/v1/uattributes.proto`, field 12, and `umessage.proto`, field 2 | `req~uattributes-data-model-proto~2`, `req~umessage-data-model-proto~1` |

- **No second registry.** The SDK defines no registry, enumeration or assignment
  table of its own, and `up-spec/safety/` holds none. `PayloadEncoding` is a
  16-bit number. Its constants are the registry's: `CONTRACT_DEFINED` for `0`,
  and one for each of the assignments 1 to 7
  (`core/src/uattributes/payloadencoding.rs`).
- **Ordinary protocol rules.** They bind every use of the SDK, with or without a
  safety claim. No safety-manual entry restates them, and none of them is an
  optional safety guarantee.
- **Who is obliged.** The registry's `aou~` items are assumptions of use of the
  protocol: they oblige applications and deployments. What a component must do
  is stated in that component's own specification, as the registry's
  "Requirement Ownership" section says.

## Four questions that stay separate

| Question | Rule | Who answers | In the SDK |
| --- | --- | --- | --- |
| Is this number a valid identifier? | Every value from `0x0000` to `0xFFFF` is. Validation rejects none for being reserved, unassigned, private or unknown (`dsn~up-attributes-payload-encoding-id~1`) | Every component, the same way | `PayloadEncoding::try_from_wire_id` accepts every 16-bit value, and refuses a larger one before it narrows it |
| May this producer put it on the wire? | Not a registry-range identifier that is unassigned, and not a reserved one. A private-use identifier needs a deployment agreement, and `0` needs an applicable contract (`aou~payload-encoding-origination~1`, `aou~payload-encoding-private-agreement~1`, `aou~payload-encoding-contract-defined~1`) | The producer and its deployment. Forwarding is not origination | `PayloadEncoding::check_wire_originable` refuses a reserved identifier. It cannot know which registry-range identifiers are assigned, or whether an agreement exists, and those stay with the producer |
| May this component carry it? | Yes: unchanged, with no decoder, whatever the identifier (`aou~payload-encoding-opaque-carriage~1`, `aou~payload-encoding-category-independent-carriage~1`) | Whoever only carries, routes, stores or forwards | Not met in general: see "Carriers" below |
| Can this component read it? | Only an encoding that it has determined and supports. Otherwise it rejects the payload, and it never infers an encoding from the bytes (`aou~payload-encoding-supported-decoding~1`, `aou~payload-encoding-contract-defined~1`) | The endpoint that reads | The route's wire decides. A server answers a request whose encoding it does not admit with `InvalidArgument` (`req~ssr-peer-rpc-undecodable-answered~5`) |

A check that answers one question is never used to answer another. The producer
check is not run on a frame that the SDK only carries, and a reader's refusal is
no reason to drop a frame on a route that does not read it.

## Absent, present-empty and identifier 0

| Case | Payload | Identifier | Meaning |
| --- | --- | --- | --- |
| No payload | absent | absent | The message has no payload, and names no encoding |
| An empty payload | present, zero bytes | present | A payload of no bytes in the named encoding. It is not "no payload" |
| A contract-defined encoding | present, any length | present, `0` | An applicable binding, service or schema contract defines the encoding. `0` does not mean absent, unknown or "any" |

- The payload and its identifier are both present or both absent
  (`dsn~up-attributes-payload-encoding-id~1`).
- In the SDK an absent identifier is `None`, never `0`
  (`RxHeader::payload_encoding`, `core/src/frame/header.rs`), and an absent
  payload is `PayloadBytes::Absent`, which a present empty payload is not.
  `Envelope::parse` refuses a frame whose payload and identifier disagree on
  presence, and the native whole-frame envelope writes a presence flag apart
  from the payload's length (`core/src/transport/envelope.rs`).
- A present empty payload that reaches a wire's decoding is decoded or refused
  as the wire's format decides (`req~ssr-wire-no-panic~2`): decoded to the
  value that the format makes it the encoding of, and refused where the format
  gives the type no empty encoding. That item is about decoding. It does not
  make a carrier drop or change an empty payload.

## An identifier is not a type or a layout

The identifier says how a payload is encoded. It does not say which type, schema
or memory layout the payload has, and alone it establishes neither schema
compatibility nor native ABI compatibility (the registry, "Encoding Identity
versus Type Identity").

- **Separate identities.** The SDK keeps them in separate fields: the selected
  wire and the payload family, which belong to the envelope
  (`AdmittedWire::wire_id`, `AdmittedWire::payload_family`,
  `core/src/transport/envelope.rs`); the payload encoding; and, for a native
  representation, a type token (`WirePayloadIdentity`). `NativeAgreement::resolve`
  derives that token from a deployment's table, and refuses a wrong target, a
  type that the table does not list and a descriptor that does not match
  (`core/src/payload/agreement.rs`).
- **A match is a selection, not a proof.** A frame whose identifier and token
  match an agreement has been selected for a route. The token is 32 bits of a
  digest, and the identifier is a label. A match supplies no proof of layout, of
  initialized storage, of ownership, of ABI or of resource behaviour.
  Those rest on `req~ssr-native-abi-agreed~1`, `req~ssr-integrity-zero-copy~4`,
  `req~ssr-integrity-any-bits~1`, `req~ssr-integrity-checked-at-admission~1`,
  `req~ssr-heap-none~3` and `req~ssr-heap-declared~3`, and on the source-level
  argument that `req~ssr-unsafe-sound~1` and `req~ssr-unsafe-inventory~3`
  require. What a wire states about itself is never such a premise
  (`req~ssr-unsafe-wire-untrusted~1`).
- **No shared-memory identifier.** The registry assigns no identifier to shared
  memory. Which identifier a native route's frames carry is chosen under the
  registry's rules for originating one, by whoever defines the deployment's
  agreement. The native wire takes it from that table
  (`NativeAgreement::encoding`, `core/src/payload/agreement.rs`; `NativeWire`,
  `core/src/wire/native.rs`). The SDK treats no identifier as a general marker
  of shared memory, and a native payload is never an address.
- **No detour and no automatic conversion.** A native route lends storage and
  never passes through a serializer. Turning a native representation into a
  serialized encoding for another transport is a conversion that a named
  profile has to define, with its type, its target and its failure behaviour.
  No such profile is defined here. The SDK must not infer one from an
  identifier, and a route that would need one is refused by name, never
  converted silently.

## Ordinary owning serialized values

An encoding's entry in the registry says nothing about heap use. A serialized
wire, as Protobuf wires usually are, decodes into values that own what they hold.

- Such a route declares its allocation, and setup does not refuse it for
  owning what it holds (`req~ssr-heap-declared~3`, `req~ssr-heap-guarantees~1`,
  `req~ssr-heap-slot-types~3`). No encoding identifier turns it into a native
  route, and none makes it heap-free.
- A stronger resource guarantee binds a route only where the route requires it
  by name. See [memory layers and providers](memory-and-providers.md).
- The native rules stay as they are: no runtime heap use, no panic and no
  forbidden wait in native work, and no owned value ended there.

## Carriers: what is implemented and what is not

The rule for a component that only carries a message is that it keeps the
identifier and the payload bytes, needs no decoder, and does not refuse carriage
because an identifier is unknown to it (`aou~payload-encoding-opaque-carriage~1`).
The specification states that rule for the Streamer in
`req~ssr-streamer-byte-identity~2`, and names a leg's limits in
`req~ssr-streamer-qos-per-leg~2`: the payload size, the token length, what the
leg's envelope can represent, for a native representation what the route's
wires name, and an encoding filter where the route's deployment data states
one. On a route with no filter, the encoding identifier of a serialized payload
is not a limit.

- **Implemented.** The carrier primitives never decode a payload, never
  transcode one and stage no copy (`core/src/transport/carrier.rs`).
- **Not implemented: an encoding that a route was not set up with.** A carrier
  route is set up with a closed list of wires. Each admitted wire records one
  payload encoding and, for a native representation, one type token
  (`AdmittedWire`, `core/src/transport/envelope.rs`). On the `UFrameFields`
  envelope, a frame whose selected-wire prefix matches no admitted wire is
  refused (`UFrameHeaderError::WrongPrefix`). On a carrier route, a present
  payload whose encoding and type token match no admitted wire is refused too
  (`core/src/transport/carrier.rs`, `core/src/transport/inbound.rs`).
- **Not implemented: an encoding filter.** The closed list of wires is not one.
  It follows from the route's wires, it is always on, and its refusal is
  reported as an encoding that is not admitted. Revision 2 asks for the
  opposite on each point: a filter is stated on its own in deployment data, a
  route has none unless one is stated, and its refusal has a cause that names
  the filter.
- **So the carrier primitives do not meet revision 2 of either item.** A
  deployment that forwards through a carrier route names every encoding that
  the route carries. A frame with any other identifier is refused and counted,
  whether or not an endpoint could read it.
- **No tag moves.** A code or test tag that names revision 1 of either item
  stays as it is until its code or test has been reviewed against revision 2.

What the change keeps:

- The envelope and the storage shape stay admission criteria. The encoding
  identifier of a serialized payload stops being one, unless a deployment
  states a filter.
- A native representation is carried only where the route's wires name it. It
  is never carried as serialized bytes, and its token is never removed or
  changed. A filter never admits one.
- No decoder is registered only to make a frame pass.
- A lending egress still needs a storage shape for a payload that no wire
  describes: its largest size, and the alignment that its loans give. A route
  declares them. An identifier does not supply them.
- Refusals for size, token length, addressing, TTL and capacity keep causes of
  their own (`req~ssr-error-no-silent-loss~3`), and so does a filter's refusal.
  None of them is reported as an unknown or invalid encoding.

## Migrating from `UPayloadFormat`

The registry replaces the `UPayloadFormat` enumeration. These are differences
between the two texts of the protocol. No conversion code of the classic API was
inspected for this page.

| Before | Now | What a conversion has to settle |
| --- | --- | --- |
| An enumeration with values 0 to 8 | `payloadEncodingId`, a 16-bit identifier | Identifiers 1 to 7 keep their numbers and meanings. Nothing else carries over by itself |
| `UPAYLOAD_FORMAT_UNSPECIFIED = 0`, "unknown", the default | Absent: no payload. Present `0`: an applicable contract defines the encoding | A sender of the earlier field does not put a default `0` on the wire. Its message with a payload then has a payload and no identifier, which fails validation now. An adapter that accepts it has to say so, and say which identifier it supplies. None is chosen here |
| `UPAYLOAD_FORMAT_SHM = 8`, the address of a shared memory segment | No assignment: 8 is an unassigned registry-range identifier | A producer does not originate 8. A received 8 is still a valid identifier to carry, and nothing relabels it on the way. A deployment that used it moves to an identifier under the registry's rules |
| `UPayloadFormat payload_format = 12` | `optional uint32 payload_encoding_id = 12` | The field number is kept and its presence is not: an explicit `0` is now sent, and a value above 65535 fails validation. What an earlier library does with a number that its enumeration lacks is that library's behaviour |
| The former item `dsn~up-attributes-payload-format~1`, which up-spec no longer holds: the format is empty if there is no payload | `dsn~up-attributes-payload-encoding-id~1`: both present or both absent | The earlier rule allowed a payload with no format. The current one does not. A coverage tag that still names the former item is reviewed against the current one before it is changed |
| A MIME type on each enumeration value | A media type in the registry, as a description | A media type never decides carriage, and never the identifier (`aou~payload-encoding-category-independent-carriage~1`) |

## Mappings of other transports

The redesigned SDK's own transports are iceoryx2 and LoLa. Each chapter below
binds a transport that implements it, and an adapter or a Streamer leg toward
such a transport keeps its rules. This table names the constraints that matter
for identity and presence. The chapters govern.

| Chapter | What carries the identifier and the presence | Constraints |
| --- | --- | --- |
| `up-spec/up-l1/cloudevents.adoc`, `dsn~ce-payload-opaque-carriage~1` | `pformat`, present if and only if a payload data property is | Opaque carriage only: `binary_data` or `data_base64`, with `datacontenttype` `application/octet-stream`. A typed data form, and a payload without `pformat`, are rejected. `datacontenttype` never decides the identifier. An empty `data_base64` is a present empty payload |
| `up-spec/up-l1/mqtt_5.adoc`, `dsn~up-transport-mqtt5-attributes-mapping~3`, `dsn~up-transport-mqtt5-payload-mapping~1` | Content Type, the identifier in decimal, present if and only if the payload is | No Content Type means both fields absent, and needs an empty packet payload. Content Type `0` with no bytes is a present empty payload. A value that is not a decimal unsigned integer, or is above 65535, is rejected |
| `up-spec/up-l1/someip.adoc`, the `payload_encoding_id` row of its attribute table | No field: the service-interface contract defines the encoding | Outgoing, the message's identifier is checked against the contract. Incoming, the binding sets the contract's identifier, or `0` where the contract defines the encoding without one. A payload whose encoding cannot be established, or does not match, is rejected. Nothing defaults |
| `up-spec/up-l1/iceoryx2.adoc`, `dsn~up-transport-iceoryx2-attributes-mapping~1` | The serialized `UAttributes` in the user header | How the redesigned iceoryx2 binding relates to this chapter is not stated |

No implementation of these mappings was inspected for this page. Their state is
unknown, which is not the same as failing.

## Evidence plan

Checks to reuse where they exist at the exact revision, and to add where they do
not. A check on one transport is evidence for that transport only.

1. **Presence.** A present identifier `0` with zero bytes stays present, and an
   absent payload stays absent, through every envelope the SDK writes and
   parses. A frame whose payload and identifier disagree on presence is refused.
2. **Range.** Each boundary handles `0`, `1`, `2`, `7`, `8`, `0xDFFF`, `0xE000`,
   `0xEFFF`, `0xF000`, `0xFFFE` and `0xFFFF`. `65536` is refused before it is
   narrowed. No identifier is rejected by structural validation for its
   category, and `0xFFFF` matches nothing.
3. **Origination apart from carriage.** The producer check refuses a reserved
   identifier on a send that originates one, and is not run on a frame that is
   only carried.
4. **Reading.** A typed reader with no wire for an encoding returns a named
   refusal. Nothing guesses an encoding from the bytes or from a media type.
5. **Owning values.** An ordinary owning Protobuf route works with its declared
   allocation. A stronger guarantee is admitted or refused by name, apart from
   the encoding.
6. **Carriage.** With no wire for an identifier, a carrier route carries the
   identifier, the bytes and the presence unchanged, and carries them the same
   way after such a wire is added. Controls: the closed-list refusal that exists
   today is detected, and so is a carrier that carries a native representation
   it has not agreed.
7. **An encoding filter.** A route whose deployment data states none has none,
   whatever wires the build contains. A route with one forwards each identifier
   that it names, with or without a reader, and refuses every other serialized
   payload under the filter's own cause. Controls: a filter refusal reported as
   an unknown or invalid encoding is detected, and so is a filter that follows
   from the route's wires.
8. **Both wires on both transports.** The native wire and the Protobuf wire, on
   iceoryx2 and on LoLa, at one exact SDK revision. A build for a target is not
   a run on it.
