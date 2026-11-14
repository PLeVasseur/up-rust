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

//! Canonical status codes.

/// Canonical status codes for uProtocol operations.
///
/// These mirror the gRPC canonical codes, as required by the uProtocol
/// specification; every transport and role reports failures through them.
// [impl->req~ustatus-data-model-impl~2]
#[derive(Copy, Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub enum UCode {
    /// Not an error; the operation completed successfully.
    Ok = 0,
    /// The operation was cancelled, typically by the caller.
    Cancelled = 1,
    /// Unknown error, or an error from a foreign error space.
    Unknown = 2,
    /// The caller specified an invalid argument, regardless of system state.
    InvalidArgument = 3,
    /// A deadline expired before the operation could complete.
    DeadlineExceeded = 4,
    /// A requested entity (listener, topic, resource) was not found.
    NotFound = 5,
    /// The entity a caller attempted to create already exists.
    AlreadyExists = 6,
    /// The caller lacks permission to execute the operation.
    PermissionDenied = 7,
    /// A resource (quota, buffer space, loan capacity) has been exhausted.
    ResourceExhausted = 8,
    /// The system is not in a state required for the operation.
    FailedPrecondition = 9,
    /// The operation was aborted, typically due to a concurrency conflict.
    Aborted = 10,
    /// The operation was attempted past the valid range.
    OutOfRange = 11,
    /// The operation is not implemented or supported by this transport or service.
    Unimplemented = 12,
    /// An internal invariant was broken; a bug in the implementation.
    Internal = 13,
    /// The service or link is currently unavailable; retrying may succeed.
    Unavailable = 14,
    /// Unrecoverable data loss or corruption.
    DataLoss = 15,
    /// The request lacks valid authentication credentials.
    Unauthenticated = 16,
}

impl UCode {
    /// Converts a defined wire value without allocating an error for unknown codes.
    #[must_use]
    pub const fn from_i32(value: i32) -> Option<Self> {
        Some(match value {
            0 => Self::Ok,
            1 => Self::Cancelled,
            2 => Self::Unknown,
            3 => Self::InvalidArgument,
            4 => Self::DeadlineExceeded,
            5 => Self::NotFound,
            6 => Self::AlreadyExists,
            7 => Self::PermissionDenied,
            8 => Self::ResourceExhausted,
            9 => Self::FailedPrecondition,
            10 => Self::Aborted,
            11 => Self::OutOfRange,
            12 => Self::Unimplemented,
            13 => Self::Internal,
            14 => Self::Unavailable,
            15 => Self::DataLoss,
            16 => Self::Unauthenticated,
            _ => return None,
        })
    }

    /// Returns the protobuf wire value of this code.
    #[must_use]
    pub fn value(&self) -> i32 {
        *self as i32
    }
}
