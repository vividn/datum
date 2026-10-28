//! Every mark in a drawn dayview / nowview says what it is (da-s5da): its
//! field, its state as the tail prints it, its instants, the WHOLE hid of
//! the entry that generated it (da-6hg6v), and a `<title>` reading the row
//! the way the tail does — checked here against the tail's own `--json`
//! rows for the same database, not against a second copy of the rules.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    db: PathBuf,
    home: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        Self {
            db: tmp.path().join("marks.db"),
            home,
            _tmp: tmp,
        }
    }

    fn run(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.home)
            .env("NO_COLOR", "1")
            .env("TZ", "America/Chicago")
            .args(args)
            .output()
            .expect("spawn datum");
        assert!(
            out.status.success(),
            "datum {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).expect("utf-8")
    }

    fn tail_rows(&self) -> Vec<Value> {
        let doc: Value =
            serde_json::from_str(&self.run(&["tail", "--json", "-n", "100"])).expect("tail --json");
        doc["rows"].as_array().expect("rows").clone()
    }
}

/// One hoverable mark: a block `<rect>` or a dot's outer `<circle>`.
#[derive(Debug)]
struct Mark {
    tag: &'static str,
    attrs: String,
    title: String,
}

impl Mark {
    fn attr(&self, name: &str) -> Option<String> {
        let key = format!(r#" {name}=""#);
        let a = self.attrs.find(&key)? + key.len();
        let b = self.attrs[a..].find('"')? + a;
        Some(self.attrs[a..b].to_string())
    }
}

fn marks(svg: &str) -> Vec<Mark> {
    let mut out = Vec::new();
    for (open, tag) in [
        (r#"<rect class="block"#, "rect"),
        (r#"<circle class="point-bg""#, "circle"),
    ] {
        for piece in svg.split(open).skip(1) {
            let head = &piece[..piece.find('>').expect("tag close")];
            let title = piece
                .split_once("<title>")
                .and_then(|(_, t)| t.split_once("</title>"))
                .map(|(t, _)| t.to_string())
                .unwrap_or_default();
            out.push(Mark {
                tag,
                attrs: head.to_string(),
                title,
            });
        }
    }
    out
}

fn instant(iso: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(iso)
        .expect("ISO instant")
        .timestamp_millis()
}

/// Each mark is its row as the tail reads it: same field, the tail's
/// state label (`∅` for none), the row's instant as one of the mark's
/// own, the row's own `hid` as `data-id`, and a title naming `field:
/// state`. A nowview's block title goes on to the row's own `HH:MM` — the
/// clock the tail's time column starts with; a dayview's title is that
/// name and, where the row has one, its `[dur]`, and `data-created` is the
/// row's create time (da-k16pi).
fn assert_marks_are_the_tail_rows(svg: &str, rows: &[Value], dayview: bool) -> Vec<String> {
    let marks = marks(svg);
    assert!(!marks.is_empty(), "no marks in: {svg}");
    for m in &marks {
        let field = m.attr("data-field").expect("data-field");
        let times: Vec<i64> = ["data-at", "data-from", "data-to"]
            .iter()
            .filter_map(|a| m.attr(a))
            .map(|t| instant(&t))
            .collect();
        assert_eq!(times.len(), if m.tag == "circle" { 1 } else { 2 }, "{m:?}");
        let row = rows
            .iter()
            .find(|r| {
                r["field"] == field.as_str()
                    && times.contains(&instant(r["time"].as_str().unwrap()))
            })
            .unwrap_or_else(|| panic!("no tail row for {m:?} in {rows:#?}"));
        let state = row["state_label"].as_str();
        assert_eq!(m.attr("data-state").as_deref(), Some(state.unwrap_or("∅")));
        // da-6hg6v: the mark names the entry that generated it, whole —
        // the same hid the tail prints, not the short prefix.
        assert_eq!(
            m.attr("data-id").as_deref(),
            row["hid"].as_str(),
            "{m:?} does not name its entry"
        );
        let name = match state {
            Some(s) => format!("{field}: {s}"),
            None => field.clone(),
        };
        assert!(m.title.starts_with(&name), "{m:?} is not {name}");
        if dayview {
            // The `[30m]` only where the row has a duration; the list the
            // caller pins says how it is spelled.
            let rest = &m.title[name.len()..];
            let dur = row["dur_seconds"].is_number();
            assert!(
                if dur {
                    rest.starts_with(" [") && rest.ends_with(']')
                } else {
                    rest.is_empty()
                },
                "{m:?}"
            );
            assert_eq!(
                m.attr("data-created").map(|t| instant(&t)),
                row["create_time"].as_str().map(instant),
                "{m:?}"
            );
        } else if m.tag == "rect" {
            let clock = &row["time"].as_str().unwrap()[11..16];
            assert!(m.title.contains(clock), "{m:?} does not read {clock}");
        }
    }
    let mut titles: Vec<String> = marks.iter().map(|m| m.title.clone()).collect();
    titles.sort();
    titles
}

#[test]
fn dayview_marks_read_as_the_tail_rows_they_came_from() {
    let env = Env::new();
    for line in [
        "occur coffee -d 2026-01-15 -t 1405",
        "switch project coding -d 2026-01-15 -t 1400",
        "switch project writing -d 2026-01-15 -t 1430",
        "start run 30m -d 2026-01-15 -t 1600",
    ] {
        env.run(&line.split(' ').collect::<Vec<_>>());
    }
    let svg = env.run(&["dayview", "-d", "2026-01-15"]);
    assert_eq!(
        assert_marks_are_the_tail_rows(&svg, &env.tail_rows(), true),
        [
            "coffee",
            // vividn, 2026-10-28: «just field: state [dur, if dur was
            // specified on the entry that created it]».
            "project: coding",
            "project: writing",
            "run: on [30m]",
        ]
    );
    // The day says which span its axis covers, for a viewer mapping a
    // pointer to an instant.
    assert!(
        svg.contains(r#"data-from="2026-01-15T06:00:00.000Z" data-to="2026-01-16T06:00:00.000Z" data-axis="x""#),
        "{svg}"
    );
}

#[test]
fn nowview_marks_read_as_the_tail_rows_they_came_from() {
    let env = Env::new();
    env.run(&["switch", "mood", "happy", "-t", "-20"]);
    env.run(&["occur", "coffee", "-t", "-10"]);
    env.run(&["start", "run", "5m", "-t", "-3"]);
    // da-7q8r: the marks live in the time band, and bare `nowview` is the
    // snapshot — which has no band. The context window is what asks for one.
    let svg = env.run(&["nowview", "--context", "30m", "-o", "-"]);
    let titles = assert_marks_are_the_tail_rows(&svg, &env.tail_rows(), false);
    assert_eq!(titles.len(), 3, "{titles:?}");
    assert_eq!(titles[0], "coffee");
    assert!(titles[1].starts_with("mood: happy ") && titles[1].ends_with(" →"));
    assert!(titles[2].starts_with("run: on ") && titles[2].contains('–'));
}

/// The `.html` output is the SVG in a page: a mark's `<title>` is its
/// first child, which is what makes it the element's accessible name in
/// a browser — no script involved.
#[test]
fn html_output_carries_the_titles() {
    let env = Env::new();
    env.run(&["occur", "coffee", "-d", "2026-01-15", "-t", "1405"]);
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("day.html");
    env.run(&["dayview", "-d", "2026-01-15", "-o", path.to_str().unwrap()]);
    let html = std::fs::read_to_string(&path).unwrap();
    // The dot's own row, not the TRACK row the first log also wrote.
    let rows = env.tail_rows();
    let row = rows
        .iter()
        .find(|r| r["time"].as_str().is_some_and(|t| t.contains("T14:05")))
        .expect("the 14:05 row");
    let hid = row["hid"].as_str().unwrap();
    let created =
        chrono::DateTime::from_timestamp_millis(instant(row["create_time"].as_str().unwrap()))
            .unwrap()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ");
    assert!(
        html.contains(&format!(
            r#"data-field="coffee" data-state="∅" data-id="{hid}" data-at="2026-01-15T20:05:00.000Z" data-created="{created}"><title>coffee</title></circle>"#
        )),
        "{html}"
    );
}

/// da-6hg6v, the reason the browser could not derive this: WHICH END of a
/// block holds the entry that generated it depends on how the block was
/// made. A state change runs FORWARD from its entry, so the entry is at
/// `data-from`; a duration runs BACK to its entry, so the entry is at
/// `data-to`. Here two blocks cover the very same span and their ids sit
/// at opposite ends of it — no rule over `from`/`to` alone could tell
/// them apart.
#[test]
fn the_end_holding_the_entry_depends_on_how_the_block_was_made() {
    let env = Env::new();
    for line in [
        // A state change: its entry opens the block at 14:00.
        "switch state on -d 2026-01-15 -t 1400",
        "switch state off -d 2026-01-15 -t 1430",
        // A duration: its entry CLOSES the block at 14:30.
        "start span 30m -d 2026-01-15 -t 1430",
    ] {
        env.run(&line.split(' ').collect::<Vec<_>>());
    }
    let svg = env.run(&["dayview", "-d", "2026-01-15"]);
    let rows = env.tail_rows();
    let hid_at = |field: &str, clock: &str| {
        rows.iter()
            .find(|r| r["field"] == field && r["time"].as_str().is_some_and(|t| t.contains(clock)))
            .unwrap_or_else(|| panic!("no {field} row at {clock} in {rows:#?}"))["hid"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let marked = |field: &str| {
        marks(&svg)
            .into_iter()
            .find(|m| {
                m.attr("data-field").as_deref() == Some(field)
                    && m.attr("data-from").as_deref() == Some("2026-01-15T20:00:00.000Z")
                    && m.attr("data-to").as_deref() == Some("2026-01-15T20:30:00.000Z")
            })
            .unwrap_or_else(|| panic!("no 14:00–14:30 block on {field} in {svg}"))
            .attr("data-id")
            .expect("data-id")
    };
    // Same span, opposite ends.
    assert_eq!(marked("state"), hid_at("state", "T14:00"));
    assert_eq!(marked("span"), hid_at("span", "T14:30"));
}

/// da-6hg6v, vividn: «if you double tap a color, that color is an open
/// block, so it should select the datum entry that generated that open
/// block». The now-chunk is that open block, so it names the entry that
/// OPENED the state — even though that entry is drawn, as a mark, back
/// in the band to its left.
#[test]
fn the_now_chunk_names_the_entry_that_opened_the_state() {
    let env = Env::new();
    env.run(&["switch", "mood", "happy", "-t", "-20"]);
    // Its own row — not the TRACK marker the first log on a field writes.
    let rows = env.tail_rows();
    let opener = rows
        .iter()
        .find(|r| r["field"] == "mood" && r["state_label"] == "happy")
        .expect("the switch row")["hid"]
        .as_str()
        .unwrap()
        .to_string();
    // A second field the chunk must not borrow an id from.
    env.run(&["switch", "posture", "sitting", "-t", "-10"]);

    let svg = env.run(&["nowview", "--history", "30m", "-o", "-"]);
    let mood = svg
        .split(r#"<g class="field" data-field="mood">"#)
        .nth(1)
        .unwrap_or_else(|| panic!("no mood band in {svg}"));
    let chunk = mood
        .split(r#"<rect class="now-chunk"#)
        .nth(1)
        .unwrap_or_else(|| panic!("no now-chunk in {svg}"));
    let chunk = &chunk[..chunk.find('>').expect("tag close")];
    assert!(
        chunk.contains(&format!(r#"data-id="{opener}""#)),
        "chunk {chunk} does not name {opener}"
    );

    // And the panel `--json` names the same entry, so a caller reading
    // the panel and a caller reading the SVG select the same row.
    let panel: Value = serde_json::from_str(&env.run(&["nowview", "--json"])).expect("json");
    let mood = panel["panel"]
        .as_array()
        .expect("panel")
        .iter()
        .find(|r| r["field"] == "mood")
        .expect("a mood row");
    assert_eq!(mood["id"].as_str(), Some(opener.as_str()));
}
