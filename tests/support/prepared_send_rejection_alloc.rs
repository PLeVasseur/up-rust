// SPDX-License-Identifier: Apache-2.0
//! Fault-injection coverage of filler/backend errors without invalid typed values.

use std::{
    convert::Infallible,
    mem::MaybeUninit,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
};
use up_rust::StablePayloadExt as _;
use up_rust::{
    FrameSink, InitializedTx, LoanError, MessageFields, NativeProfile, NativeProfileAgreement,
    PayloadEncoding, PoolFrame, PreparedTransportCore, PreparedTxStorage, RouteConfig, RouteLimits,
    SendError, SetupError, StablePayload, StablePayloadField, StablePayloadInit, TxFrameLayout,
    TxTemplate, UFrameHeaderError, UUri, WrittenHeader,
};

#[repr(C)]
#[derive(StablePayload, StablePayloadInit)]
#[stable_payload(type_name = "uprotocol.test.SendRejectionShape")]
struct Shape {
    value: u32,
}

#[repr(C)]
#[derive(StablePayloadInit)]
struct Conservative {
    value: u32,
}

// SAFETY: this repr(C) single-u32 type has the exact descriptor/layout of Shape,
// no padding, pointers, interior mutability or drop glue. The validator only
// accepts exact-length valid u32 bytes, conservatively refusing one valid value.
// That deliberate refusal exercises the SDK's defensive field check without
// creating an invalid Rust value or violating the field trait's safety contract.
unsafe impl StablePayloadField for Conservative {
    const FIELD_BITS_ALWAYS_VALID: bool = false;
    const DESCRIPTOR: &'static up_rust::StableDescriptor =
        <Shape as up_rust::StablePayloadField>::DESCRIPTOR;
    fn validate_field_bytes(bytes: &[u8]) -> bool {
        bytes.len() == 4 && bytes != 42_u32.to_ne_bytes()
    }
}
// SAFETY: the field contract above is exact, and every invalid-size/bit input is
// rejected before typed references are formed. All valid u32 bytes are initialized.
unsafe impl StablePayload for Conservative {
    const TYPE_NAME: &'static str = Shape::TYPE_NAME;
}

#[repr(align(8))]
struct Body([MaybeUninit<u8>; 16]);
struct Buffers {
    metadata: [MaybeUninit<u8>; 4096],
    body: Body,
}
struct FaultCore {
    buffers: tokio::sync::Mutex<Buffers>,
    mode: AtomicU8,
}
struct FaultTx<'a> {
    buffers: tokio::sync::MutexGuard<'a, Buffers>,
    layout: TxFrameLayout,
    mode: u8,
}

// SAFETY: the mutex guard exclusively retains stable Buffers storage. Even the
// deliberately short/misaligned/missing views never overlap or invalidate bytes;
// mode and layout do not change, record_header never mutates storage, and failing
// layouts are rejected before the SDK creates an initialized wrapper.
unsafe impl PreparedTxStorage for FaultTx<'_> {
    fn metadata_uninit_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        let len = self.layout.metadata_len - usize::from(self.mode == 1);
        self.buffers.metadata.get_mut(..len).unwrap()
    }
    fn payload_uninit_mut(&mut self) -> Option<&mut [MaybeUninit<u8>]> {
        if self.mode == 2 {
            return None;
        }
        let len = self.layout.payload_len? - usize::from(self.mode == 3);
        let start = usize::from(self.mode == 4);
        Some(self.buffers.body.0.get_mut(start..start + len).unwrap())
    }
    fn record_header(&mut self, header: WrittenHeader) -> Result<(), SendError> {
        if self.mode == 5 || header.len != self.layout.metadata_len {
            Err(SendError::PayloadStorage)
        } else {
            Ok(())
        }
    }
}
impl PreparedTransportCore for FaultCore {
    type Route = ();
    type Tx<'a> = FaultTx<'a>;
    type Rx = PoolFrame;
    type Subscription = Infallible;
    fn prepare_route(
        &self,
        _: &UUri,
        _: Option<&UUri>,
        _: RouteLimits,
    ) -> Result<Self::Route, SetupError> {
        Ok(())
    }
    fn loan<'a>(
        &'a self,
        _: &'a Self::Route,
        layout: TxFrameLayout,
    ) -> Result<Self::Tx<'a>, LoanError> {
        let mode = self.mode.load(Ordering::Relaxed);
        if mode == 7 {
            return Err(LoanError::SlotTooSmall);
        }
        if mode == 8 {
            return Err(LoanError::InvalidRoute);
        }
        let buffers = self
            .buffers
            .try_lock()
            .map_err(|_| LoanError::PoolExhausted)?;
        Ok(FaultTx {
            buffers,
            layout,
            mode,
        })
    }
    fn publish<'a>(
        &'a self,
        _: &'a Self::Route,
        frame: InitializedTx<Self::Tx<'a>>,
    ) -> Result<(), SendError> {
        let frame = frame.into_storage();
        if frame.mode == 6 {
            Err(SendError::Delivery)
        } else {
            Ok(())
        }
    }
    async fn attach(
        &self,
        _: &UUri,
        _: Option<&UUri>,
        _: Arc<dyn FrameSink<Self::Rx>>,
    ) -> Result<Self::Subscription, SetupError> {
        Err(SetupError::InvalidConfig(
            "filler fault fixture has no receive worker",
        ))
    }
    fn detach(&self, value: &mut Self::Subscription) -> Result<(), SetupError> {
        match *value {}
    }
}

