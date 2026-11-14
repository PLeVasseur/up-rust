//! Traffic is impossible until runtime mode: a notifier setup cannot notify.
use up_rust::two_mode::*;

fn notify(setup: NotifierSetup<u32>) {
    let _ = setup.notify(&1);
}

fn main() {}
