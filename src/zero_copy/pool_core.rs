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
//! Opt-in in-process prepared core for allocation and lifecycle conformance.

use super::pool::{PoolError, SlotLoan, SlotPool, SlotRef};
use crate::{
    Frame, FrameSink, InitializedTx, LoanError, LoanedPayload, PayloadLoanProvenance,
    PreparedTransportCore, PreparedTxStorage, RouteLimits, SendError, SetupError, TxFrameLayout,
    UUri, WrittenHeader,
};
use std::{
    mem::MaybeUninit,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
};

/// Capacities reserved by the in-process conformance core at creation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PoolCoreConfig {
    /// Number of reusable frame slots.
    pub slots: usize,
    /// Usable bytes in each aligned slot.
    pub slot_bytes: usize,
    /// Maximum supported power-of-two payload alignment.
    pub alignment: usize,
    /// Number of reserved route entries.
    pub max_routes: usize,
    /// Number of reserved receive registrations and per-route fan-out entries.
    pub max_registrations: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Key {
    index: usize,
    generation: u64,
}

struct RouteEntry {
    live: bool,
    generation: u64,
    source: UUri,
    sink: Option<UUri>,
    limits: RouteLimits,
    fanout: Vec<Key>,
}

struct SinkEntry {
    source: UUri,
    sink: Option<UUri>,
    sink_impl: Arc<dyn FrameSink<PoolFrame>>,
    active: AtomicBool,
}
struct SinkSlot {
    generation: u64,
    entry: Option<Arc<SinkEntry>>,
}
struct Tables {
    routes: Vec<RouteEntry>,
    sinks: Vec<SinkSlot>,
}
struct Inner {
    config: PoolCoreConfig,
    pool: Arc<SlotPool>,
    tables: Mutex<Tables>,
    // One reserved recipient scratch vector per TX slot. The publishing handle
    // pins that slot until every scratch reference is cleared.
    scratch: Vec<Mutex<Vec<Arc<SinkEntry>>>>,
}

struct RecipientScratch<'a>(MutexGuard<'a, Vec<Arc<SinkEntry>>>);
impl Drop for RecipientScratch<'_> {
    fn drop(&mut self) {
        self.0.clear();
    }
}

/// A heap-backed core whose message storage and tables are all reserved at setup.
///
/// Available only with `test-util` and the zero-copy implementer features. This
/// models SDK allocation/lifecycle contracts; it is not physical SHM evidence.
#[derive(Clone)]
pub struct PoolCore {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for PoolCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolCore")
            .field("config", &self.inner.config)
            .finish_non_exhaustive()
    }
}

/// Instance-bound route using one reserved table entry.
pub struct PoolRoute {
    inner: Arc<Inner>,
    key: Key,
}

impl std::fmt::Debug for PoolRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolRoute")
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl Drop for PoolRoute {
    fn drop(&mut self) {
        let mut tables = self
            .inner
            .tables
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(route) = tables
            .routes
            .get_mut(self.key.index)
            .filter(|route| route.generation == self.key.generation)
        {
            route.live = false;
            route.fanout.clear();
        }
    }
}

/// Instance-bound receive registration; Drop detaches its own entry.
pub struct PoolSubscription {
    inner: Arc<Inner>,
    key: Key,
    live: bool,
}

impl std::fmt::Debug for PoolSubscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolSubscription")
            .field("key", &self.key)
            .field("live", &self.live)
            .finish_non_exhaustive()
    }
}

impl Drop for PoolSubscription {
    fn drop(&mut self) {
        if self.live {
            detach_entry(&self.inner, self.key);
        }
    }
}

fn detach_entry(inner: &Inner, key: Key) -> bool {
    let mut tables = inner
        .tables
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(slot) = tables
        .sinks
        .get_mut(key.index)
        .filter(|slot| slot.generation == key.generation)
    else {
        return false;
    };
    let Some(entry) = slot.entry.take() else {
        return false;
    };
    entry.active.store(false, Ordering::SeqCst);
    for route in &mut tables.routes {
        route.fanout.retain(|candidate| *candidate != key);
    }
    drop(tables);
    // User-provided sink closure/wakeup never runs under the registry lock.
    entry.sink_impl.close();
    true
}

/// Retained immutable frame over one in-process pool slot.
pub struct PoolFrame {
    slot: SlotRef,
    present: bool,
}

