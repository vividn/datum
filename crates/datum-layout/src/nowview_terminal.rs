//! Terminal renderer for [`NowviewField`] data.
//!
//! Three modes share this entry point, selected via [`RenderMode`]:
//!
//! - **`InlineBand`** — single-row labeled bands with `└────┘` axis chrome.
//!   Used by `datum now`'s nowview band beneath the active-states list.
//!   Visual model (per-row):
//!
//!   ```text
//!   coffee  ░░░░░░░░██░░░░░░░│██
//!   mood    █████████████████│██
//!           └────────────────┘
//!           -30m    -15m   now
//!   ```
//!
//! - **`Snapshot`** — multi-row, pure-color band stack of each tracked
//!   field's CURRENT state at "now". No labels of any kind. A single
//!   centered `hh:mm` footer row. Reference: `~/tmp/nowview_ex1.png`. Used
//!   by `dm nowview` (no time window).
//!
//! - **`Flow`** — multi-row, pure-color band stack spanning
//!   `[now - history, now]`, with a dashed-yellow now-line at the right
//!   edge and a single axis row of `-Nm` / `hh:mm` labels. Reference:
//!   `~/tmp/nowview_ex2.png`. Used by `dm nowview --window <duration>`.
//!
//! Pure: no I/O, no system deps. Compiles to `wasm32-unknown-unknown`.
//! ANSI escapes are embedded as plain UTF-8 — callers decide whether the
//! destination understands them.

use crate::nowview::NowviewField;

/// Selects the layout used by [`render_nowview_terminal`]. See the module
/// docs for the visual differences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderMode {
    /// Single-row labeled bands with axis chrome. Default for backwards
    /// compatibility with `datum now`'s band.
    #[default]
    InlineBand,
    /// Multi-row band stack, no labels, `hh:mm` footer. Vertical snapshot
    /// of each field's current state.
    Snapshot,
    /// Multi-row band stack spanning the history window, dashed
    /// now-marker at the right edge, axis-tick labels below.
    Flow,
}

/// Inputs to [`render_nowview_terminal`].
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    /// "Now" — UTC epoch ms. End of the visible window.
    pub end_ms: i64,
    /// Total history shown in the left panel. 0 → no history panel (or no
    /// history axis, for [`RenderMode::Snapshot`]).
    pub history_minutes: f64,
    /// Width of the right "now" panel in minutes. Only meaningful for
    /// [`RenderMode::InlineBand`].
    pub now_width_minutes: f64,
    /// Total terminal width in cells.
    pub cols: u16,
    /// Total terminal height in rows. Used by [`RenderMode::Snapshot`] /
    /// [`RenderMode::Flow`] to size the multi-row band stack. Ignored by
    /// `InlineBand` (which is fixed at one row per field plus axis).
    pub rows: u16,
    /// Emit ANSI 24-bit color escapes. False → pure shading characters.
    pub use_color: bool,
    /// "HH:MM" current-time label shown at the right of the now panel.
    pub current_time_label: String,
    /// Layout mode. See [`RenderMode`].
    pub mode: RenderMode,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            end_ms: 0,
            history_minutes: 60.0,
            now_width_minutes: 5.0,
            cols: 80,
            rows: 24,
            use_color: true,
            current_time_label: String::new(),
            mode: RenderMode::default(),
        }
    }
}

/// Render `fields` to a multi-line string. Returns the rendered text
/// without a trailing newline (caller adds one if desired).
pub fn render_nowview_terminal(fields: &[NowviewField], config: &TerminalConfig) -> String {
    match config.mode {
        RenderMode::InlineBand => render_inline_band(fields, config),
        RenderMode::Snapshot => render_visual(fields, config, /* with_history = */ false),
        RenderMode::Flow => render_visual(fields, config, /* with_history = */ true),
    }
}

// ─── InlineBand (legacy `datum now` band) ──────────────────────────────────

