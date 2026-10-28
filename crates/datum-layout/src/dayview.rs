//! Pure layout for `datum dayview` — multi-day stacked timeline (datum
//! parity + horizontal orientation per bead da-a7f).
//!
//! Reference: `/tmp/datum-survey/src/dayview/{dayview,singleday,allFieldsSvg,fieldSvgBlocks}.ts`.
//!
//! The CLI builds a [`DayviewConfig`] (per-day windows + per-field blocks
//! and points with pre-resolved colors), this module computes pixel-space
//! geometry, and the renderer in [`crate::dayview_svg`] paints it.
//!
//! ## Coordinate model
//!
//! Output structures use orientation-neutral coordinates:
//! - **time-axis** — runs along time (00:00 → 24:00)
//! - **cross-axis** — perpendicular to time; field bands stack on this
//!   axis within a day, AND days stack on this axis across rows/cols
//!
//! The renderer maps `(time, cross)` → SVG `(x, y)` per orientation:
//! - [`Orientation::Horizontal`]: time→x, cross→y (datum's original layout —
//!   time axis runs left-to-right, days stack top-to-bottom)
//! - [`Orientation::Vertical`]: time→y, cross→x (time axis runs top-to-bottom,
//!   each day is a column)
//!
//! No I/O, no datum-core, no system deps — compiles cleanly to
//! `wasm32-unknown-unknown` so the SvelteKit PWA can render the same view
//! client-side.
//!
//! ## A day the clock jumped in (da-1v9gp)
//!
//! A day is drawn on the wall clock it was lived by, so its axis is the
//! clock's 00:00 → 24:00 and the caller cuts it into [`DayviewSpan`]s, one
//! per UTC offset. Where the clock jumped forward no span covers the axis:
//! a dead zone. Where it jumped back two spans cover the same clock times,
//! and the band forks into a lane for each ([`RenderedSpan::lane`]).

use serde::{Deserialize, Serialize};

/// Visual orientation of the dayview. Named by the time-axis direction.
/// Default is [`Orientation::Horizontal`] per the kit-spec design
/// discussion (2026-04-30).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Orientation {
    /// Time runs vertically (Y axis, top→bottom); days stack horizontally
    /// (X axis). Date labels on top of each column, time-axis ticks on
    /// the left. Each day is a column.
    Vertical,
    /// Time runs horizontally (X axis, left→right); days stack vertically
    /// (Y axis). Date labels on the left, time-axis ticks at the bottom.
    /// This is datum's original layout.
    #[default]
    Horizontal,
}

/// One day's window + per-field input data. The caller (CLI) resolves the
/// label strings, day boundaries (in UTC ms), and the field data —
/// including state colors — before passing to [`compute_dayview`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewDayInput {
    /// Pre-formatted label lines for the date axis on the left
    /// (e.g. `["Mon", "Apr 27", "2026"]`). The layout auto-shrinks from
    /// 3 → 2 → 1 line based on row height + minimum font size.
    pub label_lines: Vec<String>,
    /// The day itself, ISO `YYYY-MM-DD`, as the caller cut it. Carried
    /// through to [`RenderedDay::date`] for a caller that draws its own
    /// date labels (`--label-width 0`) and so cannot read them off the
    /// SVG. `None` when the band is not a civic day (nowview's window).
    #[serde(default)]
    pub date: Option<String>,
    /// Day window start (inclusive) in UTC ms.
    pub start_ms: i64,
    /// Day window end (exclusive) in UTC ms.
    pub end_ms: i64,
    /// What `datum check integrity` finds in this day (da-0lful): the
    /// warning icon next to the date label, and a line across the day at
    /// each instant.
    #[serde(default)]
    pub errors: Vec<DayviewError>,
    /// Per-field data for this day, with positioning and colors already
    /// resolved.
    pub fields: Vec<DayviewFieldInput>,
    /// The day's stretches in time order (da-1v9gp). Empty is one: the
    /// whole window across the whole axis.
    #[serde(default)]
    pub spans: Vec<DayviewSpan>,
}

/// An integrity finding at an instant of the day (da-0lful).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewError {
    pub time_ms: i64,
    pub field: String,
    /// What is wrong, in check's words: `opened while already open`.
    pub what: String,
}

/// A [`DayviewError`] placed: where along the time axis, and in which of
/// [`RenderedDay::spans`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedError {
    pub time_ms: i64,
    pub field: String,
    pub what: String,
    pub time_pos: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub span: usize,
}

/// One stretch of a day lived at a single UTC offset (da-1v9gp).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewSpan {
    /// The stretch's instants in UTC ms, `[start_ms, end_ms)`.
    pub start_ms: i64,
    pub end_ms: i64,
    /// Where its wall clock begins and ends along the day's axis, as
    /// fractions of it: 0 is 00:00, 1 is 24:00.
    pub lo: f64,
    pub hi: f64,
    /// What an end of it inside the day is labelled: `UTC+2`.
    pub label: String,
    /// Its clock in seconds east of UTC, for a reader of the picture to
    /// tell a mark's time by (da-3ctgr). `None` is the reader's own clock.
    pub offset_s: Option<i32>,
}

/// One field's data for a day. Position/height/z-index are fractions of
/// the day's data-row (not pixels) — the layout multiplies by `day_height`
/// and sorts by `z_index` (low to high → low renders first / underneath).
///
/// `y_frac` and `height_frac` describe **perpendicular-to-time** positioning,
/// which the render layer rotates per orientation (per bead da-a7f).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewFieldInput {
    pub field: String,
    /// Position along the cross-axis (perpendicular to time) as a fraction
    /// (0.0 = near edge, 1.0 = far edge) of the day's data area. In horizontal
    /// mode 0.0 = top of the row, in vertical mode 0.0 = left of the day's
    /// column. Caller resolves from `FieldSpec` (y, height, z_index columns)
    /// with deterministic-hash fallback.
    pub y_frac: f64,
    /// Cross-axis extent as a fraction of the day's data area.
    pub height_frac: f64,
    /// Stacking order; higher = on top. Defaults to `y_frac` when unset
    /// per the original datum behavior.
    pub z_index: f64,
    pub blocks: Vec<DayviewBlock>,
    pub points: Vec<DayviewPoint>,
    /// The field's custom SVG icon (da-45wgz), the whole `<svg>` element.
    /// The renderer writes it ONCE, in `<defs>`, and each dot whose
    /// [`DayviewPoint::icon`] is set `<use>`s it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// How the field's dots are sized against the one the layout picks.
    #[serde(default, skip_serializing_if = "GlyphSize::is_default")]
    pub glyph_size: GlyphSize,
}

/// An occur dot's size (da-c05v1): `scale` is its height over
/// [`STANDARD_DOT`]'s (da-rjf34), `aspect` the mark's width over its
/// height. Blocks ignore both; `1, 1` is a round dot the standard's size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GlyphSize {
    pub scale: f64,
    pub aspect: f64,
}

impl GlyphSize {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for GlyphSize {
    fn default() -> Self {
        Self {
            scale: 1.0,
            aspect: 1.0,
        }
    }
}

/// A dot's height, a fraction of the day, before `--symbol-scale` multiplies
/// it (da-ijjjn): two thirds of the lane of a field whose spec names no
/// height, datum-core's `DEFAULT_FIELD_HEIGHT` (da-sjhuj, da-a87dh). Its lane takes no part, so a lane
/// thinner than it has the dot pass its edges: a dot marks a moment, not
/// the lane's area. datum.day's placement editor keeps a copy
/// (`web/src/lib/Placement.svelte`), which `web/tests/standard-dot-pure.test.mjs` holds to this.
pub const STANDARD_DOT: f64 = 0.08;

/// One block (closed or ongoing) on a field row. Blocks may extend beyond
/// the day's window; [`compute_dayview`] clips them to the visible domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewBlock {
    pub start_ms: i64,
    pub end_ms: i64,
    pub fill: BlockFill,
    pub state_label: Option<String>,
    /// The hid of THE ENTRY THAT GENERATED this block (da-6hg6v), whole,
    /// as the durable identity — not the short prefix the CLI prints.
    /// Which end of the span holds that entry depends on how the block
    /// was made (a state change runs forward from it, a duration back to
    /// it), so only the walk that emitted it can say; the renderer just
    /// carries it out to `data-id`.
    #[serde(default)]
    pub id: Option<String>,
    /// What the mark says on hover (da-s5da) — `work: coding 14:00–14:30`
    /// in a nowview, `run: on [30m]` in a dayview (da-k16pi).
    /// The caller writes it, since it owns the clock; the renderer puts it
    /// in the mark's `<title>`.
    #[serde(default)]
    pub title: Option<String>,
    /// When the entry behind the mark was logged, UTC ms (da-k16pi): the
    /// renderer's `data-created`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_ms: Option<i64>,
}

