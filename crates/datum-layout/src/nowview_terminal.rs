//! Terminal renderer for [`NowviewField`] data — produces a string of ANSI
//! 24-bit-color Unicode characters approximating the SVG nowview.
//!
//! Layout is computed inline (cells, not pixels) because terminal cells
//! don't map cleanly onto SVG pixel widths. Inputs are the same per-field
//! `NowviewField` slice the SVG path consumes; outputs are a multi-line
//! string ready to `println!`.
//!
//! Visual model:
//!
//! ```text
//! coffee  ░░░░░░░░██░░░░░░░│██
//! mood    █████████████████│██
//! workout ░░░░░░░░░░░░░░░░░│
//!         └────────────────┘
//!         -30m    -15m   now
//! ```
//!
//! Pure: no I/O, no system deps. Compiles to `wasm32-unknown-unknown`.
//! ANSI escapes are embedded as plain UTF-8 — callers are responsible for
//! deciding whether the destination understands them.
//!
//! When `use_color` is `false`, the output uses `█`/`░`/`│` shading only —
//! safe for `--no-color` mode, redirected stdout, or NO_COLOR-aware callers.

use crate::nowview::NowviewField;

/// Inputs to [`render_nowview_terminal`].
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    /// "Now" — UTC epoch ms. End of the visible window.
    pub end_ms: i64,
    /// Total history shown in the left panel. 0 → no history panel.
    pub history_minutes: f64,
    /// Width of the right "now" panel in minutes.
    pub now_width_minutes: f64,
    /// Total terminal width in cells.
    pub cols: u16,
    /// Emit ANSI 24-bit color escapes. False → pure shading characters.
    pub use_color: bool,
    /// "HH:MM" current-time label shown under the `now` boundary.
    pub current_time_label: String,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            end_ms: 0,
            history_minutes: 60.0,
            now_width_minutes: 5.0,
            cols: 80,
            use_color: true,
            current_time_label: String::new(),
        }
    }
}

/// Render `fields` to a multi-line string. Fields are sorted alphabetically
/// to match the SVG path. Returns the rendered text without a trailing
/// newline (caller adds one if desired).
pub fn render_nowview_terminal(fields: &[NowviewField], config: &TerminalConfig) -> String {
    let cols = config.cols.max(20) as usize;

    // Sort & label width. Cap label width so panels still get useful space.
    let mut sorted: Vec<&NowviewField> = fields.iter().collect();
    sorted.sort_by(|a, b| a.field.cmp(&b.field));
    let max_label_len = sorted
        .iter()
        .map(|f| f.field.chars().count())
        .max()
        .unwrap_or(11); // matches "(no fields)" placeholder
    let max_label_cap = (cols / 3).max(6);
    let label_w = max_label_len.min(max_label_cap).max(1);

    // Layout: [label][space][history_cells][now-divider][now_cells]
    // Reserve 1 cell each for the post-label space and the now divider.
    let chrome = label_w + 1 + 1;
    let plot_w = cols.saturating_sub(chrome).max(2);

    let history_minutes = config.history_minutes.max(0.0);
    let now_width_minutes = config.now_width_minutes.max(0.5);

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

    // Field rows.
    for f in &sorted {
        push_padded_label(&mut out, &f.field, label_w);
        out.push(' ');
        if history_cols > 0 {
            push_history_row(
                &mut out,
                f,
                history_start_ms,
                config.end_ms,
                history_cols,
                config.use_color,
            );
        }
        push_divider(&mut out, history_cols > 0, config.use_color);
        push_now_row(&mut out, f, config.end_ms, now_cols, config.use_color);
        out.push('\n');
    }

    if sorted.is_empty() {
        // Empty view: still render the chrome so the user sees something.
        push_padded_label(&mut out, "(no fields)", label_w);
        out.push(' ');
        for _ in 0..plot_w {
            out.push(' ');
        }
        out.push('\n');
    }

    // Axis row: bottom border + tick labels.
    push_axis_border(&mut out, label_w, history_cols, now_cols);
    out.push('\n');
    push_axis_labels(
        &mut out,
        label_w,
        history_cols,
        now_cols,
        history_minutes,
        &config.current_time_label,
    );

    out
}

/// Pad/truncate `label` to exactly `width` cells (chars are assumed ASCII;
/// non-ASCII degrades to char-count which is good-enough for field names).
fn push_padded_label(out: &mut String, label: &str, width: usize) {
    if width == 0 {
        return;
    }
    let mut count = 0;
    for ch in label.chars().take(width) {
        out.push(ch);
        count += 1;
    }
    for _ in count..width {
        out.push(' ');
    }
}

