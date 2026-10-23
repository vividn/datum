//! da-w149: `-T` on a state verb is a note about a state. The row keeps the
//! state, has no `occur_time`, and is a MOMENT point; it renders as `¢` and
//! the state label, both in the state's colour as fg, with no LEAD/TRAIL and
//! no bg chip — and no surface compiles it into a state. A duration beside
//! `-T` is dropped (da-fu7a0).
//!
//! Runs the binary so the rendered lines carry their real SGR bytes.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value};

const BG: &str = "\x1b[48;2;";

fn tmp_db() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("t.db");
    (dir, db)
}

fn run(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("FORCE_COLOR", "1")
        .env_remove("NO_COLOR")
        .args(args)
        .output()
        .expect("run datum")
}

fn ok(db: &Path, args: &[&str]) -> String {
    let out = run(db, args);
    assert!(
        out.status.success(),
        "datum {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn json_of(db: &Path, args: &[&str]) -> Value {
    serde_json::from_str(&ok(db, args)).expect("valid JSON")
}

/// The fg SGR for a name's MD5 colour (no spec), as `render::colors` paints it.
fn fg(name: &str) -> String {
    use md5::Digest;
    let h = md5::Md5::digest(name.as_bytes());
    format!("\x1b[38;2;{};{};{}m", h[0], h[1], h[2])
}

fn paint(color: &str, body: &str) -> String {
    format!("{color}{body}\x1b[0m")
}

/// The echo's info line from everything after the field badge on: the part
/// that renders the state. (The badge is the field's own bg chip on every
/// row, timed or not.)
fn state_part(stdout: &str, field: &str) -> String {
    // The entry's own info line — the one under its `CREATE` header. A
    // field's first log prints the TRACK document that declares it first
    // (da-c8mh), and data lines may follow.
    let lines: Vec<&str> = stdout.lines().collect();
    let header = lines
        .iter()
        .rposition(|l| l.contains("CREATE"))
        .expect("CREATE header");
    let line = lines[header + 1];
    let badge_end = line.find(field).expect("field badge") + field.len();
    line[badge_end..].to_string()
}

/// The last `tail --json` row.
fn last_row(db: &Path) -> Value {
    let doc = json_of(db, &["tail", "--json"]);
    doc["rows"].as_array().unwrap().last().unwrap().clone()
}

// ─── Tests 1 + 2: the row, the render, and every surface skipping it ────────

#[test]
fn switch_dash_t_is_a_moment_note_rendered_in_the_states_fg() {
    let (_dir, db) = tmp_db();
    let out = ok(&db, &["switch", "environment", "outside", "-T"]);

    assert_eq!(
        state_part(&out, "environment"),
        format!(
            "\x1b[0m {}{}",
            paint(&fg("outside"), "¢"),
            paint(&fg("outside"), "outside")
        )
    );
    assert!(!state_part(&out, "environment").contains(BG));
    assert!(out.lines().nth(1).unwrap().starts_with("\x1b[90mc\x1b[0m"));

    let row = last_row(&db);
    assert_eq!(row["verb"], "point");
    assert_eq!(row["state"], "outside");
    assert_eq!(row["dur"], "MOMENT");
    assert_eq!(row["time_metric"], "create");
    let slug = row["slug"].as_str().unwrap();
    assert!(
        slug.starts_with("environment:") && slug.ends_with('c'),
        "{slug}"
    );

    // No state compiles out of it: `now` has nothing to say.
    let now = json_of(&db, &["now", "--json"]);
    assert_eq!(now["active"], json!([]));
}

#[test]
fn a_dash_t_note_leaves_the_state_it_names_untouched() {
    let (_dir, db) = tmp_db();
    ok(&db, &["switch", "environment", "inside", "-t", "-5"]);
    ok(&db, &["switch", "environment", "outside", "-T"]);

    let now = json_of(&db, &["now", "--json"]);
    assert_eq!(now["active"].as_array().unwrap().len(), 1);
    assert_eq!(now["active"][0]["field"], "environment");
    assert_eq!(now["active"][0]["state"], "inside");

    let dir = tempfile::tempdir().unwrap();
    let dayview = dir.path().join("d.json");
    ok(&db, &["dayview", "--json", "-o", dayview.to_str().unwrap()]);
    let layout: Value = serde_json::from_str(&std::fs::read_to_string(&dayview).unwrap()).unwrap();
    let fields = layout["days"][0]["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 1);
    let blocks = fields[0]["blocks"].as_array().unwrap();
    assert_eq!(blocks.len(), 1, "only inside's block: {blocks:?}");
    assert_eq!(blocks[0]["state_label"], "inside");
    assert_eq!(fields[0]["points"], json!([]));

    let nowview = dir.path().join("n.svg");
    ok(
        &db,
        &["nowview", "--output-file", nowview.to_str().unwrap()],
    );
    let svg = std::fs::read_to_string(&nowview).unwrap();
    let hex = |name: &str| {
        use md5::Digest;
        let h = md5::Md5::digest(name.as_bytes());
        format!("#{:02x}{:02x}{:02x}", h[0], h[1], h[2])
    };
    assert!(svg.contains(&hex("inside")), "nowview draws inside");
    assert!(!svg.contains(&hex("outside")), "nowview draws no outside");

    let stats = json_of(&db, &["stats", "environment", "--json"]);
    let states: Vec<&str> = stats["states"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["state"].as_str().unwrap())
        .collect();
    assert_eq!(states, ["inside"]);
    assert_eq!(stats["total_count"], 1);
}

// ─── Test 3: booleans, structured and parallel states ───────────────────────

#[test]
fn boolean_states_render_on_and_off_in_the_field_colour() {
    let (_dir, db) = tmp_db();
    let work = fg("work");
    for (verb, label, state) in [("start", "on", true), ("end", "off", false)] {
        let out = ok(&db, &[verb, "work", "-T"]);
        assert_eq!(
            state_part(&out, "work"),
            format!("\x1b[0m {}{}", paint(&work, "¢"), paint(&work, label))
        );
        let row = last_row(&db);
        assert_eq!(row["state"], state);
        assert_eq!(row["dur"], "MOMENT");
        assert_eq!(row["verb"], "point");
    }
}

#[test]
fn structured_and_parallel_states_render_through_the_moment_label() {
    let (_dir, db) = tmp_db();
    let out = ok(&db, &["switch", "project", ".id=x", ".title=y", "-T"]);
    let part = state_part(&out, "project");
    assert!(part.contains("¢") && part.contains("{x}"), "{part:?}");
    assert!(!part.contains(BG), "{part:?}");

    let out = ok(&db, &["switch", "mood", "a,b", "-T"]);
    assert_eq!(
        state_part(&out, "mood"),
        format!(
            "\x1b[0m {}{}{}{}",
            paint(&fg("a"), "¢"),
            paint(&fg("a"), "a"),
            paint(&fg("b"), ","),
            paint(&fg("b"), "b")
        )
    );
}

// ─── Test 4: a duration is dropped, `-m` is redundant ───────────────────────

#[test]
fn dash_t_drops_a_duration() {
    let (_dir, db) = tmp_db();
    for args in [
        &["start", "work", "30m", "-T"][..],
        &["switch", "environment", "outside", "dur=5m", "-T"][..],
    ] {
        ok(&db, args);
        let row = last_row(&db);
        assert_eq!(row["dur"], "MOMENT", "{args:?}");
        assert_eq!(row["verb"], "point", "{args:?}");
        assert_eq!(row["time_metric"], "create", "{args:?}");
    }
}

#[test]
fn dash_m_with_dash_t_is_accepted() {
    let (_dir, db) = tmp_db();
    ok(&db, &["switch", "environment", "outside", "-m", "-T"]);
    let row = last_row(&db);
    assert_eq!(row["dur"], "MOMENT");
    assert_eq!(row["verb"], "point");
    assert_eq!(row["time_metric"], "create");
}

// ─── Test 5: plain `occur -T` unchanged ─────────────────────────────────────

#[test]
fn occur_dash_t_keeps_the_bare_glyph_in_the_field_colour() {
    let (_dir, db) = tmp_db();
    let out = ok(&db, &["occur", "coffee", "-T"]);
    assert_eq!(
        state_part(&out, "coffee"),
        format!("\x1b[0m {}", paint(&fg("coffee"), "¢"))
    );
    assert_eq!(last_row(&db)["state"], Value::Null);
}

// ─── Test 7: the tail table carries the same fg cell ────────────────────────

#[test]
fn tail_state_cell_is_the_same_fg_label() {
    let (_dir, db) = tmp_db();
    ok(&db, &["switch", "environment", "outside", "-T"]);
    let tail = ok(&db, &["tail"]);
    // The last row: the field's TRACK document (da-c8mh) is the row above.
    let line = tail.lines().last().expect("tail row");
    let cell = format!(
        "{}{}",
        paint(&fg("outside"), "¢"),
        paint(&fg("outside"), "outside")
    );
    assert!(line.contains(&cell), "{line:?}");
    let after_badge = &line[line.find("environment").unwrap()..];
    assert!(!after_badge.contains(BG), "{line:?}");
}
