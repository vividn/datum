//! Shared `--since` / `--until` bound parsing for the query commands
//! (`count`, `ids`, `stats`).
//!
//! These flags name a *moment*, so they defer to [`parse_time_arg`] — the
//! parser behind `-t` and every logging command — rather than the private
//! reimplementation each command used to carry (da-rye2). Routing them
//! here means every future addition to the shared time vocabulary reaches
//! all three query surfaces for free.

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use datum_parse::time::{parse_date_arg, parse_duration, parse_time_arg, DatumTime, TimeBuilder};
use datum_parse::time::{TimeCtx, Zone};

/// Which end of the window a bound is being parsed for. Only matters for
/// date-only inputs, which name a whole local day: `--since 2026-06-01`
/// starts at that day's midnight, `--until 2026-06-01` runs through its
/// end (i.e. the *next* midnight, so the named day is included).
#[derive(Debug, Clone, Copy)]
pub(crate) enum Edge {
    Since,
    Until,
}

impl Edge {
    fn flag(self) -> &'static str {
        match self {
            Edge::Since => "--since",
            Edge::Until => "--until",
        }
    }
}

/// Parse `--since` into a UTC lower bound.
pub(crate) fn parse_since(s: &str) -> Result<DateTime<Utc>> {
    parse_bound(s, Edge::Since)
}

/// Parse `--until` into a UTC upper bound.
pub(crate) fn parse_until(s: &str) -> Result<DateTime<Utc>> {
    parse_bound(s, Edge::Until)
}

/// Resolve a window bound into a UTC instant.
///
/// Grammar, tried in order:
///
///   1. An unsigned duration (`1d`, `30m`, `1:30`, `PT5M`, bare `5` =
///      5 min) — read as "that long ago". This tier goes first because
///      the duration dialect owns the ambiguous numeric forms: `--since
///      1:30` has always meant 90 minutes ago here, and the `-t` grammar
///      would read it as 01:30 today. Signed forms are deliberately
///      excluded — `-30` / `+2h` is `-t`'s own spelling for a relative
///      offset, so it resolves in tier 3 and the sign points the same
///      way on every surface.
///   2. A calendar date via [`parse_date_arg`] — `2026-06-01`, `apr29`.
///      Resolved to the local midnight that opens (`Since`) or closes
///      (`Until`) that day. Only the absolute forms: a bare integer is a
///      day offset to `-d`, and tier 1 has already claimed it.
///   3. The shared `-t` grammar via [`parse_time_arg`] — `now`, clock
///      times (`9:15am`, `21:30`), signed offsets (`-30`, `+2h`),
///      ISO-8601 datetimes with or without a zone suffix, and every word
///      the shared vocabulary learns from here on.
fn parse_bound(s: &str, edge: Edge) -> Result<DateTime<Utc>> {
    if !s.starts_with(['+', '-']) {
        if let Ok(dur) = parse_duration(s) {
            // Through `DatumTime::now`, like every other clock read in the
            // codebase — `Utc::now()` would ignore an installed clock and
            // make `--since 1h` disagree with `-t -60` under a test clock.
            return Ok(DatumTime::now().utc - dur);
        }
    }
    if let Some(date) = parse_date_arg(s, TimeCtx::now())
        .ok()
        .and_then(|d| d.absolute)
    {
        return day_edge(date, edge);
    }
    let arg = parse_time_arg(s, TimeCtx::now())
        .map_err(|e| anyhow!("invalid {} '{s}': {e}", edge.flag()))?;
    Ok(TimeBuilder::now().with_time_arg(arg).build().utc)
}

