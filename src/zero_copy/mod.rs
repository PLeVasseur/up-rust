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

//! Prepared physical storage and synchronous delivery mechanics.
//!
//! Bindings own storage lifetimes and physical mappings. Receive items remain
//! opaque; typed payload admission is separate from these storage contracts.

#[cfg(all(
    any(test, feature = "test-util"),
    feature = "transport-implementer-api"
))]
mod pool;
#[cfg(all(
    any(test, feature = "test-util"),
    feature = "transport-implementer-api"
))]
mod pool_core;
#[cfg(feature = "transport-implementer-api")]
mod prepared;
#[cfg(all(
    any(test, feature = "test-util"),
    feature = "transport-implementer-api"
))]
pub use pool_core::{PoolCore, PoolCoreConfig, PoolFrame};
#[cfg(feature = "transport-implementer-api")]
pub use prepared::{
    FrameSink, InitializedTx, LoanError, PreparedTransportCore, PreparedTxStorage, RouteLimits,
    SendError, SetupError, TxFrameLayout,
};
