//! `datum stats` — count / total-duration / per-state-ratio rollup for a
//! field (or a single `field:state`) over a flexible time-slice — and with
//! no field at all, the same table one level out: every field, ranked by
//! the time it was tracked (da-ptuwr).
//!
//! The core life-review statistic: `count` answers "how often", `stats`
//! answers "how much time, and split how". Durations are reconstructed from
//! the entry log — `dur` is not stored for switch-blocks (it is derived), so
//! each entry's block runs from its `occur_time` to the next entry's
//! `occur_time` (switch continuity), with the still-open trailing block
//! running to `now`. An entry that *does* carry a stored `dur` (a discrete
//! block, e.g. `run 30m`) is measured by that span instead of the
//! gap-to-next, and the remainder up to the next entry goes to its
//! `following_state` — or, when it has none, back to the state the block
//! interrupted, because a plain block is an interruption and NULL there
//! means "resume what I was" (da-611d, da-mkqo). Ref algorithm:
//! crew/vremya/scripts/timespent.py.
//!
//! The ratio is a share of *tracked* time, not of wall-clock: the closed
//! state (`false` — every gap between blocks) and the untracked states
//! (`null`, the `IGNORED` sentinel, no state at all) are out of the
//! denominator unless `--include-false` / `--include-untracked` put them
//! back. Without that, a real db's states all read under 1% (da-icba).
//! Counts and durations are unaffected — only the denominator moves.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Utc, Weekday};
use datum_core::db::{Db, ListParams, TimeMetric};
use datum_core::shape::{is_inactive_state, is_off_state};
use datum_core::time::DatumTime;
use datum_parse::input::TimeArgs;
use datum_parse::time::{resolve_timezone, Zone, ZoneTimeline};
use serde_json::Value as JsonValue;

use crate::args::{ShowFlag, StatsArgs, StatsPeriod};
use crate::commands::field_visuals::state_label;
use crate::commands::time_bounds::{parse_since, parse_until};
use crate::config::resolve_week_start;
use crate::render::colors::{field_hex, paint, parse_hex, Rgb};
use datum_parse::time::{format_human_duration, DurationPrecision};

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
    /// The first and last counted entry's instant, in the zone it was
    /// logged in — when the window's record starts and ends (da-aynet).
    /// `None` when the window counts no entry. It is the instant the entry
    /// is counted by, so a block's is its END (da-zwct): its leading edge
    /// may lie before the window, which bounds are never outside of.
    pub earliest: Option<DateTime<FixedOffset>>,
    pub latest: Option<DateTime<FixedOffset>>,
    /// Longest-duration first; ties broken by state name.
    pub states: Vec<StateStat>,
}

/// One field's line in the all-fields rollup (da-ptuwr).
///
/// `duration_seconds` is the field's TRACKED duration — the time it spent
/// in a state that counts, which is [`StatsResult::tracked_duration_seconds`]
/// for that field. Not its total: a block field's total is nearly the whole
/// window, because the gaps between its blocks are the `false` state, and a
/// table ranked on that would rank every field the same (da-icba's reason,
/// one level up).
#[derive(Debug, Clone, PartialEq)]
pub struct FieldStat {
    pub field: String,
    /// The field's RESOLVED colour (`#rrggbb`) — spec override, else
    /// `md5(field)`, exactly as `datum fields --json` gives it (da-yiyj).
    /// The table paints the name with it, so a reader picks a field out of
    /// the list by the colour it wears everywhere else.
    pub color: String,
    pub count: usize,
    pub duration_seconds: i64,
    /// Share of [`AllFieldsStats::tracked_duration_seconds`], or `None`
    /// when nothing at all was tracked in the window.
    pub ratio: Option<f64>,
}

/// Structured result of a bare `stats` — every field, ranked.
///
/// The same table one level out: where [`StatsResult`] ranks the STATES
/// within a field, this ranks the FIELDS within the database. Fields run in
/// parallel, so the total is a sum of per-field tracked time and not a span
/// of wall-clock — it is what the ratio column is a share of.
#[derive(Debug, Clone, PartialEq)]
pub struct AllFieldsStats {
    /// Longest-tracked first; ties broken by field name.
    pub fields: Vec<FieldStat>,
    pub total_count: usize,
    pub tracked_duration_seconds: i64,
    /// [`StatsResult::earliest`] / [`StatsResult::latest`] across every field.
    pub earliest: Option<DateTime<FixedOffset>>,
    pub latest: Option<DateTime<FixedOffset>>,
}

