//! A wire cannot hold on to its startup slot past `init_slot`.
use up_rust::two_mode::*;

struct Keep;

impl Wire<u32> for Keep {
    fn describe(&self) -> WireInfo {
        unimplemented!()
    }
    fn encode(&self, _value: &u32, _out: &mut PayloadWriter<'_>) -> Result<(), WireError> {
        Ok(())
    }
    fn init_slot<'s>(&self, slot: StartupSlot<'s, u32>) -> Ready<'s, u32> {
        let ready = slot.write(1);
        let _kept: Ready<'static, u32> = ready;
        loop {}
    }
    fn decode(
        &self,
        _encoding: up_rust::PayloadEncoding,
        _bytes: &[u8],
        _into: &mut u32,
    ) -> Result<(), WireError> {
        Ok(())
    }
}

fn main() {}

// [utest->req~ssr-unsafe-wire-untrusted~1] method: compile-fail
