//! The browser's rows must be the CLI's rows.
//!
//! `datum-parse-wasm` is what datum.day runs instead of a hand-written
//! second parser (da-7scm). Every grammatical rule it applies comes from
//! `datum-parse` and is therefore shared outright, but the assembly between
//! those calls — which slots a verb declares, which remainder bucket
//! applies, `occur`'s `MOMENT` stamp, `start`/`stop`/`switch`'s state
//! canonicalization — lives in `log_core`, which cannot compile to wasm
//! because it reaches the database. That assembly is mirrored, and mirrors
//! drift.
//!
//! So this test holds the two side by side: each argv runs through the real
//! `dispatch_argv` against a real database, and through the wasm crate,
//! and the stored row is compared with what the browser would have stored.
//! A change to `log_core` that the browser does not follow fails here,
//! in `cargo test --workspace`, rather than in someone's logged data.
//!
//! What is compared is the entry's *semantics* — field, verb, state,
//! duration, data, and when it happened — plus, where the argv pins a
//! wall clock, the derived identity (slug and content hash) as well. The
//! per-write facts (hid, device id, the microsecond the row was created)
//! could not match across two implementations and should not have to; see
//! `web/tests/support/semantics.mjs`, which draws the same line.

use chrono::{DateTime, Utc};
use datum_commands::cmd::dispatch_argv;
use datum_core::test_util::fresh_db;
use datum_parse::time::clock;
use datum_parse_wasm::log::{self, Host};
use serde_json::Value as JsonValue;

/// What a case compares about the entry's time.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TimeMode {
    /// The argv pinned a wall clock (`-t 0915`): both sides land on the
    /// same instant, so the whole identity — occur_time, slug, hash —
    /// compares exactly.
    Clock,
    /// The argv asked for "N minutes ago" (`-q`, bare): the two sides read
    /// their clocks microseconds apart, so the comparison is the whole-
    /// second distance from the write.
    Relative,
    /// The argv asked for a whole day back (`-y`): compare the calendar
    /// distance and that the instant is midnight.
    Day,
}

struct Case {
    argv: &'static [&'static str],
    time: TimeMode,
}

const CASES: &[Case] = &[
    Case {
        argv: &["occur", "coffee", "-t", "0915"],
        time: TimeMode::Clock,
    },
    Case {
        argv: &["start", "work", "-t", "0800"],
        time: TimeMode::Clock,
    },
    Case {
        argv: &["stop", "work", "-t", "1200"],
        time: TimeMode::Clock,
    },
    Case {
        argv: &["switch", "mood", "happy", "-t", "0930"],
        time: TimeMode::Clock,
    },
    Case {
        argv: &["mood", "switch", "sad", "-t", "0945"],
        time: TimeMode::Clock,
    },
    Case {
        argv: &["occur", "nap", "dur=30m", "-t", "1000"],
        time: TimeMode::Clock,
    },
    Case {
        argv: &["occur", "walk", "dur=1:30", "-t", "1100"],
        time: TimeMode::Clock,
    },
    Case {
        argv: &["occur", "nap", "dur=PT30M", "-t", "1400"],
        time: TimeMode::Clock,
    },
    Case {
        argv: &["occur", "coffee", "strength=strong", "-t", "0916"],
        time: TimeMode::Clock,
    },
    Case {
        argv: &["occur", "weight", "amount=75.2kg", "-t", "0917"],
        time: TimeMode::Clock,
    },
    Case {
        argv: &["switch", "project", "household", "-t", "0918"],
        time: TimeMode::Clock,
    },
    Case {
        argv: &["stop", "work", "30m", "-t", "1300"],
        time: TimeMode::Clock,
    },
    Case {
        argv: &["occur", "coffee", "-t", "-30"],
        time: TimeMode::Relative,
    },
    Case {
        argv: &["occur", "coffee"],
        time: TimeMode::Relative,
    },
    Case {
        argv: &["occur", "coffee", "-q"],
        time: TimeMode::Relative,
    },
    Case {
        argv: &["occur", "coffee", "-qq"],
        time: TimeMode::Relative,
    },
    Case {
        argv: &["switch", "mood", "low", "-qqqq"],
        time: TimeMode::Relative,
    },
    Case {
        argv: &["occur", "coffee", "-y"],
        time: TimeMode::Day,
    },
    Case {
        argv: &["occur", "coffee", "-yy"],
        time: TimeMode::Day,
    },
    Case {
        argv: &["start", "work", "-y"],
        time: TimeMode::Day,
    },
];

/// The row the CLI stored, read straight off the columns.
struct Row {
    field: String,
    verb: String,
    state: Option<String>,
    dur: Option<String>,
    data: Option<String>,
    occur_time: String,
    occur_uncertainty: Option<String>,
    slug: String,
    content_hash: String,
    create_time: String,
}

fn cli_row(argv: &[&str]) -> Row {
    let db = fresh_db();
    let mut full: Vec<String> = vec!["datum".to_string()];
    full.extend(argv.iter().map(|s| s.to_string()));
    dispatch_argv(full, &db).unwrap_or_else(|e| panic!("`datum {}` failed: {e}", argv.join(" ")));

    let mut stmt = db
        .conn()
        .prepare(
            "SELECT field, verb, state, dur, data, occur_time, occur_time_uncertainty, \
                    slug, content_hash, create_time \
             FROM entries WHERE deleted_at IS NULL",
        )
        .unwrap();
    let mut rows = stmt
        .query_map([], |r| {
            Ok(Row {
                field: r.get(0)?,
                verb: r.get(1)?,
                state: r.get(2)?,
                dur: r.get(3)?,
                data: r.get(4)?,
                occur_time: r.get(5)?,
                occur_uncertainty: r.get(6)?,
                slug: r.get(7)?,
                content_hash: r.get(8)?,
                create_time: r.get(9)?,
            })
        })
        .unwrap()
        .map(|r| r.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1, "expected exactly one row for {argv:?}");
    rows.pop().unwrap()
}

