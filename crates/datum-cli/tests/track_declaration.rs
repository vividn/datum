//! da-gvcen: the first log of a field is ONE entry, and it declares its
//! own field.
//!
//! da-c8mh wrote a second row beside it — a TRACK document a millisecond
//! earlier, carrying the TRACKED sentinel. vividn, 2026-09-21: "a separate
//! entry gets logged at a separate time point (slightly earlier iirc). I
//! don't like this. It's very clunky. […] we should use following_state,
//! so a POINT_SWITCH." So the declaration now rides in the entry's own
//! trailing edge, and the SPEC is what records that the field exists.
//!
//! Driven through the real binary: what the *process* writes and echoes
//! is the contract. `show_json.rs` covers the shape of a doc and
//! `e2e_subprocess.rs` the table; what lives here is the matrix the other
//! two leave out — one case per verb, the spec that is written either
//! way, the `_LAST` group `undo` takes back, the IGNORED field a first
//! log brings back, and the import, which declares with specs alone.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    home: PathBuf,
    config_home: PathBuf,
    db: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("create tmpdir");
        let home = tmp.path().join("home");
        let config_home = tmp.path().join("config");
        let db = tmp.path().join("track_declaration.db");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&config_home).unwrap();
        Self {
            _tmp: tmp,
            home,
            config_home,
            db,
        }
    }

    fn run(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.config_home)
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("spawn datum");
        assert!(
            out.status.success(),
            "{args:?} exited {:?}\n  stdout: {}\n  stderr: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8(out.stdout).expect("utf-8 stdout")
    }

    /// `args` through the binary without demanding success: the exit
    /// code and stdout, for the paths whose contract IS a non-zero exit.
    fn try_run(&self, args: &[&str]) -> (Option<i32>, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.config_home)
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("spawn datum");
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    }

    /// The `action` of every doc `args` echoed, in printed order.
    fn actions(&self, args: &[&str]) -> Vec<String> {
        let argv: Vec<&str> = args.iter().copied().chain(["--show", "json"]).collect();
        let stdout = self.run(&argv);
        let doc: Value = serde_json::from_str(stdout.trim_end())
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout:?}"));
        doc["docs"]
            .as_array()
            .expect("docs array")
            .iter()
            .map(|d| d["action"].as_str().expect("action string").to_string())
            .collect()
    }
}

/// One case per verb, through the real process: the first log of a field
/// echoes ONE document, and it is the log itself.
#[test]
fn the_first_log_of_a_field_is_one_document() {
    for argv in [
        vec!["occur", "coffee"],
        vec!["start", "work"],
        vec!["end", "chore"],
        vec!["switch", "mood", "happy"],
        vec!["note", "idea", "words"],
    ] {
        // A fresh db per case: what is under test is the FIRST log.
        let env = Env::new();
        assert_eq!(
            env.actions(&argv),
            vec!["CREATE".to_string()],
            "{argv:?} should echo the entry and nothing beside it",
        );
    }
}

