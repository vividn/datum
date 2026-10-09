//! Per-verb write-action renderer.
//!
//! Implements the layout described in `docs/visual-parity-analysis.md` §§2, 6, 7,
//! revised by `da-fck` (the state-transition group):
//!
//! ```text
//! <time>  <field-badge>  <LEAD><state-cell><TRAIL>  [duration]
//! ```
//!
//! The state-transition group is `[LEAD][state cell][TRAIL]`, with no
//! whitespace inside the trio. The state cell is the existing bg-coloured
//! badge. The LEAD and TRAIL slots are single literal spaces painted with
//! the prior / continuing state's colour, replacing the old directional
//! arrows. When no prior or continuing state exists (or it's IGNORED), the
//! slot falls back to a lighter-grey bg with the `∅` glyph as fg.
//!
//! See `crates/datum-cli/tests/render_state_group.rs` for the worked
//! examples encoded as fixtures.

use chrono::{FixedOffset, Timelike, Utc};
use datum_core::db::Db;
use datum_core::state::FieldStateRows;
use datum_core::{
    entry::{Entry, EntryShape},
    field_spec::{FieldSpec, SpecRegistry},
    shape::{hands_field_back, has_state_badge, TRACKED_STATE_VALUE},
    time::{DatumTime, Zone},
};
use datum_parse::time::{format_human_duration, DurationPrecision};
use serde_json::Value as JsonValue;

use super::characters::*;
use super::colors::*;

// ─── Relative-day prefix (single source of truth) ────────────────────────────

/// Datum-style relative-day prefix per `humanTime.ts`:
///
/// | target's local date vs. now's | prefix       |
/// |---                            |---           |
/// | same calendar day             | `None`       |
/// | 1–3 days back                 | `-Nd`        |
/// | 4+ days back                  | `YYYY-MM-DD` |
/// | 1–3 days ahead                | `+Nd`        |
/// | 4+ days ahead                 | `YYYY-MM-DD` |
///
/// Both timestamps are projected onto **target's** local calendar (the
/// `offset_minutes` recorded with the row) so "today" follows the row's
/// timezone rather than the host's. `now` is supplied by the caller so
/// callers control "what today means" (testable + zone-correct).
pub fn format_relative_day_prefix(target: &DatumTime, now: &DatumTime) -> Option<String> {
    let off = FixedOffset::east_opt(target.offset_minutes * 60)
        .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    let target_date = target.utc.with_timezone(&off).date_naive();
    let now_date = now.utc.with_timezone(&off).date_naive();
    let delta = (target_date - now_date).num_days();
    if delta == 0 {
        return None;
    }
    if delta.abs() <= 3 {
        Some(if delta < 0 {
            format!("-{}d", -delta)
        } else {
            format!("+{delta}d")
        })
    } else {
        Some(target_date.format("%Y-%m-%d").to_string())
    }
}

/// The one place the host's clock enters the renderer. Every `…_at`
/// function takes `now` as a parameter; these conveniences supply it from the
/// local clock.
pub(crate) fn now_local() -> DatumTime {
    DatumTime::now()
}

// ─── Render context ──────────────────────────────────────────────────────────

/// Caller-supplied rendering context — slot bg-colour inputs + first-time hint.
///
/// * `lead_state` — the field's state at `start - ε` with the row left
///   out ([`FieldStateRows::at`]). Drives the LEAD slot's bg colour. `None` = no prior
///   history; the slot falls back to grey-`∅`.
/// * `trail_state` — the state continuing forward from this row. For a
///   `Block` shape, this is the field's state at `end_time` with the row
///   left out — what resumes after the block closes. For `Point`/
///   `Ongoing`, it's the row's own emitted state (the new switch value,
///   `Bool(true)` for start, `Bool(false)` for stop). Drives the TRAIL
///   slot's bg colour, with the same grey-`∅` fallback.
/// * `prior_state_ignored` — the field's compiled state just before this
///   row is absent or IGNORED: the LEAD slot's grey-`∅` rule, carried to
///   the bare occur dot, which has no LEAD slot of its own. Derived from
///   `lead_state` for the state-bearing verbs and from `ongoing_state` for
///   the dot, never from a row count.
/// * `ongoing_state` — the field's compiled state at the row's own time,
///   excluding the row itself. Drives the bg of an occur dot per
///   `da-k5z`. State-transition verbs (switch / start / stop / block) use
///   `lead_state` and `trail_state` instead.
/// * `is_state_repeat` — set when the row's emitted state equals
///   `lead_state` (per `integrity::states_equal_set_aware`).
///   Drives the LEAD `!` warning marker (`da-qfv0`): the visual signal
///   that a switch left the field's compiled state unchanged, which most
///   often means an intermediate transition was forgotten and the user
///   should backfill it via `-t` while it's still in working memory.
#[derive(Clone, Debug, Default)]
pub struct RenderContext {
    pub lead_state: Option<JsonValue>,
    pub trail_state: Option<JsonValue>,
    pub prior_state_ignored: bool,
    pub ongoing_state: Option<JsonValue>,
    pub is_state_repeat: bool,
    /// da-8sx8: the row's `_N` quickId ordinal, when it has one. Only the
    /// listing surfaces fill this (see `build_table_contexts`); everywhere
    /// else it stays `None` and the table's `_n` column drops out.
    pub quick_ordinal: Option<usize>,
    /// da-uxgv: another row lost an identity contest to this one and is
    /// sitting superseded behind it. The table marks such a row with a
    /// trailing `*` on its hid, so a conflict is met where the user is
    /// already looking rather than in a command they have to remember.
    /// Only the listing surfaces fill this (see `build_table_contexts`).
    pub has_supersedees: bool,
}

/// Which renderer is asking for the context. Per da-9p2u: pick the
/// per-surface lead/trail/first-occurrence semantics in exactly one place
/// instead of letting every call site re-derive them ad-hoc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// Write-action two-line header (`CREATE`/`UPDATE`/…) printed by
    /// `cmd::log_entry` and friends.
    Action,
    /// Tail/grep/head table row.
    Table,
    /// Bare `format_info_line_at` consumers that don't supply their own
    /// transition history — renders the entry without a state-transition
    /// group.
    InfoLine,
    /// `datum now` overview rows. State-only display; no lead/trail or
    /// first-occurrence marker.
    NowView,
}

impl RenderContext {
    /// Canonical [`RenderContext`] constructor — the single entry point all
    /// renderer surfaces route through. Picks the right lead/trail/ongoing
    /// probes and `prior_state_ignored` semantics from `surface`.
    ///
    /// `now` is reserved for surface arms that need "is this row in the
    /// future relative to now" semantics (none today). It's accepted so
    /// callers don't have to retrofit when an arm starts using it.
    ///
    /// Batched callers (table renderer, `datum tail`) load the field's
    /// state rows once per field and call
    /// [`RenderContext::from_rows`] directly, which shares the same
    /// per-row logic as the `Action`/`Table` arms here.
    pub fn for_entry(
        db: &Db,
        entry: &Entry,
        surface: Surface,
        _now: Option<&DatumTime>,
    ) -> anyhow::Result<RenderContext> {
        Ok(match surface {
            Surface::InfoLine | Surface::NowView => RenderContext::default(),
            Surface::Action | Surface::Table => {
                let rows = probed_state_rows(db, &entry.field, [entry]);
                Self::from_rows(Some(&rows), entry)
            }
        })
    }

