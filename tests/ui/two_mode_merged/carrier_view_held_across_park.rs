//! `park` consumes the frame, so a byte view borrowed from it cannot be held across it.
use up_rust::two_mode::*;

fn view_then_park(runtime: TransportRuntime, route: IngressRoute) {
    let _ = runtime.receive(route, &mut |frame: CarriedFrame<'_>| {
        let view: &[u8] = frame.metadata();
        let kept = frame.park();
        drop(kept);
        let _ = view[0];
        Flow::Continue
    });
}

fn main() {}
