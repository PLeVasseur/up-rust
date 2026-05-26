# Payload Codecs And Zero-Copy Support

This crate separates whole-frame wire formats from payload codecs. Transports carry `UFrameMetadata` and payload bytes; they must not parse codec-specific stable type metadata or schema details.

## Copy Semantics

| Path | Payload copy behavior |
| --- | --- |
| `UOwnedFrame::from_payload_as` | Encodes into a new owned byte buffer. |
| `UOwnedFrame::from_encoded_payload` | Moves an `EncodedPayload<C>` into the frame without an encode-copy. |
| `UOwnedFrame::from_bytes_as` | Compatibility helper that wraps bytes in `EncodedPayload<C>` for byte-oriented codecs. |
| `UOwnedTransportExt::send_payload_as` | Encodes into an owned frame, then sends it. |
| `UOwnedTransportExt::send_encoded_payload` | Moves an `EncodedPayload<C>` into an owned frame and sends it. |
| `UOwnedTransportExt::send_bytes_as` | Sends already encoded byte payloads without an additional encode-copy. |
| `UZeroCopyTransportExt::send_encoded_payload` | Copies an `EncodedPayload<C>` directly into a TX loan, avoiding owned-frame construction. |
| `UZeroCopyTransportExt::send_encoded_payload_as` | Encodes directly into a transport TX loan, avoiding an intermediate owned payload buffer. |
| `UZeroCopyTransportExt::send_loaned_payload_as` | Initializes the typed payload directly in the transport TX loan after codec-level default initialization. This avoids a source payload copy but still initializes first. |
| `UZeroCopyUninitTransportExt::send_uninit_loaned_payload_as` | Constructs the typed payload directly in uninitialized transport storage exactly once. This is the true stable typed zero-copy TX path when the transport supports uninitialized loans. |
| `UZeroCopyUninitTransportExt::send_uninit_loaned_bytes_as` | Lets generators write encoded bytes directly into uninitialized transport storage through an exact-length cursor. |
| `UContiguousZeroCopyRxFrame::borrow_payload_as` | Borrows from one contiguous payload slice. This does not prove the slice is loan-backed. |
| `ULoanedContiguousZeroCopyRxFrame::borrow_loaned_payload_as` | Borrows from contiguous loan-backed payload storage. This is the typed payload zero-copy RX path. |

## Stable Containers

`StableContainerPayload<T>` follows the iceoryx2 `ZeroCopySend` safety model. `StablePayload` is the unsafe shared-memory contract, the stable type name is the cross-process identity, and runtime compatibility checks use type name, `variant=fixed`, exact size, and sufficient advertised alignment. There is no required layout hash or fingerprint.

Typed RX borrowing requires broad `StablePayload` plus matching metadata, exact payload length, and actual pointer alignment. Safe stable-container TX and owned/raw encode paths are stricter: they require `ByteBackedStablePayload`, the recursive proof that every byte in `size_of::<T>()` is initialized by safe construction. This excludes implicit-padding layouts from safe byte-backed TX/encode while still allowing them on RX when the received payload bytes are already initialized by the transport.

For payloads that are already encoded, prefer `EncodedPayload<C>` over passing raw bytes plus a separate codec type at the call site. `UOwnedFrame::from_encoded_payload`, `UOwnedTransportExt::send_encoded_payload`, and `UZeroCopyTransportExt::send_encoded_payload` keep the codec tag attached to the bytes.

Use the broad derive for stable payload identity and RX borrowing:

```rust,ignore
#[repr(C)]
#[derive(StablePayload)]
#[stable_payload(type_name = "example.vehicle.VehiclePose")]
struct VehiclePose {
    x: f32,
    y: f32,
}
```

Add the stricter checked derive when the type is used with safe stable-container TX or owned/raw encode:

```rust,ignore
#[repr(C)]
#[derive(PlacementDefault, StablePayload, ByteBackedStablePayload)]
#[stable_payload(type_name = "example.vehicle.VehiclePose")]
struct VehiclePose {
    x: f32,
    y: f32,
}
```

