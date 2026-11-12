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
//! Concrete native types linked into a target-side descriptor exporter.
//!
//! Registration is opt-in through `StablePayload`'s `inventory` attribute or
//! [`crate::register_native_type!`] for a concrete generic instantiation. The
//! static records contain callbacks, not an application-wide payload-ID map.
//! Enumeration constructs descriptors only when explicitly requested.

use crate::{
    NativeByteOrder, NativeProfileError, NativeRepresentation, NativeTarget, StablePayload,
};

#[doc(hidden)]
pub use inventory as __inventory;

/// One concrete linked stable-type descriptor callback.
#[derive(Debug)]
pub struct NativeTypeRegistration {
    name: &'static str,
    revision: u32,
    representation: fn() -> NativeRepresentation,
}

impl NativeTypeRegistration {
    /// Creates a static registration for a concrete stable payload type.
    pub const fn of<T: StablePayload>() -> Self {
        Self {
            name: T::TYPE_NAME,
            revision: T::REVISION,
            representation: T::native_representation,
        }
    }
}

inventory::collect!(NativeTypeRegistration);

/// Collects the explicitly registered types actually linked into this executable.
///
/// Results are sorted by stable name and revision. This cannot discover an
/// unlinked application crate; an exporter must link every intended defining crate.
///
/// # Errors
///
/// Rejects duplicate name/revision registrations, a callback's mismatched declared
/// identity, and descriptors not qualified for the current compiler target/order.
pub fn registered_native_types() -> Result<Vec<NativeRepresentation>, NativeProfileError> {
    let current = NativeTarget::current();
    let mut result = Vec::new();
    for registration in inventory::iter::<NativeTypeRegistration> {
        let representation = (registration.representation)();
        if representation.name() != registration.name
            || representation.revision() != registration.revision
        {
            return Err(NativeProfileError::InvalidDescriptor(
                "inventory callback identity mismatch",
            ));
        }
        if representation.target() != &current
            || representation.byte_order() != NativeByteOrder::native()
        {
            return Err(NativeProfileError::InvalidDescriptor(
                "inventory callback target mismatch",
            ));
        }
        result.push(representation);
    }
    result.sort_by(|left, right| {
        (left.name(), left.revision()).cmp(&(right.name(), right.revision()))
    });
    if result.windows(2).any(|pair| {
        pair.first().zip(pair.get(1)).is_some_and(|(left, right)| {
            left.name() == right.name() && left.revision() == right.revision()
        })
    }) {
        return Err(NativeProfileError::InvalidDescriptor(
            "duplicate native inventory name/revision",
        ));
    }
    Ok(result)
}