/// Every field the database has logged, ranked by tracked duration.
///
/// A field with nothing in the window is left out rather than carried as a
/// zero row: the table answers "where did the time go", and a name with no
/// time did not take any. Ignored fields stay in — ignoring a field takes
/// it off the widget bar, not out of the record, and `stats <ignored>`
/// answers for one today.
pub fn compute_all_fields(args: &StatsArgs, db: &Db) -> Result<AllFieldsStats> {
    let (start, end) = resolve_window(args, db)?;
    let registry = db.load_registry()?;
    let mut fields: Vec<FieldStat> = Vec::new();
    let (mut earliest, mut latest) = (None, None);
    for field in db.distinct_fields()? {
        let r = field_stats(&field, None, args, db, start, end)?;
        earliest = earliest.into_iter().chain(r.earliest).min();
        latest = latest.into_iter().chain(r.latest).max();
        if r.total_count == 0 && r.tracked_duration_seconds == 0 {
            continue;
        }
        fields.push(FieldStat {
            color: field_hex(&r.field, registry.fields.get(&r.field)),
            field: r.field,
            count: r.total_count,
            duration_seconds: r.tracked_duration_seconds,
            ratio: None,
        });
    }

    let tracked_duration_seconds: i64 = fields.iter().map(|f| f.duration_seconds).sum();
    let total_count: usize = fields.iter().map(|f| f.count).sum();
    for f in &mut fields {
        f.ratio = (tracked_duration_seconds > 0)
            .then(|| f.duration_seconds as f64 / tracked_duration_seconds as f64);
    }
    fields.sort_by(|a, b| {
        b.duration_seconds
            .cmp(&a.duration_seconds)
            .then_with(|| a.field.cmp(&b.field))
    });

    Ok(AllFieldsStats {
        fields,
        total_count,
        tracked_duration_seconds,
        earliest,
        latest,
    })
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

    let (start, end) = resolve_window(args, db)?;
    field_stats(&field, selected_state, args, db, start, end)
}

/// One field's rollup over an already-resolved window — the core both the
/// per-field command and the all-fields table are made of.
fn field_stats(
    field: &str,
    selected_state: Option<String>,
    args: &StatsArgs,
    db: &Db,
    start: Option<DateTime<Utc>>,
    end: DateTime<Utc>,
) -> Result<StatsResult> {
    let field = field.to_string();
    let now = DatumTime::now().utc;

    // Oldest→newest, time-bearing state rows only. `TimeMetric::Occur` drops
    // TRACK documents too (da-c8mh): declaring a field is not an
    // observation of it.
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
        .filter(|e| datum_core::track::track_verb(e).is_none())
        .filter_map(|e| {
            Some(TimelineRow {
                state: state_label(e.state.as_ref()).unwrap_or_else(|| "(none)".to_string()),
                class: classify(e.state.as_ref()),
                occur: e.occur_time.as_ref()?.utc,
                local: e.occur_time.as_ref()?.local_time(),
                dur: e.duration_seconds(),
                following: state_label(e.following_state.as_ref()),
                following_class: classify(e.following_state.as_ref()),
                hands_back: datum_parse::shape::hands_field_back(e.verb),
            })
        })
        .collect();

    let mut dur_by_state: BTreeMap<String, i64> = BTreeMap::new();
    let mut count_by_state: BTreeMap<String, usize> = BTreeMap::new();
    let mut class_by_state: BTreeMap<String, StateClass> = BTreeMap::new();
    let (mut earliest, mut latest) = (None, None);

    // The state the field returns to when a block hands it back: the one
    // that governed just before the block opened. A block is an
    // interruption, so it never becomes that state itself — `None` until
    // some row has actually set a state (a block that interrupted nothing
    // resumes nothing, the answer `now` gives for the same rows).
    let mut governing: Option<(&str, StateClass)> = None;

    // Where a row's influence begins: `occur - dur` for a block, whose
    // stored instant is its END (da-zwct), and `occur` for everything
    // else. This — not the stored instant — is where the row before it
    // stops speaking.
    let leading = |r: &TimelineRow| r.occur - Duration::seconds(r.dur.unwrap_or(0));

    for (i, row) in timeline.iter().enumerate() {
        let next = timeline.get(i + 1).map(&leading).unwrap_or(now);
        let prev = i.checked_sub(1).map(|j| timeline[j].occur);
        // Block A — the row's own state. da-zwct: a stored `dur` is how
        // long the state LASTED up to `row.occur`, so a discrete block is
        // `[occur - dur, occur]` and never reaches past its own instant;
        // clamped at the previous entry's trailing edge so a block logged
        // longer than the gap before it does not claim time another row
        // already spoke for. Without a `dur` the row runs from its
        // instant to where the next entry starts speaking (switch
        // continuity), or to `now` for the still-open trailing block.
        let (block_a_start, block_a_end) = match row.dur {
            Some(d) => {
                let mut from = row.occur - Duration::seconds(d);
                if let Some(prev) = prev {
                    from = from.max(prev);
                }
                (from, row.occur)
            }
            None => (row.occur, next),
        };
        add_overlap(
            &mut dur_by_state,
            &row.state,
            block_a_start,
            block_a_end,
            start,
            end,
        );
        note_class(&mut class_by_state, &row.state, row.class);

        // Block B — what holds from the block's trailing edge (`occur`
        // itself, da-zwct) until the next entry opens. An explicit
        // `following_state` says so outright; a plain
        // block says it by omission — `following_state IS NULL` means
        // "resume what I interrupted" (da-611d), so the remainder belongs to
        // the interrupted state and not to nothing (da-mkqo).
        if row.dur.is_some() {
            if let Some(fs) = &row.following {
                add_overlap(&mut dur_by_state, fs, block_a_end, next, start, end);
                note_class(&mut class_by_state, fs, row.following_class);
            } else if row.hands_back {
                if let Some((resumed, class)) = governing {
                    add_overlap(&mut dur_by_state, resumed, block_a_end, next, start, end);
                    note_class(&mut class_by_state, resumed, class);
                }
            }
        }

        // What the NEXT block will resume to. A hand-back row leaves it
        // untouched, so back-to-back blocks all resume the same state.
        if !row.hands_back {
            governing = match &row.following {
                Some(fs) => Some((fs, row.following_class)),
                None => Some((&row.state, row.class)),
            };
        }

        // Count the entry itself when its occur instant lands in-window.
        // The timeline runs oldest→newest, so the first counted row is the
        // earliest and the last the latest.
        if within(row.occur, start, end) {
            *count_by_state.entry(row.state.clone()).or_default() += 1;
            earliest.get_or_insert(row.local);
            latest = Some(row.local);
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
        earliest,
        latest,
        states,
    })
}