impl std::fmt::Debug for PoolFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolFrame")
            .field("slot", &self.slot)
            .field("present", &self.present)
            .finish()
    }
}

impl Frame for PoolFrame {
    fn encoded_metadata(&self) -> &[u8] {
        self.slot.header()
    }
    fn payload(&self) -> Option<LoanedPayload<'_>> {
        self.present.then(|| {
            LoanedPayload::new(
                self.slot.payload(),
                PayloadLoanProvenance::OwnedReceiveLease,
            )
        })
    }
}

impl PoolFrame {
    /// Retains this slot without copying payload bytes or allocating a new owner.
    /// Returns None only when the checked reference count cannot be increased.
    #[must_use]
    pub fn try_retain(&self) -> Option<Self> {
        self.slot.try_retain().ok().map(|slot| Self {
            slot,
            present: self.present,
        })
    }
}

/// Exclusive transmit storage from the prepared in-process pool.
pub struct PoolTx {
    inner: Arc<Inner>,
    route: Key,
    slot: Option<SlotLoan>,
    layout: TxFrameLayout,
    payload_offset: usize,
}

impl std::fmt::Debug for PoolTx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolTx")
            .field("layout", &self.layout)
            .finish_non_exhaustive()
    }
}

// SAFETY: the exclusive SlotLoan owns this fixed slot; its checked, disjoint
// header/payload ranges never move or change. Recording layout only updates
// out-of-band bookkeeping. The slot cannot return to the pool while this loan lives.
unsafe impl PreparedTxStorage for PoolTx {
    fn metadata_uninit_mut(&mut self) -> &mut [MaybeUninit<u8>] {
        self.slot
            .as_mut()
            .expect("live TX loan")
            .bytes_uninit()
            .get_mut(..self.layout.metadata_len)
            .expect("checked metadata extent")
    }
    fn payload_uninit_mut(&mut self) -> Option<&mut [MaybeUninit<u8>]> {
        let len = self.layout.payload_len?;
        Some(
            self.slot
                .as_mut()
                .expect("live TX loan")
                .bytes_uninit()
                .get_mut(self.payload_offset..self.payload_offset + len)
                .expect("checked payload extent"),
        )
    }
    fn record_header(&mut self, header: WrittenHeader) -> Result<(), SendError> {
        if header.len != self.layout.metadata_len {
            return Err(SendError::PayloadStorage);
        }
        self.slot
            .as_mut()
            .expect("live TX loan")
            .record_header(header)
            .map_err(|_| SendError::PayloadStorage)
    }
}

impl Drop for PoolTx {
    fn drop(&mut self) {
        if let Some(slot) = self.slot.as_mut() {
            // Cancellation erases the whole owned metadata extent, including a
            // token whose initialization was cancelled before record_header.
            for byte in slot
                .bytes_uninit()
                .get_mut(..self.layout.metadata_len)
                .expect("checked metadata extent")
            {
                // SAFETY: this unpublished loan uniquely owns each u8 location;
                // zero stores initialize it without reading uninitialized bytes.
                unsafe { byte.as_mut_ptr().write_volatile(0) };
            }
            std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
        }
    }
}

fn reserved<T>(capacity: usize) -> Result<Vec<T>, SetupError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| SetupError::Allocation)?;
    Ok(values)
}

impl PoolCore {
    /// Reserves all slots, route/fan-out entries, registration entries and scratch storage.
    ///
    /// # Errors
    /// Rejects invalid geometry and setup allocation failures.
    pub fn new(config: PoolCoreConfig) -> Result<Self, SetupError> {
        let pool =
            SlotPool::new(config.slots, config.slot_bytes, config.alignment).map_err(|error| {
                match error {
                    PoolError::Allocation => SetupError::Allocation,
                    PoolError::InvalidCapacity
                    | PoolError::Exhausted
                    | PoolError::InvalidLayout
                    | PoolError::ReferenceLimit => SetupError::InvalidConfig("slot geometry"),
                    #[cfg(test)]
                    PoolError::Header(_) => SetupError::InvalidConfig("slot geometry"),
                }
            })?;
        let mut routes = reserved(config.max_routes)?;
        for _ in 0..config.max_routes {
            routes.push(RouteEntry {
                live: false,
                generation: 0,
                source: UUri::any(),
                sink: None,
                limits: RouteLimits {
                    max_metadata_len: 0,
                    max_payload_len: 0,
                    payload_alignment: 1,
                },
                fanout: reserved(config.max_registrations)?,
            });
        }
        let mut sinks = reserved(config.max_registrations)?;
        sinks.resize_with(config.max_registrations, || SinkSlot {
            generation: 0,
            entry: None,
        });
        let mut scratch = reserved(config.slots)?;
        for _ in 0..config.slots {
            scratch.push(Mutex::new(reserved(config.max_registrations)?));
        }
        Ok(Self {
            inner: Arc::new(Inner {
                config,
                pool,
                tables: Mutex::new(Tables { routes, sinks }),
                scratch,
            }),
        })
    }