fn agreement() -> NativeProfileAgreement {
    let profile = NativeProfile::new(
        "filler-faults",
        1,
        PayloadEncoding::private_use(0xF101),
        [Shape::native_representation()],
    )
    .unwrap();
    NativeProfileAgreement::new(Arc::new(profile.clone()), &profile).unwrap()
}

pub(super) fn run() {
    let core = Arc::new(FaultCore {
        buffers: tokio::sync::Mutex::new(Buffers {
            metadata: [MaybeUninit::uninit(); 4096],
            body: Body([MaybeUninit::uninit(); 16]),
        }),
        mode: AtomicU8::new(0),
    });
    let agreement = agreement();
    let route = TxTemplate::<Shape>::new(agreement.clone())
        .unwrap()
        .route(
            Arc::clone(&core),
            super::definition(),
            RouteConfig::default(),
        )
        .unwrap();
    let conservative = TxTemplate::<Conservative>::new(agreement)
        .unwrap()
        .route(
            Arc::clone(&core),
            super::definition(),
            RouteConfig::default(),
        )
        .unwrap();
    super::measure("send_backend_storage_and_delivery_rejections", || {
        for mode in 1..=5 {
            core.mode.store(mode, Ordering::Relaxed);
            assert_eq!(
                route.send(MessageFields::default(), |_| panic!(
                    "bad storage must reject before typed initialization"
                )),
                Err(SendError::PayloadStorage)
            );
        }
        core.mode.store(6, Ordering::Relaxed);
        assert_eq!(
            route.send(MessageFields::default(), |init| init
                .into_initializer()
                .value(1)
                .finish()),
            Err(SendError::Delivery)
        );
        for (mode, error) in [(7, LoanError::SlotTooSmall), (8, LoanError::InvalidRoute)] {
            core.mode.store(mode, Ordering::Relaxed);
            assert_eq!(
                route.send(MessageFields::default(), |_| panic!(
                    "failed loan must reject before initialization"
                )),
                Err(SendError::Loan(error))
            );
        }
        core.mode.store(0, Ordering::Relaxed);
        assert_eq!(
            conservative.send(MessageFields::default(), |init| init
                .into_initializer()
                .value(42)
                .finish()),
            Err(SendError::InvalidFieldBits)
        );
        conservative
            .send(MessageFields::default(), |init| {
                init.into_initializer().value(1).finish()
            })
            .unwrap();
        assert!(matches!(
            route.send_status(MessageFields::default(), up_rust::UCode::NotFound),
            Err(SendError::Header(UFrameHeaderError::Invalid(_)))
        ));
    });

    let foreign = Box::into_raw(Box::new([MaybeUninit::<u8>::uninit(); 4]));
    // SAFETY: this allocation stays live and exclusively borrowed by the proof
    // until send consumes/drops it; ownership is reconstructed only afterward.
    let storage = unsafe { &mut *foreign };
    let proof = Shape::init(storage).unwrap().value(7).finish();
    super::measure("send_foreign_initialization_witness", || {
        assert_eq!(
            route.send(MessageFields::default(), move |_| proof),
            Err(SendError::WitnessMismatch)
        );
    });
    // SAFETY: the send call consumed the proof, so its mutable borrow has ended.
    unsafe {
        drop(Box::from_raw(foreign));
    }
}
