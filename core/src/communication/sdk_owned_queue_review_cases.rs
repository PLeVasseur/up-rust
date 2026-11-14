// SPDX-License-Identifier: Apache-2.0
// Source-only G1-G3 additions. No accepted test or production algorithm is replaced.
#[cfg(test)]
mod sdk_owned_queue_review_cases {
    use super::*;
    use super::std::boxed::Box;
    use crate::transport::slots::queue_schedule::{self, Event};
    use crate::transport::inbound::TypedInbound;
    use crate::transport::{Flow, Overflow};
    use crate::transport::carrier::{CarrierRoute, CarriedWire};
    use crate::transport::tests::{WIRE_ID, FAMILY_ID, ENCODING};

    // No Clone: every issued incarnation must run both the outer destructor and
    // its last field destructor exactly once, including refusal and replacement.
    struct Oracle {
        issued: [AtomicU32; 8], started: [AtomicU32; 8], complete: [AtomicU32; 8],
    }
    impl Oracle {
        fn new() -> Self { Self { issued: [const { AtomicU32::new(0) }; 8],
            started: [const { AtomicU32::new(0) }; 8], complete: [const { AtomicU32::new(0) }; 8] } }
        fn issue(&self, tag: usize) {
            let n = self.issued[tag].load(SeqCst);
            assert_eq!(self.started[tag].load(SeqCst), n);
            assert_eq!(self.complete[tag].load(SeqCst), n, "previous owner still live");
            self.issued[tag].fetch_add(1, SeqCst);
        }
        fn native(&'static self, tag: usize, metadata: &'static [u8], value: u32) -> RetainedFrame {
            self.issue(tag); RetainedFrame::new(ObservedNative { metadata, value: Scalar(value), tail: OwnerTail { oracle: self, tag } })
        }
        fn frame(&'static self, tag: usize, bytes: &'static [u8]) -> RetainedFrame {
            self.issue(tag); RetainedFrame::new(ObservedFrame { bytes, tail: OwnerTail { oracle: self, tag } })
        }
        fn live(&self, tag: usize) {
            assert_eq!(self.issued[tag].load(SeqCst), self.complete[tag].load(SeqCst) + 1);
            assert_eq!(self.started[tag].load(SeqCst), self.complete[tag].load(SeqCst));
        }
        fn done(&self, tag: usize) {
            let n = self.issued[tag].load(SeqCst); assert!(n > 0);
            assert_eq!(self.started[tag].load(SeqCst), n);
            assert_eq!(self.complete[tag].load(SeqCst), n);
        }
        fn all_done(&self) { for tag in 0..8 { let n = self.issued[tag].load(SeqCst);
            assert_eq!(self.started[tag].load(SeqCst), n); assert_eq!(self.complete[tag].load(SeqCst), n); } }
    }
    struct OwnerTail { oracle: &'static Oracle, tag: usize }
    impl Drop for OwnerTail { fn drop(&mut self) {
        let n = self.oracle.complete[self.tag].fetch_add(1, SeqCst) + 1;
        assert_eq!(n, self.oracle.started[self.tag].load(SeqCst));
        assert_eq!(n, self.oracle.issued[self.tag].load(SeqCst));
    } }
    struct ObservedFrame { bytes: &'static [u8], tail: OwnerTail }
    impl Frame for ObservedFrame {
        fn encoded_metadata(&self) -> &[u8] { &[] }
        fn payload(&self) -> Option<LoanedPayload<'_>> {
            Some(LoanedPayload::new(self.bytes, PayloadLoanProvenance::OpaqueTransportLoan))
        }
        fn peer(&self) -> PeerRef { PeerRef::UNKNOWN }
    }
    impl Drop for ObservedFrame { fn drop(&mut self) {
        let n = self.tail.oracle.started[self.tail.tag].fetch_add(1, SeqCst) + 1;
        assert_eq!(n, self.tail.oracle.issued[self.tail.tag].load(SeqCst));
    } }
    // Inline owner payload, using the already retained SDK-only NativePush/
    // NativeWire fixture. This is not an installed middleware sample.
    struct ObservedNative { metadata: &'static [u8], value: Scalar, tail: OwnerTail }
    impl Frame for ObservedNative {
        fn encoded_metadata(&self) -> &[u8] { self.metadata }
        fn payload(&self) -> Option<LoanedPayload<'_>> {
            // SAFETY: initialized transparent u32; self owns it for this borrow.
            let data = unsafe { core::slice::from_raw_parts(core::ptr::from_ref(&self.value).cast::<u8>(), 4) };
            Some(LoanedPayload::new(data, PayloadLoanProvenance::OpaqueTransportLoan))
        }
        fn peer(&self) -> PeerRef { PeerRef::UNKNOWN }
    }
    impl Drop for ObservedNative { fn drop(&mut self) {
        let n = self.tail.oracle.started[self.tail.tag].fetch_add(1, SeqCst) + 1;
        assert_eq!(n, self.tail.oracle.issued[self.tail.tag].load(SeqCst));
    } }
    fn typed(memory: &crate::memory::StartupMemory, depth: u16, held: usize)
        -> (&'static TypedInbound<Scalar, InPlace>, TransportRuntime, &'static [u8]) {
        let topic = TestUri(1).get_resource_uri(0x8001);
        let (agreement, metadata) = native_header(memory, &topic);
        let setup = TransportSetup::lending_on(memory, &CLOCK, NativePush { fault: false }).unwrap();
        let role = Subscriber::<Scalar>::on(&setup, topic).wire(crate::wire::NativeWire::new(agreement))
            .qos(Qos::DEFAULT.with_depth(depth).with_overflow(Overflow::DropOldest))
            .max_held(held).build().unwrap();
        let route = role.route; let runtime = setup.enter_runtime().unwrap();
        let _active = role.activate(&runtime).unwrap(); (route, runtime, metadata)
    }
    fn bytes(memory: &crate::memory::StartupMemory, value: u32) -> &'static [u8] {
        whole_frame(memory, &TestUri(1).get_resource_uri(0x8001), &value.to_le_bytes())
    }
    fn receive(route: &'static TypedInbound<Scalar, InPlace>, value: u32) {
        let owner = route.try_receive().unwrap().unwrap(); assert_eq!(owner.payload().unwrap().0, value); drop(owner);
    }

