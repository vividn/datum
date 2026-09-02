//! `datum stats` — count / total-duration / per-state-ratio rollup for a
//! field (or a single `field:state`) over a flexible time-slice.
//!
//! The core life-review statistic: `count` answers "how often", `stats`
//! answers "how much time, and split how". Durations are reconstructed from
//! the entry log — `dur` is not stored for switch-blocks (it is derived), so
//! each entry's block runs from its `occur_time` to the next entry's
//! `occur_time` (switch continuity), with the still-open trailing block
//! running to `now`. An entry that *does* carry a stored `dur` (a discrete
//! block, e.g. `run --dur 30m`) is measured by that span instead of the
//! gap-to-next, and its `following_state` (if any) fills the remainder up to
//! the next entry. Ref algorithm: crew/vremya/scripts/timespent.py.
//!
//! The ratio is a share of *tracked* time, not of wall-clock: the closed
//! state (`false` — every gap between blocks) and the untracked states
//! (`null`, the `IGNORED` sentinel, no state at all) are out of the
//! denominator unless `--include-false` / `--include-untracked` put them
//! back. Without that, a real db's states all read under 1% (da-icba).
//! Counts and durations are unaffected — only the denominator moves.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Utc};
use datum_core::db::{Db, ListParams, TimeMetric, IGNORED_STATE_VALUE};
use datum_core::time::DatumTime;
use serde_json::Value as JsonValue;

use crate::args::StatsArgs;
use crate::commands::field_visuals::state_label;
use crate::commands::ids::parse_since;
use crate::render::action::format_duration;

/// Per-state slice of a field's rollup.
#[derive(Debug, Clone, PartialEq)]
pub struct StateStat {
    pub state: String,
    pub count: usize,
    pub duration_seconds: i64,
    /// Fraction of the field's *tracked* duration (0.0..=1.0), or `None`
    /// when the state is itself outside the denominator (`false` /
    /// untracked, absent `--include-false` / `--include-untracked`).
    pub ratio: Option<f64>,
}

/// Structured result of `stats` — exposed so tests can assert on the data
/// without parsing stdout.
#[derive(Debug, Clone, PartialEq)]
pub struct StatsResult {
    pub field: String,
    /// `Some` when the selector was `field:state` — `states` then holds
    /// only that one state, while the totals remain the whole-field totals.
    pub selected_state: Option<String>,
    pub total_count: usize,
    pub total_duration_seconds: i64,
    /// The ratio denominator: measured duration in states that count as
    /// tracked. Equals `total_duration_seconds` when nothing is excluded.
    pub tracked_duration_seconds: i64,
    /// Longest-duration first; ties broken by state name.
    pub states: Vec<StateStat>,
}

