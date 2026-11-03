//! Terminal renderer for [`NowviewField`] data: the band `datum now` draws
//! beneath its active-states list, one labeled row a field with `└────┘`
//! axis chrome (da-apbrm: `nowview` draws only the SVG).
//!
//! ```text
//! coffee  ░░░░░░░░██░░░░░░░│██
//! mood    █████████████████│██
//!         └────────────────┘
//!         -30m    -15m   now
//! ```
//!
//! Pure: no I/O, no system deps. Compiles to `wasm32-unknown-unknown`.
//! ANSI escapes are embedded as plain UTF-8 — callers decide whether the
//! destination understands them.

use crate::nowview::{relative_label, NowviewField, NowviewPoint};

/// Inputs to [`render_nowview_terminal`].
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    /// End of the visible window — now, unless the window is centred on
    /// another instant — in UTC epoch ms.
    pub end_ms: i64,
    /// Now, in UTC epoch ms: a cell past it is blank, not yet known.
    pub now_ms: i64,
    /// Total history shown in the left panel. 0 → no history panel.
    pub history_minutes: f64,
    /// Width of the right "now" panel in minutes.
    pub now_width_minutes: f64,
    /// Minutes of timeline drawn right of the divider, after `end_ms` — a
    /// band CENTRED on a pinned instant (da-coi0m, `datum now -t`),
    /// `history_minutes` either side and the clock on the divider.
    /// 0 → the solid now panel.
    pub after_minutes: f64,
    /// Total terminal width in cells.
    pub cols: u16,
    /// Emit ANSI 24-bit color escapes. False → pure shading characters.
    pub use_color: bool,
    /// "HH:MM" current-time label shown at the right of the now panel.
    pub current_time_label: String,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            end_ms: 0,
            now_ms: 0,
            history_minutes: 60.0,
            now_width_minutes: 5.0,
            after_minutes: 0.0,
            cols: 80,
            use_color: true,
            current_time_label: String::new(),
        }
    }
}

/// Render `fields` to a multi-line string. Returns the rendered text
/// without a trailing newline (caller adds one if desired).
pub fn render_nowview_terminal(fields: &[NowviewField], config: &TerminalConfig) -> String {
    let cols = config.cols.max(20) as usize;

    let mut sorted: Vec<&NowviewField> = fields.iter().collect();
    sorted.sort_by(|a, b| a.field.cmp(&b.field));
    let max_label_len = sorted
        .iter()
        .map(|f| unicode_width::UnicodeWidthStr::width(f.field.as_str()))
        .max()
        .unwrap_or(11);
    let max_label_cap = (cols / 3).max(6);
    let label_w = max_label_len.min(max_label_cap).max(1);
    let after = config.after_minutes > 0.0;

    let chrome = label_w + 1 + 1;
    let plot_w = cols.saturating_sub(chrome).max(2);

    let history_minutes = config.history_minutes.max(0.0);
    let now_width_minutes = if after {
        config.after_minutes
    } else {
        config.now_width_minutes.max(0.5)
    };

    let (history_cols, now_cols) = if history_minutes > 0.0 {
        let total = history_minutes + now_width_minutes;
        let mut nc = ((plot_w as f64) * (now_width_minutes / total)).round() as usize;
        nc = nc.clamp(1, plot_w.saturating_sub(1));
        (plot_w - nc, nc)
    } else {
        (0, plot_w)
    };

    let history_start_ms = config.end_ms - (history_minutes * 60_000.0) as i64;

    let mut out = String::with_capacity(cols * (sorted.len() + 3) * 4);

    for f in &sorted {
        push_padded_label(&mut out, &f.field, label_w);
        out.push(' ');
        if history_cols > 0 {
            push_history_row(
                &mut out,
                f,
                history_start_ms,
                config.end_ms,
                config.now_ms,
                history_cols,
                config.use_color,
            );
        }
        push_divider(&mut out, history_cols > 0);
        if after {
            let after_end_ms = config.end_ms + (config.after_minutes * 60_000.0) as i64;
            let (end, now) = (config.end_ms, config.now_ms);
            push_history_row(
                &mut out,
                f,
                end,
                after_end_ms,
                now,
                now_cols,
                config.use_color,
            );
        } else {
            push_now_row(&mut out, f, config.end_ms, now_cols, config.use_color);
        }
        out.push('\n');
    }

    if sorted.is_empty() {
        push_padded_label(&mut out, "(no fields)", label_w);
        out.push(' ');
        for _ in 0..plot_w {
            out.push(' ');
        }
        out.push('\n');
    }

    push_axis_border(&mut out, label_w, history_cols, now_cols);
    out.push('\n');
    push_axis_labels(
        &mut out,
        label_w,
        history_cols,
        now_cols,
        history_minutes,
        config.after_minutes,
        &config.current_time_label,
    );

    out
}

