//! An occur dot's GLYPH (da-gzowt): per field, per state, or mapped from a
//! data key — in the dayview SVG and in `datum now`'s terminal band.
//!
//! Runs the real binary so both surfaces are read off stdout, as a person
//! reads them.

use std::process::Command;

struct Env {
    tmp: tempfile::TempDir,
}

impl Env {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().unwrap(),
        }
    }

    fn run(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", self.tmp.path().join("g.db"))
            .env("HOME", self.tmp.path())
            .env("DATUM_HOME", self.tmp.path())
            .env("DATUM_CONFIG_HOME", self.tmp.path())
            .env("DATUM_COLOR", "0")
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "datum {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn fails(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", self.tmp.path().join("g.db"))
            .env("HOME", self.tmp.path())
            .args(args)
            .output()
            .unwrap();
        assert!(!out.status.success(), "datum {args:?} should fail");
        String::from_utf8_lossy(&out.stderr).into_owned()
    }

    /// Each dot of `field` in the dayview of yesterday and today, oldest
    /// first, as the mark it draws: its glyph, or `●` for the inner circle.
    /// Two days, because the entries are minutes old and just after midnight
    /// some of them are yesterday's.
    fn svg_marks(&self, field: &str) -> Vec<String> {
        let svg = self.run(&["dayview", "-n", "2"]);
        let attr = format!(r#"data-field="{field}""#);
        svg.split(r#"<circle class="point-bg""#)
            .skip(1)
            .filter(|dot| dot.split('>').next().unwrap().contains(&attr))
            .map(|dot| {
                let fg = dot.split(r#"class="point-fg""#).nth(1).unwrap();
                match dot.contains(r#"<text class="point-fg""#) {
                    true => fg
                        .split('>')
                        .nth(1)
                        .unwrap()
                        .split('<')
                        .next()
                        .unwrap()
                        .to_string(),
                    false => "●".to_string(),
                }
            })
            .collect()
    }

    /// The marks `field`'s row of the terminal band draws, left to right.
    fn band_marks(&self, field: &str) -> String {
        let now = self.run(&["now"]);
        let row = now
            .lines()
            .find(|l| l.starts_with(&format!("{field} ")))
            .unwrap_or_else(|| panic!("no band row for {field}:\n{now}"));
        row[field.len()..]
            .chars()
            .filter(|c| !"░▞█│ ".contains(*c))
            .collect()
    }
}

#[test]
fn field_glyph_draws_in_svg_and_band_and_no_glyph_draws_the_circle() {
    let env = Env::new();
    env.run(&["occur", "mood", "-t", "-30"]);
    env.run(&["occur", "mood", "-t", "-20"]);
    assert_eq!(env.svg_marks("mood"), ["●", "●"]);
    assert_eq!(env.band_marks("mood"), "●●");

    env.run(&["spec", "set", "mood", "--glyph", "▲"]);
    assert_eq!(env.svg_marks("mood"), ["▲", "▲"]);
    assert_eq!(env.band_marks("mood"), "▲▲");

    // An empty value clears it, back to the circle.
    env.run(&["spec", "set", "mood", "--glyph", ""]);
    assert_eq!(env.svg_marks("mood"), ["●", "●"]);
}

/// `switch FIELD STATE -m` writes a POINT carrying the state — the dot the
/// state override exists for.
#[test]
fn state_glyph_draws_on_a_moment_switch_dot_and_other_dots_draw_the_field_glyph() {
    let env = Env::new();
    env.run(&["switch", "mood", "happy", "-t", "-40"]);
    env.run(&["switch", "mood", "sad", "-m", "-t", "-30"]);
    env.run(&["switch", "mood", "calm", "-m", "-t", "-20"]);
    env.run(&["spec", "set", "mood", "--glyph", "▲"]);
    env.run(&["spec", "add-state", "mood", "sad", "-", "-", "▼"]);
    assert_eq!(env.svg_marks("mood"), ["▼", "▲"]);
    assert_eq!(env.band_marks("mood"), "▼▲");

    let shown = env.run(&["spec", "show", "mood", "--text"]);
    assert!(
        shown.contains("glyph: ▲") && shown.contains("(auto) ▼"),
        "{shown}"
    );
}

#[test]
fn glyph_by_maps_a_data_key_and_an_unmapped_value_draws_the_field_glyph() {
    let env = Env::new();
    env.run(&["occur", "coffee", "size=small", "-t", "-40"]);
    env.run(&["occur", "coffee", "size=medium", "-t", "-30"]);
    env.run(&["occur", "coffee", "-t", "-20"]);
    env.run(&[
        "spec",
        "set",
        "coffee",
        "--glyph-by",
        "size",
        "--glyph-map",
        "small=•",
    ]);
    // No field glyph: an unmapped value, and no value, draw the circle.
    assert_eq!(env.svg_marks("coffee"), ["•", "●", "●"]);
    env.run(&["spec", "set", "coffee", "--glyph", "◆"]);
    assert_eq!(env.svg_marks("coffee"), ["•", "◆", "◆"]);
    assert_eq!(env.band_marks("coffee"), "•◆◆");

    // `--glyph-map` alone adds to the map; `VALUE=` removes an entry.
    env.run(&["spec", "set", "coffee", "--glyph-map", "medium=▪"]);
    env.run(&["spec", "set", "coffee", "--glyph-map", "small="]);
    assert_eq!(env.svg_marks("coffee"), ["◆", "▪", "◆"]);
}

/// A two-column glyph draws as itself in the SVG and as `●` in the band,
/// which gives every slice of time one column (as `spec --help` says).
#[test]
fn a_double_width_glyph_draws_the_dot_in_the_band_so_no_column_moves() {
    let env = Env::new();
    env.run(&["occur", "coffee", "size=large", "-t", "-30"]);
    let plain = env.run(&["now"]);
    env.run(&[
        "spec",
        "set",
        "coffee",
        "--glyph-by",
        "size",
        "--glyph-map",
        "large=☕",
    ]);
    assert_eq!(env.svg_marks("coffee"), ["☕"]);
    assert_eq!(env.band_marks("coffee"), "●");
    assert_eq!(
        env.run(&["now"]),
        plain,
        "the band is unchanged, byte for byte"
    );
}

#[test]
fn spec_json_round_trips_the_glyph_keys() {
    let env = Env::new();
    env.run(&["occur", "coffee", "-t", "-30"]);
    env.run(&[
        "spec",
        "set",
        "coffee",
        "--glyph",
        "◆",
        "--state-glyph",
        "decaf=○",
        "--glyph-by",
        "size",
        "--glyph-map",
        "large=☕",
    ]);
    let json: serde_json::Value =
        serde_json::from_str(&env.run(&["spec", "show", "coffee"])).unwrap();
    assert_eq!(json["glyph"], "◆");
    assert_eq!(json["state_glyphs"]["decaf"], "○");
    assert_eq!(
        json["glyph_by"],
        serde_json::json!({"key": "size", "map": {"large": "☕"}})
    );

    env.run(&["spec", "set", "coffee", "--from-json", &json.to_string()]);
    let again: serde_json::Value =
        serde_json::from_str(&env.run(&["spec", "show", "coffee"])).unwrap();
    assert_eq!(again, json);
}

#[test]
fn a_glyph_is_one_printable_character() {
    let env = Env::new();
    for bad in ["ab", " ", "\u{301}"] {
        let err = env.fails(&["spec", "set", "mood", "--verb", "occur", "--glyph", bad]);
        assert!(
            err.contains("--glyph expects one printable character"),
            "{bad:?}: {err}"
        );
    }
    let err = env.fails(&[
        "spec",
        "set",
        "mood",
        "--verb",
        "occur",
        "--glyph-map",
        "a=b",
    ]);
    assert!(err.contains("--glyph-map needs --glyph-by"), "{err}");
}