    /// Number of currently unloaned/unretained slots.
    #[must_use]
    pub fn free_slots(&self) -> usize {
        self.inner.pool.free_slots()
    }
}

fn matches(source: &UUri, sink: Option<&UUri>, route: &RouteEntry) -> bool {
    source.matches(&route.source)
        && match (sink, route.sink.as_ref()) {
            (None, None) => true,
            (Some(filter), Some(actual)) => filter.matches(actual),
            _ => false,
        }
}

impl PreparedTransportCore for PoolCore {
    type Route = PoolRoute;
    type Tx<'a> = PoolTx;
    type Rx = PoolFrame;
    type Subscription = PoolSubscription;

    fn prepare_route(
        &self,
        source: &UUri,
        sink: Option<&UUri>,
        limits: RouteLimits,
    ) -> Result<Self::Route, SetupError> {
        if !limits.payload_alignment.is_power_of_two()
            || limits.payload_alignment > self.inner.config.alignment
        {
            return Err(SetupError::InvalidConfig("route alignment"));
        }
        let maximum = limits
            .max_metadata_len
            .checked_add(self.inner.config.alignment - 1)
            .and_then(|value| {
                (value & !(self.inner.config.alignment - 1)).checked_add(limits.max_payload_len)
            })
            .ok_or(SetupError::InvalidConfig("route size overflow"))?;
        if maximum > self.inner.config.slot_bytes {
            return Err(SetupError::InvalidConfig("route exceeds slot capacity"));
        }
        let mut tables = self
            .inner
            .tables
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let index = tables
            .routes
            .iter()
            .position(|route| !route.live)
            .ok_or(SetupError::TableFull)?;
        let Tables { routes, sinks } = &mut *tables;
        let route = routes.get_mut(index).expect("selected route entry");
        let generation = route
            .generation
            .checked_add(1)
            .ok_or(SetupError::TableFull)?;
        route.source = source.clone();
        route.sink = sink.cloned();
        route.limits = limits;
        route.fanout.clear();
        for (index, slot) in sinks.iter().enumerate() {
            if let Some(entry) = &slot.entry {
                if matches(&entry.source, entry.sink.as_ref(), route) {
                    route.fanout.push(Key {
                        index,
                        generation: slot.generation,
                    });
                }
            }
        }
        route.generation = generation;
        route.live = true;
        Ok(PoolRoute {
            inner: Arc::clone(&self.inner),
            key: Key { index, generation },
        })
    }

