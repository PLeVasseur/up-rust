// Source-only GCR-1 regression; UNCOMPILED and UNRUN. Included inside the
// existing SDK fixture module with Scalar, NativePush, CLOCK and native_header.
// Box-owned adversarial frames intentionally test Rust liveness, not the
// production no-runtime-heap/free profile or a real middleware sample.
#[cfg(test)]
mod rpc_ownership_regressions {
    use super::*;
    use crate::transport::envelope::CustomEnvelope;
    use crate::transport::inbound::{InboundDispatch, PeerTable, RxCommon};
    use crate::transport::rpc::{Call, CallTable, ClientResponses};
    use crate::frame::header::{HeaderLimits, RxHeader, TxHeaderParts, UFrameHeaderError};
    use crate::ucode::UCode;

    struct SecondParseRefusal { prefix: &'static [u8], calls: AtomicU32 }
    impl CustomEnvelope for SecondParseRefusal {
        fn name(&self) -> &'static str { "gcr-stateful-safe-parser" }
        fn max_head_len(&self, _: usize) -> usize { 256 }
        fn head_len(&self, _: &TxHeaderParts<'_>, _: Option<usize>) -> Result<usize, UFrameHeaderError> {
            Err(UFrameHeaderError::Malformed("receive-only adversarial fixture"))
        }
        fn write_head(&self, _: &TxHeaderParts<'_>, _: Option<usize>, _: &mut [u8]) -> Result<usize, UFrameHeaderError> {
            Err(UFrameHeaderError::Malformed("receive-only adversarial fixture"))
        }
        fn parse<'a>(&self, metadata: &'a [u8], payload: PayloadBytes<'a>, limits: &HeaderLimits)
            -> Result<(RxHeader<'a>, PayloadBytes<'a>), UFrameHeaderError>
        {
            // A: calls0/1 succeed. B: pre-admission2 succeeds, final-address3 fails.
            if self.calls.fetch_add(1, SeqCst) == 3 {
                return Err(UFrameHeaderError::Malformed("second parse refused"));
            }
            crate::frame::header::parse_header(self.prefix, metadata, limits).map(|h| (h, payload))
        }
    }
    struct BoxOwner { metadata: &'static [u8], value: std::boxed::Box<Scalar> }
    impl Frame for BoxOwner {
        fn encoded_metadata(&self) -> &[u8] { self.metadata }
        fn payload(&self) -> Option<LoanedPayload<'_>> {
            // SAFETY: Scalar is the existing initialized transparent-u32 fixture.
            // Its Box allocation lives for self's borrow, has exactly4 initialized
            // bytes, and is never mutated through any other alias while borrowed.
            let bytes = unsafe { core::slice::from_raw_parts(core::ptr::from_ref(&*self.value).cast::<u8>(), 4) };
            Some(LoanedPayload::new(bytes, PayloadLoanProvenance::OpaqueTransportLoan))
        }
        fn peer(&self) -> PeerRef { PeerRef::UNKNOWN }
    }
    fn id(n: u64) -> UUID {
        UUID::try_from_wire_pair((crate::transport::tests::WALL_MS << 16) | 0x7000,
            0x8000_0000_0000_0000 | n).unwrap()
    }
    fn response_header(memory: &crate::memory::StartupMemory,
        agreement: &crate::payload::agreement::NativeAgreement, source: &UUri,
        sink: &UUri, reqid: &UUID, n: u64) -> &'static [u8]
    {
        let identity = agreement.resolve::<Scalar>().unwrap();
        let message_id = id(n);
        let parts = TxHeaderParts { kind: UMessageType::Response, priority: Some(UPriority::CS4),
            id: &message_id, reqid: Some(reqid), ttl: None, comm_status: Some(UCode::Ok),
            permission_level: None, source, sink: Some(sink), token: None,
            traceparent: None, payload_encoding: Some(identity.encoding), native_type_token: identity.native_type_token };
        let mut prefix = [MaybeUninit::uninit(); 256];
        let prefix_len = crate::wire::render_selected_prefix(crate::wire::STABLE_CONTAINER_WIRE_ID,
            crate::wire::STABLE_CONTAINER_PAYLOAD_FAMILY_ID, &mut prefix).unwrap();
        // SAFETY: render_selected_prefix initialized exactly prefix_len bytes.
        let prefix = unsafe { core::slice::from_raw_parts(prefix.as_ptr().cast::<u8>(), prefix_len) };
        let target = memory.bytes(crate::frame::header::header_len(prefix, &parts).unwrap()).unwrap();
        crate::frame::header::write_header_uninit(prefix, &parts, target).unwrap();
        // SAFETY: the writer initialized this whole extent in permanent startup memory.
        unsafe { core::slice::from_raw_parts(target.as_ptr().cast::<u8>(), target.len()) }
    }

