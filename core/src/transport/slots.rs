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

//! Fixed startup storage for the runtime paths.
//!
//! A free-index stack, a ready queue of indices and byte slot pools. Everything
//! here is reserved in startup memory during initialization. After that no
//! operation acquires or releases heap memory, blocks, or panics on capacity.
//! The queue commits one immutable control descriptor with a native-word CAS.
//! Payload ownership and existing receive credits bound its control protection.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use crate::platform::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use crate::memory::{StartupError, StartupMemory};

use super::pool::{IndexPool, LifetimePolicy, LifetimeState, PoolError};

/// Internal queue adapter. Each popped index represents one non-cloneable lease;
/// exactly one owner returns it after its complete payload cleanup.
#[derive(Debug)]
pub(crate) struct FreeIndices {
    // Safety invariant: one live raw-index obligation exists for each held pool
    // cell. It is transferred, never duplicated, and ends only after all payload
    // borrows and destructor accesses have ended.
    pool: IndexPool<'static>,
}
impl FreeIndices {
    /// # Safety-usable invariant
    /// Success creates count initialized pool cells, count<u32::MAX, with no
    /// outstanding leases. Each successful pop has one unique in-range index.
    pub fn new_in(memory: &StartupMemory, count: usize) -> Result<Self, StartupError> {
        if count >= u32::MAX as usize { return Err(StartupError::MemoryExhausted { requested: count, remaining: 0 }); }
        Ok(Self { pool: IndexPool::new_in(memory, count)? })
    }
    pub fn with_policy(mut self, policy: LifetimePolicy) -> Self { self.pool = self.pool.with_policy(policy); self }
    /// # Safety-usable invariant
    /// `Ok(index)` transfers the only live return obligation for this pool cell;
    /// `index < count < u32::MAX`. Its payload may be accessed exclusively until
    /// ownership transfers; no other pop obtains it before its one matching push.
    pub fn pop(&self) -> Result<u32, PoolError> { self.pool.try_acquire().map(|lease| lease.into_index() as u32) }
    /// # Safety
    /// The caller must own this pool's unique live return obligation for `index`,
    /// from `pop` or a transfer of that obligation. All payload borrows/accesses,
    /// including the entire destructor, must have ended. Call exactly once and
    /// never access that incarnation again. This consumes the obligation, even
    /// when the pool is retired; it does not revoke any other owner.
    pub unsafe fn push(&self, index: u32) {
        // SAFETY: the unique obligation and completed cleanup are push's caller
        // contract; the lossless index conversion preserves the originating cell.
        unsafe { self.pool.return_index(index as usize) };
    }
    pub fn lifetime(&self) -> LifetimeState { self.pool.lifetime() }
    pub fn policy(&self) -> LifetimePolicy { self.pool.lifetime_policy() }
    /// Seeds control state only while the complete pool is exclusively in setup.
    /// # Safety
    /// Caller owns initialization rights for this pool and must exclude every
    /// acquisition, queue operation, observer and other reset for the call.
    /// No held/queued/retiring lease or payload reference may exist, including a
    /// forgotten lease or an outstanding raw-index return obligation. The pool
    /// has never entered runtime; reset is not a way to reclaim a live tenure.
    /// On success the selected FREE counter is initialized to `even`; ordinary
    /// acquisition may begin only after this call and setup publication complete.
    #[allow(unsafe_code)]
    #[cfg(any(test, up_rust_test_hooks))]
    pub unsafe fn seed_for_setup(&self, index: usize, even: u64) -> Result<(), super::pool::CapacityError> {
        // SAFETY: this function's caller supplies exclusive, never-runtime setup
        // rights for this same pool, excluding every live owner and observer.
        unsafe { self.pool.seed_for_setup(index, even) }
    }
    pub fn retire(&self) { self.pool.retire(); }
    pub fn warn(&self) { self.pool.warn(); }
}

// Control descriptors only: exactly one root determines unread membership.
// No descriptor owns another payload or another unread queue. See QUEUE-PROOF.md.
const NO_CONTROL: usize = usize::MAX;
const CONTROL_FREE: u32 = 0;
const CONTROL_PRIVATE: u32 = 1;
const CONTROL_PUBLISHED: u32 = 2;
const CONTROL_RECLAIMING: u32 = 3;
const CONTROL_CANDIDATE: u32 = 4;

struct QueueControl {
    // Safety invariant: FREE -> PRIVATE -> CANDIDATE is exclusively claimed by
    // one context. Only a root-CAS-committed candidate may become PUBLISHED.
    // RECLAIMING exclusively checks root and protections before FREE publication.
    phase: AtomicU32,
    // Safety invariant: a protected published tenure has len <= queue.depth;
    // entries 0..len are distinct live payload indices < queue.payload_slots.
    // The tenure's length/entries stay immutable until reclamation establishes
    // absence from root and every validated protection.
    len: AtomicU32,
}
struct QueueProtection {
    // Safety invariant: NO_CONTROL or a descriptor index in the associated
    // queue. A validated root protection remains until its reader ends access.
    root: AtomicUsize,
    // Safety invariant: NO_CONTROL or the context's uniquely claimed candidate;
    // it cannot be rewritten after root CAS transfers its tenure to the queue.
    candidate: AtomicUsize,
}

pub(crate) enum ReadyOffer {
    Inserted,
    Replaced(u32),
    Full,
}

pub(crate) struct ReadyQueue {
    // Safety invariant: Q-geometry: N=payload_slots>0, H=consumer_claimed.len(),
    // K=N+H, B=3K+2, D=depth>0. All sums/products fit usize; controls.len()=B,
    // order.len()=B*D, protection.len()=K. Arrays are fully initialized, static.
    controls: &'static [QueueControl],
    // Safety invariant: bank b occupies b*D..(b+1)*D. Only its exclusive
    // candidate owner writes; published/protected tenures are immutable.
    order: &'static [AtomicU32],
    // Safety invariant: contexts 0..N belong to unique producer slot owners (or
    // the queue after transfer); N..K belong to exclusively reserved consumers.
    protection: &'static [QueueProtection],
    // Safety invariant: true means exactly one unreleased consumer reservation.
    consumer_claimed: &'static [AtomicBool],
    // Safety invariant: Q-root: in 0..B, names the sole authoritative unread
    // descriptor. SeqCst CAS publishes initialized entries/meta/payload ownership.
    root: AtomicUsize,
    // Safety invariant: Q-geometry's D; each published len <= D.
    depth: usize,
    // Safety invariant: Q-geometry's N; every queued index is in 0..N.
    payload_slots: usize,
}