    /// Cached variant of [`RenderContext::for_entry`] for batched render
    /// paths, and the whole of the context for callers without a database:
    /// everything here is derived from the field's state rows and the row.
    /// Reuses the field's rows, read once, so a `tail` over N rows of the
    /// same field issues one state read instead of N.
    ///
    /// Pass `None` for `rows` when the entry's verb doesn't need a
    /// state-history probe (`Add`/`Note`); the helpers degrade to a
    /// default-context build.
    pub fn from_rows(rows: Option<&FieldStateRows>, entry: &Entry) -> RenderContext {
        let lead_state = compute_lead_state(rows, entry);
        let trail_state = compute_trail_state(rows, entry);
        let ongoing_state = compute_ongoing_state(rows, entry);
        // The `∅` slot follows one rule on every surface: the field's compiled
        // state just before this row is absent or IGNORED. The state-bearing
        // verbs read that off `lead_state` in their LEAD slot; the bare dot
        // reads the ambient state, so it takes the same predicate from there.
        let prior = if has_state_badge(entry.verb, entry.dur.as_ref()) {
            lead_state.clone()
        } else {
            ongoing_state.clone()
        };
        let prior_state_ignored = StateTransition {
            previous: prior,
            ..StateTransition::default()
        }
        .previous_is_ignored();
        let is_state_repeat = compute_is_state_repeat(lead_state.as_ref(), entry);
        RenderContext {
            lead_state,
            trail_state,
            prior_state_ignored,
            ongoing_state,
            is_state_repeat,
            quick_ordinal: None,
            has_supersedees: false,
        }
    }
}

/// Whether this row draws a state badge rather than a bare dot —
/// [`datum_core::shape::has_state_badge`] for a caller that holds the whole
/// entry.
///
/// It exists so the three places da-fknn8 added read the dispatch from ONE
/// of them: [`StateTransition::of`], [`format_state_body`], and
/// `tail_rows`' timeless gate are all there to MIRROR
/// [`format_state_group`], and a row the painter draws as a dot while the
/// JSON reports it as a badge is precisely the drift this bead removes. One
/// call, so the mirror cannot be half-adjusted.
///
/// ⚠ da-gvcen (#68, landed as `f81a101c`) made this predicate
/// `has_state_badge(verb, dur)`, because a MOMENT is a point whatever else
/// it carries — a first-log `occur newfield` is a `PointSwitch` that must
/// still draw the dot. **The `dur` is not optional here**: passing `None`
/// compiles and silently puts exactly those rows back on the badge branch,
/// which is the whole of what da-gvcen changed.
pub fn draws_state_badge(entry: &Entry) -> bool {
    has_state_badge(entry.verb, entry.dur.as_ref())
}

/// A row's state transition as three values — what the field was in just
/// before this row, what this row itself says, and what continues forward
/// from it (da-fknn8).
///
/// This trio is what the `[LEAD][body][TRAIL]` group has always drawn. Naming
/// it makes it DATA: `tail --json` carries the same three values the painter
/// composes from, so a consumer reads a transition instead of recovering it
/// from background colours. The painter and the JSON row therefore cannot
/// disagree — both call [`StateTransition::of`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StateTransition {
    /// The field's state immediately before this row's leading edge.
    /// `None` where the field had no tracked state there, which reads as
    /// IGNORED (see [`StateTransition::previous_is_ignored`]).
    pub previous: Option<JsonValue>,
    /// The row's own state — `Entry::state`, unchanged.
    pub current: Option<JsonValue>,
    /// The state that continues forward past this row. For a block, what
    /// resumes when it closes; for a block-switch / point-switch, the
    /// state it switches TO (its own `following_state`); for a
    /// point-shaped switch, the row's own emitted state.
    pub following: Option<JsonValue>,
}

impl StateTransition {
    /// The trio for one row, read off the context the renderer already
    /// builds. The LEAD/TRAIL probes exist only on the state-badge verbs;
    /// the bare dot has no slots of its own, so the field's ambient state
    /// at the row's time is both what came before and what continues —
    /// unless the dot itself switches the field ([`switched_to`]).
    pub fn of(ctx: &RenderContext, entry: &Entry) -> StateTransition {
        if draws_state_badge(entry) {
            StateTransition {
                previous: ctx.lead_state.clone(),
                current: entry.state.clone(),
                following: ctx.trail_state.clone(),
            }
        } else {
            StateTransition {
                previous: ctx.ongoing_state.clone(),
                current: entry.state.clone(),
                // A first log's dot declares its field past it (da-gvcen).
                following: switched_to(entry).or_else(|| ctx.ongoing_state.clone()),
            }
        }
    }

    /// The `∅` rule as a predicate on the data: the state before this row
    /// is absent or IGNORED. [`RenderContext::prior_state_ignored`] is this
    /// value, computed once where the context is built.
    pub fn previous_is_ignored(&self) -> bool {
        self.previous.as_ref().is_none_or(|v| {
            datum_core::state::normalize_state(v) == datum_core::state::StateValue::Ignored
        })
    }
}

/// True when the trigger's emitted state equals `prior` (per
/// [`datum_core::integrity::states_equal_set_aware`]). Mirrors
/// `list_repeated_states` so the visual `!` marker fires on exactly the
/// same shape as the `SameStateRepeat` warning — including da-g1b8's
/// rule that adding an element to a parallel state (`"vertical"` →
/// `["vertical","sit"]`) is a transition, not a repeat.
///
/// Non-state-bearing verbs (`Point`, `Note`, `Add`) always return `false`
/// — they don't drive a state cell to mark. A `None` prior or a missing
/// row state likewise can't form a repeat.
pub(crate) fn compute_is_state_repeat(prior: Option<&JsonValue>, row: &Entry) -> bool {
    if !has_state_badge(row.verb, row.dur.as_ref()) {
        return false;
    }
    let (Some(prior), Some(row_state)) = (prior, row.state.as_ref()) else {
        return false;
    };
    let prior_norm = datum_core::state::normalize_state(prior).to_json();
    let row_norm = datum_core::state::normalize_state(row_state).to_json();
    datum_core::state::states_equal_set_aware(&row_norm, &prior_norm)
}

/// The rows of `field` that [`RenderContext::from_rows`] reads for
/// `entries`, all of that field: every probe below lies between the
/// earliest leading edge (less the lead probe's microsecond) and the latest
/// `occur`, and [`FieldStateRows::load_between`] answers there as the whole
/// history would — at the cost of the span, not of the field's history
/// (da-asgt4). Declarations are among them (da-ycbyx): the row after a
/// track row comes from TRACKED, which is what `state_at` says of it.
pub fn probed_state_rows<'a>(
    db: &Db,
    field: &str,
    entries: impl IntoIterator<Item = &'a Entry>,
) -> FieldStateRows {
    let span = entries
        .into_iter()
        .filter_map(|e| Some((e.leading_edge_utc()?, e.occur_time.as_ref()?.utc)))
        .reduce(|(lo, hi), (l, h)| (lo.min(l), hi.max(h)));
    let Some((lo, hi)) = span else {
        return FieldStateRows::default();
    };
    let lo = lo - chrono::Duration::microseconds(1);
    FieldStateRows::load_between(db.conn(), field, lo, hi).unwrap_or_default()
}

fn compute_lead_state(rows: Option<&FieldStateRows>, entry: &Entry) -> Option<JsonValue> {
    // The LEAD/TRAIL slots exist only on the state-badge group, so the
    // gate is exactly `has_state_badge` — shape.rs owns the verb set.
    if !has_state_badge(entry.verb, entry.dur.as_ref()) {
        return None;
    }
    // da-zwct: a block's duration PRECEDES `occur_time`, so the state
    // it walked in on sits just before `occur - dur`. `leading_edge_utc`
    // is `occur` for every shape that spans nothing.
    let at = entry.leading_edge_utc()?;
    let probe = at - chrono::Duration::microseconds(1);
    rows?.at(probe, Some(&entry.hid))
}

fn compute_trail_state(rows: Option<&FieldStateRows>, entry: &Entry) -> Option<JsonValue> {
    if !has_state_badge(entry.verb, entry.dur.as_ref()) {
        return None;
    }
    if let Some(to) = switched_to(entry) {
        return Some(to);
    }
    if matches!(entry.shape(), EntryShape::Block) {
        // da-zwct: the block's END is the stored `occur_time` itself —
        // the duration ran up to it — so the trail probe needs no
        // arithmetic and no zone table at all.
        let end = entry.occur_time.as_ref()?.utc;
        return rows?.at(end, Some(&entry.hid));
    }
    entry_self_trail_state(entry)
}