pub fn stats(args: StatsArgs, db: &Db) -> Result<()> {
    // No selector: every field, ranked (da-ptuwr). Bare `stats` was an
    // error until now, which is what the web widget's own default settings
    // already called the model (`DEFAULT_STATS`, web/src/lib/stats.ts).
    if args.selector.is_none() {
        let all = compute_all_fields(&args, db)?;
        match args.show {
            ShowFlag::None => {}
            ShowFlag::Json => print!("{}", render_all_json(&all)),
            _ => print!("{}", render_all_text(&all)),
        }
        return Ok(());
    }
    let result = compute_stats(&args, db)?;
    match args.show {
        ShowFlag::None => {}
        ShowFlag::Json => print!("{}", render_json(&result)),
        _ => print!("{}", render_text(&result)),
    }
    Ok(())
}

// ─── Window resolution ─────────────────────────────────────────────────────

/// Resolve the time-slice into `(start, end)` UTC bounds — half-open
/// `[start, end)`. `start = None` means unbounded below (everything up to
/// `end`). `end` defaults to `now`.
///
/// Exactly one thing may name the *start*: the `SLICE` positional, a period
/// flag, the timing flags, or `--since`. `--until` is not one of them — it
/// refines whichever end the start-source implies, so `-y --until now` turns
/// a bounded day back into an open-ended "since" (da-rf57).
///
/// The timing flags split on whether the user named a clock time:
///
/// * a DATE (`-d 2026-06-15`, `-d sunday`, `-y`, `-yy`) names *that whole
///   day* — `-yy` is the day two days ago, not "since two days ago"; spell
///   the open-ended reading `--since 2d` or `-yy --until now`;
/// * anything carrying a clock time (`-t 9:15`, `-q`, `-d ... -t ...`) is a
///   lower bound, exactly like `--since`.
///
/// Both read the shared `-t` grammar with its ordinary past-facing default
/// (da-0gs4): on a query "10:00" means the most recent 10:00, which is the
/// direction a backward-looking statistic wants.
///
/// Every local boundary here is resolved through the database's
/// [`ZoneTimeline`] — the zone that was local *at that period*, not the one
/// that is local now (da-ztae). A machine that moved Berlin → Lisbon has an
/// hour of skew in every boundary otherwise.
fn resolve_window(args: &StatsArgs, db: &Db) -> Result<(Option<DateTime<Utc>>, DateTime<Utc>)> {
    let now = DatumTime::now().utc;
    let zones = window_zones(args, db)?;

    let sources = [
        (args.slice.is_some(), "a time-slice positional"),
        (args.period.is_some(), "a period flag"),
        (names_a_moment(&args.time), "the timing flags (-t/-d/-q/-y)"),
        (args.since.is_some(), "--since"),
    ];
    let named: Vec<&str> = sources
        .iter()
        .filter(|(given, _)| *given)
        .map(|(_, label)| *label)
        .collect();
    if named.len() > 1 {
        bail!(
            "stats takes one window start, but got {}",
            named.join(" and ")
        );
    }

    // Parsed once, up front: `--until` overrides the end of every branch.
    let until = args.until.as_deref().map(parse_until).transpose()?;

    let (start, end) = if let Some(slice) = &args.slice {
        let (s, e) = parse_slice(slice, &zones)?;
        (Some(s), until.unwrap_or(e))
    } else if let Some(period) = args.period {
        let (s, e) = period_window(period, resolve_week_start()?, &zones, now)?;
        (Some(s), until.unwrap_or(e))
    } else if names_a_moment(&args.time) {
        let built = args.time.build(&DatumTime::now())?;
        let at = built
            .time
            .clone()
            .context("stats does not expose -T, so the timing flags always yield a moment")?;
        if built.only_date {
            let (s, e) = day_window(local_date_at(&zones, at.utc), &zones)?;
            (Some(s), until.unwrap_or(e))
        } else {
            (Some(at.utc), until.unwrap_or(now))
        }
    } else {
        let start = args.since.as_deref().map(parse_since).transpose()?;
        (start, until.unwrap_or(now))
    };

    // An inverted or empty window reports zero, which is indistinguishable
    // from "you did nothing" — the one reading a life-review tool must never
    // hand back by accident. Say so instead.
    if let Some(s) = start {
        if end <= s {
            let show = |t: DateTime<Utc>| {
                zones
                    .zone_at(t)
                    .local_naive(t)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            };
            bail!(
                "empty window: the end ({}) is not after the start ({}), local time in {}",
                show(end),
                show(s),
                zones.zone_at(s).name(),
            );
        }
    }
    Ok((start, end))
}