// ─── Shared low-level cells ────────────────────────────────────────────────

/// `label` in exactly `width` columns: padded, or cut with `…` when it
/// does not fit (da-coi0m) — by display width, so a wide glyph is dropped
/// rather than pushing the row a column right.
fn push_padded_label(out: &mut String, label: &str, width: usize) {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    let fits = label.width() <= width;
    let budget = if fits { width } else { width.saturating_sub(1) };
    let mut count = 0;
    for ch in label.chars() {
        let w = ch.width().unwrap_or(0);
        if count + w > budget {
            break;
        }
        out.push(ch);
        count += w;
    }
    if !fits && width > 0 {
        out.push('…');
        count += 1;
    }
    for _ in count..width {
        out.push(' ');
    }
}

fn push_divider(out: &mut String, has_history: bool) {
    if has_history {
        out.push('│');
    } else {
        out.push(' ');
    }
}

/// One timeline row over `[history_start_ms, end_ms]`; a cell that starts
/// at or after `now_ms` is blank — a band centred on an instant can run
/// past now, and what has not happened yet is not `░` (no state). A
/// `now_ms` of 0 (the config's default: no clock given) blanks nothing.
fn push_history_row(
    out: &mut String,
    field: &NowviewField,
    history_start_ms: i64,
    end_ms: i64,
    now_ms: i64,
    cols: usize,
    use_color: bool,
) {
    let span_ms = (end_ms - history_start_ms).max(1) as f64;
    let cell_ms = span_ms / cols as f64;
    for i in 0..cols {
        let cell_center_ms = history_start_ms + (cell_ms * (i as f64 + 0.5)) as i64;
        let cell_start_ms = history_start_ms + (cell_ms * i as f64) as i64;
        let cell_end_ms = history_start_ms + (cell_ms * (i + 1) as f64) as i64;
        if now_ms > 0 && cell_start_ms >= now_ms {
            out.push(' ');
            continue;
        }

        let mut block_colors: Option<&[String]> = None;
        for b in &field.blocks {
            if b.start_ms < cell_end_ms && b.end_ms > cell_start_ms {
                block_colors = Some(b.colors.as_slice());
            }
        }

        let point = field
            .points
            .iter()
            .find(|p| p.time_ms >= cell_start_ms && p.time_ms < cell_end_ms);

        match (point, block_colors) {
            (Some(p), _) => push_cell_point(out, p, use_color),
            (None, Some(cs)) => push_block_cell(out, cs, use_color),
            (None, None) => push_cell_empty(out),
        }

        let _ = cell_center_ms;
    }
}

fn push_now_row(out: &mut String, field: &NowviewField, end_ms: i64, cols: usize, use_color: bool) {
    // da-zwct: the now panel paints what is still RUNNING at `end_ms`, a
    // half-open test — a block that ends AT the instant is over, and
    // `datum start posture 4` writes exactly that block (the four
    // minutes up to now). Regions that have not ended are handed an end
    // past the window by the caller, so they still qualify.
    let active_colors: Option<&[String]> = field
        .blocks
        .iter()
        .filter(|b| b.start_ms <= end_ms && b.end_ms > end_ms)
        .max_by_key(|b| b.start_ms)
        .map(|b| b.colors.as_slice());

    let recent_point: Option<&NowviewPoint> = field
        .points
        .iter()
        .filter(|p| p.time_ms >= end_ms - 1_000 && p.time_ms <= end_ms)
        .max_by_key(|p| p.time_ms);

    for i in 0..cols {
        match (recent_point, active_colors) {
            (Some(pc), Some(cs)) => {
                if i + 1 == cols {
                    push_cell_point(out, pc, use_color);
                } else {
                    push_block_cell(out, cs, use_color);
                }
            }
            (Some(pc), None) => {
                if i + 1 == cols {
                    push_cell_point(out, pc, use_color);
                } else {
                    push_cell_empty(out);
                }
            }
            (None, Some(cs)) => push_block_cell(out, cs, use_color),
            (None, None) => push_cell_empty(out),
        }
    }
}