    #[test]
    fn queue_review_late_reader_validates_reused_descriptor_tenure() {
        static REGION: StartupRegion<{ 1 << 20 }> = StartupRegion::new();
        let memory = REGION.claim().unwrap();
        let oracle: &'static Oracle = memory.place(Oracle::new()).unwrap();
        let hits: &'static [AtomicU32; 4] = memory.place([const { AtomicU32::new(0) }; 4]).unwrap();
        let (route, _runtime, metadata) = typed(&memory, 1, 2);
        let a = 17; let b = 29;
        route.deliver_retained(oracle.native(0, metadata, a)).unwrap();
        let old = route.__review_queue_snapshot().0;
        let hook = queue_schedule::install(Box::new(move |event| match event {
            Event::ReviewRootLoaded(_, root) if hits[0].fetch_add(1, SeqCst) == 0 => {
                assert_eq!(root, old);
                // Hook allocation/installation is outside every heap window.
                let inner = queue_schedule::install(Box::new(move |e| {
                    if let Event::ReviewCandidateClaimed(_, bank) = e {
                        if bank == old { hits[1].fetch_add(1, SeqCst); }
                    }
                }));
                heap_window(|| receive(route, 17)); oracle.done(0);
                for _ in 0..24 { heap_window(|| {
                    route.deliver_retained(oracle.native(1, metadata, b)).unwrap(); receive(route, 29);
                }); }
                assert!(hits[1].load(SeqCst) > 0, "old bank must actually be reused before hazard publication");
                heap_window(|| route.deliver_retained(oracle.native(1, metadata, b)).unwrap());
                drop(inner);
            }
            Event::ReviewRootRetried(_, _) => { hits[2].fetch_add(1, SeqCst); }
            Event::ReviewRootValidated(_, root, phase) => {
                assert_eq!(phase, 2); assert_eq!(root, route.__review_queue_snapshot().0);
                assert_eq!(route.__review_queue_snapshot().1, 1); hits[3].fetch_add(1, SeqCst);
            }
            _ => {}
        }));
        // Main operation crosses scheduler Box lifetime; nested production work
        // alone is measured above. No whole-schedule allocation claim.
        receive(route, 29); drop(hook);
        assert!(hits[0].load(SeqCst) > 0); assert!(hits[3].load(SeqCst) > 0);
        // Same numeric root is allowed only in its new validated tenure; exact B
        // value + actual bank reuse distinguish it from the destroyed old A.
        oracle.all_done(); assert!(route.try_receive().unwrap().is_none());
    }

