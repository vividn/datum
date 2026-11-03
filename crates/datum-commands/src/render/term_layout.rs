//! Terminal-responsive table layout helpers (da-5025).
//!
//! Two concerns live here:
//!
//! 1. **Probe the terminal** — width and height from a live
//!    `TIOCGWINSZ` ioctl when stdout is a TTY, falling back to
//!    `$COLUMNS` / `$LINES` env vars and finally to a caller-chosen
//!    default (80×24 here; `now` / `nowview` band rendering picks its
//!    own via [`detect_cols_or`]). The live ioctl is what lets
//!    `datum tail --watch` re-layout against the *actual* terminal
//!    size after a resize (da-hsqd) — env vars are captured at fork
//!    and never update.
//! 2. **Fit a cell to a width budget** — ANSI-aware truncate that
//!    preserves opening colour escapes, appends `…`, and emits a
//!    closing reset (`\x1b[0m`) so the trailing pad doesn't inherit
//!    whatever background colour was open at the truncation point.

use std::io::IsTerminal;
use unicode_width::UnicodeWidthChar;

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
    detect_cols_or(20, 80)
}

/// [`detect_cols`] with caller-chosen sanity floor and fallback.
/// `now` / `nowview` band rendering needs different fallbacks (da-tpny:
/// nowview's documented 40-col Snapshot reference width) but the same
/// ioctl-first probe — env vars like `$COLUMNS` are rarely exported to
/// children, so the env-only path renders 80 cols in a 200-col terminal.
pub fn detect_cols_or(min: usize, fallback: usize) -> usize {
    if is_stdout_tty() {
        if let Some((cols, _)) = query_winsize() {
            if cols >= min {
                return cols;
            }
        }
    }
    env_dim_or("COLUMNS", min, fallback)
}

/// The width a caller was GIVEN (da-coi0m): `$COLUMNS` when set — a
/// `COLUMNS=60 datum now` means sixty, whatever the terminal is — else the
/// terminal's, else `None`: piped with nothing set, where `datum now`
/// prints its list whole. Env first is the opposite of [`detect_cols_or`],
/// which `tail --watch` needs so a resize beats a stale exported value.
pub fn given_cols(min: usize) -> Option<usize> {
    given_dim("COLUMNS", min, |(c, _)| c)
}

/// [`given_cols`] for the height (da-q0xe5): `$LINES`, else the terminal's.
pub fn given_rows(min: usize) -> Option<usize> {
    given_dim("LINES", min, |(_, r)| r)
}

fn given_dim(var: &str, min: usize, pick: fn((usize, usize)) -> usize) -> Option<usize> {
    let env = std::env::var(var).ok().and_then(|s| s.parse().ok());
    let tty = || is_stdout_tty().then(query_winsize).flatten().map(pick);
    env.filter(|&n| n >= min)
        .or_else(|| tty().filter(|&n| n >= min))
}

/// Detect terminal height. Same ioctl-first / env-fallback shape as
/// [`detect_cols`]. Used to pick a default `tail -n` that fills the
/// visible window. The minimum 5 keeps tests/CI runs from collapsing
/// to a single-row table when the terminal is unset or absurdly small.
pub fn detect_rows() -> usize {
    detect_rows_or(5, 24)
}

/// [`detect_rows`] with caller-chosen sanity floor and fallback
/// (see [`detect_cols_or`]).
pub fn detect_rows_or(min: usize, fallback: usize) -> usize {
    if is_stdout_tty() {
        if let Some((_, rows)) = query_winsize() {
            if rows >= min {
                return rows;
            }
        }
    }
    env_dim_or("LINES", min, fallback)
}

/// Env-var half of the probe (`$COLUMNS` / `$LINES`), split out so tests
/// can exercise the fallback chain without depending on whether the test
/// runner's stdout is a real terminal (the ioctl path would shadow it).
fn env_dim_or(var: &str, min: usize, fallback: usize) -> usize {
    if let Ok(s) = std::env::var(var) {
        if let Ok(n) = s.parse::<usize>() {
            if n >= min {
                return n;
            }
        }
    }
    fallback
}

