//! `datum tail --watch` on a TTY: a scrollable, live-tailing TUI (da-3l6m).
//!
//! The non-TTY watch path stays on the append/repaint loop in
//! [`super::tail`]. When stdout is a real terminal we instead enter a
//! crossterm **alternate screen** + raw mode and run an event loop that
//! marries two input sources:
//!
//! * **key / mouse events** — scroll the viewport (line / page / top /
//!   bottom, mouse wheel) and quit (`q` / `Esc` / `Ctrl-C`);
//! * **a poll tick** — the same `--watch` cadence refetches a generous
//!   history window so live entries arrive and corrections to already-
//!   visible rows reconcile in place.
//!
//! Browse-by-default (bead point 2): scrolling up *holds* position; it
//! does not snap to the newest row. While scrolled up, new arrivals are
//! announced by a status-bar indicator (`▼ N new below`) plus a hotkey
//! hint. Pressing `G`/`End` jumps to the latest row *and* resumes live
//! follow.
//!
//! Mutation-safe viewport (bead NOTES): every refetch re-sorts the
//! buffer (the DB already returns oldest-first by metric+slug) and
//! re-anchors the viewport to a **stable entry** (the slug of the
//! top-visible row), not a fixed row index — so a corrected/inserted
//! row slots into its time-sorted position without jerking the view the
//! user is reading.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    cursor,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
        MouseEventKind,
    },
    queue,
    style::{Attribute, Print, ResetColor, SetAttribute},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use datum_core::{db::Db, entry::Entry, field_spec::SpecRegistry, time::DatumTime};

use super::render_helpers::build_render_contexts;
use super::tail::{split_columns, tail_params};
use crate::args::OutputFlagsExt;
use crate::render::term_layout::{truncate_visible, visible_width};
use crate::render::{format_table_with, OutputArgs, TableArgs, TimeMetricCol};

/// Initial history window fetched when the user hasn't pinned `-n`. Large
/// enough that scroll-up reveals far more than one screenful; grows on
/// demand (see [`MAX_BUFFER`]) when the user scrolls to the very top and
/// the window is saturated.
const INITIAL_BUFFER: usize = 500;
/// Hard ceiling on the auto-grown history window, so a held PgUp on a
/// million-row DB can't pull the whole table into memory.
const MAX_BUFFER: usize = 50_000;