impl core::fmt::Debug for ReadyQueue {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ReadyQueue").field("depth", &self.depth)
            .field("control_descriptors", &self.controls.len()).finish_non_exhaustive()
    }
}

// A producer transfers this protection to its already-owned payload slot at
// the root CAS. A receiver/evictor clears it BEFORE returning that payload slot.
// Thus a publisher paused after commit does not retain an independent context.
struct QueueProtectionGuard<'a> {
    queue: &'a ReadyQueue,
    // Safety invariant: context is in queue.protection and uniquely owned by
    // this guard until it is dropped or forgotten at successful producer CAS.
    // Candidate, if any, belongs to this context; no guard survives transfer.
    context: usize,
    // Safety invariant: set immediately after this consumer's winning root CAS,
    // before any callback. A committed candidate belongs to the queue even if
    // promotion has not happened; cleanup must never abandon it as private.
    committed: bool,
}
impl Drop for QueueProtectionGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: QueueProtectionGuard.context gives an in-range, exclusively owned
        // context. protected/clear_protection therefore have their context contracts.
        // Its candidate is NO_CONTROL or in controls by QueueProtection.candidate;
        // only an uncommitted CANDIDATE may be freed. pop marks commitment before
        // any post-CAS hook. On hook unwind the queue keeps its committed root;
        // protect_root can help promotion before a later operation moves it.
        // offer forgets this guard before any post-CAS context access. The phase
        // CAS cannot free PUBLISHED/RECLAIMING tenures.
        unsafe {
            let candidate = self.queue.protected(self.context).candidate.load(Ordering::SeqCst);
            if candidate != NO_CONTROL && !self.committed {
                // Only an uncommitted private candidate may be abandoned. A winning
                // pop marks its descriptor committed before this guard drops.
                let _ = self.queue.control(candidate).phase.compare_exchange(
                    CONTROL_CANDIDATE, CONTROL_FREE, Ordering::SeqCst, Ordering::SeqCst);
            }
            self.queue.clear_protection(self.context);
        }
    }
}

impl ReadyQueue {
    /// # Safety-usable invariant
    /// Success establishes Q-geometry/Q-root and initialized atomic arrays;
    /// root 0 describes no payloads and all contexts are unreserved. No payload
    /// owner is fabricated. Overflow/zero geometry returns an error before use.
    pub fn new_in(memory: &StartupMemory, slots: u32, depth: u32, consumers: usize)
        -> Result<Self, StartupError>
    {
        let exhausted = || StartupError::MemoryExhausted { requested: usize::MAX, remaining: 0 };
        if depth == 0 || slots == 0 { return Err(exhausted()); }
        let payload_slots = slots as usize;
        let contexts = payload_slots.checked_add(consumers).ok_or_else(exhausted)?;
        let banks = contexts.checked_mul(3).and_then(|n| n.checked_add(2)).ok_or_else(exhausted)?;
        let entries = banks.checked_mul(depth as usize).ok_or_else(exhausted)?;
        let mut controls = memory.fixed_vec::<QueueControl>(banks)?;
        for index in 0..banks {
            controls.push(QueueControl {
                phase: AtomicU32::new(if index == 0 { CONTROL_PUBLISHED } else { CONTROL_FREE }),
                len: AtomicU32::new(0),
            })?;
        }
        let mut order = memory.fixed_vec::<AtomicU32>(entries)?;
        for _ in 0..entries { order.push(AtomicU32::new(0))?; }
        let mut protection = memory.fixed_vec::<QueueProtection>(contexts)?;
        for _ in 0..contexts {
            protection.push(QueueProtection { root: AtomicUsize::new(NO_CONTROL), candidate: AtomicUsize::new(NO_CONTROL) })?;
        }
        let mut consumer_claimed = memory.fixed_vec::<AtomicBool>(consumers)?;
        for _ in 0..consumers { consumer_claimed.push(AtomicBool::new(false))?; }
        Ok(Self { controls: controls.into_static_slice(), order: order.into_static_slice(),
            protection: protection.into_static_slice(), consumer_claimed: consumer_claimed.into_static_slice(),
            root: AtomicUsize::new(0), depth: depth as usize, payload_slots })
    }

    /// # Safety
    /// Caller must provide `index < self.controls.len()`. Returns that initialized
    /// atomic descriptor; this grants no payload or candidate ownership.
    unsafe fn control(&self, index: usize) -> &QueueControl {
        // SAFETY: index is in bounds by this function's explicit caller contract.
        unsafe { self.controls.get_unchecked(index) }
    }
    /// # Safety
    /// Caller must provide `context < self.protection.len()`. The returned
    /// atomic record alone grants no right to overwrite another context's state.
    unsafe fn protected(&self, context: usize) -> &QueueProtection {
        // SAFETY: context is in bounds by this function's explicit caller contract.
        unsafe { self.protection.get_unchecked(context) }
    }
    /// # Safety
    /// Caller must provide `bank < controls.len()` and `position < depth`.
    /// Read/write access must also respect QueueControl's tenure invariant.
    unsafe fn entry(&self, bank: usize, position: usize) -> &AtomicU32 {
        let offset = bank.wrapping_mul(self.depth).wrapping_add(position);
        // SAFETY: Q-geometry checked B*D; bank<B and position<D imply offset<B*D
        // without mathematical overflow. order contains exactly B*D atomics.
        unsafe { self.order.get_unchecked(offset) }
    }
    /// # Safety
    /// Caller must exclusively own this in-range context and have finished all
    /// accesses using its protected root/candidate. Clears both protections;
    /// after this call neither protected descriptor may be accessed via them.
    unsafe fn clear_protection(&self, context: usize) {
        // SAFETY: clear_protection's context contract gives protected's bound.
        let p = unsafe { self.protected(context) };
        p.root.store(NO_CONTROL, Ordering::SeqCst);
        p.candidate.store(NO_CONTROL, Ordering::SeqCst);
    }

