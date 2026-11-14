//! A carried frame is borrowed for the sink call; it cannot be kept past it.
use up_rust::two_mode::*;

fn keep(runtime: TransportRuntime, route: IngressRoute) {
    let mut kept = None;
    let _ = runtime.receive(route, &mut |frame: CarriedFrame<'_>| {
        kept = Some(frame);
        Flow::Continue
    });
    let _ = kept;
}

fn main() {}
