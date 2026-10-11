// Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0
//! Private fixtures shared by SDK capability tests.

#[cfg(feature = "owned-frame-transport")]
pub(crate) fn topic(resource: u16) -> crate::UUri {
    crate::UUri::try_from_parts("vehicle", 0x4210, 1, resource).expect("fixture topic")
}

#[cfg(feature = "zero-copy-transport")]
pub(crate) fn self_agreement(profile: crate::NativeProfile) -> crate::NativeProfileAgreement {
    crate::NativeProfileAgreement::new(std::sync::Arc::new(profile.clone()), &profile)
        .expect("matching test profiles")
}

/// Aligned owned storage for byte-validity tests of encoded native frames.
#[cfg(all(feature = "zero-copy-transport", feature = "owned-frame-transport"))]
#[derive(Debug)]
pub(crate) struct AlignedRawFrame {
    encoded_metadata: Vec<u8>,
    payload: crate::UVecTxBuffer,
}

#[cfg(all(feature = "zero-copy-transport", feature = "owned-frame-transport"))]
impl AlignedRawFrame {
    pub(crate) fn new(
        metadata: crate::UFrameMetadata,
        encoded_metadata: Vec<u8>,
        bytes: &[u8],
        alignment: usize,
    ) -> Self {
        use crate::UTxBuffer;
        let mut payload =
            crate::UVecTxBuffer::with_alignment(metadata, bytes.len(), alignment).unwrap();
        payload.payload_mut().copy_from_slice(bytes);
        Self {
            encoded_metadata,
            payload,
        }
    }
}

#[cfg(all(feature = "zero-copy-transport", feature = "owned-frame-transport"))]
impl crate::wire_transport::UEncodedRxFrame for AlignedRawFrame {
    type PayloadReader<'a> = std::io::Cursor<&'a [u8]>;
    type PayloadSlices<'a> = std::iter::Once<&'a [u8]>;
    fn encoded_metadata(&self) -> &[u8] {
        &self.encoded_metadata
    }
    fn payload_len(&self) -> usize {
        self.payload.payload().len()
    }
    fn payload_reader(&self) -> Self::PayloadReader<'_> {
        std::io::Cursor::new(self.payload.payload())
    }
    fn payload_slices(&self) -> Self::PayloadSlices<'_> {
        std::iter::once(self.payload.payload())
    }
    fn try_contiguous_payload(&self) -> Option<&[u8]> {
        Some(self.payload.payload())
    }
}

#[cfg(all(feature = "zero-copy-transport", feature = "owned-frame-transport"))]
impl crate::wire_transport::UEncodedLoanedRxFrame for AlignedRawFrame {
    fn loaned_contiguous_payload(&self) -> Result<crate::LoanedPayload<'_>, crate::UWireError> {
        Ok(crate::LoanedPayload::new(
            self.payload.payload(),
            crate::PayloadLoanProvenance::OwnedReceiveLease,
        ))
    }
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

#[cfg(feature = "owned-frame-transport")]
pub(crate) use encoded::RawRx;
#[cfg(feature = "owned-frame-transport")]
pub(crate) use encoded::RecordingCore;

#[cfg(feature = "owned-frame-transport")]
mod encoded {
    #[cfg(feature = "zero-copy-transport")]
    use crate::wire_transport::UEncodedLoanedRxFrame;
    use crate::wire_transport::UEncodedRxFrame;
    #[cfg(feature = "owned-frame-transport")]
    use crate::wire_transport::{
        EncodedOwnedFrame, PreparedOwnedFrame, UEncodedOwnedListener, UOwnedTransportCore,
    };
    #[cfg(feature = "zero-copy-transport")]
    use crate::{LoanedPayload, PayloadLoanProvenance, UWireError};
    use crate::{UCode, UStatus, UUri};
    use async_trait::async_trait;
    use std::{
        collections::VecDeque,
        io::Cursor,
        sync::{Arc, Mutex},
    };

    #[derive(Clone, Debug)]
    pub(crate) struct RawRx {
        pub(crate) encoded_metadata: Vec<u8>,
        pub(crate) payload: Vec<u8>,
    }

    impl UEncodedRxFrame for RawRx {
        type PayloadReader<'a> = Cursor<&'a [u8]>;
        type PayloadSlices<'a> = std::iter::Once<&'a [u8]>;
        fn encoded_metadata(&self) -> &[u8] {
            &self.encoded_metadata
        }
        fn payload_len(&self) -> usize {
            self.payload.len()
        }
        fn payload_reader(&self) -> Self::PayloadReader<'_> {
            Cursor::new(&self.payload)
        }
        fn payload_slices(&self) -> Self::PayloadSlices<'_> {
            std::iter::once(self.payload.as_slice())
        }
        fn try_contiguous_payload(&self) -> Option<&[u8]> {
            Some(&self.payload)
        }
    }

    #[cfg(feature = "zero-copy-transport")]
    impl UEncodedLoanedRxFrame for RawRx {
        fn loaned_contiguous_payload(&self) -> Result<LoanedPayload<'_>, UWireError> {
            Ok(LoanedPayload::new(
                &self.payload,
                PayloadLoanProvenance::OwnedReceiveLease,
            ))
        }
    }

    #[derive(Default)]
    pub(crate) struct RecordingCore {
        pub(crate) received: Mutex<VecDeque<RawRx>>,
        pub(crate) receive_filters: Mutex<Vec<(UUri, Option<UUri>)>>,
        pub(crate) listener_filters: Mutex<Vec<(bool, UUri, Option<UUri>)>>,
        #[cfg(feature = "owned-frame-transport")]
        pub(crate) owned_listeners: Mutex<Vec<Arc<dyn UEncodedOwnedListener>>>,
    }

    #[cfg(feature = "owned-frame-transport")]
    #[async_trait]
    impl UOwnedTransportCore for RecordingCore {
        async fn register_encoded_owned_listener(
            &self,
            source: &UUri,
            sink: Option<&UUri>,
            listener: Arc<dyn UEncodedOwnedListener>,
        ) -> Result<(), UStatus> {
            self.listener_filters
                .lock()
                .unwrap()
                .push((true, source.clone(), sink.cloned()));
            self.owned_listeners.lock().unwrap().push(listener);
            Ok(())
        }
        async fn unregister_encoded_owned_listener(
            &self,
            source: &UUri,
            sink: Option<&UUri>,
            listener: Arc<dyn UEncodedOwnedListener>,
        ) -> Result<(), UStatus> {
            self.listener_filters
                .lock()
                .unwrap()
                .push((false, source.clone(), sink.cloned()));
            self.owned_listeners
                .lock()
                .unwrap()
                .retain(|registered| !Arc::ptr_eq(registered, &listener));
            Ok(())
        }
        async fn send_prepared_owned(&self, _frame: PreparedOwnedFrame) -> Result<(), UStatus> {
            Ok(())
        }
        async fn receive_encoded_owned(
            &self,
            source: &UUri,
            sink: Option<&UUri>,
        ) -> Result<EncodedOwnedFrame, UStatus> {
            self.receive_filters
                .lock()
                .unwrap()
                .push((source.clone(), sink.cloned()));
            self.received
                .lock()
                .unwrap()
                .pop_front()
                .map(|raw| EncodedOwnedFrame::new(raw.encoded_metadata, Some(raw.payload.into())))
                .ok_or_else(|| {
                    UStatus::fail_with_code(UCode::NotFound, "no test owned frame available")
                })
        }
    }
}
