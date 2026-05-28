# Zero-Copy API Decision Tree

Use the narrowest API that matches the copy boundary you can prove.

| Goal | API | Copy behavior |
| --- | --- | --- |
| Send ordinary owned bytes | `UOwnedFrame::from_bytes_as` or `UOwnedTransportExt::send_bytes_as` | Moves or owns payload bytes; no zero-copy transport claim. |
| Send an already tagged encoded payload | `UOwnedFrame::from_encoded_payload` or `UOwnedTransportExt::send_encoded_payload` | Moves `EncodedPayload<C>` into an owned frame. |
| Serialize a value into an owned frame | `UOwnedFrame::from_payload_as` or `UOwnedTransportExt::send_payload_as` | Encodes into owned bytes. |
| Serialize a value into a TX loan | `UZeroCopyTransportExt::send_encoded_payload_as` | Avoids owned-frame payload allocation, but serialization may copy or transform from the source value. |
| Initialize a typed payload in a TX loan | `UZeroCopyTransportExt::send_loaned_payload_as` | Initializes directly in the loan after codec-level initialization. |
| Construct a stable typed payload in uninitialized TX storage | `UZeroCopyUninitTransportExt::send_uninit_loaned_payload_as::<StableContainerPayload<T>, T>` | Direct true-zero-copy stable TX proof when the transport loan is native. |
| Borrow stable typed RX | `ULoanedContiguousZeroCopyRxFrame::borrow_stable_payload<T>()` | Loan-backed only; validates stable encoding, exact size, alignment, and RX lease lifetime. |

## Claim Matrix

| Path | Required wording |
| --- | --- |
| Direct zero-copy transport API with native TX loan and RX lease conformance | Direct true-zero-copy TX/RX. |
| `send_encoded_payload_as` and serializers writing into a loan | Serializes into a TX loan; may still copy or transform from the source object. |
| `UOwnedFrameEndpoint::from_zero_copy_copying_adapter` | Copying adapter. |
| Owned listeners over zero-copy transports | Copies into owned frames. |
| Streamer default route | Owned native-frame routing. |
| Streamer experimental optimized route | Copy-minimized, one payload copy into egress loan. |
| MQTT5 and vSomeIP | Owned native-frame transports. |
| Stable-container typed receive | Loan-backed receive via `borrow_stable_payload<T>()` only. |

Strict Streamer zero-copy forwarding, dynamic stable slices, stronger stable layout fingerprints, and safe owned stable-container typed decode are future work, not part of this contract.