    /// Called only AFTER the existing max_held counter admitted this receiver.
    /// At most max_held receivers/leases exist, so this scan always has a free
    /// credit for this admitted call. Failed strong CAS witnesses other work.
    /// # Safety-usable invariant
    /// On return, the result is in N..K and exclusively reserved by this call;
    /// no other reservation returns it until one release_consumer consumes it.
    /// This does not access payloads. Admission is required for termination,
    /// not soundness: without an available context this method keeps scanning.
    pub fn reserve_consumer(&self) -> usize {
        loop {
            for (index, claimed) in self.consumer_claimed.iter().enumerate() {
                if claimed.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                    return self.payload_slots.wrapping_add(index);
                }
            }
        }
    }
    /// # Safety
    /// Caller must own this queue's unique unreleased reserve_consumer result,
    /// have no active pop using it, and never use/release it after this call.
    /// All protected descriptor accesses must have ended. Consumes reservation.
    pub unsafe fn release_consumer(&self, context: usize) {
        // SAFETY: reserve_consumer's postcondition and this caller contract give
        // N<=context<K and exclusive ownership. clear_protection ends descriptor
        // protection before reuse; context-N<H proves get_unchecked's bound.
        unsafe {
            self.clear_protection(context);
            let index = context.wrapping_sub(self.payload_slots);
            // SAFETY: reserve_consumer produced context; its unique reservation
            // calls this exactly once, before returning the max_held credit.
            { self.consumer_claimed.get_unchecked(index) }.store(false, Ordering::SeqCst);
        }
    }

    /// # Safety
    /// Caller exclusively owns an in-range context with no concurrent operation
    /// on it. Returned descriptor is in controls and remains immutable while
    /// this context's root protection is kept; its initialized length is <=D.
    /// Caller must end reads before clearing/overwriting the protection.
    unsafe fn protect_root(&self, context: usize) -> usize {
        // SAFETY: context is exclusively held/in range by contract; root is in
        // controls by Q-root. The SeqCst protection store precedes root revalidation.
        // A reclaimer claims RECLAIMING before observing root then protections; a
        // validated reader's protection precedes removal and that scan. A delayed
        // reader either fails revalidation or observes a newly published tenure.
        // Thus the returned tenure is immutable and its len/entries are published.
        // Every control/protected call uses these bounds; no payload is accessed.
        unsafe {
            loop {
                let root = self.root.load(Ordering::SeqCst);
                #[cfg(test)]
                queue_schedule::fire(queue_schedule::Event::ReviewRootLoaded(context, root));
                self.protected(context).root.store(root, Ordering::SeqCst);
                if self.root.load(Ordering::SeqCst) == root {
                    // A root is immutable even if its publisher was suspended at
                    // the CAS. Help record its commitment before moving it again.
                    let _ = self.control(root).phase.compare_exchange(
                        CONTROL_CANDIDATE, CONTROL_PUBLISHED, Ordering::SeqCst, Ordering::SeqCst);
                    #[cfg(test)]
                    queue_schedule::fire(queue_schedule::Event::ProtectedRoot(root));
                    #[cfg(test)]
                    queue_schedule::fire(queue_schedule::Event::ReviewRootValidated(context, root, self.control(root).phase.load(Ordering::SeqCst)));
                    return root;
                }
                #[cfg(test)]
                queue_schedule::fire(queue_schedule::Event::ReviewRootRetried(context, root));
                self.protected(context).root.store(NO_CONTROL, Ordering::SeqCst);
            }
        }
    }

    /// # Safety
    /// Caller must pass index < controls.len(). May free only a committed tenure
    /// absent from root and all root/candidate protections after its exclusive
    /// RECLAIMING claim. Does not access payload storage or transfer its owners.
    unsafe fn reclaim(&self, index: usize) {
        // SAFETY: index is in bounds by contract. The successful phase CAS owns
        // this RECLAIMING tenure, preventing recycle/republish until the final store.
        // Only PUBLISHED is claimed; root is checked before the SeqCst protection scan.
        // protect_root's validated readers precede removal/scan, so an unreferenced
        // old tenure can become FREE; no payload ownership depends on old entries.
        unsafe {
            let control = self.control(index);
            if control.phase.compare_exchange(CONTROL_PUBLISHED, CONTROL_RECLAIMING,
                Ordering::SeqCst, Ordering::SeqCst).is_err() { return; }
            #[cfg(test)]
            queue_schedule::fire(queue_schedule::Event::ReclaimClaimed(index));
            // PUBLISHED means previously committed, never merely a candidate.
            // After claiming this tenure, establish that it has left root BEFORE
            // scanning protections. It cannot be republished or recycled while
            // RECLAIMING. Even a stale phase CAS must inspect its current tenure.
            let current = self.root.load(Ordering::SeqCst) == index;
            #[cfg(test)]
            queue_schedule::fire(queue_schedule::Event::ReclaimRootChecked(index, current));
            let referenced = current || self.protection.iter().any(|p|
                p.root.load(Ordering::SeqCst) == index || p.candidate.load(Ordering::SeqCst) == index);
            #[cfg(test)]
            queue_schedule::fire(queue_schedule::Event::ReviewReclaimDecision(index, current, referenced));
            control.phase.store(if referenced { CONTROL_PUBLISHED } else { CONTROL_FREE }, Ordering::SeqCst);
        }
    }

    /// # Safety
    /// Caller exclusively owns an in-range context. Its existing candidate, if
    /// any, is uncommitted and exclusively writable by that context. Returns an
    /// in-range CANDIDATE with the same exclusive right, until successful root
    /// CAS transfers it or guard cleanup abandons it. No payload ownership moves.
    unsafe fn candidate(&self, context: usize) -> usize {
        // SAFETY: protected uses the in-range context contract. Existing candidate
        // bounds follow QueueProtection.candidate; every reclaim/control index comes
        // from controls.enumerate(). FREE->PRIVATE CAS claims a descriptor uniquely;
        // PRIVATE prevents reclaim before protection, CANDIDATE prevents reclaim
        // until a root CAS commits it. The 3K+2 bank bound affects progress, not bounds.
        unsafe {
            let existing = self.protected(context).candidate.load(Ordering::SeqCst);
            if existing != NO_CONTROL { return existing; }
            loop {
                for (index, control) in self.controls.iter().enumerate() {
                    self.reclaim(index);
                    if control.phase.compare_exchange(CONTROL_FREE, CONTROL_PRIVATE,
                        Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                        #[cfg(test)]
                        queue_schedule::fire(queue_schedule::Event::ReviewCandidateClaimed(context, index));
                        // PRIVATE prevents reclamation before the protection is
                        // installed. CANDIDATE cannot be reclaimed or treated as
                        // committed; its owner may still publish it on any retry.
                        self.protected(context).candidate.store(index, Ordering::SeqCst);
                        control.phase.store(CONTROL_CANDIDATE, Ordering::SeqCst);
                        return index;
                    }
                }
                // There are 3K+2 banks: at most 2K protections, K simultaneous
                // reclaimers/private claims, and one current root. A quiescent
                // scan has an available/reclaimable bank. Retry requires progress,
                // not the resumption of a particular participant.
            }
        }
    }

    /// Incoming index is already exclusively owned and fully initialized.
    /// Insert/replacement membership and occupancy commit in the SAME CAS.
    /// # Safety
    /// Caller must exclusively own one live payload index < payload_slots for
    /// this queue, with initialized payload/metadata and no conflicting borrow.
    /// It must not already be queued or used by another offer. On Inserted or
    /// Replaced ownership of the incoming index transfers at root CAS: caller
    /// must never access/return it. Replaced transfers sole ownership of its
    /// victim to caller, whose cleanup must precede pool return. Full leaves
    /// incoming ownership with caller. No callback may observe partial payload.
    pub unsafe fn offer(&self, index: u32, replace_oldest: bool) -> ReadyOffer {
        // SAFETY: offer's caller contract makes the producer context exclusive and
        // in range. protect_root gives root bounds/immutability and len<=D. candidate
        // gives a distinct exclusive bank. Each entry read has position<len<=D;
        // replacement starts at1 only for len=D>0; destination=len-start<D. Copy
        // writes are within that same bound. The root CAS Release publishes all
        // payload/meta initialization; its Acquire carries old-root publication.
        // A successful CAS alone transfers membership and victim ownership. guard
        // is forgotten immediately, so the incoming context is never touched after
        // transfer. victim<N comes from the committed descriptor; clearing its
        // protection is exclusive only after successful removal. Failure retains
        // caller ownership and candidate rights for the next retry.
        unsafe {
            let context = index as usize;
            let guard = QueueProtectionGuard { queue: self, context, committed: false };
            #[cfg(test)]
            queue_schedule::fire(queue_schedule::Event::Counted(index));
            #[cfg(test)]
            queue_schedule::fire(queue_schedule::Event::TailSwapped(index));
            loop {
                let root = self.protect_root(context);
                let len = self.control(root).len.load(Ordering::SeqCst) as usize;
                if len == self.depth && !replace_oldest { return ReadyOffer::Full; }
                let replacing = len == self.depth;
                if replacing {
                    #[cfg(test)]
                    queue_schedule::fire(queue_schedule::Event::BeforeSurplus);
                }
                let next = self.candidate(context);
                let start = if replacing { 1 } else { 0 };
                let victim = if replacing { Some(self.entry(root, 0).load(Ordering::SeqCst)) } else { None };
                for position in start..len {
                    let value = self.entry(root, position).load(Ordering::SeqCst);
                    self.entry(next, position.wrapping_sub(start)).store(value, Ordering::SeqCst);
                }
                let destination = len.wrapping_sub(start);
                self.entry(next, destination).store(index, Ordering::SeqCst);
                self.control(next).len.store(destination.wrapping_add(1) as u32, Ordering::SeqCst);
                if self.root.compare_exchange(root, next, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                    // No access to this protection after transfer. A receiver may
                    // already consume/return/reuse index before we run again.
                    core::mem::forget(guard);
                    #[cfg(test)]
                    queue_schedule::fire(queue_schedule::Event::ReviewOfferTransferred(index, root, next, victim));
                    if let Some(victim) = victim { self.clear_protection(victim as usize); }
                    #[cfg(test)]
                    queue_schedule::fire(queue_schedule::Event::Linked(index));
                    return match victim { Some(victim) => ReadyOffer::Replaced(victim), None => ReadyOffer::Inserted };
                }
            }
        }
    }

    /// context belongs to a real max_held reservation for this entire call.
    /// # Safety
    /// Caller must hold this queue's unique unreleased consumer reservation for
    /// the complete call, without another operation on that context. Some(index)
    /// transfers sole ownership of one initialized published payload index<N;
    /// caller must clean it before one pool return. None transfers no payload.
    /// The consumer reservation remains caller-owned, with protections cleared.
    ///
    /// Test-only unwind postcondition: `ReviewPopCommitted` reports the winning
    /// removal before this call clears the removed index's producer protection.
    /// At that cut the removed index is initialized and uniquely withheld by this
    /// call; no payload borrow or owner has been returned. If the hook records the
    /// event and immediately unwinds without operating on the queue or payload,
    /// this call does not clean, return, or destroy that payload. Guard unwinding
    /// ends this call's descriptor accesses and clears its consumer protection;
    /// it neither releases the consumer reservation nor frees its committed bank.
    /// The committed bank remains eligible for normal promotion by helpers.
    ///
    /// After such an unwind, a test that still owns the consumer reservation and
    /// excludes concurrent/reentrant access may recover the recorded index once:
    /// first clear its retained producer protection, then resume sole ownership
    /// of the initialized payload and eventually perform exactly one pool return.
    /// No payload view or owner may have escaped the interrupted call or hook.
    /// This exceptional recovery contract does not cover arbitrary hook behavior,
    /// authorize recovery from an observation alone, or promise production panic
    /// handling or availability. Normal `Some`/`None` postconditions are unchanged.
    pub unsafe fn pop(&self, context: usize) -> Option<u32> {
        // SAFETY: context is uniquely reserved/in range by contract. protect_root
        // provides an immutable in-range bank with len<=D. len0 returns before any
        // entry access. Otherwise reads0..len and writes0..len-1 are within D;
        // candidate supplies exclusive in-range storage. Only successful root CAS
        // transfers index<N; Acquire observes payload/meta initialization. Recording
        // commitment before any post-CAS callback prevents guard abandonment even
        // if a test hook unwinds before phase promotion. The root remains helpable.
        // Removal uniquely grants clearing the producer protection. The guard then
        // ends descriptor reads before clearing consumer protection.
        unsafe {
            let mut guard = QueueProtectionGuard { queue: self, context, committed: false };
            loop {
                let root = self.protect_root(context);
                let len = self.control(root).len.load(Ordering::SeqCst) as usize;
                if len == 0 { return None; }
                #[cfg(test)]
                queue_schedule::fire(queue_schedule::Event::PopLocked);
                let next = self.candidate(context);
                let index = self.entry(root, 0).load(Ordering::SeqCst);
                for position in 1..len {
                    let value = self.entry(root, position).load(Ordering::SeqCst);
                    self.entry(next, position.wrapping_sub(1)).store(value, Ordering::SeqCst);
                }
                self.control(next).len.store(len.wrapping_sub(1) as u32, Ordering::SeqCst);
                if self.root.compare_exchange(root, next, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                    guard.committed = true;
                    #[cfg(test)]
                    queue_schedule::fire(queue_schedule::Event::ReviewPopCommitted(context, root, next, index, self.control(next).phase.load(Ordering::SeqCst)));
                    // Consumer context remains uniquely held until this call ends.
                    // Its candidate is now committed, even if root already moved.
                    let _ = self.control(next).phase.compare_exchange(
                        CONTROL_CANDIDATE, CONTROL_PUBLISHED, Ordering::SeqCst, Ordering::SeqCst);
                    // Only the winner owns index and its transferred protection.
                    self.clear_protection(index as usize);
                    return Some(index);
                }
            }
        }
    }

    #[cfg(test)]
    pub fn review_snapshot(&self) -> (usize, u32, Option<u32>) {
        // SAFETY: Q-root keeps root in controls; the atomic len is bounded by D
        // in every initialized/recycled tenure. An entry0 read occurs only for len>0
        // and D>0. This diagnostic never authorizes payload access or ownership.
        unsafe {
            // Only quiescent schedule cuts use this diagnostic; never admission.
            let root = self.root.load(Ordering::SeqCst);
            let len = self.control(root).len.load(Ordering::SeqCst);
            (root, len, if len == 0 { None } else { Some(self.entry(root, 0).load(Ordering::SeqCst)) })
        }
    }

    /// Diagnostic only; never an admission decision. Tests call it quiescently.
    #[cfg(test)]
    pub fn unread_for_test(&self) -> u32 {
        // SAFETY: Q-root keeps the loaded root within controls. Only an atomic
        // length is read; no payload ownership or synchronization claim follows.
        unsafe {
            self.control(self.root.load(Ordering::SeqCst)).len.load(Ordering::SeqCst)
        }
    }
}

