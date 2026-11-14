//! The `forbid(unsafe_code)` wire and wrapper, exercised end to end.

use std::alloc::System;
use std::sync::atomic::{AtomicUsize, Ordering};

use forbid_unsafe_wire::{Counted, Reading, ReadingWire};
use up_rust::heap_guard::HeapGuard;
use up_rust::startup::LEAKED_HEAP_REGIONS;
use up_rust::test_util::contract::assert_wire_contract;
use up_rust::test_util::lending::LendingBus;
use up_rust::two_mode::*;
use up_rust::{StaticUriProvider, UEntityId};

#[global_allocator]
static HEAP: HeapGuard = HeapGuard::new(System);

fn sample(value: &mut Reading) {
    *value = Reading {
        value: 7,
        sequence: 1,
    };
}

fn other(value: &mut Reading) {
    *value = Reading {
        value: 9,
        sequence: 2,
    };
}

fn zeroslot_derive_under_forbid_unsafe() {
    // `ReadingWire::init_slot` uses `zeroed`, which only a `ZeroSlot` can call,
    // and the derive runs inside a crate that forbids `unsafe`.
    let memory = StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 4096).unwrap();
    let slots = startup_values_for_test(&memory);
    assert!(slots.iter().all(|slot| *slot == Reading::default()));
}

fn startup_values_for_test(memory: &StartupMemory) -> &'static mut [Reading] {
    up_rust::two_mode::startup_values::<Reading, ReadingWire>(memory, &ReadingWire, 3).unwrap()
}

fn hand_written_wire_passes_the_contract_and_carries_traffic() {
    static CONTRACT_BYTES: AtomicUsize = AtomicUsize::new(0);
    static TRAFFIC_BYTES: AtomicUsize = AtomicUsize::new(0);
    assert_wire_contract(&ReadingWire, sample, other);
    assert_wire_contract(&Counted::new(ReadingWire, &CONTRACT_BYTES), sample, other);
    let entity = StaticUriProvider::for_entity("sensor", UEntityId::first_instance(1), 1).unwrap();
    let bus = LendingBus::new();
    let memory = || StartupMemory::from_source(&LEAKED_HEAP_REGIONS, 64 * 1024).unwrap();
    let a = TransportSetup::lending_in(&memory(), bus.participant(1)).unwrap();
    let b = TransportSetup::lending_in(&memory(), bus.participant(2)).unwrap();
    let publisher = Publisher::<Reading>::on(&a, &entity, 0x8001)
        .wire(Counted::new(ReadingWire, &TRAFFIC_BYTES))
        .build()
        .unwrap();
    // The wrapper names its own wire identity, so the receiver uses it too.
    let subscriber = Subscriber::<Reading>::on(&b, entity.get_resource_uri(0x8001))
        .wire(Counted::new(ReadingWire, &TRAFFIC_BYTES))
        .build()
        .unwrap();
    let tx = a.enter_runtime().unwrap();
    let rx = b.enter_runtime().unwrap();
    let publisher = publisher.activate(&tx).unwrap();
    let subscriber = subscriber.activate(&rx).unwrap();
    publisher
        .publish(&Reading {
            value: 42,
            sequence: 3,
        })
        .unwrap();
    let frame = subscriber.try_recv().unwrap().expect("a frame");
    assert_eq!((frame.value, frame.sequence), (42, 3));
    assert_eq!(
        TRAFFIC_BYTES.load(Ordering::Relaxed),
        8,
        "the wrapper's own encode ran"
    );
}

fn main() {
    zeroslot_derive_under_forbid_unsafe();
    println!("zeroslot_derive_under_forbid_unsafe: ok");
    hand_written_wire_passes_the_contract_and_carries_traffic();
    println!("hand_written_wire_passes_the_contract_and_carries_traffic: ok");
}
