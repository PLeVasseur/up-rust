# Native inventory exporter fixture

This test-only executable explicitly links the companion defining crate. Its
default features export DoorState and SeatState; `air` adds AirState. Disabling
`linked-types` demonstrates that required types cannot be discovered from a
manifest-only dependency. `--expect-target` rejects a wrong compiler target
instead of changing the exported target label.

```text
cargo run -p native-inventory-fixture-export -- --output inventory.json \
  --expect-target x86_64-unknown-linux-gnu --require-type example.DoorState
cargo run -p native-inventory-fixture-export --features air -- --output inventory-added.json
cargo run -p native-inventory-fixture-export --no-default-features --features linked-types,air -- --output inventory-seat-removed.json
```

Build and run the executable for the desired cross target to obtain that target's
actual descriptors. See the [deployment guide](../../tools/up-native-profile/docs/deployment-agreement.md)
for format versions, generation, distribution and admission.