fn push_block_cell(out: &mut String, colors: &[String], use_color: bool) {
    match colors {
        [] => push_cell_empty(out),
        [hex] => push_cell_filled(out, hex, use_color),
        _ => push_cell_parallel(out, &colors[0], &colors[1], use_color),
    }
}

fn push_cell_filled(out: &mut String, hex: &str, use_color: bool) {
    if use_color {
        if let Some((r, g, b)) = parse_hex(hex) {
            out.push_str(&format!("\x1b[48;2;{r};{g};{b}m \x1b[0m"));
            return;
        }
    }
    out.push('█');
}

fn push_cell_parallel(out: &mut String, hex_bg: &str, hex_fg: &str, use_color: bool) {
    if use_color {
        if let (Some((br, bg, bb)), Some((fr, fg, fb))) = (parse_hex(hex_bg), parse_hex(hex_fg)) {
            out.push_str(&format!(
                "\x1b[48;2;{br};{bg};{bb}m\x1b[38;2;{fr};{fg};{fb}m▞\x1b[0m",
            ));
            return;
        }
    }
    out.push('▞');
}

fn push_cell_empty(out: &mut String) {
    out.push('░');
}

/// An occur dot's cell: its glyph (da-gzowt) when that is ONE column wide,
/// else `●`. Every cell of the band is one slice of time, so a two-column
/// glyph (most emoji) would push the rest of the row a column right; it
/// draws as the dot here and as itself in the SVG.
fn push_cell_point(out: &mut String, p: &NowviewPoint, use_color: bool) {
    let mark = p
        .glyph
        .as_deref()
        .filter(|g| unicode_width::UnicodeWidthStr::width(*g) == 1)
        .unwrap_or("●");
    if use_color {
        if let Some((r, g, b)) = parse_hex(&p.fg_color) {
            out.push_str(&format!("\x1b[38;2;{r};{g};{b}m{mark}\x1b[0m"));
            return;
        }
    }
    out.push_str(mark);
}

/// The band's floor, exactly as wide as a row above it (da-coi0m): `└`
/// under the gap after the label, `┴` under the divider `│`, `┘` under the
/// last cell.
fn push_axis_border(out: &mut String, label_w: usize, history_cols: usize, now_cols: usize) {
    for _ in 0..label_w {
        out.push(' ');
    }
    out.push('└');
    for _ in 0..history_cols {
        out.push('─');
    }
    out.push(if history_cols > 0 { '┴' } else { '─' });
    for _ in 1..now_cols {
        out.push('─');
    }
    out.push('┘');
}

fn push_axis_labels(
    out: &mut String,
    label_w: usize,
    history_cols: usize,
    now_cols: usize,
    history_minutes: f64,
    after_minutes: f64,
    current_time_label: &str,
) {
    // The row under the band's cells and divider, and no wider (da-coi0m).
    let row_w = history_cols + 1 + now_cols;
    let now_label: String = if current_time_label.is_empty() {
        "now".into()
    } else {
        current_time_label.to_string()
    };
    let mut row: Vec<char> = vec![' '; row_w];

    // The clock starts at the divider, or is centred on it when the band
    // runs on past the instant; `place_label` keeps it inside the row.
    place_label(&mut row, history_cols, &now_label, after_minutes > 0.0);

    if after_minutes > 0.0 {
        for m in pick_ticks(after_minutes) {
            let x = history_cols + (m / after_minutes * now_cols as f64).round() as usize;
            let label = format!("+{}", relative_label(m, tick_major(after_minutes)));
            place_label(&mut row, x, &label, true);
        }
    }

    if history_cols > 0 && history_minutes > 0.0 {
        let ticks = pick_ticks(history_minutes);
        for m in ticks {
            let frac = 1.0 - (m / history_minutes);
            let x = (frac * history_cols as f64).round() as i64;
            if x < 0 || x >= history_cols as i64 {
                continue;
            }
            let label = format!("-{}", relative_label(m, tick_major(history_minutes)));
            place_label(&mut row, x as usize, &label, true);
        }
    }

    for _ in 0..=label_w {
        out.push(' ');
    }
    let s: String = row.into_iter().collect();
    out.push_str(s.trim_end());
}