fn render_inline_band(fields: &[NowviewField], config: &TerminalConfig) -> String {
    let cols = config.cols.max(20) as usize;

    let mut sorted: Vec<&NowviewField> = fields.iter().collect();
    sorted.sort_by(|a, b| a.field.cmp(&b.field));
    let max_label_len = sorted
        .iter()
        .map(|f| f.field.chars().count())
        .max()
        .unwrap_or(11);
    let max_label_cap = (cols / 3).max(6);
    let label_w = max_label_len.min(max_label_cap).max(1);

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
        push_divider(&mut out, history_cols > 0);
        push_now_row(&mut out, f, config.end_ms, now_cols, config.use_color);
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
        &config.current_time_label,
    );

    out
}

// ─── Snapshot / Flow (visual band stack — `dm nowview`) ───────────────────

/// Hex color of the dashed yellow now-line in Flow mode. Matches the SVG
/// nowview's `current-time-line` stroke for visual parity across surfaces.
const NOW_LINE_HEX: &str = "#ffcc00";

/// Render a multi-row band stack. When `with_history` is false the bands
/// are painted with each field's CURRENT state across all columns (Snapshot
/// mode); when true, each column samples the block ongoing at that point
/// in time and the right edge gets a dashed yellow now-marker (Flow mode).
fn render_visual(fields: &[NowviewField], config: &TerminalConfig, with_history: bool) -> String {
    let cols = config.cols.max(4) as usize;
    let total_rows = config.rows.max(3) as usize;

    // One row reserved for the footer (hh:mm in Snapshot, axis labels in
    // Flow). Everything else is the data area.
    let data_rows = total_rows.saturating_sub(1).max(1);

    let mut sorted: Vec<&NowviewField> = fields.iter().collect();
    sorted.sort_by(|a, b| {
        a.y_frac
            .partial_cmp(&b.y_frac)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.z_index
                    .partial_cmp(&b.z_index)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.field.cmp(&b.field))
    });

    let band_rows = distribute_rows(&sorted, data_rows);

    let mut out = String::with_capacity(cols * total_rows * 4);

    let history_minutes = config.history_minutes.max(0.0);
    let history_start_ms = config.end_ms - (history_minutes * 60_000.0) as i64;

    for (field, rows_for_field) in sorted.iter().zip(band_rows.iter()) {
        for _ in 0..*rows_for_field {
            if with_history && history_minutes > 0.0 {
                push_flow_data_row(
                    &mut out,
                    field,
                    history_start_ms,
                    config.end_ms,
                    cols,
                    config.use_color,
                );
            } else {
                push_snapshot_data_row(&mut out, field, config.end_ms, cols, config.use_color);
            }
            out.push('\n');
        }
    }

    // Pad any unused data rows with blanks so the footer always lands on
    // the bottom row even when the band heights don't cover the budget.
    let used_data_rows: usize = band_rows.iter().sum();
    for _ in used_data_rows..data_rows {
        for _ in 0..cols {
            out.push(' ');
        }
        out.push('\n');
    }

    // Footer.
    if with_history && history_minutes > 0.0 {
        push_flow_axis_labels(&mut out, cols, history_minutes, &config.current_time_label);
    } else {
        push_snapshot_footer(&mut out, cols, &config.current_time_label);
    }

    out
}

