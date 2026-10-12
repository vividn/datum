//! Tests for `datum reparse <hid>...` (da-bkak, persisted-intent loop
//! producer is da-g5uk).
//!
//! Per da-bkak: reparse operates ONLY on entries explicitly named by
//! hid. Walking all entries is intentionally out of scope — long-
//! standing user-validated data is sacrosanct.

use crate::common;

use common::run_cli;
use datum_core::test_util::fresh_db;
use rusqlite::params;

/// Bare `reparse` (no hids) must error rather than walk all entries.
/// This is the load-bearing design constraint of da-bkak: opt-in only.
#[test]
fn reparse_with_no_hids_errors_does_not_walk() {
    let db = fresh_db();
    run_cli(&["occur", "coffee"], &db).expect("occur");

    let err = run_cli(&["reparse"], &db).expect_err("bare reparse must error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("requires at least one"),
        "error must mention the missing hid argument; got: {msg}"
    );
}

/// Golden invariant: a freshly-written entry has no parser drift.
/// Naming it explicitly to `reparse` (dry-run) must touch nothing.
#[test]
fn reparse_dryrun_clean_row_reports_zero_drift() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy"], &db).expect("switch");

    let hid = {
        let entries = datum_core::test_util::logged_all(&db);
        assert_eq!(entries.len(), 1);
        entries[0].hid.0.clone()
    };

    run_cli(&["reparse", &hid], &db).expect("reparse dry-run on clean row");

    let cur = datum_core::entry::CURRENT_PARSER_VERSION;
    let after = db.get_entry(&hid).unwrap();
    assert_eq!(
        after.parser_version, cur,
        "fresh write must stamp CURRENT_PARSER_VERSION"
    );
}

/// Drift detection: hand-mutate a stored row's `state` column out
/// from under the entry, then run `reparse <hid> --fix`. The repair
/// must restore the parser-derived state, and a second pass must be
/// a no-op.
#[test]
fn reparse_fix_repairs_a_hand_mutated_row() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy"], &db).expect("switch");

    let (slug, hid) = {
        let entries = datum_core::test_util::logged_all(&db);
        assert_eq!(entries.len(), 1);
        (entries[0].slug.clone(), entries[0].hid.0.clone())
    };

    db.conn()
        .execute(
            "UPDATE entries SET state = ?2 WHERE slug = ?1",
            params![slug.as_str(), "\"sad\""],
        )
        .unwrap();

    let mutated = db.get_entry(&slug).unwrap();
    assert_eq!(
        mutated.state.as_ref().and_then(|v| v.as_str()),
        Some("sad"),
        "row must be in the mutated 'sad' state before --fix"
    );

    run_cli(&["reparse", &hid, "--fix"], &db).expect("reparse --fix");

    let repaired = db.get_entry(&slug).unwrap();
    assert_eq!(
        repaired.state.as_ref().and_then(|v| v.as_str()),
        Some("happy"),
        "--fix must restore the parser-derived state"
    );

    // Idempotence: a second --fix is a no-op.
    let pre_modify = repaired.modify_time;
    run_cli(&["reparse", &hid, "--fix"], &db).expect("reparse --fix idempotent");
    let after = db.get_entry(&slug).unwrap();
    assert_eq!(
        after.state, repaired.state,
        "second --fix must not re-mutate a clean row"
    );
    assert!(
        after.modify_time >= pre_modify,
        "modify_time must not move backwards"
    );
}