/// True when the timing flags name a moment at all. `-z` alone only picks
/// the zone boundaries are read in; it is not a window of its own.
fn names_a_moment(t: &TimeArgs) -> bool {
    t.date.is_some() || t.time.is_some() || t.yesterday > 0 || t.quick > 0
}

/// The zone history local boundaries are read in: whatever an explicit `-z`
/// named, else the zone the database says the user was in at that instant.
fn window_zones(args: &StatsArgs, db: &Db) -> Result<ZoneTimeline> {
    match args.time.timezone.as_deref() {
        Some(spec) => Ok(ZoneTimeline::single(Zone::iana(&resolve_timezone(spec)?)?)),
        None => Ok(db.zone_timeline(Zone::host())?),
    }
}

/// `[start, end)` for a whole calendar period, relative to `now`.
///
/// The in-progress periods end at **now**, not at midnight: at 09:00
/// `--today` is nine hours, so every ratio in the output is a share of the
/// day so far rather than of a day mostly unlived. `--last-week` and
/// `--last-month` are closed at both ends — `--last-month` is *the previous
/// month*, never "since the start of it".
fn period_window(
    period: StatsPeriod,
    week_start: Weekday,
    zones: &ZoneTimeline,
    now: DateTime<Utc>,
) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let today = local_date_at(zones, now);
    Ok(match period {
        StatsPeriod::Today => (midnight(zones, today), now),
        StatsPeriod::Week => (midnight(zones, week_open(today, week_start)), now),
        StatsPeriod::LastWeek => {
            let this = week_open(today, week_start);
            (
                midnight(zones, this - Duration::days(7)),
                midnight(zones, this),
            )
        }
        StatsPeriod::Month => (
            midnight(zones, first_of_month(today.year(), today.month())?),
            now,
        ),
        StatsPeriod::LastMonth => {
            let this = first_of_month(today.year(), today.month())?;
            let (py, pm) = if today.month() == 1 {
                (today.year() - 1, 12)
            } else {
                (today.year(), today.month() - 1)
            };
            (
                midnight(zones, first_of_month(py, pm)?),
                midnight(zones, this),
            )
        }
    })
}

/// The most recent `week_start` weekday at or before `date` — `date` itself
/// when it *is* that weekday. Month lengths and year ends need no special
/// case: this is plain day arithmetic.
fn week_open(date: NaiveDate, week_start: Weekday) -> NaiveDate {
    let back = (date.weekday().num_days_from_monday() + 7 - week_start.num_days_from_monday()) % 7;
    date - Duration::days(i64::from(back))
}

