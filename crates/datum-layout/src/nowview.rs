//! Pure data + helpers for `datum nowview`.
//!
//! Per bead da-6jy the SVG render path now reuses [`crate::dayview`] —
//! nowview is a single-day dayview scoped to the last `history_minutes`
//! with a relative-minute axis. The CLI builds a [`crate::dayview::DayviewConfig`]
//! directly, so this module no longer carries its own compute/layout types.
//!
//! What remains:
//! - `NowviewField` / `NowviewBlock` / `NowviewPoint`: input shape consumed
//!   by [`crate::nowview_terminal`] for the terminal band rendered under
//!   `datum now`. The CLI's nowview command emits both shapes (terminal +
//!   dayview-style) from the same DB scan.
//! - `default_width_for_history`: the history-aware width default used when
//!   the user omits `--width`.
//! - `tick_config`: the major/minor tick interval chooser shared with the
//!   axis-tick generator below.
//! - `nowview_axis_ticks`: produces [`crate::dayview::DayviewAxisTick`]s
//!   labelled relative to "now" (`-15m`, `-30m`, …) so dayview's renderer
//!   can draw a sub-hour axis without knowing nowview exists.
//!
//! No I/O, no datum-core, no system deps — compiles cleanly to
//! `wasm32-unknown-unknown` so the SvelteKit PWA can render the same view
//! client-side.

use serde::{Deserialize, Serialize};

use crate::dayview::DayviewAxisTick;

/// One block (closed or ongoing) inside a field row. Consumed by the
/// terminal renderer in [`crate::nowview_terminal`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowviewBlock {
    pub start_ms: i64,
    pub end_ms: i64,
    /// Hex fill color (e.g. `#a1b2c3`). The caller resolves this from the
    /// state via the project's color algorithm — the layout is color-blind.
    pub color: String,
    /// Opaque label for hover / accessibility. Optional.
    pub state_label: Option<String>,
}

/// One point-in-time marker (occur dot) inside a field row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowviewPoint {
    pub time_ms: i64,
    /// Foreground fill — typically the state's color.
    pub fg_color: String,
    /// Background ring/halo color — typically the underlying ongoing-state
    /// color at this instant (per the `da-k5z` fg/bg rule).
    pub bg_color: String,
    pub state_label: Option<String>,
}

/// One field's data for the nowview window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowviewField {
    pub field: String,
    pub blocks: Vec<NowviewBlock>,
    pub points: Vec<NowviewPoint>,
}

/// One entry in the current-state panel rendered to the right of the
/// now-line (per bead da-34w). The CLI resolves the active state per
/// tracked field at "now" and passes the result as a list of these
/// entries; the SVG overlay aligns each row to the matching field band.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowviewPanelEntry {
    pub field: String,
    /// Active state value rendered as text. `None` means the field has no
    /// state at "now" (e.g. an occur-only field with no recent dot); the
    /// row still renders the field name as a glanceable presence cue.
    pub state_label: Option<String>,
    /// Hex fill color for the row's swatch and label.
    pub color: String,
}

/// Pick the default SVG width when the user didn't pass `--width`. Mirrors
/// datum's nowviewCmd.ts §"Scale default width based on history size":
/// ≤0  → 100, ≤5m → 200, ≤10m → 300, else 400.
pub fn default_width_for_history(history_minutes: f64) -> f64 {
    if history_minutes <= 0.0 {
        100.0
    } else if history_minutes <= 5.0 {
        200.0
    } else if history_minutes <= 10.0 {
        300.0
    } else {
        400.0
    }
}

/// Major / minor tick interval in minutes. Mirrors `getTickConfig` in
/// nowview.ts. `minor_minutes` is `None` for very short histories where the
/// 5-minute major spacing is already fine-grained enough.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickConfig {
    pub major_minutes: f64,
    pub minor_minutes: Option<f64>,
}

pub fn tick_config(history_minutes: f64) -> TickConfig {
    if history_minutes <= 15.0 {
        TickConfig { major_minutes: 5.0, minor_minutes: None }
    } else if history_minutes <= 60.0 {
        TickConfig { major_minutes: 15.0, minor_minutes: Some(5.0) }
    } else if history_minutes <= 180.0 {
        TickConfig { major_minutes: 30.0, minor_minutes: Some(10.0) }
    } else {
        TickConfig { major_minutes: 60.0, minor_minutes: Some(15.0) }
    }
}

