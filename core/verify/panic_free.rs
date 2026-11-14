//! Link-time no-panic and no-heap proof for `up-rust-core`.
//!
//! A `no_std` cdylib whose panic handler references an undefined symbol: if any
//! panic path survives optimization anywhere in the reachable code, including
//! inside `core` calls, the link fails. Inputs come from the caller as raw
//! pointers and integers so the optimizer cannot fold the runtime paths away.
//! `--cfg inject_panic` adds one reachable panic, as the negative control.

#![no_std]
#![allow(clippy::all)]

use core::fmt::Write;
use core::mem::MaybeUninit;
use core::panic::PanicInfo;
use core::str::FromStr;

use up_rust_core::frame::header::{parse_header, write_header_uninit, HeaderLimits, TxHeaderParts};
use up_rust_core::frame::native::NativeTypeToken;
use up_rust_core::memory::StartupRegion;
use up_rust_core::platform::Native;
use up_rust_core::signal::{Signal, SignalWait};
use up_rust_core::uattributes::{PayloadEncoding, UMessageType, UPriority};
use up_rust_core::ucode::UCode;
use up_rust_core::uri::UUri;
use up_rust_core::uuid::UUID;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    extern "C" {
        fn __up_rust_core_panic_path_survived() -> !;
    }
    // SAFETY: never called when the link succeeds; a surviving path fails the link.
    unsafe { __up_rust_core_panic_path_survived() }
}

fn text<'a>(ptr: *const u8, len: usize) -> Result<&'a str, ()> {
    // SAFETY: the caller promises a valid byte range for the fixture's duration.
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    core::str::from_utf8(bytes).map_err(|_| ())
}

/// Parses a URI and returns its resource ID, or a negative code.
///
/// # Safety
/// `ptr` and `len` must describe readable bytes.
#[no_mangle]
pub unsafe extern "C" fn fixture_parse_uri(ptr: *const u8, len: usize) -> i64 {
    let Ok(text) = text(ptr, len) else {
        return -1;
    };
    match UUri::from_str(text) {
        Ok(uri) => i64::from(uri.resource_id()),
        Err(_) => -2,
    }
}

/// Runs the text lookups of the identifier types.
///
/// # Safety
/// `ptr` and `len` must describe readable bytes.
#[no_mangle]
pub unsafe extern "C" fn fixture_text_codes(ptr: *const u8, len: usize) -> i64 {
    let Ok(text) = text(ptr, len) else {
        return -1;
    };
    let mut seen = 0_i64;
    if PayloadEncoding::from_media_type_str(text).is_ok() {
        seen |= 1;
    }
    if UPriority::from_priority_code(text).is_some() {
        seen |= 2;
    }
    if UMessageType::from_cloudevent_type(text).is_some() {
        seen |= 4;
    }
    seen
}

/// Runs the numeric lookups of the identifier types.
#[no_mangle]
pub extern "C" fn fixture_wire_codes(value: u32) -> i64 {
    let mut seen = 0_i64;
    if PayloadEncoding::try_from_wire_id(value).is_ok() {
        seen |= 1;
    }
    if UPriority::from_wire_code(value as u8).is_some() {
        seen |= 2;
    }
    if UMessageType::from_wire_code(value as u8).is_some() {
        seen |= 4;
    }
    if UCode::from_i32(value as i32).is_some() {
        seen |= 8;
    }
    seen
}

/// Checks a URI authority.
///
/// # Safety
/// `ptr` and `len` must describe readable bytes.
#[no_mangle]
pub unsafe extern "C" fn fixture_wire_parts(
    ptr: *const u8,
    len: usize,
    entity: u32,
    version: u8,
    resource: u16,
) -> i64 {
    let Ok(authority) = text(ptr, len) else {
        return -1;
    };
    match UUri::try_from_wire_parts(authority, entity, version, resource) {
        Ok(uri) => i64::from(uri.ue_type_id()),
        Err(_) => -2,
    }
}

struct FixedBuffer<'a> {
    bytes: &'a mut [u8],
    used: usize,
}

impl Write for FixedBuffer<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            let slot = self.bytes.get_mut(self.used).ok_or(core::fmt::Error)?;
            *slot = byte;
            self.used = self.used.wrapping_add(1);
        }
        Ok(())
    }
}

/// Formats a URI into a caller buffer.
///
/// # Safety
/// The pointers must describe readable and writable bytes.
#[no_mangle]
pub unsafe extern "C" fn fixture_format_uri(
    ptr: *const u8,
    len: usize,
    out: *mut u8,
    out_len: usize,
) -> i64 {
    let Ok(text) = text(ptr, len) else {
        return -1;
    };
    let Ok(uri) = UUri::from_str(text) else {
        return -2;
    };
    // SAFETY: the caller promises a writable range.
    let bytes = unsafe { core::slice::from_raw_parts_mut(out, out_len) };
    let mut buffer = FixedBuffer { bytes, used: 0 };
    match write!(buffer, "{}", uri.display_with_scheme(false)) {
        Ok(()) => buffer.used as i64,
        Err(_) => -3,
    }
}