/// Shorthand whole-period slice: `YYYY`, `YYYY-MM`, or `YYYY-MM-DD`, each
/// interpreted in local time. Returns half-open `[start, end)` covering the
/// whole period, so `stats activity 2026-06` ≡ `--since 2026-06-01 --until
/// 2026-06-30`.
fn parse_slice(s: &str, zones: &ZoneTimeline) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let digits = |v: &str| v.chars().all(|c| c.is_ascii_digit());

    // YYYY — a whole year.
    if s.len() == 4 && digits(s) {
        let y: i32 = s.parse().context("invalid year")?;
        return Ok((
            midnight(zones, first_of_month(y, 1)?),
            midnight(zones, first_of_month(y + 1, 1)?),
        ));
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
        return Ok((
            midnight(zones, first_of_month(y, m)?),
            midnight(zones, first_of_month(ny, nm)?),
        ));
    }
    // YYYY-MM-DD — a whole day.
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return day_window(date, zones);
    }
    bail!("invalid time-slice '{s}' (expected YYYY, YYYY-MM, or YYYY-MM-DD)")
}

fn first_of_month(year: i32, month: u32) -> Result<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, 1).context("date out of range")
}

/// `[midnight, next midnight)` — the whole of one local day.
fn day_window(date: NaiveDate, zones: &ZoneTimeline) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let next = date.succ_opt().context("date overflow")?;
    Ok((midnight(zones, date), midnight(zones, next)))
}

/// The local date `at` falls on, read in the zone that was in effect then.
fn local_date_at(zones: &ZoneTimeline, at: DateTime<Utc>) -> NaiveDate {
    zones.zone_at(at).local_naive(at).date()
}

fn midnight(zones: &ZoneTimeline, date: NaiveDate) -> DateTime<Utc> {
    // da-ztae: the one sanctioned local→UTC conversion. `for_boundary` takes
    // the first of an ambiguous pair and clamps a nonexistent instant to the
    // transition, so month and day edges abut exactly — nothing skipped, nothing
    // counted twice, even on the two days a year a zone changes offset.
    zones
        .resolve_local(date.and_hms_opt(0, 0, 0).unwrap())
        .for_boundary()
}

// ─── Block math ─────────────────────────────────────────────────────────────

struct TimelineRow {
    state: String,
    class: StateClass,
    occur: DateTime<Utc>,
    /// `occur` in the zone the entry was logged in.
    local: DateTime<FixedOffset>,
    dur: Option<i64>,
    following: Option<String>,
    following_class: StateClass,
    /// Whether this row's trailing edge hands the field back to whatever
    /// it interrupted, rather than deciding the state past that edge.
    hands_back: bool,
}

/// How a state value participates in the ratio denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateClass {
    /// A real state the user tracked — always in the denominator.
    Tracked,
    /// `false` or TRACKED — the field is stopped. This is every gap between blocks.
    Closed,
    /// `null`, the `IGNORED` sentinel, or no state at all.
    Untracked,
}

fn classify(v: Option<&JsonValue>) -> StateClass {
    match v {
        Some(v) if is_off_state(v) => StateClass::Closed,
        v if is_inactive_state(v) => StateClass::Untracked,
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
    let total_dur = format_dur(r.total_duration_seconds);

    if let Some(sel) = &r.selected_state {
        let s = &r.states[0];
        return format!(
            "{}:{}  ·  {}  {} of {}  ({}×)\n",
            r.field,
            sel,
            format_dur(s.duration_seconds),
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
        .map(|s| format_dur(s.duration_seconds).len())
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
        format!("  ({} tracked)", format_dur(r.tracked_duration_seconds))
    };
    let mut out = format!(
        "{}  ·  {} over {} entries{}\n\n",
        r.field, total_dur, r.total_count, tracked_note
    );
    for s in &r.states {
        out.push_str(&format!(
            "  {:>dur_w$}  {:>6}  {:>cnt_w$}×  {}\n",
            format_dur(s.duration_seconds),
            percent(s.ratio),
            s.count,
            s.state,
        ));
    }
    out
}

/// The all-fields table: the same shape as a field's own body, with the
/// FIELD where the state goes. One reader learns one table.
fn render_all_text(r: &AllFieldsStats) -> String {
    if r.fields.is_empty() {
        return "No entries yet.\n".to_string();
    }
    let dur_w = r
        .fields
        .iter()
        .map(|f| format_dur(f.duration_seconds).len())
        .max()
        .unwrap_or(0);
    let cnt_w = r
        .fields
        .iter()
        .map(|f| f.count.to_string().len())
        .max()
        .unwrap_or(0);

    // "tracked", not a bare total: fields run in parallel, so this is a sum
    // of their tracked time and not a span of clock.
    let mut out = format!(
        "all fields  ·  {} tracked over {} entries\n\n",
        format_dur(r.tracked_duration_seconds),
        r.total_count,
    );
    for f in &r.fields {
        out.push_str(&format!(
            "  {:>dur_w$}  {:>6}  {:>cnt_w$}×  {}\n",
            format_dur(f.duration_seconds),
            percent(f.ratio),
            f.count,
            paint(&ink(&f.color), &f.field),
        ));
    }
    out
}

/// The escape a `#rrggbb` paints a name with, or nothing when it does not
/// parse — [`paint`] leaves the body alone for an empty code.
fn ink(hex: &str) -> String {
    parse_hex(hex).map(Rgb::fg).unwrap_or_default()
}

fn render_all_json(r: &AllFieldsStats) -> String {
    let fields: Vec<serde_json::Value> = r
        .fields
        .iter()
        .map(|f| {
            serde_json::json!({
                "field": f.field,
                "color": f.color,
                "count": f.count,
                "duration_seconds": f.duration_seconds,
                "ratio": f.ratio,
            })
        })
        .collect();
    format!(
        "{}\n",
        serde_json::json!({
            "fields": fields,
            "total_count": r.total_count,
            "tracked_duration_seconds": r.tracked_duration_seconds,
            "earliest": r.earliest.map(|t| t.to_rfc3339()),
            "latest": r.latest.map(|t| t.to_rfc3339()),
        })
    )
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
            "earliest": r.earliest.map(|t| t.to_rfc3339()),
            "latest": r.latest.map(|t| t.to_rfc3339()),
            "states": states,
        })
    )
}

