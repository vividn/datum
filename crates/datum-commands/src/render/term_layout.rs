//! Terminal-responsive table layout helpers (da-5025).
//!
//! Two concerns live here:
//!
//! 1. **Probe the terminal** — width and height from a live
//!    `TIOCGWINSZ` ioctl when stdout is a TTY, falling back to
//!    `$COLUMNS` / `$LINES` env vars (the legacy path `now` / `nowview`
//!    still use) and finally to 80×24. The live ioctl is what lets
//!    `datum tail --watch` re-layout against the *actual* terminal
//!    size after a resize (da-hsqd) — env vars are captured at fork
//!    and never update.
//! 2. **Fit a cell to a width budget** — ANSI-aware truncate that
//!    preserves opening colour escapes, appends `…`, and emits a
//!    closing reset (`\x1b[0m`) so the trailing pad doesn't inherit
//!    whatever background colour was open at the truncation point.

use std::io::IsTerminal;

/// Live `TIOCGWINSZ` query against stdout. Returns `(cols, rows)`, or
/// `None` if the ioctl fails (stdout not a tty, or kernel reports a
/// zero-sized winsize as some pty wrappers do during teardown). Both
/// dimensions are clamped to `usize` — `ws_col` / `ws_row` are
/// `u16`, so no overflow path matters here.
///
/// da-hsqd: this is the "or equivalent" the bead asked for. We avoid
/// pulling in `crossterm` for one ioctl when `libc` is already a
/// workspace dep.
#[cfg(unix)]
fn query_winsize() -> Option<(usize, usize)> {
    // SAFETY: `winsize` is plain POD; `ioctl` writes into it on
    // success and is well-defined to return 0/-1. We pass STDOUT_FILENO
    // (always 1) and the correct request constant for the kernel's
    // winsize layout.
    unsafe {
        let mut sz: libc::winsize = std::mem::zeroed();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut sz) == 0
            && sz.ws_col > 0
            && sz.ws_row > 0
        {
            Some((sz.ws_col as usize, sz.ws_row as usize))
        } else {
            None
        }
    }
}

#[cfg(not(unix))]
fn query_winsize() -> Option<(usize, usize)> {
    None
}

/// Detect terminal width. Prefers a live `TIOCGWINSZ` ioctl when
/// stdout is a TTY (so `datum tail --watch` reflows after resize);
/// falls back to `$COLUMNS` and finally 80.
pub fn detect_cols() -> usize {
    if is_stdout_tty() {
        if let Some((cols, _)) = query_winsize() {
            if cols >= 20 {
                return cols;
            }
        }
    }
    if let Ok(s) = std::env::var("COLUMNS") {
        if let Ok(n) = s.parse::<usize>() {
            if n >= 20 {
                return n;
            }
        }
    }
    80
}

/// Detect terminal height. Same ioctl-first / env-fallback shape as
/// [`detect_cols`]. Used to pick a default `tail -n` that fills the
/// visible window. The minimum 5 keeps tests/CI runs from collapsing
/// to a single-row table when the terminal is unset or absurdly small.
pub fn detect_rows() -> usize {
    if is_stdout_tty() {
        if let Some((_, rows)) = query_winsize() {
            if rows >= 5 {
                return rows;
            }
        }
    }
    if let Ok(s) = std::env::var("LINES") {
        if let Ok(n) = s.parse::<usize>() {
            if n >= 5 {
                return n;
            }
        }
    }
    24
}

/// True when stdout is attached to a real terminal. Piped / redirected
/// output stays on the legacy fixed-width path so scripts get the
/// stable behaviour they expect.
pub fn is_stdout_tty() -> bool {
    std::io::stdout().is_terminal()
}

/// Truncate `s` so its visible width is at most `max_w`, preserving
/// ANSI escape structure. When truncation actually happens we append
/// `…` (which counts as one visible char) and then a `\x1b[0m` reset
/// so the post-padding background doesn't bleed.
///
/// `max_w == 0` collapses to the empty string — callers should clamp
/// before calling if zero is invalid for their layout.
pub fn truncate_visible(s: &str, max_w: usize) -> String {
    if max_w == 0 {
        return String::new();
    }
    let visible = visible_width(s);
    if visible <= max_w {
        return s.to_string();
    }
    // We need to leave room for the trailing `…`.
    let keep_visible = max_w.saturating_sub(1);
    let mut out = String::with_capacity(s.len());
    let mut in_esc = false;
    let mut seen_visible = 0usize;
    let mut any_escape = false;
    for c in s.chars() {
        if in_esc {
            out.push(c);
            if c == 'm' {
                in_esc = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_esc = true;
            any_escape = true;
            out.push(c);
            continue;
        }
        if seen_visible >= keep_visible {
            break;
        }
        out.push(c);
        seen_visible += 1;
    }
    out.push('…');
    if any_escape {
        out.push_str("\x1b[0m");
    }
    out
}

/// Count visible characters (re-export so callers don't have to
/// reach into `super::colors`).
pub fn visible_width(s: &str) -> usize {
    crate::render::colors::visible_width(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string_returns_unchanged() {
        assert_eq!(truncate_visible("hello", 10), "hello");
        assert_eq!(truncate_visible("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string_adds_ellipsis() {
        let got = truncate_visible("happy_camper", 8);
        assert_eq!(visible_width(&got), 8);
        assert!(got.ends_with('…'), "got: {got:?}");
    }

    /// ANSI escape sequences are preserved and counted as zero width;
    /// the result still has visible width <= `max_w` and ends in a
    /// reset.
    #[test]
    fn truncate_preserves_ansi_and_emits_reset() {
        let raw = "\x1b[48;2;120;200;100mhappy_camper\x1b[0m";
        let got = truncate_visible(raw, 8);
        assert_eq!(visible_width(&got), 8);
        assert!(got.starts_with("\x1b[48;2;120;200;100m"));
        assert!(got.ends_with("\x1b[0m"), "got: {got:?}");
        assert!(got.contains('…'));
    }

    /// Truncating to zero (an edge that callers should guard but we
    /// must not panic on) returns empty.
    #[test]
    fn truncate_zero_returns_empty() {
        assert_eq!(truncate_visible("anything", 0), "");
    }

    /// `max_w == 1` keeps room for only the ellipsis — no preserved
    /// visible character before it.
    #[test]
    fn truncate_to_one_just_emits_ellipsis() {
        let got = truncate_visible("anything", 1);
        assert_eq!(visible_width(&got), 1);
        assert!(got.contains('…'));
    }
}
