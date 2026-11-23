//! Ctrl-C while an `--atomic` script's lines run (da-ykeor): each has echoed
//! an entry, none written, so it says so, as a failing line does, before
//! SIGINT ends the process as it would have. A SIGINT ignored stays ignored.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The line read last, which the sentence names.
pub static LINE: AtomicUsize = AtomicUsize::new(1);

/// Said once: a second SIGINT, which `timeout -s INT` sends and a person may
/// press, can land on another thread while the first is being said.
static SAID: AtomicBool = AtomicBool::new(false);

/// The handler while it lives, the one before it put back on drop.
pub struct Said(Option<libc::sigaction>);

impl Said {
    pub fn install() -> Self {
        // SAFETY: plain `sigaction` calls on zeroed structs, the handler
        // an `extern "C"` fn making only async-signal-safe calls.
        unsafe {
            let mut old: libc::sigaction = std::mem::zeroed();
            libc::sigaction(libc::SIGINT, std::ptr::null(), &mut old);
            if old.sa_sigaction == libc::SIG_IGN {
                return Self(None);
            }
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = said as *const () as libc::sighandler_t;
            libc::sigemptyset(&mut sa.sa_mask);
            sa.sa_flags = libc::SA_RESTART;
            libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
            Self(Some(old))
        }
    }
}

impl Drop for Said {
    fn drop(&mut self) {
        if let Some(old) = &self.0 {
            // SAFETY: puts back the disposition `install` read.
            unsafe { libc::sigaction(libc::SIGINT, old, std::ptr::null_mut()) };
        }
    }
}

/// The sentence, written with no allocation, then the signal again with
/// the default put back. A second meanwhile returns, the first ending it.
extern "C" fn said(_: libc::c_int) {
    if SAID.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut buf = [0u8; 80];
    let mut len = 0;
    let mut put = |bytes: &[u8]| {
        buf[len..len + bytes.len()].copy_from_slice(bytes);
        len += bytes.len();
    };
    put(b"Error: stopped at line ");
    let (mut digits, mut n, mut at) = ([0u8; 20], LINE.load(Ordering::Relaxed), 20);
    loop {
        at -= 1;
        digits[at] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    put(&digits[at..]);
    put(b": interrupted, nothing was written\n");
    // SAFETY: `write`, `signal` and `raise` are async-signal-safe.
    unsafe {
        libc::write(2, buf.as_ptr().cast(), len);
        libc::signal(libc::SIGINT, libc::SIG_DFL);
        libc::raise(libc::SIGINT);
    }
}