/// Allocate `data_rows` across `fields` proportional to `height_frac`,
/// guaranteeing each visible field at least one row when budget allows.
/// When `height_frac` sums to zero, the rows are spread evenly.
fn distribute_rows(fields: &[&NowviewField], data_rows: usize) -> Vec<usize> {
    let n = fields.len();
    if n == 0 || data_rows == 0 {
        return vec![0; n];
    }

    let total: f64 = fields.iter().map(|f| f.height_frac.max(0.0)).sum();

    if total <= 0.0 {
        let base = data_rows / n;
        let mut leftover = data_rows - base * n;
        let mut out = vec![base; n];
        for slot in out.iter_mut() {
            if leftover == 0 {
                break;
            }
            *slot += 1;
            leftover -= 1;
        }
        return out;
    }

    let mut raw: Vec<f64> = fields
        .iter()
        .map(|f| (f.height_frac.max(0.0) / total) * data_rows as f64)
        .collect();
    let mut out: Vec<usize> = raw.iter().map(|x| x.floor() as usize).collect();
    let mut assigned: usize = out.iter().sum();

    // Ensure each field gets at least one row when there's any budget,
    // pulling from the most-over-allocated fields.
    if data_rows >= n {
        for idx in 0..n {
            if out[idx] == 0 {
                let mut donor = 0usize;
                let mut donor_excess = -1.0_f64;
                for j in 0..n {
                    if j == idx {
                        continue;
                    }
                    let excess = raw[j] - out[j] as f64;
                    if out[j] > 1 && excess > donor_excess {
                        donor = j;
                        donor_excess = excess;
                    }
                }
                if donor_excess >= 0.0 {
                    out[donor] -= 1;
                    out[idx] = 1;
                    raw[idx] = 1.0;
                }
            }
        }
        assigned = out.iter().sum();
    }

    // Distribute the remaining rows by the largest fractional remainder.
    while assigned < data_rows {
        let mut best = 0usize;
        let mut best_rem = -1.0_f64;
        for idx in 0..n {
            let rem = raw[idx] - out[idx] as f64;
            if rem > best_rem {
                best = idx;
                best_rem = rem;
            }
        }
        out[best] += 1;
        assigned += 1;
    }

    out
}

fn push_snapshot_data_row(
    out: &mut String,
    field: &NowviewField,
    end_ms: i64,
    cols: usize,
    use_color: bool,
) {
    let colors = active_colors(field, end_ms);
    for _ in 0..cols {
        push_visual_cell(out, colors, use_color);
    }
}

fn push_flow_data_row(
    out: &mut String,
    field: &NowviewField,
    history_start_ms: i64,
    end_ms: i64,
    cols: usize,
    use_color: bool,
) {
    let span_ms = (end_ms - history_start_ms).max(1) as f64;
    let cell_ms = span_ms / cols as f64;
    let last_col = cols.saturating_sub(1);

    for i in 0..cols {
        // Right-most column always carries the now-line (overwrites any
        // band fill so the user sees a continuous yellow stripe).
        if i == last_col {
            push_now_line_cell(out, use_color);
            continue;
        }

        let cell_center_ms = history_start_ms + (cell_ms * (i as f64 + 0.5)) as i64;
        let cell_start_ms = history_start_ms + (cell_ms * i as f64) as i64;
        let cell_end_ms = history_start_ms + (cell_ms * (i + 1) as f64) as i64;

        let mut block_colors: Option<&[String]> = None;
        for b in &field.blocks {
            if b.start_ms < cell_end_ms && b.end_ms > cell_start_ms {
                block_colors = Some(b.colors.as_slice());
            }
        }
        let colors: &[String] = block_colors.unwrap_or(field.default_colors.as_slice());
        push_visual_cell(out, colors, use_color);
        let _ = cell_center_ms;
    }
}

/// Resolve the color(s) painted for one snapshot/flow cell. Returns the
/// block-ongoing slice when one covers `end_ms`, else the field's spec
/// default (already populated by the CLI). Empty → caller draws the
/// shading fallback.
fn active_colors(field: &NowviewField, end_ms: i64) -> &[String] {
    let ongoing = field
        .blocks
        .iter()
        .filter(|b| b.start_ms <= end_ms && b.end_ms >= end_ms)
        .max_by_key(|b| b.start_ms);
    match ongoing {
        Some(b) => b.colors.as_slice(),
        None => field.default_colors.as_slice(),
    }
}