pub fn compute_stats(args: &StatsArgs, db: &Db) -> Result<StatsResult> {
    let selector = args.selector.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "stats requires a field, e.g. `datum stats activity` or `datum stats activity:coding`"
        )
    })?;
    let (field, selected_state) = match selector.split_once(':') {
        Some((f, s)) if !f.is_empty() && !s.is_empty() => (f.to_string(), Some(s.to_string())),
        _ => (selector.to_string(), None),
    };

    let (start, end) = resolve_window(args)?;
    let now = DatumTime::now().utc;

    // Oldest→newest, time-bearing state rows only. `TimeMetric::Occur` drops
    // timeless `add`/`note` rows (NULL occur_time) and the default meta
    // filter drops `flag` / protocol-note markers — the same exclusions
    // tail / dayview / now apply, so the timeline matches what the user sees.
    let params = ListParams {
        fields: vec![field.clone()],
        metric: TimeMetric::Occur,
        ..Default::default()
    };
    let entries = db.list_entries(&params)?;
    let timeline: Vec<TimelineRow> = entries
        .iter()
        .filter_map(|e| {
            Some(TimelineRow {
                state: state_label(e.state.as_ref()).unwrap_or_else(|| "(none)".to_string()),
                class: classify(e.state.as_ref()),
                occur: e.occur_time.as_ref()?.utc,
                dur: e.duration_seconds(),
                following: state_label(e.following_state.as_ref()),
                following_class: classify(e.following_state.as_ref()),
            })
        })
        .collect();

    let mut dur_by_state: BTreeMap<String, i64> = BTreeMap::new();
    let mut count_by_state: BTreeMap<String, usize> = BTreeMap::new();
    let mut class_by_state: BTreeMap<String, StateClass> = BTreeMap::new();

    for (i, row) in timeline.iter().enumerate() {
        let next = timeline.get(i + 1).map(|r| r.occur).unwrap_or(now);
        // Block A — the row's own state. A stored `dur` bounds it (discrete
        // block); otherwise it runs to the next entry (switch continuity),
        // or to `now` for the still-open trailing block.
        let block_a_end = match row.dur {
            Some(d) => (row.occur + Duration::seconds(d)).min(next),
            None => next,
        };
        add_overlap(
            &mut dur_by_state,
            &row.state,
            row.occur,
            block_a_end,
            start,
            end,
        );
        note_class(&mut class_by_state, &row.state, row.class);

        // Block B — with an explicit `dur` and a `following_state`, the field
        // reverts to that state from the block's end until the next entry.
        if let (Some(_), Some(fs)) = (row.dur, &row.following) {
            add_overlap(&mut dur_by_state, fs, block_a_end, next, start, end);
            note_class(&mut class_by_state, fs, row.following_class);
        }

        // Count the entry itself when its occur instant lands in-window.
        if within(row.occur, start, end) {
            *count_by_state.entry(row.state.clone()).or_default() += 1;
        }
    }

    // A counted state with zero measured duration (single-instant window)
    // still deserves a row.
    for state in count_by_state.keys() {
        dur_by_state.entry(state.clone()).or_insert(0);
    }

    // The denominator. `false` (every gap between blocks) and the untracked
    // states would otherwise swamp the ratio column, turning it into "share
    // of wall-clock" instead of "share of tracked time" (da-icba). A state
    // named outright by the selector is always its own subject, so it stays
    // in the denominator whatever its class.
    let counted = |state: &str| {
        selected_state.as_deref() == Some(state)
            || match class_by_state
                .get(state)
                .copied()
                .unwrap_or(StateClass::Tracked)
            {
                StateClass::Tracked => true,
                StateClass::Closed => args.include_false,
                StateClass::Untracked => args.include_untracked,
            }
    };

    let total_duration_seconds: i64 = dur_by_state.values().sum();
    let tracked_duration_seconds: i64 = dur_by_state
        .iter()
        .filter(|(state, _)| counted(state))
        .map(|(_, d)| *d)
        .sum();
    let total_count: usize = count_by_state.values().sum();

    let mut states: Vec<StateStat> = dur_by_state
        .into_iter()
        .map(|(state, duration_seconds)| {
            let count = count_by_state.get(&state).copied().unwrap_or(0);
            let ratio = counted(&state).then(|| {
                if tracked_duration_seconds > 0 {
                    duration_seconds as f64 / tracked_duration_seconds as f64
                } else {
                    0.0
                }
            });
            StateStat {
                state,
                count,
                duration_seconds,
                ratio,
            }
        })
        .collect();
    states.sort_by(|a, b| {
        b.duration_seconds
            .cmp(&a.duration_seconds)
            .then_with(|| a.state.cmp(&b.state))
    });

    if let Some(sel) = &selected_state {
        // Keep only the requested state — synthesizing a zero row when the
        // field was never in that state during the window, so the output is
        // still a well-formed answer rather than "empty".
        let one = states
            .into_iter()
            .find(|s| &s.state == sel)
            .unwrap_or(StateStat {
                state: sel.clone(),
                count: 0,
                duration_seconds: 0,
                ratio: Some(0.0),
            });
        states = vec![one];
    }

    Ok(StatsResult {
        field,
        selected_state,
        total_count,
        total_duration_seconds,
        tracked_duration_seconds,
        states,
    })
}

pub fn stats(args: StatsArgs, db: &Db) -> Result<()> {
    let result = compute_stats(&args, db)?;
    if args.json {
        print!("{}", render_json(&result));
    } else {
        print!("{}", render_text(&result));
    }
    Ok(())
}

// ─── Window resolution ─────────────────────────────────────────────────────