/// True when stdout is attached to a real terminal. Piped / redirected
/// output stays on the legacy fixed-width path so scripts get the
/// stable behaviour they expect.
pub fn is_stdout_tty() -> bool {
    std::io::stdout().is_terminal()
}

/// Truncate `s` so its visible width is at most `max_w`, preserving
/// ANSI escape structure. When truncation actually happens we append
/// `…` (which counts as one visible column) and then a `\x1b[0m` reset
/// so the post-padding background doesn't bleed.
///
/// Width is Unicode display width (da-dhtp): a double-width grapheme
/// (emoji, CJK) that would straddle the budget is dropped, not split,
/// so the result may come in one column under `max_w` but never over.
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
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if seen_visible + w > keep_visible {
            break;
        }
        out.push(c);
        seen_visible += w;
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

thread_local! {
    /// `--width N` for the command running on this thread (da-ky7m6):
    /// `None` unset, `Some(0)` unlimited.
    static WIDTH: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// `--width N` in force until the guard drops, as [`super::colors::ColorOverride`].
#[must_use = "the width lasts only as long as the guard"]
pub struct WidthOverride(Option<usize>);

impl WidthOverride {
    pub fn set(width: usize) -> Self {
        Self(WIDTH.with(|w| w.replace(Some(width))))
    }
}

impl Drop for WidthOverride {
    fn drop(&mut self) {
        WIDTH.with(|w| w.set(self.0));
    }
}

/// The `--width` given, `Some(None)` for `--width 0`.
pub fn width_given() -> Option<Option<usize>> {
    WIDTH.with(|w| w.get()).map(|n| (n > 0).then_some(n))
}

/// The width a list of rows fits (da-ky7m6): `--width N`, `0` unlimited;
/// else, when stdout is a terminal, `$COLUMNS` or the terminal's width;
/// else (piped, redirected) unlimited, every column whole.
pub fn row_width() -> Option<usize> {
    width_given().unwrap_or_else(|| is_stdout_tty().then(|| given_cols(20).unwrap_or(80)))
}

/// How a column gives way to a width.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Give {
    /// A number, date, time or id: never cut.
    Whole,
    /// Words: cut with `…`, the widest first.
    Text,
    /// A field's name, or what else names the row: text, capped at a
    /// third of the width, kept to 6 and never left out.
    Field,
}

impl Give {
    /// A view column's kind, by its name and then its cells: `field` is a
    /// field, `hid`, `id` and `*_id` ids, and a column whose every cell is
    /// a number, date or time is whole.
    pub fn of_column(name: &str, cells: impl IntoIterator<Item = impl AsRef<str>>) -> Give {
        let whole = |c: &str| {
            c.chars().any(|c| c.is_ascii_digit())
                && c.chars()
                    .all(|c| c.is_ascii_digit() || "-+:.,/±TZ ".contains(c))
        };
        match name {
            "field" => Give::Field,
            "hid" | "id" => Give::Whole,
            n if n.ends_with("_id") => Give::Whole,
            // A column with no cells has nothing whole to keep.
            _ => {
                let mut filled = cells
                    .into_iter()
                    .filter(|c| !c.as_ref().is_empty())
                    .peekable();
                match filled.peek().is_some() && filled.all(|c| whole(c.as_ref())) {
                    true => Give::Whole,
                    false => Give::Text,
                }
            }
        }
    }
}

