# PR #328 Payload Clone Benchmark

This benchmark compares the cost of cloning payload-bearing `UMessage` values on `main` and on PR #328.

The benchmark is intentionally narrow: it isolates the ownership change from generated `UMessage.payload: Option<bytes::Bytes>` on `main` to handwritten `UMessage.payload: Option<Vec<u8>>` on PR #328. It repeatedly clones one already-built publish message for several payload sizes.

## Environment

- Host: `Linux oss-work 7.0.5-orbstack-00330-ge3df4e19b0a0-dirty #1 SMP PREEMPT Sun May 10 11:47:42 UTC 2026 x86_64 x86_64 x86_64 GNU/Linux`
- Rust: `rustc 1.93.1 (01f6ddf75 2026-02-11)`
- Build command: `cargo run --release --example payload_clone_bench`
- `main`: `4f9c5b65c81f1e38eb0b3f252cc97a13b93d54c4`
- PR #328: `50b432d5614be67215e9d824fcf720972c071bf2`

## Benchmark Source

The benchmark source lives at `examples/payload_clone_bench.rs` on this branch.

It reports the median of 7 clone-loop runs per payload size after a small warmup. The clone counts are lower for larger payloads to keep total copied data reasonable on the PR branch.

## Results

### `main`

| payload bytes | clones | median total ms | median ns/clone |
| ---: | ---: | ---: | ---: |
| 0 | 200000 | 16.727 | 83.6 |
| 1024 | 100000 | 8.243 | 82.4 |
| 65536 | 10000 | 0.840 | 84.0 |
| 1048576 | 1000 | 0.088 | 87.6 |
| 4194304 | 250 | 0.020 | 79.8 |

### PR #328

| payload bytes | clones | median total ms | median ns/clone |
| ---: | ---: | ---: | ---: |
| 0 | 200000 | 5.231 | 26.2 |
| 1024 | 100000 | 4.523 | 45.2 |
| 65536 | 10000 | 9.332 | 933.2 |
| 1048576 | 1000 | 21.840 | 21840.2 |
| 4194304 | 250 | 22.042 | 88169.3 |

## Interpretation

`main` is effectively flat across payload sizes because cloning `bytes::Bytes` shares the underlying buffer. PR #328 gets faster for empty/tiny messages because the handwritten wrapper is smaller/simpler, but clone cost grows with payload size because cloning `Vec<u8>` copies the payload buffer.

For a 1 MiB payload, this run measured about `88 ns/clone` on `main` versus about `21,840 ns/clone` on PR #328. For a 4 MiB payload, this run measured about `80 ns/clone` on `main` versus about `88,169 ns/clone` on PR #328.

This validates the review comment's ownership/copying concern. It does not claim that every workload regresses: small payloads may improve, and workloads that never clone payload-bearing messages may not notice. The highest-risk paths are fan-out or buffering paths that clone the same payload-bearing `UMessage`, such as in-process dispatch to multiple listeners.
