/********************************************************************************
 * Copyright (c) 2026 Contributors to the Eclipse Foundation
 *
 * See the NOTICE file(s) distributed with this work for additional
 * information regarding copyright ownership.
 *
 * This program and the accompanying materials are made available under the
 * terms of the Apache License Version 2.0 which is available at
 * https://www.apache.org/licenses/LICENSE-2.0
 *
 * SPDX-License-Identifier: Apache-2.0
 ********************************************************************************/

//! `wire_table!`: one wire value for several message types.

/// Declares a table that is itself a wire for each listed message type.
///
/// ```rust
/// # use up_rust_core::wire::*;
/// # use up_rust_core::uattributes::PayloadEncoding;
/// # struct DoorWire; struct LampWire;
/// # #[derive(Default)] struct DoorState; #[derive(Default)] struct LampState;
/// # impl Wire<DoorState> for DoorWire {
/// #   fn describe(&self) -> WireInfo { unimplemented!() }
/// #   fn encode(&self, _: &DoorState, _: &mut PayloadWriter<'_>) -> Result<(), WireError> { Ok(()) }
/// #   fn init_slot<'s>(&self, s: StartupSlot<'s, DoorState>) -> Ready<'s, DoorState> { s.write(DoorState) }
/// #   fn decode(&self, _: PayloadEncoding, _: &[u8], _: &mut DoorState) -> Result<(), WireError> { Ok(()) }
/// # }
/// # impl Wire<LampState> for LampWire {
/// #   fn describe(&self) -> WireInfo { unimplemented!() }
/// #   fn encode(&self, _: &LampState, _: &mut PayloadWriter<'_>) -> Result<(), WireError> { Ok(()) }
/// #   fn init_slot<'s>(&self, s: StartupSlot<'s, LampState>) -> Ready<'s, LampState> { s.write(LampState) }
/// #   fn decode(&self, _: PayloadEncoding, _: &[u8], _: &mut LampState) -> Result<(), WireError> { Ok(()) }
/// # }
/// up_rust_core::wire_table!(VehicleTable { door: DoorWire => DoorState, lamp: LampWire => LampState });
/// // Attach attributes, doc comments included, before the name and before each field:
/// // `wire_table!(#[derive(Clone)] VehicleTable { /// Doors. door: .. })`.
/// let table = VehicleTable { door: DoorWire, lamp: LampWire };
/// ```
#[macro_export]
macro_rules! wire_table {
    (
        $(#[$meta:meta])* $name:ident {
            $($(#[$field_meta:meta])* $field:ident : $wire:ty => $msg:ty),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        pub struct $name {
            $($(#[$field_meta])* pub $field: $wire,)+
        }

        $(
            impl $crate::wire::Wire<$msg> for $name {
                fn describe(&self) -> $crate::wire::WireInfo {
                    <$wire as $crate::wire::Wire<$msg>>::describe(&self.$field)
                }
                fn encoded_size(&self, value: &$msg) -> $crate::wire::EncodedSize {
                    <$wire as $crate::wire::Wire<$msg>>::encoded_size(&self.$field, value)
                }
                fn encode(
                    &self,
                    value: &$msg,
                    out: &mut $crate::wire::PayloadWriter<'_>,
                ) -> ::core::result::Result<(), $crate::wire::WireError> {
                    <$wire as $crate::wire::Wire<$msg>>::encode(&self.$field, value, out)
                }
                fn init_slot<'s>(
                    &self,
                    slot: $crate::wire::StartupSlot<'s, $msg>,
                ) -> $crate::wire::Ready<'s, $msg> {
                    <$wire as $crate::wire::Wire<$msg>>::init_slot(&self.$field, slot)
                }
                fn decode(
                    &self,
                    encoding: $crate::uattributes::PayloadEncoding,
                    bytes: &[u8],
                    into: &mut $msg,
                ) -> ::core::result::Result<(), $crate::wire::WireError> {
                    <$wire as $crate::wire::Wire<$msg>>::decode(&self.$field, encoding, bytes, into)
                }
            }
        )+
    };
}
