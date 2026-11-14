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

//! The re-check's N2, `PeerTable::forget` racing an `admit` of the same peer, and
//! the second re-check's R2-1, first sightings of one peer racing each other.
//!
//! N2: at `a3b8396` a late frame of the lost peer could raise the entry's
//! incarnation after `forget` had reset it, leaving a free entry that still
//! carried the lost peer's incarnation; the next peer to take it was then refused
//! as `StalePeer` on every frame after its first. These are the reviewer's three
//! programs (`review-recheck/proofs/tests/peer_table_race.rs`,
//! `peer_table_sequential.rs`, `declared_client_stale.rs`), kept as regressions.
//!
//! R2-1: at `85dae7d` two first frames of one peer admitted at once could take two
//! entries, so on a table sized to its client list another declared client was
//! refused for good. These are the second reviewer's programs
//! (`review-recheck2/proofs-core/tests/n2_race.rs`, `n2b_e2e.rs`), two more
//! interleavings in which an entry a first sighting passed becomes free behind it,
//! and the third reviewer's wider race of three first sightings
//! (`review-recheck3/proofs-core/tests/r21_extra.rs`), which fails natively far
//! more often than the narrow ones when the fix is undone.
//!
//! `ROUNDS` sets the rounds; under Miri the default is small, and many seeds with
//! preemption explore the interleavings.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    missing_docs
)]

use core::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use core::time::Duration;

#[path = "support/review_doubles.rs"]
mod doubles;
use doubles::*;

use up_rust_core::communication::two_mode::RpcServer;
use up_rust_core::memory::StartupRegion;
use up_rust_core::transport::binding::{InboundFrame, PayloadBytes, PeerRef, ReplyToken};
use up_rust_core::transport::inbound::PeerTable;
use up_rust_core::transport::{ReceiveError, TransportSetup};
use up_rust_core::uattributes::UMessageType;
use up_rust_core::uri::UUri;

const LOST: PeerRef = PeerRef {
    peer: 7,
    incarnation: 5,
};
/// The same peer as `LOST`, back at a later incarnation.
const LOST_LATER: PeerRef = PeerRef {
    peer: 7,
    incarnation: 6,
};
const NEXT: PeerRef = PeerRef {
    peer: 9,
    incarnation: 1,
};
const OTHER: PeerRef = PeerRef {
    peer: 11,
    incarnation: 1,
};
/// A second other peer and a second new one, for the three-entry race.
const OTHER_TWO: PeerRef = PeerRef {
    peer: 13,
    incarnation: 1,
};
const NEXT_TWO: PeerRef = PeerRef {
    peer: 15,
    incarnation: 1,
};