/// Parses and reformats a UUID.
///
/// # Safety
/// The pointers must describe readable and writable bytes.
#[no_mangle]
pub unsafe extern "C" fn fixture_uuid(ptr: *const u8, len: usize, out: *mut u8) -> i64 {
    let Ok(text) = text(ptr, len) else {
        return -1;
    };
    match UUID::from_str(text) {
        Ok(uuid) => {
            let formatted = uuid.hyphenated_ascii();
            // SAFETY: the caller promises 36 writable bytes.
            unsafe { core::ptr::copy_nonoverlapping(formatted.as_ptr(), out, 36) };
            uuid.time() as i64
        }
        Err(_) => -2,
    }
}

/// Parses a native header and reports its token.
///
/// # Safety
/// The pointers must describe readable bytes.
#[no_mangle]
pub unsafe extern "C" fn fixture_parse_header(
    prefix: *const u8,
    prefix_len: usize,
    bytes: *const u8,
    len: usize,
) -> i64 {
    // SAFETY: the caller promises valid ranges.
    let prefix = unsafe { core::slice::from_raw_parts(prefix, prefix_len) };
    // SAFETY: as above.
    let bytes = unsafe { core::slice::from_raw_parts(bytes, len) };
    let limits = HeaderLimits { max_token_len: 64 };
    match parse_header(prefix, bytes, &limits) {
        Ok(header) => header
            .native_type_token
            .map_or(0, |token| i64::from(token.as_u32())),
        Err(_) => -1,
    }
}

/// Writes a native header into uninitialized caller storage.
///
/// # Safety
/// The pointers must describe valid ranges.
#[no_mangle]
pub unsafe extern "C" fn fixture_write_header(
    uri_ptr: *const u8,
    uri_len: usize,
    id_msb: u64,
    id_lsb: u64,
    token: u32,
    out: *mut MaybeUninit<u8>,
    out_len: usize,
) -> i64 {
    let Ok(text) = text(uri_ptr, uri_len) else {
        return -1;
    };
    let Ok(source) = UUri::from_str(text) else {
        return -2;
    };
    let Ok(id) = UUID::try_from_wire_pair(id_msb, id_lsb) else {
        return -3;
    };
    let parts = TxHeaderParts {
        kind: UMessageType::Publish,
        priority: Some(UPriority::CS1),
        id: &id,
        reqid: None,
        ttl: None,
        comm_status: None,
        permission_level: None,
        source: &source,
        sink: None,
        token: None,
        traceparent: None,
        payload_encoding: Some(PayloadEncoding::RAW),
        native_type_token: Some(NativeTypeToken::from_u32(token)),
    };
    // SAFETY: the caller promises a writable range.
    let out = unsafe { core::slice::from_raw_parts_mut(out, out_len) };
    match write_header_uninit(b"prefix", &parts, out) {
        Ok(written) => written.len as i64,
        Err(_) => -4,
    }
}

static REGION: StartupRegion<8192> = StartupRegion::new();

/// Places values into startup memory.
///
/// # Safety
/// Must be called at most once with `size` describing a plausible request.
#[no_mangle]
pub unsafe extern "C" fn fixture_memory(size: usize, value: u32) -> i64 {
    let Ok(memory) = REGION.claim() else {
        return -1;
    };
    let Ok(slot) = memory.place(value) else {
        return -2;
    };
    *slot = slot.wrapping_add(1);
    let Ok(bytes) = memory.bytes(size) else {
        return -3;
    };
    let Ok(mut table) = memory.fixed_vec::<u32>(4) else {
        return -4;
    };
    let pushed = table.push(*slot);
    let total = table
        .as_slice()
        .iter()
        .copied()
        .fold(0_u32, u32::wrapping_add);
    let report = memory.seal();
    (bytes.len() as i64)
        .wrapping_add(i64::from(total))
        .wrapping_add(pushed.is_ok() as i64)
        .wrapping_add(report.used as i64)
}

static SIGNAL: Signal = Signal::new();

/// Waits for and raises a signal.
#[no_mangle]
pub extern "C" fn fixture_signal(notify: u32) -> i64 {
    let token = SIGNAL.token();
    if notify != 0 {
        SIGNAL.notify();
    }
    let result = SIGNAL.wait(
        token,
        core::time::Duration::from_millis(u64::from(notify % 8)),
    );
    #[cfg(inject_panic)]
    {
        // Negative control: an index the optimizer cannot prove in bounds.
        let table = [1_i64, 2, 3];
        if notify > 1 {
            return table[notify as usize];
        }
    }
    match result {
        SignalWait::Notified => 1,
        SignalWait::TimedOut => 0,
        SignalWait::WrongSignal => -1,
    }
}

/// Touches the native platform directly.
#[no_mangle]
pub extern "C" fn fixture_platform() -> u64 {
    use up_rust_core::platform::Platform;
    Native
        .now()
        .0
        .wrapping_add(Native.unix_time_ms().unwrap_or(0))
}