/// Place `text` at `at` (centred on it when `center`), pulled back inside
/// the row at either edge — whole, with a blank column either side of it,
/// or not at all (da-coi0m): a narrow band once wrote `-6-45-30m`.
fn place_label(row: &mut [char], at: usize, text: &str, center: bool) {
    let chars: Vec<char> = text.chars().collect();
    let Some(last_start) = row.len().checked_sub(chars.len()) else {
        return;
    };
    let start = if center {
        at.saturating_sub(chars.len() / 2)
    } else {
        at
    }
    .min(last_start);
    let end = start + chars.len();
    let clear = start.saturating_sub(1)..(end + 1).min(row.len());
    if row[clear].iter().any(|&c| c != ' ') {
        return;
    }
    row[start..end].copy_from_slice(&chars);
}

/// Major tick positions, in minutes from the instant (ascending), for an
/// axis spanning `span_minutes` on one side of it, by the window's length
/// alone, as `nowview::tick_config` chose before it read the pixels
/// (da-w4vfw) — only majors, so the row stays uncluttered. The clock anchors `0m`, so the list starts at the first
/// major.
fn pick_ticks(span_minutes: f64) -> Vec<f64> {
    let major = tick_major(span_minutes);
    let mut out = Vec::new();
    let mut m = major;
    while m <= span_minutes + 1e-9 {
        out.push(m);
        m += major;
    }
    out
}

/// The major step [`pick_ticks`] rules an axis spanning `span_minutes` at,
/// on past an hour for a window of days (da-wd6h5), and in seconds under
/// five minutes, down to 5s, so a window of 5s or more has a label
/// (da-6flnk).
fn tick_major(span_minutes: f64) -> f64 {
    [
        (10.0 / 60.0, 5.0 / 60.0),
        (0.5, 10.0 / 60.0),
        (2.0, 0.5),
        (5.0, 1.0),
        (15.0, 5.0),
        (60.0, 15.0),
        (180.0, 30.0),
        (720.0, 60.0),
        (2880.0, 360.0),
        (20160.0, 1440.0),
        (100800.0, 10080.0),
    ]
    .into_iter()
    .find(|(span, _)| span_minutes <= *span)
    .map_or(43200.0, |(_, major)| major)
}

