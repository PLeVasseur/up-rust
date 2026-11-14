//! Traffic is impossible until runtime mode: a listener setup cannot receive.
use up_rust::two_mode::*;

fn listen(setup: NotificationListenerSetup<u32>) {
    let _ = setup.try_recv();
}

fn main() {}