/// `slots` byte slots of `slot_len` bytes, reserved in startup memory.
///
/// Storage is byte-initialized at creation, so no later write can read
/// uninitialized memory, and a slot is reachable only through the guard that
/// popped its index from the free list.
impl core::fmt::Debug for SlotPool {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SlotPool")
            .field("slots", &self.slots)
            .field("slot_len", &self.slot_len)
            .finish_non_exhaustive()
    }
}

#[doc(hidden)]
pub struct SlotPool {
    // Safety invariant: single permanent allocation of fully initialized cells;
    // byte contents stay initialized, and indexed regions are disjoint. Contents
    // may be accessed only under the corresponding unique/read-shared slot lease.
    storage: &'static [UnsafeCell<u8>],
    // Safety invariant: fixed region width; constructor checked total product.
    slot_len: usize,
    slots: usize,
    free: FreeIndices,
}

// SAFETY: Sync requires that shared access cannot create data races. Nonempty
// slot ranges are disjoint by the checked product and storage invariant. Safe
// acquire transfers FreeIndices::pop's unique index obligation to one SlotGuard;
// its borrowing methods exclude writes while shared byte borrows exist. The
// other byte-access paths, fixed_slot/fixed_slot_mut, are unsafe and require
// ownership and exclusion of conflicting access for the entire returned borrow.
// release/push/return_index require all accesses to end before the unique return,
// whose atomic publication precedes the next acquisition. Forget retains a lease
// and cannot authorize reuse. Empty ranges expose no shared mutable bytes. The
// remaining fields are immutable geometry and synchronized index controls; no
// thread-affinity or safe caller's semantic promise is required.
unsafe impl Sync for SlotPool {}

