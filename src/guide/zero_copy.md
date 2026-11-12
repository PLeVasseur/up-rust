# Native loan mechanics

Applications use native roles through `communication::application` or
`communication::advanced::typed` with `Native<C>`; see the [typed guide](crate::guide::typed).
C must implement `NativeLoanCore` after its native-only binding constructor admits
actual middleware TX/RX. Subscription service coordination lives in those layers.

`PreparedTransportCore`, `TxRoute`, `RxAdmission`, retained `RxLease`, the
`communication::zero_copy` (`communication::advanced::zero_copy` with `typed-communication`)
publisher, notifier and receiver engines and the
prepared RPC engines are available to binding and bridge implementers. A failed
receiver or server stop reports `completion()`: `Pending` while a detach failure or
a nonterminal same-worker refusal remains, `Failed` after a terminal worker error.
`PoolCore` conforms to prepared ownership for testing; it does not implement
production `NativeLoanCore`. The explicit test wrapper in
`tests/support/native_fixture.rs` exercises only SDK mechanics; iceoryx2, LoLa and
Zenoh SHM bindings need their own physical tests.
