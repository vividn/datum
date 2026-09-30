//! `datum stats` — count / total-duration / per-state-ratio rollup for a
//! field (or a single `field:state`) over a flexible time-slice — and with
//! no field at all, the same table one level out: every field, ranked by
//! the time it was tracked (da-ptuwr).
//!
//! The core life-review statistic: `count` answers "how often", `stats`
//! answers "how much time, and split how". Durations are the field's state
//! spans as the one state oracle reads them ([`FieldStateRows::changes`],
//! da-795cy): each change holds until the next, the last until `now`. So a
//! discrete block (`run 30m`) owns its own span, what follows it is its
//! `following_state` — or, when it has none, the state it interrupted,
//! because a plain block is an interruption and NULL there means "resume
//! what I was" (da-611d, da-mkqo) — and the rule is `now`'s and the tail's,
//! not a second copy of it. Ref algorithm: crew/vremya/scripts/timespent.py.
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
use datum_core::state::{FieldStateRows, StateChange};
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
    /// How often, for a field that took no time in the window — an occur
    /// field's, whose only "duration" is the gap between its instants
    /// (da-uv7fu). Of the selected state's count when there is one. `None`
    /// when the field tracked any time, or counted nothing.
    pub frequency: Option<Frequency>,
    /// How often, or how much time, per the one unit that suits the field
    /// (da-ehxou) — over the same span as [`Self::frequency`]. `None` when
    /// the window counts no entry.
    pub rate: Option<Rate>,
    /// The field as of the window's end over fixed calendar periods —
    /// today, this week, … YTD — whatever the window's start (da-ehxou).
    /// Empty for the all-fields table, which does not compute them.
    pub periods: Vec<Period>,
}

/// One named period of [`StatsResult::periods`]: how many entries it
/// counted and how much time it tracked, of the selected state when there
/// is one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Period {
    pub name: &'static str,
    pub count: usize,
    pub duration_seconds: i64,
}

/// What a rate can be per, with its length in days.
const PER: [(&str, f64); 4] = [
    ("day", 1.0),
    ("week", 7.0),
    ("month", 365.2425 / 12.0),
    ("year", 365.2425),
];

/// ONE rate in the unit that suits the field (da-ehxou). vividn: «I would
/// pick a denominator that makes the most sense for a given field depending
/// on its occurrence frequency rather than showing all of them».
///
/// The rule: the smallest of day / week / month / year in which the mean is
/// at least one — one entry, or for a field that takes time one HOUR (the
/// same rule on the time's own scale) — so coffee reads /day and a haircut
/// /month. A unit longer than the span is never picked: when the smallest
/// qualifying unit is longer, the longest unit the span covers stands in,
/// so three days of data never reads «/year» (#232's 504/year did).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rate {
    /// `day`, `week`, `month` or `year`.
    pub per: &'static str,
    /// Entries, or seconds when `timed`, per `per`.
    pub amount: f64,
    pub timed: bool,
}

impl Rate {
    fn new(amount: f64, span_seconds: i64, timed: bool) -> Self {
        let span_days = span_seconds.max(86_400) as f64 / 86_400.0;
        let per_day = amount / span_days;
        let threshold = if timed { 3600.0 } else { 1.0 };
        // A day is always covered: the span is never under one.
        let covered = &PER[..PER.iter().filter(|(_, d)| *d <= span_days).count()];
        let (per, days) = covered
            .iter()
            .find(|(_, d)| per_day * d >= threshold)
            .or(covered.last())
            .copied()
            .unwrap_or(PER[0]);
        Rate {
            per,
            amount: per_day * days,
            timed,
        }
    }
}

impl std::fmt::Display for Rate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.timed {
            true => write!(f, "{}/{}", format_dur(self.amount.round() as i64), self.per),
            false => write!(f, "{}/{}", rate(self.amount), self.per),
        }
    }
}

/// Entries per day / week / month / year over `span_seconds`: from the
/// first counted entry to the window's end, and never under a day, so one
/// entry logged a minute ago is not a thousand a day (da-uv7fu).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frequency {
    pub span_seconds: i64,
    pub per_day: f64,
    pub per_week: f64,
    pub per_month: f64,
    pub per_year: f64,
}