The `StablePayload` derive requires `#[repr(C)]` or `#[repr(transparent)]`, rejects obvious process-local fields such as references, raw pointers, function pointers, `String`, `Vec`, `Box`, and `Arc`, rejects types that need drop glue, and emits the matching unsafe `ZeroCopySend`/`StablePayload` impls. The `ByteBackedStablePayload` derive repeats the structural checks for byte-backed TX/encode: top-level fields must exactly cover the type size and every field must be recursively byte-backed. If this derive fails, represent padding explicitly as initialized fields, use the type for broad RX only, or use the expert unsafe TX hatch with a full-byte initialization proof.

The `up_rust::__derive_support` module and similarly named hidden support traits are macro implementation details. They are not stable application extension points. Manual byte-backed opt-in should use `unsafe impl up_rust::payload::ByteBackedStablePayload for T {}` only in expert FFI/codegen cases where the author can prove the byte-level contract.

### Expert unsafe manual implementation

Manual byte-backed impls are always possible because `ByteBackedStablePayload` is a public unsafe trait. They are not recommended for normal hand-written payloads; prefer `#[derive(StablePayload, ByteBackedStablePayload)]` so the macro checks layout and recursive field eligibility.

```rust,ignore
unsafe impl up_rust::payload::ByteBackedStablePayload for GeneratedPayload {}
```

The impl author must prove that `StablePayload::SUPPORTS_BYTE_BACKED_UNINIT` is true, the type does not need drop glue, and every transported byte in `size_of::<T>()` is initialized by all safe construction paths used with stable-container TX/encode.

Stable typed uninitialized TX uses a type-state capability so uninitialized memory is never exposed through `UTxBuffer::payload()` or `payload_mut()`:

```rust,ignore
transport
    .send_uninit_loaned_payload_as::<StableContainerPayload<VehiclePose>, VehiclePose>(
        UFrameMetadata::publish(topic),
        |slot| Ok(slot.write(VehiclePose { x: 1.0, y: 2.0 })),
    )
    .await?;
```

For padded stable payloads, safe byte-backed TX/encode is intentionally unavailable. The expert unsafe hatch is behind `unsafe-stable-payload-tx` or `expert-unsafe-payloads`; callers must initialize every transported byte, including implicit padding, before commit. Prefer `UnsafeStablePayloadTxSlot::zeroed()` before raw field writes so padding starts initialized. Raw uninitialized byte-slice access is separately gated by `unsafe-uninit-payload-bytes`, and raw typed field-initialization helpers are gated by `unsafe-stable-payload-init`.

| Feature | Exposes | Caller obligation |
| --- | --- | --- |
| `unsafe-stable-payload-tx` | Unsafe non-byte-backed stable TX hatch. | Initialize every transported byte, including implicit padding. |
| `unsafe-stable-payload-init` | Raw typed init pointers and `assume_init`. | Fully initialize a valid `T` before commit. |
| `unsafe-uninit-payload-bytes` | Raw `MaybeUninit<u8>` byte slices. | Keep byte initialization proof consistent. Prefer `LoanedUninitByteWriter`. |
| `expert-unsafe-payloads` | All unsafe payload features. | All active unsafe obligations above. |

The stable-container custom encoding uses `up.stable-container` as the codec family and carries type-detail metadata in the content type, for example `application/vnd.uprotocol.stable-container;type="example.vehicle.VehiclePose";variant=fixed;size=8;align=4`. This metadata is for endpoints/codecs; transports keep it opaque.

Runtime-length dynamic slice payloads map to a separate future API. This fixed-size stable-container path always emits and requires `variant=fixed`.

## Stable-Container Migration

