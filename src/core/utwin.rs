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

use crate::{UOwnedFrame, UUri};

/// Native response entry for a uTwin last-message lookup.
#[derive(Clone, Debug, PartialEq)]
pub struct MessageResponse {
    pub topic: UUri,
    pub frame: Option<UOwnedFrame>,
}

/// Native response containing last known frames for requested topics.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GetLastMessagesResponse {
    pub messages: Vec<MessageResponse>,
}