/// The state a row SWITCHES TO past itself, when it names one (da-qxmqe):
/// a block-switch / point-switch carries it in its own `following_state`,
/// and that is what continues forward — not what it interrupted, and not
/// the ambient state around a dot. `None` for a row that hands the field
/// back ([`hands_field_back`]) or names nothing.
fn switched_to(entry: &Entry) -> Option<JsonValue> {
    entry
        .following_state
        .clone()
        .filter(|_| !hands_field_back(entry.verb))
}

fn compute_ongoing_state(rows: Option<&FieldStateRows>, entry: &Entry) -> Option<JsonValue> {
    // Ongoing-state probes apply only to the badge-less verb (the bare
    // dot reads ambient state) — exactly the complement of the badge set.
    if has_state_badge(entry.verb, entry.dur.as_ref()) {
        return None;
    }
    let at = entry.occur_time.as_ref()?.utc;
    rows?.at(at, Some(&entry.hid))
}

// ─── State-transition group (§6 / §7) ────────────────────────────────────────

/// Render the `<left-char><state-badge><right-char>` group for an entry.
///
/// The field badge is rendered separately and prepended in the info line.
/// Tail and other table consumers route the state cell through this so
/// every surface that shows state shares the same left/state/right group.
pub fn format_state_group(
    entry: &Entry,
    spec: Option<&FieldSpec>,
    ctx: &RenderContext,
) -> Option<String> {
    // Timeless rows (legacy `add` rows with no occur_time) keep the
    // NON_OCCUR glyph. The dispatch table in `datum_core::shape` doesn't
    // distinguish timeless points; we gate it here before consulting the
    // verb-keyed renderer split.
    if entry.occur_time.is_none() && !has_state_badge(entry.verb, entry.dur.as_ref()) {
        return Some(render_timeless(entry, spec));
    }
    // State-bearing verbs render the `[LEAD][cell][TRAIL]` group; the
    // lone non-stateful verb (`Verb::Point`) renders a bare dot. Adding
    // a new verb only updates `has_state_badge` in `shape.rs`.
    if has_state_badge(entry.verb, entry.dur.as_ref()) {
        Some(render_state_transition_group(entry, spec, ctx))
    } else {
        Some(render_occur(entry, spec, ctx))
    }
}

// ─── Per-verb renderers ──────────────────────────────────────────────────────

/// A timeless point — note-data (`occur f -T`, `switch f s -T`): `¢`, then
/// the state it names, if any, as the moment-state label. Both are painted
/// in the state's colour as fg (the field's when the state paints none, or
/// when there is no state); no LEAD/TRAIL, since nothing transitions (da-w149).
fn render_timeless(entry: &Entry, spec: Option<&FieldSpec>) -> String {
    let field_color = || field_rgb(&entry.field, spec);
    let Some(state) = entry.state.as_ref() else {
        return paint(&field_color().fg(), NON_OCCUR);
    };
    let color = state_value_rgb(state, &entry.field, spec).unwrap_or_else(field_color);
    paint(&color.fg(), NON_OCCUR) + &render_point_state_label(state, &entry.field, spec)
}

fn render_occur(entry: &Entry, spec: Option<&FieldSpec>, ctx: &RenderContext) -> String {
    let mut out = String::new();
    // Occur: `∅` when the field's prior state is absent or IGNORED —
    // the LEAD slot's rule, applied to the dot; otherwise no left-char.
    // It is the PREVIOUS state speaking, not this row's (da-fknn8), which
    // is why it is not part of the body below.
    if ctx.prior_state_ignored {
        out.push_str(&paint(GRAY_FG, NULL_SET));
    }
    out.push_str(&render_occur_body(entry, spec, ctx));
    out
}

fn render_occur_body(entry: &Entry, spec: Option<&FieldSpec>, ctx: &RenderContext) -> String {
    let mut out = String::new();
    // Dot: field/state color as fg; bg encodes the field's ambient
    // state at the row's time (ongoing block / IGNORED tracking / none).
    let (fg, bg) = compute_occur_dot_colors(entry, spec, ctx.ongoing_state.as_ref());
    out.push_str(&paint_dot(fg, bg, DOT));
    // Moment-state: a `Verb::Point` row that still carries a state
    // (`switch <field> <state> --moment`) renders its state name right
    // after the dot, matching datumv2's `isPoint` branch of `formatState`
    // (the dot followed by the state name painted in the state's colour,
    // as fg text — no bg chip, no surrounding slots). A plain `occur`
    // (`state = None`) keeps the bare dot.
    if let Some(state) = entry.state.as_ref() {
        out.push_str(&render_point_state_label(state, &entry.field, spec));
    }
    // Occur is a point — no right-char.
    out
}

/// The state cell's BODY alone — the state chip, or the occur dot — with
/// neither the LEAD/TRAIL slots nor the `∅` the dot borrows from its prior
/// state. [`format_state_group`] is this plus those.
///
/// For a caller that lays a row out itself (datum.day's tail cell,
/// da-fknn8): the slots ARE the row's transition, and a browser draws them
/// from [`StateTransition`] — where the previous and following states are
/// words it can show — rather than from a one-character coloured space
/// that reads as padding.
pub fn format_state_body(entry: &Entry, spec: Option<&FieldSpec>, ctx: &RenderContext) -> String {
    if entry.occur_time.is_none() && !draws_state_badge(entry) {
        return render_timeless(entry, spec);
    }
    if draws_state_badge(entry) {
        render_switch_badge(entry.state.as_ref(), &entry.field, spec)
    } else {
        render_occur_body(entry, spec, ctx)
    }
}

/// The painted LEAD slot alone — everything [`format_state_group`] draws
/// BEFORE the body, and nothing else.
///
/// For a state-bearing verb that is the one-character slot the previous
/// state paints, including the repeated-state `!` (da-qfv0). For the bare
/// dot it is the grey `∅` when the field was not tracked before this row,
/// and empty otherwise. For a timeless row it is empty.
///
/// ⚑ vividn, on da-fknn8: *"The previous and following states should just
/// have the colors, not the full text — just like the CLI. Really mirror
/// the CLI here. The rerendering in the web is just for some niceties and
/// being able to select."* So a browser does not compose its own version
/// of this out of a hex and a rule; it draws THIS, and adds only what a
/// terminal cannot — a hover carrying the value, and a selectable box.
/// `lead + `[`format_state_body`]` + trail` is `format_state_group`,
/// byte for byte, for every verb.
pub fn format_lead_cell(entry: &Entry, spec: Option<&FieldSpec>, ctx: &RenderContext) -> String {
    if entry.occur_time.is_none() && !draws_state_badge(entry) {
        return String::new();
    }
    if draws_state_badge(entry) {
        let t = StateTransition::of(ctx, entry);
        return render_lead_slot(t.previous.as_ref(), ctx.is_state_repeat, &entry.field, spec);
    }
    // The bare dot borrows the LEAD's rule and nothing else.
    if ctx.prior_state_ignored {
        return paint(GRAY_FG, NULL_SET);
    }
    String::new()
}

/// The painted TRAIL slot alone — everything [`format_state_group`] draws
/// AFTER the body. Empty for the bare dot and for a timeless row, which
/// have no trailing slot at all. See [`format_lead_cell`].
pub fn format_trail_cell(entry: &Entry, spec: Option<&FieldSpec>, ctx: &RenderContext) -> String {
    if !draws_state_badge(entry) {
        return String::new();
    }
    let t = StateTransition::of(ctx, entry);
    render_slot(t.following.as_ref(), &entry.field, spec)
}