    #[test]
    fn queue_review_noncurrent_protected_descriptor_reclaims_only_after_exit() {
        static REGION: StartupRegion<{ 1 << 20 }> = StartupRegion::new();
        let memory = REGION.claim().unwrap();
        let oracle: &'static Oracle = memory.place(Oracle::new()).unwrap();
        let hits: &'static [AtomicU32; 3] = memory.place([const { AtomicU32::new(0) }; 3]).unwrap();
        let (route, _runtime, metadata) = typed(&memory, 1, 2);
        let a = 17; let b = 29;
        route.deliver_retained(oracle.native(0, metadata, a)).unwrap(); let old = route.__review_queue_snapshot().0;
        let hook = queue_schedule::install(Box::new(move |event| {
            if let Event::ReviewRootValidated(_, root, _) = event {
                if hits[0].fetch_add(1, SeqCst) != 0 { return; } assert_eq!(root, old);
                let inner = queue_schedule::install(Box::new(move |e| match e {
                    Event::ReviewReclaimDecision(bank, false, true) if bank == old => { hits[1].fetch_add(1, SeqCst); }
                    Event::ReviewCandidateClaimed(_, bank) => assert_ne!(bank, old, "protected old tenure recycled"),
                    _ => {}
                }));
                heap_window(|| receive(route, 17)); oracle.done(0);
                for _ in 0..24 { heap_window(|| {
                    route.deliver_retained(oracle.native(1, metadata, b)).unwrap(); receive(route, 29);
                }); }
                assert!(hits[1].load(SeqCst) > 0, "must execute current=false AND protection=true scan");
                heap_window(|| route.deliver_retained(oracle.native(1, metadata, b)).unwrap()); drop(inner);
            }
        }));
        receive(route, 29); drop(hook);
        let later = queue_schedule::install(Box::new(move |e| {
            if let Event::ReviewCandidateClaimed(_, bank) = e { if bank == old { hits[2].fetch_add(1, SeqCst); } }
        }));
        heap_window(|| for _ in 0..24 { route.deliver_retained(oracle.native(1, metadata, b)).unwrap(); receive(route, 29); });
        drop(later); assert!(hits[2].load(SeqCst) > 0, "observe actual freed-bank reuse after reader exit");
        oracle.all_done();
    }

    #[test]
    fn queue_review_pop_commit_before_promotion_is_helped_and_owner_survives() {
        static REGION: StartupRegion<{ 1 << 20 }> = StartupRegion::new();
        let memory = REGION.claim().unwrap();
        let oracle: &'static Oracle = memory.place(Oracle::new()).unwrap();
        let hits: &'static [AtomicU32; 3] = memory.place([const { AtomicU32::new(0) }; 3]).unwrap();
        let (route, _runtime, metadata) = typed(&memory, 2, 2);
        let a = 17; let b = 29; let c = 41;
        route.deliver_retained(oracle.native(0, metadata, a)).unwrap(); let victim = route.__review_queue_snapshot().2.unwrap();
        route.deliver_retained(oracle.native(1, metadata, b)).unwrap();
        let hook = queue_schedule::install(Box::new(move |event| {
            if let Event::ReviewPopCommitted(_, old, next, index, phase) = event {
                if hits[0].fetch_add(1, SeqCst) != 0 { return; }
                assert_ne!(old, next); assert_eq!(index, victim); assert_eq!(phase, 4, "pause before candidate promotion"); oracle.live(0);
                assert_eq!(route.__review_queue_snapshot().0, next);
                let inner = queue_schedule::install(Box::new(move |e| match e {
                    Event::ReviewRootValidated(_, root, phase) if root == next => {
                        assert_eq!(phase, 2, "competing operation helped candidate promotion"); hits[1].fetch_add(1, SeqCst);
                    }
                    Event::Counted(incoming) => assert_ne!(incoming, victim),
                    Event::ReviewCandidateClaimed(_, bank) => { assert_ne!(bank, old); assert_ne!(bank, next); }
                    _ => {}
                }));
                heap_window(|| receive(route, 29)); oracle.done(1);
                for _ in 0..24 { heap_window(|| { route.deliver_retained(oracle.native(2, metadata, c)).unwrap(); receive(route, 41); }); oracle.live(0); }
                assert!(hits[1].load(SeqCst) > 0); drop(inner);
            }
        }));
        receive(route, 17); drop(hook); oracle.done(0); assert_eq!(hits[0].load(SeqCst), 1);
        let later = queue_schedule::install(Box::new(move |e| if let Event::Counted(index) = e {
            assert_eq!(index, victim); hits[2].fetch_add(1, SeqCst);
        }));
        heap_window(|| { route.deliver_retained(oracle.native(2, metadata, c)).unwrap(); receive(route, 41); });
        drop(later); assert_eq!(hits[2].load(SeqCst), 1); oracle.all_done();
    }

