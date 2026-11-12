//! Bounded correlation storage shared by serialized and native RPC engines.
//! Storage contains retained frame handles, never an enum of application payloads.

use crate::UUID;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, MutexGuard,
    },
    task::{Context, Poll, Waker},
};

fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Why a correlation slot could not be reserved. Reservation has no other effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ReserveError {
    /// Every reserved slot is occupied.
    #[error("every correlation slot is occupied")]
    Full,
    /// The table has been closed.
    #[error("correlation table closed")]
    Closed,
}

/// The table closed before a response arrived.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("correlation table closed")]
pub struct PendingClosed;

enum Entry<F> {
    Free,
    Waiting {
        key: (u64, u64),
        waker: Option<Waker>,
    },
    Done(F),
    Closed,
    Consumed,
}

/// Bounded table correlating in-flight request IDs with their responses.
///
/// `F` is a retained frame handle, never a decoded payload. The table matches on
/// the request ID alone, so a caller must check the frame's exact source, sink,
/// kind and payload identity before [`deliver`](Self::deliver), and must use IDs
/// unique among in-flight calls, as [`UUID::build`] provides.
pub struct PendingCalls<F> {
    entries: Box<[Mutex<Entry<F>>]>,
    closed: AtomicBool,
}

impl<F> PendingCalls<F> {
    /// Reserves all storage up front; calls allocate nothing further.
    ///
    /// # Errors
    /// Returns the allocation failure, without constructing a partial table.
    pub fn new(capacity: usize) -> Result<Self, std::collections::TryReserveError> {
        let mut entries = Vec::new();
        entries.try_reserve_exact(capacity)?;
        entries.resize_with(capacity, || Mutex::new(Entry::Free));
        Ok(Self {
            entries: entries.into_boxed_slice(),
            closed: AtomicBool::new(false),
        })
    }

