use std::{mem::MaybeUninit, ptr::NonNull};

use up_rust::LoanedUninitPayload;

#[repr(C)]
struct ManualPayload {
    value: u32,
}

fn main() {
    let mut storage = MaybeUninit::<ManualPayload>::uninit();
    let ptr = NonNull::from(&mut storage);
    let mut slot = unsafe { LoanedUninitPayload::new_unchecked(ptr) };

    let _raw = unsafe { LoanedUninitPayload::as_uninit_bytes_mut(&mut slot) };
}
