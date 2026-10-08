//! Regression test for `datum db v2import` against a committed synthetic
//! fixture (da-zio, rewritten for v=20 schema under da-5ns.6).
//!
//! Locks the **structural correctness** of legacy-data ingestion against
//! the new (occur_time, state, dur, following_state, verb) schema so
//! future refactors cannot silently break it. Doesn't assert rendered
//! output — golden-file diffs from the in-flux renderer would be noise.
//!
//! Fixture: `tests/fixtures/datum_backup_synthetic.json`. Each doc covers
//! a distinct slice of the v=20 import mapping. Verbs below are derived
//! from (state, dur, following_state) via `datum_core::derive_verb` —
//! the `entry.verb` column is hardcoded "point" until da-5ns.3 wires
//! derivation into the insert path.
//!
//! Two verb columns matter:
//! - "Verb stored" — the `verb` column written by `derive_verb_for_storage`
//!   from the **raw** `(state_in, dur_in, fs)` triple at insert time. This
//!   is the authoritative classifier the renderer reads.
//! - "Verb re-derived" — what `derive_verb(state, dur, fs)` returns when
//!   called on the columns **read back** from the row. After da-ju71.2
//!   dropped the null→true promotion, an absent legacy state stays
//!   `state=null`, so re-deriving a "no state, no dur" doc gives Point —
//!   the stored verb and the re-derived verb agree on Point.
//!
//! | Doc # | Field          | dur in   | dur out | state in        | state out      | Verb stored | Verb re-derived |
//! | ----- | -------------- | -------- | ------- | --------------- | -------------- | ----------- | --------------- |
//! | 1     | mood           | absent   | NULL    | "happy"         | "happy"        | Switch      | Switch          |
//! | 2     | sleep          | "PT8H"   | "PT8H"  | absent          | null           | Block       | Block           |
//! | 3     | work           | "PT3M"   | "PT3M"  | "focus"         | "focus"        | Block       | Block           |
//! | 4     | brushteeth     | absent   | NULL    | absent          | null           | Point       | Point           |
//! | 5     | tracking       | absent   | NULL    | true            | true           | Switch      | Switch          |
//! | 6     | test           | "PT1M"   | "PT1M"  | absent          | null           | Block       | Block           |
//! | 7     | consume        | absent   | NULL    | absent          | null           | Point       | Point           |
//! | 8     | malformed_dur  | bad str  | NULL    | absent          | null           | Point       | Point           |
//! | 9     | feeling        | absent   | NULL    | {prim, int}     | {prim, int}    | Switch      | Switch          |
//! | 10    | tags           | absent   | NULL    | [arr]           | [arr]          | Switch      | Switch          |
//! | 11    | thinking       | null     | MOMENT  | "idea"          | "idea"         | Point       | Point           |
//!
//! Counts: 11 imported (7 dur=NULL, 1 dur=MOMENT (doc 11), 3 dur=positive).

use crate::common;

use common::run_cli;
use datum_core::derive_verb;
use datum_core::entry::Verb;
use datum_core::test_util::fresh_db;
use std::collections::HashSet;

fn verb_of(e: &datum_core::entry::Entry) -> Verb {
    derive_verb(e.state.clone(), e.dur.as_ref(), e.following_state.clone())
        .expect("derive_verb on imported entry")
        .0
}

fn is_block_dur(d: &Option<datum_core::dur::Dur>) -> bool {
    match d.as_ref() {
        None => false,
        Some(d) if d.is_zero_span() || d.is_moment() || d.is_unknown() => false,
        Some(_) => true,
    }
}

const FIXTURE: &str = "tests/fixtures/datum_backup_synthetic.json";