/// Fill mode for a block. Multi-state values (arrays) render as diagonal
/// stripes — one stripe per sub-state, colored by its hue. Mirrors the
/// original `fieldSvgBlocks.ts` lines 105–177.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockFill {
    /// Single solid hex color (e.g. `#a1b2c3`).
    Single(String),
    /// Multi-state — each entry is a hex color for one stripe (in order),
    /// or `none` for a stripe left unpainted.
    MultiState(Vec<String>),
}

/// A point-in-time marker (occur dot), drawn in `fg_color`: the state's color for a
/// point-switch, the field's own for a plain occur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewPoint {
    pub time_ms: i64,
    pub fg_color: String,
    pub state_label: Option<String>,
    /// As [`DayviewBlock::id`] — a dot's own entry.
    #[serde(default)]
    pub id: Option<String>,
    /// As [`DayviewBlock::title`].
    #[serde(default)]
    pub title: Option<String>,
    /// As [`DayviewBlock::created_ms`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_ms: Option<i64>,
    /// The character drawn in place of the dot's circle (da-gzowt),
    /// resolved by the caller from the field's spec. `None` is the circle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
    /// Draw the field's [`DayviewFieldInput::icon`] here, over `glyph`
    /// (da-45wgz). Resolved by the caller, as `glyph` is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub icon: bool,
    /// A moment of an active state (`switch run -m`), not an occurrence
    /// (da-lqo7x): drawn as a dotted line across its lane, not a dot. A
    /// moment of `false`, TRACKED or IGNORED is a dot, as an occur is
    /// (da-2028s).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub moment: bool,
}

/// Top-level dayview knobs. Per-day data is in `days`. The `now_ms` field
/// drives the yellow dashed now-line which appears only on the day whose
/// window contains `now_ms`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewConfig {
    pub days: Vec<DayviewDayInput>,
    pub now_ms: i64,
    /// Total SVG width in pixels (including margins + label area).
    pub width: f64,
    /// Total SVG height in pixels. None → auto from `day_height + n_days`
    /// in horizontal mode; defaults to a reasonable time-axis extent in
    /// vertical mode.
    pub height: Option<f64>,
    /// Per-day cross-axis extent in pixels (the size perpendicular to
    /// time). Horizontal mode: per-day band height. Vertical mode: per-day
    /// column width. Default 100.
    pub day_height: f64,
    /// Pixel gap between adjacent days along the day-stacking axis.
    /// Default 15 (matches datum).
    pub interday_margin: f64,
    /// Pixel size of the time-axis strip (perpendicular to time). Horizontal
    /// mode: height of bottom strip. Vertical mode: width of left strip.
    /// Default 15.
    pub time_axis_height: f64,
    /// Outer margin around the plot. Default 2.
    pub margin: f64,
    /// Optional fixed date-label strip size on the side adjacent to the
    /// time-axis-perpendicular edge. Horizontal: width on the left.
    /// Vertical: height on top. When None, derived from `font_size * 4`
    /// (matches datum).
    pub label_width: Option<f64>,
    /// Render orientation. Default [`Orientation::Horizontal`].
    pub orientation: Orientation,
    /// Pre-computed axis ticks. When `None`, [`compute_dayview`] auto-fills
    /// 25 hourly ticks across the data area (legacy 24h behavior). When
    /// `Some`, the layout uses the supplied vector verbatim — this is how
    /// `nowview` injects its sub-hour relative-minute ticks while reusing
    /// the dayview render path (per bead da-6jy).
    pub axis_ticks_override: Option<Vec<DayviewAxisTick>>,
    /// Pixel width reserved on the far side of the data area for an
    /// orientation-specific overlay. Horizontal mode: shrinks
    /// `time_axis_extent` so the overlay sits to the right of the now-line
    /// without clipping data. Vertical mode is unused (set to 0). Per bead
    /// da-34w nowview uses this to stack a current-state panel beside the
    /// now-line.
    pub right_panel_width: f64,
}

impl Default for DayviewConfig {
    fn default() -> Self {
        Self {
            days: Vec::new(),
            now_ms: 0,
            width: 2000.0,
            height: None,
            day_height: 100.0,
            interday_margin: 15.0,
            time_axis_height: 15.0,
            margin: 2.0,
            label_width: None,
            orientation: Orientation::default(),
            axis_ticks_override: None,
            right_panel_width: 0.0,
        }
    }
}

/// Computed pixel-space layout. Coords are in the orientation-neutral
/// `(time_pos, cross_pos)` space; the renderer maps to SVG `(x, y)` per
/// `orientation`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewLayout {
    pub orientation: Orientation,
    pub width: f64,
    pub height: f64,
    pub margin: f64,
    /// Resolved date-label strip size (in the dimension perpendicular to
    /// the day-stack axis).
    pub label_width: f64,
    pub day_height: f64,
    /// Band + gap: what one day advances the day-stack axis by.
    #[serde(default)]
    pub day_pitch: f64,
    pub interday_margin: f64,
    pub time_axis_height: f64,
    /// Font size used for the date labels (after 3→2→1 shrink).
    pub label_font_size: f64,
    /// Number of lines actually shown in the date label (1, 2, or 3).
    pub label_lines_shown: usize,
    /// Inner data-area rect (relative to the SVG origin). The full plot
    /// area, including label/axis strips.
    pub plot: Rect,
    /// Total pixel extent along the time axis (across the data area).
    pub time_axis_extent: f64,
    pub days: Vec<RenderedDay>,
    /// Hourly axis ticks shared across every day, in time-axis pixels
    /// (relative to the plot's data-area origin on the time axis).
    pub axis_ticks: Vec<DayviewAxisTick>,
    /// The same ticks in SVG coordinates — what a caller drawing its own
    /// axis labels needs, without redoing the strip arithmetic.
    #[serde(default)]
    pub hours: Vec<DayviewHourMark>,
    /// Pixel width reserved beside the data area for an orientation-specific
    /// overlay. Carried through from [`DayviewConfig::right_panel_width`] so
    /// nowview's overlay knows where its panel sits.
    pub right_panel_width: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// One day's geometry within the layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedDay {
    /// Truncated (when shrinking) date labels actually shown.
    pub label_lines: Vec<String>,
    /// The day, ISO `YYYY-MM-DD`, from [`DayviewDayInput::date`] —
    /// untruncated, unlike `label_lines`.
    #[serde(default)]
    pub date: Option<String>,
    /// SVG y of the band's leading edge. Horizontal: the top of this day's
    /// row. Vertical: the top of the column strip, the same for every day.
    #[serde(default)]
    pub y: f64,
    /// Offset of this day along the day-stacking (cross) axis, in pixels,
    /// relative to the data-area origin.
    pub day_offset: f64,
    pub has_error: bool,
    /// [`DayviewDayInput::errors`], placed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<RenderedError>,
    pub fields: Vec<RenderedField>,
    /// Time-axis pixel position of the now-line within the data area. Some
    /// only on the day whose window contains `now_ms`.
    pub now_time_pos: Option<f64>,
    /// Far end of the now-line along the day-stack/cross axis, relative to
    /// the data-area origin. For the last day of a horizontal layout this
    /// extends through the time-axis tick strip; otherwise stops at the
    /// day's far edge.
    pub now_extent_far: f64,
    /// The day's window in UTC ms — what the time axis spans, from
    /// [`DayviewDayInput`].
    #[serde(default)]
    pub start_ms: i64,
    #[serde(default)]
    pub end_ms: i64,
    /// Near end of the now-line, as `now_extent_far` is its far one: the
    /// day's leading edge, or its lane's where the band is forked.
    #[serde(default)]
    pub now_extent_near: f64,
    /// The day's stretches, placed; each mark names its own by index.
    #[serde(default)]
    pub spans: Vec<RenderedSpan>,
    /// The clock times that never happened, each `[time_offset,
    /// time_extent]` in pixels: the renderer hatches them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dead_zones: Vec<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zone_labels: Vec<RenderedZoneLabel>,
}

