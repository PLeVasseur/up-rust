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

//! Test, fake, proof, and fixture support. These are not production transport evidence.

#[cfg(feature = "payload-contract-fixtures")]
pub use crate::bench_fixtures;
#[cfg(feature = "test-util")]
pub use crate::utransport::MockLocalUriProvider;
#[cfg(feature = "test-util")]
pub use crate::{InMemoryZeroCopyTransport, MockTransport, MockUListener};

#[cfg(any(test, feature = "test-util"))]
#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    crate::StablePayload,
    crate::ByteBackedStablePayload,
    crate::StablePayloadInit,
)]
#[stable_payload(type_name = "uprotocol.test.StableTestBytes")]
pub struct StableTestBytes {
    pub bytes: [u8; 4],
}
