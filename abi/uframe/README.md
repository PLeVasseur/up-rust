# UFrame metadata interoperability

Cross-language artifacts for explicitly agreed frame metadata representations.
The fixed-layout struct is an opt-in derived profile. The variable-length metadata
field block is a separate SDK-defined serialized representation. Neither assigns
a normative native-profile registry entry; the README provisional-interface policy
applies to both.

## Contents

| Path | Representation | Check |
|---|---|---|
| `rust/uframe_metadata_abi_v1.rs` | standalone `no_std` Rust reference definition | selected current/MSRV `rustc --edition 2021 -D warnings --crate-type lib` and const layout assertions |
| `c/uframe_metadata_abi_v1.h` (+ `check_layout.c`) | C11 definition | `gcc -std=c11 -Wall -Wextra -Werror -pedantic` + `static_assert` |
| `cpp/uframe_metadata_abi_v1.hpp` (+ `check_layout.cpp`) | C++17 definition | `g++ -std=c++17 -Wall -Wextra -Werror -pedantic` + `static_assert` |

The crate, standalone Rust, C and C++ definitions assert the same sizes,
alignments, and field offsets at compile time; a drift in any view fails its
build.

Run `bash tools/ci/check_abi.sh 1.95.0` or `bash tools/ci/check_abi.sh 1.88.0`
from the repository. The C/C++ executables are layout probes, not native middleware
runtime acceptance or proof of interoperability with every external implementation.

## Representation layers

The semantic model, serialized bytes and fixed-layout ABI have distinct contracts:

1. **Semantic model:** `up_rust::UFrameMetadata`, with owned fields and no
   representation-specific fixed string capacities.
2. **Field-block bytes:** the variable-length, presence-driven format
   implemented by `up_rust::frame::codec`, for peers that agree to that codec.
3. **Fixed ABI profile:** `UFrameMetadataAbiV1` (this folder,
   size 928, align 8, magic `"UFA1"`, type name
   `uprotocol.v2.UFrameMetadataAbiV1`). For boundaries where both sides
   explicitly agree to exchange a directly readable typed struct. Its per-field
   capacities (authority 128, traceparent 63, token
   510) are **profile policy**;
    conversions from the semantic model fail rather than truncate on
   overflow, and larger values remain fully supported by the semantic
   model and the field block.

Profile properties:

* Magic is `"UFA1"`; the profile is 928 bytes and carries
  one `u32` payload-encoding registry identifier.
* Kind/priority/presence vocabularies are defined by this provisional native
  profile with explicit projection tables to the protobuf enums; the value
  coincidence is a documented choice, not an inheritance.
* Payload identifiers are structurally 16-bit values stored in a four-byte
  slot. Presence is independent of zero, including a present empty payload
  with contract-defined encoding. Allocation/decoder policy is not inferred
  by the field codec.
* TTL is carried as `u64` nanoseconds (matching the semantic
  `Duration`); the legacy 32-bit-millisecond form is a fallible
  projection.
* The presence-bit vocabulary is shared verbatim between the field block
  and the ABI profile, so a fixed-profile consumer and a field-block
  consumer reason about the same bits.

## TTL projection and canonicalization

| Native TTL | Rust UAttributes TTL | Protobuf scalar TTL | Reverse native TTL |
| --- | --- | --- | --- |
| Absent | None | 0 | Absent |
| Present zero nanoseconds | None | 0 | Absent |
| Positive exact milliseconds fitting u32 | Some(milliseconds) | Same value | Same duration |
| Positive fractional milliseconds or overflow | Error | No projection | Not applicable |

The native profiles retain a present zero TTL; classic projection intentionally
canonicalizes it to no expiry and loses its separate presence. Requests still
require positive TTL. Codec, ABI and projection tests distinguish the retained
native presence from the classic/protobuf canonical form. Valid bytes and layout
are independent of the application payload's encoding identifier.