/// The same argv through the browser's module, anchored on `now`.
///
/// The zone comes from the same host lookup the CLI's `TimeBuilder` uses,
/// so the two agree about what "today" and "09:15" mean whatever `TZ` the
/// test process happens to carry.
fn browser_entry(argv: &[&str], now: DateTime<Utc>) -> JsonValue {
    let owned: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    let tz = clock::host_iana_tz_name();
    let host = Host {
        now,
        timezone: tz.as_deref(),
        offset_minutes: clock::host_offset().local_minus_utc() / 60,
        device_id: "web-parity",
        hid_bytes: &[0; 12],
    };
    match log::build(&owned, &host) {
        Ok(log::Parsed::Entry(e)) => log::entry_to_json(&e),
        Ok(log::Parsed::Unsupported(reason)) => {
            panic!(
                "`datum {}` is unsupported in the browser: {reason}",
                argv.join(" ")
            )
        }
        Err(e) => panic!("`datum {}` failed to parse: {e}", argv.join(" ")),
    }
}

fn parse_utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .unwrap_or_else(|e| panic!("not rfc3339: {s:?} ({e})"))
        .with_timezone(&Utc)
}

/// A stored JSON column against the browser's typed value. The CLI writes
/// `state` / `data` as JSON text; comparing parsed values rather than the
/// text keeps the assertion about content instead of key order.
fn same_json(column: Option<&str>, browser: Option<&JsonValue>, what: &str, argv: &[&str]) {
    let stored: Option<JsonValue> = column.map(|s| {
        serde_json::from_str(s).unwrap_or_else(|e| panic!("{what} column is not JSON: {s:?} ({e})"))
    });
    assert_eq!(
        stored.as_ref(),
        browser,
        "{what} differs for `datum {}`",
        argv.join(" ")
    );
}

#[test]
fn the_browser_stores_what_the_cli_stores() {
    for case in CASES {
        let argv = case.argv;
        let label = argv.join(" ");
        let row = cli_row(argv);
        // Anchoring on the CLI row's own create_time is what makes a
        // clock-pinned case compare exactly: both sides then resolve
        // "today" and "now" from the same instant.
        let create_time = parse_utc(&row.create_time);
        let web = browser_entry(argv, create_time);

        assert_eq!(
            web["field"],
            row.field.as_str(),
            "field differs for `datum {label}`"
        );
        assert_eq!(
            web["verb"],
            row.verb.as_str(),
            "verb differs for `datum {label}`"
        );
        same_json(row.state.as_deref(), web.get("state"), "state", argv);
        same_json(row.data.as_deref(), web.get("data"), "data", argv);
        assert_eq!(
            row.dur.as_deref(),
            web.get("dur").and_then(JsonValue::as_str),
            "dur differs for `datum {label}`"
        );
        assert_eq!(
            row.occur_uncertainty.as_deref(),
            web.get("occurTimeUncertainty").and_then(JsonValue::as_str),
            "occur_time_uncertainty differs for `datum {label}`"
        );

        let cli_occur = parse_utc(&row.occur_time);
        let web_occur = parse_utc(web["occurTime"]["utc"].as_str().unwrap());
        match case.time {
            TimeMode::Clock => {
                assert_eq!(
                    web_occur, cli_occur,
                    "occur_time differs for `datum {label}`"
                );
                // With the instant identical, the derived identity must be
                // too — the browser's row is byte-for-byte the CLI's row
                // everywhere the two are supposed to agree.
                assert_eq!(
                    web["slug"],
                    row.slug.as_str(),
                    "slug differs for `datum {label}`"
                );
                assert_eq!(
                    web["contentHash"],
                    row.content_hash.as_str(),
                    "content_hash differs for `datum {label}`"
                );
            }
            TimeMode::Relative => {
                let cli_back = (create_time - cli_occur).num_seconds();
                let web_back = (create_time - web_occur).num_seconds();
                assert_eq!(
                    cli_back, web_back,
                    "seconds-before-the-write differ for `datum {label}`"
                );
            }
            TimeMode::Day => {
                assert_eq!(
                    cli_occur, web_occur,
                    "a whole-day entry is midnight-anchored, so it must land \
                     on the same instant for `datum {label}`"
                );
                assert_eq!(
                    web["slug"],
                    row.slug.as_str(),
                    "slug differs for `datum {label}`"
                );
            }
        }
    }
}

/// The seam's contract for everything else: a well-formed command this
/// module does not build an entry for must say so, not guess.
#[test]
fn commands_outside_the_browser_subset_report_themselves() {
    for argv in [
        &["tail"][..],
        &["note", "word", "correr"][..],
        &["add", "word", "-K", "id=correr"][..],
        &["occur", "coffee", "--dry-run"][..],
        &["dayview"][..],
    ] {
        let owned: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
        let host = Host {
            now: Utc::now(),
            timezone: Some("UTC"),
            offset_minutes: 0,
            device_id: "web-parity",
            hid_bytes: &[0; 12],
        };
        assert!(
            matches!(log::build(&owned, &host), Ok(log::Parsed::Unsupported(_))),
            "expected `datum {}` to report itself unsupported",
            argv.join(" ")
        );
    }
}