/// A [`DayviewSpan`] placed along the time axis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedSpan {
    pub start_ms: i64,
    pub end_ms: i64,
    pub time_offset: f64,
    pub time_extent: f64,
    /// [`DayviewSpan::offset_s`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset_s: Option<i32>,
    /// Where the band narrows to this stretch's lane and widens again:
    /// `[time_pos, near, far]` knots, the band's two cross-axis edges as
    /// fractions of it, straight between two knots and level outside
    /// them. Empty is the whole band.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lane: Vec<[f64; 3]>,
}

impl RenderedSpan {
    /// The band's cross-axis edges at `time_pos`, as fractions of it.
    pub fn lane_at(&self, time_pos: f64) -> (f64, f64) {
        let Some(i) = self.lane.iter().rposition(|k| k[0] <= time_pos) else {
            return self.lane.first().map_or((0.0, 1.0), |k| (k[1], k[2]));
        };
        let a = self.lane[i];
        match self.lane.get(i + 1) {
            Some(b) => {
                let f = (time_pos - a[0]) / (b[0] - a[0]);
                (a[1] + f * (b[1] - a[1]), a[2] + f * (b[2] - a[2]))
            }
            None => (a[1], a[2]),
        }
    }
}

/// The offset written at an end of a stretch, in the room beside it: the
/// text runs across the band, centered on these positions, its foot
/// against the end it names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedZoneLabel {
    pub time_pos: f64,
    /// Cross-axis pixel position within the day's band.
    pub cross_pos: f64,
    pub text: String,
    pub font_size: f64,
    /// The stretch it names is the one before it, which ended there: its
    /// foot faces back along the axis.
    #[serde(default)]
    pub names_earlier: bool,
}

/// One field's positioned blocks + points within a day.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedField {
    pub field: String,
    /// Cross-axis offset within the day's band, in pixels.
    pub cross_offset: f64,
    /// Cross-axis extent in pixels.
    pub cross_extent: f64,
    pub z_index: f64,
    pub blocks: Vec<RenderedBlock>,
    pub points: Vec<RenderedPoint>,
    /// As [`DayviewFieldInput::icon`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// [`GlyphSize::aspect`]; the scale is already in each point's `r`.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub glyph_aspect: f64,
}

fn one() -> f64 {
    1.0
}

