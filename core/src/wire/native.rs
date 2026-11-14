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

//! Genuine in-place native carriage under one agreed profile (R18, R19, R20).

use core::mem;

use crate::payload::agreement::{AgreementError, NativeAgreement};
use crate::payload::stable::StablePayload;

use super::{RuntimeHeap, WireInfo, STABLE_CONTAINER_PAYLOAD_FAMILY_ID, STABLE_CONTAINER_WIRE_ID};

/// Native carriage: the payload is initialized directly in the middleware loan,
/// and a receiver keeps the native storage and borrows the payload checked.
#[derive(Clone, Copy, Debug)]
pub struct NativeWire {
    agreement: &'static NativeAgreement,
}

impl NativeWire {
    /// A native wire for an explicitly agreed generation.
    #[must_use]
    pub const fn new(agreement: &'static NativeAgreement) -> Self {
        Self { agreement }
    }

    /// The agreement this wire resolves types under.
    #[must_use]
    pub const fn agreement(&self) -> &'static NativeAgreement {
        self.agreement
    }

    /// Resolves `T` at setup, refusing a stale or foreign agreement.
    pub(crate) fn info<T: StablePayload>(&self) -> Result<WireInfo, &'static str> {
        let identity = self.agreement.resolve::<T>().map_err(|error| match error {
            AgreementError::WrongTarget => "native agreement is for a different target",
            AgreementError::NotAgreed => "payload type is not in the native agreement",
            AgreementError::DescriptorMismatch => {
                "payload descriptor disagrees with the native agreement"
            }
        })?;
        Ok(WireInfo::new(
            T::TYPE_NAME,
            STABLE_CONTAINER_WIRE_ID,
            STABLE_CONTAINER_PAYLOAD_FAMILY_ID,
            identity,
            RuntimeHeap::None,
            RuntimeHeap::None,
        )
        .max_encoded_len(mem::size_of::<T>())
        .alignment(mem::align_of::<T>())
        .exact())
    }
}