/// Entry point from [`super::tail::tail`] when `--watch` is set and
/// stdout is a TTY. Owns the screen until the user quits, then restores
/// it. Returns once the loop exits cleanly.
pub fn run(
    args: &crate::args::TailArgs,
    db: &Db,
    metric: TimeMetricCol,
    registry: Option<&SpecRegistry>,
    interval: Duration,
) -> Result<()> {
    let output = args.output()?;
    let columns = split_columns(&args.columns);
    let _guard = TuiGuard::enter()?;

    let (mut cols, mut rows) = terminal::size().unwrap_or((80, 24));
    let mut fetch_cap = args.count.unwrap_or(INITIAL_BUFFER);
    let mut buffer = fetch_buffer(args, db, fetch_cap)?;
    let mut state = ViewState::new();
    state.recompute(&slugs_of(&buffer), view_height(rows), None);

    // Per-frame fingerprint: terminal dims, buffer size + freshness, and
    // the viewport/indicator state. Equal signatures paint identical
    // pixels, so we skip the repaint (idle-frame suppression).
    type FrameSig = (
        u16,
        u16,
        usize,
        Option<chrono::DateTime<chrono::Utc>>,
        usize,
        bool,
        usize,
    );
    let mut last_sig: Option<FrameSig> = None;
    let mut next_fetch = Instant::now() + interval;

    loop {
        // ── Render (idle-frame suppression: only repaint when the frame
        //    signature actually changes, mirroring the append loop's
        //    da-d6yo behaviour). ──
        let view_h = view_height(rows);
        let slugs = slugs_of(&buffer);
        let max = max_scroll(slugs.len(), view_h);
        let new_below = state.new_below(&slugs, view_h);
        let sig = (
            cols,
            rows,
            buffer.len(),
            max_modify(&buffer),
            state.scroll,
            state.follow,
            new_below,
        );
        if last_sig.as_ref() != Some(&sig) {
            render_frame(
                &buffer, &state, new_below, db, metric, registry, &output, &columns, cols, rows,
            )?;
            last_sig = Some(sig);
        }

        // Scroll-to-top while the window is saturated → pull older rows
        // in on the next tick (paging). Forcing the deadline keeps it
        // responsive instead of waiting out the poll interval.
        if state.scroll == 0
            && args.count.is_none()
            && buffer.len() == fetch_cap
            && fetch_cap < MAX_BUFFER
        {
            next_fetch = Instant::now();
        }

        // ── Wait for input or the next poll tick, whichever comes first. ──
        let timeout = next_fetch.saturating_duration_since(Instant::now());
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(k) if k.kind != KeyEventKind::Release => {
                    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
                    match k.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('c') if ctrl => break,
                        KeyCode::Up | KeyCode::Char('k') => state.line_up(),
                        KeyCode::Down | KeyCode::Char('j') => state.line_down(max),
                        KeyCode::PageUp => state.page_up(page(view_h)),
                        KeyCode::PageDown => state.page_down(page(view_h), max),
                        KeyCode::Home | KeyCode::Char('g') => state.jump_top(),
                        KeyCode::End | KeyCode::Char('G') => state.jump_bottom(max),
                        _ => {}
                    }
                }
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::ScrollUp => {
                        for _ in 0..3 {
                            state.line_up();
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        for _ in 0..3 {
                            state.line_down(max);
                        }
                    }
                    _ => {}
                },
                Event::Resize(w, h) => {
                    cols = w;
                    rows = h;
                    // Re-clamp against the new height; follow sticks to bottom.
                    state.recompute(&slugs, view_height(rows), None);
                }
                _ => {}
            }
        }

        // ── Poll tick: refetch + reconcile against a stable anchor. ──
        if Instant::now() >= next_fetch {
            let anchor = if state.follow {
                None
            } else {
                slugs.get(state.scroll).cloned()
            };
            if state.scroll == 0 && args.count.is_none() && buffer.len() == fetch_cap {
                fetch_cap = (fetch_cap.saturating_mul(2)).min(MAX_BUFFER);
            }
            buffer = fetch_buffer(args, db, fetch_cap)?;
            state.recompute(&slugs_of(&buffer), view_height(rows), anchor);
            next_fetch = Instant::now() + interval;
        }
    }

    Ok(())
}

/// Fetch the history window backing the viewport. Honours an explicit
/// `-n` (the user asked for exactly that many); otherwise fills to `cap`,
/// which the loop grows as the user pages toward older entries. The DB
/// returns oldest-first by metric+slug, so the buffer is already the
/// chronological top-to-bottom order the viewport renders.
fn fetch_buffer(args: &crate::args::TailArgs, db: &Db, cap: usize) -> Result<Vec<Entry>> {
    let mut params = tail_params(args)?;
    if args.count.is_none() {
        params.limit = Some(cap);
    }
    Ok(db.list_entries(&params)?)
}

fn slugs_of(buffer: &[Entry]) -> Vec<String> {
    buffer.iter().map(|e| e.slug.as_str().to_string()).collect()
}

fn max_modify(buffer: &[Entry]) -> Option<chrono::DateTime<chrono::Utc>> {
    buffer.iter().map(|e| e.modify_time).max()
}

/// Body rows available for entries: terminal height minus the pinned
/// header row and the bottom status bar. Floored at 1 so absurdly short
/// terminals still render a row.
fn view_height(rows: u16) -> usize {
    (rows as usize).saturating_sub(2).max(1)
}

/// Page stride for PgUp/PgDn: a screenful minus one row of overlap so the
/// user keeps a line of context across the jump.
fn page(view_h: usize) -> usize {
    view_h.saturating_sub(1).max(1)
}

/// Largest valid scroll offset: the index of the first body row such that
/// the remaining rows exactly fill the viewport. Zero when everything fits.
fn max_scroll(body_len: usize, view_h: usize) -> usize {
    body_len.saturating_sub(view_h)
}

/// Viewport scroll state — deliberately terminal-agnostic so the scroll /
/// follow / new-below arithmetic is unit-testable without a TTY.
///
/// `scroll` is the buffer index of the top visible body row (0 = oldest).
/// `follow` is the explicit "stick to the newest row" intent: it survives
/// data growth (so live arrivals keep the viewport pinned to the bottom),
/// and is *not* merely `scroll == max` — that equivalence breaks the
/// instant new rows extend the buffer past the old maximum.
/// `bottom_seen` freezes the newest slug the user has actually viewed at
/// the bottom; it is the baseline the "N new below" count measures against.
#[derive(Debug, Default)]
struct ViewState {
    scroll: usize,
    follow: bool,
    bottom_seen: Option<String>,
}

