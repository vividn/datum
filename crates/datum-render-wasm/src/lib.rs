//! datum's renderer for the browser: an entry in, the CLI's lines out.
//!
//! datum.day parses with datum's own parser (`datum-parse-wasm`) and stores
//! the row it builds — and then, until this crate, echoed it through a
//! hand-written TypeScript stand-in: `16:18 mood = happy` where `datum`
//! prints
//!
//! ```text
//! CREATE 65nfi mood:2026-09-08T14:17:25.061Z
//! 16:17:25+2  mood  sad
//! ```
//!
//! Nothing keeps a second renderer in agreement with the first, so this
//! crate removes it the same way `datum-parse-wasm` removed the second
//! parser: the browser now runs `datum_commands::render::action_lines_at`,
//! the function the CLI's own printer prints from.
//!
//! ## The ABI
//!
//! The same five exports as `datum-parse-wasm` and `datum-layout-wasm`,
//! for the same reasons (no wasm-bindgen, no imports, a bare
//! `WebAssembly.instantiate`, a web CI job with no Rust in it):
//!
//! ```text
//! alloc(len) -> ptr          reserve `len` bytes for the caller to write
//! dealloc(ptr, len)          release them
//! render(ptr, len) -> ptr    JSON request -> JSON response, held module-side
//! response_len() -> len      the length of that response
//! response_clear()           drop it once the caller has copied it out
//! ```
//!
//! See `web/src/lib/renderer.ts` for the other half.

pub mod request;
pub use request::render_json;

// The five exports below exist only where they are called: in the wasm
// module. On the host this crate is an rlib whose whole interface is
// `render_json`, and its tests link `datum-parse-wasm` beside it — which
// exports the same five `#[no_mangle]` names. Two copies of one C symbol in
// a binary is a link error, so each export is gated to the target that
// needs it.

/// Reserve `len` bytes and hand back a pointer the caller can write into.
/// Returns null for a zero-length or unsatisfiable request.
///
/// # Safety
/// The returned pointer is owned by the caller until it is passed back to
/// [`dealloc`] with the same `len`.
#[cfg(target_arch = "wasm32")]
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

/// Release a buffer from [`alloc`].
///
/// # Safety
/// `ptr` must have come from [`alloc`] with this exact `len`, and must not
/// be used afterwards.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    drop(Vec::from_raw_parts(ptr, 0, len));
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    /// The last response, kept alive until the caller has read it. wasm is
    /// single-threaded; the `thread_local` is what makes the same code sound
    /// on the host, where the tests run.
    static RESPONSE: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Render one JSON request. The UTF-8 JSON response is held module-side:
/// this returns its pointer, [`response_len`] its length, and
/// [`response_clear`] releases it.
///
/// # Safety
/// `ptr`/`len` must describe an initialized buffer inside this module's
/// memory, as produced by [`alloc`].
#[cfg(target_arch = "wasm32")]
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
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn response_len() -> usize {
    RESPONSE.with(|slot| slot.borrow().len())
}

/// Drop the held response once the caller has copied it out.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn response_clear() {
    RESPONSE.with(|slot| *slot.borrow_mut() = Vec::new());
}
