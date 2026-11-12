// SPDX-License-Identifier: Apache-2.0
//! The per-message native path must not use the heap once an identity is resolved.
//!
//! Counting covers allocation, release and reallocation. A window charges only
//! the thread that opened it, so other tests in this binary (which build
//! profiles and descriptors concurrently) cannot pollute a measurement, and a
//! panic inside a window closes it through the guard's `Drop`.
#![cfg(feature = "zero-copy-transport")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};

use up_rust::heap_guard::{observe, HeapGuard, HeapReport, Scope};
use up_rust::payload::loan::BorrowPayload;
use up_rust::{
    NativeByteOrder, NativeProfile, NativeProfileAgreement, NativeRepresentation, PayloadCodec,
    PayloadEncoding, StableContainerPayload, StablePayload, StablePayloadField, UFrameMetadata,
    UUri,
};

#[global_allocator]
static ALLOCATOR: HeapGuard = HeapGuard::new(std::alloc::System);

/// Runs `operation` on the current thread and returns its result with the
/// heap operations charged to this thread while it ran.
fn counted<T>(operation: impl FnOnce() -> T) -> (T, HeapReport) {
    observe(Scope::Enrolled, operation)
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, StablePayload)]
#[stable_payload(type_name = "uprotocol.test.ZeroAllocSignal")]
struct Signal {
    id: u32,
    flag: bool,
    padding: [u8; 3],
    sample: u64,
}

#[repr(C)]
#[derive(Debug, Eq, PartialEq, StablePayload)]
#[stable_payload(type_name = "uprotocol.test.ZeroAllocPlain")]
struct Plain {
    id: u32,
    sample: u32,
}

const SIGNAL_ENCODING: PayloadEncoding = PayloadEncoding::private_use(0xF101);

fn profile(version: u32) -> NativeProfile {
    NativeProfile::new(
        "zero-alloc",
        version,
        SIGNAL_ENCODING,
        [
            Signal::native_representation(),
            Plain::native_representation(),
        ],
    )
    .unwrap()
}

fn fresh_agreement() -> NativeProfileAgreement {
    let profile = profile(1);
    NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap()
}

fn bytes_of<T: StablePayload>(value: &T) -> &[u8] {
    // SAFETY: StablePayload requires a fully initialized representation without
    // implicit padding. The slice covers this live value for its borrow lifetime
    // and is only read; no typed reference is formed from unvalidated bytes.
    unsafe {
        core::slice::from_raw_parts(
            core::ptr::from_ref(value).cast::<u8>(),
            core::mem::size_of::<T>(),
        )
    }
}

#[test]
fn warm_native_paths_do_not_allocate() {
    let agreement = fresh_agreement();
    // Warm-up: the first resolution of each type builds and validates its
    // descriptor once per generation; nothing after this line may allocate.
    let identity = StableContainerPayload::<Signal>::identity(&agreement).unwrap();
    let plain_identity = StableContainerPayload::<Plain>::identity(&agreement).unwrap();
    assert_ne!(identity, plain_identity);
    let metadata =
        UFrameMetadata::publish(UUri::try_from_parts("v", 0x0001_4210, 1, 0x9000).unwrap())
            .with_native_payload_identity(identity)
            .build()
            .unwrap();
    let signal = Signal {
        id: 7,
        flag: true,
        padding: [0; 3],
        sample: 9,
    };
    let plain = Plain { id: 1, sample: 2 };

    for _ in 0..3 {
        let (resolved, allocations) =
            counted(|| StableContainerPayload::<Signal>::identity(&agreement));
        assert_eq!(resolved.unwrap(), identity);
        allocations.assert_no_heap("warm identity resolution");
    }

    let (verified, allocations) =
        counted(|| StableContainerPayload::<Signal>::verify_metadata(&metadata, Some(&agreement)));
    verified.unwrap();
    allocations.assert_no_heap("metadata verification");

    let (checked, allocations) =
        counted(|| agreement.verify_carried(identity.encoding(), identity.token()));
    checked.unwrap();
    allocations.assert_no_heap("carried pair verification");

    // A constrained layout (bool) walks its bytes; an all-valid layout does not.
    let (borrowed, allocations) =
        counted(|| StableContainerPayload::<Signal>::borrow_payload(bytes_of(&signal)));
    assert_eq!(borrowed.unwrap(), &signal);
    allocations.assert_no_heap("constrained typed borrow");
    let (borrowed, allocations) =
        counted(|| StableContainerPayload::<Plain>::borrow_payload(bytes_of(&plain)));
    assert_eq!(borrowed.unwrap(), &plain);
    allocations.assert_no_heap("all-valid typed borrow");

    let clone = agreement.clone();
    let (same, allocations) = counted(|| clone.is_same_generation(&agreement));
    assert!(same);
    allocations.assert_no_heap("generation comparison");
}

