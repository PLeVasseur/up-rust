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

//! Message-attribute value types that need no heap.

pub mod payloadencoding;
pub mod umessagetype;
pub mod upriority;

pub use payloadencoding::{
    PayloadEncoding, PayloadEncodingError, PAYLOAD_ENCODING_PRIVATE_USE_MAX,
    PAYLOAD_ENCODING_PRIVATE_USE_MIN, PAYLOAD_ENCODING_RESERVED_MIN,
};
pub use umessagetype::UMessageType;
pub use upriority::UPriority;