/// Build the relative-minute axis ticks for a nowview-style window. The
/// returned ticks plug directly into
/// [`crate::dayview::DayviewConfig::axis_ticks_override`] — they share the
/// `(time_pos, label, is_major)` shape so dayview's renderer can paint
/// gridlines and labels without nowview-specific code.
///
/// `time_axis_extent` is the data-area pixel extent along the time axis.
/// The "now" instant maps to `time_axis_extent`; "history_minutes ago" maps
/// to 0. The `now` tick is included so callers (and dayview's gridlines)
/// see the right edge marked.
pub fn nowview_axis_ticks(
    time_axis_extent: f64,
    history_minutes: f64,
) -> Vec<DayviewAxisTick> {
    if history_minutes <= 0.0 || time_axis_extent <= 0.0 {
        return Vec::new();
    }
    let cfg = tick_config(history_minutes);
    let mut ticks: Vec<DayviewAxisTick> = Vec::new();

    // Place every minor stride (which subsumes major positions because
    // major is always a multiple of minor in `tick_config`); flag majors.
    let minor = cfg.minor_minutes.unwrap_or(cfg.major_minutes);
    let mut m = minor;
    while m <= history_minutes + 1e-9 {
        let is_major = (m % cfg.major_minutes).abs() < 1e-6
            || ((m % cfg.major_minutes) - cfg.major_minutes).abs() < 1e-6;
        let frac = (history_minutes - m) / history_minutes;
        let time_pos = frac * time_axis_extent;
        ticks.push(DayviewAxisTick {
            time_pos,
            hour: 0,
            label: format!("-{}m", m as i64),
            is_major,
        });
        m += minor;
    }
    // "Now" tick at the right edge (no label — the now-line owns it).
    ticks.push(DayviewAxisTick {
        time_pos: time_axis_extent,
        hour: 0,
        label: String::new(),
        is_major: true,
    });
    // Sort ascending by time_pos so the renderer's "skip last" logic in
    // emit_time_axis (which omits the trailing tick label) drops the now
    // tick rather than a meaningful one.
    ticks.sort_by(|a, b| a.time_pos.partial_cmp(&b.time_pos).unwrap_or(std::cmp::Ordering::Equal));
    ticks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_width_scales_with_history() {
        assert_eq!(default_width_for_history(0.0), 100.0);
        assert_eq!(default_width_for_history(5.0), 200.0);
        assert_eq!(default_width_for_history(10.0), 300.0);
        assert_eq!(default_width_for_history(15.0), 400.0);
        assert_eq!(default_width_for_history(60.0), 400.0);
    }

    #[test]
    fn tick_config_picks_intervals_from_history() {
        assert_eq!(tick_config(15.0), TickConfig { major_minutes: 5.0, minor_minutes: None });
        assert_eq!(tick_config(60.0), TickConfig { major_minutes: 15.0, minor_minutes: Some(5.0) });
        assert_eq!(tick_config(180.0), TickConfig { major_minutes: 30.0, minor_minutes: Some(10.0) });
        assert_eq!(tick_config(360.0), TickConfig { major_minutes: 60.0, minor_minutes: Some(15.0) });
    }

    #[test]
    fn axis_ticks_30min_window_use_15min_majors_and_5min_minors() {
        // 30 min → major=15, minor=5. Expect ticks at -5,-10,-15,-20,-25,-30
        // labeled, plus a trailing unlabeled "now" tick at the right edge.
        let ticks = nowview_axis_ticks(600.0, 30.0);
        let labels: Vec<&str> = ticks.iter().map(|t| t.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["-30m", "-25m", "-20m", "-15m", "-10m", "-5m", ""],
        );
        // -15m and -30m are the majors.
        let majors: Vec<&str> = ticks
            .iter()
            .filter(|t| t.is_major)
            .map(|t| t.label.as_str())
            .collect();
        assert_eq!(majors, vec!["-30m", "-15m", ""]);
        // -30m is at the left edge; "now" at the right.
        assert!(ticks.first().unwrap().time_pos.abs() < 1e-6);
        assert!((ticks.last().unwrap().time_pos - 600.0).abs() < 1e-6);
    }

    #[test]
    fn axis_ticks_empty_for_zero_history() {
        assert!(nowview_axis_ticks(600.0, 0.0).is_empty());
        assert!(nowview_axis_ticks(0.0, 30.0).is_empty());
    }
}
