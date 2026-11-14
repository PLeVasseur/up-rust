# Atomic facade and ABI preparation

This finite batch implements the single atomic import boundary and the core ABI
value. It does not port concurrency protocols, enable the atomic-width ban,
establish 32-bit support, or attach ABI metadata to routes/bindings. `AtomicU64`
remains a plain re-export until the protocol port. All 16 existing core users
(including refusal tests) now import from `crate::platform::atomic`; no atomic
call, state, width, ordering, mode transition or RPC containment behavior changes.

`payload::agreement::{AbiFingerprint, ABI_FINGERPRINT, AbiMismatch}` is the core
surface. It has no dependency, feature, heap use or unsafe code. Version 1 is an
eight-byte canonical value with byte order, pointer width, and seven packed
alignment logarithms, including u128 and i128. Unknown versions, invalid byte
orders/widths and nonzero reserved fields are rejected. A comparison error names
the first difference, both ABIs and the serialized-wire remedy. NativeAgreement,
resolve's WrongTarget check, descriptor digests and generated tables are unchanged.
Only the future route/binding admission can enforce the ABI exchange; this value
does not itself prevent a cross-ABI deployment. Matching ABI values do not replace
type tokens, descriptor validation or the integrator's coherence duty.

The source gate rejects direct paths and ancestor/alias/glob imports outside the
exact facade path. It accounts for grouped use trees, raw identifiers, whitespace
and nested comments. Ordinary, byte, C and raw string literals are skipped as
complete tokens, so literal comment markers cannot hide later imports. Use-tree
parsing stops on non-import syntax and requires progress in every group; the outer
scan still examines real imports inside macro bodies. It is a lexical source gate,
not Rust name resolution; it
does not expand generated code, include strings, or identifiers assembled by a
macro. Those boundaries still need source review. Width bans remain deferred.

## C9 trace disposition at the actual neutral pin

Neutral spec/gitlink: 733b006bed71baafb2fa4ee4369cd1164323f8c9.
Neither req~ssr-atomic-width~1 (platform.adoc) nor
req~ssr-native-abi-agreed~1 (peers.adoc) carries SafetyGateUpRust at this pin.
References below and in code are preparatory mappings, deliberately not OFT
bracket tags. No requirement or gate is invented. G3, its report-only/gating OFT
runs, full atomic protocols/32-bit Miri, and 4C-2 setup tests remain pending.

| Source/test | Requirement mapping | Evidence/remaining condition |
| --- | --- | --- |
| platform/atomic.rs; check_core_gates.atomic_import_violations | req~ssr-atomic-width~1, facade clause only | Plain native types; full width/target requirement still open |
| test_atomic_import_negative_controls | same, facade clause | Direct, alias, group, glob, disabled-cfg and macro-body violations |
| test_atomic_import_positive_controls | same, facade clause | Facade permitted; comments/strings and unrelated imports permitted |
| test_atomic_import_exemption_and_line | same, facade clause | Exact-path exemption and diagnostic line |
| test_atomic_import_c_string_boundaries | same, facade clause | Raw/ordinary C literals preserve subsequent import boundaries and line diagnostics |
| test_atomic_import_macro_tokens_progress | same, facade clause | Bounded macro-token parsing; actual macro-body and later imports remain checked |
| ABI_FINGERPRINT and AbiFingerprint/AbiMismatch | req~ssr-native-abi-agreed~1, representation/comparison only | Setup exchange/refusal requires 4C-2 |
| abi_fingerprint_host_const_matches_layout | same | Const value, seven actual host alignments, width/endian, eight bytes/no Drop |
| abi_fingerprint_encoding_and_roundtrip | same | Independent vectors, each nibble's full range, u128/i128 discrimination |
| abi_fingerprint_each_field_and_refusal_text | same | Every field differs; both comparison directions name cause and remedy |
| abi_fingerprint_rejects_noncanonical_encoding | same | Unknown/invalid fields and reserved bits rejected |

Mapping author: assigned stage4m1 implementation worker. Parent review and G3
admission are outstanding. These targeted checks are not final safety lanes.

Brief 4.10 item 0 applies before protocol implementation. This patch changes no
concurrency algorithm or ordering, so the retained RECHECK-4M-2 models remain
design evidence at their original sequentially consistent scope. No state-space
run is justified by import routing or an immutable ABI value. Never use the old
two-checker monitor_refresh.py runner. Later protocol work must consume the
correct per-checker monitor_refresh2/3, delivering-phase/loop, callslot_e, mark3
pull-lock and refusal-answer evidence at its exact bindings, and qualify changed
protocols and weak-memory behavior separately.