/// Naming multiple hids in one invocation reparses each independently:
/// the named one repairs, the unnamed one is left alone (proving we
/// never walk).
#[test]
fn reparse_only_touches_named_rows() {
    let db = fresh_db();
    run_cli(&["switch", "mood", "happy"], &db).expect("switch mood");
    run_cli(&["switch", "energy", "high"], &db).expect("switch energy");

    let mood_entry = db
        .entries_for_field("mood", None)
        .unwrap()
        .first()
        .unwrap()
        .clone();
    let energy_entry = db
        .entries_for_field("energy", None)
        .unwrap()
        .first()
        .unwrap()
        .clone();

    for slug in [&mood_entry.slug, &energy_entry.slug] {
        db.conn()
            .execute(
                "UPDATE entries SET state = ?2 WHERE slug = ?1",
                params![slug.as_str(), "\"DRIFTED\""],
            )
            .unwrap();
    }

    // Name only the mood row; energy must stay drifted.
    run_cli(&["reparse", &mood_entry.hid.0, "--fix"], &db).expect("reparse mood by hid");

    let mood = db.get_entry(&mood_entry.slug).unwrap();
    let energy = db.get_entry(&energy_entry.slug).unwrap();
    assert_eq!(
        mood.state.as_ref().and_then(|v| v.as_str()),
        Some("happy"),
        "named mood row must be repaired"
    );
    assert_eq!(
        energy.state.as_ref().and_then(|v| v.as_str()),
        Some("DRIFTED"),
        "unnamed energy row must be left alone (reparse never walks)"
    );
}

/// da-jbzmb: a relative `*_date` / `*_time` key replays anchored where
/// the insert anchored it — the row's occur time, or a timeless row's
/// create time — not at the replay's "now" and not as the raw text. Run
/// months later, `--fix` must leave the resolved values (and the field)
/// as they were.
#[test]
fn reparse_fix_keeps_relative_temporal_keys_resolved() {
    let db = fresh_db();
    // Logged on the real clock: `create_time` is always the wall clock.
    run_cli(
        &["occur", "dishes", "-t", "2026-09-20T10:00", "next_date=+1"],
        &db,
    )
    .unwrap();
    run_cli(
        &["occur", "vacuum", "-T", "next_date=+7", "next_time=+2h"],
        &db,
    )
    .unwrap();
    let _g = datum_core::test_util::freeze_time("2030-01-01T12:00:00");

    for field in ["dishes", "vacuum"] {
        let before = datum_core::test_util::logged_entries(&db, field)[0].clone();
        let hid = before.hid.to_string();
        run_cli(&["reparse", &hid, "--fix"], &db).unwrap();
        let after = db.get_entry(&hid).unwrap();
        assert_eq!(after.field, field);
        assert_eq!(after.data, before.data, "{field}");
    }
    let dishes = &datum_core::test_util::logged_entries(&db, "dishes")[0];
    assert_eq!(
        dishes.data,
        Some(serde_json::json!({"next_date": "2026-09-21"}))
    );
}

/// da-9p48y: a field's first row declares the field in its own
/// `following_state` (da-gvcen), which `log_entry` adds from a spec
/// lookup and not from the line. Replay must keep that declaration —
/// and only that: a trailing state the line never spelled is still
/// drift.
#[test]
fn reparse_fix_keeps_the_field_declaration() {
    let db = fresh_db();
    run_cli(&["occur", "plain", "x=1"], &db).unwrap();
    let before = datum_core::test_util::logged_entries(&db, "plain")[0].clone();
    assert_eq!(before.following_state, Some(serde_json::json!("TRACKED")));
    let hid = before.hid.to_string();

    run_cli(&["reparse", &hid, "--fix"], &db).unwrap();
    let after = db.get_entry(&hid).unwrap();
    assert_eq!(after.following_state, before.following_state);
    assert_eq!(
        after.content_hash, before.content_hash,
        "a clean row is not rewritten"
    );

    db.conn()
        .execute(
            "UPDATE entries SET following_state = '\"sit\"' WHERE hid = ?1",
            params![hid.as_str()],
        )
        .unwrap();
    run_cli(&["reparse", &hid, "--fix"], &db).unwrap();
    assert_eq!(db.get_entry(&hid).unwrap().following_state, None);
}