impl ViewState {
    fn new() -> Self {
        Self {
            scroll: 0,
            follow: true,
            bottom_seen: None,
        }
    }

    /// Re-derive the scroll offset after the buffer changes (refetch or
    /// resize). Following pins to the new bottom and advances the
    /// seen-baseline; otherwise we re-anchor to `anchor` (the slug that
    /// was top-visible before the change) so a corrected/inserted row
    /// keeps the user looking at the same entry, then clamp.
    fn recompute(&mut self, slugs: &[String], view_h: usize, anchor: Option<String>) {
        let max = max_scroll(slugs.len(), view_h);
        if self.follow {
            self.scroll = max;
            self.bottom_seen = slugs.last().cloned();
        } else {
            if let Some(a) = anchor {
                if let Some(idx) = slugs.iter().position(|s| s == &a) {
                    self.scroll = idx;
                }
            }
            self.scroll = self.scroll.min(max);
        }
    }

    /// Count of entries newer than the seen-baseline — the "N new below"
    /// figure. Zero while following or parked at the bottom (nothing is
    /// below you), and zero once the baseline catches the newest row.
    fn new_below(&self, slugs: &[String], view_h: usize) -> usize {
        let max = max_scroll(slugs.len(), view_h);
        if self.follow || self.scroll >= max {
            return 0;
        }
        match &self.bottom_seen {
            Some(b) => match slugs.iter().position(|s| s == b) {
                Some(idx) => slugs.len().saturating_sub(idx + 1),
                None => 0,
            },
            None => 0,
        }
    }

    fn line_up(&mut self) {
        self.follow = false;
        self.scroll = self.scroll.saturating_sub(1);
    }

    fn line_down(&mut self, max: usize) {
        self.scroll = (self.scroll + 1).min(max);
        if self.scroll >= max {
            self.follow = true;
        }
    }

    fn page_up(&mut self, page: usize) {
        self.follow = false;
        self.scroll = self.scroll.saturating_sub(page);
    }

    fn page_down(&mut self, page: usize, max: usize) {
        self.scroll = (self.scroll + page).min(max);
        if self.scroll >= max {
            self.follow = true;
        }
    }

    fn jump_top(&mut self) {
        self.follow = false;
        self.scroll = 0;
    }

    fn jump_bottom(&mut self, max: usize) {
        self.follow = true;
        self.scroll = max;
    }
}

/// Paint one frame: pinned header row, the scrolled body window, blank
/// fill, and the bottom status bar. Repaints line-by-line (no full-screen
/// clear) so the alt-screen surface doesn't flash between frames.
#[allow(clippy::too_many_arguments)]
fn render_frame(
    buffer: &[Entry],
    state: &ViewState,
    new_below: usize,
    db: &Db,
    metric: TimeMetricCol,
    registry: Option<&SpecRegistry>,
    output: &OutputArgs,
    columns: &[String],
    cols: u16,
    rows: u16,
) -> io::Result<()> {
    let (header, body) = render_lines(buffer, db, metric, registry, output, columns, cols);
    let view_h = view_height(rows);
    let width = cols as usize;

    let mut out = io::stdout().lock();
    // Header row.
    queue!(out, cursor::MoveTo(0, 0), Clear(ClearType::CurrentLine))?;
    if let Some(h) = &header {
        queue!(out, Print(truncate_visible(h, width)), ResetColor)?;
    }
    // Body window.
    for r in 0..view_h {
        let row = (r + 1) as u16;
        queue!(out, cursor::MoveTo(0, row), Clear(ClearType::CurrentLine))?;
        if let Some(line) = body.get(state.scroll + r) {
            queue!(out, Print(truncate_visible(line, width)), ResetColor)?;
        }
    }
    // Status bar (last row).
    let bar = status_bar(state, new_below, buffer.len(), view_h, width);
    queue!(
        out,
        cursor::MoveTo(0, rows.saturating_sub(1)),
        Clear(ClearType::CurrentLine),
        SetAttribute(Attribute::Reverse),
        Print(bar),
        SetAttribute(Attribute::Reset),
        ResetColor,
    )?;
    out.flush()
}

