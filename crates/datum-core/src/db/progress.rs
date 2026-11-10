//! A progress handler on a borrowed connection. rusqlite's own
//! `Connection::progress_handler` takes `&mut` since 0.41, and the
//! connections a view's budget (`view.rs`) and a test's step count watch are
//! only borrowed from their [`super::Db`].

use std::ffi::{c_int, c_void};

use rusqlite::{ffi, Connection};

/// `body`, with `f` run every `n` SQLite VM steps on the connection while it
/// runs; `f`'s `true` interrupts the statement running. There is one handler
/// per connection: this replaces any other, and clears it as `body` returns
/// or unwinds. A scope and not a guard, since a guard could be forgotten
/// with the handler still pointing at `f`.
pub fn with_progress<F: FnMut() -> bool, R>(
    conn: &Connection,
    n: c_int,
    mut f: F,
    body: impl FnOnce() -> R,
) -> R {
    unsafe extern "C" fn call<F: FnMut() -> bool>(f: *mut c_void) -> c_int {
        let f = std::panic::AssertUnwindSafe(unsafe { &mut *f.cast::<F>() });
        // A panic may not cross into C: it interrupts the statement instead.
        std::panic::catch_unwind(move || { f }.0()).unwrap_or(true) as c_int
    }
    struct Clear<'c>(&'c Connection);
    impl Drop for Clear<'_> {
        fn drop(&mut self) {
            unsafe {
                ffi::sqlite3_progress_handler(self.0.handle(), 0, None, std::ptr::null_mut())
            };
        }
    }
    // SAFETY: SQLite calls `f` only from a step on this connection, and
    // `_clear`, dropped before `f` on every way out, removes it first.
    unsafe {
        let f = std::ptr::from_mut(&mut f).cast();
        ffi::sqlite3_progress_handler(conn.handle(), n, Some(call::<F>), f);
    }
    let _clear = Clear(conn);
    body()
}