/// The colour a LEAD or TRAIL slot paints for `state`, as hex — the same
/// resolution [`render_slot`] paints with, for a caller rendering to
/// something other than a terminal (da-fknn8).
///
/// `None` ONLY for the explicit "field has ended" sentinel, which paints
/// nothing at all. An untracked, null or IGNORED state is grey: on this
/// surface "nothing was tracked" is a state, not an absence. A parallel
/// set reports its first member's colour, which is the slot's background.
pub fn slot_hex(
    state: Option<&JsonValue>,
    field: &str,
    spec: Option<&FieldSpec>,
) -> Option<String> {
    match resolve_slot(state, field, spec) {
        SlotKind::State(rgb) | SlotKind::Parallel(rgb, _) => Some(rgb.hex()),
        SlotKind::Ignored => Some(Rgb::IGNORED.hex()),
        SlotKind::FieldEnded => None,
    }
}

/// Render a moment-state's state name as fg-coloured text trailing the
/// occur dot. Mirrors datumv2's `currentChalk = chalk.hex(stateColor)`
/// point-case: the label is painted with the state's colour as foreground
/// rather than the bg chip used by the switch/block state cell, so a
/// moment-state reads as "a point that happens to name a state" rather
/// than a state transition.
fn render_point_state_label(state: &JsonValue, field: &str, spec: Option<&FieldSpec>) -> String {
    match state {
        JsonValue::Bool(true) => paint(&field_rgb(field, spec).fg(), "start"),
        JsonValue::Bool(false) => paint(&field_rgb(field, spec).fg(), "end"),
        JsonValue::Null => paint(&Rgb::IGNORED.fg(), NULL_SET),
        JsonValue::String(s) if s == IGNORED_STATE => paint(&Rgb::IGNORED.fg(), s),
        JsonValue::String(s) => paint(&state_rgb(s, spec).fg(), s),
        JsonValue::Object(obj) => {
            let body = match obj.get("id").and_then(JsonValue::as_str) {
                Some(id) => format!("{{{id}}}"),
                None => "{}".to_string(),
            };
            paint(&structured_state_rgb(obj, spec).fg(), &body)
        }
        // Parallel moment-state — render each member in its own colour,
        // comma-joined (the comma tinted with the upcoming member's hue,
        // matching the switch-badge convention).
        JsonValue::Array(items) if items.len() >= 2 => {
            let mut out = String::new();
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    let color = state_value_rgb(item, field, spec).unwrap_or(Rgb::IGNORED);
                    out.push_str(&paint(&color.fg(), ","));
                }
                out.push_str(&render_point_state_label(item, field, spec));
            }
            out
        }
        JsonValue::Array(items) if items.len() == 1 => {
            render_point_state_label(&items[0], field, spec)
        }
        other => paint(
            &state_rgb(&other.to_string(), spec).fg(),
            &other.to_string(),
        ),
    }
}

/// Per da-k5z: the occur "dot" paints the row's field/state color as **fg**
/// (a colored glyph against the terminal bg). `bg` is reserved for ambient
/// context derived from `state_at(field, row.start_time)`:
///
/// 1. `IGNORED` (sentinel string or `Null`) → grey (`Rgb::IGNORED`)
/// 2. Any other ongoing-state value → that state's MD5/spec color
/// 3. `None` or `Bool(false)` (no ongoing block) → `None` (default term bg)
///
/// The caller is responsible for excluding the row itself from the
/// `state_at` query so a non-IGNORED occur on an active block sees the
/// block, not its own (false-emitting) state event.
pub(crate) fn compute_occur_dot_colors(
    row: &Entry,
    spec: Option<&FieldSpec>,
    ongoing_state: Option<&JsonValue>,
) -> (Rgb, Option<Rgb>) {
    // A moment's dot is its state's colour (da-22750); an occur's, with
    // no state or `true`, the field's.
    let fg = row
        .state
        .as_ref()
        .and_then(|v| state_value_rgb(v, &row.field, spec))
        .unwrap_or_else(|| field_rgb(&row.field, spec));
    let bg = match ongoing_state {
        // No prior state, or block was explicitly stopped → no bg.
        None | Some(JsonValue::Bool(false)) => None,
        // IGNORED tracking state → grey bg (sentinel string or null).
        Some(JsonValue::Null) => Some(Rgb::IGNORED),
        Some(JsonValue::String(s)) if s == IGNORED_STATE => Some(Rgb::IGNORED),
        // Any other ongoing-state value → that state's color.
        Some(v) => state_value_rgb(v, &row.field, spec),
    };
    (fg, bg)
}

/// Paint a glyph with explicit fg and an optional bg. With `bg = None`
/// only the fg escape is emitted, leaving the terminal's default bg.
fn paint_dot(fg: Rgb, bg: Option<Rgb>, body: &str) -> String {
    match bg {
        Some(bg) => paint_bg_fg(bg, fg, body),
        None => paint(&fg.fg(), body),
    }
}

/// Render the full `[LEAD][state cell][TRAIL]` trio for switch / start /
/// stop / block. Per da-25l the three segments are flush against each
/// other — no whitespace, coloured or otherwise, between LEAD/cell/TRAIL.
/// LEAD and TRAIL are single bg-coloured cells; the cell body is the
/// state label painted with the state's bg. Outer joining to the field
/// badge is handled by the caller.
fn render_state_transition_group(
    entry: &Entry,
    spec: Option<&FieldSpec>,
    ctx: &RenderContext,
) -> String {
    // da-fknn8: the trio the group draws IS the row's transition — the
    // same three values `tail --json` carries. Read them from one place so
    // the painted cell and the reported row cannot drift apart.
    let t = StateTransition::of(ctx, entry);
    let mut out = String::new();
    out.push_str(&render_lead_slot(
        t.previous.as_ref(),
        ctx.is_state_repeat,
        &entry.field,
        spec,
    ));
    out.push_str(&render_switch_badge(t.current.as_ref(), &entry.field, spec));
    out.push_str(&render_slot(t.following.as_ref(), &entry.field, spec));
    out
}

/// Render the LEAD slot, optionally substituting the `!` warning marker
/// (per da-qfv0) when [`RenderContext::is_state_repeat`] is set. The marker
/// only attaches to a slot with a concrete state colour — `Ignored` /
/// `FieldEnded` slots have no "prior block" to paint the glyph over, so
/// they fall through to the default slot rendering.
pub(crate) fn render_lead_slot(
    previous: Option<&JsonValue>,
    is_state_repeat: bool,
    field: &str,
    spec: Option<&FieldSpec>,
) -> String {
    if !is_state_repeat {
        return render_slot(previous, field, spec);
    }
    match resolve_slot(previous, field, spec) {
        SlotKind::State(rgb) => paint_warning(rgb, WARNING_GLYPH),
        SlotKind::Parallel(bg, _) => paint_warning(bg, WARNING_GLYPH),
        SlotKind::Ignored | SlotKind::FieldEnded => render_slot(previous, field, spec),
    }
}

/// Paint `body` in bold warning-yellow over `bg`. With color disabled the
/// raw glyph is still emitted so non-TTY consumers (pipes, tests, log
/// scrapes) can detect the marker by substring rather than by ANSI escape.
fn paint_warning(bg: Rgb, body: &str) -> String {
    if !color_enabled() {
        return body.to_string();
    }
    let fg = warning_fg(bg);
    format!("{}{}{}{}{}", BOLD, bg.bg(), fg.fg(), body, RESET)
}

