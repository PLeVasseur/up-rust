# Native inventory type fixtures

This test-only defining crate registers DoorState, the default `seat` feature's
SeatState, and the optional `air` feature's AirState. All have the same physical
size; their stable identities remain distinct. The companion exporter explicitly
links this crate to test registration reachability and feature changes.

See the [deployment guide](../../tools/up-native-profile/docs/deployment-agreement.md)
for the public inventory and append-only allocation contract. This crate assigns
no payload IDs and is not a production application schema.