impl Frequency {
    fn new(count: usize, span_seconds: i64) -> Self {
        let span_seconds = span_seconds.max(86_400);
        let per_day = count as f64 * 86_400.0 / span_seconds as f64;
        let year = 365.2425;
        Frequency {
            span_seconds,
            per_day,
            per_week: per_day * 7.0,
            per_month: per_day * year / 12.0,
            per_year: per_day * year,
        }
    }
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
    /// [`StatsResult::frequency`]: how often, when the field took no time.
    pub frequency: Option<Frequency>,
    /// [`StatsResult::rate`].
    pub rate: Option<Rate>,
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
    let (start, end, _) = resolve_window(args, db)?;
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
            frequency: r.frequency,
            rate: r.rate,
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

    let (start, end, zones) = resolve_window(args, db)?;
    // Every period is read as of the window's end, in the window's zones.
    let anchor = end.min(DatumTime::now().utc);
    let periods = named_periods(anchor, resolve_week_start()?, &zones)?;
    // One read covers the window and every period.
    let from = start.map(|s| periods.iter().map(|(_, (p, _))| *p).fold(s, DateTime::min));
    let until = periods
        .iter()
        .map(|(_, (_, e))| *e)
        .fold(end, DateTime::max);
    let timeline = load_timeline(&field, db, from, until)?;
    let mut r = rollup(&field, selected_state.clone(), args, &timeline, start, end);
    for (name, (s, e)) in periods {
        let p = rollup(&field, selected_state.clone(), args, &timeline, Some(s), e);
        let (count, duration_seconds) = p.subject();
        r.periods.push(Period {
            name,
            count,
            duration_seconds,
        });
    }
    Ok(r)
}

impl StatsResult {
    /// The count and the time of what the selector names: the one state,
    /// or the whole field's tracked time.
    fn subject(&self) -> (usize, i64) {
        match &self.selected_state {
            Some(_) => (self.states[0].count, self.states[0].duration_seconds),
            None => (self.total_count, self.tracked_duration_seconds),
        }
    }
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
    let timeline = load_timeline(field, db, start, end)?;
    Ok(rollup(field, selected_state, args, &timeline, start, end))
}

/// The field's timeline for rollups within `[from, until)`: its whole
/// history when `from` is unbounded, otherwise only the rows that decide
/// the spans there (da-307e0).
fn load_timeline(
    field: &str,
    db: &Db,
    from: Option<DateTime<Utc>>,
    until: DateTime<Utc>,
) -> Result<Timeline> {
    // Oldest→newest, time-bearing state rows only. `TimeMetric::Occur` drops
    // TRACK documents too (da-c8mh): declaring a field is not an
    // observation of it.
    // timeless `add`/`note` rows (NULL occur_time) and the default meta
    // filter drops `flag` / protocol-note markers — the same exclusions
    // tail / dayview / now apply, so the timeline matches what the user sees.
    let params = ListParams {
        fields: vec![field.to_string()],
        metric: TimeMetric::Occur,
        ..Default::default()
    };
    let entries = db.list_entries(&params)?;
    let rows = entries
        .iter()
        .filter(|e| datum_core::track::track_verb(e).is_none())
        .filter_map(|e| {
            Some(TimelineRow {
                state: state_label(e.state.as_ref()).unwrap_or_else(|| "(none)".to_string()),
                class: classify(e.state.as_ref()),
                occur: e.occur_time.as_ref()?.utc,
                local: e.occur_time.as_ref()?.local_time(),
            })
        })
        .collect();
    let spans = state_spans(field, db, from, until)?;
    Ok(Timeline { rows, spans })
}