/// The trailing edge is where the declaration goes — so a shape that HAS
/// one carries a `following_state` and becomes a compound verb, and a
/// shape that does not is declared by its spec alone.
///
/// What the trailing state SAYS is `track::declared_state` (da-5ifp),
/// unchanged: `false` for a field that has an off state to sit in, the
/// TRACKED sentinel for one that does not. The declaration moved column,
/// not content — so `stats` and the state cell read a first-log block
/// exactly as they read an ordinary `end`.
///
/// `end chore 30m` is in here on purpose twice over: vividn settled at
/// 19:13 (da-c8mh) that an end brings its field into being too, and a
/// closed block is the other first-log shape with a trailing instant —
/// so it declares the same way the dot does. Without it, `end` on a
/// field that had been IGNORED would leave it ignored, because a plain
/// `Block` HANDS THE FIELD BACK to the state before it (da-611d) and
/// that state is IGNORED.
#[test]
fn a_first_log_declares_in_its_own_trailing_edge_where_it_has_one() {
    for (argv, verb, following) in [
        (vec!["occur", "coffee"], "point-switch", Some("\"TRACKED\"")),
        (
            vec!["occur", "nap", "dur=30m"],
            "block-switch",
            Some("\"TRACKED\""),
        ),
        (vec!["start", "work", "30m"], "block-switch", Some("false")),
        (vec!["end", "chore", "30m"], "block-switch", Some("false")),
        // No trailing instant: an open start and a switch say what the
        // field is with their own state, and a timeless note has no
        // instant at all.
        (vec!["start", "work"], "switch", None),
        (vec!["switch", "mood", "happy"], "switch", None),
        (vec!["note", "idea", "words"], "point", None),
    ] {
        let env = Env::new();
        env.run(&[argv.clone(), vec!["--show", "none"]].concat());
        let field = argv[1];
        let conn = rusqlite::Connection::open(&env.db).expect("open db");
        let (stored_verb, stored_following): (String, Option<String>) = conn
            .query_row(
                "SELECT verb, following_state FROM entries WHERE field = ?1",
                [field],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or_else(|e| panic!("one row for {argv:?}: {e}"));
        assert_eq!(stored_verb, verb, "{argv:?} verb");
        assert_eq!(
            stored_following.as_deref(),
            following,
            "{argv:?} following_state",
        );
    }
}

/// Whichever shape the log took, the SPEC is written — that is what
/// records the field's existence now, and what the next log of the field
/// reads its verb from instead of guessing from the log.
#[test]
fn the_spec_is_the_record_that_the_field_exists() {
    for (argv, verb) in [
        (vec!["occur", "coffee"], "occur"),
        (vec!["start", "work"], "start"),
        (vec!["end", "chore"], "start"),
        (vec!["switch", "mood", "happy"], "switch"),
        (vec!["note", "idea", "words"], "add"),
    ] {
        let env = Env::new();
        env.run(&[argv.clone(), vec!["--show", "none"]].concat());
        let field = argv[1];
        let conn = rusqlite::Connection::open(&env.db).expect("open db");
        let spec: String = conn
            .query_row(
                "SELECT data FROM entries WHERE slug = 'spec:' || ?1 AND deleted_at IS NULL",
                [field],
                |r| r.get(0),
            )
            .unwrap_or_else(|e| panic!("no spec for {argv:?}: {e}"));
        let spec: Value = serde_json::from_str(&spec).unwrap();
        assert_eq!(spec["verb"], verb, "{argv:?} spec verb");

        // And nothing in the log names itself a declaration any more.
        let declarations: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entries
                 WHERE json_extract(COALESCE(extra_metadata, '{}'), '$.track') IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(declarations, 0, "{argv:?} wrote a TRACK row");
    }
}

/// The trailing switch is not decoration — it is the whole of how a field
/// that was IGNORED comes back. `datum ignore` writes no spec, so the
/// next log of an ignored field is still a FIRST log, and its
/// `following_state` is the switch that supersedes the IGNORED one.
#[test]
fn a_first_log_brings_an_ignored_field_back() {
    for argv in [
        vec!["occur", "zap"],
        vec!["end", "zap", "30m"],
        vec!["start", "zap"],
        vec!["switch", "zap", "happy"],
    ] {
        let env = Env::new();
        env.run(&["ignore", "zap"]);
        env.run(&[argv.clone(), vec!["--show", "none"]].concat());
        let listing = env.run(&["fields", "--show", "json"]);
        let listing: Value = serde_json::from_str(listing.trim_end()).expect("fields json");
        let ignored = listing
            .as_array()
            .expect("array")
            .iter()
            .find(|f| f["field"] == "zap")
            .expect("zap is listed")["ignored"]
            .clone();
        assert_eq!(ignored, Value::Bool(false), "{argv:?} left zap ignored");
    }
}

/// ⚠ TRACKED is a status, not a state a field is ON in
/// (`is_inactive_state`, da-5ifp). A `following_state` of TRACKED must
/// not make the field read as active — `now --json`'s `active` rows are
/// what the web's `FieldButton.active` is drawn from.
#[test]
fn a_field_declared_by_its_first_log_is_not_active() {
    for argv in [
        vec!["occur", "coffee"],
        vec!["end", "chore", "30m"],
        vec!["start", "work", "30m"],
    ] {
        let env = Env::new();
        env.run(&[argv.clone(), vec!["--show", "none"]].concat());
        let now = env.run(&["now", "--json"]);
        let now: Value = serde_json::from_str(now.trim_end()).expect("now json");
        assert_eq!(
            now["active"].as_array().map(Vec::len),
            Some(0),
            "{argv:?} left the field active: {now}",
        );
    }
}

/// `_LAST` is the one row, and `undo` takes it back — with the spec it
/// wrote, which a field left with no entry does not keep while it is still
/// bare (da-elv6d). One `undo`, the whole first log.
#[test]
fn the_first_log_undoes_in_one() {
    let env = Env::new();
    env.run(&["occur", "coffee", "--show", "none"]);

    let last = env.run(&["get", "_LAST"]);
    let headers: Vec<&str> = last.lines().filter(|l| l.starts_with("EXISTS ")).collect();
    assert_eq!(headers.len(), 1, "`_LAST` is the entry: {last:?}");

    let undone = env.run(&["undo"]);
    assert_eq!(
        undone.lines().filter(|l| l.starts_with("UNDONE ")).count(),
        2,
        "one `undo`, the entry and its spec: {undone:?}"
    );
    assert!(
        !env.run(&["fields"]).contains("coffee"),
        "the field is gone"
    );
    assert!(
        env.run(&["tail"]).contains("[No data]"),
        "nothing should be left standing",
    );
}

/// `datum track` still writes its TRACK row, and that is not legacy: the
/// explicit command has no entry of its own to carry a trailing state,
/// and the row is what un-ignores a field (ignored-ness is read off the
/// log, da-d08c).
///
/// Where the row NAMES itself is `get` — a single document, the whole of
/// the output. da-ftg8p took the action column out of the TABLE, so the
/// same row in `tail` is its state and no action word; both surfaces are
/// asserted here so neither half can drift alone.
///
/// The action word is still SIX display columns, like `CREATE` — the
/// column the CLI aligns by display width has one cell per row, and a
/// glyph that measured two would push every later cell out of true.
#[test]
fn the_explicit_track_command_still_writes_its_document() {
    for (verb, glyph) in [
        ("occur", "TRACK\u{25cf}"),
        ("start", "TRACK\u{2588}"),
        ("switch", "TRACK\u{259e}"),
        ("add", "TRACK\u{a2}"),
    ] {
        let env = Env::new();
        // A spec first, so the `track` is the row alone and `_LAST` is it.
        env.run(&["setup", "coffee", "--verb", verb]);
        env.run(&["track", "coffee"]);

        // The single-document surface names it.
        let got = env.run(&["get", "_LAST"]);
        let header = got.lines().next().unwrap_or_default();
        assert!(header.starts_with(glyph), "{verb}: {header:?}");
        assert_eq!(
            datum_cli::output::visible_width(glyph),
            6,
            "{glyph:?} is not six columns"
        );

        // And the row is really in the log — in the table as its state,
        // with no action word beside it (da-ftg8p).
        let tail = env.run(&["tail"]);
        assert!(
            tail.lines().any(|l| l.contains("coffee")),
            "{verb}: the declaration is not in the log: {tail:?}"
        );
        // The GLYPH, not the substring: an occur field's declaration sits
        // in the TRACKED state (da-5ifp), so `tail` says TRACKED in its
        // state cell — what must be absent is the action WORD and the
        // column it would conjure.
        assert!(
            !tail.contains(glyph),
            "{verb}: a table names no action: {tail:?}"
        );
        assert!(
            !tail.lines().next().unwrap_or_default().contains("action"),
            "{verb}: no action column: {tail:?}"
        );
    }
}

/// The declaration is hashed into the entry's content (da-4vyw), so a
/// field's DECLARING log and a later identical one are not the same
/// bytes — and without a rule for it, re-running the exact command that
/// first declared a field would reach the duplicate-IDENTITY refusal
/// da-2fkr wrote for two different events at one instant, instead of the
/// benign no-op da-9sfm reserves exit 11 for.
///
/// `is_relog_of_a_declaring_row` is that rule, and this is it end to end:
/// the same command twice is EXISTS and one row, exactly as it is for a
/// field that was already declared.
#[test]
fn re_running_the_declaring_log_is_the_benign_no_op() {
    let env = Env::new();
    env.run(&["occur", "coffee", "-t", "0900", "--show", "none"]);
    // A pinned time: a bare log lands on a fresh millisecond and never
    // collides, so only an explicit `-t` can reproduce the shape.
    let (code, stdout) = env.try_run(&["occur", "coffee", "-t", "0900"]);
    assert_eq!(code, Some(11), "the duplicate exit code: {stdout:?}");
    assert!(
        stdout.contains("EXISTS"),
        "the re-log renders the existing entry: {stdout:?}"
    );
    let tail: Value =
        serde_json::from_str(env.run(&["tail", "--json"]).trim_end()).expect("tail json");
    assert_eq!(
        tail["rows"].as_array().map(Vec::len),
        Some(1),
        "no second row: {tail}",
    );
}

/// ⚠ FIRST-WRITTEN, not first-HAPPENED. `is_relog_of_a_declaring_row`
/// asks whether the stored row is the field's first LOG, and backfill
/// separates the two orders: a row logged later can sit earlier on the
/// clock. Keyed on `occur_time` the re-log below stopped being
/// recognised the moment anything was backdated in front of it
/// (canary-kimi-k3 on #68). `create_time` is the order things were
/// logged in, which is what the rule is about.
#[test]
fn a_backfill_in_front_of_the_declaring_log_does_not_unmake_it() {
    let env = Env::new();
    env.run(&["occur", "foo", "-t", "0900", "--show", "none"]);
    // Logged second, but it sits EARLIER on the clock than the row that
    // declared the field.
    env.run(&["occur", "foo", "-t", "0800", "--show", "none"]);

    let (code, stdout) = env.try_run(&["occur", "foo", "-t", "0900"]);
    assert_eq!(code, Some(11), "the duplicate exit code: {stdout:?}");
    assert!(
        stdout.contains("EXISTS"),
        "the backfill must not unmake the declaring log: {stdout:?}"
    );
    let tail: Value =
        serde_json::from_str(env.run(&["tail", "--json"]).trim_end()).expect("tail json");
    assert_eq!(
        tail["rows"].as_array().map(Vec::len),
        Some(2),
        "no third row: {tail}",
    );

    // The complement, so the rule is pinned from both sides: re-running
    // the BACKFILL is a no-op too, but it reaches it as an ordinary
    // content-hash duplicate rather than through the declaring-row rule —
    // the 0800 row carries no declaration. If the rule ever started
    // recognising non-declaring rows this would still pass, so it is the
    // row count that carries the claim.
    let (code, stdout) = env.try_run(&["occur", "foo", "-t", "0800"]);
    assert_eq!(
        code,
        Some(11),
        "the backfill re-log is a no-op too: {stdout:?}"
    );
    let tail: Value =
        serde_json::from_str(env.run(&["tail", "--json"]).trim_end()).expect("tail json");
    assert_eq!(
        tail["rows"].as_array().map(Vec::len),
        Some(2),
        "still two rows: {tail}",
    );
}

/// ⛔ BOTH OR NEITHER. da-c8mh bought that with an atomic PAIR of rows;
/// da-gvcen has one row and a spec in a second table, and the property is
/// not optional. A spec written before an insert that then fails makes
/// every later log of the field see a spec — so `declares_field` is
/// false, no trailing switch is ever written, and an IGNORED field would
/// stay ignored until someone thought to run `datum track`
/// (canary-kimi-k3 on #68).
#[test]
fn a_failed_first_log_leaves_no_spec_behind() {
    let env = Env::new();
    // A row at the instant the multi below will share, so the second
    // side collides with it and the atomic insert fails.
    env.run(&["occur", "bb", "-t", "1100", "--show", "none"]);

    let (code, _) = env.try_run(&[
        "occur", "brandnew", "-t", "1100", ",", "occur", "bb", "-t", "1100", "cups=2",
    ]);
    assert_ne!(code, Some(0), "the colliding side must fail the window");

    let conn = rusqlite::Connection::open(&env.db).expect("open db");
    let specs: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE slug = 'spec:brandnew'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        specs, 0,
        "a failed window wrote a spec for a field with no rows"
    );
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE field = 'brandnew'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0, "and no row either");
}

