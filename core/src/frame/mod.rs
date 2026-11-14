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

//! The frames-as-data pieces that need no heap.
//!
//! The metadata field block's constants ([`codec`]), the fixed native header
//! ([`header`]) and the native identity values ([`native`]). The owned
//! `UFrameMetadata` model and its `std`-based codecs stay in `up-rust`.

pub mod codec;
pub mod header;
pub mod native;
pub mod view;