impl SlotPool {
    /// Reserves and zeroes `slots` slots of `slot_len` bytes.
    ///
    /// # Errors
    /// Refuses when memory is exhausted or the total overflows.
    /// # Safety-usable invariant
    /// Establishes the storage/geometry invariants with slots disjoint regions
    /// of initialized bytes, all indices free and no byte-access capability
    /// outstanding. No reference to a mutable byte escapes the constructor.
    pub fn new_in(
        memory: &StartupMemory,
        slots: usize,
        slot_len: usize,
    ) -> Result<Self, StartupError> {
        let total = slots
            .checked_mul(slot_len)
            .ok_or(StartupError::MemoryExhausted {
                requested: usize::MAX,
                remaining: 0,
            })?;
        let bytes = memory.bytes(total)?;
        for byte in bytes.iter_mut() {
            byte.write(0);
        }
        let free = FreeIndices::new_in(memory, slots)?;
        // SAFETY: each MaybeUninit byte now contains initialized0. MaybeUninit<u8>
        // and UnsafeCell<u8> have size/alignment1; Reference array/slice layout
        // gives identical stride/extent, and the unsized cast preserves count.
        // bytes is a unique static allocation from StartupMemory.bytes; its mutable
        // borrow is relinquished here and all future mutation uses UnsafeCell.
        let storage = unsafe { &*(core::ptr::from_mut(bytes) as *const [UnsafeCell<u8>]) };
        Ok(Self {
            storage,
            slot_len,
            slots,
            free,
        })
    }

    pub(crate) fn with_policy(mut self, policy: LifetimePolicy) -> Self { self.free = self.free.with_policy(policy); self }
    pub(crate) fn lifetime(&self) -> LifetimeState { self.free.lifetime() }

    /// Seeds control state only while the complete pool is exclusively in setup.
    /// # Safety
    /// Caller owns initialization rights for this pool and must exclude every
    /// acquisition, queue operation, observer and other reset for the call.
    /// No held/queued/retiring lease or payload reference may exist, including a
    /// forgotten lease or an outstanding raw-index return obligation. The pool
    /// has never entered runtime; reset is not a way to reclaim a live tenure.
    /// On success the selected FREE counter is initialized to `even`; ordinary
    /// acquisition may begin only after this call and setup publication complete.
    #[allow(unsafe_code)]
    #[cfg(any(test, up_rust_test_hooks))]
    pub(crate) unsafe fn seed_for_setup(&self, index: usize, even: u64) -> Result<(), super::pool::CapacityError> {
        // SAFETY: this function's caller supplies exclusive, never-runtime setup
        // rights for this same pool, excluding every live owner and observer.
        unsafe { self.free.seed_for_setup(index, even) }
    }
    /// Bytes in each slot.
    #[must_use]
    pub fn slot_len(&self) -> usize {
        self.slot_len
    }

    /// Number of slots.
    #[must_use]
    pub fn slots(&self) -> usize {
        self.slots
    }

    /// Whether a slot is free right now. For a capacity check, not a reservation.
    pub fn has_free(&'static self) -> bool {
        self.acquire().is_ok()
    }

