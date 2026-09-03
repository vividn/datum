//! `datum-parse` for the browser: argv in, the row to insert out.
//!
//! datum.day used to carry a second parser, hand-written in TypeScript
//! (`web/src/lib/{parse,verb,entryId}.ts`). In under two days it produced
//! `-q`/`-qq` off by a factor of three, `-y`/`-yy` reading DAY offsets as
//! MINUTES, and `-y` stored as `state="-y"` — data corruption in a tool
//! whose whole purpose is recording when things happened. Nothing keeps a
//! hand-written second parser in agreement with the first, so this crate
//! removes it: the browser now runs datum's own parser (da-7scm), the same
//! way the dayview/nowview panes run datum's own layout (da-6h6p).
//!
//! ## The ABI
//!
//! Deliberately not wasm-bindgen. A `wasm32-unknown-unknown` module with no
//! imports loads with a bare `WebAssembly.instantiate` and needs no
//! toolchain beyond `cargo build --target wasm32-unknown-unknown`, which
//! keeps the web CI job free of Rust (see `.github/workflows/ci.yml`) and
//! the artifact small.
//!
//! Five exports, all operating on UTF-8 in the module's own linear memory —
//! the same shape `datum-layout-wasm` uses:
//!
//! ```text
//! alloc(len) -> ptr          reserve `len` bytes for the caller to write
//! dealloc(ptr, len)          release them
//! parse(ptr, len) -> ptr     JSON request -> JSON response, held module-side
//! response_len() -> len      the length of that response
//! response_clear()           drop it once the caller has copied it out
//! ```
//!
//! The response is parked in the module rather than packed into the return
//! value because a pointer and a length do not fit in one `i32`. See
//! `web/src/lib/parser.ts` for the other half.
//!
//! ## What the caller supplies
//!
//! Everything this crate would otherwise read from a host it does not have:
//! the wall clock (`now_ms`), the viewer's timezone, the device id, and the
//! random bytes a hid is made of. `datum-parse` is built here with
//! `--no-default-features`, so none of those can be reached for by accident
//! — the crate simply does not contain the code that would.

pub mod log;
pub mod request;

pub use request::parse_json;

/// Reserve `len` bytes and hand back a pointer the caller can write into.
/// Returns null for a zero-length or unsatisfiable request.
///
/// # Safety
/// The returned pointer is owned by the caller until it is passed back to
/// [`dealloc`] with the same `len`.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    let mut buf = Vec::<u8>::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Release a buffer obtained from [`alloc`].
///
/// # Safety
/// `ptr` must have come from [`alloc`] with the same `len`, and must not be
/// used afterwards.
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    drop(Vec::from_raw_parts(ptr, 0, len));
}

thread_local! {
    /// The most recent response, kept alive between [`parse`] and the
    /// caller's read of it. wasm is single-threaded; the `thread_local` is
    /// what makes the same code sound on the host, where the tests run.
    static RESPONSE: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Parse one JSON request. The UTF-8 JSON response is held module-side:
/// this returns its pointer, [`response_len`] its length, and
/// [`response_clear`] releases it.
///
/// # Safety
/// `ptr`/`len` must describe an initialized buffer inside this module's
/// memory, as produced by [`alloc`].
#[no_mangle]
pub unsafe extern "C" fn parse(ptr: *const u8, len: usize) -> *const u8 {
    let body = if ptr.is_null() || len == 0 {
        String::new()
    } else {
        String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).into_owned()
    };
    RESPONSE.with(|slot| {
        let mut slot = slot.borrow_mut();
        *slot = parse_json(&body).into_bytes();
        slot.as_ptr()
    })
}

/// The length in bytes of the response [`parse`] last produced.
#[no_mangle]
pub extern "C" fn response_len() -> usize {
    RESPONSE.with(|slot| slot.borrow().len())
}

/// Drop the held response once the caller has copied it out.
#[no_mangle]
pub extern "C" fn response_clear() {
    RESPONSE.with(|slot| *slot.borrow_mut() = Vec::new());
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bad_json_becomes_an_error_response_not_a_panic() {
        let raw = parse_json("{not json");
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(parsed["error"]
            .as_str()
            .unwrap()
            .contains("bad parse request"));
        assert!(parsed.get("entry").is_none());
    }

    #[test]
    fn the_c_abi_round_trips_a_request_through_linear_memory() {
        let body = json!({
            "argv": ["occur", "coffee"],
            "now_ms": 1_700_000_000_000i64,
            "timezone": "UTC",
            "offset_minutes": 0,
            "device_id": "web-test",
            "hid_bytes": vec![0u8; 12],
        })
        .to_string();
        unsafe {
            let ptr = alloc(body.len());
            std::ptr::copy_nonoverlapping(body.as_ptr(), ptr, body.len());

            let out_ptr = parse(ptr, body.len());
            let out_len = response_len();
            let text =
                String::from_utf8(std::slice::from_raw_parts(out_ptr, out_len).to_vec()).unwrap();
            response_clear();
            dealloc(ptr, body.len());

            assert_eq!(response_len(), 0, "response not released");
            let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(parsed["entry"]["field"], "coffee");
        }
    }
}