/// da-0rx53: `start_time=` / `end_time=` (da-mjwq) place the block, so
/// replay lifts them as the insert did — nothing of them in `data`, the
/// pair's span as the dur, `end_time=` alone on `start` an `end` — and a
/// relative key anchors where the insert anchored it: at `start_time=`
/// when a duration follows it (a3 crosses midnight: stored at 00:00 on
/// the 2nd, anchored at 23:00 on the 1st), at the stored time otherwise.
#[test]
fn reparse_fix_lifts_start_time_and_end_time() {
    let db = fresh_db();
    let lines: [&[&str]; 4] = [
        &[
            "start",
            "a2",
            "start_time=2026-01-01T10:00",
            "end_time=2026-01-01T12:00",
        ],
        &[
            "occur",
            "a7",
            "start_time=2026-01-01T10:00",
            "end_time=2026-01-01T11:00",
        ],
        &["start", "a5", "end_time=2026-01-01T23:00"],
        &["start", "a3", "start_time=2026-01-01T23:00", "1h"],
    ];
    for line in lines {
        run_cli(&[line, &["next_date=+1"]].concat(), &db).unwrap();
    }
    let _g = datum_core::test_util::freeze_time("2030-01-01T12:00:00");

    for field in ["a2", "a7", "a5", "a3"] {
        let before = datum_core::test_util::logged_entries(&db, field)[0].clone();
        assert_eq!(
            before.data,
            Some(serde_json::json!({"next_date": "2026-01-02"})),
            "{field}"
        );
        let hid = before.hid.to_string();
        run_cli(&["reparse", &hid, "--fix"], &db).unwrap();
        let after = db.get_entry(&hid).unwrap();
        assert_eq!(after.content_hash, before.content_hash, "{field} is clean");
    }
}

/// da-98u27: a lifted `start_time=` is a clock time on replay as on the
/// insert, so `-y` with it is not date-only and its duration keeps its
/// slot rather than failing as extra data.
#[test]
fn reparse_keeps_the_duration_of_a_yesterday_start_time() {
    let db = fresh_db();
    run_cli(&["start", "bike", "-y", "start_time=10:00", "30m"], &db).unwrap();

    let before = datum_core::test_util::logged_entries(&db, "bike")[0].clone();
    let hid = before.hid.to_string();
    run_cli(&["reparse", &hid, "--fix"], &db).expect("replay parses");
    assert_eq!(
        db.get_entry(&hid).unwrap().content_hash,
        before.content_hash
    );
}

/// da-pdk6q: `end -L` records the closed block's state as
/// `data.last_state`, which `log_entry` looks up and the line never
/// spells — nor does `switch … end_time=E`, which runs as one (da-mjwq).
/// Replay keeps it.
#[test]
fn reparse_fix_keeps_the_closed_blocks_last_state() {
    let db = fresh_db();
    run_cli(&["start", "p", "-t", "2026-01-01T10:00"], &db).unwrap();
    run_cli(&["end", "p", "-L", "true", "-t", "2026-01-01T11:00"], &db).unwrap();
    run_cli(&["start", "q", "-t", "2026-01-01T10:00"], &db).unwrap();
    run_cli(&["switch", "q", "true", "end_time=2026-01-01T11:00"], &db).unwrap();

    for field in ["p", "q"] {
        let before = datum_core::test_util::logged_entries(&db, field)
            .into_iter()
            .find(|e| e.data.is_some())
            .unwrap();
        assert_eq!(
            before.data,
            Some(serde_json::json!({"last_state": true})),
            "{field}"
        );
        let hid = before.hid.to_string();
        run_cli(&["reparse", &hid, "--fix"], &db).unwrap();
        let after = db.get_entry(&hid).unwrap();
        assert_eq!(after.content_hash, before.content_hash, "{field} is clean");
    }
}

/// da-s1hot: a global `--db` rides at the head of raw_input and would
/// shadow the alias name at argv[1] on replay, as it would on the insert
/// (da-hfi3). Replay strips it too, in either spelling.
#[test]
fn reparse_replays_an_alias_logged_with_a_global_db_flag() {
    let db = fresh_db();
    run_cli(&["alias", "set", "over", "end activity"], &db).unwrap();
    run_cli(&["switch", "activity", "coding", "-t", "-30"], &db).unwrap();
    run_cli(&["--db", "g", "over", "-t", "-25"], &db).unwrap();
    run_cli(&["--db=g", "over", "-t", "-20"], &db).unwrap();

    let rows: Vec<_> = datum_core::test_util::logged_entries(&db, "activity")
        .into_iter()
        .filter(|e| e.alias_chain.is_some())
        .collect();
    assert_eq!(rows.len(), 2);
    for before in rows {
        let hid = before.hid.to_string();
        run_cli(&["reparse", &hid, "--fix"], &db)
            .unwrap_or_else(|e| panic!("{:?}: {e:?}", before.raw_input));
        let after = db.get_entry(&hid).unwrap();
        assert_eq!(
            after.content_hash, before.content_hash,
            "{:?}",
            before.raw_input
        );
    }
}

