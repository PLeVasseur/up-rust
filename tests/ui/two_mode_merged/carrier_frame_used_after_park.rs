//! `park` consumes the frame, so it cannot be used afterwards (views: `carrier_view_*`).
use up_rust::two_mode::*;

fn race(runtime: TransportRuntime, route: IngressRoute, egress: EgressRoute) {
    let _ = runtime.receive(route, &mut |frame: CarriedFrame<'_>| {
        let kept = frame.park();
        drop(kept);
        let _ = runtime.send(egress, &frame);
        Flow::Continue
    });
}

fn main() {}