    /// Claims a slot for `id`. Call this before sending the request.
    ///
    /// # Errors
    /// Reports a full or closed table; the table is unchanged.
    pub fn reserve(&self, id: &UUID) -> Result<PendingGuard<'_, F>, ReserveError> {
        for (index, entry) in self.entries.iter().enumerate() {
            let mut state = lock(entry);
            if self.closed.load(Ordering::Acquire) {
                return Err(ReserveError::Closed);
            }
            if matches!(*state, Entry::Free) {
                *state = Entry::Waiting {
                    key: id.as_u64_pair(),
                    waker: None,
                };
                return Ok(PendingGuard { table: self, index });
            }
        }
        Err(if self.closed.load(Ordering::Acquire) {
            ReserveError::Closed
        } else {
            ReserveError::Full
        })
    }

    /// Completes the matching waiter. Returns false when no reserved slot matches,
    /// so an unknown, cancelled, duplicate or late response is rejected here.
    pub fn deliver(&self, id: &UUID, frame: F) -> bool {
        if self.closed.load(Ordering::Acquire) {
            return false;
        }
        for entry in &self.entries {
            let mut state = lock(entry);
            if let Entry::Waiting { key, waker } = &mut *state {
                if *key == id.as_u64_pair() {
                    let wake = waker.take();
                    *state = Entry::Done(frame);
                    drop(state);
                    if let Some(waker) = wake {
                        waker.wake();
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Counts occupied slots, including completed waits not yet consumed.
    pub fn in_flight(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| !matches!(*lock(entry), Entry::Free))
            .count()
    }

    /// Whether [`close`](Self::close) has run.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Rejects new reservations and wakes every waiter with [`PendingClosed`].
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        for entry in &self.entries {
            let mut state = lock(entry);
            if let Entry::Waiting { waker, .. } = &mut *state {
                let wake = waker.take();
                *state = Entry::Closed;
                drop(state);
                if let Some(waker) = wake {
                    waker.wake();
                }
            }
        }
    }
}

/// Dropping a wait (including before its first poll) releases its exact slot.
#[must_use = "dropping the guard cancels the reservation"]
pub struct PendingGuard<'a, F> {
    table: &'a PendingCalls<F>,
    index: usize,
}
impl<F> PendingGuard<'_, F> {
    /// Waits for the correlated frame, consuming the guard so a completed wait
    /// cannot be polled again. Dropping this future cancels the reservation.
    ///
    /// # Errors
    /// Reports table closure before a response arrived.
    pub async fn wait(self) -> Result<F, PendingClosed> {
        std::future::poll_fn(|context| self.poll(context))
            .await
            .map_err(|()| PendingClosed)
    }

    pub(crate) fn poll(&self, context: &mut Context<'_>) -> Poll<Result<F, ()>> {
        let mut state = lock(
            self.table
                .entries
                .get(self.index)
                .expect("reserved pending entry"),
        );
        match &mut *state {
            Entry::Waiting { waker, .. } => {
                if !waker.as_ref().is_some_and(|w| w.will_wake(context.waker())) {
                    *waker = Some(context.waker().clone());
                }
                Poll::Pending
            }
            Entry::Done(_) => {
                let Entry::Done(frame) = std::mem::replace(&mut *state, Entry::Consumed) else {
                    unreachable!()
                };
                Poll::Ready(Ok(frame))
            }
            Entry::Closed => Poll::Ready(Err(())),
            Entry::Consumed | Entry::Free => panic!("completed RPC wait polled again"),
        }
    }
}
impl<F> std::fmt::Debug for PendingCalls<F> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PendingCalls")
            .field("capacity", &self.entries.len())
            .finish_non_exhaustive()
    }
}
impl<F> std::fmt::Debug for PendingGuard<'_, F> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PendingGuard")
            .finish_non_exhaustive()
    }
}
impl<F> Drop for PendingGuard<'_, F> {
    fn drop(&mut self) {
        let previous = std::mem::replace(
            &mut *lock(
                self.table
                    .entries
                    .get(self.index)
                    .expect("reserved pending entry"),
            ),
            Entry::Free,
        );
        drop(previous);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn reversed_completion_cancellation_late_reply_and_capacity() {
        let table = PendingCalls::new(2).unwrap();
        let a = UUID::build();
        let b = UUID::build();
        let ga = table.reserve(&a).ok().unwrap();
        let gb = table.reserve(&b).ok().unwrap();
        assert!(matches!(
            table.reserve(&UUID::build()),
            Err(ReserveError::Full)
        ));
        assert!(table.deliver(&b, 22));
        assert!(table.deliver(&a, 11));
        assert_eq!(std::future::poll_fn(|cx| ga.poll(cx)).await, Ok(11));
        assert_eq!(std::future::poll_fn(|cx| gb.poll(cx)).await, Ok(22));
        drop(ga);
        drop(gb);
        let cancelled = table.reserve(&a).ok().unwrap();
        drop(cancelled);
        assert!(!table.deliver(&a, 99));
        assert_eq!(table.in_flight(), 0);
        let guard = table.reserve(&b).ok().unwrap();
        table.close();
        assert_eq!(std::future::poll_fn(|cx| guard.poll(cx)).await, Err(()));
        drop(guard);
        assert!(matches!(table.reserve(&a), Err(ReserveError::Closed)));
    }

    #[tokio::test]
    async fn public_wait_is_consuming_and_reports_closure() {
        let table = PendingCalls::new(2).unwrap();
        let (a, b, c) = (UUID::build(), UUID::build(), UUID::build());
        let guard = table.reserve(&a).unwrap();
        assert!(table.deliver(&a, 7));
        assert_eq!(guard.wait().await, Ok(7));
        assert_eq!(table.in_flight(), 0);
        // A dropped wait frees its slot, so the late reply is refused.
        drop(table.reserve(&b).unwrap());
        assert!(!table.deliver(&b, 8));
        let guard = table.reserve(&c).unwrap();
        table.close();
        assert!(table.is_closed());
        assert_eq!(guard.wait().await, Err(PendingClosed));
        assert_eq!(table.reserve(&a).unwrap_err(), ReserveError::Closed);
        assert_eq!(
            ReserveError::Full.to_string(),
            "every correlation slot is occupied"
        );
    }
}