    #[test]
    fn queue_review_replacement_transfer_reuses_incoming_before_victim_clear() {
        static REGION: StartupRegion<{ 1 << 20 }> = StartupRegion::new();
        let memory = REGION.claim().unwrap();
        let oracle: &'static Oracle = memory.place(Oracle::new()).unwrap();
        let hits: &'static [AtomicU32; 3] = memory.place([const { AtomicU32::new(0) }; 3]).unwrap();
        let (route, runtime, metadata) = typed(&memory, 1, 1);
        let a = 17; let b = 29; let c = 41;
        route.deliver_retained(oracle.native(0, metadata, a)).unwrap(); let victim = route.__review_queue_snapshot().2.unwrap();
        let hook = queue_schedule::install(Box::new(move |event| {
            if let Event::ReviewOfferTransferred(incoming, old, next, Some(removed)) = event {
                hits[0].fetch_add(1, SeqCst); assert_ne!(old, next); assert_eq!(removed, victim); assert_ne!(incoming, victim);
                oracle.live(0); oracle.live(1);
                let inner = queue_schedule::install(Box::new(move |e| if let Event::Counted(index) = e {
                    assert_eq!(index, incoming, "same incoming index must really be reused"); assert_ne!(index, victim);
                    hits[1].fetch_add(1, SeqCst);
                }));
                heap_window(|| receive(route, 29)); oracle.done(1);
                for _ in 0..24 { heap_window(|| { route.deliver_retained(oracle.native(2, metadata, c)).unwrap(); receive(route, 41); }); oracle.live(0); }
                assert_eq!(hits[1].load(SeqCst), 24); drop(inner);
            }
        }));
        route.deliver_retained(oracle.native(1, metadata, b)).unwrap(); drop(hook);
        assert_eq!(hits[0].load(SeqCst), 1); oracle.done(0);
        assert_eq!(runtime.counters(route.common.id).dropped_oldest, 1);
        let later = queue_schedule::install(Box::new(move |e| if let Event::Counted(index) = e {
            assert_eq!(index, victim); hits[2].fetch_add(1, SeqCst);
        }));
        heap_window(|| { route.deliver_retained(oracle.native(2, metadata, c)).unwrap(); receive(route, 41); });
        drop(later); assert_eq!(hits[2].load(SeqCst), 1); oracle.all_done();
    }

    #[test]
    fn queue_review_d1_h1_n3_physical_full_preserves_unread_and_held() {
        static REGION: StartupRegion<{ 1 << 20 }> = StartupRegion::new();
        let memory = REGION.claim().unwrap();
        let oracle: &'static Oracle = memory.place(Oracle::new()).unwrap();
        let hit: &'static AtomicU32 = memory.place(AtomicU32::new(0)).unwrap();
        let (route, runtime, metadata) = typed(&memory, 1, 1);
        let x = 11; let a = 17; let b = 29; let c = 41;
        route.deliver_retained(oracle.native(0, metadata, x)).unwrap(); let held = route.try_receive().unwrap().unwrap();
        route.deliver_retained(oracle.native(1, metadata, a)).unwrap(); let before = route.__review_queue_snapshot();
        let hook = queue_schedule::install(Box::new(move |event| if let Event::Counted(prepared) = event {
            assert_eq!(hit.fetch_add(1, SeqCst), 0); assert_eq!(prepared, 2); assert_eq!(before.2, Some(1));
            // Actual pool has X=0, A=1, prepared B=2. C must not reach offer.
            assert_eq!(route.deliver_retained(oracle.native(3, metadata, c)), Err(ReceiveError::SlotsExhausted));
            assert_eq!(route.__review_queue_snapshot(), before);
            oracle.done(3); oracle.live(0); oracle.live(1); oracle.live(2);
            let counters = route.common.counters.snapshot();
            assert_eq!(counters.slots_exhausted, 1); assert_eq!(counters.dropped_oldest, 0);
            assert_eq!(counters.dropped_newest, 0); assert_eq!(counters.busy, 0);
        }));
        heap_window(|| route.deliver_retained(oracle.native(2, metadata, b)).unwrap()); drop(hook);
        assert_eq!(hit.load(SeqCst), 1); oracle.done(1); oracle.live(0); oracle.live(2); assert_eq!(held.payload().unwrap().0, 11);
        assert_eq!(runtime.counters(route.common.id).dropped_oldest, 1);
        heap_window(|| { drop(held); receive(route, 29); route.deliver_retained(oracle.native(3, metadata, c)).unwrap(); receive(route, 41); });
        oracle.all_done(); let counters = runtime.counters(route.common.id);
        assert_eq!(counters.slots_exhausted, 1); assert_eq!(counters.dropped_oldest, 1);
        assert_eq!(counters.dropped_newest, 0); assert_eq!(counters.busy, 0);
    }