    /// Claims a free slot, distinguishing exact fullness from permanent retirement.
    /// # Safety-usable invariant
    /// Success transfers FreeIndices::pop's unique live obligation to a single
    /// non-Clone SlotGuard with index < slots. Prior-owner accesses ended before
    /// the release observed by this acquisition. Error transfers no capability.
    pub fn acquire(&'static self) -> Result<SlotGuard, PoolError> {
        let index = self.free.pop()?;
        Ok(SlotGuard { pool: self, index })
    }

    /// # Safety-usable invariant
    /// Returns the checked single-allocation slot extent, or a valid empty slice.
    /// Every element is an initialized UnsafeCell<u8>; no byte-access permission
    /// is implied. The storage is permanent and its total extent fits isize.
    fn slot_range(&self, index: u32) -> &'static [UnsafeCell<u8>] {
        let start = (index as usize).saturating_mul(self.slot_len);
        let end = start.saturating_add(self.slot_len);
        self.storage.get(start..end).unwrap_or_default()
    }

    /// Reads one fixed slot, for tables that map index to slot one-to-one.
    ///
    /// # Safety
    /// Caller must retain this slot's exclusive owner or a read-only lease for
    /// the entire returned borrow. No writer, destructor or index return/reuse
    /// may overlap. Storage remains initialized; the result borrows self, not
    /// the process lifetime of the underlying region.
    #[must_use]
    pub unsafe fn fixed_slot(&self, index: u32) -> &[u8] {
        let range = self.slot_range(index);
        if range.is_empty() { return &[]; }
        // SAFETY: slot_range returns one initialized allocation subrange. raw_get
        // is the documented UnsafeCell contents projection; u8 has alignment1
        // and the live slice bounds give nonnull/isize fit/no address wrap.
        // fixed_slot's contract excludes mutation for the returned self borrow.
        unsafe { core::slice::from_raw_parts(UnsafeCell::raw_get(range.as_ptr()).cast_const(), range.len()) }
    }

    /// Writes one fixed slot.
    ///
    /// # Safety
    /// As [`Self::fixed_slot`], but caller must exclude all non-derived accesses
    /// (including existing shared references) for the entire mutable borrow.
    /// Do not return/transfer the slot while any derived reference survives.
    #[allow(clippy::mut_from_ref)]
    #[must_use]
    pub unsafe fn fixed_slot_mut(&self, index: u32) -> &mut [u8] {
        let range = self.slot_range(index);
        if range.is_empty() { return &mut []; }
        // SAFETY: slot_range returns one initialized allocation subrange. raw_get
        // supplies its UnsafeCell contents pointer; nonempty slice origin gives
        // nonnull/alignment1/isize fit/no wrap. fixed_slot_mut's caller retains
        // exclusive access to these bytes for the entire returned self borrow.
        unsafe { core::slice::from_raw_parts_mut(UnsafeCell::raw_get(range.as_ptr()), range.len()) }
    }

    /// Returns a slot its guard no longer owns. Private: only a dropping guard,
    /// which owned the index and is going away, may return one.
    /// # Safety
    /// Caller owns this pool's unique live index and has ended every byte borrow;
    /// consumes that ownership and must be the final access to this incarnation.
    unsafe fn release(&self, index: u32) {
        // SAFETY: release's contract supplies push's unique return and no-borrow
        // obligations. Bytes have no destructor; their initialized state persists.
        unsafe { self.free.push(index) };
    }
}

/// `slots` byte regions of `slot_len` bytes reached by index only, reserved in
/// startup memory and zeroed. The owner's free list decides who holds an index.
pub(crate) struct ByteSlots {
    // Safety invariant: single permanent allocation of fully initialized cells;
    // byte contents stay initialized, and indexed regions are disjoint. Contents
    // may be accessed only under the corresponding unique/read-shared slot lease.
    storage: &'static [UnsafeCell<u8>],
    // Safety invariant: fixed region width; constructor checked total product.
    slot_len: usize,
}

// SAFETY: a region is reachable only through the index held by its owner, so
// accesses never overlap; the shared parts are immutable.
unsafe impl Sync for ByteSlots {}
// SAFETY: as above; the regions are `'static` startup memory.
unsafe impl Send for ByteSlots {}

impl ByteSlots {
    pub(crate) fn new_in(
        memory: &StartupMemory,
        slots: usize,
        slot_len: usize,
    ) -> Result<Self, StartupError> {
        let total = slots
            .checked_mul(slot_len)
            .ok_or(StartupError::MemoryExhausted {
                requested: usize::MAX,
                remaining: 0,
            })?;
        let bytes = memory.bytes(total)?;
        for byte in bytes.iter_mut() {
            byte.write(0);
        }
        // SAFETY: initialized0 in each byte is valid for UnsafeCell<u8>.
        // MaybeUninit/u8/UnsafeCell element size/alignment1 plus Reference
        // array/slice layout prove the entire equal-length cast, not an arbitrary
        // wrapper-layout assumption. The unique StartupMemory.bytes allocation
        // is static; bytes is not used again after this shared-cell conversion.
        let storage = unsafe { &*(core::ptr::from_mut(bytes) as *const [UnsafeCell<u8>]) };
        Ok(Self { storage, slot_len })
    }

    pub(crate) fn slot_len(&self) -> usize {
        self.slot_len
    }

    /// # Safety-usable invariant
    /// Returns a checked single-allocation extent or a valid empty slice. Cells
    /// contain initialized u8 values, total extent fits isize, storage is static.
    /// The result does not grant mutable or immutable access to cell contents.
    fn range(&self, index: u32) -> &'static [UnsafeCell<u8>] {
        let start = (index as usize).saturating_mul(self.slot_len);
        let end = start.saturating_add(self.slot_len);
        self.storage.get(start..end).unwrap_or_default()
    }

    /// One region, for shared reading.
    ///
    /// # Safety
    /// Caller must retain the indexed region's owner/read-only lease and exclude
    /// mutation, destruction and reuse for the complete returned self borrow.
    pub(crate) unsafe fn slot(&self, index: u32) -> &[u8] {
        let range = self.range(index);
        if range.is_empty() { return &[]; }
        // SAFETY: range's postcondition gives initialized single-allocation
        // extent/isize fit and a nonnull aligned base. raw_get projects the cell
        // contents. slot's contract excludes writes for the returned borrow.
        unsafe { core::slice::from_raw_parts(UnsafeCell::raw_get(range.as_ptr()).cast_const(), range.len()) }
    }

    /// One region, for exclusive writing.
    ///
    /// # Safety
    /// Caller must exclusively own this region for the complete returned borrow;
    /// no non-derived access, shared reference, destruction or index reuse may
    /// overlap. Its initialized bytes must remain initialized on all exits.
    #[allow(clippy::mut_from_ref)]
    pub(crate) unsafe fn slot_mut(&self, index: u32) -> &mut [u8] {
        let range = self.range(index);
        if range.is_empty() { return &mut []; }
        // SAFETY: range's postcondition gives initialized single-allocation
        // extent/isize fit and nonnull alignment1. raw_get projects the cell
        // contents. slot_mut's contract excludes all non-derived access for the
        // returned borrow; no borrow survives the index's ownership transfer.
        unsafe { core::slice::from_raw_parts_mut(UnsafeCell::raw_get(range.as_ptr()), range.len()) }
    }
}