fn push_visual_cell(out: &mut String, colors: &[String], use_color: bool) {
    match colors {
        [] => push_cell_empty(out),
        [hex] => push_cell_filled(out, hex, use_color),
        _ => push_cell_parallel(out, &colors[0], &colors[1], use_color),
    }
}

/// Dashed-yellow now-line cell. ANSI mode paints a `┊` glyph in
/// [`NOW_LINE_HEX`]; monochrome mode falls back to a bare `┊`.
fn push_now_line_cell(out: &mut String, use_color: bool) {
    if use_color {
        if let Some((r, g, b)) = parse_hex(NOW_LINE_HEX) {
            out.push_str(&format!("\x1b[38;2;{r};{g};{b}m┊\x1b[0m"));
            return;
        }
    }
    out.push('┊');
}

/// Centered `hh:mm` footer for Snapshot mode. The label is the only
/// non-blank content on the row.
fn push_snapshot_footer(out: &mut String, cols: usize, label: &str) {
    let label_chars: Vec<char> = label.chars().collect();
    let n = label_chars.len().min(cols);
    let start = (cols.saturating_sub(n)) / 2;
    for i in 0..cols {
        if i >= start && i < start + n {
            out.push(label_chars[i - start]);
        } else {
            out.push(' ');
        }
    }
}

/// Axis-tick row for Flow mode: `-Nm` labels under the appropriate
/// fractional positions plus the `hh:mm` (or `now` fallback) anchored at
/// the right edge.
fn push_flow_axis_labels(out: &mut String, cols: usize, history_minutes: f64, time_label: &str) {
    let mut row: Vec<char> = vec![' '; cols];
    let last_col = cols.saturating_sub(1);

    // Right-edge `hh:mm` (or `now` fallback). Anchor by its right edge so
    // the colon lines up with the now-line column above.
    let time_label = if time_label.is_empty() {
        "now"
    } else {
        time_label
    };
    let time_chars: Vec<char> = time_label.chars().collect();
    let time_len = time_chars.len();
    if time_len > 0 && time_len <= cols {
        let start = (last_col + 1).saturating_sub(time_len);
        for (k, ch) in time_chars.iter().enumerate() {
            row[start + k] = *ch;
        }
    }

    // `-Nm` ticks. Place from longest-history → shortest so the leftmost
    // (`-30m`) wins ties at the edge.
    for m in flow_axis_ticks(history_minutes) {
        let frac = 1.0 - (m / history_minutes.max(1e-6));
        let x = (frac * last_col as f64).round() as i64;
        if x < 0 || x as usize > last_col {
            continue;
        }
        let label = format!("-{}m", m as i64);
        // Center on `x` but bias right so the label doesn't overlap a
        // tick further left.
        let label_chars: Vec<char> = label.chars().collect();
        let half = label_chars.len() / 2;
        let start = (x as usize).saturating_sub(half);
        place_chars_skip_filled(&mut row, start, &label_chars);
    }

    for ch in row {
        out.push(ch);
    }
}

/// Tick positions in minutes-ago (ascending) for Flow's axis labels.
/// Mirrors `nowview::tick_config`'s major spacing — we only emit majors so
/// the row stays uncluttered. The right-edge clock anchors `0m`, so the
/// list starts at the first major.
fn flow_axis_ticks(history_minutes: f64) -> Vec<f64> {
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

fn place_chars_skip_filled(row: &mut [char], start: usize, chars: &[char]) {
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

// ─── Shared low-level cells ────────────────────────────────────────────────

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

fn push_divider(out: &mut String, has_history: bool) {
    if has_history {
        out.push('│');
    } else {
        out.push(' ');
    }
}

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

        let mut block_colors: Option<&[String]> = None;
        for b in &field.blocks {
            if b.start_ms < cell_end_ms && b.end_ms > cell_start_ms {
                block_colors = Some(b.colors.as_slice());
            }
        }

        let point_color = field
            .points
            .iter()
            .find(|p| p.time_ms >= cell_start_ms && p.time_ms < cell_end_ms)
            .map(|p| p.fg_color.as_str());

        match (point_color, block_colors) {
            (Some(pc), _) => push_cell_point(out, pc, use_color),
            (None, Some(cs)) => push_block_cell(out, cs, use_color),
            (None, None) => push_cell_empty(out),
        }

        let _ = cell_center_ms;
    }
}