/// Render a LEAD or TRAIL slot — a single bg-coloured space. Per da-25l
/// "no prior tracked state" is itself a state (IGNORED), so the slot is
/// painted with the IGNORED grey rather than the old `∅`-glyph fallback.
/// Only the explicit "field has ended" case (`Bool(false)` trail after a
/// stop) renders as an uncoloured space.
///
/// Per da-9vop, a parallel state set (`["vertical","sit"]`) renders as a
/// `▞` checkerboard glyph painted with the first state's bg + the second
/// state's fg, so both colours are visible inside a single 1-cell slot.
pub(crate) fn render_slot(
    state: Option<&JsonValue>,
    field: &str,
    spec: Option<&FieldSpec>,
) -> String {
    match resolve_slot(state, field, spec) {
        SlotKind::State(rgb) => paint_bg(rgb, " "),
        SlotKind::Parallel(bg, fg) => paint_bg_fg(bg, fg, SHADING),
        SlotKind::Ignored => paint_bg(Rgb::IGNORED, " "),
        SlotKind::FieldEnded => " ".to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotKind {
    State(Rgb),
    /// Parallel state set — render as a checkerboard glyph with two
    /// state colours (bg = first, fg = second).
    Parallel(Rgb, Rgb),
    Ignored,
    FieldEnded,
}

/// Resolve a slot's colour from a raw `state_at` output. `None`,
/// `Some(Null)` and the `IGNORED` sentinel string collapse to
/// [`SlotKind::Ignored`] — a 1-cell IGNORED-bg space, treating "no prior
/// tracked state" as a regular prior state (IGNORED).
/// `Some(Bool(false))` — the explicit "field has ended" sentinel — maps to
/// [`SlotKind::FieldEnded`] so the trail renders as terminal-default
/// whitespace rather than a coloured cell. Array values with ≥2 resolvable
/// member colours map to [`SlotKind::Parallel`]; arrays with 0–1
/// distinguishable members fall back to single-state rendering.
fn resolve_slot(state: Option<&JsonValue>, field: &str, spec: Option<&FieldSpec>) -> SlotKind {
    match state {
        None => SlotKind::Ignored,
        Some(JsonValue::Null) => SlotKind::Ignored,
        Some(JsonValue::Bool(false)) => SlotKind::FieldEnded,
        Some(JsonValue::String(s)) if s == IGNORED_STATE => SlotKind::Ignored,
        Some(JsonValue::Array(items)) if items.len() >= 2 => {
            let mut iter = items.iter().filter_map(|v| state_value_rgb(v, field, spec));
            match (iter.next(), iter.next()) {
                (Some(c1), Some(c2)) => SlotKind::Parallel(c1, c2),
                (Some(c1), None) => SlotKind::State(c1),
                _ => SlotKind::Ignored,
            }
        }
        Some(v) => match state_value_rgb(v, field, spec) {
            Some(rgb) => SlotKind::State(rgb),
            None => SlotKind::Ignored,
        },
    }
}

/// Compute the trail-state from an entry alone — the post-effect state
/// for `Point` / `Ongoing` shapes. Block-shaped rows can't be answered
/// from the entry alone; the caller queries `state_at(end_time)` for
/// those and supplies the result via `RenderContext::trail_state`.
pub fn entry_self_trail_state(entry: &Entry) -> Option<JsonValue> {
    if matches!(entry.shape(), EntryShape::Block) {
        return None;
    }
    // da-8y2q.2: legacy `Start`/`Stop` are now `Switch` rows with
    // canonicalized `state=true`/`state=false`. `entry.state.clone()`
    // returns the same `Some(Bool(true|false))` the legacy arms
    // produced explicitly. Non-state-bearing verbs (Verb::Point) yield
    // no trail state.
    if has_state_badge(entry.verb, entry.dur.as_ref()) {
        entry.state.clone()
    } else {
        None
    }
}

/// Render the switch state-badge body. Per da-gvj1:
///
/// * Scalar string → solid bg chip with the state text.
/// * Structured (`{id: …, …}`) → bg chip whose text is `{id}` and whose bg
///   color is hashed by `.id` alone (rules 1–2). Sub-fields (`.title`,
///   `.format`, …) move to the doc-only state second line.
/// * Parallel array → adjacent chips for each member, separated by commas
///   tinted with the upcoming state's colour. Per da-9vop, every state in
///   a parallel set renders in its own colour rather than borrowing the
///   first state's hue. Comma carries **no** trailing space here (rule 6:
///   tail / chip-line uses `,`; doc inline lists use `, `).
fn render_switch_badge(value: Option<&JsonValue>, field: &str, spec: Option<&FieldSpec>) -> String {
    match value {
        None => String::new(),
        Some(JsonValue::Bool(true)) => {
            let fc = field_rgb(field, spec);
            paint_bg(fc, "start")
        }
        Some(JsonValue::Bool(false)) => "end".to_string(),
        Some(JsonValue::Null) => paint_bg_fg(Rgb::IGNORED, Rgb::IGNORED_FG, NULL_SET),
        Some(JsonValue::String(s)) if s == IGNORED_STATE => {
            paint_bg_fg(Rgb::IGNORED, Rgb::IGNORED_FG, s)
        }
        // da-6fvys: «tracking began» is a marker, not a state anyone
        // picked, so it gets no chip: the word alone, in grey.
        Some(JsonValue::String(s)) if s == TRACKED_STATE_VALUE => paint(GRAY_FG, s),
        Some(JsonValue::String(s)) => paint_bg(state_rgb(s, spec), s),
        Some(JsonValue::Object(obj)) => render_structured_chip(obj, spec),
        Some(JsonValue::Array(items)) if items.len() >= 2 => {
            let mut out = String::new();
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    let color = state_value_rgb(item, field, spec).unwrap_or(Rgb::IGNORED);
                    out.push_str(&paint(&color.fg(), ","));
                }
                out.push_str(&render_switch_badge(Some(item), field, spec));
            }
            out
        }
        Some(JsonValue::Array(items)) if items.len() == 1 => {
            render_switch_badge(Some(&items[0]), field, spec)
        }
        Some(other) => paint_bg(state_rgb(&other.to_string(), spec), &other.to_string()),
    }
}

/// Render a structured-state chip — the `{id}` form used on the main info
/// line (rule 1). Background hashed by `.id` alone via
/// [`colors::structured_state_rgb`] so sub-field differences don't change
/// the colour.
fn render_structured_chip(
    obj: &serde_json::Map<String, JsonValue>,
    spec: Option<&FieldSpec>,
) -> String {
    let rgb = structured_state_rgb(obj, spec);
    let body = match obj.get("id").and_then(JsonValue::as_str) {
        Some(id) => format!("{{{id}}}"),
        None => "{}".to_string(),
    };
    paint_bg(rgb, &body)
}

// ─── Field badge (§4) ────────────────────────────────────────────────────────

/// Render the field name as a bg-painted cell — no decorations, no leading
/// space. Single source of truth for "how to render a field cell"; called
/// from both the info-line (via [`format_field_badge`]) and the table path
/// (via `render::extract_with_context_at`).
///
/// Per spec §11 the field cell carries no sparkle or first-time adornment;
/// the first-tracked signal is the `∅` glyph inside the state-transition
/// group (§6), which is owned by [`format_state_group`].
pub(crate) fn format_field_cell(field: &str, spec: Option<&FieldSpec>) -> String {
    let rgb = field_rgb(field, spec);
    paint_bg(rgb, field)
}

/// Info-line wrapper for [`format_field_cell`]: prepends the single
/// uncoloured leading space the §4 layout requires. The trailing space is
/// supplied by `parts.join(" ")` in [`format_info_line_at`], yielding a
/// 1-space gap between the field name and the state group; combined with
/// the join space after the timestamp this gives the §2 2-space landmark.
pub(crate) fn format_field_badge(field: &str, spec: Option<&FieldSpec>) -> String {
    format!(" {}", format_field_cell(field, spec))
}

// ─── Timestamp (§3) ──────────────────────────────────────────────────────────

/// Format a `DatumTime` per §3: `HH:MM:SS+TZ` with the TZ offset rendered
/// in the gray slug-color and compressed (leading zeros dropped). The whole
/// stamp is underlined when `t` is in the future relative to `now`.
///
/// When `t`'s local calendar day differs from `now`'s (per
/// [`format_relative_day_prefix`]), a gray relative-day prefix (`-1d`,
/// `2026-04-15`, etc.) is prepended so day-axis context isn't lost when
/// scanning across rows.
///
/// `uncertainty` is the row's `occur_time_uncertainty`. When it is `Some("P1D")`
/// the row is a date-only entry (per da-fc0l) and the stamp degrades to the
/// bare local date `YYYY-MM-DD` — no clock, no offset, no relative-day
/// prefix. Any other value renders the full datetime; non-`P1D` values are
/// `±N` uncertainties (da-lkji) and append a gray `±N<unit>` suffix.
pub(crate) fn format_timestamp(
    t: &DatumTime,
    now: &DatumTime,
    uncertainty: Option<&str>,
) -> String {
    format_timestamp_ref(t, now, uncertainty, None)
}