#[test]
fn db_import_synthetic_locks_db_shape() {
    let db = fresh_db();
    run_cli(&["db", "v2import", FIXTURE], &db).expect("synthetic import");

    let entries = datum_core::test_util::logged_all(&db);
    assert_eq!(entries.len(), 11, "fixture defines 11 importable docs");

    // Per the table above: 8 entries have dur NULL or MOMENT, 3 have positive dur.
    let zero_or_null = entries.iter().filter(|e| !is_block_dur(&e.dur)).count();
    let positive = entries.iter().filter(|e| is_block_dur(&e.dur)).count();
    assert_eq!(
        zero_or_null, 8,
        "expected 8 entries with dur=NULL or dur=MOMENT (docs 1, 4, 5, 7, 8, 9, 10, 11)",
    );
    assert_eq!(
        positive, 3,
        "expected 3 entries with positive dur (docs 2, 3, 6)"
    );

    // Every entry with positive dur stores it in the `dur` column alone —
    // its literal no longer rides along in `data` (da-w94vu, vividn 10-08).
    for e in entries.iter().filter(|e| is_block_dur(&e.dur)) {
        assert!(
            e.occur_time.is_some(),
            "block {} missing occur_time",
            e.slug
        );
        assert!(
            e.data.as_ref().and_then(|d| d.get("dur")).is_none(),
            "block {} kept its dur in data",
            e.slug,
        );
    }

    // Provenance + sentinel intent fields on every imported row.
    // `_import` lives in `extra_metadata` (da-tsw), strictly disjoint from
    // user-payload `data`.
    for e in &entries {
        let metadata = e
            .extra_metadata
            .as_ref()
            .unwrap_or_else(|| panic!("row {} missing extra_metadata", e.slug));
        assert_eq!(
            metadata["_import"]["source"],
            serde_json::json!("datum-v2-couchdb"),
            "row {} missing _import.source marker",
            e.slug,
        );
        assert!(
            e.data.as_ref().and_then(|d| d.get("_import")).is_none(),
            "row {} leaked `_import` into user-payload `data`",
            e.slug,
        );
        assert_eq!(
            e.parser_version, 0,
            "row {} parser_version must be 0 (sentinel for imported)",
            e.slug,
        );
    }

    // hid is non-empty and unique across imported rows (PK invariant).
    let mut seen = HashSet::new();
    for e in &entries {
        assert!(!e.hid.is_empty(), "row {} has empty hid", e.slug);
        assert!(seen.insert(e.hid.clone()), "duplicate hid {}", e.hid);
    }

    // ─── Per-doc structural asserts (v=20 columns) ──────────────────────

    // Doc 1 (mood, string state, no dur): state set + no dur → Switch.
    let mood = entries
        .iter()
        .find(|e| e.field == "mood")
        .expect("doc 1: mood");
    assert!(mood.dur.is_none());
    assert_eq!(mood.state, Some(serde_json::json!("happy")));
    assert_eq!(mood.verb, Verb::Switch, "doc 1: stored verb");
    assert_eq!(verb_of(mood), Verb::Switch);

    // Doc 2 (sleep / PT8H): no state → state stays null (promotion dropped),
    // dur="PT8H" → Block.
    let sleep = entries
        .iter()
        .find(|e| e.field == "sleep")
        .expect("doc 2: sleep");
    assert_eq!(
        sleep
            .dur
            .as_ref()
            .map(datum_core::dur::Dur::to_storage_string)
            .as_deref(),
        Some("PT8H"),
        "sleep dur verbatim"
    );
    assert!(
        sleep.state.is_none(),
        "null state stays null (no promotion)"
    );
    assert_eq!(sleep.verb, Verb::Block, "doc 2: stored verb");
    assert_eq!(verb_of(sleep), Verb::Block);
    assert!(sleep.data.is_none(), "sleep dur is a column, not data");

    // Doc 3 (work, string state + dur): Block with state preserved.
    let work = entries
        .iter()
        .find(|e| e.field == "work")
        .expect("doc 3: work");
    assert_eq!(
        work.dur
            .as_ref()
            .map(datum_core::dur::Dur::to_storage_string)
            .as_deref(),
        Some("PT3M")
    );
    assert_eq!(work.state, Some(serde_json::json!("focus")));
    assert_eq!(work.verb, Verb::Block, "doc 3: stored verb");
    assert_eq!(verb_of(work), Verb::Block);

    // Doc 4 (brushteeth, no state, no dur): da-7sr — must store verb='point'.
    // After da-ju71.2 the state column stays null (no promotion), so the
    // stored verb and the re-derived verb both agree on Point.
    let brushteeth = entries
        .iter()
        .find(|e| e.field == "brushteeth")
        .expect("doc 4: brushteeth");
    assert!(brushteeth.dur.is_none());
    assert!(
        brushteeth.state.is_none(),
        "null state stays null (no promotion)"
    );
    assert_eq!(
        brushteeth.verb,
        Verb::Point,
        "doc 4: da-7sr — no-state-no-dur legacy doc must store verb='point', not 'switch'",
    );
    assert_eq!(verb_of(brushteeth), Verb::Point);

    // Doc 5 (tracking, boolean state): Switch with state=true.
    let tracking = entries
        .iter()
        .find(|e| e.field == "tracking")
        .expect("doc 5: tracking");
    assert!(tracking.dur.is_none());
    assert_eq!(tracking.state, Some(serde_json::json!(true)));
    assert_eq!(tracking.verb, Verb::Switch, "doc 5: stored verb");
    assert_eq!(verb_of(tracking), Verb::Switch);

    // Doc 6 (test / PT1M, o:0 UTC): tz=UTC, offset=0 round-trip.
    let test = entries
        .iter()
        .find(|e| e.field == "test")
        .expect("doc 6: test");
    assert_eq!(
        test.dur
            .as_ref()
            .map(datum_core::dur::Dur::to_storage_string)
            .as_deref(),
        Some("PT1M")
    );
    assert_eq!(test.verb, Verb::Block, "doc 6: stored verb");
    assert_eq!(test.occur_time.as_ref().unwrap().offset_minutes, 0);
    assert_eq!(
        test.occur_time.as_ref().unwrap().timezone.as_deref(),
        Some("UTC")
    );

    // Doc 7 (consume): extras `id`, `medium` go into `data` and don't
    // collide with Entry.hid. da-7sr: also must store verb='point'.
    let consume = entries
        .iter()
        .find(|e| e.field == "consume")
        .expect("doc 7: consume");
    assert!(consume.dur.is_none());
    assert!(
        consume.state.is_none(),
        "null state stays null (no promotion)"
    );
    assert_eq!(
        consume.verb,
        Verb::Point,
        "doc 7: da-7sr — no-state-no-dur (with extras) must store verb='point'",
    );
    assert_eq!(verb_of(consume), Verb::Point);
    let consume_extra = consume.data.as_ref().unwrap();
    assert_eq!(consume_extra["id"], serde_json::json!("song-42"));
    assert_eq!(consume_extra["medium"], serde_json::json!("audio"));
    assert_ne!(
        consume.hid, "song-42",
        "datum doc's `data.id` must NOT overwrite Entry.hid",
    );

    // Doc 8 (malformed dur): warn-not-fail, dur=NULL, literal preserved
    // in `data`. State absent → state column stays null (promotion
    // dropped in da-ju71.2). Stored and re-derived verb both Point.
    let malformed = entries
        .iter()
        .find(|e| e.field == "malformed_dur")
        .expect("doc 8: malformed_dur");
    assert!(malformed.dur.is_none(), "unparseable dur → dur column NULL");
    assert!(
        malformed.state.is_none(),
        "null state stays null (no promotion)"
    );
    assert_eq!(
        malformed.verb,
        Verb::Point,
        "doc 8: da-7sr — malformed dur degrades to no-dur and must store verb='point'",
    );
    assert_eq!(verb_of(malformed), Verb::Point);
    assert_eq!(
        malformed.data.as_ref().unwrap()["dur"],
        serde_json::json!("not-a-duration"),
        "malformed dur literal must survive verbatim in data",
    );

    // Doc 9 (feeling, object state): object state → Switch verbatim.
    let feeling = entries
        .iter()
        .find(|e| e.field == "feeling")
        .expect("doc 9: feeling");
    assert!(feeling.dur.is_none());
    assert_eq!(feeling.verb, Verb::Switch, "doc 9: stored verb");
    assert_eq!(verb_of(feeling), Verb::Switch);
    assert_eq!(
        feeling.state,
        Some(serde_json::json!({"primary": "calm", "intensity": 5})),
        "object state must round-trip verbatim",
    );

    // Doc 10 (tags, array state): array state → Switch verbatim.
    let tags = entries
        .iter()
        .find(|e| e.field == "tags")
        .expect("doc 10: tags");
    assert!(tags.dur.is_none());
    assert_eq!(tags.verb, Verb::Switch, "doc 10: stored verb");
    assert_eq!(verb_of(tags), Verb::Switch);
    assert_eq!(tags.state, Some(serde_json::json!(["focus", "deep-work"])));

    // Doc 11 (thinking, state + dur=null): explicit null → dur=MOMENT
    // (da-w94vu; was PT0S). state="idea" + a moment → Verb::Point.
    let thinking = entries
        .iter()
        .find(|e| e.field == "thinking")
        .expect("doc 11: thinking");
    assert_eq!(
        thinking
            .dur
            .as_ref()
            .map(datum_core::dur::Dur::to_storage_string)
            .as_deref(),
        Some("MOMENT"),
        "null dur → MOMENT"
    );
    assert_eq!(thinking.state, Some(serde_json::json!("idea")));
    assert_eq!(thinking.verb, Verb::Point, "doc 11: stored verb");
    assert_eq!(verb_of(thinking), Verb::Point);
    assert!(thinking.data.is_none(), "dur=null left in data");
}

