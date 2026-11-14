//! The program that freed a frame under a live view: the fields are private now.
use up_rust::two_mode::*;

fn view_then_park(runtime: TransportRuntime, route: IngressRoute) {
    let _ = runtime.receive(route, &mut |frame| {
        let view: &[u8] = frame.metadata;
        let parked = frame.park().expect("park");
        drop(parked);
        let _ = view[0];
        Flow::Continue
    });
}

fn main() {}