/// ⚠ And the rule does NOT swallow a real duplicate identity. A second,
/// DIFFERENT assertion at the declaring row's instant is still refused
/// loudly, with the message that names the way out (da-2fkr) — the
/// narrowness is the point, or the declaration would become a licence to
/// drop writes.
#[test]
fn a_different_event_at_the_declaring_rows_instant_still_refuses() {
    let env = Env::new();
    env.run(&["occur", "coffee", "-t", "0900", "--show", "none"]);
    let (code, stdout) = env.try_run(&["occur", "coffee", "-t", "0900", "cups=2"]);
    assert_eq!(code, Some(11), "still exit 11: {stdout:?}");
    assert!(
        !stdout.contains("EXISTS"),
        "a different assertion is not a no-op: {stdout:?}"
    );
    let tail: Value =
        serde_json::from_str(env.run(&["tail", "--json"]).trim_end()).expect("tail json");
    assert_eq!(
        tail["rows"].as_array().map(Vec::len),
        Some(1),
        "and nothing was written: {tail}",
    );
}

/// An imported database is whole under the same rule a typed one is: a
/// v2 backup carrying three fields in three different shapes comes out
/// with a spec for each, with the verb its entries imply — and with
/// exactly the rows the backup holds. vividn, 2026-10-08, of the TRACK row
/// da-c8mh put a millisecond ahead of each field's first entry: «I don't
/// think anything should be there» (da-r6ofr). It was an event that never
/// happened, which `tail` drew as a plain `end`.
#[test]
fn v2import_declares_every_field_it_brings() {
    let env = Env::new();
    let backup = env._tmp.path().join("backup.json");
    std::fs::write(
        &backup,
        r#"{"backupTime":"2026-01-01T00:00:00.000Z","docs":[
{"_id":"coffee:2026-01-01T09:00:00.000Z","data":{"field":"coffee","occurTime":{"utc":"2026-01-01T09:00:00.000Z","o":0}},"meta":{"createTime":{"utc":"2026-01-01T09:00:00.000Z","o":0}}},
{"_id":"mood:2026-01-01T10:00:00.000Z","data":{"field":"mood","state":"happy","occurTime":{"utc":"2026-01-01T10:00:00.000Z","o":0}},"meta":{"createTime":{"utc":"2026-01-01T10:00:00.000Z","o":0}}},
{"_id":"work:2026-01-01T11:00:00.000Z","data":{"field":"work","state":true,"occurTime":{"utc":"2026-01-01T11:00:00.000Z","o":0}},"meta":{"createTime":{"utc":"2026-01-01T11:00:00.000Z","o":0}}}
]}"#,
    )
    .unwrap();

    env.run(&["db", "v2import", "--from", backup.to_str().unwrap()]);

    let conn = rusqlite::Connection::open(&env.db).expect("open db");
    let mut stmt = conn
        .prepare(
            "SELECT field, COUNT(*) FROM entries
             WHERE deleted_at IS NULL AND field IN ('coffee', 'mood', 'work')
             GROUP BY field ORDER BY field",
        )
        .unwrap();
    let rows: Vec<(String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        rows,
        vec![
            ("coffee".to_string(), 1),
            ("mood".to_string(), 1),
            ("work".to_string(), 1),
        ],
        "one row per imported document, and no TRACK row beside it",
    );

    // The spec is written too, so the next log of an imported field has a
    // verb to read rather than a log to guess from.
    for (field, verb) in [("coffee", "occur"), ("mood", "switch"), ("work", "start")] {
        let spec: String = conn
            .query_row(
                "SELECT data FROM entries WHERE slug = 'spec:' || ?1 AND deleted_at IS NULL",
                [field],
                |r| r.get(0),
            )
            .unwrap_or_else(|e| panic!("no spec for {field}: {e}"));
        let spec: Value = serde_json::from_str(&spec).unwrap();
        assert_eq!(spec["verb"], verb, "{field}'s spec");
        assert!(spec["default_verb"].is_null(), "{field} kept the old key");
    }
}