    // Callback-lifetime guarantee permits observing the original byte pointer
    // until sink return, even when park consumes the safe API view. No pointer
    // survives the callback. Destructor counts independently detect early return.
    fn parked_cut(frame: crate::transport::carrier::CarriedFrame<'_>,
        route: &'static crate::transport::carrier::CarrierInbound, oracle: &'static Oracle) {
        let payload = frame.payload(); let data = payload.as_contiguous().unwrap();
        assert_eq!(data, &[17, 0, 0, 0]); let ptr = data.as_ptr(); let len = data.len();
        let parked = frame.park().unwrap(); drop(parked); oracle.live(0);
        assert_eq!(route.carry_pulled(&mut |_| None, &mut |_| panic!("retained budget returned early")), 0);
        oracle.live(0); oracle.live(1);
        // SAFETY: still in the originating sink invocation; documented carrier
        // hold protocol keeps original retained storage alive until sink returns.
        assert_eq!(unsafe { core::slice::from_raw_parts(ptr, len) }, &[17, 0, 0, 0]);
    }
    macro_rules! carrier_setup { ($memory:ident, $oracle:ident, $route:ident, $runtime:ident, $ingress:ident) => {
        let topic = TestUri(1).get_resource_uri(0x8001);
        let $oracle: &'static Oracle = $memory.place(Oracle::new()).unwrap();
        let a = bytes(&$memory, 17); let b = bytes(&$memory, 29);
        let setup = TransportSetup::lending_on(&$memory, &CLOCK, PushLending).unwrap();
        let wires = [CarriedWire { wire_id: WIRE_ID, payload_family: FAMILY_ID,
            identity: crate::wire::WirePayloadIdentity::serialized(ENCODING), alignment: 1 }];
        let mut config = CarrierRoute::new(&topic, &wires, 64); config.kind = Some(UMessageType::Publish);
        config.qos = Qos::DEFAULT.with_depth(2); config.slots = 1;
        let $ingress = setup.carrier_ingress(&config).unwrap(); let $route = $ingress.route;
        let $runtime = setup.enter_runtime().unwrap();
        $route.deliver_retained($oracle.frame(0, a)).unwrap(); $route.deliver_retained($oracle.frame(1, b)).unwrap();
    }; }
    #[test]
    fn queue_review_carrier_park_drop_defers_owner_and_credit_until_sink_exit() {
        static REGION: StartupRegion<{ 1 << 20 }> = StartupRegion::new(); let memory = REGION.claim().unwrap();
        carrier_setup!(memory, oracle, route, runtime, ingress);
        let mut cuts = 0;
        heap_window(|| {
            assert_eq!(runtime.receive(ingress, &mut |frame| { cuts += 1; parked_cut(frame, route, oracle); Flow::Stop }).unwrap(), 1);
            oracle.done(0); oracle.live(1);
            assert_eq!(runtime.receive(ingress, &mut |frame| {
                assert_eq!(frame.payload().as_contiguous().unwrap(), &[29, 0, 0, 0]); Flow::Continue
            }).unwrap(), 1);
            oracle.all_done(); assert_eq!(runtime.receive(ingress, &mut |_| panic!("duplicate owner")).unwrap(), 0);
        });
        assert_eq!(cuts, 1);
    }
    #[test]
    fn queue_review_carrier_park_drop_unwind_returns_owner_and_credit_once() {
        static REGION: StartupRegion<{ 1 << 20 }> = StartupRegion::new(); let memory = REGION.claim().unwrap();
        carrier_setup!(memory, oracle, route, runtime, ingress);
        let mut cuts = 0;
        // Intentional test unwind is outside heap_window. It grants no runtime
        // panic response and makes no allocation claim for panic infrastructure.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.receive(ingress, &mut |frame| { cuts += 1; parked_cut(frame, route, oracle); panic!("test-only callback unwind"); }).unwrap();
        }));
        assert!(result.is_err()); assert_eq!(cuts, 1); oracle.done(0); oracle.live(1);
        heap_window(|| {
            assert_eq!(runtime.receive(ingress, &mut |frame| {
                assert_eq!(frame.payload().as_contiguous().unwrap(), &[29, 0, 0, 0]); Flow::Continue
            }).unwrap(), 1);
            oracle.all_done(); assert_eq!(runtime.receive(ingress, &mut |_| panic!("duplicate owner")).unwrap(), 0);
        });
    }
}