/// Render the divider between history and now panels.
fn push_divider(out: &mut String, has_history: bool, _use_color: bool) {
    if has_history {
        out.push('│');
    } else {
        // No history panel — still emit a leading marker so the row
        // visually starts at a consistent column.
        out.push(' ');
    }
}

/// Render the history-panel cells for one field.
fn push_history_row(
    out: &mut String,
    field: &NowviewField,
    history_start_ms: i64,
    end_ms: i64,
    cols: usize,
    use_color: bool,
) {
    let span_ms = (end_ms - history_start_ms).max(1) as f64;
    let cell_ms = span_ms / cols as f64;
    for i in 0..cols {
        let cell_center_ms = history_start_ms + (cell_ms * (i as f64 + 0.5)) as i64;
        let cell_start_ms = history_start_ms + (cell_ms * i as f64) as i64;
        let cell_end_ms = history_start_ms + (cell_ms * (i + 1) as f64) as i64;

        // Pick the "freshest" block covering this cell. Most recent
        // start_ms wins, matching nowview's draw order (later overwrites).
        let mut color: Option<&str> = None;
        for b in &field.blocks {
            if b.start_ms < cell_end_ms && b.end_ms > cell_start_ms {
                match color {
                    None => color = Some(&b.color),
                    Some(_) => color = Some(&b.color),
                }
            }
        }

        // Points override the block fill if any point falls in this cell.
        let point_color = field
            .points
            .iter()
            .find(|p| p.time_ms >= cell_start_ms && p.time_ms < cell_end_ms)
            .map(|p| p.fg_color.as_str());

        match (point_color, color) {
            (Some(pc), _) => push_cell_point(out, pc, use_color),
            (None, Some(c)) => push_cell_filled(out, c, use_color),
            (None, None) => push_cell_empty(out, use_color),
        }

        // Suppress unused warning when cell_center_ms is computed for
        // future per-cell scoring rules. (Kept as documentation of intent.)
        let _ = cell_center_ms;
    }
}

/// Render the now-panel cells for one field. Paints all `cols` with the
/// color of the block ongoing at `end_ms`, or a point at `end_ms` if any.
fn push_now_row(out: &mut String, field: &NowviewField, end_ms: i64, cols: usize, use_color: bool) {
    let active_color: Option<&str> = field
        .blocks
        .iter()
        .filter(|b| b.start_ms <= end_ms && b.end_ms >= end_ms)
        .max_by_key(|b| b.start_ms)
        .map(|b| b.color.as_str());

    // A point in the very last second counts as "current".
    let recent_point: Option<&str> = field
        .points
        .iter()
        .filter(|p| p.time_ms >= end_ms - 1_000 && p.time_ms <= end_ms)
        .max_by_key(|p| p.time_ms)
        .map(|p| p.fg_color.as_str());

    for i in 0..cols {
        match (recent_point, active_color) {
            // Paint the point as the right-most cell so it visually
            // aligns with "now"; rest is the active block fill.
            (Some(pc), Some(bc)) => {
                if i + 1 == cols {
                    push_cell_point(out, pc, use_color);
                } else {
                    push_cell_filled(out, bc, use_color);
                }
            }
            (Some(pc), None) => {
                if i + 1 == cols {
                    push_cell_point(out, pc, use_color);
                } else {
                    push_cell_empty(out, use_color);
                }
            }
            (None, Some(bc)) => push_cell_filled(out, bc, use_color),
            (None, None) => push_cell_empty(out, use_color),
        }
    }
}

/// Solid-filled cell (used for blocks).
fn push_cell_filled(out: &mut String, hex: &str, use_color: bool) {
    if use_color {
        if let Some((r, g, b)) = parse_hex(hex) {
            out.push_str(&format!("\x1b[48;2;{r};{g};{b}m \x1b[0m"));
            return;
        }
    }
    out.push('█');
}

/// Empty cell (no data).
fn push_cell_empty(out: &mut String, _use_color: bool) {
    out.push('░');
}

/// Point/dot cell (used for occur markers and the last-instant point).
fn push_cell_point(out: &mut String, hex: &str, use_color: bool) {
    if use_color {
        if let Some((r, g, b)) = parse_hex(hex) {
            out.push_str(&format!("\x1b[38;2;{r};{g};{b}m●\x1b[0m"));
            return;
        }
    }
    out.push('●');
}