fn rounds(native: u32, miri: u32) -> u32 {
    std::env::var("ROUNDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(if cfg!(miri) { miri } else { native })
}

/// Runs `rounds` rounds in which each action runs once, on its own thread, all at
/// once; `start` runs before each round and `finish` after it, on this thread.
fn race_rounds(
    rounds: u32,
    mut start: impl FnMut(),
    actions: &[&(dyn Fn() + Sync)],
    mut finish: impl FnMut(u32),
) {
    let round = AtomicU32::new(0);
    let done = AtomicUsize::new(0);
    let threads = actions.len();
    std::thread::scope(|scope| {
        for &action in actions {
            let (round, done) = (&round, &done);
            scope.spawn(move || {
                let mut seen = 0;
                loop {
                    let now = loop {
                        let now = round.load(Ordering::Acquire);
                        if now != seen {
                            break now;
                        }
                        std::thread::yield_now();
                    };
                    if now == u32::MAX {
                        return;
                    }
                    seen = now;
                    action();
                    done.fetch_add(1, Ordering::AcqRel);
                }
            });
        }
        for now in 1..=rounds {
            start();
            done.store(0, Ordering::Release);
            round.store(now, Ordering::Release);
            while done.load(Ordering::Acquire) < threads {
                std::thread::yield_now();
            }
            finish(now);
        }
        round.store(u32::MAX, Ordering::Release);
    });
}

/// After a round on a two-entry table: whether `peer` holds at most one entry, so
/// that `third` still finds room. Leaves both of them forgotten.
fn third_peer_finds_room(table: &PeerTable, peer: PeerRef, third: PeerRef) -> bool {
    let room = table.admit(third).is_ok();
    table.forget(third);
    table.forget(peer);
    room
}

/// N2, direct. One entry; each round the lost peer holds it, then a late frame of
/// that peer races its `forget`, and the next peer takes the entry. The next peer
/// must never be refused as stale. At `a3b8396`: 61,020 of 200,000 rounds
/// poisoned, and under Miri seed 12 of `0..24`. Threaded: many seeds under Miri.
#[test]
fn n2_forget_racing_an_admit_never_poisons_the_next_peer() {
    static REGION: StartupRegion<4096> = StartupRegion::new();
    let memory = REGION.claim().expect("region");
    let table = PeerTable::new_in(&memory, 1).expect("table");
    let table = &table;
    let mut poisoned = 0_u32;
    let mut readmitted = 0_u32;
    race_rounds(
        rounds(20_000, 40),
        || assert!(table.admit(LOST).is_ok(), "the lost peer holds the entry"),
        &[
            // A late frame from the lost peer, on the receive path.
            &|| {
                let _ = table.admit(LOST);
            },
            // The binding reports the peer gone: `RuntimePort::peer_lost`.
            &|| table.forget(LOST),
        ],
        |_| {
            match (table.admit(NEXT), table.admit(NEXT)) {
                (Ok(_), Err(ReceiveError::StalePeer)) => poisoned += 1,
                // The late frame re-admitted the lost peer after `forget`: legitimate.
                (Err(ReceiveError::PeerTableFull), _) => readmitted += 1,
                _ => {}
            }
            table.forget(NEXT);
            table.forget(LOST);
        },
    );
    assert_eq!(
        poisoned, 0,
        "a freed entry kept a lost peer's incarnation ({readmitted} rounds re-admitted the lost peer)"
    );
}

/// The sequential control: the same calls one after the other, in both orders,
/// never poison, before or after the fix.
#[test]
fn n2_forget_and_admit_one_after_the_other_never_poison() {
    static REGION: StartupRegion<4096> = StartupRegion::new();
    let memory = REGION.claim().expect("region");
    let table = PeerTable::new_in(&memory, 1).expect("table");
    for forget_first in [false, true] {
        for _ in 0..rounds(1_000, 4) {
            assert!(table.admit(LOST).is_ok());
            if forget_first {
                table.forget(LOST);
                let _ = table.admit(LOST);
                table.forget(LOST);
            } else {
                let _ = table.admit(LOST);
                table.forget(LOST);
            }
            assert!(table.admit(NEXT).is_ok());
            assert_eq!(table.admit(NEXT).map(|_| ()), Ok(()));
            table.forget(NEXT);
        }
    }
}

/// R2-1, direct. Two first sightings of one peer at once, on a two-entry table:
/// the peer must end with one entry, so a third peer still finds room. At
/// `85dae7d`: 149 and 173 of 20,000 rounds took both entries, and under Miri every
/// seed of `0..8`. Threaded: many seeds under Miri.
#[test]
fn r2_1_two_first_sightings_of_one_peer_take_one_entry() {
    static REGION: StartupRegion<4096> = StartupRegion::new();
    let memory = REGION.claim().expect("region");
    let table = PeerTable::new_in(&memory, 2).expect("table");
    let table = &table;
    let mut doubled = 0_u32;
    race_rounds(
        rounds(20_000, 40),
        || {},
        &[
            &|| {
                let _ = table.admit(LOST);
            },
            &|| {
                let _ = table.admit(LOST);
            },
        ],
        |_| {
            if !third_peer_finds_room(table, LOST, OTHER) {
                doubled += 1;
            }
        },
    );
    assert_eq!(doubled, 0, "one peer took both entries");
}

/// R2-1 with the peer's `forget` in the race: first sightings at two incarnations
/// race the report that the peer was lost. Afterwards it holds at most one entry,
/// and once it is forgotten both entries are free. At `85dae7d`: 148 and 111 of
/// 20,000 rounds doubled. Threaded: many seeds under Miri.
#[test]
fn r2_1_first_sightings_racing_the_peers_forget_take_one_entry() {
    static REGION: StartupRegion<4096> = StartupRegion::new();
    let memory = REGION.claim().expect("region");
    let table = PeerTable::new_in(&memory, 2).expect("table");
    let table = &table;
    let (mut doubled, mut stuck) = (0_u32, 0_u32);
    race_rounds(
        rounds(20_000, 40),
        || {},
        &[
            &|| {
                let _ = table.admit(LOST);
            },
            &|| {
                let _ = table.admit(LOST_LATER);
            },
            &|| table.forget(LOST),
        ],
        |_| {
            if !third_peer_finds_room(table, LOST, OTHER) {
                doubled += 1;
            }
            // Everything forgotten: both entries are free for anyone.
            if !(table.admit(NEXT).is_ok() && table.admit(OTHER).is_ok()) {
                stuck += 1;
            }
            table.forget(NEXT);
            table.forget(OTHER);
        },
    );
    assert_eq!((doubled, stuck), (0, 0), "(doubled, stuck)");
}

/// R2-1, the entry a first sighting passed is freed behind it. Another peer holds
/// one of two entries and is reported lost while two first frames of one peer
/// race: one frame can pass the other peer's entry as taken and go on to claim
/// the second entry, while its twin claims the freed one. The peer must still
/// end with one entry. At `85dae7d`: 465 of 20,000 rounds doubled, and with the
/// reviewer's change alone 591, 134 and 835. At `4a3d15b`, whose check read the
/// other entry's word by a load, Miri doubled a round on its default seed under
/// both models: the load read the freed entry's zero after the twin's claim.
/// Threaded: many seeds under Miri.
#[test]
fn r2_1_first_sightings_racing_another_peers_departure_take_one_entry() {
    static REGION: StartupRegion<4096> = StartupRegion::new();
    let memory = REGION.claim().expect("region");
    let table = PeerTable::new_in(&memory, 2).expect("table");
    let table = &table;
    let mut doubled = 0_u32;
    race_rounds(
        rounds(20_000, 40),
        || assert!(table.admit(OTHER).is_ok(), "the other peer holds an entry"),
        &[
            &|| table.forget(OTHER),
            &|| {
                let _ = table.admit(LOST);
            },
            &|| {
                let _ = table.admit(LOST);
            },
        ],
        |_| {
            if !third_peer_finds_room(table, LOST, NEXT) {
                doubled += 1;
            }
            table.forget(OTHER);
        },
    );
    assert_eq!(doubled, 0, "one peer took both entries");
}

/// R2-1, the peer's own old entry is retiring. A late frame of the peer is still
/// being classified against its old entry when the peer is reported lost and
/// comes back at a later incarnation with two first frames at once. The old entry
/// is freed by its last user, possibly after one first frame has passed it; the
/// new tenure must take one entry. Threaded: many seeds under Miri.
#[test]
fn r2_1_first_sightings_racing_the_retirement_of_the_peers_old_entry_take_one_entry() {
    static REGION: StartupRegion<4096> = StartupRegion::new();
    let memory = REGION.claim().expect("region");
    let table = PeerTable::new_in(&memory, 2).expect("table");
    let table = &table;
    let mut doubled = 0_u32;
    race_rounds(
        rounds(20_000, 40),
        || assert!(table.admit(LOST).is_ok(), "the peer holds its old entry"),
        &[
            // A late frame, classified against the old entry.
            &|| {
                let _ = table.admit(LOST);
            },
            &|| table.forget(LOST),
            &|| {
                let _ = table.admit(LOST_LATER);
            },
            &|| {
                let _ = table.admit(LOST_LATER);
            },
        ],
        |_| {
            if !third_peer_finds_room(table, LOST, NEXT) {
                doubled += 1;
            }
        },
    );
    assert_eq!(doubled, 0, "one peer took both entries");
}

/// R2-1, wider: the third re-check's `three_twins_two_departures`. On a
/// three-entry table two other peers hold two entries and are both reported lost
/// while three first frames of one peer race. The peer must end with one entry,
/// so two new peers find room. This is the native guard: with the check after a
/// won claim removed (the re-check's m3), 2,265 of 20,000 rounds doubled, where
/// the narrower races above doubled in 1 round or none. Threaded: many seeds
/// under Miri.
#[test]
fn r2_1_three_first_sightings_racing_two_departures_take_one_entry() {
    static REGION: StartupRegion<4096> = StartupRegion::new();
    let memory = REGION.claim().expect("region");
    let table = PeerTable::new_in(&memory, 3).expect("table");
    let table = &table;
    let mut doubled = 0_u32;
    race_rounds(
        rounds(20_000, 40),
        || {
            assert!(table.admit(OTHER).is_ok(), "one other peer holds an entry");
            assert!(table.admit(OTHER_TWO).is_ok(), "so does another");
        },
        &[
            &|| table.forget(OTHER),
            &|| table.forget(OTHER_TWO),
            &|| {
                let _ = table.admit(LOST);
            },
            &|| {
                let _ = table.admit(LOST);
            },
            &|| {
                let _ = table.admit(LOST);
            },
        ],
        |_| {
            if !(table.admit(NEXT).is_ok() && table.admit(NEXT_TWO).is_ok()) {
                doubled += 1;
            }
            for peer in [NEXT, NEXT_TWO, LOST, OTHER, OTHER_TWO] {
                table.forget(peer);
            }
        },
    );
    assert_eq!(doubled, 0, "one peer took two entries");
}

fn request(from: &UUri, method: &UUri, n: u64) -> Vec<u8> {
    whole_frame(&Spec {
        kind: UMessageType::Request,
        id: uuid_at(2_000_000_000_000, n),
        reqid: None,
        ttl: Some(Duration::from_secs(600)),
        source: from.clone(),
        sink: Some(method.clone()),
        token: None,
        payload: Some(&1u32.to_le_bytes()),
    })
}

const CLIENT_A: PeerRef = PeerRef {
    peer: 9,
    incarnation: 1,
};
const CLIENT_B: PeerRef = PeerRef {
    peer: 7,
    incarnation: 5,
};

/// N2, end to end, through `RpcServer`. Two declared clients, so the table has two
/// entries. Two late frames of B's race B's `peer_lost`, then A sends twice. A must
/// never be refused. At `a3b8396`: 3 of 20,000 rounds, and from then on every
/// request of A's was refused (`stale_peer` reached 1,003). With one late frame a
/// round, this test once missed `a3b8396`'s table in 20,000 rounds (R2-5.1); the
/// second late frame widens the race. Threaded: many seeds under Miri.
#[test]
fn n2_a_declared_client_survives_another_clients_peer_lost_race() {
    let clock = Clock::leak(1_000_000_000, 2_000_000_000_000);
    let shared = Shared::leak();
    let setup = TransportSetup::handoff_on(&memory(), clock, HandoffDouble(shared)).unwrap();
    let method = uri(3, 1);
    let (a, b) = (uri(10, 0), uri(11, 0));
    let clients = [a.clone(), b.clone()];
    let server = RpcServer::<u32, u32>::on(&setup, &Uri(3), 1_u16)
        .wire(U32Wire { hook: None })
        .clients(&clients)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let server = server.activate(&runtime).unwrap();
    let (frame_a, frame_b) = (request(&a, &method, 1), request(&b, &method, 2));
    let port = shared.port();
    let route = shared.route(0);
    let deliver = |frame: &[u8], peer: PeerRef| {
        port.deliver(
            route,
            InboundFrame {
                metadata: &[],
                payload: PayloadBytes::one(frame),
                peer,
                reply: ReplyToken::default(),
            },
        )
    };
    let drain = || while server.try_recv().ok().flatten().is_some() {};
    assert_eq!(
        deliver(&frame_a, CLIENT_A),
        Ok(()),
        "control: A is admitted"
    );
    assert_eq!(
        deliver(&frame_b, CLIENT_B),
        Ok(()),
        "control: B is admitted"
    );
    drain();
    port.peer_lost(route, CLIENT_A);
    port.peer_lost(route, CLIENT_B);
    let mut refused = 0_u32;
    race_rounds(
        rounds(20_000, 40),
        || {
            // B holds an entry at its incarnation, as after any earlier request.
            let _ = deliver(&frame_b, CLIENT_B);
        },
        &[
            // Late frames of B's, admitted on two receiving threads.
            &|| {
                let _ = deliver(&frame_b, CLIENT_B);
            },
            &|| {
                let _ = deliver(&frame_b, CLIENT_B);
            },
            // The binding's thread reports B gone.
            &|| port.peer_lost(route, CLIENT_B),
        ],
        |_| {
            let _ = deliver(&frame_a, CLIENT_A);
            if matches!(deliver(&frame_a, CLIENT_A), Err(ReceiveError::StalePeer)) {
                refused += 1;
            }
            drain();
            port.peer_lost(route, CLIENT_A);
            port.peer_lost(route, CLIENT_B);
        },
    );
    assert_eq!(refused, 0, "declared client A was refused as a stale peer");
    assert_eq!(server.counters().stale_peer, 0);
}

/// R2-1, end to end, through `RpcServer`: two declared clients, so two entries.
/// Two of A's first requests are admitted at once, as by two threads pulling one
/// server route; then B sends, and must be admitted. At `85dae7d`: B was refused
/// `PeerTableFull` in 553 of 20,000 rounds, and, with nothing reporting A lost,
/// 1,000 of 1,000 of B's later requests too. Threaded: many seeds under Miri.
#[test]
fn r2_1_a_declared_client_is_admitted_after_another_clients_first_requests_race() {
    let clock = Clock::leak(1_000_000_000, 2_000_000_000_000);
    let shared = Shared::leak();
    let setup = TransportSetup::handoff_on(&memory(), clock, HandoffDouble(shared)).unwrap();
    let method = uri(3, 1);
    let (a, b) = (uri(10, 0), uri(11, 0));
    let clients = [a.clone(), b.clone()];
    let server = RpcServer::<u32, u32>::on(&setup, &Uri(3), 1_u16)
        .wire(U32Wire { hook: None })
        .clients(&clients)
        .build()
        .unwrap();
    let runtime = setup.enter_runtime().unwrap();
    let server = server.activate(&runtime).unwrap();
    let port = shared.port();
    let route = shared.route(0);
    let next_id = AtomicU64::new(1);
    let deliver = |from: &UUri, peer: PeerRef| {
        let frame = request(from, &method, next_id.fetch_add(1, Ordering::Relaxed));
        port.deliver(
            route,
            InboundFrame {
                metadata: &[],
                payload: PayloadBytes::one(&frame),
                peer,
                reply: ReplyToken::default(),
            },
        )
    };
    let drain = || while server.try_recv().ok().flatten().is_some() {};
    assert_eq!(deliver(&a, CLIENT_A), Ok(()), "control: A is admitted");
    assert_eq!(deliver(&b, CLIENT_B), Ok(()), "control: B is admitted");
    drain();
    port.peer_lost(route, CLIENT_A);
    port.peer_lost(route, CLIENT_B);
    let mut refused = 0_u32;
    race_rounds(
        rounds(20_000, 40),
        || {},
        &[
            // One of A's first requests, admitted on each of two threads.
            &|| {
                let _ = deliver(&a, CLIENT_A);
            },
            &|| {
                let _ = deliver(&a, CLIENT_A);
            },
        ],
        |_| {
            if matches!(deliver(&b, CLIENT_B), Err(ReceiveError::PeerTableFull)) {
                refused += 1;
            }
            drain();
            // Only so that rounds are independent; no real binding does this.
            port.peer_lost(route, CLIENT_A);
            port.peer_lost(route, CLIENT_B);
        },
    );
    assert_eq!(
        refused, 0,
        "declared client B was refused: A took both entries"
    );
}