/// Resolve the time-slice into `(start, end)` UTC bounds — half-open
/// `[start, end)`. `start = None` means unbounded below (everything up to
/// `end`). `end` defaults to `now`.
fn resolve_window(args: &StatsArgs) -> Result<(Option<DateTime<Utc>>, DateTime<Utc>)> {
    let now = DatumTime::now().utc;
    if let Some(slice) = &args.slice {
        if args.since.is_some() || args.until.is_some() {
            bail!("a shorthand time-slice ('{slice}') can't be combined with --since/--until");
        }
        let (s, e) = parse_slice(slice)?;
        return Ok((Some(s), e));
    }
    let start = args.since.as_deref().map(parse_since).transpose()?;
    let end = match args.until.as_deref() {
        Some(u) => parse_until(u)?,
        None => now,
    };
    Ok((start, end))
}

/// Shorthand whole-period slice: `YYYY`, `YYYY-MM`, or `YYYY-MM-DD`, each
/// interpreted in local time. Returns half-open `[start, end)` covering the
/// whole period, so `stats activity 2026-06` ≡ `--since 2026-06-01 --until
/// 2026-06-30`.
fn parse_slice(s: &str) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let digits = |v: &str| v.chars().all(|c| c.is_ascii_digit());

    // YYYY — a whole year.
    if s.len() == 4 && digits(s) {
        let y: i32 = s.parse().context("invalid year")?;
        return Ok((first_of_month(y, 1)?, first_of_month(y + 1, 1)?));
    }
    // YYYY-MM — a whole month. `m` is exactly two digits, so this never
    // collides with the `YYYY-MM-DD` form (whose `m` would be `MM-DD`).
    if let Some((y, m)) = s
        .split_once('-')
        .filter(|(y, m)| y.len() == 4 && digits(y) && m.len() == 2 && digits(m))
    {
        let y: i32 = y.parse().context("invalid year")?;
        let m: u32 = m.parse().context("invalid month")?;
        if !(1..=12).contains(&m) {
            bail!("invalid month in time-slice '{s}'");
        }
        let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
        return Ok((first_of_month(y, m)?, first_of_month(ny, nm)?));
    }
    // YYYY-MM-DD — a whole day.
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let next = date.succ_opt().context("date overflow")?;
        return Ok((local_midnight(date)?, local_midnight(next)?));
    }
    bail!("invalid time-slice '{s}' (expected YYYY, YYYY-MM, or YYYY-MM-DD)")
}

/// Parse `--until` into a UTC upper bound. A bare `YYYY-MM-DD` is inclusive
/// of the whole named day (upper = next local midnight); an RFC3339 instant
/// is used verbatim.
fn parse_until(s: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let next = date.succ_opt().context("date overflow")?;
        return local_midnight(next);
    }
    bail!("invalid --until '{s}' (expected RFC3339 or YYYY-MM-DD)")
}

fn first_of_month(year: i32, month: u32) -> Result<DateTime<Utc>> {
    let date = NaiveDate::from_ymd_opt(year, month, 1).context("date out of range")?;
    local_midnight(date)
}

fn local_midnight(date: NaiveDate) -> Result<DateTime<Utc>> {
    let naive = date.and_hms_opt(0, 0, 0).unwrap();
    Ok(Local
        .from_local_datetime(&naive)
        .single()
        .context("ambiguous local midnight")?
        .with_timezone(&Utc))
}

// ─── Block math ─────────────────────────────────────────────────────────────

struct TimelineRow {
    state: String,
    class: StateClass,
    occur: DateTime<Utc>,
    dur: Option<i64>,
    following: Option<String>,
    following_class: StateClass,
}

/// How a state value participates in the ratio denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateClass {
    /// A real state the user tracked — always in the denominator.
    Tracked,
    /// `false` — the field is stopped. This is every gap between blocks.
    Closed,
    /// `null`, the `IGNORED` sentinel, or no state at all.
    Untracked,
}

fn classify(v: Option<&JsonValue>) -> StateClass {
    match v {
        None | Some(JsonValue::Null) => StateClass::Untracked,
        Some(JsonValue::String(s)) if s == IGNORED_STATE_VALUE => StateClass::Untracked,
        Some(JsonValue::Bool(false)) => StateClass::Closed,
        _ => StateClass::Tracked,
    }
}