/// Exclusive use of one slot until it is dropped or adopted.
#[must_use]
#[doc(hidden)]
pub struct SlotGuard {
    pool: &'static SlotPool,
    // Safety invariant: one unreleased free.pop obligation of pool; index<slots.
    // No second guard or owner holds it, and all byte borrows derive from this
    // guard. Forget leaks the obligation in permanent storage; it never releases.
    index: u32,
}

impl core::fmt::Debug for SlotGuard {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SlotGuard")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

impl SlotGuard {
    /// The whole slot.
    /// # Safety-usable invariant
    /// Exclusively borrows this guard's initialized slot for the returned borrow;
    /// the guard cannot be used, dropped or transferred until that borrow ends.
    pub fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: SlotGuard's index invariant supplies this pool's unique live
        // index; &mut self excludes every existing byte borrow and other use of
        // this guard for the result's lifetime. The result is reborrowed to self,
        // not the pool's static lifetime, satisfying fixed_slot_mut's complete
        // no-access/no-return interval. Only initialized u8 can be safely written.
        unsafe { self.pool.fixed_slot_mut(self.index) }
    }

    /// The whole slot, read-only.
    #[must_use]
    /// # Safety-usable invariant
    /// Shares initialized bytes only for self's borrow; that borrow excludes
    /// bytes_mut, destruction and returning this guard's unique index.
    pub fn bytes(&self) -> &[u8] {
        // SAFETY: the unique index invariant prevents another owner from writing
        // or returning the slot. This &self borrow permits only other shared
        // byte views, excludes bytes_mut/Drop, and bounds the returned slice's
        // lifetime, satisfying fixed_slot's no-writer/no-return contract.
        unsafe { self.pool.fixed_slot(self.index) }
    }

    /// The slot's index.
    #[must_use]
    pub fn index(&self) -> u32 {
        self.index
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        // SAFETY: SlotGuard's index invariant owns the one return obligation;
        // &mut self at Drop excludes safe surviving byte borrows. No payload drop.
        unsafe { self.pool.release(self.index) };
    }
}

/// Temporarily treats initialized bytes as storage for initialization-preserving
/// crate-controlled writers. No unrestricted safe callback may receive the view.
///
/// # Safety
/// The caller must ensure every byte remains initialized on every exit, including
/// errors and unwinding, for the whole returned borrow. Only write initialized
/// bytes or leave existing bytes untouched; do not assign MaybeUninit::uninit(),
/// swap in uninitialized storage, or pass it to unconstrained safe code. The
/// original initialized-byte borrow remains suspended until this view ends.
/// The returned view has the identical extent/alignment/provenance and lifetime.
/// This conversion itself neither reads nor deinitializes any byte.
pub(crate) unsafe fn as_uninit_mut(bytes: &mut [u8]) -> &mut [MaybeUninit<u8>] {
    // SAFETY: MaybeUninit<u8> has u8 size/alignment; Reference array/slice layout
    // preserves element count and stride, and the raw slice cast keeps metadata.
    // The &mut input provides exclusive live storage and the result reborrows it.
    // The caller's all-exit contract prevents later uninitialized u8 reads.
    unsafe { &mut *(core::ptr::from_mut(bytes) as *mut [MaybeUninit<u8>]) }
}

// Scheduler code is test-only, including all heap-owning test infrastructure.
#[cfg(test)]
pub(crate) mod queue_schedule {
    extern crate std;
    use self::std::{boxed::Box, cell::RefCell};
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Event {
        Counted(u32), TailSwapped(u32), Linked(u32), PopLocked, BeforeSurplus, ProtectedRoot(usize), ReclaimClaimed(usize), ReclaimRootChecked(usize, bool),
        ReviewRootLoaded(usize, usize), ReviewRootValidated(usize, usize, u32),
        ReviewRootRetried(usize, usize), ReviewReclaimDecision(usize, bool, bool),
        ReviewCandidateClaimed(usize, usize),
        ReviewOfferTransferred(u32, usize, usize, Option<u32>),
        /// (consumer context, prior bank, committed bank, removed index, phase).
        /// Emitted after the winning pop CAS and guard commitment, before this
        /// call promotes the bank or clears producer protection. Other helpers
        /// may already have promoted the bank. The removed payload has not been
        /// returned or borrowed by this call. An observer owns no payload merely
        /// by seeing this event: exceptional one-time recovery requires all of
        /// `ReadyQueue::pop`'s test-only unwind postconditions and caller facts.
        ReviewPopCommitted(usize, usize, usize, u32, u32),
    }
    type Hook = Box<dyn FnMut(Event)>;
    std::thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }
    #[derive(Debug)]
    pub(crate) struct Guard;
    pub(crate) fn install(hook: Hook) -> Guard {
        HOOK.with(|cell| {
            let mut slot = cell.borrow_mut();
            assert!(slot.is_none(), "nested schedule installation");
            *slot = Some(hook);
        });
        Guard
    }
    pub(crate) fn fire(event: Event) {
        // Remove the hook before invoking it: a reentrant competing operation
        // executes production code without recursively firing the same hook.
        let hook = HOOK.with(|cell| cell.borrow_mut().take());
        if let Some(mut hook) = hook {
            hook(event);
            HOOK.with(|cell| *cell.borrow_mut() = Some(hook));
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) { HOOK.with(|cell| *cell.borrow_mut() = None); }
    }
}