/// Stats durations are wall-clock totals of tracked activity, so they
/// name their seconds (`2m5s`) rather than rounding them away.
fn format_dur(secs: i64) -> String {
    format_human_duration(secs, DurationPrecision::Seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn date(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn zone(name: &str) -> ZoneTimeline {
        ZoneTimeline::single(Zone::iana(name).unwrap())
    }

    /// Zones with a DST rule and with none, so every boundary case below can
    /// be asserted somewhere the arithmetic is not trivially 24h.
    fn chicago() -> ZoneTimeline {
        zone("America/Chicago")
    }

    fn window(
        period: StatsPeriod,
        week_start: Weekday,
        zones: &ZoneTimeline,
        now: &str,
    ) -> (DateTime<Utc>, DateTime<Utc>) {
        period_window(period, week_start, zones, utc(now)).unwrap()
    }

    fn hours(span: (DateTime<Utc>, DateTime<Utc>)) -> f64 {
        (span.1 - span.0).num_seconds() as f64 / 3600.0
    }

    // ── week_open ─────────────────────────────────────────────────────

    /// For every weekday and every configured week start, the opening day
    /// is the right weekday, is not in the future, and is less than a week
    /// back. That is the whole contract, and it holds across month and
    /// year ends because this is plain day arithmetic.
    #[test]
    fn week_open_lands_on_the_configured_weekday() {
        let starts = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ];
        let mut day = date("2025-12-28"); // spans a year end
        for _ in 0..21 {
            for start in starts {
                let open = week_open(day, start);
                assert_eq!(open.weekday(), start, "{day} with week_start {start:?}");
                let back = (day - open).num_days();
                assert!((0..7).contains(&back), "{day} → {open} is {back} days");
            }
            day = day.succ_opt().unwrap();
        }
    }

    #[test]
    fn week_open_on_the_start_day_is_that_day() {
        // 2026-06-15 is a Monday.
        assert_eq!(
            week_open(date("2026-06-15"), Weekday::Mon),
            date("2026-06-15")
        );
        // …and with a Sunday week that same day belongs to the week that
        // opened the day before.
        assert_eq!(
            week_open(date("2026-06-15"), Weekday::Sun),
            date("2026-06-14")
        );
    }

    // ── in-progress periods end at now ────────────────────────────────

    #[test]
    fn today_runs_from_midnight_to_now_not_to_midnight() {
        // 09:00 local in Chicago on a plain (non-transition) day.
        let (start, end) = window(
            StatsPeriod::Today,
            Weekday::Mon,
            &chicago(),
            "2026-06-15T14:00:00Z",
        );
        assert_eq!(start, utc("2026-06-15T05:00:00Z"), "local midnight, CDT");
        assert_eq!(end, utc("2026-06-15T14:00:00Z"), "now, not tomorrow");
        assert_eq!(hours((start, end)), 9.0);
    }

    #[test]
    fn week_and_month_also_end_at_now() {
        let now = "2026-06-17T14:00:00Z"; // Wednesday, 09:00 CDT
        let w = window(StatsPeriod::Week, Weekday::Mon, &chicago(), now);
        assert_eq!(w.0, utc("2026-06-15T05:00:00Z"), "Monday midnight");
        assert_eq!(w.1, utc(now));
        let m = window(StatsPeriod::Month, Weekday::Mon, &chicago(), now);
        assert_eq!(m.0, utc("2026-06-01T05:00:00Z"));
        assert_eq!(m.1, utc(now));
    }

    #[test]
    fn week_start_config_moves_the_week_boundary() {
        let now = "2026-06-17T14:00:00Z"; // Wednesday
        let mon = window(StatsPeriod::Week, Weekday::Mon, &chicago(), now);
        let sun = window(StatsPeriod::Week, Weekday::Sun, &chicago(), now);
        assert_eq!(mon.0, utc("2026-06-15T05:00:00Z"));
        assert_eq!(sun.0, utc("2026-06-14T05:00:00Z"), "a day earlier");
    }

    // ── closed periods are bounded at both ends ───────────────────────

    #[test]
    fn last_week_is_a_closed_range_abutting_this_week() {
        let now = "2026-06-17T14:00:00Z";
        let last = window(StatsPeriod::LastWeek, Weekday::Mon, &chicago(), now);
        let this = window(StatsPeriod::Week, Weekday::Mon, &chicago(), now);
        assert_eq!(last.1, this.0, "no gap, no overlap");
        assert_eq!(hours(last), 168.0, "a whole week");
        assert!(last.1 < utc(now), "closed, not running to now");
    }

    #[test]
    fn last_month_is_the_previous_month_not_since_its_start() {
        let now = "2026-06-17T14:00:00Z";
        let last = window(StatsPeriod::LastMonth, Weekday::Mon, &chicago(), now);
        assert_eq!(last.0, utc("2026-05-01T05:00:00Z"));
        assert_eq!(last.1, utc("2026-06-01T05:00:00Z"), "ends when June opens");
        assert!(last.1 < utc(now), "this month is NOT included");
    }

    /// The bead's `--last-month from March 31` case: there is no February
    /// 31, and month arithmetic that walks back by day-of-month would
    /// produce one. Anchoring on the first of the month cannot.
    #[test]
    fn last_month_from_the_31st_reaches_february() {
        let last = window(
            StatsPeriod::LastMonth,
            Weekday::Mon,
            &chicago(),
            "2026-03-31T17:00:00Z",
        );
        assert_eq!(last.0, utc("2026-02-01T06:00:00Z"), "Feb 1, CST");
        assert_eq!(last.1, utc("2026-03-01T06:00:00Z"));
        assert_eq!(hours(last), 28.0 * 24.0, "2026 is not a leap year");
    }

    #[test]
    fn month_lengths_of_28_29_30_and_31_days_all_come_out_whole() {
        // (now, expected length of the previous month in days)
        for (now, days) in [
            ("2026-03-15T12:00:00Z", 28.0), // Feb 2026
            ("2024-03-15T12:00:00Z", 29.0), // Feb 2024, a leap year
            ("2026-05-15T12:00:00Z", 30.0), // April
            ("2026-06-15T12:00:00Z", 31.0), // May
        ] {
            // UTC has no DST, so a whole month is exactly 24h per day.
            let last = window(StatsPeriod::LastMonth, Weekday::Mon, &zone("UTC"), now);
            assert_eq!(hours(last), days * 24.0, "previous month at {now}");
        }
    }

    #[test]
    fn january_looks_back_into_the_previous_year() {
        let last = window(
            StatsPeriod::LastMonth,
            Weekday::Mon,
            &zone("UTC"),
            "2026-01-15T12:00:00Z",
        );
        assert_eq!(last.0, utc("2025-12-01T00:00:00Z"));
        assert_eq!(last.1, utc("2026-01-01T00:00:00Z"));
    }

    // ── DST ───────────────────────────────────────────────────────────

    /// Spring forward: 2026-03-08 in Chicago is a 23-hour day, and the
    /// month containing it is an hour short of 31×24. A boundary computed
    /// as "midnight + N days" would silently be an hour out.
    #[test]
    fn dst_days_and_months_are_measured_not_assumed() {
        let short_day = window(
            StatsPeriod::LastMonth,
            Weekday::Mon,
            &chicago(),
            "2026-04-10T12:00:00Z",
        );
        assert_eq!(hours(short_day), 31.0 * 24.0 - 1.0, "March loses an hour");

        let long_month = window(
            StatsPeriod::LastMonth,
            Weekday::Mon,
            &chicago(),
            "2026-12-10T12:00:00Z",
        );
        assert_eq!(hours(long_month), 30.0 * 24.0 + 1.0, "November gains one");
    }

    #[test]
    fn a_week_containing_a_transition_is_not_168_hours() {
        // The week of 2026-03-08 (spring forward) in Chicago.
        let last = window(
            StatsPeriod::LastWeek,
            Weekday::Mon,
            &chicago(),
            "2026-03-12T12:00:00Z",
        );
        assert_eq!(last.0, utc("2026-03-02T06:00:00Z"));
        assert_eq!(last.1, utc("2026-03-09T05:00:00Z"));
        assert_eq!(hours(last), 167.0);
    }

    // ── the zone that was local at the period ─────────────────────────

    /// vividn's rule: a boundary is read in the zone that was local *at that
    /// period*, not the one that is local now. This machine moved Berlin →
    /// Lisbon; the month before the move opens on Berlin's midnight even
    /// though "now" is in Lisbon.
    #[test]
    fn boundaries_use_the_zone_in_effect_at_the_period() {
        let moved = utc("2026-07-25T12:00:00Z");
        let timeline = ZoneTimeline::new(
            [
                (
                    utc("2026-01-01T00:00:00Z"),
                    Zone::iana("Europe/Berlin").unwrap(),
                ),
                (moved, Zone::iana("Europe/Lisbon").unwrap()),
            ],
            Zone::iana("UTC").unwrap(),
        );
        // Now is August, in Lisbon (UTC+1 in summer): August opens at
        // 23:00 UTC on July 31 …
        let this = window(
            StatsPeriod::Month,
            Weekday::Mon,
            &timeline,
            "2026-08-10T12:00:00Z",
        );
        assert_eq!(this.0, utc("2026-07-31T23:00:00Z"), "Lisbon midnight");
        // … while June, wholly before the move, opens on Berlin's midnight
        // (UTC+2 in summer), an hour earlier in UTC than Lisbon's would be.
        let june = window(
            StatsPeriod::LastMonth,
            Weekday::Mon,
            &timeline,
            "2026-07-10T12:00:00Z",
        );
        assert_eq!(june.0, utc("2026-05-31T22:00:00Z"), "Berlin midnight");
        assert_eq!(june.1, utc("2026-06-30T22:00:00Z"));
    }

    // ── day / slice windows ───────────────────────────────────────────

    #[test]
    fn a_day_window_is_midnight_to_midnight() {
        let (start, end) = day_window(date("2026-06-15"), &chicago()).unwrap();
        assert_eq!(start, utc("2026-06-15T05:00:00Z"));
        assert_eq!(end, utc("2026-06-16T05:00:00Z"));
    }

    #[test]
    fn slice_forms_cover_their_whole_period() {
        let tl = zone("UTC");
        assert_eq!(
            parse_slice("2026", &tl).unwrap(),
            (utc("2026-01-01T00:00:00Z"), utc("2027-01-01T00:00:00Z"))
        );
        assert_eq!(
            parse_slice("2026-12", &tl).unwrap(),
            (utc("2026-12-01T00:00:00Z"), utc("2027-01-01T00:00:00Z")),
            "December rolls into the next year"
        );
        assert_eq!(
            parse_slice("2026-06-15", &tl).unwrap(),
            (utc("2026-06-15T00:00:00Z"), utc("2026-06-16T00:00:00Z"))
        );
        assert!(parse_slice("2026-13", &tl).is_err());
        assert!(parse_slice("junes", &tl).is_err());
    }

    // ── which flags name a window ─────────────────────────────────────

    #[test]
    fn only_time_bearing_flags_name_a_window() {
        let mut t = TimeArgs::default();
        assert!(!names_a_moment(&t));
        // `-z` picks the zone boundaries are read in; it is not a window.
        t.timezone = Some("UTC".to_string());
        assert!(!names_a_moment(&t));
        for set in [
            (|a: &mut TimeArgs| a.date = Some("sunday".into())) as fn(&mut TimeArgs),
            |a: &mut TimeArgs| a.time = Some("9:15".into()),
            |a: &mut TimeArgs| a.yesterday = 1,
            |a: &mut TimeArgs| a.quick = 1,
        ] {
            let mut t = TimeArgs::default();
            set(&mut t);
            assert!(names_a_moment(&t));
        }
    }

    #[test]
    fn json_carries_the_window_bounds_and_null_when_it_counts_nothing() {
        let at = DateTime::parse_from_rfc3339("2026-06-15T04:00:00-05:00").unwrap();
        let one = StatsResult {
            field: "coffee".into(),
            selected_state: None,
            total_count: 1,
            total_duration_seconds: 0,
            tracked_duration_seconds: 0,
            earliest: Some(at),
            latest: Some(at),
            states: vec![],
        };
        let json: JsonValue = serde_json::from_str(&render_json(&one)).unwrap();
        assert_eq!(json["earliest"], "2026-06-15T04:00:00-05:00");
        assert_eq!(json["latest"], "2026-06-15T04:00:00-05:00");

        let none = AllFieldsStats {
            fields: vec![],
            total_count: 0,
            tracked_duration_seconds: 0,
            earliest: None,
            latest: None,
        };
        let json: JsonValue = serde_json::from_str(&render_all_json(&none)).unwrap();
        // null, not left out: the widget reads the key either way.
        assert_eq!(json.get("earliest"), Some(&JsonValue::Null));
        assert_eq!(json.get("latest"), Some(&JsonValue::Null));
    }
}
