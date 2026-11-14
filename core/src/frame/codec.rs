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

//! Constants of the UFrame metadata field block, version 1.

/// Version of the field block emitted by the metadata field-block encoder.
pub const FRAME_FIELDS_VERSION: u8 = 1;

/// Presence bit: the frame has a sink URI.
pub const FIELD_SINK: u32 = 1 << 0;
/// Presence bit: the frame has a correlated request id.
pub const FIELD_REQID: u32 = 1 << 1;
/// Presence bit: the frame has a time-to-live.
pub const FIELD_TTL: u32 = 1 << 2;
/// Presence bit: the frame has a communication status.
pub const FIELD_COMM_STATUS: u32 = 1 << 3;
/// Presence bit: the frame has a permission level.
pub const FIELD_PERMISSION_LEVEL: u32 = 1 << 4;
/// Presence bit: the frame has an access token.
pub const FIELD_TOKEN: u32 = 1 << 5;
/// Presence bit: the frame has a W3C traceparent.
pub const FIELD_TRACEPARENT: u32 = 1 << 6;
/// Presence bit: the frame has a payload encoding.
pub const FIELD_PAYLOAD_ENCODING: u32 = 1 << 7;
/// Presence bit: an independent structural native type token is carried.
pub const FIELD_NATIVE_TYPE_TOKEN: u32 = 1 << 8;
/// All presence bits defined by field block version 1.
pub const FIELD_MASK_V1: u32 = (1 << 9) - 1;
