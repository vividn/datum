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
            .env("NO_COLOR", "1")
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
    /// first, as the mark it draws: its glyph, or `●` for the circle.
    /// Two days, because the entries are minutes old and just after midnight
    /// some of them are yesterday's.
    ///
    /// da-qietq: every dot is its symbol alone — the disc under it stays
    /// the mark but is never painted, and a plain dot's circle is the
    /// disc's full size.
    fn svg_marks(&self, field: &str) -> Vec<String> {
        let svg = self.run(&["dayview", "-n", "2"]);
        let attr = format!(r#"data-field="{field}""#);
        let r = |tag: &str| {
            tag.split(r#" r=""#)
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_string()
        };
        svg.split(r#"<circle class="point-bg""#)
            .skip(1)
            .filter(|dot| dot.split('>').next().unwrap().contains(&attr))
            .map(|dot| {
                let disc = dot.split('>').next().unwrap();
                assert!(
                    disc.contains(r#"fill-opacity="0""#),
                    "a painted disc: {disc}"
                );
                // The split cuts each piece at the next dot's disc, so a
                // piece holds this dot's own symbol and no other.
                assert_eq!(dot.matches(r#"class="point-fg""#).count(), 1, "{dot}");
                let fg = dot.split(r#"class="point-fg""#).nth(1).unwrap();
                if dot.contains(r#"<circle class="point-fg""#) {
                    assert_eq!(r(fg), r(disc), "a shrunken dot: {dot}");
                }
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
        band_row(&self.run(&["now"]), field)
            .chars()
            .filter(|c| !"░▞█│ ".contains(*c))
            .collect()
    }
}

/// `field`'s row of the terminal band in `now`, after its name column.
fn band_row<'a>(now: &'a str, field: &str) -> &'a str {
    now.lines()
        .find_map(|l| l.strip_prefix(&format!("{field} ")))
        .unwrap_or_else(|| panic!("no band row for {field}:\n{now}"))
        .trim_start_matches(' ')
}

#[test]
fn field_glyph_draws_in_svg_and_band_and_no_glyph_draws_the_circle() {
    let env = Env::new();
    env.run(&["occur", "mood", "-t", "-30"]);
    env.run(&["occur", "mood", "-t", "-20"]);
    assert_eq!(env.svg_marks("mood"), ["●", "●"]);
    assert_eq!(env.band_marks("mood"), "●●");

    env.run(&["setup", "mood", "--glyph", "▲"]);
    assert_eq!(env.svg_marks("mood"), ["▲", "▲"]);
    assert_eq!(env.band_marks("mood"), "▲▲");

    // An empty value clears it, back to the circle.
    env.run(&["setup", "mood", "--glyph", ""]);
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
    env.run(&["setup", "mood", "--glyph", "▲"]);
    env.run(&["setup", "mood", "--states", "sad", "--state-glyph", "sad=▼"]);
    assert_eq!(env.svg_marks("mood"), ["▼", "▲"]);
    assert_eq!(env.band_marks("mood"), "▼▲");

    let shown = env
        .run(&["setup", "mood"])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        shown.contains("glyph ▲") && shown.contains("state sad ▼"),
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
        "setup",
        "coffee",
        "--glyph-by",
        "size",
        "--glyph-map",
        "small=•",
    ]);
    // No field glyph: an unmapped value, and no value, draw the circle.
    assert_eq!(env.svg_marks("coffee"), ["•", "●", "●"]);
    env.run(&["setup", "coffee", "--glyph", "◆"]);
    assert_eq!(env.svg_marks("coffee"), ["•", "◆", "◆"]);
    assert_eq!(env.band_marks("coffee"), "•◆◆");

    // `--glyph-map` alone adds to the map; `VALUE=` removes an entry.
    env.run(&["setup", "coffee", "--glyph-map", "medium=▪"]);
    env.run(&["setup", "coffee", "--glyph-map", "small="]);
    assert_eq!(env.svg_marks("coffee"), ["◆", "▪", "◆"]);
}

/// A two-column glyph draws as itself in the SVG and as `●` in the band,
/// which gives every slice of time one column (as `setup --help` says).
///
/// Its row is compared with a glyphless field's at the same instant in ONE
/// render: the band is anchored to the real clock, so two renders a moment
/// apart can differ in the axis label and the dot's column (da-idb2f).
#[test]
fn a_double_width_glyph_draws_the_dot_in_the_band_so_no_column_moves() {
    let env = Env::new();
    let at = (chrono::Utc::now() - chrono::Duration::minutes(30)).to_rfc3339();
    env.run(&["occur", "coffee", "size=large", "-t", &at]);
    env.run(&["occur", "tea", "size=large", "-t", &at]);
    env.run(&[
        "setup",
        "coffee",
        "--glyph-by",
        "size",
        "--glyph-map",
        "large=☕",
    ]);
    assert_eq!(env.svg_marks("coffee"), ["☕"]);
    assert_eq!(env.band_marks("coffee"), "●");
    let now = env.run(&["now"]);
    assert_eq!(
        band_row(&now, "coffee"),
        band_row(&now, "tea"),
        "the glyphless row, byte for byte"
    );
}

#[test]
fn spec_json_round_trips_the_glyph_keys() {
    let env = Env::new();
    env.run(&["occur", "coffee", "-t", "-30"]);
    env.run(&[
        "setup",
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
        serde_json::from_str(&env.run(&["setup", "coffee", "--json"])).unwrap();
    assert_eq!(json["glyph"], "◆");
    assert_eq!(json["state_glyphs"]["decaf"], "○");
    assert_eq!(
        json["glyph_by"],
        serde_json::json!({"key": "size", "map": {"large": "☕"}})
    );

    env.run(&["setup", "coffee", "--from-json", &json.to_string()]);
    let again: serde_json::Value =
        serde_json::from_str(&env.run(&["setup", "coffee", "--json"])).unwrap();
    assert_eq!(again, json);
}

#[test]
fn a_glyph_is_one_printable_character() {
    let env = Env::new();
    for bad in ["ab", " ", "\u{301}"] {
        let err = env.fails(&["setup", "mood", "--verb", "occur", "--glyph", bad]);
        assert!(
            err.contains("--glyph expects one printable character"),
            "{bad:?}: {err}"
        );
    }
    let err = env.fails(&["setup", "mood", "--verb", "occur", "--glyph-map", "a=b"]);
    assert!(err.contains("--glyph-map needs --glyph-by"), "{err}");
}

/// da-c05v1: `--glyph-scale` multiplies a dot's radius and `--glyph-aspect`
/// stretches its mark along x about its centre; 1 is stored as unset, and
/// a block's picture does not move for either.
#[test]
fn glyph_scale_and_aspect_size_a_dot_and_leave_blocks_alone() {
    let env = Env::new();
    env.run(&["occur", "coffee", "-t", "-30"]);
    env.run(&["start", "work", "-t", "-60"]);
    env.run(&["stop", "work", "-t", "-10"]);
    let radii = || -> Vec<f64> {
        let layout: serde_json::Value =
            serde_json::from_str(&env.run(&["dayview", "-n", "2", "--show", "json"])).unwrap();
        layout["days"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|d| d["fields"].as_array().unwrap().clone())
            .filter(|f| f["field"] == "coffee")
            .flat_map(|f| f["points"].as_array().unwrap().clone())
            .map(|p| p["r"].as_f64().unwrap())
            .collect()
    };
    let work = || {
        let svg = env.run(&["dayview", "-n", "2"]);
        let group = svg.split(r#"data-field="work""#).nth(1).unwrap();
        group.split("</g>").next().unwrap().to_string()
    };
    let (r, blocks) = (radii(), work());
    assert_eq!(r.len(), 1);
    assert!(!env
        .run(&["dayview", "-n", "2"])
        .contains("transform=\"matrix("));

    env.run(&[
        "setup",
        "coffee",
        "--glyph-scale",
        "2",
        "--glyph-aspect",
        "1.5",
    ]);
    env.run(&["setup", "work", "--glyph-scale", "3", "--glyph-aspect", "2"]);
    let json: serde_json::Value =
        serde_json::from_str(&env.run(&["setup", "coffee", "--json"])).unwrap();
    assert_eq!(
        (json["glyph_scale"].as_f64(), json["glyph_aspect"].as_f64()),
        (Some(2.0), Some(1.5))
    );
    let text = env
        .run(&["setup", "coffee"])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        text.contains("glyph scale ×2") && text.contains("glyph aspect 1.5"),
        "{text}"
    );

    assert_eq!(radii(), [r[0] * 2.0]);
    let svg = env.run(&["dayview", "-n", "2"]);
    let fg = svg.split(r#"<circle class="point-fg""#).nth(1).unwrap();
    let cx: f64 = fg
        .split(r#"cx=""#)
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        fg.split('>')
            .next()
            .unwrap()
            .contains(&format!(r#"transform="matrix(1.5 0 0 1 {} 0)""#, cx * -0.5)),
        "stretched about its centre: {fg}"
    );
    assert_eq!(work(), blocks, "a block ignores both");

    env.run(&[
        "setup",
        "coffee",
        "--glyph-scale",
        "1",
        "--glyph-aspect",
        "1",
    ]);
    let json: serde_json::Value =
        serde_json::from_str(&env.run(&["setup", "coffee", "--json"])).unwrap();
    assert!(
        json.get("glyph_scale").is_none() && json.get("glyph_aspect").is_none(),
        "{json}"
    );
    assert_eq!(radii(), r);

    for bad in ["0", "-1", "x"] {
        let err = env.fails(&["setup", "coffee", "--glyph-scale", bad]);
        assert!(
            err.contains("--glyph-scale expects a number"),
            "{bad}: {err}"
        );
    }
    json_refuses(
        &env,
        r#"{"field":"coffee","verb":"occur","glyph_aspect":-2}"#,
    );
}

fn json_refuses(env: &Env, json: &str) {
    let err = env.fails(&["setup", "coffee", "--from-json", json]);
    assert!(
        err.contains("glyph_aspect must be a number above 0"),
        "{err}"
    );
}

/// da-c05v1 review: `--from-json` stores a factor of 1 as unset, as the
/// flag does.
#[test]
fn a_glyph_factor_of_one_from_json_is_stored_unset() {
    let env = Env::new();
    env.run(&["occur", "coffee", "-t", "-30"]);
    env.run(&[
        "setup",
        "coffee",
        "--from-json",
        r#"{"field":"coffee","verb":"occur","glyph_scale":1,"glyph_aspect":1}"#,
    ]);
    let json: serde_json::Value =
        serde_json::from_str(&env.run(&["setup", "coffee", "--json"])).unwrap();
    assert!(
        json.get("glyph_scale").is_none() && json.get("glyph_aspect").is_none(),
        "{json}"
    );
}

/// da-ijjjn: a dot is one standard size, two thirds of the height of a
/// lane with no `--height` (0.08 of the day, da-sjhuj, da-a87dh), however tall or thin its own lane — a thin
/// lane's dot passes its edges — and `--glyph-scale` multiplies it: a
/// fraction of the day and not pixels (da-c1b6m), the same in the dayview
/// and the nowview, at a short band and a tall one.
#[test]
fn a_dot_is_the_standard_size_whatever_its_lane_and_scaled() {
    let env = Env::new();
    let at = "2026-04-27T12:00+00:00";
    for f in ["tall", "plain", "thin", "double"] {
        env.run(&["occur", f, "-t", at]);
    }
    env.run(&["setup", "tall", "--y", "0.5", "--height", "0.4"]);
    env.run(&["setup", "thin", "--y", "0.1", "--height", "0.05"]);
    env.run(&["setup", "double", "--glyph-scale", "2"]);
    let tall = |svg: &str, field: &str| -> f64 {
        let group = svg
            .split(&format!(r#"<g class="field" data-field="{field}">"#))
            .nth(1)
            .unwrap_or_else(|| panic!("no {field} in {svg}"));
        let fg = group.split(r#"<circle class="point-fg""#).nth(1).unwrap();
        let r = fg.split(r#" r=""#).nth(1).unwrap();
        2.0 * r.split('"').next().unwrap().parse::<f64>().unwrap()
    };
    let bare = ["--margin", "0", "--time-axis-height", "0"];
    for h in ["60", "200"] {
        let dayview = env.run(
            &[
                &["dayview", "--width", "300", "--day-height", h],
                &bare[..],
                &["--label-width", "0", "--end-date", "2026-04-27"],
            ]
            .concat(),
        );
        let nowview = env.run(
            &[
                &["nowview", "--height", h],
                &bare[..],
                &["--context", "1h", "-t", at, "-o", "-"],
            ]
            .concat(),
        );
        let h: f64 = h.parse().unwrap();
        for (view, svg) in [("dayview", &dayview), ("nowview", &nowview)] {
            for (field, frac) in [
                ("plain", 0.08),
                ("tall", 0.08),
                ("thin", 0.08),
                ("double", 0.16),
            ] {
                let got = tall(svg, field);
                assert!(
                    (got - frac * h).abs() < 0.5,
                    "{view} at {h}px draws {field}'s dot {got}px tall, not {frac} of the band"
                );
            }
        }
    }
}

/// da-lqo7x: a state change at an instant (`switch … -m`) in a tall lane
/// is a dotted line the lane's height, in the dayview and the nowview, its
/// mark titled as a dot's is; an occur in the same picture is still a dot.
#[test]
fn a_moment_is_a_dotted_line_its_lane_tall_and_an_occur_is_a_dot() {
    let env = Env::new();
    let at = "2026-04-27T12:00+00:00";
    env.run(&["switch", "body", "run", "-m", "distanceKm=5", "-t", at]);
    env.run(&["occur", "coffee", "-t", at]);
    env.run(&["setup", "body", "--y", "0.6", "--height", "0.4"]);
    let bare = ["--margin", "0", "--time-axis-height", "0"];
    let dayview = env.run(
        &[
            &["dayview", "--width", "300", "--day-height", "200"],
            &bare[..],
            &["--label-width", "0", "--end-date", "2026-04-27"],
        ]
        .concat(),
    );
    let nowview = env.run(
        &[
            &["nowview", "--height", "200"],
            &bare[..],
            &["--context", "1h", "-t", at, "-o", "-"],
        ]
        .concat(),
    );
    let num = |tag: &str, k: &str| -> f64 {
        let v = tag.split(&format!(r#" {k}=""#)).nth(1).unwrap();
        v[..v.find('"').unwrap()].parse().unwrap()
    };
    for (view, svg) in [("dayview", &dayview), ("nowview", &nowview)] {
        let body = svg
            .split(r#"<g class="field" data-field="body">"#)
            .nth(1)
            .unwrap_or_else(|| panic!("{view}: no body lane\n{svg}"));
        let body = &body[..body.find("</g>").unwrap()];
        assert!(!body.contains("<circle"), "{view}: {body}");
        assert!(body.contains("<title>body: run</title>"), "{view}: {body}");
        let line = body
            .split(r#"<line class="point-fg moment""#)
            .nth(1)
            .unwrap_or_else(|| panic!("{view}: no dotted line\n{body}"));
        assert!(line.contains("stroke-dasharray"), "{view}: {line}");
        let tall = (num(line, "y2") - num(line, "y1")).abs();
        assert!(
            (tall - 80.0).abs() < 1.0,
            "{view}: the line is {tall}px, not 80"
        );

        let coffee = svg
            .split(r#"<g class="field" data-field="coffee">"#)
            .nth(1)
            .unwrap_or_else(|| panic!("{view}: no coffee lane\n{svg}"));
        let coffee = &coffee[..coffee.find("</g>").unwrap()];
        assert!(
            coffee.contains(r#"<circle class="point-fg""#),
            "{view}: {coffee}"
        );
        assert!(!coffee.contains("moment"), "{view}: {coffee}");
    }
}

/// da-2028s: a moment is a dotted line where the state it records is
/// active, `true` or a named state, and a dot, as an occur is, where it is
/// not: `false`, TRACKED or IGNORED.
#[test]
fn a_moment_of_an_inactive_state_is_a_dot() {
    let env = Env::new();
    let at = "2026-04-27T12:00+00:00";
    for (field, state) in [
        ("on", "true"),
        ("named", "run"),
        ("off", "false"),
        ("tracked", "TRACKED"),
        ("ignored", "IGNORED"),
    ] {
        env.run(&["switch", field, state, "-m", "-t", at]);
    }
    env.run(&["occur", "coffee", "-t", at]);
    let svg = env.run(&[
        "dayview",
        "--width",
        "300",
        "--day-height",
        "200",
        "--end-date",
        "2026-04-27",
    ]);
    for (field, moment) in [
        ("on", true),
        ("named", true),
        ("off", false),
        ("tracked", false),
        ("ignored", false),
        ("coffee", false),
    ] {
        let lane = svg
            .split(&format!(r#"<g class="field" data-field="{field}">"#))
            .nth(1)
            .unwrap_or_else(|| panic!("no {field} lane\n{svg}"));
        let lane = &lane[..lane.find("</g>").unwrap()];
        assert_eq!(
            lane.contains(r#"<line class="point-fg moment""#),
            moment,
            "{field}: {lane}"
        );
        assert_eq!(
            lane.contains(r#"<circle class="point-fg""#),
            !moment,
            "{field}: {lane}"
        );
    }
}
