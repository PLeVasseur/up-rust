//! Bounded, inline, lifetime-preserving future erasure for serialized sends.
//!
//! This stores future state, not payload storage. It neither allocates nor falls
//! back to a box. `new` checks layout at monomorphization: an unsupported future
//! is a build error, never a refusal after a send has reached the transport.
//! Code-generation/build gates, not only `cargo check`, must exercise admission.
//! State is moved before pinning and never moved after polling begins.

use std::{
    cell::{Cell, UnsafeCell},
    future::Future,
    marker::{PhantomData, PhantomPinned},
    mem::{align_of, size_of, MaybeUninit},
    pin::Pin,
    task::{Context, Poll},
};

/// Bytes reserved per public serialized send, excluding small dispatch fields.
pub const SEND_FUTURE_BYTES: usize = 4096;
/// Bytes reserved per selected-wire core send.
pub const CORE_FUTURE_BYTES: usize = 2048;
/// Maximum supported future alignment in bytes.
pub const FUTURE_ALIGNMENT: usize = 16;

#[repr(C, align(16))]
struct Storage<const BYTES: usize>(UnsafeCell<MaybeUninit<[u8; BYTES]>>);

/// A bounded, `Send`, deliberately non-`Sync` and non-`Unpin` erased future.
///
/// `F: Send + 'a` is required only at construction. The stored future need not
/// be `Sync`, `Unpin`, or `'static`. No reference is extended to `'static` and
/// no pointer into the storage is retained across a move before pinning.
/// Completion disables repolling but retains the completed future until drop.
/// A panic while polling likewise disables repolling; unwinding drops it once.
///
/// ```compile_fail
/// use up_rust::inline_dispatch::InlineFuture;
/// let future;
/// {
///     let local = String::from("borrowed");
///     future = InlineFuture::<(), 128>::new(async { std::hint::black_box(&local); });
/// }
/// drop(future);
/// ```
///
/// ```compile_fail
/// use up_rust::inline_dispatch::InlineFuture;
/// let state = std::rc::Rc::new(1);
/// let _future = InlineFuture::<(), 128>::new(async move { drop(state); });
/// ```
///
/// ```compile_fail
/// use up_rust::inline_dispatch::InlineFuture;
/// fn requires_unpin<T: Unpin>(_: T) {}
/// requires_unpin(InlineFuture::<(), 128>::new(async {}));
/// ```
pub struct InlineFuture<'a, Output, const BYTES: usize> {
    storage: Storage<BYTES>,
    poll: unsafe fn(*mut u8, &mut Context<'_>) -> Poll<Output>,
    drop: unsafe fn(*mut u8),
    pollable: bool,
    lifetime: PhantomData<&'a mut ()>,
    not_sync: PhantomData<Cell<()>>,
    _pinned: PhantomPinned,
}

impl<Output, const BYTES: usize> std::fmt::Debug for InlineFuture<'_, Output, BYTES> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InlineFuture")
            .field("capacity", &BYTES)
            .field("pollable", &self.pollable)
            .finish_non_exhaustive()
    }
}

impl<'a, Output, const BYTES: usize> InlineFuture<'a, Output, BYTES> {
    /// Erases a `Send` future in place, with build-time layout admission.
    ///
    /// The compile-time assertions reject oversize and over-aligned futures.
    /// There is no heap fallback. Increasing capacity is an explicit API change.
    pub fn new<F>(future: F) -> Self
    where
        F: Future<Output = Output> + Send + 'a,
    {
        const {
            assert!(
                size_of::<F>() <= BYTES,
                "future exceeds inline dispatch capacity"
            );
            assert!(
                align_of::<F>() <= align_of::<Storage<BYTES>>(),
                "future exceeds inline dispatch alignment"
            );
        }
        unsafe fn poll<F: Future>(pointer: *mut u8, context: &mut Context<'_>) -> Poll<F::Output> {
            // SAFETY: constructor installs this function only with an initialized F;
            // the enclosing !Unpin owner is pinned throughout every call and drop.
            unsafe { Pin::new_unchecked(&mut *pointer.cast::<F>()).poll(context) }
        }
        unsafe fn drop_future<F>(pointer: *mut u8) {
            // SAFETY: storage still contains the unique initialized F. Drop is
            // called exactly once by its owner's destructor, including on unwind.
            unsafe { pointer.cast::<F>().drop_in_place() };
        }
        let mut erased = Self {
            storage: Storage(UnsafeCell::new(MaybeUninit::uninit())),
            poll: poll::<F>,
            drop: drop_future::<F>,
            pollable: true,
            lifetime: PhantomData,
            not_sync: PhantomData,
            _pinned: PhantomPinned,
        };
        // SAFETY: compile-time size/alignment admission proves storage fits F.
        // Storage is uninitialized; writing initializes it exactly once before
        // escape. repr(C)'s first field and its transparent wrappers share offset
        // zero. UnsafeCell prevents shared owner borrows freezing hidden state.
        // No pin or address-dependent projection exists at this point.
        unsafe {
            UnsafeCell::raw_get(std::ptr::addr_of_mut!(erased.storage.0))
                .cast::<F>()
                .write(future)
        };
        erased
    }
}

impl<Output, const BYTES: usize> Future for InlineFuture<'_, Output, BYTES> {
    type Output = Output;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Output> {
        // SAFETY: this projection never moves storage, replaces F, or returns an
        // unpinned reference to F. Only control fields and the pinned F are used.
        let this = unsafe { self.get_unchecked_mut() };
        assert!(
            this.pollable,
            "inline future polled after completion or panic"
        );
        this.pollable = false;
        // SAFETY: F remains initialized at its pinned address, with the matching
        // constructor-installed vtable and exclusive access through Pin<&mut>.
        // Raw projection avoids reborrowing the Unpin byte-storage field, which
        // would invalidate references held inside a self-referential future.
        let result = unsafe {
            (this.poll)(
                UnsafeCell::raw_get(std::ptr::addr_of_mut!(this.storage.0)).cast(),
                context,
            )
        };
        if result.is_pending() {
            this.pollable = true;
        }
        result
    }
}

impl<Output, const BYTES: usize> Drop for InlineFuture<'_, Output, BYTES> {
    fn drop(&mut self) {
        // SAFETY: F was initialized exactly once; polling never removes it.
        // Drop preserves its address if pinned and cannot run twice for this owner.
        // As in poll, no reference to the Unpin backing storage is constructed.
        unsafe { (self.drop)(UnsafeCell::raw_get(std::ptr::addr_of_mut!(self.storage.0)).cast()) };
    }
}

/// Public dyn-dispatch send future; no transport/codec type escapes into handles.
pub type SendFuture<'a> = InlineFuture<'a, Result<(), crate::UStatus>, SEND_FUTURE_BYTES>;
/// Inner core future, deliberately smaller so selected-wire composition fits.
pub type CoreSendFuture<'a> = InlineFuture<'a, Result<(), crate::UStatus>, CORE_FUTURE_BYTES>;