#[test]
fn db_import_synthetic_is_idempotent() {
    let db = fresh_db();
    run_cli(&["db", "v2import", FIXTURE], &db).expect("first import");
    let n1 = datum_core::test_util::logged_all(&db).len();
    assert_eq!(n1, 11);

    // Second run on the same DB: content_hash dedup must skip every doc.
    run_cli(&["db", "v2import", FIXTURE], &db).expect("second import");
    let n2 = datum_core::test_util::logged_all(&db).len();
    assert_eq!(n2, n1, "re-importing same fixture must not add rows");
}

// ─── Binary harness: lock the user-visible summary line ──────────────────

#[test]
fn db_import_synthetic_summary_reports_correct_counts() {
    let mut db_path = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    db_path.push(format!("datum-db-import-regression-{pid}-{nonce}.db"));
    let _ = std::fs::remove_file(&db_path);

    let bin = env!("CARGO_BIN_EXE_datum");
    let out = std::process::Command::new(bin)
        .env("DATUM_DB", &db_path)
        .env("DATUM_COLOR", "0")
        .args(["db", "v2import", FIXTURE])
        .output()
        .expect("run datum binary");
    let _ = std::fs::remove_file(&db_path);

    assert!(
        out.status.success(),
        "import failed: stderr={}",
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("11 imported"),
        "expected '11 imported' in stdout, got: {stdout}",
    );
    assert!(
        stdout.contains("8 point"),
        "expected '8 point' in stdout, got: {stdout}",
    );
    assert!(
        stdout.contains("3 block"),
        "expected '3 block' in stdout, got: {stdout}",
    );
}

// ─── `db v2import -`: the backup on stdin (da-podv) ──────────────────────

/// Run the binary against a fresh db, `stdin` piped in.
fn datum_with_stdin(args: &[&str], stdin: &[u8]) -> std::process::Output {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", dir.path().join("datum.db"))
        .env("DATUM_COLOR", "0")
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run datum binary");
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn db_v2import_reads_the_backup_from_stdin() {
    let backup = std::fs::read("tests/fixtures/datum_backup_3days.json").unwrap();
    for args in [
        &["db", "v2import", "-"][..],
        &["db", "v2import", "--from", "-"],
    ] {
        let out = datum_with_stdin(args, &backup);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && stdout.contains("120 imported"),
            "{args:?}: stdout={stdout} stderr={}",
            String::from_utf8_lossy(&out.stderr),
        );
    }
}

#[test]
fn db_v2import_without_a_file_and_db_import_are_errors() {
    let out = datum_with_stdin(&["db", "v2import"], b"");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("db v2import requires a backup file"));

    let out = datum_with_stdin(&["db", "import", FIXTURE], b"");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        stderr.contains("unknown db subcommand: 'import'") && stderr.contains("v2import"),
        "stderr={stderr}"
    );
}
