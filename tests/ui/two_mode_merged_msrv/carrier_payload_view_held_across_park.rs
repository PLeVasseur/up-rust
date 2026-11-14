//! A payload view borrows the frame too, so it cannot be held across `park`.
use up_rust::two_mode::*;

fn view_then_park(runtime: TransportRuntime, route: IngressRoute) {
    let _ = runtime.receive(route, &mut |frame: CarriedFrame<'_>| {
        let view = frame.payload();
        let kept = frame.park();
        drop(kept);
        let _ = view.as_contiguous();
        Flow::Continue
    });
}

fn main() {}