    #[test]
    fn rpc_retained_parse_error_after_reuse_never_exposes_previous_payload() {
        static REGION: StartupRegion<{ 1 << 20 }> = StartupRegion::new();
        let memory = REGION.claim().unwrap();
        let topic = TestUri(1).get_resource_uri(0x8001);
        let source = TestUri(2).get_resource_uri(1);
        let sink = TestUri(1).get_source_uri();
        let (agreement, _) = native_header(&memory, &topic);
        let requests = [id(1), id(2), id(3)];
        let headers = [response_header(&memory, agreement, &source, &sink, &requests[0], 101),
            response_header(&memory, agreement, &source, &sink, &requests[1], 102),
            response_header(&memory, agreement, &source, &sink, &requests[2], 103)];
        let prefix_len = crate::wire::exact_prefix_len(crate::wire::STABLE_CONTAINER_WIRE_ID,
            crate::wire::STABLE_CONTAINER_PAYLOAD_FAMILY_ID, headers[0]).unwrap();
        let parser = memory.place(SecondParseRefusal { prefix: &headers[0][..prefix_len], calls: AtomicU32::new(0) }).unwrap();
        let setup = TransportSetup::lending_on(&memory, &CLOCK, NativePush { fault: false }).unwrap();
        // Reuse existing qualified fixture geometry/platform and native wire admission.
        // This separate receive-only table tests the private RPC ownership boundary;
        // it does not claim setup admission of the deliberately partial custom profile.
        let role = Subscriber::<Scalar>::on(&setup, topic).wire(crate::wire::NativeWire::new(agreement)).build().unwrap();
        let base = &role.route.common;
        let table = CallTable::<Scalar>::place(&memory, 1, 4, true, &[], None, setup.shared().lifetime_policy()).unwrap();
        let common = RxCommon::new(base.name, base.id, base.binding_route,
            Some(UMessageType::Response), source, Some(sink), &[], Envelope::Custom(parser),
            base.admitted, HeaderLimits { max_token_len: 0 }, PeerTable::new_in(&memory, 1).unwrap(),
            base.qos, base.heap, base.deviations, base.shared);
        let responses: &'static ClientResponses<Scalar> = memory.place(ClientResponses { common, table }).unwrap();
        let owners = [BoxOwner { metadata: headers[0], value: std::boxed::Box::new(Scalar(73)) },
            BoxOwner { metadata: headers[1], value: std::boxed::Box::new(Scalar(91)) },
            BoxOwner { metadata: headers[2], value: std::boxed::Box::new(Scalar(117)) }];
        let runtime = setup.enter_runtime().unwrap();
        let _role = role.activate(&runtime).unwrap();
        let mut previous = None;
        for (ordinal, owner) in owners.into_iter().enumerate() {
            let index = table.reserve(&requests[ordinal], crate::platform::Monotonic(u64::MAX), 0).unwrap();
            if let Some(old) = previous { assert_eq!(index, old); }
            previous = Some(index);
            // SAFETY: reserve just returned the sole obligation for this table.
            // responses and shared are its initialized route context. Only call
            // owns the return after adoption; the loop never returns index itself.
            let mut call: Call<Scalar, InPlace> = unsafe {
                Call::from_reserved(table, responses, responses.common.shared, index)
            };
            assert!(call.__taken_payload().is_err());
            responses.deliver_retained(RetainedFrame::new(owner)).unwrap();
            if ordinal == 1 {
                assert_eq!(parser.calls.load(SeqCst), 4);
                // This independent safe accessor is used BEFORE try_response.
                // On old source it can create a reference into freed owner A.
                assert!(call.__taken_payload().is_err());
                assert!(call.try_response().is_err());
                assert!(call.__taken_payload().is_err());
            } else {
                assert_eq!(call.__taken_payload().unwrap().0, if ordinal == 0 { 73 } else { 117 });
            }
            drop(call); // destroy the complete boxed owner before slot reuse
        }
        assert_eq!(parser.calls.load(SeqCst), 6);
    }
}