fn push_now_row(out: &mut String, field: &NowviewField, end_ms: i64, cols: usize, use_color: bool) {
    let active_colors: Option<&[String]> = field
        .blocks
        .iter()
        .filter(|b| b.start_ms <= end_ms && b.end_ms >= end_ms)
        .max_by_key(|b| b.start_ms)
        .map(|b| b.colors.as_slice());

    let recent_point: Option<&str> = field
        .points
        .iter()
        .filter(|p| p.time_ms >= end_ms - 1_000 && p.time_ms <= end_ms)
        .max_by_key(|p| p.time_ms)
        .map(|p| p.fg_color.as_str());

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

fn push_cell_point(out: &mut String, hex: &str, use_color: bool) {
    if use_color {
        if let Some((r, g, b)) = parse_hex(hex) {
            out.push_str(&format!("\x1b[38;2;{r};{g};{b}m●\x1b[0m"));
            return;
        }
    }
    out.push('●');
}

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

fn push_axis_labels(
    out: &mut String,
    label_w: usize,
    history_cols: usize,
    now_cols: usize,
    history_minutes: f64,
    current_time_label: &str,
) {
    let plot_w = history_cols + 1 + now_cols;
    let now_label: String = if current_time_label.is_empty() {
        "now".into()
    } else {
        current_time_label.to_string()
    };
    let row_w = plot_w + now_label.chars().count();
    let mut row: Vec<char> = vec![' '; row_w];

    let now_anchor = history_cols;
    place_label(&mut row, now_anchor, &now_label, false);

    if history_cols > 0 && history_minutes > 0.0 {
        let ticks = pick_ticks(history_minutes);
        for m in ticks {
            let frac = 1.0 - (m / history_minutes);
            let x = (frac * history_cols as f64).round() as i64;
            if x < 0 || x >= history_cols as i64 {
                continue;
            }
            let label = format!("-{}m", m as i64);
            place_label(&mut row, x as usize, &label, true);
        }
    }

    for _ in 0..label_w {
        out.push(' ');
    }
    out.push(' ');
    let s: String = row.into_iter().collect();
    out.push_str(s.trim_end());
}

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
                end_ms: now_ms,
                colors: vec!["#ff0000".into()],
                state_label: Some("happy".into()),
            }],
            points: vec![],
            y_frac: 0.0,
            height_frac: 0.0,
            z_index: 0.0,
            default_colors: vec![],
        }]
    }

    fn cfg(now_ms: i64) -> TerminalConfig {
        TerminalConfig {
            end_ms: now_ms,
            history_minutes: 60.0,
            now_width_minutes: 5.0,
            cols: 50,
            rows: 12,
            use_color: false,
            current_time_label: "12:34".into(),
            mode: RenderMode::InlineBand,
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

    // ─── InlineBand (existing) ────────────────────────────────────────────

    #[test]
    fn render_emits_one_row_per_field_plus_axis() {
        let now = 1_700_000_000_000_i64;
        let fields = vec![
            NowviewField {
                field: "coffee".into(),
                blocks: vec![],
                points: vec![],
                y_frac: 0.0,
                height_frac: 0.0,
                z_index: 0.0,
                default_colors: vec![],
            },
            NowviewField {
                field: "mood".into(),
                blocks: vec![],
                points: vec![],
                y_frac: 0.0,
                height_frac: 0.0,
                z_index: 0.0,
                default_colors: vec![],
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
                y_frac: 0.0,
                height_frac: 0.0,
                z_index: 0.0,
                default_colors: vec![],
            },
            NowviewField {
                field: "alpha".into(),
                blocks: vec![],
                points: vec![],
                y_frac: 0.0,
                height_frac: 0.0,
                z_index: 0.0,
                default_colors: vec![],
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
            y_frac: 0.0,
            height_frac: 0.0,
            z_index: 0.0,
            default_colors: vec![],
        }];
        let s = render_nowview_terminal(&fields, &c);
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
            y_frac: 0.0,
            height_frac: 0.0,
            z_index: 0.0,
            default_colors: vec![],
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
            y_frac: 0.0,
            height_frac: 0.0,
            z_index: 0.0,
            default_colors: vec![],
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
            y_frac: 0.0,
            height_frac: 0.0,
            z_index: 0.0,
            default_colors: vec![],
        }];
        let s = render_nowview_terminal(&fields, &c);
        assert!(s.contains("\x1b[48;2;255;0;0m"), "bg = first state: {s:?}");
        assert!(s.contains("\x1b[38;2;0;255;0m"), "fg = second state: {s:?}");
        assert!(s.contains('▞'), "checker glyph between bg+fg: {s:?}");
    }

    // ─── Snapshot ─────────────────────────────────────────────────────────

    fn snap_cfg(now_ms: i64) -> TerminalConfig {
        TerminalConfig {
            end_ms: now_ms,
            history_minutes: 0.0,
            now_width_minutes: 0.0,
            cols: 20,
            rows: 11,
            use_color: false,
            current_time_label: "21:11".into(),
            mode: RenderMode::Snapshot,
        }
    }

    #[test]
    fn snapshot_drops_field_and_state_labels() {
        let now = 1_700_000_000_000_i64;
        let fields = vec![
            NowviewField {
                field: "mood".into(),
                blocks: vec![NowviewBlock {
                    start_ms: now - 60_000,
                    end_ms: now,
                    colors: vec!["#aa00aa".into()],
                    state_label: Some("happy".into()),
                }],
                points: vec![],
                y_frac: 0.2,
                height_frac: 0.5,
                z_index: 0.2,
                default_colors: vec!["#aa00aa".into()],
            },
            NowviewField {
                field: "workout".into(),
                blocks: vec![],
                points: vec![],
                y_frac: 0.7,
                height_frac: 0.5,
                z_index: 0.7,
                default_colors: vec!["#226699".into()],
            },
        ];
        let s = render_nowview_terminal(&fields, &snap_cfg(now));
        assert!(!s.contains("mood"));
        assert!(!s.contains("workout"));
        assert!(!s.contains("happy"));
    }

    #[test]
    fn snapshot_renders_hh_mm_footer() {
        let now = 1_700_000_000_000_i64;
        let fields = vec![NowviewField {
            field: "mood".into(),
            blocks: vec![],
            points: vec![],
            y_frac: 0.0,
            height_frac: 1.0,
            z_index: 0.0,
            default_colors: vec!["#101010".into()],
        }];
        let s = render_nowview_terminal(&fields, &snap_cfg(now));
        assert!(s.contains("21:11"), "expected hh:mm footer in:\n{s}");
        // Footer is the last line and it's the only line containing the
        // label (no other chrome echoes the clock).
        let lines: Vec<&str> = s.split('\n').collect();
        assert!(lines.last().unwrap().contains("21:11"));
    }

    #[test]
    fn snapshot_uses_default_colors_when_no_block() {
        let now = 1_700_000_000_000_i64;
        let mut c = snap_cfg(now);
        c.use_color = true;
        let fields = vec![NowviewField {
            field: "lonely".into(),
            blocks: vec![],
            points: vec![],
            y_frac: 0.0,
            height_frac: 1.0,
            z_index: 0.0,
            default_colors: vec!["#80c0ff".into()],
        }];
        let s = render_nowview_terminal(&fields, &c);
        assert!(
            s.contains("\x1b[48;2;128;192;255m"),
            "expected default-color fill ANSI, got:\n{s:?}"
        );
    }

    #[test]
    fn snapshot_orders_bands_by_y_frac_then_z_index() {
        let now = 1_700_000_000_000_i64;
        let mut c = snap_cfg(now);
        c.use_color = true;
        let fields = vec![
            NowviewField {
                field: "bottom".into(),
                blocks: vec![],
                points: vec![],
                y_frac: 0.8,
                height_frac: 1.0,
                z_index: 0.8,
                default_colors: vec!["#0000ff".into()],
            },
            NowviewField {
                field: "top".into(),
                blocks: vec![],
                points: vec![],
                y_frac: 0.1,
                height_frac: 1.0,
                z_index: 0.1,
                default_colors: vec!["#ff0000".into()],
            },
        ];
        let s = render_nowview_terminal(&fields, &c);
        let red = s.find("\x1b[48;2;255;0;0m").expect("red present");
        let blue = s.find("\x1b[48;2;0;0;255m").expect("blue present");
        assert!(
            red < blue,
            "top (red) must precede bottom (blue) in:\n{s:?}"
        );
    }

    // ─── Flow ─────────────────────────────────────────────────────────────

    fn flow_cfg(now_ms: i64) -> TerminalConfig {
        TerminalConfig {
            end_ms: now_ms,
            history_minutes: 30.0,
            now_width_minutes: 0.0,
            cols: 40,
            rows: 11,
            use_color: false,
            current_time_label: "21:12".into(),
            mode: RenderMode::Flow,
        }
    }

    #[test]
    fn flow_drops_field_labels_but_keeps_axis_ticks() {
        let now = 1_700_000_000_000_i64;
        let fields = vec![NowviewField {
            field: "mood".into(),
            blocks: vec![NowviewBlock {
                start_ms: now - 20 * 60_000,
                end_ms: now,
                colors: vec!["#aa00aa".into()],
                state_label: Some("happy".into()),
            }],
            points: vec![],
            y_frac: 0.0,
            height_frac: 1.0,
            z_index: 0.0,
            default_colors: vec!["#aa00aa".into()],
        }];
        let s = render_nowview_terminal(&fields, &flow_cfg(now));
        assert!(!s.contains("mood"));
        assert!(!s.contains("happy"));
        assert!(s.contains("21:12"), "expected hh:mm at right edge in:\n{s}");
        assert!(
            s.contains("-15m") || s.contains("-30m"),
            "expected axis tick in:\n{s}"
        );
    }

    #[test]
    fn flow_paints_dashed_now_line_at_right_edge() {
        let now = 1_700_000_000_000_i64;
        let fields = vec![NowviewField {
            field: "mood".into(),
            blocks: vec![NowviewBlock {
                start_ms: now - 20 * 60_000,
                end_ms: now,
                colors: vec!["#aa00aa".into()],
                state_label: None,
            }],
            points: vec![],
            y_frac: 0.0,
            height_frac: 1.0,
            z_index: 0.0,
            default_colors: vec!["#aa00aa".into()],
        }];
        let s = render_nowview_terminal(&fields, &flow_cfg(now));
        assert!(s.contains('┊'), "expected dashed now-line glyph in:\n{s}");
    }

    #[test]
    fn flow_now_line_uses_yellow_in_color_mode() {
        let now = 1_700_000_000_000_i64;
        let mut c = flow_cfg(now);
        c.use_color = true;
        let fields = vec![NowviewField {
            field: "mood".into(),
            blocks: vec![],
            points: vec![],
            y_frac: 0.0,
            height_frac: 1.0,
            z_index: 0.0,
            default_colors: vec!["#101010".into()],
        }];
        let s = render_nowview_terminal(&fields, &c);
        assert!(
            s.contains("\x1b[38;2;255;204;0m"),
            "expected #ffcc00 now-line ANSI in:\n{s:?}"
        );
    }
}