/// As [`format_timestamp`] but with an optional table-header reference
/// offset (da-lkji). `reference_offset` is the UTC offset (minutes) the
/// table prints once in the time-column header:
///
/// * `None` — standalone mode: every stamp carries its own trailing gray
///   offset (`22:55:00+2`), as the info-line always has.
/// * `Some(r)` — table mode: rows whose offset equals `r` print no offset
///   at all; rows that differ print theirs as a gray parenthesized
///   *prefix* (`(+1)09:15:00`) so the `HH:MM:SS` columns stay aligned.
pub(crate) fn format_timestamp_ref(
    t: &DatumTime,
    now: &DatumTime,
    uncertainty: Option<&str>,
    reference_offset: Option<i32>,
) -> String {
    // Render in the row's own timezone — "today" is the row's local day,
    // not the host's. See da-n1q acceptance criteria.
    let local = t.local_time();
    let composed = if uncertainty == Some("P1D") {
        // da-5uph: P1D rows have no meaningful time-of-day. Show the bare
        // local date; no relative-day prefix needed since the date itself
        // is on-screen.
        local.format("%Y-%m-%d").to_string()
    } else {
        let base = format!("{}:{:02}", clock_label(&local), local.second());
        let offset = compress_offset_minutes(t.offset_minutes);
        let suffix = format_uncertainty_suffix(uncertainty)
            .map(|u| paint(GRAY_FG, &u))
            .unwrap_or_default();
        let stamp = match reference_offset {
            Some(r) if r == t.offset_minutes => format!("{base}{suffix}"),
            Some(_) => format!("{}{base}{suffix}", paint(GRAY_FG, &format!("({offset})"))),
            None => format!("{base}{}{suffix}", paint(GRAY_FG, &offset)),
        };
        match format_relative_day_prefix(t, now) {
            Some(prefix) => format!("{} {stamp}", paint(GRAY_FG, &prefix)),
            None => stamp,
        }
    };
    if t.utc > now.utc && color_enabled() {
        format!("{UNDERLINE}{composed}{RESET}")
    } else {
        // Even without color, the underline escape is a no-op under
        // strip_ansi — return the composed string unchanged.
        composed
    }
}

/// `HH:MM`, 24-hour: the clock every stamp above starts with, and the one
/// a dayview/nowview mark's `<title>` reads its span in (da-s5da).
pub(crate) fn clock_label(local: &impl Timelike) -> String {
    format!("{:02}:{:02}", local.hour(), local.minute())
}

/// Wrap a host-local [`chrono::DateTime<Utc>`] (e.g. `create_time`,
/// `modify_time`) as a [`DatumTime`] anchored to the host's local offset, so
/// the same renderer used for occur/end times can format it.
pub(crate) fn synthesize_local_datumtime(utc: chrono::DateTime<Utc>) -> DatumTime {
    let offset_minutes = Zone::host().offset_at(utc).local_minus_utc() / 60;
    DatumTime {
        utc,
        offset_minutes,
        timezone: Some("Local".to_string()),
        uncertainty: None,
    }
}

/// Compress a UTC offset in minutes → `+2` / `-5` / `+1:30` per §3.
pub(crate) fn compress_offset_minutes(minutes: i32) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let a = minutes.abs();
    let (h, m) = (a / 60, a % 60);
    if m == 0 {
        format!("{sign}{h}")
    } else if h == 0 {
        format!("{sign}0:{m:02}")
    } else {
        format!("{sign}{h}:{m:02}")
    }
}

/// Render the stored `occur_time_uncertainty` (ISO-8601, as written by
/// `Uncertainty::to_iso`) as the display suffix body — `PT3M` →
/// `±3m`, `PT90S` → `±90s`, `P2D` → `±2d` (da-lkji). Unparseable or
/// compound forms return `None`. A time's `P1D` is the date-only marker
/// (da-5uph), which [`format_timestamp_ref`] draws before it asks here; a
/// duration's is a day either way.
fn format_uncertainty_suffix(uncertainty: Option<&str>) -> Option<String> {
    let iso = uncertainty?;
    let rest = iso.strip_prefix('P')?;
    let (num, unit) = match rest.strip_prefix('T') {
        Some(t) => {
            let (n, u) = t.split_at(t.len().checked_sub(1)?);
            let u = match u {
                "S" => "s",
                "M" => "m",
                "H" => "h",
                _ => return None,
            };
            (n, u)
        }
        None => {
            let (n, u) = rest.split_at(rest.len().checked_sub(1)?);
            let u = match u {
                "D" => "d",
                "M" => "mo",
                "Y" => "y",
                _ => return None,
            };
            (n, u)
        }
    };
    if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!("±{num}{unit}"))
}

/// An entry's duration as every surface prints it — `30m`, and `30m±5m`
/// when its uncertainty is stated, the `±` gray as a time's is (da-avqf4).
pub(crate) fn format_dur(entry: &Entry) -> Option<String> {
    let dur = format_human_duration(entry.duration_seconds()?, DurationPrecision::Seconds);
    let suffix = format_uncertainty_suffix(entry.dur_uncertainty())
        .map(|u| paint(GRAY_FG, &u))
        .unwrap_or_default();
    Some(format!("{dur}{suffix}"))
}

// ─── Info-line composition ───────────────────────────────────────────────────

/// Produce the main info line `<time>  <field>  <state-group>  [dur]`.
/// Returns an empty string when there's nothing to show.
///
/// Defaults `now` to the host's local time. Callers that render a batch of
/// rows (e.g. `datum tail`) should pass a single shared `now` via
/// [`format_info_line_at`] so every row in the batch agrees on what
/// "today" means.
pub fn format_info_line(entry: &Entry, spec: Option<&FieldSpec>, ctx: &RenderContext) -> String {
    format_info_line_at(entry, spec, ctx, &now_local())
}

/// As [`format_info_line`] but with an explicit reference instant. Use this
/// for any caller that renders multiple rows or that needs deterministic
/// "today" semantics in a test.
pub fn format_info_line_at(
    entry: &Entry,
    spec: Option<&FieldSpec>,
    ctx: &RenderContext,
    now: &DatumTime,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    let uncertainty = entry.occur_uncertainty();
    if let Some(t) = &entry.occur_time {
        parts.push(format_timestamp(t, now, uncertainty));
    } else {
        // Hybrid fallback: row has no occur_time, so render create_time
        // prefixed by a gray 'c' — same convention the table renderer
        // applies via `format_gray_prefix("c", ...)` in the
        // `time_hybrid` extractor (render/mod.rs). create_time is a
        // wall-clock instant, never date-only.
        let synth = synthesize_local_datumtime(entry.create_time);
        parts.push(format!(
            "{}{}",
            paint(GRAY_FG, "c"),
            format_timestamp(&synth, now, None)
        ));
    }
    parts.push(format_field_badge(&entry.field, spec));
    if let Some(group) = format_state_group(entry, spec, ctx) {
        if !group.is_empty() {
            parts.push(group);
        }
    }
    parts.extend(format_dur(entry));
    parts.join(" ")
}

