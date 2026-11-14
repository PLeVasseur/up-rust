// SPDX-License-Identifier: Apache-2.0
// Included inside FINAL-3's retirement_integration_tests; corrected queue cases.
// Required-result tests are deliberately ordinary tests, never should_panic or
// ignored. Diagnostic failures are preserved separately at their original source.
mod sdk_owned_queue_cases {
    use super::*;
    use super::std::boxed::Box;
    use crate::transport::slots::queue_schedule::{self, Event};
    use crate::transport::Overflow;

    // [utest->req~ssr-runtime-no-heap~1]
    // [utest->req~ssr-release-lease-drop~2]
    #[test]
    fn queue_correction_count_before_visible_owner_can_return() {
        static REGION: StartupRegion<{1 << 19}> = StartupRegion::new();
        let memory = REGION.claim().unwrap();
        let topic = TestUri(1).get_resource_uri(0x8001);
        let bytes = whole_frame(&memory, &topic, &17_u32.to_le_bytes());
        let setup = TransportSetup::lending_on(&memory, &CLOCK, PushLending).unwrap();
        let role = Subscriber::<u32>::on(&setup, topic).wire(TestWire {fail_encode:false})
            .qos(Qos::DEFAULT.with_depth(8).with_overflow(Overflow::DropOldest)).build().unwrap();
        let route = role.route;
        let runtime = setup.enter_runtime().unwrap();
        let _role = role.activate(&runtime).unwrap();
        let mut fired = false;
        let hook = queue_schedule::install(Box::new(move |event| {
            if event == Event::Linked(0) && !fired {
                fired = true;
                let owner = route.try_receive().unwrap().unwrap();
                assert_eq!(*owner, 17);
                drop(owner);
                assert_eq!(route.__queued_for_test(), 0, "no unsigned underflow");
            }
        }));
        heap_window(|| route.deliver_retained(RetainedFrame::new(StaticFrame {bytes,peer:PeerRef::UNKNOWN})).unwrap());
        drop(hook);
        assert_eq!(route.__queued_for_test(), 0);
        assert_eq!(runtime.counters(route.common.id).carried, 1);
    }

    #[test]
    fn queue_correction_prepublication_reservation_is_not_a_completed_message() {
        static REGION: StartupRegion<{1 << 19}> = StartupRegion::new();
        let memory=REGION.claim().unwrap(); let topic=TestUri(1).get_resource_uri(0x8001);
        let bytes=whole_frame(&memory,&topic,&23_u32.to_le_bytes());
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let role=Subscriber::<u32>::on(&setup,topic).wire(TestWire {fail_encode:false})
            .qos(Qos::DEFAULT.with_depth(8)).build().unwrap();
        let route=role.route; let runtime=setup.enter_runtime().unwrap(); let role=role.activate(&runtime).unwrap();
        let hook=queue_schedule::install(Box::new(move |event| if event==Event::Counted(0) {
            assert_eq!(route.__queued_for_test(),0); // prepared input is not unread before CAS
            assert!(route.try_receive().unwrap().is_none()); // offer still overlaps this receive
        }));
        route.deliver_retained(RetainedFrame::new(StaticFrame {bytes,peer:PeerRef::UNKNOWN})).unwrap();
        drop(hook); assert_eq!(*role.try_recv().unwrap().unwrap(),23);
    }

