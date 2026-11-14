//! A `Ready` can only come from a `StartupSlot`.
use up_rust::two_mode::Ready;

fn main() {
    let mut value = 1_u32;
    let _ = Ready { value: &mut value, _lifetime: std::marker::PhantomData };
}

// [utest->req~ssr-unsafe-wire-untrusted~1] method: compile-fail
