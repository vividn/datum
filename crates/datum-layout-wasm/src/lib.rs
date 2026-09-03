//! `datum-layout` for the browser: JSON in, SVG out, ~9 KB gzipped.
//!
//! The PWA's dayview and nowview panes are the CLI's own compute and render
//! functions, not a second implementation of them (da-6h6p). Everything
//! those two views need is pure — `compute_dayview` takes a config struct,
//! not a database, and `datum-layout` depends on nothing — so the browser
//! can hold the layout while the SQLite-wasm it already runs holds the data.
//!
//! ## The ABI
//!
//! Deliberately not wasm-bindgen. A `wasm32-unknown-unknown` module with no
//! imports loads with a bare `WebAssembly.instantiate` and needs no
//! toolchain beyond `cargo build --target wasm32-unknown-unknown`, which
//! keeps the web CI job free of Rust (see `.github/workflows/ci.yml`) and
//! the artifact small.
//!
//! Five exports, all operating on UTF-8 in the module's own linear memory:
//!
//! ```text
//! alloc(len) -> ptr          reserve `len` bytes for the caller to write
//! dealloc(ptr, len)          release them
//! render(ptr, len) -> ptr    JSON request -> JSON response, held module-side
//! response_len() -> len      the length of that response
//! response_clear()           drop it once the caller has copied it out
//! ```
//!
//! The response is parked in the module rather than packed into the return
//! value because a pointer and a length do not fit in one `i32` — and
//! returning a pair means either a second export or an ABI that only works
//! at one pointer width, which would make these functions untestable on the
//! host. See `web/src/lib/layout.ts` for the other half.

pub mod colors;
pub mod request;

pub use request::render_json;

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
    /// The most recent response, kept alive between [`render`] and the
    /// caller's read of it. wasm is single-threaded; the `thread_local` is
    /// what makes the same code sound on the host, where the tests run.
    static RESPONSE: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Render one JSON request. The UTF-8 JSON response is held module-side:
/// this returns its pointer, [`response_len`] its length, and
/// [`response_clear`] releases it.
///
/// # Safety
/// `ptr`/`len` must describe an initialized buffer inside this module's
/// memory, as produced by [`alloc`].
#[no_mangle]
pub unsafe extern "C" fn render(ptr: *const u8, len: usize) -> *const u8 {
    let body = if ptr.is_null() || len == 0 {
        String::new()
    } else {
        String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).into_owned()
    };
    RESPONSE.with(|slot| {
        let mut slot = slot.borrow_mut();
        *slot = render_json(&body).into_bytes();
        slot.as_ptr()
    })
}

/// The length in bytes of the response [`render`] last produced.
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

    fn svg_of(request: serde_json::Value) -> String {
        let raw = render_json(&request.to_string());
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(parsed.get("error").is_none(), "unexpected error: {raw}");
        parsed["svg"].as_str().unwrap().to_string()
    }

    #[test]
    fn bad_json_becomes_an_error_response_not_a_panic() {
        let raw = render_json("{not json");
        let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(parsed["error"]
            .as_str()
            .unwrap()
            .contains("bad layout request"));
        assert!(parsed.get("svg").is_none());
    }

    #[test]
    fn dayview_paints_a_switch_block_in_the_state_colour() {
        let svg = svg_of(json!({
            "view": "dayview",
            "now_ms": 1_700_000_000_000i64,
            "days": [{
                "label_lines": ["Mon", "Apr 27"],
                "start_ms": 1_699_999_000_000i64,
                "end_ms": 1_700_003_000_000i64,
                "fields": [{
                    "field": "mood",
                    "blocks": [{
                        "start_ms": 1_700_000_000_000i64,
                        "end_ms": 1_700_001_000_000i64,
                        "state": "happy"
                    }]
                }]
            }]
        }));
        assert!(
            svg.starts_with("<svg"),
            "not an svg: {}",
            &svg[..60.min(svg.len())]
        );
        // md5("happy")[0..3] — the same colour `datum dayview` picks.
        assert!(
            svg.contains(&colors::md5_hex("happy")),
            "state colour missing from {svg}"
        );
    }

    #[test]
    fn a_field_with_no_marks_is_dropped_rather_than_drawn_empty() {
        let svg = svg_of(json!({
            "view": "dayview",
            "now_ms": 0,
            "days": [{
                "start_ms": 0,
                "end_ms": 86_400_000i64,
                "fields": [{ "field": "ghost" }]
            }]
        }));
        assert!(!svg.contains("ghost"), "empty field drawn: {svg}");
    }

    #[test]
    fn nowview_renders_the_clock_and_the_panel_state() {
        let svg = svg_of(json!({
            "view": "nowview",
            "now_ms": 1_700_000_000_000i64,
            "clock": "14:32",
            "fields": [{
                "field": "focus",
                "blocks": [{
                    "start_ms": 1_699_999_800_000i64,
                    "end_ms": 1_700_000_000_000i64,
                    "state": "deep"
                }]
            }],
            "panel_states": { "focus": "deep" }
        }));
        assert!(svg.contains("14:32"), "clock missing from {svg}");
        assert!(svg.contains("deep"), "panel state missing from {svg}");
    }

    #[test]
    fn the_c_abi_round_trips_a_request_through_linear_memory() {
        let body = json!({ "view": "dayview", "now_ms": 0, "days": [] }).to_string();
        unsafe {
            let ptr = alloc(body.len());
            std::ptr::copy_nonoverlapping(body.as_ptr(), ptr, body.len());

            let out_ptr = render(ptr, body.len());
            let out_len = response_len();
            let text =
                String::from_utf8(std::slice::from_raw_parts(out_ptr, out_len).to_vec()).unwrap();
            response_clear();
            dealloc(ptr, body.len());

            assert_eq!(response_len(), 0, "response not released");
            let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert!(parsed["svg"].as_str().unwrap().starts_with("<svg"));
        }
    }
}