    #[test]
    fn queue_correction_required_completed_successor_not_hidden_by_paused_link() {
        static REGION: StartupRegion<{1 << 19}> = StartupRegion::new();
        let memory=REGION.claim().unwrap(); let topic=TestUri(1).get_resource_uri(0x8001);
        let a=whole_frame(&memory,&topic,&17_u32.to_le_bytes());
        let b=whole_frame(&memory,&topic,&29_u32.to_le_bytes());
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let role=Subscriber::<u32>::on(&setup,topic).wire(TestWire {fail_encode:false})
            .qos(Qos::DEFAULT.with_depth(8)).max_held(56).build().unwrap(); // N=65, D=8
        let route=role.route; let runtime=setup.enter_runtime().unwrap(); let _role=role.activate(&runtime).unwrap();
        let hook=queue_schedule::install(Box::new(move |event| if event==Event::TailSwapped(0) {
            route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:b,peer:PeerRef::UNKNOWN})).unwrap();
            assert!(route.try_receive().unwrap().is_some(), "completed successor cannot vanish behind a paused predecessor");
        }));
        route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:a,peer:PeerRef::UNKNOWN})).unwrap();
        drop(hook);
    }

    #[test]
    fn queue_correction_required_pop_lock_is_not_public_empty() {
        static REGION: StartupRegion<{1 << 19}> = StartupRegion::new();
        let memory=REGION.claim().unwrap(); let topic=TestUri(1).get_resource_uri(0x8001);
        let a=whole_frame(&memory,&topic,&17_u32.to_le_bytes());
        let b=whole_frame(&memory,&topic,&29_u32.to_le_bytes());
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let role=Subscriber::<u32>::on(&setup,topic).wire(TestWire {fail_encode:false})
            .qos(Qos::DEFAULT.with_depth(8)).max_held(56).build().unwrap();
        let route=role.route; let runtime=setup.enter_runtime().unwrap(); let role=role.activate(&runtime).unwrap();
        for bytes in [a,b] {route.deliver_retained(RetainedFrame::new(StaticFrame {bytes,peer:PeerRef::UNKNOWN})).unwrap();}
        let hook=queue_schedule::install(Box::new(move |event| if event==Event::PopLocked {
            assert!(route.try_receive().unwrap().is_some(), "two completed unread messages and only one pending receiver");
        }));
        let _=role.try_recv(); drop(hook);
    }

    #[test]
    fn queue_correction_required_drop_oldest_never_exceeds_depth() {
        static REGION: StartupRegion<{1 << 19}> = StartupRegion::new();
        let memory=REGION.claim().unwrap(); let topic=TestUri(1).get_resource_uri(0x8001);
        let a=whole_frame(&memory,&topic,&17_u32.to_le_bytes());
        let b=whole_frame(&memory,&topic,&29_u32.to_le_bytes());
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let role=Subscriber::<u32>::on(&setup,topic).wire(TestWire {fail_encode:false})
            .qos(Qos::DEFAULT.with_depth(1).with_overflow(Overflow::DropOldest)).max_held(3).build().unwrap();
        let route=role.route; let runtime=setup.enter_runtime().unwrap(); let _role=role.activate(&runtime).unwrap();
        route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:a,peer:PeerRef::UNKNOWN})).unwrap();
        let hook=queue_schedule::install(Box::new(move |event| if event==Event::BeforeSurplus {
            // Count alone includes reservations: demonstrate actual owners too.
            let first=route.try_receive().unwrap();
            let second=route.try_receive().unwrap();
            assert!(first.is_none() || second.is_none(), "D1 exposed two unread owners before eviction");
        }));
        route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:b,peer:PeerRef::UNKNOWN})).unwrap(); drop(hook);
    }

    #[test]
    fn queue_correction_required_consume_does_not_pay_stale_surplus_twice() {
        static REGION: StartupRegion<{1 << 19}> = StartupRegion::new();
        let memory=REGION.claim().unwrap(); let topic=TestUri(1).get_resource_uri(0x8001);
        let a=whole_frame(&memory,&topic,&17_u32.to_le_bytes());
        let b=whole_frame(&memory,&topic,&29_u32.to_le_bytes());
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let role=Subscriber::<u32>::on(&setup,topic).wire(TestWire {fail_encode:false})
            .qos(Qos::DEFAULT.with_depth(1).with_overflow(Overflow::DropOldest)).build().unwrap();
        let route=role.route; let runtime=setup.enter_runtime().unwrap(); let role=role.activate(&runtime).unwrap();
        route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:a,peer:PeerRef::UNKNOWN})).unwrap();
        let hook=queue_schedule::install(Box::new(move |event| if event==Event::BeforeSurplus {
            assert_eq!(*route.try_receive().unwrap().unwrap(),17);
        }));
        route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:b,peer:PeerRef::UNKNOWN})).unwrap(); drop(hook);
        assert_eq!(role.try_recv().unwrap().as_deref().copied(),Some(29), "stale count must not evict B after consume removed A");
        assert_eq!(runtime.counters(route.common.id).dropped_oldest,0);
    }

    #[test]
    fn queue_correction_drop_newest_and_wait_preserve_first_at_full() {
        static REGIONS:[StartupRegion<{1 << 19}>;2]=[const {StartupRegion::new()};2];
        for (region,overflow) in REGIONS.iter().zip([Overflow::DropNewest,Overflow::Wait {timeout:core::time::Duration::from_millis(7)}]) {
            let memory=region.claim().unwrap(); let topic=TestUri(1).get_resource_uri(0x8001);
            let a=whole_frame(&memory,&topic,&17_u32.to_le_bytes()); let b=whole_frame(&memory,&topic,&29_u32.to_le_bytes());
            let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
            let role=Subscriber::<u32>::on(&setup,topic).wire(TestWire {fail_encode:false})
                .qos(Qos::DEFAULT.with_depth(1).with_overflow(overflow)).build().unwrap();
            let route=role.route;let runtime=setup.enter_runtime().unwrap();let role=role.activate(&runtime).unwrap();
            route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:a,peer:PeerRef::UNKNOWN})).unwrap();
            assert_eq!(route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:b,peer:PeerRef::UNKNOWN})),Err(ReceiveError::SlotsExhausted));
            assert_eq!(*role.try_recv().unwrap().unwrap(),17); assert!(role.try_recv().unwrap().is_none());
            let c=runtime.counters(route.common.id);
            match overflow { Overflow::DropNewest=>{assert_eq!(c.dropped_newest,1);assert_eq!(c.busy,0);},
                Overflow::Wait {..}=>{assert_eq!(c.busy,1);assert_eq!(c.dropped_newest,0);}, other=>assert!(matches!(other, Overflow::DropNewest | Overflow::Wait {..}), "unexpected fixture overflow: {other:?}") }
        }
        // This checks the existing SDK receive refusal, not sender deadline retry.
    }

    #[test]
    fn queue_correction_reserved_publication_counts_exactly_once() {
        static REGION:StartupRegion<{1 << 19}>=StartupRegion::new();
        let memory=REGION.claim().unwrap(); let topic=TestUri(1).get_resource_uri(0x8001);
        let bytes=whole_frame(&memory,&topic,&31_u32.to_le_bytes());
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let role=Subscriber::<u32>::on(&setup,topic).wire(TestWire {fail_encode:false})
            .qos(Qos::DEFAULT.with_depth(1).with_overflow(Overflow::DropNewest)).build().unwrap();
        let route=role.route;let runtime=setup.enter_runtime().unwrap();let role=role.activate(&runtime).unwrap();
        assert!(route.begin_pull());assert_eq!(route.__queued_for_test(),1);
        route.deliver_retained_pulled(RetainedFrame::new(StaticFrame {bytes,peer:PeerRef::UNKNOWN})).unwrap();
        assert_eq!(route.__queued_for_test(),1);assert_eq!(*role.try_recv().unwrap().unwrap(),31);
        assert_eq!(route.__queued_for_test(),0);
    }

    #[test]
    fn queue_correction_n65_depth8_preserves_held_and_newest_eight() {
        static REGION:StartupRegion<{1 << 20}>=StartupRegion::new();
        let memory=REGION.claim().unwrap();let topic=TestUri(1).get_resource_uri(0x8001);
        let mut frames=[&[][..];72];for (i,slot) in frames.iter_mut().enumerate(){*slot=whole_frame(&memory,&topic,&(i as u32).to_le_bytes());}
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let role=Subscriber::<u32>::on(&setup,topic).wire(TestWire {fail_encode:false})
            .qos(Qos::DEFAULT.with_depth(8).with_overflow(Overflow::DropOldest)).max_held(56).build().unwrap();
        let route=role.route;let runtime=setup.enter_runtime().unwrap();let role=role.activate(&runtime).unwrap();
        let mut held=[const {None};56];
        for (slot,bytes) in held.iter_mut().zip(frames.iter()) {
            route.deliver_retained(RetainedFrame::new(StaticFrame {bytes,peer:PeerRef::UNKNOWN})).unwrap();
            *slot=Some(role.try_recv().unwrap().unwrap());
        }
        heap_window(|| {
            // 56 retained +8 unread +1 producer actually reaches index64.
            for bytes in frames.iter().skip(56){route.deliver_retained(RetainedFrame::new(StaticFrame {bytes,peer:PeerRef::UNKNOWN})).unwrap();}
            for (i,owner) in held.iter().enumerate(){assert_eq!(**owner.as_ref().unwrap(),i as u32);}
            drop(held[0].take()); // leave room for one application receive lease
            for expected in 64..72 {assert_eq!(*role.try_recv().unwrap().unwrap(),expected);}
            assert_eq!(runtime.counters(route.common.id).dropped_oldest,8);
            assert_eq!(route.__queued_for_test(),0);drop(held);
        });
    }

    #[test]
    fn queue_correction_unread_and_retained_native_owner_survive_pool_close() {
        static REGION:StartupRegion<{1 << 20}>=StartupRegion::new();
        static DROPS:AtomicU32=AtomicU32::new(0);
        let memory=REGION.claim().unwrap();let topic=TestUri(1).get_resource_uri(0x8001);
        let (agreement,metadata)=native_header(&memory,&topic);
        let c1=memory.place([IndexSlot::new()]).unwrap();let p1=memory.place(IndexPoolSetup::new(c1).unwrap().enter_runtime()).unwrap();
        let c2=memory.place([IndexSlot::new()]).unwrap();let p2=memory.place(IndexPoolSetup::new(c2).unwrap().enter_runtime()).unwrap();
        let setup=TransportSetup::lending_on(&memory,&CLOCK,NativePush {fault:false}).unwrap();
        let role=Subscriber::<Scalar>::on(&setup,topic).wire(crate::wire::NativeWire::new(agreement))
            .qos(Qos::DEFAULT.with_depth(8)).max_held(56).build().unwrap();
        setup.__seed_pool_free_for_test(0,u64::MAX-3).unwrap();
        let route=role.route;let runtime=setup.enter_runtime().unwrap();let role=role.activate(&runtime).unwrap();
        heap_window(|| {
            let first=NativeOwner {metadata,value:Scalar(73),not_sync:Cell::new(0),_tail:Tail {pool:p1,drops:&DROPS},credit:p1.try_acquire().unwrap()};
            route.deliver_retained(RetainedFrame::new(first)).unwrap();let held=role.try_recv().unwrap().unwrap();
            let second=NativeOwner {metadata,value:Scalar(91),not_sync:Cell::new(0),_tail:Tail {pool:p2,drops:&DROPS},credit:p2.try_acquire().unwrap()};
            route.deliver_retained(RetainedFrame::new(second)).unwrap(); // stays UNREAD through closure
            drop(held);assert_eq!(DROPS.load(SeqCst),1);assert_eq!(runtime.pool_lifetime(role.route_id()),Some(LifetimeState::Retired));
            assert!(matches!(p2.try_acquire(),Err(PoolError::Full)));
            let unread=role.try_recv().unwrap().unwrap();assert_eq!(unread.payload().unwrap().0,91);
            assert!(matches!(role.try_recv(),Err(ReceiveError::PoolRetired)));
            drop(unread);assert_eq!(DROPS.load(SeqCst),2);drop(p2.try_acquire().unwrap());
        });
        // NativeOwner is the retained SDK fixture, not an installed middleware Sample.
    }

    #[test]
    fn queue_correction_shared_publish_pop_return_conservation() {
        static REGION:StartupRegion<{1 << 20}>=StartupRegion::new();
        let memory=REGION.claim().unwrap();let topic=TestUri(1).get_resource_uri(0x8001);
        let mut frames=[&[][..];32];for (i,slot) in frames.iter_mut().enumerate(){*slot=whole_frame(&memory,&topic,&(i as u32).to_le_bytes());}
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let role=Subscriber::<u32>::on(&setup,topic).wire(TestWire {fail_encode:false})
            .qos(Qos::DEFAULT.with_depth(8).with_overflow(Overflow::DropOldest)).max_held(56).build().unwrap();
        let route=role.route;let runtime=setup.enter_runtime().unwrap();let role=role.activate(&runtime).unwrap();
        let seen=AtomicU32::new(0);let delivered=AtomicU32::new(0);
        let observe=|value:u32|{let bit=1_u32.checked_shl(value).unwrap();assert_eq!(seen.fetch_or(bit,SeqCst)&bit,0,"duplicate owner");delivered.fetch_add(1,SeqCst);};
        std::thread::scope(|scope| {
            for half in frames.chunks(16) {scope.spawn(move || heap_window(|| {
                for bytes in half {route.deliver_retained(RetainedFrame::new(StaticFrame {bytes,peer:PeerRef::UNKNOWN})).unwrap();}
            }));}
            for _ in 0..2 {let observe=&observe;scope.spawn(move || heap_window(|| {
                for _ in 0..64 {if let Some(owner)=route.try_receive().unwrap(){observe(*owner);drop(owner);}}
            }));}
        });
        while let Some(owner)=role.try_recv().unwrap(){observe(*owner);drop(owner);}
        let counters=runtime.counters(route.common.id);
        assert_eq!(delivered.load(SeqCst) as u128+counters.dropped_oldest as u128,32);
        assert_eq!(route.__queued_for_test(),0);
        // Bounded stress checks ownership/conservation only. None during racing
        // traffic is NOT used as evidence for exact-empty or lock-free progress.
    }

    #[test]
    fn queue_correction_paused_reclaimer_does_not_hold_other_operations() {
        static REGION:StartupRegion<{1 << 20}>=StartupRegion::new();
        let memory=REGION.claim().unwrap();let topic=TestUri(1).get_resource_uri(0x8001);
        let a=whole_frame(&memory,&topic,&17_u32.to_le_bytes());let b=whole_frame(&memory,&topic,&29_u32.to_le_bytes());
        let observed=memory.place(AtomicU32::new(0)).unwrap();
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let role=Subscriber::<u32>::on(&setup,topic).wire(TestWire {fail_encode:false})
            .qos(Qos::DEFAULT.with_depth(8).with_overflow(Overflow::DropOldest)).max_held(56).build().unwrap();
        let route=role.route;let runtime=setup.enter_runtime().unwrap();let role=role.activate(&runtime).unwrap();
        let observed:&'static AtomicU32=observed;
        let hook=queue_schedule::install(Box::new(move |event| {
            if matches!(event,Event::ReclaimClaimed(_)) && observed.fetch_add(1,SeqCst)==0 {
                for _ in 0..32 {
                    route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:b,peer:PeerRef::UNKNOWN})).unwrap();
                    assert_eq!(*route.try_receive().unwrap().unwrap(),29);
                }
            }
        }));
        heap_window(||route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:a,peer:PeerRef::UNKNOWN})).unwrap());
        drop(hook);assert!(observed.load(SeqCst)>0);assert_eq!(*role.try_recv().unwrap().unwrap(),17);
    }

    #[test]
    fn queue_correction_stale_reader_keeps_control_not_payload_tenure() {
        static REGION:StartupRegion<{1 << 20}>=StartupRegion::new();
        let memory=REGION.claim().unwrap();let topic=TestUri(1).get_resource_uri(0x8001);
        let a=whole_frame(&memory,&topic,&17_u32.to_le_bytes());let b=whole_frame(&memory,&topic,&29_u32.to_le_bytes());
        let observed=memory.place(AtomicU32::new(0)).unwrap();
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let role=Subscriber::<u32>::on(&setup,topic).wire(TestWire {fail_encode:false})
            .qos(Qos::DEFAULT.with_depth(1).with_overflow(Overflow::DropOldest)).max_held(3).build().unwrap();
        let route=role.route;let runtime=setup.enter_runtime().unwrap();let role=role.activate(&runtime).unwrap();
        route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:a,peer:PeerRef::UNKNOWN})).unwrap();
        let observed:&'static AtomicU32=observed;
        let hook=queue_schedule::install(Box::new(move |event| {
            if matches!(event,Event::ProtectedRoot(_)) && observed.fetch_add(1,SeqCst)==0 {
                for _ in 0..32 {route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:b,peer:PeerRef::UNKNOWN})).unwrap();}
            }
        }));
        heap_window(||assert_eq!(*role.try_recv().unwrap().unwrap(),29));
        drop(hook);assert!(observed.load(SeqCst)>0);assert_eq!(runtime.counters(route.common.id).dropped_oldest,32);
    }

    #[test]
    fn queue_correction_same_payload_index_reuses_before_old_publish_returns() {
        static REGION:StartupRegion<{1 << 20}>=StartupRegion::new();
        let memory=REGION.claim().unwrap();let topic=TestUri(1).get_resource_uri(0x8001);
        let a=whole_frame(&memory,&topic,&17_u32.to_le_bytes());let b=whole_frame(&memory,&topic,&29_u32.to_le_bytes());
        let observed=memory.place(AtomicU32::new(0)).unwrap();
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let role=Subscriber::<u32>::on(&setup,topic).wire(TestWire {fail_encode:false})
            .qos(Qos::DEFAULT.with_depth(8)).max_held(56).build().unwrap();
        let route=role.route;let runtime=setup.enter_runtime().unwrap();let role=role.activate(&runtime).unwrap();
        let observed:&'static AtomicU32=observed;
        let hook=queue_schedule::install(Box::new(move |event| {
            if event==Event::Linked(0) && observed.fetch_add(1,SeqCst)==0 {
                assert_eq!(*route.try_receive().unwrap().unwrap(),17);
                // Index0 returned to the actual first-fit pool, then reused
                // while the old publisher is still paused after its commit.
                for _ in 0..32 {
                    route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:b,peer:PeerRef::UNKNOWN})).unwrap();
                    assert_eq!(*route.try_receive().unwrap().unwrap(),29);
                }
            }
        }));
        heap_window(||route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:a,peer:PeerRef::UNKNOWN})).unwrap());
        drop(hook);assert!(observed.load(SeqCst)>0);assert!(role.try_recv().unwrap().is_none());
        assert_eq!(runtime.counters(route.common.id).carried,33);
    }

    #[test]
    fn queue_correction_reclaimer_current_root_observation_precedes_reader_scan() {
        static REGION:StartupRegion<{1 << 20}>=StartupRegion::new();
        let memory=REGION.claim().unwrap();let topic=TestUri(1).get_resource_uri(0x8001);
        let a=whole_frame(&memory,&topic,&17_u32.to_le_bytes());let b=whole_frame(&memory,&topic,&29_u32.to_le_bytes());
        let observed=memory.place(AtomicU32::new(0)).unwrap();
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let role=Subscriber::<u32>::on(&setup,topic).wire(TestWire {fail_encode:false})
            .qos(Qos::DEFAULT.with_depth(8).with_overflow(Overflow::DropOldest)).max_held(56).build().unwrap();
        let route=role.route;let runtime=setup.enter_runtime().unwrap();let role=role.activate(&runtime).unwrap();
        let observed:&'static AtomicU32=observed;
        let hook=queue_schedule::install(Box::new(move |event| {
            if matches!(event,Event::ReclaimRootChecked(_,true)) && observed.fetch_add(1,SeqCst)==0 {
                // Move the observed current root while reclamation is suspended
                // before its protection scan; this cannot revoke that observation.
                for _ in 0..32 {
                    route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:b,peer:PeerRef::UNKNOWN})).unwrap();
                    assert_eq!(*route.try_receive().unwrap().unwrap(),29);
                }
            }
        }));
        heap_window(||route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:a,peer:PeerRef::UNKNOWN})).unwrap());
        drop(hook);assert!(observed.load(SeqCst)>0);assert_eq!(*role.try_recv().unwrap().unwrap(),17);
    }

    #[test]
    fn queue_correction_carrier_drop_oldest_commits_depth_and_retained_owner() {
        use crate::transport::carrier::{CarrierRoute,CarriedWire};
        use crate::transport::Flow;
        use crate::transport::tests::{WIRE_ID,FAMILY_ID,ENCODING};
        static REGION:StartupRegion<{1 << 20}>=StartupRegion::new();
        let memory=REGION.claim().unwrap();let topic=TestUri(1).get_resource_uri(0x8001);
        let a=whole_frame(&memory,&topic,&[17,0,0,0]);let b=whole_frame(&memory,&topic,&[29,0,0,0]);
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let wires=[CarriedWire {wire_id:WIRE_ID,payload_family:FAMILY_ID,
            identity:crate::wire::WirePayloadIdentity::serialized(ENCODING),alignment:1}];
        let mut config=CarrierRoute::new(&topic,&wires,64);config.kind=Some(UMessageType::Publish);
        config.qos=Qos::DEFAULT.with_depth(1).with_overflow(Overflow::DropOldest);config.slots=3;
        let ingress=setup.carrier_ingress(&config).unwrap();let route=ingress.route;
        let runtime=setup.enter_runtime().unwrap();
        route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:a,peer:PeerRef::UNKNOWN})).unwrap();
        let hook=queue_schedule::install(Box::new(move |event| if event==Event::BeforeSurplus {
            let mut seen=0;
            route.carry_pulled(&mut |_|None,&mut |_|{seen+=1;Flow::Continue});
            assert!(seen<=1,"carrier depth1 must not expose two unread owners");
        }));
        heap_window(||route.deliver_retained(RetainedFrame::new(StaticFrame {bytes:b,peer:PeerRef::UNKNOWN})).unwrap());
        drop(hook);let mut seen=0;
        assert_eq!(runtime.receive(ingress,&mut |_|{seen+=1;Flow::Continue}).unwrap(),1);
        assert_eq!(seen,1);assert_eq!(runtime.counters(ingress.id()).dropped_oldest,0);
    }

    #[test]
    fn queue_correction_carrier_retained_budget_reserves_before_pop() {
        use crate::transport::carrier::{CarrierRoute,CarriedWire};
        use crate::transport::Flow;
        use crate::transport::tests::{WIRE_ID,FAMILY_ID,ENCODING};
        static REGION:StartupRegion<{1 << 20}>=StartupRegion::new();
        let memory=REGION.claim().unwrap();let topic=TestUri(1).get_resource_uri(0x8001);
        let bytes=whole_frame(&memory,&topic,&[17,0,0,0]);
        let setup=TransportSetup::lending_on(&memory,&CLOCK,PushLending).unwrap();
        let wires=[CarriedWire {wire_id:WIRE_ID,payload_family:FAMILY_ID,
            identity:crate::wire::WirePayloadIdentity::serialized(ENCODING),alignment:1}];
        let mut config=CarrierRoute::new(&topic,&wires,64);config.kind=Some(UMessageType::Publish);
        config.qos=Qos::DEFAULT.with_depth(2);config.slots=1;
        let ingress=setup.carrier_ingress(&config).unwrap();let route=ingress.route;
        let runtime=setup.enter_runtime().unwrap();
        for _ in 0..2 {route.deliver_retained(RetainedFrame::new(StaticFrame {bytes,peer:PeerRef::UNKNOWN})).unwrap();}
        heap_window(|| {
            assert_eq!(runtime.receive(ingress,&mut |_| {
                // The first frame owns the one existing retained credit.
                assert_eq!(route.carry_pulled(&mut |_|None,&mut |_|panic!("budget exceeded")),0);
                Flow::Stop
            }).unwrap(),1);
            assert_eq!(runtime.receive(ingress,&mut |_|Flow::Continue).unwrap(),1);
        });
    }
}