pub(crate) fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some((r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nowview::{NowviewBlock, NowviewPoint};

    fn fields_one_block(now_ms: i64) -> Vec<NowviewField> {
        vec![NowviewField {
            field: "mood".into(),
            blocks: vec![NowviewBlock {
                start_ms: now_ms - 30 * 60_000,
                end_ms: now_ms,
                colors: vec!["#ff0000".into()],
                state_label: Some("happy".into()),
            }],
            points: vec![],
        }]
    }

    fn cfg(now_ms: i64) -> TerminalConfig {
        TerminalConfig {
            end_ms: now_ms,
            now_ms,
            history_minutes: 60.0,
            now_width_minutes: 5.0,
            after_minutes: 0.0,
            cols: 50,
            use_color: false,
            current_time_label: "12:34".into(),
        }
    }

    #[test]
    fn parse_hex_reads_six_digit() {
        assert_eq!(parse_hex("#ff0000"), Some((255, 0, 0)));
        assert_eq!(parse_hex("#00ff00"), Some((0, 255, 0)));
        assert_eq!(parse_hex("ABCDEF"), Some((0xAB, 0xCD, 0xEF)));
        assert_eq!(parse_hex("xyz"), None);
        assert_eq!(parse_hex("#abc"), None);
    }

    #[test]
    fn a_window_under_five_minutes_has_ticks_in_seconds() {
        let labels = |span: f64| -> Vec<String> {
            let major = tick_major(span);
            pick_ticks(span)
                .into_iter()
                .map(|m| relative_label(m, major))
                .collect()
        };
        assert_eq!(labels(10.0 / 60.0), ["5s", "10s"]);
        assert_eq!(labels(0.5), ["10s", "20s", "30s"]);
        assert_eq!(labels(2.0), ["30s", "60s", "90s", "120s"]);
        assert_eq!(labels(4.0), ["1m", "2m", "3m", "4m"]);
    }

    #[test]
    fn a_thirty_second_band_labels_its_axis() {
        let now = 1_700_000_000_000_i64;
        let config = TerminalConfig {
            history_minutes: 0.5,
            now_width_minutes: 0.1,
            ..cfg(now)
        };
        let s = render_nowview_terminal(&fields_one_block(now), &config);
        let axis = s.lines().last().unwrap();
        assert!(axis.contains("-30s"), "expected -30s in:\n{s}");
        assert!(axis.contains("-10s"), "expected -10s in:\n{s}");
    }

    // ─── InlineBand (existing) ────────────────────────────────────────────

    #[test]
    fn render_emits_one_row_per_field_plus_axis() {
        let now = 1_700_000_000_000_i64;
        let fields = vec![
            NowviewField {
                field: "coffee".into(),
                blocks: vec![],
                points: vec![],
            },
            NowviewField {
                field: "mood".into(),
                blocks: vec![],
                points: vec![],
            },
        ];
        let s = render_nowview_terminal(&fields, &cfg(now));
        let nl = s.matches('\n').count();
        assert_eq!(nl, 3, "expected 3 newlines, got {nl} in:\n{s}");
    }

    #[test]
    fn fields_render_alphabetically() {
        let now = 1_700_000_000_000_i64;
        let fields = vec![
            NowviewField {
                field: "workout".into(),
                blocks: vec![],
                points: vec![],
            },
            NowviewField {
                field: "alpha".into(),
                blocks: vec![],
                points: vec![],
            },
        ];
        let s = render_nowview_terminal(&fields, &cfg(now));
        let alpha_pos = s.find("alpha").unwrap();
        let workout_pos = s.find("workout").unwrap();
        assert!(alpha_pos < workout_pos);
    }

    #[test]
    fn ongoing_block_paints_now_panel_filled_chars() {
        let now = 1_700_000_000_000_i64;
        let s = render_nowview_terminal(&fields_one_block(now), &cfg(now));
        assert!(s.contains('█'), "expected filled cell in:\n{s}");
    }

    #[test]
    fn no_history_panel_when_history_zero() {
        let now = 1_700_000_000_000_i64;
        let mut c = cfg(now);
        c.history_minutes = 0.0;
        let fields = vec![NowviewField {
            field: "mood".into(),
            blocks: vec![],
            points: vec![],
        }];
        let s = render_nowview_terminal(&fields, &c);
        assert!(!s.contains('┴'), "did not expect history boundary in:\n{s}");
    }

    /// A config with no clock (`now_ms: 0`, the default) draws its history:
    /// only a real now blanks the cells past it (da-coi0m).
    #[test]
    fn zero_now_ms_blanks_no_history() {
        let now = 1_700_000_000_000_i64;
        let c = TerminalConfig {
            now_ms: 0,
            ..cfg(now)
        };
        let s = render_nowview_terminal(&fields_one_block(now), &c);
        assert!(s.contains('█'), "history drawn:\n{s}");
    }

    #[test]
    fn color_mode_emits_ansi_escapes() {
        let now = 1_700_000_000_000_i64;
        let mut c = cfg(now);
        c.use_color = true;
        let s = render_nowview_terminal(&fields_one_block(now), &c);
        assert!(
            s.contains("\x1b[48;2;255;0;0m"),
            "expected ansi bg red in:\n{s:?}"
        );
    }

    #[test]
    fn axis_labels_show_now_label() {
        let now = 1_700_000_000_000_i64;
        let s = render_nowview_terminal(&fields_one_block(now), &cfg(now));
        assert!(s.contains("12:34"), "expected current_time_label in:\n{s}");
    }

    #[test]
    fn point_marker_renders_dot() {
        let now = 1_700_000_000_000_i64;
        let fields = vec![NowviewField {
            field: "coffee".into(),
            blocks: vec![],
            points: vec![NowviewPoint {
                time_ms: now - 30 * 60_000,
                fg_color: "#00ff00".into(),
                state_label: None,
                glyph: None,
            }],
        }];
        let s = render_nowview_terminal(&fields, &cfg(now));
        assert!(s.contains('●'), "expected point dot in:\n{s}");
    }

    /// da-gzowt: the band draws a one-column glyph as itself, and a
    /// two-column one (an emoji) as `●` — so the row is exactly as wide as
    /// the unglyphed row and no column after the dot moves.
    #[test]
    fn point_glyph_draws_when_one_column_wide_and_a_wide_one_draws_the_dot() {
        let now = 1_700_000_000_000_i64;
        let band = |glyph: Option<&str>| {
            let fields = vec![NowviewField {
                field: "coffee".into(),
                blocks: vec![],
                points: vec![NowviewPoint {
                    time_ms: now - 30 * 60_000,
                    fg_color: "#00ff00".into(),
                    state_label: None,
                    glyph: glyph.map(str::to_string),
                }],
            }];
            render_nowview_terminal(&fields, &cfg(now))
        };
        let plain = band(None);
        let tri = band(Some("▲"));
        assert!(
            tri.contains('▲') && !tri.contains('●'),
            "narrow glyph drawn:\n{tri}"
        );
        assert_eq!(tri.replace('▲', "●"), plain, "only the mark changed");
        let wide = band(Some("☕"));
        assert_eq!(unicode_width::UnicodeWidthStr::width("☕"), 2);
        assert_eq!(wide, plain, "a two-column glyph draws as the dot:\n{wide}");
    }

    #[test]
    fn empty_fields_still_render_axis() {
        let now = 1_700_000_000_000_i64;
        let s = render_nowview_terminal(&[], &cfg(now));
        assert!(s.contains("(no fields)"));
        assert!(s.contains('└') && s.contains('┘'));
    }

    #[test]
    fn parallel_state_block_renders_split_glyph() {
        let now = 1_700_000_000_000_i64;
        let fields = vec![NowviewField {
            field: "body".into(),
            blocks: vec![NowviewBlock {
                start_ms: now - 30 * 60_000,
                end_ms: now,
                colors: vec!["#aa3333".into(), "#33aa33".into()],
                state_label: Some("vertical,sit".into()),
            }],
            points: vec![],
        }];
        let s = render_nowview_terminal(&fields, &cfg(now));
        assert!(
            s.contains('▞'),
            "expected parallel-state checker glyph in:\n{s}"
        );
        assert!(
            !s.contains('█'),
            "must not borrow single-state glyph for parallel block:\n{s}"
        );
    }

    #[test]
    fn parallel_state_block_ansi_carries_both_colours() {
        let now = 1_700_000_000_000_i64;
        let mut c = cfg(now);
        c.use_color = true;
        let fields = vec![NowviewField {
            field: "body".into(),
            blocks: vec![NowviewBlock {
                start_ms: now - 30 * 60_000,
                end_ms: now,
                colors: vec!["#ff0000".into(), "#00ff00".into()],
                state_label: None,
            }],
            points: vec![],
        }];
        let s = render_nowview_terminal(&fields, &c);
        assert!(s.contains("\x1b[48;2;255;0;0m"), "bg = first state: {s:?}");
        assert!(s.contains("\x1b[38;2;0;255;0m"), "fg = second state: {s:?}");
        assert!(s.contains('▞'), "checker glyph between bg+fg: {s:?}");
    }
}
