// SPDX-License-Identifier: Apache-2.0
//! Deliberately allocating control for the same prepared SDK send path.

use std::{
    convert::Infallible,
    mem::MaybeUninit,
    ops::Range,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use up_rust::{
    FrameSink, InitializedTx, LoanError, PoolFrame, PreparedTransportCore, PreparedTxStorage,
    RouteLimits, SendError, SetupError, TxFrameLayout, UUri, WrittenHeader,
};

static NEXT: AtomicU64 = AtomicU64::new(1);

pub(super) struct HeapCore(u64);
impl HeapCore {
    pub(super) fn new() -> Self {
        Self(
            NEXT.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .expect("control core identity available"),
        )
    }
}
pub(super) struct HeapRoute {
    owner: u64,
    limits: RouteLimits,
}
pub(super) struct HeapTx {
    owner: u64,
    bytes: Box<[MaybeUninit<u8>]>,
    metadata_len: usize,
    payload: Option<Range<usize>>,
    recorded: bool,
}

impl Drop for HeapTx {
    fn drop(&mut self) {
        for byte in self
            .bytes
            .get_mut(..self.metadata_len)
            .expect("owned header extent")
        {
            // SAFETY: these slots are exclusively owned and live throughout Drop.
            unsafe {
                std::ptr::write_volatile(byte, MaybeUninit::new(0));
            }
        }
        std::sync::atomic::compiler_fence(Ordering::SeqCst);
    }
}

// SAFETY: the Box and immutable extents are exclusive to HeapTx; accessors never
// resize/move bytes, payload is aligned beyond metadata, and recording only sets
// an out-of-band flag. The Box is reclaimed only after this loan is consumed.
unsafe impl PreparedTxStorage for HeapTx {
    fn metadata_uninit_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        self.bytes.get_mut(..self.metadata_len).unwrap()
    }
    fn payload_uninit_mut(&mut self) -> Option<&mut [MaybeUninit<u8>]> {
        self.payload
            .clone()
            .map(|range| self.bytes.get_mut(range).unwrap())
    }
    fn record_header(&mut self, header: WrittenHeader) -> Result<(), SendError> {
        if header.len != self.metadata_len
            || header.token.is_some_and(|(offset, len)| {
                offset.checked_add(len).is_none_or(|end| end > header.len)
            })
        {
            return Err(SendError::PayloadStorage);
        }
        self.recorded = true;
        Ok(())
    }
}

impl PreparedTransportCore for HeapCore {
    type Route = HeapRoute;
    type Tx<'a> = HeapTx;
    type Rx = PoolFrame;
    type Subscription = Infallible;

    fn prepare_route(
        &self,
        _: &UUri,
        _: Option<&UUri>,
        limits: RouteLimits,
    ) -> Result<Self::Route, SetupError> {
        if !limits.payload_alignment.is_power_of_two() {
            return Err(SetupError::InvalidConfig("control alignment"));
        }
        Ok(HeapRoute {
            owner: self.0,
            limits,
        })
    }
    fn loan<'a>(
        &'a self,
        route: &'a Self::Route,
        layout: TxFrameLayout,
    ) -> Result<Self::Tx<'a>, LoanError> {
        if route.owner != self.0 {
            return Err(LoanError::InvalidRoute);
        }
        if layout.metadata_len > route.limits.max_metadata_len
            || layout.payload_len.unwrap_or(0) > route.limits.max_payload_len
            || layout.payload_alignment > route.limits.payload_alignment
        {
            return Err(LoanError::SlotTooSmall);
        }
        if !layout.payload_alignment.is_power_of_two() {
            return Err(LoanError::SlotTooSmall);
        }
        let padding = if layout.payload_len.is_some() {
            layout.payload_alignment - 1
        } else {
            0
        };
        let length = layout
            .metadata_len
            .checked_add(padding)
            .and_then(|length| length.checked_add(layout.payload_len.unwrap_or(0)))
            .ok_or(LoanError::SlotTooSmall)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| LoanError::PoolExhausted)?;
        bytes.resize(length, MaybeUninit::uninit());
        let payload = if let Some(len) = layout.payload_len {
            let base = bytes.as_ptr() as usize;
            let start = base
                .checked_add(layout.metadata_len)
                .and_then(|value| value.checked_add(layout.payload_alignment - 1))
                .ok_or(LoanError::SlotTooSmall)?
                & !(layout.payload_alignment - 1);
            let offset = start - base;
            let end = offset.checked_add(len).ok_or(LoanError::SlotTooSmall)?;
            if end > bytes.len() {
                return Err(LoanError::SlotTooSmall);
            }
            Some(offset..end)
        } else {
            None
        };
        Ok(HeapTx {
            owner: self.0,
            bytes: bytes.into_boxed_slice(),
            metadata_len: layout.metadata_len,
            payload,
            recorded: false,
        })
    }
    fn publish<'a>(
        &'a self,
        route: &'a Self::Route,
        frame: InitializedTx<Self::Tx<'a>>,
    ) -> Result<(), SendError> {
        let frame = frame.into_storage();
        if route.owner != self.0 || frame.owner != self.0 || !frame.recorded {
            return Err(SendError::Delivery);
        }
        // SAFETY: InitializedTx proves these declared extents have been written;
        // no alignment gap is exposed and frame retains exclusive backing storage.
        let header = unsafe {
            std::slice::from_raw_parts(frame.bytes.as_ptr().cast::<u8>(), frame.metadata_len)
        };
        std::hint::black_box(header);
        if let Some(range) = &frame.payload {
            let bytes = frame.bytes.get(range.clone()).unwrap();
            // SAFETY: InitializedTx also covers this payload extent; frame stays live.
            let payload =
                unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<u8>(), bytes.len()) };
            std::hint::black_box(payload);
        }
        Ok(())
    }
    async fn attach(
        &self,
        _: &UUri,
        _: Option<&UUri>,
        _: Arc<dyn FrameSink<Self::Rx>>,
    ) -> Result<Self::Subscription, SetupError> {
        Err(SetupError::InvalidConfig(
            "heap control has no receive subscription",
        ))
    }
    fn detach(&self, subscription: &mut Self::Subscription) -> Result<(), SetupError> {
        match *subscription {}
    }
}