/// `setup --from-json --track` declares a new field whole — the spec, a
/// glyph and all, and its TRACK row — in one command, which one `undo`
/// takes back whole (da-2pbvd): the web's new-field Save, which was `track`
/// and then a `setup` for the glyph, two undos with the field half made.
/// On a field already tracked it writes the spec and no second row.
#[test]
fn spec_set_track_declares_in_one_and_undoes_in_one() {
    let env = Env::new();
    let spec = r##"{"verb":"occur","color":"#c69c6d","glyph":"☕"}"##;
    env.run(&["setup", "coffee", "--from-json", spec, "--track"]);

    let shown: Value = serde_json::from_str(&env.run(&["setup", "coffee", "--json"])).unwrap();
    assert_eq!(shown["glyph"], "☕", "{shown}");
    let rows = |env: &Env| {
        env.run(&["tail"])
            .lines()
            .filter(|l| l.contains("coffee"))
            .count()
    };
    assert_eq!(rows(&env), 1, "the TRACK row is in the log");

    let undone = env.run(&["undo"]);
    assert_eq!(
        undone.lines().filter(|l| l.starts_with("UNDONE ")).count(),
        2,
        "one `undo`, the TRACK row and the spec: {undone:?}"
    );
    assert!(env
        .run(&["setup", "coffee", "--json"])
        .contains("No spec defined"));
    assert!(env.run(&["tail"]).contains("[No data]"), "the row is gone");

    // Already tracked: the status has no change, so the spec alone.
    env.run(&["setup", "coffee", "--from-json", spec, "--track"]);
    let again = r##"{"verb":"occur","glyph":"☕","description":"cups"}"##;
    env.run(&["setup", "coffee", "--from-json", again, "--track"]);
    assert_eq!(rows(&env), 1, "a second `--track` wrote a second TRACK row");
    // And `--ignore` likewise: one IGNORED row however often it is said.
    env.run(&["setup", "coffee", "--ignore"]);
    env.run(&["setup", "coffee", "--ignore"]);
    assert_eq!(
        rows(&env),
        2,
        "the TRACK row and ONE IGNORED row: a second `--ignore` wrote another"
    );
}

/// `setup F` on a field with no spec names the next step (da-jjzwj).
#[test]
fn setup_of_a_field_with_no_spec_says_how_to_set_one_up() {
    let env = Env::new();
    env.run(&["occur", "coffee"]);
    assert_eq!(
        env.run(&["setup", "tea"]),
        "No spec defined for field 'tea'; set one up: `datum setup tea --verb occur` \
         (or start, switch, add; `datum help logging` says what each is)\n"
    );
}