/// da-1bagp: `raw_input` is written with `shell_join`, the inverse of the
/// `shell_split` replay reads it with, so a token holding whitespace, a
/// double quote or a backslash comes back as the one token it was typed
/// as. Space-joined, `text=a b c` replayed as `text=a` plus two extras.
#[test]
fn reparse_replays_quoted_tokens_as_typed() {
    let db = fresh_db();
    let lines: [&[&str]; 4] = [
        &["occur", "q1", "text=a b c"],
        &["switch", "q2", "coding", "-c", "two words"],
        &["occur", "q3", r#"text=say "hi""#],
        &["occur", "q4", r"path=C:\dir\x"],
    ];
    for line in lines {
        run_cli(line, &db).unwrap();
    }

    for field in ["q1", "q2", "q3", "q4"] {
        let before = datum_core::test_util::logged_entries(&db, field)[0].clone();
        let hid = before.hid.to_string();
        run_cli(&["reparse", &hid, "--fix"], &db).unwrap();
        let after = db.get_entry(&hid).unwrap();
        assert_eq!(after.content_hash, before.content_hash, "{field} is clean");
    }
    let q3 = &datum_core::test_util::logged_entries(&db, "q3")[0];
    assert_eq!(q3.data, Some(serde_json::json!({"text": r#"say "hi""#})));
}

/// da-fohr0: a note's bare words are its comment, a default the insert
/// applied and replay did not, so every plain note replayed its text as
/// a state and `--fix` moved it there.
#[test]
fn reparse_replays_a_plain_note_as_its_comment() {
    let db = fresh_db();
    run_cli(&["note", "town", "hello", "world"], &db).unwrap();
    let before = datum_core::test_util::logged_entries(&db, "town")[0].clone();
    let hid = before.hid.to_string();
    run_cli(&["reparse", &hid, "--fix"], &db).unwrap();
    let after = db.get_entry(&hid).unwrap();
    assert_eq!(after.state, None);
    assert_eq!(
        after.data,
        Some(serde_json::json!({"comment": ["hello world"]}))
    );
    assert_eq!(after.content_hash, before.content_hash);
}

/// da-fohr0: one canonical line per log verb and flag, each replayed with
/// `--fix`, must come back unchanged — the insert and the replay are two
/// callers of one parser, and this is where they would drift apart.
#[test]
fn reparse_round_trips_every_log_verb() {
    let db = fresh_db();
    let lines: [&[&str]; 17] = [
        &["occur", "r1"],
        &["occur", "r2", "occur_time=12:00"],
        &["start", "r3", "-y"],
        &["start", "r4", "start_time=-60", "30m"],
        &["end", "r5", "20"],
        &["switch", "r6", "happy", "5"],
        &["switch", "r7", "-I", "very", "happy"],
        &["switch", "r8", "sit", "5", ",", "switch", "r8", "stand"],
        &["add", "r9", "-K", "x", "x=1"],
        &["add", "r10", "--id", "b", "start_time=-60", "end_time=-30"],
        &["note", "r11", "hello", "world"],
        &["note", "r12", "-R", "topic", "hi", "there"],
        &["note", "r13", "-S", "hi", "there"],
        &["note", "r14", "-C", "hi", "there"],
        &["switch", "r15", "coding", "-C", "deep", "work"],
        &["start", "r16", "-S", "a", "title", "90"],
        &["note", "r17", "end_time=-5", "why", "not"],
    ];
    for line in lines {
        run_cli(line, &db).unwrap_or_else(|e| panic!("{line:?}: {e:?}"));
    }

    for (i, line) in lines.iter().enumerate() {
        let field = format!("r{}", i + 1);
        let before = datum_core::test_util::logged_entries(&db, &field)
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("{line:?} logged nothing"));
        let hid = before.hid.to_string();
        run_cli(&["reparse", &hid, "--fix"], &db).unwrap_or_else(|e| panic!("{line:?}: {e:?}"));
        let after = db.get_entry(&hid).unwrap();
        assert_eq!(
            (after.state, after.data, after.content_hash),
            (before.state, before.data, before.content_hash),
            "{line:?}"
        );
    }
}

/// da-b2f8o: a row logged before its key was typed — `wake_time=soonish`,
/// stored as text then, refused now — is reported and passed over, and the
/// rows named after it are still replayed and fixed.
#[test]
fn reparse_passes_over_a_row_that_no_longer_parses() {
    let db = fresh_db();
    run_cli(&["occur", "coffee", "wake_time=07:00"], &db).expect("occur coffee");
    run_cli(&["switch", "mood", "happy"], &db).expect("switch mood");
    let hid = |field: &str| db.entries_for_field(field, None).unwrap()[0].hid.0.clone();
    let (coffee, mood) = (hid("coffee"), hid("mood"));
    db.conn()
        .execute_batch(
            "UPDATE entries SET raw_input = replace(raw_input, '07:00', 'soonish') \
               WHERE field = 'coffee';
             UPDATE entries SET state = '\"sad\"' WHERE field = 'mood';",
        )
        .unwrap();

    run_cli(&["reparse", &coffee, &mood, "--fix"], &db).expect("reparse runs on");
    assert_eq!(
        db.get_entry(&mood).unwrap().state,
        Some(serde_json::json!("happy"))
    );
}

/// da-b2f8o: the types a spec gave its keys ride with the entry, as its
/// bound keys do (da-zv2jr), so `--fix` replays `at=yesterday 10am` as the
/// datetime it was logged as — not as the text, which no longer says which
/// day it was.
#[test]
fn reparse_fix_replays_a_spec_typed_value_as_its_type() {
    let db = fresh_db();
    let spec = serde_json::json!({"field": "run", "verb": "occur", "subfields": {
        "at": {"value_type": "datetime"},
        "on": {"value_type": "date"},
        "code": {"value_type": "string"},
    }});
    run_cli(
        &["spec", "set", "run", "--from-json", &spec.to_string()],
        &db,
    )
    .unwrap();
    let line = [
        "occur",
        "run",
        "at=yesterday 10am",
        "on=yesterday",
        "code=007",
    ];
    run_cli(&[&line[..], &["-t", "2026-10-07T12:00"]].concat(), &db).unwrap();
    let hid = db.entries_for_field("run", None).unwrap()[0].hid.0.clone();
    let logged =
        serde_json::json!({"at": "2026-10-06T10:00:00", "on": "2026-10-06", "code": "007"});
    assert_eq!(db.get_entry(&hid).unwrap().data, Some(logged.clone()));

    run_cli(&["reparse", &hid, "--fix"], &db).expect("reparse --fix");
    assert_eq!(db.get_entry(&hid).unwrap().data, Some(logged));
}

/// da-x15of: a `*_time` value logged before it was an instant — a clock
/// time that lost its day — is listed by `datum check`, typed by its name
/// alone, and the command the listing names reads the day again from the
/// entry's line.
#[test]
fn check_lists_a_clock_time_and_its_reparse_recovers_the_day() {
    let db = fresh_db();
    run_cli(
        &[
            "occur",
            "plants",
            "-t",
            "2024-01-15T20:00",
            "next_time=+16h",
        ],
        &db,
    )
    .unwrap();
    let hid = datum_core::test_util::logged_entries(&db, "plants")[0]
        .hid
        .0
        .clone();
    db.conn()
        .execute(
            r#"UPDATE entries SET data = '{"next_time":"12:00:00"}' WHERE hid = ?1"#,
            params![hid.as_str()],
        )
        .unwrap();
    let listed = || {
        let entries = db.list_including_deleted().unwrap();
        let report = datum_commands::commands::check::collect_issues(&db, &entries).unwrap();
        report
            .issues
            .into_iter()
            .find(|i| i.category == "mistyped_data_value")
            .map(|i| i.entries[0].state.clone())
    };
    let detail = listed().expect("check lists the clock time");
    let cmd = detail.split('`').nth(1).expect("a command in backticks");
    let argv: Vec<&str> = cmd.split_whitespace().skip(1).collect();
    assert_eq!(argv[0], "reparse", "{detail}");
    run_cli(&argv, &db).unwrap();

    let entry = &datum_core::test_util::logged_entries(&db, "plants")[0];
    let next = &entry.data.as_ref().unwrap()["next_time"];
    let next = datum_parse::time::DatumTime::from_v2_json(next).expect("an instant again");
    assert_eq!(
        next.local_time().naive_local().to_string(),
        "2024-01-16 12:00:00"
    );
    assert_eq!(listed(), None, "nothing left to list");
}

/// What plain `datum check` prints.
fn plain_check(db: &datum_core::db::Db) -> String {
    let entries = db.list_including_deleted().unwrap();
    let report = datum_commands::commands::check::collect_issues(db, &entries).unwrap();
    datum_commands::commands::check::render_human(&report)
}

/// da-q3r7v: plain `datum check` says under each row what is wrong with it,
/// and names the reparse for a clock time whose line still replays. A row
/// with no line — merged from another device, imported — is listed without
/// a command that would skip it.
#[test]
fn plain_check_names_the_reparse_under_each_row_it_mends() {
    let db = fresh_db();
    for at in ["2024-01-15T20:00", "2024-01-16T20:00"] {
        run_cli(&["occur", "plants", "-t", at, "next_time=+16h"], &db).unwrap();
    }
    let logged = datum_core::test_util::logged_entries(&db, "plants");
    let (mended, lineless) = (logged[0].short_uid(), logged[1].short_uid());
    let conn = db.conn();
    conn.execute(
        r#"UPDATE entries SET data = '{"next_time":"12:00:00"}' WHERE field = 'plants'"#,
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE entries SET raw_input = NULL WHERE hid = ?1",
        params![logged[1].hid.0.as_str()],
    )
    .unwrap();

    let plain = plain_check(&db);
    let lines: Vec<&str> = plain.lines().collect();
    let under = |hid: &str| {
        let row = lines.iter().position(|l| l.contains(hid));
        lines[row.unwrap_or_else(|| panic!("no row for {hid}:\n{plain}")) + 1]
    };
    let wrong = r#"      next_time: "12:00:00" is not a time"#;
    assert_eq!(
        under(&mended),
        format!("{wrong}: `datum reparse {mended} --fix`"),
        "{plain}"
    );
    assert_eq!(under(&lineless), wrong, "{plain}");
}

/// A newline in a state spreads its row over two lines, so nothing is paired
/// by position: every row stays, and each detail names the row it is about.
#[test]
fn plain_check_keeps_every_row_when_one_spans_lines() {
    let db = fresh_db();
    let at = ["-t", "2024-01-15T20:00", "next_time=+16h"];
    run_cli(&[&["switch", "mood", "two\nlines"], &at[..]].concat(), &db).unwrap();
    run_cli(&[&["occur", "plants"], &at[..]].concat(), &db).unwrap();
    db.conn()
        .execute(
            r#"UPDATE entries SET data = '{"next_time":"12:00:00"}' WHERE field IN ('mood', 'plants')"#,
            [],
        )
        .unwrap();

    let plain = plain_check(&db);
    for field in ["mood", "plants"] {
        let hid = datum_core::test_util::logged_entries(&db, field)[0].short_uid();
        let named: Vec<&str> = plain.lines().filter(|l| l.contains(&hid)).collect();
        let detail = format!(
            r#"      {hid} next_time: "12:00:00" is not a time: `datum reparse {hid} --fix`"#
        );
        assert_eq!(named.len(), 2, "a row and a detail for {field}:\n{plain}");
        assert_eq!(named[1], detail, "{plain}");
    }
}