/// Render the buffer to a header line + body lines via the shared table
/// formatter. The table emits exactly one line per entry, so `body[i]`
/// corresponds to `buffer[i]` — the invariant the scroll/anchor math
/// relies on. An empty buffer yields no header and a single `[No data]`
/// body line.
fn render_lines(
    buffer: &[Entry],
    db: &Db,
    metric: TimeMetricCol,
    registry: Option<&SpecRegistry>,
    output: &OutputArgs,
    columns: &[String],
    cols: u16,
) -> (Option<String>, Vec<String>) {
    let tbl = TableArgs {
        output: output.clone(),
        metric,
        columns: columns.to_vec(),
        suppress_header: false,
        max_total_width: Some(cols as usize),
    };
    let now = DatumTime::now();
    let ctxs = build_render_contexts(buffer, db);
    let text = match format_table_with(buffer, Some(&ctxs), Some(&now), registry, &tbl) {
        Some(t) => t,
        None => return (None, Vec::new()),
    };
    let mut lines = text.lines().map(|l| l.to_string());
    if buffer.is_empty() {
        // `[No data]` — no header row.
        (None, lines.collect())
    } else {
        let header = lines.next();
        (header, lines.collect())
    }
}

/// Build the bottom status bar, padded to the full terminal width so the
/// reverse-video bar spans the row. Surfaces the live/browse mode and,
/// when scrolled up with fresh arrivals, the `▼ N new below` indicator
/// plus the jump-to-latest hotkey hint (bead point 2).
fn status_bar(
    state: &ViewState,
    new_below: usize,
    total: usize,
    view_h: usize,
    width: usize,
) -> String {
    let text = if total == 0 {
        " no entries · q quit".to_string()
    } else if new_below > 0 {
        format!(" ▼ {new_below} new below · G→latest · q quit")
    } else if state.follow {
        format!(" ● live · {total} entries · ↑↓/PgUp/PgDn scroll · q quit")
    } else {
        let top = state.scroll + 1;
        let bot = (state.scroll + view_h).min(total);
        format!(" ◌ browse {top}–{bot}/{total} · G→latest · q quit")
    };
    let mut bar = truncate_visible(&text, width);
    let pad = width.saturating_sub(visible_width(&bar));
    bar.push_str(&" ".repeat(pad));
    bar
}

/// RAII terminal restore. Entering installs raw mode + the alternate
/// screen + mouse capture and hides the cursor; `Drop` undoes all four —
/// so a normal return, an early `?`, or an unwinding panic all leave the
/// user's terminal as we found it. A chained panic hook covers the same
/// teardown for the panic-message path (where `Drop` ordering vs. the
/// default hook's stderr write would otherwise scramble the output).
type PanicHook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

struct TuiGuard {
    prior_hook: Option<PanicHook>,
}

impl TuiGuard {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let mut out = io::stdout();
        crossterm::execute!(out, EnterAlternateScreen, EnableMouseCapture, cursor::Hide)?;

        // Restore the terminal before the default hook prints the panic
        // message, so the backtrace lands on the normal screen.
        let prior = std::panic::take_hook();
        std::panic::set_hook(Box::new(|info| {
            restore_terminal();
            eprintln!("{info}");
        }));
        Ok(Self {
            prior_hook: Some(prior),
        })
    }
}

impl Drop for TuiGuard {
    fn drop(&mut self) {
        restore_terminal();
        if let Some(hook) = self.prior_hook.take() {
            std::panic::set_hook(hook);
        }
    }
}

