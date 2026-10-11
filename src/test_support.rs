// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Private fixtures shared by SDK capability tests.

#[cfg(feature = "owned-frame-transport")]
pub(crate) fn topic(resource: u16) -> crate::UUri {
    crate::UUri::try_from_parts("vehicle", 0x4210, 1, resource).expect("fixture topic")
}

#[cfg(all(feature = "owned-frame-transport", feature = "usubscription"))]
#[derive(Default, Debug)]
pub(crate) struct RecordingMessageListener {
    messages: std::sync::Mutex<Vec<crate::UMessage>>,
}

#[cfg(all(feature = "owned-frame-transport", feature = "usubscription"))]
impl RecordingMessageListener {
    pub(crate) fn messages(&self) -> Vec<crate::UMessage> {
        self.messages
            .lock()
            .expect("messages lock poisoned")
            .clone()
    }
}

#[cfg(all(feature = "owned-frame-transport", feature = "usubscription"))]
#[async_trait::async_trait]
impl crate::UListener for RecordingMessageListener {
    async fn on_receive(&self, message: crate::UMessage) {
        self.messages
            .lock()
            .expect("messages lock poisoned")
            .push(message);
    }
}
