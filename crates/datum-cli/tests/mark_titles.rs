//! Every mark in a drawn dayview / nowview says what it is (da-s5da): its
//! field, its state as the tail prints it, its instants, and a `<title>`
//! reading the row the way the tail does — checked here against the
//! tail's own `--json` rows for the same database, not against a second
//! copy of the rules.

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
            .env("DATUM_COLOR", "0")
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
/// own, and a title naming `field: state` and the row's own `HH:MM` —
/// the clock the tail's time column starts with.
fn assert_marks_are_the_tail_rows(svg: &str, rows: &[Value]) -> Vec<String> {
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
        let name = match state {
            Some(s) => format!("{field}: {s}"),
            None => field.clone(),
        };
        assert!(m.title.starts_with(&name), "{m:?} is not {name}");
        if m.tag == "rect" {
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
        assert_marks_are_the_tail_rows(&svg, &env.tail_rows()),
        [
            "coffee",
            "project: coding 14:00–14:30",
            // Still running when the day ends: an open span.
            "project: writing 14:30 →",
            // `start run 30m` ENDS at 16:00 — the span precedes it.
            "run: start 15:30–16:00",
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
    let titles = assert_marks_are_the_tail_rows(&svg, &env.tail_rows());
    assert_eq!(titles.len(), 3, "{titles:?}");
    assert_eq!(titles[0], "coffee");
    assert!(titles[1].starts_with("mood: happy ") && titles[1].ends_with(" →"));
    assert!(titles[2].starts_with("run: start ") && titles[2].contains('–'));
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
    assert!(html.contains(r#"data-field="coffee" data-state="∅" data-at="2026-01-15T20:05:00.000Z"><title>coffee</title></circle>"#), "{html}");
}