The stable-container API now has one broad fixed-size contract, `StablePayload`, and one stricter safe byte-backed TX/encode contract, `ByteBackedStablePayload`. Application types should derive `StablePayload` and should not derive or implement `ZeroCopySend` separately unless they have a separate non-uprotocol use case. Types used with safe stable-container TX/encode should also derive `ByteBackedStablePayload`.

Old shape:

```rust,ignore
#[repr(C)]
#[derive(PlacementDefault, ZeroCopySend, StablePayload)]
#[stable_payload(type_id = "example.vehicle.VehiclePose", version = 1)]
struct VehiclePose {
    x: f32,
    y: f32,
}
```

New shape:

```rust,ignore
#[repr(C)]
#[derive(PlacementDefault, StablePayload, ByteBackedStablePayload)]
#[stable_payload(type_name = "example.vehicle.VehiclePose")]
struct VehiclePose {
    x: f32,
    y: f32,
}
```

| Removed API or metadata | Replacement |
| --- | --- |
| `PayloadContract`, `StableLayout`, `StableFromBytes`, `StableIntoBytes` | `StablePayload` plus `StableContainerPayload<T>` encode, borrow, and loan traits. |
| `StableFieldDescriptor`, `StableLayoutDescriptor`, `PayloadEndian`, `stable_payload_contract!` | No replacement. Runtime compatibility is type name, `variant=fixed`, exact size, and sufficient alignment. |
| `#[stable_payload(type_id = "...", version = ...)]` | `#[stable_payload(type_name = "...")]`. Keep the same stable type string previously used as `type_id`. |
| Separate `ZeroCopySend` derive for uProtocol stable payloads | `#[derive(StablePayload)]`, which emits the matching `ZeroCopySend` impl. Add `ByteBackedStablePayload` derive for safe TX/encode. |
| Stable metadata parameters `version`, `endian`, `layout`, `layout_hash` | New metadata parameters `type`, `variant=fixed`, `size`, and `align`. Legacy metadata is rejected. |
| `UWireError::IncompatibleStableLayout` | `UWireError::IncompatibleStablePayload`. |

## Dynamic Slice Follow-Up

`variant=dynamic` is intentionally not accepted by `StableContainerPayload<T>` yet. A correct dynamic payload API must be slice-shaped instead of pretending one runtime-length payload is a single `T`.

Planned follow-up shape:

| Area | Required work |
| --- | --- |
| Public codec | Add a separate slice payload codec, for example `StableSlicePayload<T>`, so fixed `T` and runtime-length `[T]` payloads have distinct Rust APIs. |
| Metadata | Emit and verify `variant=dynamic` with stable element type name, element size, and element alignment. The payload length or transport slice header supplies the element count. |
| TX loaning | Add helpers that reserve `count * size_of::<T>()` bytes and return `&mut [T]`, initialized through `PlacementDefault` element by element. |
| RX borrowing | Add borrowed and loaned slice views that validate metadata, exact byte length, element count, pointer alignment, and loan provenance before exposing `&[T]`. |
| Transport conformance | Add tests that reject wrong element counts, non-multiple payload lengths, copied fallback receive storage, wrong `variant`, and wrong stable type metadata. |
| iceoryx2 mapping | Map to iceoryx2 slice publish/subscribe APIs that carry `number_of_elements`; do not encode dynamic slices through the fixed-size typed API. |

## Transport Conformance

Transport tests should use the conformance helpers:

| Helper | Checks |
| --- | --- |
| `verify_tx_buffer_payload_layout` | TX loan length and alignment. |
| `verify_contiguous_rx_payload_layout` | contiguous RX length/alignment. |
| `verify_loaned_rx_payload_layout` | loan-backed RX length/alignment. |

With the `test-util` feature, downstream transports can also use `test_util::zero_copy_conformance` wrappers around these checks.

For payload-level zero-copy claims, a receive lease should implement `ULoanedContiguousZeroCopyRxFrame` and return `LoanedPayload<'_>`. If a transport can sometimes receive copied bytes, the trait implementation must return an error for those frames instead of silently treating them as typed zero-copy.