fn is_one(n: &f64) -> bool {
    *n == 1.0
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedBlock {
    /// Time-axis offset of the block (clipped to the day window), in pixels.
    pub time_offset: f64,
    /// Time-axis pixel extent (block width along time).
    pub time_extent: f64,
    pub fill: BlockFill,
    pub state_label: Option<String>,
    /// As [`DayviewBlock::id`].
    #[serde(default)]
    pub id: Option<String>,
    /// The block's own span in UTC ms, before clipping to the day.
    #[serde(default)]
    pub start_ms: i64,
    #[serde(default)]
    pub end_ms: i64,
    #[serde(default)]
    pub title: Option<String>,
    /// As [`DayviewBlock::created_ms`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_ms: Option<i64>,
    /// Which of [`RenderedDay::spans`] this piece is drawn in. A block the
    /// clock jumped under is a piece in each.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub span: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedPoint {
    /// Time-axis pixel position.
    pub time_pos: f64,
    /// Cross-axis pixel position.
    pub cross_pos: f64,
    pub r: f64,
    pub fg_color: String,
    pub state_label: Option<String>,
    /// As [`DayviewBlock::id`].
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub time_ms: i64,
    #[serde(default)]
    pub title: Option<String>,
    /// As [`DayviewBlock::created_ms`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_ms: Option<i64>,
    /// As [`DayviewPoint::glyph`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
    /// As [`DayviewPoint::icon`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub icon: bool,
    /// As [`DayviewPoint::moment`].
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub moment: bool,
    /// As [`RenderedBlock::span`].
    #[serde(default, skip_serializing_if = "is_zero")]
    pub span: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayviewAxisTick {
    /// Time-axis pixel position.
    pub time_pos: f64,
    pub hour: u8,
    pub label: String,
    /// Major ticks render with a brighter gridline / are eligible for axis
    /// labels. The hourly default marks every 3rd hour as major; sub-hour
    /// callers (e.g. nowview's relative-minute ticks) supply their own.
    #[serde(default)]
    pub is_major: bool,
}

/// One gridline in SVG coordinates (da-55ru).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DayviewHourMark {
    pub hour: u8,
    /// Position along the time axis, from the SVG origin: x in horizontal
    /// mode, y in vertical mode (where the gridline runs the other way).
    pub x: f64,
}

/// Minimum font size below which we drop a label line.
const MIN_LABEL_FONT: f64 = 7.0;
const MAX_LABEL_FONT: f64 = 20.0;
/// Padding subtracted from `day_height / n_lines` when picking font size
/// (matches datum's `Math.min(dayHeight / nLabels - 6, 20)`).
const LABEL_FONT_PAD: f64 = 6.0;

/// Default time-axis extent (px) when caller doesn't pass `height` in
/// vertical mode (time runs along Y). Roughly matches the default
/// horizontal-mode density (24 hours over ~2000px → ~83 px/hour) scaled
/// to a portrait aspect.
const DEFAULT_VERTICAL_TIME_EXTENT: f64 = 1200.0;

/// Floor for a derived per-day extent — below this a band is a line, not
/// a row, so a box too small to hold `n_days` gets a scrollable overflow
/// rather than an invisible day.
const MIN_DAY_HEIGHT: f64 = 10.0;

/// The per-day cross-axis extent that makes `n_days` days fill `extent`
/// pixels of the day-stack axis, or `None` when the box leaves no room.
///
/// [`compute_dayview`] only ever derives the total from `day_height`,
/// never the reverse, so a caller that knows the box it must fit resolves
/// `day_height` here first. That is what `datum dayview --height` does
/// when no `--day-height` is given; the browser's layout module calls the
/// same function, so a pane asking for a 102-pixel band gets the geometry
/// `datum dayview --height 102` writes.
///
/// `extent` is the SVG's total height in horizontal mode and its total
/// width in vertical mode — the axis the days stack along, either way.
pub fn day_height_for_extent(
    extent: f64,
    margin: f64,
    time_axis_height: f64,
    interday_margin: f64,
    n_days: usize,
) -> Option<f64> {
    if n_days == 0 {
        return None;
    }
    let usable = extent - 2.0 * margin - time_axis_height - (n_days - 1) as f64 * interday_margin;
    (usable > 0.0).then(|| (usable / n_days as f64).max(MIN_DAY_HEIGHT))
}

/// Air between a zone label's foot and the end it names.
const ZONE_LABEL_PAD: f64 = 1.5;

/// A label's text from its foot to its baseline, and from its baseline to
/// the top of its capitals, in ems: `dy="0.35em"` in the SVG, and a sans
/// serif's cap height.
const ZONE_LABEL_DROP: f64 = 0.35;
const ZONE_LABEL_CAPS: f64 = 0.75;

/// Air between two lanes, a share of the band (vividn, 2026-10-26: «a
/// small margin between the two horizontal bars, let's try 8% of the day
/// height», then «let's reduce it to 5%», da-0b9z4).
const LANE_GAP: f64 = 0.05;

/// The straight pieces a lane's S is drawn in.
const EASE_STEPS: usize = 8;

/// Place a day's stretches along `extent` pixels of axis (da-1v9gp): the
/// lanes where their clock times overlap, the dead zones no stretch
/// covers, and a label at each end that falls inside the day.
///
/// Stretches that overlap share the band, the earliest nearest. The first
/// narrows to its lane on a quick S before the overlap and the last widens
/// again after it, each leaving room past the end beside it for that
/// end's label, which runs along the end across the band, and two labels'
/// width of air between it and the S.
fn place_spans(
    spans: &[DayviewSpan],
    extent: f64,
    day_height: f64,
) -> (Vec<RenderedSpan>, Vec<[f64; 2]>, Vec<RenderedZoneLabel>) {
    const EPS: f64 = 1e-6;
    let lo = |i: usize| spans[i].lo * extent;
    let hi = |i: usize| spans[i].hi * extent;
    let mut placed: Vec<RenderedSpan> = (0..spans.len())
        .map(|i| RenderedSpan {
            start_ms: spans[i].start_ms,
            end_ms: spans[i].end_ms,
            time_offset: lo(i),
            time_extent: hi(i) - lo(i),
            offset_s: spans[i].offset_s,
            lane: Vec::new(),
        })
        .collect();
    let mut labels = Vec::new();
    // vividn, 2026-10-25: «rather abrupt S curve, so still smooth, but
    // quick. Maybe about 20 minutes worth of x room (at least in a 24-hour
    // window, probably quicker in a zoomed in window)»: a share of the axis.
    let ease = extent / 72.0;
    // From the whole band at `whole` to the lane at `lane`, on a smoothstep.
    let s_curve = |whole: f64, lane: f64, (near, far): (f64, f64)| {
        (0..=EASE_STEPS).map(move |n| {
            let t = n as f64 / EASE_STEPS as f64;
            let e = t * t * (3.0 - 2.0 * t);
            [whole + t * (lane - whole), e * near, (1.0 - e) + e * far]
        })
    };
    let mut i = 0;
    while i < spans.len() {
        let mut end = i + 1;
        let mut reach = hi(i);
        while end < spans.len() && lo(end) < reach - EPS {
            reach = reach.max(hi(end));
            end += 1;
        }
        // Each lane's share of the band and where it starts across it. The
        // two ends of a dead zone share the band as two lanes do, with no
        // bars to keep apart.
        let k = end - i;
        let (width, step) = match k {
            1 => (0.5, 0.5),
            _ => {
                let width = (1.0 - (k - 1) as f64 * LANE_GAP) / k as f64;
                (width, width + LANE_GAP)
            }
        };
        // The text runs across the band, so its length is a lane's and
        // only its height is the axis's.
        let widest = (i..end).map(|s| spans[s].label.chars().count()).max();
        let fits = (day_height * width - 2.0 * ZONE_LABEL_PAD) / (widest.unwrap_or(1) as f64 * 0.6);
        let font_size = fits.clamp(6.0, 11.0);
        // vividn, 2026-10-26: the label «actually on the edge of [the]
        // break, there can be a bit of room between the label and the S
        // so that it is obvious that it pertain to the broken timeline»,
        // then «about 50% of» two labels' room: one (da-0b9z4).
        let room = ZONE_LABEL_PAD + (ZONE_LABEL_CAPS + 1.0) * font_size;
        for s in i..end {
            let j = s - i;
            let (near, far) = (j as f64 * step, j as f64 * step + width);
            if k > 1 {
                let split = (i + 1..end).map(lo).fold(f64::INFINITY, f64::min) - room;
                let merge = (i..s).map(hi).fold(0.0, f64::max) + room;
                placed[s].lane = if j == 0 && split > lo(s) {
                    s_curve((split - ease).max(lo(s)), split, (near, far)).collect()
                } else if s + 1 == end && merge < hi(s) {
                    let mut knots: Vec<_> =
                        s_curve((merge + ease).min(hi(s)), merge, (near, far)).collect();
                    knots.reverse();
                    knots
                } else {
                    vec![[lo(s), near, far]]
                };
            }
            // Beside its end, its baseline the pad from it, but never off
            // the axis: against an edge the text is over its own stretch
            // instead.
            let (half, off) = (
                font_size / 2.0 + ZONE_LABEL_PAD,
                ZONE_LABEL_PAD + ZONE_LABEL_DROP * font_size,
            );
            let mut label = |time_pos: f64, lane: usize, names_earlier: bool| {
                labels.push(RenderedZoneLabel {
                    time_pos: time_pos.max(half).min(extent - half),
                    cross_pos: (lane as f64 * step + width / 2.0) * day_height,
                    text: spans[s].label.clone(),
                    font_size,
                    names_earlier,
                });
            };
            if lo(s) > EPS {
                label(lo(s) - off, if k > 1 { j } else { 1 }, false);
            }
            if hi(s) < extent - EPS {
                label(hi(s) + off, if k > 1 { j } else { 0 }, true);
            }
        }
        i = end;
    }
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by(|a, b| lo(*a).total_cmp(&lo(*b)));
    let mut dead = Vec::new();
    let mut reach = 0.0_f64;
    for s in order {
        if lo(s) > reach + EPS {
            dead.push([reach, lo(s) - reach]);
        }
        reach = reach.max(hi(s));
    }
    if reach < extent - EPS {
        dead.push([reach, extent - reach]);
    }
    (placed, dead, labels)
}

/// Compute the dayview layout.
pub fn compute_dayview(config: &DayviewConfig) -> DayviewLayout {
    let n_days = config.days.len().max(1);

    // Pick label font size by trying 3 → 2 → 1 lines, taking the largest
    // that yields a usable font size.
    let mut label_lines_shown = 1usize;
    let mut label_font_size = MIN_LABEL_FONT;
    for n_lines in (1..=3).rev() {
        let fs = (config.day_height / n_lines as f64 - LABEL_FONT_PAD).min(MAX_LABEL_FONT);
        if fs >= MIN_LABEL_FONT {
            label_lines_shown = n_lines;
            label_font_size = fs;
            break;
        }
    }

    // A negative strip would draw the bands left of the margin; 0 is the
    // floor, and it means no date labels at all (da-55ru).
    let label_width = config.label_width.unwrap_or(label_font_size * 4.0).max(0.0);

    let data_rows_extent =
        n_days as f64 * config.day_height + (n_days - 1) as f64 * config.interday_margin;

    // Resolve total SVG dimensions + plot rect + time-axis extent based on
    // orientation. The layout always lays out `(time, cross)` logically;
    // the renderer maps to SVG (x, y).
    let (width, height, plot, time_axis_extent) = match config.orientation {
        Orientation::Horizontal => {
            let plot_width = (config.width - 2.0 * config.margin).max(0.0);
            let time_axis_extent =
                (plot_width - label_width - config.right_panel_width.max(0.0)).max(0.0);
            let plot_height = data_rows_extent + config.time_axis_height;
            let height = plot_height + 2.0 * config.margin;
            let plot = Rect {
                x: config.margin,
                y: config.margin,
                width: plot_width,
                height: plot_height,
            };
            (config.width, height, plot, time_axis_extent)
        }
        Orientation::Vertical => {
            // Days stack along x; time runs along y. The data area sits
            // below the date-label strip and to the right of the time-axis
            // strip.
            let plot_width = (config.width - 2.0 * config.margin).max(0.0);
            // Day-stack extent is determined by data_rows_extent; the
            // configured `width` is informational and may not match (the
            // renderer pads or letterboxes if so).
            let _ = plot_width;
            let total_height_resolved = config
                .height
                .unwrap_or(DEFAULT_VERTICAL_TIME_EXTENT + label_width + 2.0 * config.margin);
            let plot_height = (total_height_resolved - 2.0 * config.margin).max(0.0);
            let time_axis_extent =
                (plot_height - label_width - config.right_panel_width.max(0.0)).max(0.0);
            // Recompute total width from the day-stack data plus axis strip.
            let resolved_plot_width = config.time_axis_height + data_rows_extent;
            let width = resolved_plot_width + 2.0 * config.margin;
            let plot = Rect {
                x: config.margin,
                y: config.margin,
                width: resolved_plot_width,
                height: plot_height,
            };
            (width, total_height_resolved, plot, time_axis_extent)
        }
    };

    // Axis ticks: caller may supply pre-computed ticks (used by nowview
    // for sub-hour relative-minute ticks). Otherwise hourly defaults across
    // `time_axis_extent` — same positions for every day.
    let axis_ticks = config
        .axis_ticks_override
        .clone()
        .unwrap_or_else(|| hourly_axis_ticks(time_axis_extent));

    // SVG origin of the data area: the plot's translate plus the strips
    // the renderer puts before it (`dayview_svg::PlotGeom`).
    let (data_x, data_y) = match config.orientation {
        Orientation::Horizontal => (config.margin + label_width, config.margin),
        Orientation::Vertical => (
            config.margin + config.time_axis_height,
            config.margin + label_width,
        ),
    };
    let hours: Vec<DayviewHourMark> = axis_ticks
        .iter()
        .map(|t| DayviewHourMark {
            hour: t.hour,
            x: match config.orientation {
                Orientation::Horizontal => data_x + t.time_pos,
                Orientation::Vertical => data_y + t.time_pos,
            },
        })
        .collect();

    let mut days: Vec<RenderedDay> = Vec::with_capacity(config.days.len());

    for (i, day) in config.days.iter().enumerate() {
        let day_offset = i as f64 * (config.day_height + config.interday_margin);

        // Truncate label_lines to `label_lines_shown` (preserve top-down order).
        let label_lines: Vec<String> = day
            .label_lines
            .iter()
            .take(label_lines_shown)
            .cloned()
            .collect();

        let whole = [DayviewSpan {
            start_ms: day.start_ms,
            end_ms: day.end_ms,
            lo: 0.0,
            hi: 1.0,
            label: String::new(),
            offset_s: None,
        }];
        // A reserved strip with time on past now sits AT now (da-9g3bf):
        // the past, then the strip, then the future, as two stretches
        // `right_panel_width` apart. The future's starts a millisecond on,
        // so what is at now is drawn in the past's. One at the day's very
        // start has no past before it. The strip is now's alone, and a day
        // now is outside has none (da-dv45l).
        let panel_at = config.now_ms;
        let split = config.right_panel_width > 0.0
            && day.spans.is_empty()
            && day.start_ms <= panel_at
            && panel_at < day.end_ms;
        let at = |t: i64| (t - day.start_ms) as f64 / (day.end_ms - day.start_ms) as f64;
        let past_and_future = split.then(|| {
            let strip = config.right_panel_width / time_axis_extent.max(f64::MIN_POSITIVE);
            let past = (day.start_ms < panel_at).then(|| DayviewSpan {
                end_ms: panel_at,
                hi: at(panel_at),
                ..whole[0].clone()
            });
            let start_ms = if past.is_some() {
                panel_at + 1
            } else {
                panel_at
            };
            let future = DayviewSpan {
                start_ms,
                lo: at(start_ms) + strip,
                hi: 1.0 + strip,
                ..whole[0].clone()
            };
            past.into_iter().chain([future]).collect::<Vec<_>>()
        });
        let day_spans: &[DayviewSpan] = match (&past_and_future, day.spans.is_empty()) {
            (Some(two), _) => two,
            (None, true) => &whole,
            (None, false) => &day.spans,
        };
        let (spans, mut dead_zones, mut zone_labels) =
            place_spans(day_spans, time_axis_extent, config.day_height);
        if split {
            // The gap between them is the strip, not a clock skipped.
            dead_zones.clear();
            zone_labels.clear();
        }
        let time_to_pos = |s: &DayviewSpan, t_ms: i64| -> f64 {
            let f = (t_ms - s.start_ms) as f64 / (s.end_ms - s.start_ms).max(1) as f64;
            (s.lo + f * (s.hi - s.lo)) * time_axis_extent
        };
        // The stretch an instant is drawn in: the later of two that meet at it.
        let span_of = |t_ms: i64| {
            (day_spans.iter().enumerate()).rfind(|(_, s)| s.start_ms <= t_ms && t_ms <= s.end_ms)
        };

        // Sort fields by z_index ascending (low first → renders below).
        let mut field_order: Vec<&DayviewFieldInput> = day.fields.iter().collect();
        field_order.sort_by(|a, b| {
            a.z_index
                .partial_cmp(&b.z_index)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut fields: Vec<RenderedField> = Vec::with_capacity(field_order.len());

        for f in field_order {
            // Skip empty fields per spec ("Empty fields are SKIPPED").
            if f.blocks.is_empty() && f.points.is_empty() {
                continue;
            }

            let cross_offset = f.y_frac * config.day_height;
            // Match original's `Math.ceil(pHeight * height)`.
            let cross_extent = (f.height_frac * config.day_height).ceil();

            let mut blocks: Vec<RenderedBlock> = Vec::with_capacity(f.blocks.len());
            for b in &f.blocks {
                for (span, within) in day_spans.iter().enumerate() {
                    // Clip to the stretch's window.
                    let s = b.start_ms.max(within.start_ms);
                    let e = b.end_ms.min(within.end_ms);
                    if e <= s {
                        continue;
                    }
                    let t_off = time_to_pos(within, s);
                    let t_ext = (time_to_pos(within, e) - t_off).max(0.5);
                    blocks.push(RenderedBlock {
                        time_offset: t_off,
                        time_extent: t_ext,
                        fill: b.fill.clone(),
                        state_label: b.state_label.clone(),
                        id: b.id.clone(),
                        start_ms: b.start_ms,
                        end_ms: b.end_ms,
                        title: b.title.clone(),
                        created_ms: b.created_ms,
                        span,
                    });
                }
            }

            let mut points: Vec<RenderedPoint> = Vec::with_capacity(f.points.len());
            // A dot is the standard times its scale, whatever its lane
            // (da-ijjjn): a fraction of the day, never pixels (da-c1b6m).
            let r = STANDARD_DOT * config.day_height / 2.0 * f.glyph_size.scale;

            for p in &f.points {
                let Some((span, within)) = span_of(p.time_ms) else {
                    continue;
                };
                let t = time_to_pos(within, p.time_ms);
                let c = cross_offset + cross_extent / 2.0;
                points.push(RenderedPoint {
                    time_pos: t,
                    cross_pos: c,
                    r,
                    fg_color: p.fg_color.clone(),
                    state_label: p.state_label.clone(),
                    id: p.id.clone(),
                    time_ms: p.time_ms,
                    title: p.title.clone(),
                    created_ms: p.created_ms,
                    glyph: p.glyph.clone(),
                    icon: p.icon,
                    moment: p.moment,
                    span,
                });
            }

            fields.push(RenderedField {
                field: f.field.clone(),
                cross_offset,
                cross_extent,
                z_index: f.z_index,
                blocks,
                points,
                icon: f.icon.clone(),
                glyph_aspect: f.glyph_size.aspect,
            });
        }

        let errors = (day.errors.iter())
            .filter_map(|e| {
                let (span, within) = span_of(e.time_ms)?;
                Some(RenderedError {
                    time_ms: e.time_ms,
                    field: e.field.clone(),
                    what: e.what.clone(),
                    time_pos: time_to_pos(within, e.time_ms),
                    span,
                })
            })
            .collect();

        // Now-line: only on the day whose window contains now_ms.
        let now = span_of(config.now_ms).map(|(i, s)| (i, time_to_pos(s, config.now_ms)));
        let now_time_pos = now.map(|(_, pos)| pos);
        let (now_near, now_far) = now.map_or((0.0, 1.0), |(i, pos)| spans[i].lane_at(pos));
        // Last day extends now-line through the axis; otherwise stops at
        // day's far edge along the cross/day-stack axis. Only horizontally:
        // a vertical axis strip is BEFORE the first day, not past the last
        // (da-78evn), and the renderer reaches back into it instead.
        let is_last_day = i + 1 == config.days.len();
        let now_extent_far = if now_far < 1.0 {
            day_offset + now_far * config.day_height
        } else if is_last_day && config.orientation == Orientation::Horizontal {
            day_offset + config.day_height + config.time_axis_height
        } else {
            day_offset + config.day_height
        };

        days.push(RenderedDay {
            label_lines,
            date: day.date.clone(),
            y: match config.orientation {
                Orientation::Horizontal => data_y + day_offset,
                Orientation::Vertical => data_y,
            },
            day_offset,
            has_error: !day.errors.is_empty(),
            errors,
            fields,
            now_time_pos,
            now_extent_far,
            start_ms: day.start_ms,
            end_ms: day.end_ms,
            now_extent_near: day_offset + now_near * config.day_height,
            spans,
            dead_zones,
            zone_labels,
        });
    }

    DayviewLayout {
        orientation: config.orientation,
        width,
        height,
        margin: config.margin,
        label_width,
        day_height: config.day_height,
        day_pitch: config.day_height + config.interday_margin,
        interday_margin: config.interday_margin,
        time_axis_height: config.time_axis_height,
        label_font_size,
        label_lines_shown,
        plot,
        time_axis_extent,
        days,
        axis_ticks,
        hours,
        right_panel_width: config.right_panel_width.max(0.0),
    }
}

/// Build the 25 hourly tick positions across the data-area time-axis
/// extent. Uses integer-hour boundaries 0..24 mapped proportionally —
/// independent of each day's exact UTC ms range so all days align (the
/// shared axis assumption matches the original datum).
fn hourly_axis_ticks(time_axis_extent: f64) -> Vec<DayviewAxisTick> {
    let mut ticks = Vec::with_capacity(25);
    for h in 0..=24 {
        let frac = h as f64 / 24.0;
        let time_pos = frac * time_axis_extent;
        let hour = (h % 24) as u8;
        ticks.push(DayviewAxisTick {
            time_pos,
            hour,
            label: format!("{:02}", hour),
            is_major: hour.is_multiple_of(3),
        });
    }
    ticks
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY_START: i64 = 1_745_712_000_000; // 2025-04-27T00:00:00Z
    const HOUR: i64 = 60 * 60 * 1000;

    fn empty_day(label: &[&str]) -> DayviewDayInput {
        DayviewDayInput {
            date: None,
            label_lines: label.iter().map(|s| s.to_string()).collect(),
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            errors: Vec::new(),
            fields: vec![],
            spans: Vec::new(),
        }
    }

    /// Helper: build a horizontal-orientation config (datum's original
    /// layout: time runs L→R, days stack top-to-bottom) for the legacy
    /// tests.
    fn horizontal(cfg: DayviewConfig) -> DayviewConfig {
        DayviewConfig {
            orientation: Orientation::Horizontal,
            ..cfg
        }
    }

    #[test]
    fn default_orientation_is_horizontal() {
        let cfg = DayviewConfig::default();
        assert_eq!(cfg.orientation, Orientation::Horizontal);
    }

    #[test]
    fn label_font_picks_three_lines_when_fits() {
        let cfg = horizontal(DayviewConfig {
            days: vec![empty_day(&["Mon", "Apr 27", "2026"])],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        // 100/3 - 6 = ~27.3, capped at 20 → 20.
        assert_eq!(layout.label_lines_shown, 3);
        assert!((layout.label_font_size - 20.0).abs() < 1e-6);
        assert_eq!(layout.days[0].label_lines.len(), 3);
    }

    #[test]
    fn label_font_shrinks_to_two_lines_then_one() {
        let cfg = horizontal(DayviewConfig {
            days: vec![empty_day(&["Mon", "Apr 27", "2026"])],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 18.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        assert_eq!(layout.label_lines_shown, 1);
        assert!((layout.label_font_size - 12.0).abs() < 1e-6);
        assert_eq!(layout.days[0].label_lines.len(), 1);
        assert_eq!(layout.days[0].label_lines[0], "Mon");
    }

    #[test]
    fn label_font_shrinks_to_two_lines() {
        let cfg = horizontal(DayviewConfig {
            days: vec![empty_day(&["Mon", "Apr 27", "2026"])],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 30.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        assert_eq!(layout.label_lines_shown, 2);
        assert!((layout.label_font_size - 9.0).abs() < 1e-6);
    }

    #[test]
    fn n_day_stack_height_includes_axis_and_margins() {
        let day = empty_day(&["Mon", "Apr 27", "2026"]);
        let cfg = horizontal(DayviewConfig {
            days: vec![day.clone(), day.clone(), day.clone()],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            interday_margin: 10.0,
            time_axis_height: 15.0,
            margin: 2.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        // 3*100 + 2*10 + 15 + 2*2 = 300 + 20 + 15 + 4 = 339.
        assert!(
            (layout.height - 339.0).abs() < 1e-6,
            "height was {}",
            layout.height
        );
        assert_eq!(layout.days.len(), 3);
        assert!((layout.days[0].day_offset - 0.0).abs() < 1e-6);
        assert!((layout.days[1].day_offset - 110.0).abs() < 1e-6);
        assert!((layout.days[2].day_offset - 220.0).abs() < 1e-6);
    }

    #[test]
    fn now_line_only_on_today_row() {
        let day1 = DayviewDayInput {
            date: None,
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            errors: Vec::new(),
            fields: vec![],
            spans: Vec::new(),
        };
        let day2 = DayviewDayInput {
            date: None,
            label_lines: vec!["Tue".into()],
            start_ms: DAY_START + 24 * HOUR,
            end_ms: DAY_START + 48 * HOUR,
            errors: Vec::new(),
            fields: vec![],
            spans: Vec::new(),
        };
        let now_ms = DAY_START + 24 * HOUR + 12 * HOUR;
        let cfg = horizontal(DayviewConfig {
            days: vec![day1, day2],
            now_ms,
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        assert!(
            layout.days[0].now_time_pos.is_none(),
            "day 1 should NOT have now"
        );
        assert!(
            layout.days[1].now_time_pos.is_some(),
            "day 2 should have now"
        );
    }

    #[test]
    fn now_line_extends_through_axis_on_last_day_only() {
        let day1 = DayviewDayInput {
            date: None,
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            errors: Vec::new(),
            fields: vec![],
            spans: Vec::new(),
        };
        let day2 = DayviewDayInput {
            date: None,
            label_lines: vec!["Tue".into()],
            start_ms: DAY_START + 24 * HOUR,
            end_ms: DAY_START + 48 * HOUR,
            errors: Vec::new(),
            fields: vec![],
            spans: Vec::new(),
        };
        let cfg = horizontal(DayviewConfig {
            days: vec![day1, day2],
            now_ms: DAY_START + 24 * HOUR + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            interday_margin: 15.0,
            time_axis_height: 15.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        // Last day: now_extent_far = 115 + 100 + 15 = 230.
        let last = &layout.days[1];
        assert!(
            (last.now_extent_far - 230.0).abs() < 1e-6,
            "got {}",
            last.now_extent_far
        );
    }

    #[test]
    fn empty_fields_are_skipped() {
        let day = DayviewDayInput {
            date: None,
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            errors: Vec::new(),
            fields: vec![
                DayviewFieldInput {
                    field: "empty".into(),
                    y_frac: 0.1,
                    height_frac: 0.1,
                    z_index: 0.1,
                    blocks: vec![],
                    points: vec![],
                    icon: None,
                    glyph_size: Default::default(),
                },
                DayviewFieldInput {
                    field: "has_data".into(),
                    y_frac: 0.5,
                    height_frac: 0.1,
                    z_index: 0.5,
                    blocks: vec![],
                    points: vec![DayviewPoint {
                        time_ms: DAY_START + 6 * HOUR,
                        fg_color: "#ff0000".into(),
                        state_label: None,
                        id: None,
                        title: None,
                        created_ms: None,
                        glyph: None,
                        icon: false,
                        moment: false,
                    }],
                    icon: None,
                    glyph_size: Default::default(),
                },
            ],
            spans: Vec::new(),
        };
        let cfg = horizontal(DayviewConfig {
            days: vec![day],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        let names: Vec<&str> = layout.days[0]
            .fields
            .iter()
            .map(|f| f.field.as_str())
            .collect();
        assert_eq!(names, vec!["has_data"]);
    }

    #[test]
    fn fields_render_in_z_index_order_low_to_high() {
        let mk_field = |name: &str, z: f64| DayviewFieldInput {
            field: name.into(),
            y_frac: 0.0,
            height_frac: 0.1,
            z_index: z,
            blocks: vec![DayviewBlock {
                start_ms: DAY_START + HOUR,
                end_ms: DAY_START + 2 * HOUR,
                fill: BlockFill::Single("#abcdef".into()),
                state_label: None,
                id: None,
                title: None,
                created_ms: None,
            }],
            points: vec![],
            icon: None,
            glyph_size: Default::default(),
        };
        let day = DayviewDayInput {
            date: None,
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            errors: Vec::new(),
            fields: vec![mk_field("c", 0.9), mk_field("a", 0.1), mk_field("b", 0.5)],
            spans: Vec::new(),
        };
        let cfg = horizontal(DayviewConfig {
            days: vec![day],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        let order: Vec<&str> = layout.days[0]
            .fields
            .iter()
            .map(|f| f.field.as_str())
            .collect();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn blocks_clip_to_day_window() {
        let day = DayviewDayInput {
            date: None,
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            errors: Vec::new(),
            fields: vec![DayviewFieldInput {
                field: "long".into(),
                y_frac: 0.0,
                height_frac: 0.1,
                z_index: 0.0,
                blocks: vec![DayviewBlock {
                    start_ms: DAY_START - HOUR,
                    end_ms: DAY_START + 25 * HOUR,
                    fill: BlockFill::Single("#abcdef".into()),
                    state_label: None,
                    id: None,
                    title: None,
                    created_ms: None,
                }],
                points: vec![],
                icon: None,
                glyph_size: Default::default(),
            }],
            spans: Vec::new(),
        };
        let cfg = horizontal(DayviewConfig {
            days: vec![day],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            label_width: Some(0.0),
            margin: 0.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        let block = &layout.days[0].fields[0].blocks[0];
        // Should start at time_offset=0 (clipped to day start).
        assert!(
            block.time_offset.abs() < 1e-6,
            "time_offset was {}",
            block.time_offset
        );
        // And span the full data width (800 since label_width=0, margin=0).
        assert!(
            (block.time_extent - 800.0).abs() < 1.0,
            "time_extent was {}",
            block.time_extent
        );
    }

    #[test]
    fn axis_ticks_emit_25_hourly_positions() {
        let cfg = horizontal(DayviewConfig {
            days: vec![empty_day(&["Mon"])],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            label_width: Some(0.0),
            margin: 0.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        assert_eq!(layout.axis_ticks.len(), 25);
        assert_eq!(layout.axis_ticks[0].hour, 0);
        assert_eq!(layout.axis_ticks[12].hour, 12);
        // 12:00 → time_pos = 800 * 12/24 = 400.
        assert!((layout.axis_ticks[12].time_pos - 400.0).abs() < 1e-6);
        assert_eq!(layout.axis_ticks[24].hour, 0);
    }

    #[test]
    fn multi_state_block_preserves_stripe_colors() {
        let day = DayviewDayInput {
            date: None,
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            errors: Vec::new(),
            fields: vec![DayviewFieldInput {
                field: "mood".into(),
                y_frac: 0.0,
                height_frac: 0.1,
                z_index: 0.0,
                blocks: vec![DayviewBlock {
                    start_ms: DAY_START + 6 * HOUR,
                    end_ms: DAY_START + 7 * HOUR,
                    fill: BlockFill::MultiState(vec![
                        "#ff0000".into(),
                        "#00ff00".into(),
                        "#0000ff".into(),
                    ]),
                    state_label: Some("happy,calm,focused".into()),
                    id: None,
                    title: None,
                    created_ms: None,
                }],
                points: vec![],
                icon: None,
                glyph_size: Default::default(),
            }],
            spans: Vec::new(),
        };
        let cfg = horizontal(DayviewConfig {
            days: vec![day],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        });
        let layout = compute_dayview(&cfg);
        let block = &layout.days[0].fields[0].blocks[0];
        match &block.fill {
            BlockFill::MultiState(colors) => {
                assert_eq!(
                    colors,
                    &vec![
                        "#ff0000".to_string(),
                        "#00ff00".to_string(),
                        "#0000ff".to_string(),
                    ]
                );
            }
            _ => panic!("expected MultiState fill"),
        }
    }

    #[test]
    fn errors_are_placed_on_the_day() {
        // da-0lful: a finding is placed at its instant, whether or not its
        // field draws a track that day; one outside the day is not drawn.
        let mut day = empty_day(&["Mon", "Apr 27", "2026"]);
        day.fields = vec![DayviewFieldInput {
            field: "mood".into(),
            y_frac: 0.5,
            height_frac: 0.2,
            z_index: 0.5,
            blocks: vec![],
            points: vec![DayviewPoint {
                time_ms: DAY_START + 6 * HOUR,
                fg_color: "#fff".into(),
                state_label: None,
                id: None,
                title: None,
                created_ms: None,
                glyph: None,
                icon: false,
                moment: false,
            }],
            icon: None,
            glyph_size: Default::default(),
        }];
        let error = |field: &str, time_ms| DayviewError {
            time_ms,
            field: field.into(),
            what: "opened while already open".into(),
        };
        day.errors = vec![
            error("mood", DAY_START + 6 * HOUR),
            error("work", DAY_START + 18 * HOUR),
            error("mood", DAY_START + 30 * HOUR),
        ];
        let layout = compute_dayview(&horizontal(DayviewConfig {
            days: vec![day, empty_day(&["Tue"])],
            now_ms: DAY_START + 12 * HOUR,
            width: 800.0,
            day_height: 100.0,
            ..Default::default()
        }));
        let (marked, clean) = (&layout.days[0], &layout.days[1]);
        assert!(marked.has_error && !clean.has_error);
        let extent = layout.time_axis_extent;
        let placed: Vec<_> = (marked.errors.iter())
            .map(|e| (e.field.as_str(), e.time_pos / extent))
            .collect();
        assert_eq!(placed, [("mood", 0.25), ("work", 0.75)]);
    }

    #[test]
    fn vertical_orientation_propagates_to_layout() {
        let cfg = DayviewConfig {
            days: vec![empty_day(&["Mon", "Apr 27", "2026"])],
            now_ms: DAY_START + 12 * HOUR,
            day_height: 100.0,
            orientation: Orientation::Vertical,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        assert_eq!(layout.orientation, Orientation::Vertical);
    }

    #[test]
    fn vertical_total_width_derives_from_day_stack() {
        // 3 days × 100 + 2 × 15 (interday) + 15 (time-axis strip) + 2 × 2 (margin)
        // = 300 + 30 + 15 + 4 = 349.
        let day = empty_day(&["Mon"]);
        let cfg = DayviewConfig {
            days: vec![day.clone(), day.clone(), day.clone()],
            now_ms: DAY_START + 12 * HOUR,
            day_height: 100.0,
            interday_margin: 15.0,
            time_axis_height: 15.0,
            margin: 2.0,
            height: Some(800.0),
            orientation: Orientation::Vertical,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        assert!(
            (layout.width - 349.0).abs() < 1e-6,
            "width was {}",
            layout.width
        );
        // Total height is the explicit `height` config.
        assert!(
            (layout.height - 800.0).abs() < 1e-6,
            "height was {}",
            layout.height
        );
        // Days stack on the cross axis in 100+15 increments.
        assert!((layout.days[0].day_offset - 0.0).abs() < 1e-6);
        assert!((layout.days[1].day_offset - 115.0).abs() < 1e-6);
        assert!((layout.days[2].day_offset - 230.0).abs() < 1e-6);
    }

    #[test]
    fn vertical_time_axis_extent_is_data_height() {
        // height 800 - 2*margin(2) - label_width(label_font*4 ~=80) = 716.
        let cfg = DayviewConfig {
            days: vec![empty_day(&["Mon", "Apr 27", "2026"])],
            now_ms: DAY_START + 12 * HOUR,
            day_height: 100.0,
            margin: 2.0,
            height: Some(800.0),
            orientation: Orientation::Vertical,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        let expected = 800.0 - 4.0 - layout.label_width;
        assert!(
            (layout.time_axis_extent - expected).abs() < 1e-6,
            "time_axis_extent was {}, expected {}",
            layout.time_axis_extent,
            expected
        );
        // Axis ticks: 12:00 → time_pos = expected * 0.5.
        assert!((layout.axis_ticks[12].time_pos - expected / 2.0).abs() < 1e-6);
    }

    #[test]
    fn vertical_blocks_clip_to_day_window() {
        let day = DayviewDayInput {
            date: None,
            label_lines: vec!["Mon".into()],
            start_ms: DAY_START,
            end_ms: DAY_START + 24 * HOUR,
            errors: Vec::new(),
            fields: vec![DayviewFieldInput {
                field: "long".into(),
                y_frac: 0.0,
                height_frac: 0.1,
                z_index: 0.0,
                blocks: vec![DayviewBlock {
                    start_ms: DAY_START - HOUR,
                    end_ms: DAY_START + 25 * HOUR,
                    fill: BlockFill::Single("#abcdef".into()),
                    state_label: None,
                    id: None,
                    title: None,
                    created_ms: None,
                }],
                points: vec![],
                icon: None,
                glyph_size: Default::default(),
            }],
            spans: Vec::new(),
        };
        let cfg = DayviewConfig {
            days: vec![day],
            now_ms: DAY_START + 12 * HOUR,
            day_height: 100.0,
            margin: 0.0,
            label_width: Some(0.0),
            time_axis_height: 0.0,
            height: Some(800.0),
            orientation: Orientation::Vertical,
            ..Default::default()
        };
        let layout = compute_dayview(&cfg);
        let block = &layout.days[0].fields[0].blocks[0];
        assert!(
            block.time_offset.abs() < 1e-6,
            "time_offset was {}",
            block.time_offset
        );
        assert!(
            (block.time_extent - 800.0).abs() < 1.0,
            "time_extent was {}",
            block.time_extent
        );
    }

    /// A stretch that begins `start_h` hours of real time into the day,
    /// its wall clock reading `lo_h` → `hi_h`.
    fn span(start_h: i64, lo_h: i64, hi_h: i64, label: &str) -> DayviewSpan {
        DayviewSpan {
            start_ms: DAY_START + start_h * HOUR,
            end_ms: DAY_START + (start_h + hi_h - lo_h) * HOUR,
            lo: lo_h as f64 / 24.0,
            hi: hi_h as f64 / 24.0,
            label: label.into(),
            offset_s: None,
        }
    }

    /// A day cut into `spans`, at 100px an hour and 100px tall: one field
    /// over its near half, with a block from 1h to 4h of real time and a
    /// dot, and now, `at_h` hours in.
    fn jumped(spans: Vec<DayviewSpan>, at_h: f64) -> RenderedDay {
        let at = |h: f64| DAY_START + (h * HOUR as f64) as i64;
        let day = DayviewDayInput {
            date: None,
            label_lines: vec![],
            start_ms: DAY_START,
            end_ms: spans[spans.len() - 1].end_ms,
            errors: Vec::new(),
            fields: vec![DayviewFieldInput {
                field: "sleep".into(),
                y_frac: 0.0,
                height_frac: 0.5,
                z_index: 0.0,
                blocks: vec![DayviewBlock {
                    start_ms: at(1.0),
                    end_ms: at(4.0),
                    fill: BlockFill::Single("#111111".into()),
                    state_label: None,
                    id: None,
                    title: None,
                    created_ms: None,
                }],
                points: vec![DayviewPoint {
                    time_ms: at(at_h),
                    fg_color: "#ffffff".into(),
                    state_label: None,
                    id: None,
                    title: None,
                    created_ms: None,
                    glyph: None,
                    icon: false,
                    moment: false,
                }],
                icon: None,
                glyph_size: Default::default(),
            }],
            spans,
        };
        let cfg = DayviewConfig {
            days: vec![day],
            now_ms: at(at_h),
            width: 2400.0,
            day_height: 100.0,
            margin: 0.0,
            label_width: Some(0.0),
            time_axis_height: 0.0,
            ..Default::default()
        };
        compute_dayview(&cfg).days.remove(0)
    }

    /// Pixels to the hundredth, so a position reads as the hour it is.
    fn px(n: f64) -> f64 {
        (n * 100.0).round() / 100.0
    }

    fn pieces(day: &RenderedDay) -> Vec<(usize, f64, f64)> {
        (day.fields[0].blocks.iter())
            .map(|b| (b.span, px(b.time_offset), px(b.time_extent)))
            .collect()
    }

    fn labels(day: &RenderedDay) -> Vec<(&str, f64, f64)> {
        (day.zone_labels.iter())
            .map(|l| (l.text.as_str(), px(l.time_pos), l.cross_pos))
            .collect()
    }

    /// Berlin, 2026-03-29: the clock went from 02:00 to 03:00.
    #[test]
    fn a_forward_jump_is_a_dead_zone_labelled_at_each_end() {
        let day = jumped(vec![span(0, 0, 2, "UTC+1"), span(2, 3, 24, "UTC+2")], 2.5);
        let dead: Vec<[f64; 2]> = day.dead_zones.iter().map(|z| z.map(px)).collect();
        assert_eq!(dead, [[200.0, 100.0]]);
        // Each end's offset in the zone, the earlier one nearest, across
        // the band with its foot 1.5px from the end it names: 11px text
        // whose baseline is 0.35em from its middle.
        assert_eq!(
            labels(&day),
            [("UTC+1", 205.35, 25.0), ("UTC+2", 294.65, 75.0)]
        );
        let feet: Vec<bool> = day.zone_labels.iter().map(|l| l.names_earlier).collect();
        assert_eq!(feet, [true, false], "each foot toward its own stretch");
        // The block stops at 02:00 and goes on from 03:00; half an hour
        // after the jump is 03:30, where the dot and the now-line are.
        assert_eq!(pieces(&day), [(0, 100.0, 100.0), (1, 300.0, 200.0)]);
        let dot = &day.fields[0].points[0];
        assert_eq!((dot.span, px(dot.time_pos)), (1, 350.0));
        assert_eq!(day.now_time_pos.map(px), Some(350.0));
        assert!(day.spans.iter().all(|s| s.lane.is_empty()), "no fork");
    }

    /// Berlin, 2026-10-25: the clock went from 03:00 back to 02:00.
    #[test]
    fn a_backward_jump_forks_the_band_into_a_lane_for_each_occurrence() {
        let spans = vec![span(0, 0, 3, "UTC+2"), span(3, 2, 24, "UTC+1")];
        let (first, second) = (jumped(spans.clone(), 2.5), jumped(spans, 3.5));
        // 02:30 happened twice: the same place along the axis, a lane each,
        // 5% of the band between them.
        for (day, lane) in [(&first, 0), (&second, 1)] {
            let dot = &day.fields[0].points[0];
            assert_eq!((dot.span, px(dot.time_pos)), (lane, 250.0));
            assert_eq!(day.now_time_pos.map(px), Some(250.0));
        }
        let lane = |s: usize, at: f64| first.spans[s].lane_at(at);
        assert_eq!(
            (lane(0, 250.0), lane(1, 250.0)),
            ((0.0, 0.475), (0.525, 1.0))
        );
        // Whole again away from the fork, which each S leaves a label's
        // room beside: 1.5px of air at the break, 11px text's capitals
        // across the band (8.25px), and a label's width, 11px, between
        // them and the S.
        assert_eq!((lane(0, 0.0), lane(1, 2400.0)), ((0.0, 1.0), (0.0, 1.0)));
        let knots = |s: usize| -> Vec<[f64; 3]> {
            (first.spans[s].lane.iter().map(|k| k.map(px))).collect()
        };
        let (narrow, widen) = (knots(0), knots(1));
        assert_eq!(
            [narrow[0], narrow[8], widen[0], widen[8]],
            [
                [145.92, 0.0, 1.0],
                [179.25, 0.0, 0.48],
                [320.75, 0.53, 1.0],
                [354.08, 0.0, 1.0]
            ]
        );
        // Each over a 72nd of the axis, 20 minutes of a whole day, and
        // on an S: a quarter of the way along it has gone 5/32 of the way
        // across, not a quarter, and halfway along, half: of the 0.525 the
        // first lane's far edge comes in.
        assert_eq!(narrow.len(), 9);
        assert_eq!(px(narrow[8][0] - narrow[0][0]), px(2400.0 / 72.0));
        assert_eq!(px(widen[8][0] - widen[0][0]), px(2400.0 / 72.0));
        assert_eq!(
            [narrow[2][2], narrow[4][2]],
            [px(1.0 - 0.525 * 5.0 / 32.0), px(0.7375)]
        );
        assert_eq!(
            labels(&first),
            [("UTC+2", 305.35, 23.75), ("UTC+1", 194.65, 76.25)]
        );
        let feet: Vec<bool> = first.zone_labels.iter().map(|l| l.names_earlier).collect();
        assert_eq!(feet, [true, false], "each foot toward its own stretch");
        // The block runs to 03:00 in the first lane and from 02:00 in the
        // second, and no clock time is dead.
        assert_eq!(pieces(&first), [(0, 100.0, 200.0), (1, 200.0, 100.0)]);
        assert!(first.dead_zones.is_empty());
        // The now-line keeps to the lane now is in.
        assert_eq!((first.now_extent_near, first.now_extent_far), (0.0, 47.5));
        assert_eq!(
            (second.now_extent_near, second.now_extent_far),
            (52.5, 100.0)
        );
    }

    /// On an axis with no room beside an end, 48px here for a label 14px
    /// wide with its air, the label stays on the axis, over its own
    /// stretch: the one that starts at 02:00 (4px), and the one that ends
    /// at 23:00 (46px).
    #[test]
    fn a_zone_label_never_leaves_the_axis() {
        let at = |spans: &[DayviewSpan]| -> Vec<f64> {
            let (_, _, labels) = place_spans(spans, 48.0, 100.0);
            labels.iter().map(|l| px(l.time_pos)).collect()
        };
        let early = [span(0, 0, 3, "UTC+2"), span(3, 2, 24, "UTC+1")];
        assert_eq!(at(&early), [11.35, 7.0]);
        let late = [span(0, 0, 23, "UTC+2"), span(23, 22, 24, "UTC+1")];
        assert_eq!(at(&late), [41.0, 38.65]);
    }

    /// Two jumps back whose repeats overlap: 06:00 → 04:00, then 08:00 →
    /// 05:00, so 05:00–06:00 happened three times.
    #[test]
    fn clock_times_that_happened_n_times_get_n_lanes() {
        let spans = vec![span(0, 0, 6, "A"), span(6, 4, 8, "B"), span(10, 5, 24, "C")];
        let day = jumped(spans, 0.5);
        // Each 30% of the band, 5% between two.
        let lane = |s: usize| {
            let (near, far) = day.spans[s].lane_at(550.0);
            (px(near), px(far))
        };
        assert_eq!(
            [lane(0), lane(1), lane(2)],
            [(0.0, 0.3), (0.35, 0.65), (0.7, 1.0)]
        );
    }
}
