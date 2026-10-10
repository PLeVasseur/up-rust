# UFrame Metadata Byte Contracts (v1)

This document defines the SDK's provisional metadata field-block and whole-frame
envelope formats for peers that explicitly agree to them. It is not an upstream
wire/native-profile registry or a requirement for every transport to implement
these formats. Multi-byte integers are **little-endian**. Values that do not fit
a length field are **encode-time errors**; implementations never truncate.

The layering these contracts live in:

```text
UFrameMetadata (semantic model, per-language, owned/ergonomic)
  └─ metadata field block v1 (this doc, §1) ── canonical byte serialization
       ├─ carried behind native-prefix framing by selected-wire codecs
       ├─ carried opaquely in a binding-defined physical placement
       └─ carried inside the UPFE whole-frame envelope (§2) on classic channels
  └─ UFrameMetadataAbiV1 (fixed profile, see headers in this folder):
       opt-in typed interop only; NOT the default carrier
```

## 1. Metadata field block, version 1

The canonical serialization of frame metadata. Variable-length and
presence-driven: absent optional fields cost zero bytes.

```text
offset  size  field
0       1     block_version        = 1
1       1     kind                 frame message kind wire code (§3.1)
2       1     priority             0 = absent, 1..=7 = CS0..=CS6 (§3.2)
3       1     reserved             MUST be 0
4       4     presence             u32 bitmask (§3.3); unknown bits MUST be 0
8       8     id.msb               u64
16      8     id.lsb               u64
```

Then, in this exact order, each section present only when its presence bit
is set:

```text
[FIELD_REQID]             u64 msb, u64 lsb
[FIELD_TTL]               u64 ttl in nanoseconds
[FIELD_COMM_STATUS]       i32 UCode value
[FIELD_PERMISSION_LEVEL]  u32
source UUri block         (always present; §1.1)
[FIELD_SINK]              UUri block
[FIELD_TOKEN]             u16 len, `len` UTF-8 bytes
[FIELD_TRACEPARENT]       u8 len, `len` UTF-8 bytes
[FIELD_PAYLOAD_ENCODING]  payload encoding block (§1.2)
```

Decoders MUST consume the entire input; trailing bytes are an error.
Decoders MUST reject unknown `block_version`, unknown presence bits, a
nonzero reserved byte, unknown kind/priority codes, and non-UTF-8 strings.
Decoded metadata MUST additionally satisfy the per-kind validity rules of
the semantic model (e.g. a request has a sink, a TTL > 0, and priority ≥
CS4).

### 1.1 UUri block

```text
u32  ue_id             (uE instance id << 16) | uE type id
u16  resource_id
u8   ue_version_major
u8   authority_name_len
...  authority_name    UTF-8, `authority_name_len` bytes (≤ 128 per UUri spec)
```

### 1.2 Payload encoding identifier

A present payload encoding is one structurally valid 16-bit value in a u32 slot.
Presence is independent of its numeric value; zero means contract-defined.

```text
u32  payload_encoding_id  little-endian; 0..=65535
```

## 2. Native whole-frame envelope ("UPFE"), version 1

The lossless carrier for a complete native frame over byte channels or files.
It consists of a fixed header, the variable metadata field block, then payload.
Bindings define their own physical mapping of the resulting byte sequence.

```text
offset  size  field
0       4     magic "UPFE"
4       1     envelope version = 1
5       1     payload presence: 0 = absent, 1 = present
6       2     reserved, MUST be 0
8       4     metadata_len  (u32)
12      8     payload_len   (u64; MUST be 0 when payload absent)
20      m     metadata field block (§1), m = metadata_len
20+m    p     payload bytes, p = payload_len
```

Content type: `application/vnd.uprotocol.uframe;version=1`.

Decoders MUST verify that the input length equals `20 + metadata_len +
payload_len`, that presence and `payload_len` agree, and that the decoded
frame satisfies the v1 frame invariant: payload bytes are present **iff**
the metadata declares a payload encoding. A present empty payload
(`presence=1, payload_len=0`) is distinct from an absent payload.

## 3. Representation code tables

### 3.1 Frame message kind wire codes

These SDK representation codes have explicit projections to the protobuf
`UMessageType` values. They do not establish an independent normative registry.

| code | kind         | legacy `UMessageType` projection |
|------|--------------|----------------------------------|
| 0    | reserved (invalid) | —                          |
| 1    | Publish      | `UMESSAGE_TYPE_PUBLISH`          |
| 2    | Request      | `UMESSAGE_TYPE_REQUEST`          |
| 3    | Response     | `UMESSAGE_TYPE_RESPONSE`         |
| 4    | Notification | `UMESSAGE_TYPE_NOTIFICATION`     |
| 5..=255 | reserved  | —                                |

### 3.2 Frame priority wire codes

| code | priority | legacy `UPriority` projection |
|------|----------|-------------------------------|
| 0    | absent (wire/ABI representations only) | — |
| 1..=7 | CS0..=CS6 | `UPRIORITY_CS0..=CS6`       |
| 8..=255 | reserved | —                          |

### 3.3 Presence bits (shared by field block and ABI profile)

| bit | name                     |
|-----|--------------------------|
| 0   | FIELD_SINK               |
| 1   | FIELD_REQID              |
| 2   | FIELD_TTL                |
| 3   | FIELD_COMM_STATUS        |
| 4   | FIELD_PERMISSION_LEVEL   |
| 5   | FIELD_TOKEN              |
| 6   | FIELD_TRACEPARENT        |
| 7   | FIELD_PAYLOAD_ENCODING   |
| 8..=31 | reserved (MUST be 0 in v1) |

### 3.4 Payload encoding and metadata identity policy

The payload-encoding registry is owned by
[up-spec](https://github.com/eclipse-uprotocol/up-spec/blob/main/basics/payload_encoding_registry.adoc).
Payload IDs are structurally 16-bit. Unknown, unassigned, reserved and private
received values are preserved opaquely; allocation and origination rules are
separate from structural carriage. No media type is used to infer an identity.

Selected-wire, payload-family and metadata-layout identities are three separate
namespaces. The SDK's field-block metadata codec owns layout 2 and its literal.
Metadata-layout compact value 1 is reserved by SDK format policy and has no
decoder. This reservation does not reserve a payload ID or a wire/family value.
Private identity references carry their agreed literal; a compact alias alone
does not become registered because its number matches a known SDK identity.