#[test]
fn a_clone_shares_the_warm_cache_and_an_equal_load_does_not() {
    let agreement = fresh_agreement();
    let clone = agreement.clone();
    StableContainerPayload::<Signal>::identity(&clone).unwrap();
    // Warmed through the clone, hot through the original: one shared cache.
    let (resolved, allocations) =
        counted(|| StableContainerPayload::<Signal>::identity(&agreement));
    resolved.unwrap();
    allocations.assert_no_heap("a warm resolution");

    // An independently loaded copy of the same artifact is the same generation
    // (digest equality) but owns its own cache, so its first use warms up again.
    let reloaded = fresh_agreement();
    assert!(reloaded.is_same_generation(&agreement));
    let (resolved, allocations) = counted(|| StableContainerPayload::<Signal>::identity(&reloaded));
    resolved.unwrap();
    assert!(
        allocations.counts().allocations > 0,
        "a fresh load must resolve its own identity"
    );
    let (resolved, allocations) = counted(|| StableContainerPayload::<Signal>::identity(&reloaded));
    resolved.unwrap();
    allocations.assert_no_heap("a warm resolution");
}

#[test]
fn a_successor_is_a_new_generation_with_an_empty_cache() {
    let agreement = fresh_agreement();
    StableContainerPayload::<Signal>::identity(&agreement).unwrap();
    let next = profile(2);
    let successor = agreement.successor(Arc::new(next.clone()), &next).unwrap();
    assert!(!successor.is_same_generation(&agreement));
    let (resolved, allocations) =
        counted(|| StableContainerPayload::<Signal>::identity(&successor));
    resolved.unwrap();
    assert!(
        allocations.counts().allocations > 0,
        "a successor must not inherit its predecessor's cache"
    );
    // The predecessor keeps serving in-flight work without re-resolving.
    let (resolved, allocations) =
        counted(|| StableContainerPayload::<Signal>::identity(&agreement));
    resolved.unwrap();
    allocations.assert_no_heap("a warm resolution");
}

#[test]
fn concurrent_first_use_resolves_one_identity() {
    let agreement = Arc::new(fresh_agreement());
    let threads = 8;
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|_| {
            let agreement = Arc::clone(&agreement);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                StableContainerPayload::<Signal>::identity(&agreement).unwrap()
            })
        })
        .collect();
    let mut identities = handles.into_iter().map(|handle| handle.join().unwrap());
    let first = identities.next().unwrap();
    assert!(identities.all(|identity| identity == first));
    assert_eq!(first.encoding(), SIGNAL_ENCODING);
    let (resolved, allocations) =
        counted(|| StableContainerPayload::<Signal>::identity(&agreement));
    assert_eq!(resolved.unwrap(), first);
    allocations.assert_no_heap("a warm resolution");
}

/// A manual stable payload that counts how often its descriptor is built, so a
/// test can observe whether resolution ran rather than infer it from an error.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Probe {
    value: u32,
}

static PROBE_DESCRIPTOR_BUILDS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: `Probe` is a repr(C) struct of one u32 with no padding, pointers,
// borrowed state or drop glue; every bit pattern is a valid value and the
// descriptor below states its exact layout.
unsafe impl StablePayloadField for Probe {
    const FIELD_BITS_ALWAYS_VALID: bool = true;

    fn field_representation() -> NativeRepresentation {
        <Self as StablePayload>::native_representation()
    }

    fn validate_field_bytes(bytes: &[u8]) -> bool {
        bytes.len() == core::mem::size_of::<Self>()
    }
}

// SAFETY: see the field impl; the descriptor is derived from the Rust layout.
unsafe impl StablePayload for Probe {
    const TYPE_NAME: &'static str = "uprotocol.test.Probe";

    fn native_representation() -> NativeRepresentation {
        PROBE_DESCRIPTOR_BUILDS.fetch_add(1, Ordering::SeqCst);
        NativeRepresentation::new(
            Self::TYPE_NAME,
            Self::REVISION,
            core::mem::size_of::<Self>() as u64,
            core::mem::align_of::<Self>() as u64,
            NativeByteOrder::native(),
            vec![],
        )
        .unwrap()
    }
}