    fn loan<'a>(
        &'a self,
        route: &'a Self::Route,
        layout: TxFrameLayout,
    ) -> Result<Self::Tx<'a>, LoanError> {
        if !Arc::ptr_eq(&self.inner, &route.inner) {
            return Err(LoanError::InvalidRoute);
        }
        let tables = self
            .inner
            .tables
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = tables
            .routes
            .get(route.key.index)
            .filter(|entry| entry.live && entry.generation == route.key.generation)
            .ok_or(LoanError::InvalidRoute)?;
        if !layout.payload_alignment.is_power_of_two()
            || layout.payload_alignment > entry.limits.payload_alignment
            || layout.metadata_len > entry.limits.max_metadata_len
            || layout.payload_len.unwrap_or(0) > entry.limits.max_payload_len
        {
            return Err(LoanError::SlotTooSmall);
        }
        let offset = layout
            .metadata_len
            .checked_add(self.inner.config.alignment - 1)
            .map(|value| value & !(self.inner.config.alignment - 1))
            .ok_or(LoanError::SlotTooSmall)?;
        if offset
            .checked_add(layout.payload_len.unwrap_or(0))
            .is_none_or(|end| end > self.inner.config.slot_bytes)
        {
            return Err(LoanError::SlotTooSmall);
        }
        drop(tables);
        let slot = self
            .inner
            .pool
            .loan()
            .map_err(|_| LoanError::PoolExhausted)?;
        debug_assert_eq!(slot.capacity(), self.inner.config.slot_bytes);
        Ok(PoolTx {
            inner: Arc::clone(&self.inner),
            route: route.key,
            slot: Some(slot),
            layout,
            payload_offset: offset,
        })
    }

    fn publish<'a>(
        &'a self,
        route: &'a Self::Route,
        frame: InitializedTx<Self::Tx<'a>>,
    ) -> Result<(), SendError> {
        let mut tx = frame.into_storage();
        if !Arc::ptr_eq(&self.inner, &route.inner)
            || !Arc::ptr_eq(&self.inner, &tx.inner)
            || tx.route != route.key
        {
            return Err(SendError::Loan(LoanError::InvalidRoute));
        }
        let slot_index = tx.slot.as_ref().expect("live initialized loan").index();
        let mut recipients = RecipientScratch(
            self.inner
                .scratch
                .get(slot_index)
                .expect("slot scratch")
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        recipients.0.clear();
        {
            let tables = self
                .inner
                .tables
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let entry = tables
                .routes
                .get(route.key.index)
                .filter(|entry| entry.live && entry.generation == route.key.generation)
                .ok_or(SendError::Loan(LoanError::InvalidRoute))?;
            for key in &entry.fanout {
                if let Some(sink) = tables
                    .sinks
                    .get(key.index)
                    .filter(|slot| slot.generation == key.generation)
                    .and_then(|slot| slot.entry.as_ref())
                {
                    recipients.0.push(Arc::clone(sink));
                }
            }
        }
        let present = tx.layout.payload_len.is_some();
        let slot = tx.slot.take().expect("live initialized loan");
        // SAFETY: InitializedTx is created by the SDK filler after header/payload
        // initialization. The core checked this exact range/alignment at loan time.
        let published =
            unsafe { slot.publish(tx.payload_offset, tx.layout.payload_len.unwrap_or(0)) }
                .map_err(|_| SendError::PayloadStorage)?;
        for entry in recipients.0.iter() {
            if !entry.active.load(Ordering::SeqCst) {
                continue;
            }
            let retained = published.try_retain().map_err(|_| SendError::Delivery)?;
            // Receiver policy rejects/counts locally, like a physical subscriber;
            // its application-level DropNewest is not a sender rejection.
            entry.sink_impl.push(PoolFrame {
                slot: retained,
                present,
            });
        }
        recipients.0.clear();
        drop(recipients);
        drop(published);
        Ok(())
    }

    async fn attach(
        &self,
        source_filter: &UUri,
        sink_filter: Option<&UUri>,
        inbox: Arc<dyn FrameSink<Self::Rx>>,
    ) -> Result<Self::Subscription, SetupError> {
        let mut tables = self
            .inner
            .tables
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let index = tables
            .sinks
            .iter()
            .position(|slot| slot.entry.is_none())
            .ok_or(SetupError::TableFull)?;
        let generation = tables
            .sinks
            .get(index)
            .expect("selected sink")
            .generation
            .checked_add(1)
            .ok_or(SetupError::TableFull)?;
        let key = Key { index, generation };
        let slot = tables.sinks.get_mut(index).expect("selected sink");
        slot.generation = generation;
        slot.entry = Some(Arc::new(SinkEntry {
            source: source_filter.clone(),
            sink: sink_filter.cloned(),
            sink_impl: inbox,
            active: AtomicBool::new(true),
        }));
        for route in &mut tables.routes {
            if route.live && matches(source_filter, sink_filter, route) {
                route.fanout.push(key);
            }
        }
        Ok(PoolSubscription {
            inner: Arc::clone(&self.inner),
            key,
            live: true,
        })
    }

    fn detach(&self, subscription: &mut Self::Subscription) -> Result<(), SetupError> {
        if !Arc::ptr_eq(&self.inner, &subscription.inner) || !subscription.live {
            return Err(SetupError::InvalidConfig("foreign or closed subscription"));
        }
        if !detach_entry(&self.inner, subscription.key) {
            return Err(SetupError::InvalidConfig("stale subscription"));
        }
        subscription.live = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RegistrationInbox;
    use crate::{
        header_len, write_header_uninit, OverflowPolicy, PayloadEncoding, RegistrationConfig,
        TxHeaderParts, UFrameMetadata,
    };

    fn core(routes: usize, registrations: usize) -> PoolCore {
        PoolCore::new(PoolCoreConfig {
            slots: 1,
            slot_bytes: 512,
            alignment: 8,
            max_routes: routes,
            max_registrations: registrations,
        })
        .unwrap()
    }

    fn source() -> UUri {
        UUri::try_from_wire_parts("pool-core", 0x0001_0001, 1, 0x8001).unwrap()
    }
    fn limits() -> RouteLimits {
        RouteLimits {
            max_metadata_len: 128,
            max_payload_len: 4,
            payload_alignment: 4,
        }
    }

    fn initialized(core: &PoolCore, route: &PoolRoute) -> InitializedTx<PoolTx> {
        let metadata = UFrameMetadata::publish(source())
            .with_payload_encoding(PayloadEncoding::RAW)
            .build()
            .unwrap();
        let parts = TxHeaderParts::from(&metadata);
        let mut tx = core
            .loan(
                route,
                TxFrameLayout {
                    metadata_len: header_len(b"prefix", &parts).unwrap(),
                    payload_len: Some(4),
                    payload_alignment: 4,
                },
            )
            .unwrap();
        let written = write_header_uninit(b"prefix", &parts, tx.metadata_uninit_mut()).unwrap();
        tx.record_header(written).unwrap();
        for (slot, byte) in tx.payload_uninit_mut().unwrap().iter_mut().zip(b"data") {
            slot.write(*byte);
        }
        // The test filled every SDK-visible metadata/payload byte above.
        InitializedTx { storage: tx }
    }

    #[tokio::test]
    async fn delivery_is_opaque_and_frame_outlives_detachment_and_core() {
        let core = core(1, 1);
        let weak = Arc::downgrade(&core.inner);
        let route = core.prepare_route(&source(), None, limits()).unwrap();
        let inbox = Arc::new(
            RegistrationInbox::new(RegistrationConfig {
                capacity: 1,
                overflow: OverflowPolicy::DropNewest,
            })
            .unwrap(),
        );
        let mut subscription = core.attach(&source(), None, inbox.clone()).await.unwrap();
        core.publish(&route, initialized(&core, &route)).unwrap();
        assert_eq!(core.free_slots(), 0);
        let frame = inbox.recv().await.unwrap();
        core.detach(&mut subscription).unwrap();
        drop(subscription);
        drop(route);
        drop(core);
        assert!(weak.upgrade().is_none());
        assert_eq!(frame.payload().unwrap().bytes(), b"data");
        assert_eq!(
            frame.payload().unwrap().provenance(),
            PayloadLoanProvenance::OwnedReceiveLease
        );
        assert!(frame.encoded_metadata().starts_with(b"prefix"));
    }

    #[tokio::test]
    async fn foreign_handles_are_rejected_without_detaching_the_owner() {
        let first = core(1, 1);
        let second = core(1, 1);
        let route = first.prepare_route(&source(), None, limits()).unwrap();
        assert!(matches!(
            second.loan(
                &route,
                TxFrameLayout {
                    metadata_len: 32,
                    payload_len: Some(4),
                    payload_alignment: 4
                }
            ),
            Err(LoanError::InvalidRoute)
        ));
        let inbox = Arc::new(
            RegistrationInbox::new(RegistrationConfig {
                capacity: 1,
                overflow: OverflowPolicy::DropNewest,
            })
            .unwrap(),
        );
        let mut subscription = first.attach(&source(), None, inbox.clone()).await.unwrap();
        assert!(matches!(
            second.detach(&mut subscription),
            Err(SetupError::InvalidConfig(_))
        ));
        first.publish(&route, initialized(&first, &route)).unwrap();
        assert!(inbox.recv().await.is_some());
        first.detach(&mut subscription).unwrap();
    }

    #[test]
    fn route_capacity_and_slot_exhaustion_are_explicit() {
        let core = core(1, 0);
        let route = core.prepare_route(&source(), None, limits()).unwrap();
        assert!(matches!(
            core.prepare_route(&source(), None, limits()),
            Err(SetupError::TableFull)
        ));
        let layout = TxFrameLayout {
            metadata_len: 32,
            payload_len: Some(4),
            payload_alignment: 4,
        };
        let loan = core.loan(&route, layout).unwrap();
        assert!(matches!(
            core.loan(&route, layout),
            Err(LoanError::PoolExhausted)
        ));
        drop(loan);
        assert_eq!(core.free_slots(), 1);
        drop(route);
        assert!(core.prepare_route(&source(), None, limits()).is_ok());
    }
}