// New source-only cases for GUR-1. They have not been compiled or executed.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod unsafe_projection_cases {
    use super::{ByteSlots, SlotPool};
    use crate::memory::StartupRegion;

    #[test]
    fn unsafe_review_slot_bytes_empty_nonempty_and_reuse() {
        static MEMORY: StartupRegion<32768> = StartupRegion::new();
        let memory = MEMORY.claim().unwrap();
        let pool: &'static SlotPool = memory.place(SlotPool::new_in(&memory, 2, 3).unwrap()).unwrap();
        let mut first = pool.acquire().unwrap();
        let mut second = pool.acquire().unwrap();
        first.bytes_mut().copy_from_slice(&[3, 5, 7]);
        second.bytes_mut().copy_from_slice(&[11, 13, 17]);
        assert_eq!(first.bytes(), &[3, 5, 7]);
        assert_eq!(second.bytes(), &[11, 13, 17]);
        let returned = first.index();
        drop(first);
        let mut reused = pool.acquire().unwrap();
        assert_eq!(reused.index(), returned);
        reused.bytes_mut().fill(19);
        assert_eq!(reused.bytes(), &[19, 19, 19]);
        assert_eq!(second.bytes(), &[11, 13, 17]);
        drop(reused);
        drop(second);
        let empty: &'static SlotPool = memory.place(SlotPool::new_in(&memory, 1, 0).unwrap()).unwrap();
        let mut slot = empty.acquire().unwrap();
        assert!(slot.bytes().is_empty());
        slot.bytes_mut().fill(23);
        assert!(slot.bytes().is_empty());
    }

    #[test]
    fn unsafe_review_byte_regions_disjoint_and_zero_length() {
        static MEMORY: StartupRegion<8192> = StartupRegion::new();
        let memory = MEMORY.claim().unwrap();
        let regions = ByteSlots::new_in(&memory, 2, 2).unwrap();
        // SAFETY: this local test owns all indices and exposes no concurrent
        // access. Each mutable borrow ends before another access; the regions
        // are disjoint initialized extents and no slot return occurs here.
        unsafe {
            regions.slot_mut(0).copy_from_slice(&[29, 31]);
            regions.slot_mut(1).copy_from_slice(&[37, 41]);
            assert_eq!(regions.slot(0), &[29, 31]);
            assert_eq!(regions.slot(1), &[37, 41]);
        }
        let empty = ByteSlots::new_in(&memory, 1, 0).unwrap();
        // SAFETY: the sole local index is exclusively test-owned. Both zero
        // length accesses still require valid slices; no other borrow overlaps.
        unsafe {
            empty.slot_mut(0).fill(43);
            assert!(empty.slot(0).is_empty());
        }
    }
}

// GCR-4 source-only regression; UNCOMPILED and UNRUN. Deliberate hook panic is
// test-only adversarial unwinding, never production panic-catching behavior.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod gcr_commit_unwind_regressions {
    extern crate std;
    use super::*;
    use crate::memory::StartupRegion;
    use self::std::{boxed::Box, cell::Cell, rc::Rc};

    #[test]
    #[allow(clippy::panic)] // This fixture deliberately tests the hook unwind cut.
    fn queue_pop_commit_hook_unwind_preserves_current_descriptor() {
        static REGION: StartupRegion<65536> = StartupRegion::new();
        let memory = REGION.claim().unwrap();
        let pool: &'static SlotPool = memory.place(SlotPool::new_in(&memory, 2, 1).unwrap()).unwrap();
        let queue = ReadyQueue::new_in(&memory, 2, 2, 1).unwrap();
        let mut a = pool.acquire().unwrap(); *a.bytes_mut().first_mut().unwrap() = 73; let ai = a.index();
        let mut b = pool.acquire().unwrap(); *b.bytes_mut().first_mut().unwrap() = 91; let bi = b.index();
        // Transfer the two initialized raw obligations to this queue; no byte
        // reference survives. Forget suppresses only the original guard returns.
        core::mem::forget(a); core::mem::forget(b);
        // SAFETY: these are distinct owned indices in this pool/queue geometry.
        // Each successful offer consumes the corresponding sole obligation.
        unsafe {
            assert!(matches!(queue.offer(ai, false), ReadyOffer::Inserted));
            assert!(matches!(queue.offer(bi, false), ReadyOffer::Inserted));
        }
        // The test is the only consumer; the one consumer credit is available.
        let context = queue.reserve_consumer();
        let committed = Rc::new(Cell::new(None));
        let hook_copy = committed.clone();
        let hook = queue_schedule::install(Box::new(move |event| {
            if let queue_schedule::Event::ReviewPopCommitted(c, _, next, index, phase) = event {
                assert_eq!(c, context); assert_eq!(index, ai);
                // Keep the original observation cut BEFORE phase promotion.
                assert_eq!(phase, CONTROL_CANDIDATE);
                hook_copy.set(Some(next));
                panic!("GCR deliberate post-commit hook unwind");
            }
        }));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // SAFETY: this test still uniquely owns this consumer reservation.
            unsafe { queue.pop(context) }
        }));
        drop(hook);
        assert!(result.is_err());
        let next = committed.get().unwrap();
        assert_eq!(queue.root.load(Ordering::SeqCst), next);
        // SAFETY: next is the root returned by this queue's committed hook and
        // lies in controls. No other thread operates on this local queue.
        assert_ne!(unsafe { queue.control(next) }.phase.load(Ordering::SeqCst), CONTROL_FREE);
        assert_eq!(queue.review_snapshot(), (next, 1, Some(bi)));
        // SAFETY: `ReadyQueue::pop`'s test-only unwind postcondition applies:
        // the hook recorded ai's winning removal and immediately panicked, with
        // no queue/payload operation or escaped view/owner. catch_unwind has
        // returned and the hook is removed. This local queue has no concurrent
        // user, and this test retains its unique consumer reservation. Thus the
        // guard ended descriptor access and cleared consumer protection without
        // returning ai or releasing the reservation. Reusing the context below
        // is valid; Some(bi) transfers bi's distinct initialized obligation once.
        // The same documented postcondition grants one-time clearing of ai's
        // retained producer protection and recovery of its sole initialized
        // obligation. Both old byte borrows ended before producer transfer.
        unsafe {
            assert_eq!(queue.pop(context), Some(bi));
            assert_eq!(queue.pop(context), None);
            queue.clear_protection(ai as usize);
            queue.release_consumer(context);
        }
        // Ownership reconstruction: pop's documented unwind recovery gives ai;
        // its normal Some postcondition gives bi. Their protections are cleared,
        // no other guard owns either slot, and each guard below returns it once.
        let a = SlotGuard { pool, index: ai };
        let b = SlotGuard { pool, index: bi };
        assert_eq!(a.bytes(), &[73]); assert_eq!(b.bytes(), &[91]);
        drop(a); drop(b);
        let a = pool.acquire().unwrap(); let b = pool.acquire().unwrap();
        assert_ne!(a.index(), b.index()); drop(a); drop(b);
    }
}