/// Each column's width once a table, columns two apart, fits `width`
/// (da-ky7m6, the rule `now` keeps): a field's column is capped at a third
/// of it, then the text columns give way, the widest first, down to one
/// (`…`) and a field's to 6; then the last text column still shown goes, a
/// width of 0, before a whole one is cut. `None` is every column at its
/// `natural` width.
pub fn fit_widths(natural: &[usize], give: &[Give], width: Option<usize>) -> Vec<usize> {
    assert_eq!(give.len(), natural.len(), "a kind for every column");
    let mut w = natural.to_vec();
    let Some(max) = width else { return w };
    // A third of a narrow width is less than the 6 a field is kept to.
    for (w, g) in w.iter_mut().zip(give) {
        if *g == Give::Field {
            *w = (*w).min((max / 3).max(6));
        }
    }
    let total = |w: &[usize]| {
        let shown = w.iter().filter(|&&n| n > 0);
        shown.clone().sum::<usize>() + 2 * shown.count().saturating_sub(1)
    };
    let floor = |i: usize| match give[i] {
        Give::Field => natural[i].min(6),
        _ => 1,
    };
    while total(&w) > max {
        let shrinks = (0..w.len()).filter(|&i| give[i] != Give::Whole && w[i] > floor(i));
        let mut goes = (0..w.len()).filter(|&i| give[i] == Give::Text);
        if let Some(i) = shrinks.max_by_key(|&i| w[i]) {
            w[i] -= 1;
        } else if let Some(i) = goes.rfind(|&i| w[i] > 0) {
            w[i] = 0;
        } else {
            break;
        }
    }
    w
}

/// A table fit to [`row_width`]: a header line, a rule of dashes under it
/// when `rule`, then a line a row. A column `right` aligns to its right
/// edge, a cell longer than its column is cut with `…`, a column fit to
/// nothing is left out, and a line still too wide (whole columns alone
/// past the width) is cut at it.
pub fn grid(
    heads: &[&str],
    give: &[Give],
    right: &[bool],
    rows: &[Vec<String>],
    rule: bool,
) -> Vec<String> {
    let natural: Vec<usize> = (0..heads.len())
        .map(|i| {
            let cells = rows
                .iter()
                .filter_map(|r| r.get(i))
                .map(|c| visible_width(c));
            cells.chain([visible_width(heads[i])]).max().unwrap_or(0)
        })
        .collect();
    let width = row_width();
    let widths = fit_widths(&natural, give, width);
    let line = |cells: &mut dyn Iterator<Item = &str>| {
        let shown = cells.zip(&widths).enumerate().filter(|(_, (_, &w))| w > 0);
        let parts: Vec<String> = shown
            .map(|(i, (c, &w))| {
                let c = truncate_visible(c, w);
                let pad = " ".repeat(w.saturating_sub(visible_width(&c)));
                match right.get(i) == Some(&true) {
                    true => format!("{pad}{c}"),
                    false => format!("{c}{pad}"),
                }
            })
            .collect();
        clip(&parts.join("  "), width)
    };
    let dashes: Vec<String> = widths.iter().map(|&n| "-".repeat(n)).collect();
    let mut out = vec![line(&mut heads.iter().copied())];
    if rule {
        out.push(line(&mut dashes.iter().map(String::as_str)));
    }
    out.extend(rows.iter().map(|r| line(&mut r.iter().map(String::as_str))));
    out
}

/// `line` cut with `…` at `width`, when it runs past it.
pub fn clip(line: &str, width: Option<usize>) -> String {
    match width {
        Some(w) if visible_width(line) > w => truncate_visible(line, w),
        _ => line.to_string(),
    }
}