/// The field's state spans for rollups within `[from, until)`.
fn state_spans(
    field: &str,
    db: &Db,
    from: Option<DateTime<Utc>>,
    until: DateTime<Utc>,
) -> Result<Vec<StateSpan>> {
    let span = |c: StateChange| StateSpan {
        // A block that interrupted nothing hands back no state: the
        // stretch after it is left unattributed (da-phoe).
        state: c.governing.and(state_label(Some(&c.state))),
        class: classify(Some(&c.state)),
        from: c.at,
    };
    if let Some(from) = from {
        // The last span runs to the next change, or to `now` when there
        // is none: past `now`, the next change can be any later one.
        let until = match until > DatumTime::now().utc {
            true => DateTime::<Utc>::from_timestamp(253_402_300_799, 0).expect("9999-12-31"),
            false => until,
        };
        let window = FieldStateRows::load_transitions_between(db.conn(), field, from, until)?;
        let changes = window.changes_between(from, until);
        // A Null at `from` that a row may decide is the one state the
        // window cannot label: the whole history's walk keeps a Null run
        // as one change whether a row decides it or none does, so the
        // run's label is set where it began, which the window need not
        // reach.
        let null = changes
            .first()
            .is_none_or(|c| c.at != from || c.state.is_null());
        if !(null && window.says_null()) {
            return Ok(changes.into_iter().map(span).collect());
        }
    }
    let history = FieldStateRows::load_transitions(db.conn(), field)?;
    Ok(history.changes().into_iter().map(span).collect())
}