#[test]
fn failed_resolution_is_not_cached_and_success_is_resolved_once() {
    // `Probe` is not a member of this agreement: every attempt must rebuild the
    // descriptor, which proves that no negative result was cached.
    let unagreed = fresh_agreement();
    let before = PROBE_DESCRIPTOR_BUILDS.load(Ordering::SeqCst);
    assert!(matches!(
        StableContainerPayload::<Probe>::identity(&unagreed),
        Err(up_rust::UWireError::NativeProfile(
            up_rust::NativeProfileError::UnknownRepresentation
        ))
    ));
    assert!(matches!(
        StableContainerPayload::<Probe>::identity(&unagreed),
        Err(up_rust::UWireError::NativeProfile(
            up_rust::NativeProfileError::UnknownRepresentation
        ))
    ));
    let attempts = PROBE_DESCRIPTOR_BUILDS.load(Ordering::SeqCst) - before;
    assert_eq!(
        attempts, 2,
        "a failed resolution must be retried, not cached"
    );

    // Under an agreement that does contain `Probe`, the descriptor is built on
    // the first call only (plus at most one build for the per-process
    // representation proof); every later call is a warm hit.

    let profile = NativeProfile::new(
        "probe",
        1,
        PayloadEncoding::private_use(0xF1F0),
        [Probe::native_representation()],
    )
    .unwrap();
    let agreed = NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap();
    let before = PROBE_DESCRIPTOR_BUILDS.load(Ordering::SeqCst);
    let identity = StableContainerPayload::<Probe>::identity(&agreed).unwrap();
    for _ in 0..5 {
        assert_eq!(
            StableContainerPayload::<Probe>::identity(&agreed).unwrap(),
            identity
        );
    }
    let builds = PROBE_DESCRIPTOR_BUILDS.load(Ordering::SeqCst) - before;
    assert_eq!(
        builds, 1,
        "a successful resolution must build the descriptor once"
    );
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BorrowProbe {
    value: u32,
}

static BORROW_DESCRIPTOR_BUILDS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: One u32 has a fixed layout without padding, pointers or drop glue;
// the validator and descriptor below exactly describe that object representation.
unsafe impl StablePayloadField for BorrowProbe {
    const FIELD_BITS_ALWAYS_VALID: bool = true;
    fn field_representation() -> NativeRepresentation {
        BORROW_DESCRIPTOR_BUILDS.fetch_add(1, Ordering::SeqCst);
        NativeRepresentation::new(
            "uprotocol.test.BorrowProbe",
            0,
            4,
            4,
            NativeByteOrder::native(),
            vec![],
        )
        .unwrap()
    }
    fn validate_field_bytes(bytes: &[u8]) -> bool {
        bytes.len() == 4
    }
}

// SAFETY: The field contract above establishes fully initialized, stable bytes.
unsafe impl StablePayload for BorrowProbe {
    const TYPE_NAME: &'static str = "uprotocol.test.BorrowProbe";
}

#[test]
fn the_representation_proof_runs_once_per_type_and_never_per_borrow() {
    let probe = BorrowProbe { value: 5 };
    StableContainerPayload::<BorrowProbe>::representation_proven().unwrap();
    let before = BORROW_DESCRIPTOR_BUILDS.load(Ordering::SeqCst);
    for _ in 0..3 {
        let (borrowed, allocations) =
            counted(|| StableContainerPayload::<BorrowProbe>::borrow_payload(bytes_of(&probe)));
        assert_eq!(borrowed.unwrap(), &probe);
        allocations.assert_no_heap("a proven type's borrow");
    }
    assert_eq!(
        BORROW_DESCRIPTOR_BUILDS.load(Ordering::SeqCst),
        before,
        "the proof was rebuilt per borrow"
    );
}

/// A manual stable payload whose descriptor lies about its size; the
/// representation proof must reject it at the first byte-level use.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct Liar {
    value: u64,
}

// SAFETY: the memory contract holds (one u64, every pattern valid); only the
// descriptor is wrong, which is exactly what the proof must catch.
unsafe impl StablePayloadField for Liar {
    const FIELD_BITS_ALWAYS_VALID: bool = true;

    fn field_representation() -> NativeRepresentation {
        <Self as StablePayload>::native_representation()
    }

    fn validate_field_bytes(bytes: &[u8]) -> bool {
        bytes.len() == core::mem::size_of::<Self>()
    }
}

// SAFETY: see the field impl.
unsafe impl StablePayload for Liar {
    const TYPE_NAME: &'static str = "uprotocol.test.Liar";

    fn native_representation() -> NativeRepresentation {
        NativeRepresentation::new(
            Self::TYPE_NAME,
            Self::REVISION,
            4,
            4,
            NativeByteOrder::native(),
            vec![],
        )
        .unwrap()
    }
}

#[test]
fn a_lying_manual_descriptor_is_rejected_at_the_byte_level_entry_points() {
    let liar = Liar { value: 1 };
    assert!(matches!(
        StableContainerPayload::<Liar>::borrow_payload(bytes_of(&liar)),
        Err(up_rust::UWireError::InvalidPayload(_))
    ));
    // SAFETY: the unchecked lane's caller obligations hold for this value; the
    // call is expected to fail on the representation proof before any cast.
    let unchecked =
        unsafe { StableContainerPayload::<Liar>::borrow_payload_unchecked(bytes_of(&liar)) };
    assert!(matches!(
        unchecked,
        Err(up_rust::UWireError::InvalidPayload(_))
    ));
    assert!(matches!(
        StableContainerPayload::<Liar>::representation_proven(),
        Err(up_rust::UWireError::InvalidPayload(_))
    ));
}