/// Idempotent terminal teardown — safe to call from both `Drop` and the
/// panic hook (disabling raw mode / leaving the alt screen twice is
/// harmless). Errors are swallowed: we're on a cleanup path and there's
/// nowhere useful to report to.
fn restore_terminal() {
    let mut out = io::stdout();
    let _ = crossterm::execute!(out, cursor::Show, DisableMouseCapture, LeaveAlternateScreen);
    let _ = terminal::disable_raw_mode();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slugs(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("f:{i:04}")).collect()
    }

    /// A fresh state follows: recompute pins to the bottom and seeds the
    /// seen-baseline at the newest slug, so nothing reads as "new below".
    #[test]
    fn follow_pins_to_bottom_and_clears_indicator() {
        let s = slugs(100);
        let mut v = ViewState::new();
        v.recompute(&s, 10, None);
        assert_eq!(v.scroll, max_scroll(100, 10)); // 90
        assert_eq!(v.bottom_seen.as_deref(), Some("f:0099"));
        assert_eq!(v.new_below(&s, 10), 0);
    }

    /// Everything fitting on screen → scroll 0, no indicator.
    #[test]
    fn small_buffer_fits_without_scroll() {
        let s = slugs(5);
        let mut v = ViewState::new();
        v.recompute(&s, 10, None);
        assert_eq!(v.scroll, 0);
        assert_eq!(v.new_below(&s, 10), 0);
    }

    /// Scrolling up drops follow and holds position; a subsequent refetch
    /// that appends rows keeps the viewport anchored to the same entry and
    /// surfaces the new arrivals as "N new below".
    #[test]
    fn scroll_up_holds_position_and_counts_new_arrivals() {
        let s = slugs(100);
        let mut v = ViewState::new();
        v.recompute(&s, 10, None); // scroll 90, follow, seen f:0099
        v.line_up(); // scroll 89, browsing
        assert!(!v.follow);
        assert_eq!(v.scroll, 89);
        assert_eq!(v.new_below(&s, 10), 0); // no new entries yet

        // Three new entries arrive; anchor on the previously top-visible row.
        let anchor = s[v.scroll].clone();
        let s2 = slugs(103);
        v.recompute(&s2, 10, Some(anchor));
        assert_eq!(v.scroll, 89); // same entry still top-visible
        assert!(!v.follow);
        assert_eq!(v.new_below(&s2, 10), 3); // f:0100..0102 are new below
    }

    /// A correction/insertion *above* the viewport shifts the anchored
    /// entry's index; the viewport tracks the entry, not the old row index.
    #[test]
    fn insertion_above_viewport_tracks_anchor_entry() {
        let mut s = slugs(100);
        let mut v = ViewState::new();
        v.recompute(&s, 10, None);
        v.page_up(9); // scroll 81
        let anchor = s[v.scroll].clone(); // f:0081

        // An older row slots in at the top: every later index shifts by one.
        s.insert(0, "f:_new".to_string());
        v.recompute(&s, 10, Some(anchor));
        assert_eq!(v.scroll, 82); // anchor entry followed its new index
    }

    /// `G`/`End` jumps to the latest row and resumes follow, clearing the
    /// indicator.
    #[test]
    fn jump_to_bottom_resumes_follow() {
        let s = slugs(100);
        let mut v = ViewState::new();
        v.recompute(&s, 10, None);
        v.page_up(9);
        v.page_up(9);
        assert!(!v.follow);
        v.jump_bottom(max_scroll(100, 10));
        assert!(v.follow);
        assert_eq!(v.scroll, 90);
        // Following pins to bottom on the next recompute and re-seeds.
        let s2 = slugs(105);
        v.recompute(&s2, 10, None);
        assert_eq!(v.scroll, max_scroll(105, 10));
        assert_eq!(v.new_below(&s2, 10), 0);
    }

    /// Line-down that reaches the bottom edge re-arms follow, so live
    /// arrivals resume sticking to the newest row.
    #[test]
    fn line_down_to_edge_rearms_follow() {
        let s = slugs(20);
        let mut v = ViewState::new();
        v.recompute(&s, 10, None); // scroll 10
        v.line_up(); // 9, browsing
        assert!(!v.follow);
        v.line_down(max_scroll(20, 10)); // back to 10 == max
        assert!(v.follow);
    }

    /// The seen-baseline stays frozen across refetches while browsing, so
    /// the "new below" count accumulates rather than resetting each tick.
    #[test]
    fn baseline_frozen_while_browsing() {
        let mut v = ViewState::new();
        v.recompute(&slugs(50), 10, None);
        v.page_up(9); // browse, seen f:0049
        let anchor = format!("f:{:04}", v.scroll);
        v.recompute(&slugs(52), 10, Some(anchor.clone()));
        assert_eq!(v.new_below(&slugs(52), 10), 2);
        v.recompute(&slugs(55), 10, Some(anchor));
        assert_eq!(v.bottom_seen.as_deref(), Some("f:0049"));
        assert_eq!(v.new_below(&slugs(55), 10), 5); // baseline never moved
    }
}