/// Every line of `text` [`clip`]ped to [`row_width`]: a list whose text is
/// its last column.
pub fn clip_lines(text: &str) -> String {
    let width = row_width();
    text.split_inclusive('\n')
        .map(|l| match l.strip_suffix('\n') {
            Some(body) => clip(body, width) + "\n",
            None => clip(l, width),
        })
        .collect()
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

    /// da-dhtp: double-width graphemes budget two columns. Five emoji
    /// (10 columns) into an 8-column cell keep only three emoji
    /// (6 cols) + `…` (1 col) = 7 — the fourth emoji would straddle
    /// the budget, so it's dropped, not split.
    #[test]
    fn truncate_budgets_double_width_chars() {
        let got = truncate_visible("😊😊😊😊😊", 8);
        assert_eq!(got, "😊😊😊…");
        assert!(visible_width(&got) <= 8);
    }

    /// da-dhtp: truncation landing mid-double-width drops the char.
    /// "ab日" is 4 columns; a 3-column budget keeps "ab" (the 日 would
    /// need cols 3-4 but only col 3 remains before the ellipsis slot).
    #[test]
    fn truncate_drops_straddling_cjk_char() {
        let got = truncate_visible("ab日本語", 3);
        assert_eq!(got, "ab…");
        assert_eq!(visible_width(&got), 3);
    }

    /// CJK string that already fits is returned unchanged.
    #[test]
    fn truncate_cjk_fitting_returns_unchanged() {
        assert_eq!(truncate_visible("日本", 4), "日本");
    }

    /// da-tpny: the now/nowview band probes fall back to `$COLUMNS` /
    /// `$LINES` and then to a caller-chosen default. All assertions live
    /// in one test fn — `COLUMNS` is process-global state and threaded
    /// `cargo test` would race split-up variants.
    #[test]
    fn env_dim_honors_var_floor_and_per_caller_fallback() {
        std::env::set_var("COLUMNS", "137");
        assert_eq!(env_dim_or("COLUMNS", 20, 80), 137);
        // Below the caller's sanity floor → fall through to the fallback.
        std::env::set_var("COLUMNS", "3");
        assert_eq!(env_dim_or("COLUMNS", 4, 40), 40);
        std::env::remove_var("COLUMNS");
        // Unset → per-caller fallback (now band: 80, nowview Snapshot: 40).
        assert_eq!(env_dim_or("COLUMNS", 20, 80), 80);
        assert_eq!(env_dim_or("COLUMNS", 4, 40), 40);
    }

    /// da-ky7m6: a field is cut to a third, never under 6; text gives way
    /// widest first, then the last text column goes before a whole one.
    #[test]
    fn fit_widths_cuts_text_before_whole_columns() {
        use Give::*;
        let give = [Field, Whole, Text, Text];
        assert_eq!(fit_widths(&[30, 10, 8, 20], &give, None), [30, 10, 8, 20]);
        // 40: the field capped at 13, then field and text give way, widest first.
        let fit = |w| fit_widths(&[30, 10, 8, 20], &give, Some(w));
        // 60: the field alone is cut, to a third; the rest fit whole.
        assert_eq!(fit_widths(&[30, 10, 8, 5], &give, Some(60)), [20, 10, 8, 5]);
        assert_eq!(fit(40), [8, 10, 8, 8]);
        // 25: the field stops at its 6 and the text columns level down.
        assert_eq!(fit(25), [6, 10, 2, 1]);
        // 20: text at 1 each is still too wide, so the last goes, then the next.
        assert_eq!(fit(20), [6, 10, 0, 0]);
        // A width under 18 still leaves the field its 6; the line is cut.
        assert_eq!(fit(2), [6, 10, 0, 0]);
    }

    /// A column of empty cells has nothing whole to keep; one of dates does.
    #[test]
    fn of_column_reads_the_cells() {
        assert_eq!(Give::of_column("x", ["", ""]), Give::Text);
        assert_eq!(Give::of_column("x", ["2026-11-03", ""]), Give::Whole);
        assert_eq!(Give::of_column("x", ["done", "2026-11-03"]), Give::Text);
        assert_eq!(Give::of_column("field", ["1"]), Give::Field);
        assert_eq!(Give::of_column("kit_id", ["chores"]), Give::Whole);
        assert_eq!(Give::of_column("hid", ["abc12"]), Give::Whole);
        assert_eq!(Give::of_column("n", ["0", "12.5", "-3"]), Give::Whole);
        assert_eq!(
            Give::of_column("at", ["2026-11-03T08:17:55+00:00"]),
            Give::Whole
        );
    }
}