/// Backwards-compatible "info line" used by the legacy show_action path,
/// exposed for tests and alternate front-ends.
pub fn render_entry_line(
    entry: &Entry,
    registry: Option<&SpecRegistry>,
    ctx: &RenderContext,
) -> String {
    let spec = registry.and_then(|r| r.fields.get(&entry.field));
    format_info_line(entry, spec, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone};
    use datum_core::entry::Verb;

    #[test]
    fn compress_offset_minutes_drops_leading_zeros() {
        assert_eq!(compress_offset_minutes(120), "+2");
        assert_eq!(compress_offset_minutes(-300), "-5");
        assert_eq!(compress_offset_minutes(90), "+1:30");
        assert_eq!(compress_offset_minutes(0), "+0");
        assert_eq!(compress_offset_minutes(-30), "-0:30");
    }

    #[test]
    fn uncertainty_suffix_renders_stored_iso_forms() {
        assert_eq!(format_uncertainty_suffix(None), None);
        assert_eq!(
            format_uncertainty_suffix(Some("P1D")).as_deref(),
            Some("±1d"),
            "a duration's day; a time's P1D never reaches here"
        );
        assert_eq!(
            format_uncertainty_suffix(Some("PT3M")).as_deref(),
            Some("±3m")
        );
        assert_eq!(
            format_uncertainty_suffix(Some("PT90S")).as_deref(),
            Some("±90s")
        );
        assert_eq!(
            format_uncertainty_suffix(Some("PT2H")).as_deref(),
            Some("±2h")
        );
        assert_eq!(
            format_uncertainty_suffix(Some("P2D")).as_deref(),
            Some("±2d")
        );
        assert_eq!(
            format_uncertainty_suffix(Some("P1M")).as_deref(),
            Some("±1mo")
        );
        assert_eq!(format_uncertainty_suffix(Some("bogus")), None);
        assert_eq!(
            format_uncertainty_suffix(Some("PT1H30M")),
            None,
            "compound never stored"
        );
    }

    fn dt(y: i32, mo: u32, d: u32) -> DatumTime {
        let utc: DateTime<Utc> = Utc.with_ymd_and_hms(y, mo, d, 12, 0, 0).unwrap();
        DatumTime {
            utc,
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        }
    }

    #[test]
    fn relative_day_prefix_today_returns_none() {
        let now = dt(2026, 4, 22);
        assert_eq!(format_relative_day_prefix(&dt(2026, 4, 22), &now), None);
    }

    #[test]
    fn relative_day_prefix_yesterday_to_three_days_ago() {
        let now = dt(2026, 4, 22);
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 21), &now).as_deref(),
            Some("-1d")
        );
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 20), &now).as_deref(),
            Some("-2d")
        );
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 19), &now).as_deref(),
            Some("-3d")
        );
    }

    #[test]
    fn relative_day_prefix_far_past_uses_iso_date() {
        let now = dt(2026, 4, 22);
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 18), &now).as_deref(),
            Some("2026-04-18")
        );
        assert_eq!(
            format_relative_day_prefix(&dt(2024, 12, 13), &now).as_deref(),
            Some("2024-12-13")
        );
    }

    #[test]
    fn relative_day_prefix_future_one_to_three_days() {
        let now = dt(2026, 4, 22);
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 23), &now).as_deref(),
            Some("+1d")
        );
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 25), &now).as_deref(),
            Some("+3d")
        );
    }

    #[test]
    fn relative_day_prefix_far_future_uses_iso_date() {
        let now = dt(2026, 4, 22);
        assert_eq!(
            format_relative_day_prefix(&dt(2026, 4, 26), &now).as_deref(),
            Some("2026-04-26")
        );
    }

    #[test]
    fn relative_day_prefix_uses_targets_local_calendar() {
        // 22:00 UTC on the 21st in Auckland (UTC+12) → 10:00 on the 22nd
        // local; "today" must be 2026-04-22 by the row's local clock.
        let target = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 21, 22, 0, 0).unwrap(),
            offset_minutes: 12 * 60,
            timezone: Some("Pacific/Auckland".to_string()),
            uncertainty: None,
        };
        let now_in_utc = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 0, 0, 0).unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        // Per UTC: target (Apr 21) is "yesterday" relative to now (Apr 22).
        // Per target's local clock: both are Apr 22 → today, no prefix.
        assert_eq!(format_relative_day_prefix(&target, &now_in_utc), None);
    }

    #[test]
    fn timestamp_prepends_prefix_when_not_today() {
        // No-color path so we can match the prefix verbatim.
        std::env::set_var("NO_COLOR", "1");
        let target = dt(2026, 4, 19);
        let now = dt(2026, 4, 22);
        let out = format_timestamp(&target, &now, None);
        assert!(
            out.starts_with("-3d "),
            "expected '-3d ' prefix, got {out:?}"
        );
        let target_old = dt(2024, 12, 13);
        let out_old = format_timestamp(&target_old, &now, None);
        assert!(
            out_old.starts_with("2024-12-13 "),
            "expected ISO-date prefix, got {out_old:?}"
        );
        std::env::remove_var("NO_COLOR");
    }

    #[test]
    fn timestamp_today_has_no_prefix() {
        std::env::set_var("NO_COLOR", "1");
        let target = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 9, 15, 30).unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        let now = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 17, 0, 0).unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        let out = format_timestamp(&target, &now, None);
        assert!(
            out.starts_with("09:15:30"),
            "today rows must not carry a prefix: {out:?}"
        );
        std::env::remove_var("NO_COLOR");
    }

    /// da-5uph: a P1D uncertainty degrades the stamp to a bare local date.
    /// No clock, no offset suffix, no relative-day prefix.
    #[test]
    fn timestamp_p1d_renders_as_bare_local_date() {
        std::env::set_var("NO_COLOR", "1");
        // 2026-04-22 00:00 UTC in a +5h zone is 2026-04-22 05:00 local — the
        // displayed date is the local one, mirroring start-of-day stripping.
        let target = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 0, 0, 0).unwrap(),
            offset_minutes: 5 * 60,
            timezone: Some("Asia/Karachi".to_string()),
            uncertainty: None,
        };
        let now = dt(2026, 4, 22);
        let out = format_timestamp(&target, &now, Some("P1D"));
        assert_eq!(out, "2026-04-22");
        // Also: a far-past P1D row gets the bare date, never a relative-day
        // prefix wrapping a clock part.
        let old = dt(2024, 12, 13);
        let out_old = format_timestamp(&old, &now, Some("P1D"));
        assert_eq!(out_old, "2024-12-13");
        std::env::remove_var("NO_COLOR");
    }

    /// da-5uph: an uncertainty other than `P1D` (including `None`) keeps the
    /// full datetime rendering.
    #[test]
    fn timestamp_non_p1d_uncertainty_keeps_clock() {
        std::env::set_var("NO_COLOR", "1");
        let target = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 9, 15, 30).unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        let now = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 17, 0, 0).unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        let out_none = format_timestamp(&target, &now, None);
        assert!(out_none.starts_with("09:15:30"), "got {out_none:?}");
        // da-lkji: a non-P1D uncertainty keeps the clock AND surfaces the
        // stored uncertainty as a trailing `±N<unit>`.
        let out_sec = format_timestamp(&target, &now, Some("PT1S"));
        assert_eq!(
            out_sec,
            format!("{out_none}±1s"),
            "PT1S keeps clock + ± suffix"
        );
        std::env::remove_var("NO_COLOR");
    }

    /// da-lkji: table mode — a row whose offset matches the header
    /// reference prints no offset at all; a differing row prints its own
    /// offset as a parenthesized prefix so `HH:MM:SS` stays aligned.
    #[test]
    fn timestamp_ref_mode_prefixes_only_differing_offsets() {
        std::env::set_var("NO_COLOR", "1");
        let now = dt(2026, 4, 22);
        let same = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 20, 55, 0).unwrap(),
            offset_minutes: 120,
            timezone: Some("Europe/Berlin".to_string()),
            uncertainty: None,
        };
        assert_eq!(
            format_timestamp_ref(&same, &now, None, Some(120)),
            "22:55:00"
        );
        let other = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 8, 15, 0).unwrap(),
            offset_minutes: 60,
            timezone: Some("Europe/London".to_string()),
            uncertainty: None,
        };
        assert_eq!(
            format_timestamp_ref(&other, &now, None, Some(120)),
            "(+1)09:15:00"
        );
        // Uncertainty composes independently of the offset prefix.
        assert_eq!(
            format_timestamp_ref(&same, &now, Some("PT3M"), Some(120)),
            "22:55:00±3m"
        );
        assert_eq!(
            format_timestamp_ref(&other, &now, Some("PT3M"), Some(120)),
            "(+1)09:15:00±3m"
        );
        std::env::remove_var("NO_COLOR");
    }

    /// da-r1q acceptance: an entry stored with a non-host timezone renders
    /// in the entry's own zone, not the host's `Local`. The bead's example:
    /// `datum occur x -z America/New_York` at Berlin 12:00 CET stores UTC
    /// 10:00 and tail must show `06:00-4` (NY time), not the Berlin or
    /// host conversion.
    #[test]
    fn timestamp_renders_in_entrys_stored_timezone() {
        std::env::set_var("NO_COLOR", "1");
        // Berlin 12:00 CEST (UTC+2) → UTC 10:00 → NY 06:00 EDT (UTC-4).
        let ny_target = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 10, 0, 0).unwrap(),
            offset_minutes: -4 * 60,
            timezone: Some("America/New_York".to_string()),
            uncertainty: None,
        };
        // "now" is in Berlin — used to verify the relative-day calc
        // also uses the row's local calendar, not the host's.
        let berlin_now = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 10, 0, 0).unwrap(),
            offset_minutes: 2 * 60,
            timezone: Some("Europe/Berlin".to_string()),
            uncertainty: None,
        };
        let out = format_timestamp(&ny_target, &berlin_now, None);
        assert!(
            out.contains("06:00:00"),
            "NY-stored row must render as NY wall-clock 06:00:00, got {out:?}"
        );
        assert!(
            out.contains("-4"),
            "NY-stored row must carry the stored -4 offset, got {out:?}"
        );
        assert!(
            !out.contains("12:00:00"),
            "must NOT render as Berlin wall-clock 12:00:00, got {out:?}"
        );
        std::env::remove_var("NO_COLOR");
    }

    // ─── compute_occur_dot_colors (da-k5z) ───────────────────────────────

    fn occur_row(field: &str) -> Entry {
        let ts = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 9, 0, 0).unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        Entry::new(
            field.to_string(),
            Verb::Point,
            None,
            None,
            Some(ts),
            None,
            "test-device".to_string(),
        )
    }

    /// No ongoing state and an explicit-stop both collapse to "no bg" so
    /// the dot floats on the terminal's default background.
    #[test]
    fn occur_dot_default_bg_when_no_ongoing_block() {
        let row = occur_row("coffee");
        let expected_fg = field_rgb(&row.field, None);

        let (fg_none, bg_none) = compute_occur_dot_colors(&row, None, None);
        assert_eq!(fg_none, expected_fg, "fg uses field color");
        assert_eq!(bg_none, None, "no ongoing-state → no bg");

        let stopped = JsonValue::Bool(false);
        let (fg_off, bg_off) = compute_occur_dot_colors(&row, None, Some(&stopped));
        assert_eq!(fg_off, expected_fg);
        assert_eq!(bg_off, None, "explicit stop → no bg");
    }

    /// An occur during an ongoing block adopts the block's state color as
    /// bg while keeping the field color as fg.
    #[test]
    fn occur_dot_uses_ongoing_state_color_as_bg() {
        let row = occur_row("water");
        let ongoing = JsonValue::String("sleep".to_string());
        let (fg, bg) = compute_occur_dot_colors(&row, None, Some(&ongoing));
        assert_eq!(fg, field_rgb(&row.field, None));
        assert_eq!(
            bg,
            Some(state_rgb("sleep", None)),
            "bg should be derived from the ongoing state name, not the field"
        );
        assert_ne!(fg, bg.unwrap(), "fg and bg are independently colored");
    }

    /// Both the IGNORED sentinel string and a `Null` ongoing-state map to
    /// the shared grey bg per §8.
    #[test]
    fn occur_dot_uses_grey_bg_when_field_is_ignored() {
        let row = occur_row("blocked");
        let ignored_sentinel = JsonValue::String(IGNORED_STATE.to_string());
        let (_, bg_str) = compute_occur_dot_colors(&row, None, Some(&ignored_sentinel));
        assert_eq!(bg_str, Some(Rgb::IGNORED), "IGNORED sentinel → grey bg");

        let null = JsonValue::Null;
        let (_, bg_null) = compute_occur_dot_colors(&row, None, Some(&null));
        assert_eq!(bg_null, Some(Rgb::IGNORED), "null state → grey bg");
    }

    // ─── compute_is_state_repeat / `!` LEAD marker (da-qfv0) ─────────────

    fn switch_row(field: &str, state: JsonValue) -> Entry {
        let ts = DatumTime {
            utc: Utc.with_ymd_and_hms(2026, 4, 22, 9, 0, 0).unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        Entry::new(
            field.to_string(),
            Verb::Switch,
            Some(state),
            None,
            Some(ts),
            None,
            "test-device".to_string(),
        )
    }

    /// A switch whose value matches the prior state must flag the repeat.
    #[test]
    fn state_repeat_flagged_when_value_matches_prior() {
        let row = switch_row("body", JsonValue::String("vertical".into()));
        let prior = JsonValue::String("vertical".into());
        assert!(compute_is_state_repeat(Some(&prior), &row));
    }

    /// Different states must not flag — that's a normal transition.
    #[test]
    fn state_repeat_not_flagged_when_values_differ() {
        let row = switch_row("body", JsonValue::String("vertical".into()));
        let prior = JsonValue::String("sit".into());
        assert!(!compute_is_state_repeat(Some(&prior), &row));
    }

    /// `start` (canonicalized to `Bool(true)`) on a field with no history
    /// must not flag — first-time transitions can't be a repeat.
    #[test]
    fn state_repeat_not_flagged_with_no_prior_state() {
        let row = switch_row("body", JsonValue::Bool(true));
        assert!(!compute_is_state_repeat(None, &row));
    }

    /// da-g1b8: dropping a member of a parallel prior is a transition —
    /// no `!` on the row the user just wrote.
    #[test]
    fn state_repeat_not_flagged_when_value_narrows_parallel_prior() {
        let row = switch_row("body", JsonValue::String("vertical".into()));
        let prior = JsonValue::Array(vec![
            JsonValue::String("vertical".into()),
            JsonValue::String("sit".into()),
        ]);
        assert!(!compute_is_state_repeat(Some(&prior), &row));
    }

    /// da-g1b8, the reported case: adding an element to a parallel state
    /// must not stamp the write-time `!` marker.
    #[test]
    fn state_repeat_not_flagged_when_value_adds_parallel_member() {
        let row = switch_row(
            "body",
            JsonValue::Array(vec![
                JsonValue::String("vertical".into()),
                JsonValue::String("sit".into()),
            ]),
        );
        let prior = JsonValue::String("vertical".into());
        assert!(!compute_is_state_repeat(Some(&prior), &row));
    }

    /// da-g1b8: the same members reordered are the same state, so the
    /// marker still fires — `Parallel` compares as a multiset.
    #[test]
    fn state_repeat_flagged_when_parallel_members_only_reordered() {
        let row = switch_row(
            "body",
            JsonValue::Array(vec![
                JsonValue::String("sit".into()),
                JsonValue::String("vertical".into()),
            ]),
        );
        let prior = JsonValue::Array(vec![
            JsonValue::String("vertical".into()),
            JsonValue::String("sit".into()),
        ]);
        assert!(compute_is_state_repeat(Some(&prior), &row));
    }

    /// Non-state-bearing verbs (`occur`/`point`) must never trigger the
    /// marker regardless of prior state.
    #[test]
    fn state_repeat_skipped_for_non_state_verbs() {
        let mut row = occur_row("coffee");
        row.state = Some(JsonValue::String("vertical".into()));
        let prior = JsonValue::String("vertical".into());
        assert!(!compute_is_state_repeat(Some(&prior), &row));
    }

    // The rendered-output tests for `render_lead_slot` (warning glyph,
    // FieldEnded fallback, NO_COLOR survival) live in
    // `tests/render_fixtures.rs`; all env mutation serializes on the
    // process-wide `datum_test_util::env_lock` (da-3ywd).
}