/// The local-midnight instant that opens or closes `date`.
fn day_edge(date: NaiveDate, edge: Edge) -> Result<DateTime<Utc>> {
    let date = match edge {
        Edge::Since => date,
        Edge::Until => date.succ_opt().context("date overflow")?,
    };
    let naive = date.and_hms_opt(0, 0, 0).unwrap();
    // da-ztae: the one sanctioned local→UTC conversion. `for_boundary` picks
    // the FIRST of an ambiguous pair and clamps a nonexistent instant to the
    // transition, so consecutive day edges always abut — no interval skipped,
    // none double-counted. That is precisely what a range boundary wants, and
    // it replaces the hand-rolled `Local.from_local_datetime(..).single()` the
    // disallowed-methods lint now forbids.
    Ok(Zone::host().resolve(naive).for_boundary())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use chrono::Local;

    /// Assert `actual` is within a second of `expected` — the bounds that
    /// resolve against "now" are re-read from the clock inside the parser.
    fn near(actual: DateTime<Utc>, expected: DateTime<Utc>) {
        let skew = (actual - expected).num_milliseconds().abs();
        assert!(skew < 1000, "expected ~{expected}, got {actual}");
    }

    fn local_midnight(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        day_edge(NaiveDate::from_ymd_opt(y, m, d).unwrap(), Edge::Since).unwrap()
    }

    // ── tier 1: the duration dialect keeps the ambiguous numeric forms ──

    #[test]
    fn unsigned_durations_mean_that_long_ago() {
        for (input, dur) in [
            ("1h", Duration::hours(1)),
            ("30m", Duration::minutes(30)),
            ("1d", Duration::days(1)),
            ("PT5M", Duration::minutes(5)),
            // Colon and bare-number forms: `-t` would read these as the
            // clock times 01:30 and 05:00. The duration reading wins.
            ("1:30", Duration::minutes(90)),
            ("5", Duration::minutes(5)),
        ] {
            near(parse_since(input).unwrap(), Utc::now() - dur);
        }
    }

    // ── tier 2: calendar dates name a whole local day ──────────────────

    #[test]
    fn iso_date_since_opens_the_named_day() {
        assert_eq!(
            parse_since("2026-06-01").unwrap(),
            local_midnight(2026, 6, 1)
        );
    }

    #[test]
    fn iso_date_until_closes_the_named_day() {
        // `--until 2026-06-01` includes all of June 1, so the bound is
        // the midnight that opens June 2.
        assert_eq!(
            parse_until("2026-06-01").unwrap(),
            local_midnight(2026, 6, 2)
        );
    }

    #[test]
    fn compact_date_shorthand_is_accepted() {
        // `apr29` reached the query flags for free by routing through the
        // shared date parser — the point of da-rye2.
        let apr29 = parse_since("apr29-2025").unwrap();
        assert_eq!(apr29, local_midnight(2025, 4, 29));
    }

    // ── tier 3: the shared `-t` grammar ────────────────────────────────

    #[test]
    fn now_resolves_to_the_current_instant() {
        near(parse_since("now").unwrap(), Utc::now());
    }

    #[test]
    fn signed_offsets_read_as_relative_to_now() {
        // `-30` is 30 minutes *ago* and `+2h` 2 hours ahead, matching
        // `-t`. The unsigned tier is skipped for signed input precisely
        // so the sign doesn't get inverted by the "N ago" reading.
        near(
            parse_since("-30").unwrap(),
            Utc::now() - Duration::minutes(30),
        );
        near(parse_until("+2h").unwrap(), Utc::now() + Duration::hours(2));
    }

    #[test]
    fn rfc3339_instant_is_used_verbatim() {
        let s = "2026-06-15T10:45:00Z";
        assert_eq!(parse_since(s).unwrap(), s.parse::<DateTime<Utc>>().unwrap());
    }

    #[test]
    fn clock_time_lands_on_today() {
        let before = Local::now().date_naive();
        let got = parse_since("9:15am").unwrap().with_timezone(&Local);
        // Bracket the call: reading the date only afterwards makes this flake
        // when local midnight falls between them (Copilot, PR #85).
        assert!(
            [before, Local::now().date_naive()].contains(&got.date_naive()),
            "{} matched neither side of the midnight boundary",
            got.date_naive(),
        );
        assert_eq!(
            got.time(),
            chrono::NaiveTime::from_hms_opt(9, 15, 0).unwrap()
        );
    }

    // ── failure ────────────────────────────────────────────────────────

    #[test]
    fn unparseable_bound_names_its_flag() {
        // ⟳ 2026-09-02: this test used 'yesterday', and its own comment predicted
        // the day it would break — "when it lands in the shared vocabulary, it lands
        // here too". da-0gs4 landed it, so the premise expired exactly as written.
        //
        // The test's real subject is unchanged: whichever flag rejects a bound must
        // name itself, because the user typed two of them and needs to know which one
        // objected. So it now uses a string no vocabulary will ever adopt.
        let err = parse_until("zzznotatime").unwrap_err().to_string();
        assert!(err.contains("--until"), "{err}");
        assert!(err.contains("zzznotatime"), "{err}");
    }
}