/// The rollup of an already-loaded timeline over `[start, end)`.
fn rollup(
    field: &str,
    selected_state: Option<String>,
    args: &StatsArgs,
    timeline: &Timeline,
    start: Option<DateTime<Utc>>,
    end: DateTime<Utc>,
) -> StatsResult {
    let field = field.to_string();
    let now = DatumTime::now().utc;

    let mut dur_by_state: BTreeMap<String, i64> = BTreeMap::new();
    let mut count_by_state: BTreeMap<String, usize> = BTreeMap::new();
    let mut class_by_state: BTreeMap<String, StateClass> = BTreeMap::new();
    let (mut earliest, mut latest) = (None, None);

    // Each state holds until the next change, the last until `now`.
    for (i, span) in timeline.spans.iter().enumerate() {
        let Some(state) = &span.state else { continue };
        let to = timeline.spans.get(i + 1).map_or(now, |next| next.from);
        add_overlap(&mut dur_by_state, state, span.from, to, start, end);
        note_class(&mut class_by_state, state, span.class);
    }

    for row in &timeline.rows {
        note_class(&mut class_by_state, &row.state, row.class);
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

    let mut r = StatsResult {
        field,
        selected_state,
        total_count,
        total_duration_seconds,
        tracked_duration_seconds,
        earliest,
        latest,
        states,
        frequency: None,
        rate: None,
        periods: Vec::new(),
    };
    let (count, secs) = r.subject();
    let timed = r.tracked_duration_seconds > 0;
    let span = earliest.map(|first| (end.min(now) - first.to_utc()).num_seconds());
    r.frequency = span.filter(|_| !timed).map(|s| Frequency::new(count, s));
    r.rate = span.map(|s| match timed {
        true => Rate::new(secs as f64, s, true),
        false => Rate::new(count as f64, s, false),
    });
    r
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
/// hour of skew in every boundary otherwise. That timeline is returned
/// too, so the periods line reads the same zones without a second query.
fn resolve_window(
    args: &StatsArgs,
    db: &Db,
) -> Result<(Option<DateTime<Utc>>, DateTime<Utc>, ZoneTimeline)> {
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
    Ok((start, end, zones))
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

/// A half-open `[start, end)`.
type Span = (DateTime<Utc>, DateTime<Utc>);

/// The periods line's windows as of `anchor`, keyed by the names the JSON
/// carries (da-ehxou). The calendar ones are `--today`'s and friends' own
/// windows; the rolling ones are whole days back from the anchor.
fn named_periods(
    anchor: DateTime<Utc>,
    week_start: Weekday,
    zones: &ZoneTimeline,
) -> Result<[(&'static str, Span); 8]> {
    let calendar = |p| period_window(p, week_start, zones, anchor);
    let back = |days| (anchor - Duration::days(days), anchor);
    let year = local_date_at(zones, anchor).year();
    Ok([
        ("today", calendar(StatsPeriod::Today)?),
        ("this_week", calendar(StatsPeriod::Week)?),
        ("last_7_days", back(7)),
        ("last_week", calendar(StatsPeriod::LastWeek)?),
        ("this_month", calendar(StatsPeriod::Month)?),
        ("last_30_days", back(30)),
        ("last_365_days", back(365)),
        ("ytd", (midnight(zones, first_of_month(year, 1)?), anchor)),
    ])
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

/// A field's entries, which are counted, and its state spans, which are
/// timed.
struct Timeline {
    rows: Vec<TimelineRow>,
    spans: Vec<StateSpan>,
}

struct TimelineRow {
    state: String,
    class: StateClass,
    occur: DateTime<Utc>,
    /// `occur` in the zone the entry was logged in.
    local: DateTime<FixedOffset>,
}

/// One state change: `state` from `from` until the next one — `None`
/// when no row decides it.
struct StateSpan {
    state: Option<String>,
    class: StateClass,
    from: DateTime<Utc>,
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

    // A field that took no time says how often instead, and no time at all:
    // an occur field's "duration" is only the gaps between its instants.
    if r.frequency.is_some() {
        let since = r
            .earliest
            .map(|t| t.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let (name, count) = match &r.selected_state {
            Some(sel) => (format!("{}:{}", r.field, sel), r.states[0].count),
            None => (r.field.clone(), r.total_count),
        };
        let rate = r.rate.map(|q| q.to_string()).unwrap_or_default();
        let mut out = format!(
            "{name}  ·  {rate}  ·  {} total since {since}\n{}",
            group(count as f64),
            periods_line(r),
        );
        if r.selected_state.is_none() {
            let w = r.states.iter().map(|s| s.count.to_string().len()).max();
            let w = w.unwrap_or(0);
            out.push('\n');
            // How often, so a state no entry logged has no line: an
            // occur's TRACKED (da-795cy) holds time and nothing else.
            for s in r.states.iter().filter(|s| s.count > 0) {
                out.push_str(&format!("  {:>w$}×  {}\n", s.count, s.state));
            }
        }
        return out;
    }

    if let Some(sel) = &r.selected_state {
        let s = &r.states[0];
        return format!(
            "{}:{}  ·  {}  {} of {}  ({}×){}\n{}",
            r.field,
            sel,
            format_dur(s.duration_seconds),
            percent(s.ratio),
            r.field,
            s.count,
            rate_note(r),
            periods_line(r),
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
        "{}  ·  {} over {} entries{}{}\n{}\n",
        r.field,
        total_dur,
        r.total_count,
        tracked_note,
        rate_note(r),
        periods_line(r),
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
    // The time a field took, or — for one that took none — how often it
    // was logged, with no share of the time beside it.
    let cell = |f: &FieldStat| match f.rate.filter(|q| !q.timed) {
        Some(q) => (q.to_string(), percent(None)),
        None => (format_dur(f.duration_seconds), percent(f.ratio)),
    };
    let dur_w = r
        .fields
        .iter()
        .map(|f| cell(f).0.chars().count())
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
        let (amount, share) = cell(f);
        out.push_str(&format!(
            "  {:>dur_w$}  {:>6}  {:>cnt_w$}×  {}\n",
            amount,
            share,
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
                "frequency": f.frequency.as_ref().map(frequency_json),
                "rate": f.rate.as_ref().map(rate_json),
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
                "frequency": r.frequency.as_ref().map(frequency_json),
                "rate": r.rate.as_ref().map(rate_json),
                "periods": periods_json(r),
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
            "frequency": r.frequency.as_ref().map(frequency_json),
            "rate": r.rate.as_ref().map(rate_json),
            "periods": periods_json(r),
        })
    )
}

/// `{"per": "day", "count": 3.2}`, or `duration_seconds` for a field that
/// took time — the keys a state's own row uses for the same two measures.
fn rate_json(q: &Rate) -> JsonValue {
    let measure = if q.timed { "duration_seconds" } else { "count" };
    serde_json::json!({ "per": q.per, measure: q.amount })
}

/// Every period by name, each with both figures, so the widget can choose.
fn periods_json(r: &StatsResult) -> JsonValue {
    r.periods
        .iter()
        .map(|p| {
            let figures = serde_json::json!({
                "count": p.count,
                "duration_seconds": p.duration_seconds,
            });
            (p.name.to_string(), figures)
        })
        .collect::<serde_json::Map<_, _>>()
        .into()
}

fn frequency_json(f: &Frequency) -> JsonValue {
    serde_json::json!({
        "span_seconds": f.span_seconds,
        "per_day": f.per_day,
        "per_week": f.per_week,
        "per_month": f.per_month,
        "per_year": f.per_year,
    })
}

/// `  ·  45m/day` after a timed head line, or nothing without a rate.
fn rate_note(r: &StatsResult) -> String {
    r.rate.map(|q| format!("  ·  {q}")).unwrap_or_default()
}

/// `  today 2 · this week 9 · 7 d 11 · … · YTD 310`, a count for a field
/// that took no time and the time for one that did. Empty with no periods.
fn periods_line(r: &StatsResult) -> String {
    if r.periods.is_empty() {
        return String::new();
    }
    let timed = r.tracked_duration_seconds > 0;
    let cells: Vec<String> = r
        .periods
        .iter()
        .map(|p| {
            let label = match p.name {
                "this_week" => "this week",
                "last_7_days" => "7 d",
                "last_week" => "last week",
                "this_month" => "this month",
                "last_30_days" => "30 d",
                "last_365_days" => "365 d",
                "ytd" => "YTD",
                name => name,
            };
            match timed {
                true => format!("{label} {}", format_dur(p.duration_seconds)),
                false => format!("{label} {}", group(p.count as f64)),
            }
        })
        .collect();
    format!("  {}\n", cells.join(" · "))
}

/// Two significant figures under ten, so a rare field reads `0.041/day`
/// and never `0.0/day`; whole numbers, grouped, from ten up.
fn rate(v: f64) -> String {
    if v >= 10.0 || v <= 0.0 {
        return group(v);
    }
    let decimals = (1 - v.log10().floor() as i32).max(0) as usize;
    let s = format!("{v:.decimals$}");
    match s.contains('.') {
        true => s.trim_end_matches('0').trim_end_matches('.').to_string(),
        false => s,
    }
}

/// A whole number with thousands grouped: `1,168`.
fn group(v: f64) -> String {
    let digits = format!("{:.0}", v.max(0.0));
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
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
            frequency: None,
            rate: None,
            periods: vec![],
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

    /// da-307e0: a bounded window reads only the rows that decide its
    /// spans, and every rollup inside it is the whole history's, at every
    /// edge of every history and a second either side of it — with the
    /// window ending before `now` and after it, and a row in the future.
    #[test]
    fn a_windowed_timeline_rolls_up_as_the_whole_history_does() {
        use datum_core::action::Action;
        use datum_core::dur::Dur;
        use datum_core::entry::{Entry, Verb};
        use datum_parse::time::clock::{install, uninstall, MockClock};
        use serde_json::json;
        use std::sync::Arc;

        let base = utc("2026-01-01T00:00:00Z");
        let at = |min: i64| DatumTime {
            utc: base + Duration::minutes(min),
            offset_minutes: 0,
            timezone: Some("UTC".into()),
            uncertainty: None,
        };
        let now = 1000;
        install(Arc::new(MockClock::new(at(now))));
        // (verb, state, occur minute, dur minutes, following_state)
        type Row = (Verb, Option<JsonValue>, i64, Option<i64>, Option<JsonValue>);
        let sw = |s: JsonValue, t: i64| -> Row { (Verb::Switch, Some(s), t, None, None) };
        let block =
            |s: Option<JsonValue>, t: i64, d: i64| -> Row { (Verb::Block, s, t, Some(d), None) };
        let dot = |t: i64| -> Row { (Verb::Point, None, t, None, None) };
        let histories: Vec<(&str, Vec<Row>)> = vec![
            (
                "switch",
                vec![
                    sw(json!("a"), 100),
                    sw(json!("b"), 400),
                    sw(json!("a"), 700),
                ],
            ),
            (
                "start-stop",
                vec![
                    sw(json!(true), 100),
                    sw(json!(false), 300),
                    block(Some(json!(true)), 600, 60),
                ],
            ),
            (
                "interrupted",
                vec![
                    sw(json!("s1"), 100),
                    block(Some(json!("x")), 500, 100),
                    sw(json!("s2"), 800),
                ],
            ),
            (
                "compound",
                vec![
                    sw(json!("s1"), 100),
                    (
                        Verb::BlockSwitch,
                        Some(json!("sit")),
                        500,
                        Some(100),
                        Some(json!("up")),
                    ),
                ],
            ),
            (
                "blocks-only",
                vec![
                    block(Some(json!("x")), 200, 30),
                    block(Some(json!("x")), 500, 30),
                    block(Some(json!("y")), 900, 30),
                ],
            ),
            (
                "stateless-blocks",
                vec![block(None, 200, 30), block(None, 500, 30), dot(700)],
            ),
            (
                "null-switch",
                vec![
                    block(Some(json!("x")), 200, 30),
                    sw(JsonValue::Null, 400),
                    sw(json!("a"), 600),
                ],
            ),
            (
                "ignored",
                vec![
                    sw(json!("IGNORED"), 100),
                    dot(300),
                    sw(json!("x"), 500),
                    dot(600),
                ],
            ),
            ("dots", vec![dot(100), dot(300), dot(900)]),
            ("future", vec![sw(json!("a"), 100), sw(json!("b"), 1300)]),
            (
                "declared",
                vec![
                    block(Some(json!("x")), 200, 30),
                    block(Some(json!("x")), 500, 30),
                ],
            ),
            (
                "deep",
                (1..=80)
                    .map(|i| block(Some(json!("x")), 100 + i * 10, 5))
                    .chain([sw(json!("z"), 950)])
                    .collect(),
            ),
        ];
        let db = Db::open_in_memory().unwrap();
        // A declaration is not a transition, so it must not decide a span.
        let declared = datum_core::track::track_entry("declared", Action::Start, at(50), "d1");
        db.insert_entry(&declared).unwrap();
        for (field, rows) in &histories {
            let mut edges = vec![now];
            for (verb, state, t, d, following) in rows {
                let dur = d.map(|d| Dur::of(Duration::minutes(d)));
                let mut e = Entry::new(*field, *verb, state.clone(), None, Some(at(*t)), dur, "d1");
                e.following_state = following.clone();
                db.insert_entry(&e).unwrap();
                edges.extend([*t, t - d.unwrap_or(0)]);
            }
            let whole = load_timeline(field, &db, None, base).unwrap();
            let args = StatsArgs::default();
            let secs = |min: i64| Duration::minutes(min);
            edges.sort();
            edges.dedup();
            // Every edge of a short history; a sample of the long one's.
            for edge in edges.iter().step_by(edges.len() / 24 + 1) {
                for from in [-1, 0, 1].map(|s| base + secs(*edge) + Duration::seconds(s)) {
                    for until in [
                        from + secs(30),
                        from + secs(400),
                        at(now).utc,
                        at(now + 1440).utc,
                    ] {
                        if until <= from {
                            continue;
                        }
                        let window = load_timeline(field, &db, Some(from), until).unwrap();
                        for start in [from, from + (until - from) / 2] {
                            let roll =
                                |t: &Timeline| rollup(field, None, &args, t, Some(start), until);
                            assert_eq!(
                                roll(&window),
                                roll(&whole),
                                "{field} over [{start}, {until})"
                            );
                        }
                    }
                }
            }
        }
        // PR #369 review: a window inside a Null decided BEFORE it, with no
        // row of its own. The deciding row is still read (the look-back),
        // so the whole history answers there.
        let (from, until) = (at(450).utc, at(480).utc);
        let window =
            FieldStateRows::load_transitions_between(db.conn(), "null-switch", from, until);
        assert!(
            window.unwrap().says_null(),
            "the null switch before the window is read"
        );
        let args = StatsArgs::default();
        let roll = |from| {
            let t = load_timeline("null-switch", &db, from, until).unwrap();
            rollup("null-switch", None, &args, &t, Some(at(450).utc), until)
        };
        assert_eq!(roll(Some(from)), roll(None), "null-switch inside its null");
        uninstall();
    }
}