/// Bottom border under both panels (`└────┘` style).
fn push_axis_border(out: &mut String, label_w: usize, history_cols: usize, now_cols: usize) {
    for _ in 0..label_w {
        out.push(' ');
    }
    out.push(' ');
    if history_cols > 0 {
        out.push('└');
        for _ in 0..history_cols {
            out.push('─');
        }
        out.push('┴');
    } else {
        out.push('└');
    }
    for _ in 0..now_cols {
        out.push('─');
    }
    out.push('┘');
}

/// Render axis labels: `-Nm` ticks under the history panel, `now`/`HH:MM`
/// under the now boundary.
fn push_axis_labels(
    out: &mut String,
    label_w: usize,
    history_cols: usize,
    now_cols: usize,
    history_minutes: f64,
    current_time_label: &str,
) {
    // Build a tick string sized to the panel plus extra slack so the
    // `now`/clock label can extend past the right edge if needed.
    let plot_w = history_cols + 1 + now_cols; // +1 for the boundary char
    let now_label: String = if current_time_label.is_empty() {
        "now".into()
    } else {
        current_time_label.to_string()
    };
    let row_w = plot_w + now_label.chars().count();
    let mut row: Vec<char> = vec![' '; row_w];

    // Clock/now label anchored at the boundary, extending right.
    let now_anchor = history_cols;
    place_label(&mut row, now_anchor, &now_label, /*center=*/ false);

    // History tick labels: pick a few sensible positions.
    if history_cols > 0 && history_minutes > 0.0 {
        let ticks = pick_ticks(history_minutes);
        for m in ticks {
            // m minutes before "now" → x position in history panel (cell idx).
            let frac = 1.0 - (m / history_minutes);
            let x = (frac * history_cols as f64).round() as i64;
            if x < 0 || x >= history_cols as i64 {
                continue;
            }
            let label = format!("-{}m", m as i64);
            place_label(&mut row, x as usize, &label, /*center=*/ true);
        }
    }

    for _ in 0..label_w {
        out.push(' ');
    }
    out.push(' ');
    let s: String = row.into_iter().collect();
    out.push_str(s.trim_end());
}

/// Place `text` into `row` at column `at`, centered if requested. Avoids
/// overwriting non-space characters (so earlier-placed labels win).
fn place_label(row: &mut [char], at: usize, text: &str, center: bool) {
    let chars: Vec<char> = text.chars().collect();
    let start = if center {
        let half = chars.len() / 2;
        at.saturating_sub(half)
    } else {
        at
    };
    for (i, ch) in chars.iter().enumerate() {
        let pos = start + i;
        if pos >= row.len() {
            break;
        }
        if row[pos] != ' ' {
            return;
        }
        row[pos] = *ch;
    }
}

/// Pick tick spacings (in minutes-ago, ascending) for the history panel.
/// Mirrors `nowview::tick_config` but only emits major ticks.
fn pick_ticks(history_minutes: f64) -> Vec<f64> {
    let major = if history_minutes <= 15.0 {
        5.0
    } else if history_minutes <= 60.0 {
        15.0
    } else if history_minutes <= 180.0 {
        30.0
    } else {
        60.0
    };
    let mut out = Vec::new();
    let mut m = major;
    while m <= history_minutes + 1e-9 {
        out.push(m);
        m += major;
    }
    out
}

/// Parse a `#rrggbb` hex string into an RGB triple. Returns `None` for
/// invalid input (caller falls back to monochrome output).
fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
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
                end_ms: now_ms, // ongoing
                color: "#ff0000".into(),
                state_label: Some("happy".into()),
            }],
            points: vec![],
        }]
    }

    fn cfg(now_ms: i64) -> TerminalConfig {
        TerminalConfig {
            end_ms: now_ms,
            history_minutes: 60.0,
            now_width_minutes: 5.0,
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
        assert_eq!(parse_hex("#abc"), None); // shorthand not supported
    }

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
        // 2 field rows + 1 border row + 1 label row = 4 lines (3 newlines).
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
        // Without color, an ongoing block produces `█` in the now panel.
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
        // No history panel → no `┴` boundary char in the border row.
        assert!(!s.contains('┴'), "did not expect history boundary in:\n{s}");
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
                bg_color: "#000000".into(),
                state_label: None,
            }],
        }];
        let s = render_nowview_terminal(&fields, &cfg(now));
        assert!(s.contains('●'), "expected point dot in:\n{s}");
    }

    #[test]
    fn empty_fields_still_render_axis() {
        let now = 1_700_000_000_000_i64;
        let s = render_nowview_terminal(&[], &cfg(now));
        assert!(s.contains("(no fields)"));
        assert!(s.contains('└') && s.contains('┘'));
    }
}