/// Record a label's class. Distinct JSON values can share a label (the
/// string `"false"` and `Bool(false)` both render as `false`, and stats
/// already folds them into one row) — a tracked reading of a label wins, so
/// a genuine named state is never silently dropped from the denominator.
fn note_class(map: &mut BTreeMap<String, StateClass>, state: &str, class: StateClass) {
    let slot = map.entry(state.to_string()).or_insert(class);
    if class == StateClass::Tracked {
        *slot = StateClass::Tracked;
    }
}

fn add_overlap(
    map: &mut BTreeMap<String, i64>,
    state: &str,
    block_start: DateTime<Utc>,
    block_end: DateTime<Utc>,
    start: Option<DateTime<Utc>>,
    end: DateTime<Utc>,
) {
    let lo = match start {
        Some(s) => block_start.max(s),
        None => block_start,
    };
    let hi = block_end.min(end);
    let secs = (hi - lo).num_seconds();
    if secs > 0 {
        *map.entry(state.to_string()).or_default() += secs;
    }
}

fn within(t: DateTime<Utc>, start: Option<DateTime<Utc>>, end: DateTime<Utc>) -> bool {
    start.is_none_or(|s| t >= s) && t < end
}

// ─── Rendering ──────────────────────────────────────────────────────────────

fn render_text(r: &StatsResult) -> String {
    let total_dur = format_duration(r.total_duration_seconds);

    if let Some(sel) = &r.selected_state {
        let s = &r.states[0];
        return format!(
            "{}:{}  ·  {}  {} of {}  ({}×)\n",
            r.field,
            sel,
            format_duration(s.duration_seconds),
            percent(s.ratio),
            r.field,
            s.count,
        );
    }

    if r.states.is_empty() {
        return format!("No entries for '{}'.\n", r.field);
    }

    let dur_w = r
        .states
        .iter()
        .map(|s| format_duration(s.duration_seconds).len())
        .max()
        .unwrap_or(0);
    let cnt_w = r
        .states
        .iter()
        .map(|s| s.count.to_string().len())
        .max()
        .unwrap_or(0);

    // Ratios are shares of the tracked total, which is not the whole
    // measured total once `false` / untracked are out of the denominator —
    // so name it, rather than leave a column that doesn't sum to the header.
    let tracked_note = if r.tracked_duration_seconds == r.total_duration_seconds {
        String::new()
    } else {
        format!(
            "  ({} tracked)",
            format_duration(r.tracked_duration_seconds)
        )
    };
    let mut out = format!(
        "{}  ·  {} over {} entries{}\n\n",
        r.field, total_dur, r.total_count, tracked_note
    );
    for s in &r.states {
        out.push_str(&format!(
            "  {:>dur_w$}  {:>6}  {:>cnt_w$}×  {}\n",
            format_duration(s.duration_seconds),
            percent(s.ratio),
            s.count,
            s.state,
        ));
    }
    out
}

/// The ratio column. A state outside the denominator has no meaningful
/// share of it — an em dash says so without pretending to a number.
fn percent(ratio: Option<f64>) -> String {
    match ratio {
        Some(r) => format!("{:.1}%", r * 100.0),
        None => "—".to_string(),
    }
}

fn render_json(r: &StatsResult) -> String {
    if let Some(sel) = &r.selected_state {
        let s = &r.states[0];
        return format!(
            "{}\n",
            serde_json::json!({
                "field": r.field,
                "state": sel,
                "count": s.count,
                "duration_seconds": s.duration_seconds,
                "ratio": s.ratio,
            })
        );
    }
    let states: Vec<serde_json::Value> = r
        .states
        .iter()
        .map(|s| {
            serde_json::json!({
                "state": s.state,
                "count": s.count,
                "duration_seconds": s.duration_seconds,
                "ratio": s.ratio,
            })
        })
        .collect();
    format!(
        "{}\n",
        serde_json::json!({
            "field": r.field,
            "total_count": r.total_count,
            "total_duration_seconds": r.total_duration_seconds,
            "tracked_duration_seconds": r.tracked_duration_seconds,
            "states": states,
        })
    )
}
