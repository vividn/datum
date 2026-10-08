//! Text-output goldens — snapshots of original-datum's `get`, `tail`,
//! `dayview`, and `nowview` for fixed inputs.
//!
//! All fixtures live in `goldens/text/fixtures.toml`; their expected
//! stdout lives next to them in `goldens/text/expected/<name>.txt`.
//! Splitting expected output into separate files lets a code review
//! diff text changes line-by-line instead of as embedded strings.
//!
//! ## Modes
//!
//! Without `parity-runtime`:
//! * `text_fixtures_present` confirms the bundle has ≥20 fixtures (the
//!   bead floor) and each parses.
//! * `text_fixtures_cover_each_surface` confirms ≥1 fixture per CLI
//!   surface (get/tail/dayview/nowview).
//! * `text_fixtures_have_expected_outputs` requires every fixture to
//!   have a *non-empty* `expected/<name>.txt`, unless it declares
//!   `uncapturable = "<why>"`. An empty expected file used to mean
//!   "quietly skip me", which is how 10 of 21 fixtures came to be
//!   uncompared while the suite reported green.
//!
//! * `daydumb_matches_v2_snapshots` runs each fixture through daydumb's
//!   own binary and compares what it draws to v2's expected file; where
//!   they differ, the difference must be the one recorded in
//!   `goldens/text/divergences.toml` (see `daydumb_tests`). Seconds.
//!
//! With `parity-runtime`:
//! * `text_goldens_match_live_datum` runs each fixture's setup + query
//!   against the pinned datum binary and compares stdout to its
//!   expected — the check that v2 still reproduces its snapshots. With
//!   `PARITY_REGEN=1`, mismatches *overwrite* the expected file instead
//!   of failing: the one way a snapshot is rewritten, from v2.

use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const FIXTURES_FILE: &str = "fixtures.toml";

#[derive(Debug, Deserialize)]
struct FixtureBundle {
    fixture: Vec<Fixture>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    /// Stable identifier, used as the `expected/<name>.txt` filename.
    /// Must be unique within the bundle and use only `[a-z0-9_]`.
    name: String,
    /// Short human-readable description; surfaced on test failure.
    description: String,
    /// CLI surface this fixture covers — pure metadata.
    surface: Surface,
    /// Commands to run before `query`. Each is a single arg vector
    /// passed straight to `datum`. Use these to seed entries.
    setup: Vec<Vec<String>>,
    /// The read command whose output we're pinning.
    query: Vec<String>,
    /// Set when this fixture's output *cannot* be pinned, with the
    /// reason. A fixture carrying this is excluded from the live
    /// comparison and its `expected/<name>.txt` may stay empty.
    ///
    /// This exists so that "not compared" is a written, reviewable
    /// claim rather than the default. It used to be the default: an
    /// empty expected file was skipped with an `eprintln`, which meant
    /// 10 of 21 fixtures were silently uncompared and nothing said so.
    /// A golden suite whose skip list is invisible reports the same
    /// green as one that checks everything.
    #[serde(default)]
    uncapturable: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum Surface {
    Get,
    Tail,
    Dayview,
    Nowview,
}

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("goldens")
        .join("text")
}

fn load_bundle() -> FixtureBundle {
    let path = fixtures_root().join(FIXTURES_FILE);
    let body = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    toml::from_str::<FixtureBundle>(&body)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn expected_path(name: &str) -> PathBuf {
    fixtures_root().join("expected").join(format!("{name}.txt"))
}

#[test]
fn text_fixtures_present() {
    let bundle = load_bundle();
    assert!(
        bundle.fixture.len() >= 20,
        "text golden fixtures must total ≥20 (bead acceptance); found {}",
        bundle.fixture.len()
    );
}

#[test]
fn text_fixtures_have_unique_names() {
    let bundle = load_bundle();
    let mut seen = std::collections::HashSet::new();
    for f in &bundle.fixture {
        assert!(
            f.name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "name {:?} must match [a-z0-9_]",
            f.name
        );
        assert!(seen.insert(&f.name), "duplicate fixture name {:?}", f.name);
    }
}

/// Every CLI surface has at least one fixture *declared*.
///
/// Declared, not compared: all four `nowview_*` fixtures are
/// `uncapturable` (nowview's SVG is a function of wall-clock now), so
/// nowview is the one surface with a fixture and no cross-runtime
/// assertion behind it. Tightening this to "compared" is da-t5f5 — it
/// needs a fixable clock in the datum runtime first, and a gate that is
/// red for a known reason on every run teaches people to ignore it.
#[test]
fn text_fixtures_cover_each_surface() {
    let bundle = load_bundle();
    for required in [
        Surface::Get,
        Surface::Tail,
        Surface::Dayview,
        Surface::Nowview,
    ] {
        let count = bundle
            .fixture
            .iter()
            .filter(|f| f.surface == required)
            .count();
        assert!(
            count >= 1,
            "must have ≥1 fixture for surface {:?}; got {}",
            required,
            count,
        );
    }
}

#[test]
fn text_fixtures_have_expected_outputs() {
    let bundle = load_bundle();
    for f in &bundle.fixture {
        let path = expected_path(&f.name);
        assert!(
            path.exists(),
            "fixture {}: expected output missing at {}\n\
             Create an empty placeholder, then `cargo test --features parity-runtime -- text_goldens_match_live_datum` with PARITY_REGEN=1 to populate.",
            f.name,
            path.display()
        );
        let empty = fs::metadata(&path).map(|m| m.len() == 0).unwrap_or(true);
        match (&f.uncapturable, empty) {
            (None, true) => panic!(
                "fixture {}: {} is empty, so the live comparison would skip it.\n\
                 Populate it (PARITY_REGEN=1 with --features parity-runtime), or, if its\n\
                 output genuinely can't be pinned, say so in fixtures.toml:\n\
                 \tuncapturable = \"<why>\"",
                f.name,
                path.display()
            ),
            (Some(reason), _) => assert!(
                !reason.trim().is_empty(),
                "fixture {}: uncapturable must state a reason",
                f.name
            ),
            (None, false) => {}
        }
    }
}

#[test]
fn text_fixtures_have_descriptions() {
    let bundle = load_bundle();
    for f in &bundle.fixture {
        assert!(
            !f.description.trim().is_empty(),
            "fixture {}: description must be non-empty",
            f.name,
        );
        assert!(
            !f.query.is_empty(),
            "fixture {}: query must be non-empty",
            f.name
        );
    }
}

// ─── daydumb against the v2 snapshots (normal suite) ──────────────────────

/// Every fixture run through **daydumb's** binary and compared to v2's
/// committed `expected/<name>.txt` (da-ma5pr). No node, no feature flag:
/// the snapshots are v2's raw output, so the reference costs a file read.
///
/// The comparison is of what the output DRAWS, not of its bytes: v2 resets
/// a colour with `\e[39m`/`\e[49m` where daydumb writes `\e[0m`, and v2's
/// SVG is pretty-printed nested `<svg>`s where daydumb's is one flat line.
/// So text becomes one line of characters and styles per row, and SVG one
/// line per drawn shape, at its absolute position in whole pixels.
///
/// Where daydumb differs from v2, the difference is listed in
/// `goldens/text/divergences.toml` — the exact diff, and the rules that
/// explain it — for vividn to walk. A fixture whose diff is the recorded one
/// passes; any other difference, or a recorded one that went away, fails.
/// The snapshot itself is never edited to match daydumb.
mod daydumb_tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::io::Write;
    use std::process::{Command, Stdio};

    const DIVERGENCES_FILE: &str = "divergences.toml";

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Divergences {
        rule: Vec<Rule>,
        fixture: Vec<Diverging>,
    }

    /// One reason daydumb's output differs from v2's.
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Rule {
        id: String,
        /// `open` until vividn decides; `accepted` is his word, never a can's.
        status: String,
        what: String,
        why: String,
    }

    /// A fixture whose daydumb output differs from v2's, by exactly
    /// `divergences/<name>.diff`.
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Diverging {
        name: String,
        rules: Vec<String>,
    }

    fn diff_path(name: &str) -> PathBuf {
        fixtures_root()
            .join("divergences")
            .join(format!("{name}.diff"))
    }

    fn load_divergences() -> Divergences {
        let path = fixtures_root().join(DIVERGENCES_FILE);
        let body =
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        toml::from_str(&body).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
    }

    /// Run daydumb's `datum` in `dir`, hermetically: nothing of the caller's
    /// environment reaches it but what a golden pins.
    fn daydumb(dir: &Path, args: &[String], stdin: &str) -> Result<String, String> {
        let mut child = Command::new(env!("CARGO_BIN_EXE_daydumb"))
            .args(args)
            .env_clear()
            .env("HOME", dir)
            .env("DATUM_DB", dir.join("parity.db"))
            .env("TZ", datum_parity::GOLDEN_TZ)
            // v2's snapshots are coloured; daydumb colours a pipe only when asked.
            .env("FORCE_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn: {e}"))?;
        let mut input = child.stdin.take().expect("piped stdin");
        input
            .write_all(stdin.as_bytes())
            .map_err(|e| format!("stdin: {e}"))?;
        drop(input);
        let out = child.wait_with_output().map_err(|e| format!("wait: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "datum {args:?} ({}):\n{}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn renders_svg(f: &Fixture) -> bool {
        matches!(f.surface, Surface::Dayview | Surface::Nowview)
    }

    /// A fixture's setup and query through daydumb, on a fresh database
    /// carrying v2's mySpec (compiled into v2; a kit here). Returns what the
    /// query printed, or the SVG it wrote to `OUTPUT_FILE`.
    fn run_fixture(f: &Fixture) -> Result<String, String> {
        let temp = tempfile::TempDir::new().map_err(|e| format!("tempdir: {e}"))?;
        let dir = temp.path();
        let install = ["kit", "install", "-", "--yes"].map(String::from);
        daydumb(dir, &install, &datum_parity::myspec::emit_kit_toml())?;
        for cmd in &f.setup {
            daydumb(dir, cmd, "")?;
        }
        let svg = dir.join("out.svg").display().to_string();
        let mut query: Vec<String> = f
            .query
            .iter()
            .map(|a| {
                if a == "OUTPUT_FILE" {
                    svg.clone()
                } else {
                    a.clone()
                }
            })
            .collect();
        if !renders_svg(f) {
            return daydumb(dir, &query, "");
        }
        if !f.query.iter().any(|a| a == "OUTPUT_FILE") {
            query.extend(["-o".to_string(), svg.clone()]);
        }
        daydumb(dir, &query, "")?;
        fs::read_to_string(&svg).map_err(|e| format!("read {svg}: {e}"))
    }

    /// daydumb's fresh hids as `<HID>`, which is how v2's snapshots carry
    /// theirs (the live test's `normalize`, at capture).
    fn mask_hids(f: &Fixture, lines: Vec<String>) -> Vec<String> {
        let exists = regex_lite::Regex::new(r"^(⟨fg=33⟩EXISTS⟨⟩ ⟨fg=33⟩)[a-z0-9]+").unwrap();
        // Looser than the live `normalize`'s, which also runs over SVG and
        // there took the year label for a hid: this one reads `tail` alone.
        let tail = regex_lite::Regex::new(r"(\s{2,})[a-z0-9]{4,}$").unwrap();
        lines
            .into_iter()
            .map(|l| {
                let l = exists.replace(&l, "${1}<HID>").into_owned();
                match f.surface {
                    Surface::Tail => tail.replace(&l, "${1}<HID>").into_owned(),
                    _ => l,
                }
            })
            .collect()
    }

    #[derive(Clone, Default, PartialEq)]
    struct Style {
        fg: Option<String>,
        bg: Option<String>,
        /// SGR attributes on: 1 bold, 2 dim, 3 italic, 4 underline, 7 inverse…
        attrs: BTreeSet<u16>,
    }

    impl Style {
        fn token(&self) -> String {
            let mut parts: Vec<String> = Vec::new();
            parts.extend(self.fg.iter().map(|c| format!("fg={c}")));
            parts.extend(self.bg.iter().map(|c| format!("bg={c}")));
            parts.extend(self.attrs.iter().map(|a| format!("sgr{a}")));
            format!("⟨{}⟩", parts.join(" "))
        }

        fn sgr(&mut self, params: &str) {
            let mut ps = params.split(';').map(|p| p.parse::<u16>().unwrap_or(0));
            while let Some(p) = ps.next() {
                match p {
                    0 => *self = Style::default(),
                    1..=9 => {
                        self.attrs.insert(p);
                    }
                    22 => {
                        self.attrs.remove(&1);
                        self.attrs.remove(&2);
                    }
                    23..=29 => {
                        self.attrs.remove(&(p - 20));
                    }
                    30..=37 | 90..=97 => self.fg = Some(p.to_string()),
                    40..=47 | 100..=107 => self.bg = Some(p.to_string()),
                    39 => self.fg = None,
                    49 => self.bg = None,
                    38 | 48 => {
                        let colour = match ps.next() {
                            Some(2) => {
                                let [r, g, b] = [ps.next(), ps.next(), ps.next()]
                                    .map(Option::unwrap_or_default);
                                format!("#{r:02x}{g:02x}{b:02x}")
                            }
                            Some(5) => format!("@{}", ps.next().unwrap_or_default()),
                            other => panic!("SGR {p};{other:?} in {params:?}"),
                        };
                        if p == 38 {
                            self.fg = Some(colour);
                        } else {
                            self.bg = Some(colour);
                        }
                    }
                    _ => panic!("unread SGR {p} in {params:?}"),
                }
            }
        }
    }

    /// One line of ANSI text as its characters, with `⟨…⟩` where the style
    /// changes. Trailing blanks that draw nothing are dropped.
    fn ansi_line(line: &str) -> String {
        let mut cells: Vec<(char, Style)> = Vec::new();
        let mut style = Style::default();
        let mut chars = line.chars();
        while let Some(c) = chars.next() {
            if c != '\x1b' {
                cells.push((c, style.clone()));
                continue;
            }
            assert_eq!(chars.next(), Some('['), "non-CSI escape in {line:?}");
            let mut params = String::new();
            let end = loop {
                match chars.next() {
                    Some(c) if c.is_ascii_digit() || c == ';' => params.push(c),
                    other => break other,
                }
            };
            assert_eq!(end, Some('m'), "only SGR escapes are read: {line:?}");
            style.sgr(&params);
        }
        while cells.last().is_some_and(|(c, s)| {
            *c == ' ' && s.bg.is_none() && !s.attrs.contains(&4) && !s.attrs.contains(&7)
        }) {
            cells.pop();
        }
        let mut out = String::new();
        let mut current = Style::default();
        for (c, s) in cells {
            if s != current {
                out.push_str(&s.token());
                current = s;
            }
            out.push(c);
        }
        if current != Style::default() {
            out.push_str("⟨⟩");
        }
        out
    }

    /// A shape an SVG draws: its geometry, absolute, and the rest of what
    /// it looks like (tag, field, paint).
    struct Shape {
        tag: String,
        geo: Vec<(&'static str, f64)>,
        rest: String,
    }

    impl Shape {
        /// The same shape, give or take a pixel: v2 nudges its axis by d3's
        /// half-pixel and rounds block widths, and neither is a difference
        /// anyone sees.
        fn matches(&self, other: &Shape) -> bool {
            self.tag == other.tag
                && self.rest == other.rest
                && self.geo.len() == other.geo.len()
                && self
                    .geo
                    .iter()
                    .zip(&other.geo)
                    .all(|((a, x), (b, y))| a == b && (x - y).abs() <= 1.0)
        }
    }

    impl std::fmt::Display for Shape {
        fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "{}", self.tag)?;
            for (k, v) in &self.geo {
                write!(f, " {k}={}", v.round() + 0.0)?; // `+ 0.0`: no -0
            }
            write!(f, "{}", self.rest)
        }
    }

    /// The shapes an SVG draws. Scripts, styles, tooltips, `data-*` other
    /// than the field, and shapes nothing can see are not read.
    fn svg_view(svg: &str) -> Vec<Shape> {
        // v2's snapshots went through the live test's `normalize`, whose
        // tail-hid pattern took the dayview's year label (`2024`, alone on
        // its pretty-printed line) for a hid. The text reads `<HID>`, which
        // is not XML; escaped, it parses and reads «<HID>».
        let svg = svg.replace("<HID>", "&lt;HID&gt;");
        let doc = roxmltree::Document::parse(&svg).unwrap_or_else(|e| panic!("parse SVG: {e}"));
        doc.descendants().filter_map(shape).collect()
    }

    fn shape(n: roxmltree::Node) -> Option<Shape> {
        let tag = n.tag_name().name();
        let fs = font_size(n);
        let (dx, dy) = n.ancestors().fold((0.0, 0.0), |(x, y), a| {
            let (tx, ty) = offset(a);
            (x + tx, y + ty)
        });
        let len = |name: &str| n.attribute(name).map_or(0.0, |v| length(v, fs));
        let mut rest = String::new();
        let mut geo = Vec::new();
        let mut at = |key: &'static str, name: &str, base: f64| match n.attribute(name) {
            Some(v) if v.ends_with('%') => rest.push_str(&format!(" {key}={v}")),
            _ => geo.push((key, base + len(name))),
        };
        match tag {
            "rect" => {
                at("x", "x", dx);
                at("y", "y", dy);
                at("w", "width", 0.0);
                at("h", "height", 0.0);
            }
            "circle" => {
                at("x", "cx", dx);
                at("y", "cy", dy);
                at("r", "r", 0.0);
            }
            "line" => {
                at("x1", "x1", dx);
                at("y1", "y1", dy);
                at("x2", "x2", dx);
                at("y2", "y2", dy);
            }
            "path" => {
                geo.extend([("x", dx), ("y", dy)]);
                rest.push_str(&format!(" d={}", n.attribute("d")?));
            }
            "text" => {
                at("x", "x", dx);
                at("y", "y", dy + len("dy"));
                rest.push_str(&format!(" {:?}", n.text().unwrap_or("").trim()));
            }
            _ => return None,
        }
        let paint = |p: &str| inherited(n, p).filter(|v| !p.ends_with("opacity") || *v != "1");
        let stroked = paint("stroke").is_some_and(|s| s != "none");
        if paint("opacity") == Some("0") || (paint("fill-opacity") == Some("0") && !stroked) {
            return None; // drawn, but nothing shows: a hit area
        }
        if let Some(field) = inherited(n, "data-field") {
            rest.push_str(&format!(" field={field}"));
        }
        for p in [
            "fill",
            "stroke",
            "stroke-width",
            "opacity",
            "fill-opacity",
            "stroke-opacity",
            "text-anchor",
        ] {
            let unseen = match p {
                "fill" => tag == "line", // a line has no inside
                "stroke-width" => !stroked,
                "text-anchor" => tag != "text",
                _ => false,
            };
            if let Some(v) = paint(p).filter(|_| !unseen) {
                rest.push_str(&format!(" {p}={v}"));
            }
        }
        if tag == "text" {
            rest.push_str(&format!(" font-size={fs}"));
        }
        Some(Shape {
            tag: tag.to_string(),
            geo,
            rest,
        })
    }

    fn inherited<'a>(n: roxmltree::Node<'a, 'a>, attr: &str) -> Option<&'a str> {
        n.ancestors().find_map(|a| a.attribute(attr))
    }

    /// The font size a node draws at: `style="font-size:…"` over the
    /// attribute, nearest ancestor first, SVG's 16 when nothing sets it.
    fn font_size(n: roxmltree::Node) -> f64 {
        n.ancestors()
            .find_map(|a| {
                let styled = a.attribute("style").and_then(|s| {
                    s.split(';')
                        .find_map(|d| d.trim().strip_prefix("font-size:"))
                });
                styled.or(a.attribute("font-size"))
            })
            .map_or(16.0, |v| length(v, 16.0))
    }

    fn length(v: &str, font_size: f64) -> f64 {
        let v = v.trim();
        let (num, scale) = match v.strip_suffix("em") {
            Some(em) => (em, font_size),
            None => (v.strip_suffix("px").unwrap_or(v), 1.0),
        };
        num.trim()
            .parse::<f64>()
            .unwrap_or_else(|e| panic!("length {v:?}: {e}"))
            * scale
    }

    /// The offset an element moves its content by: a nested `<svg>`'s
    /// `x`/`y`, any element's `translate(…)`. Anything that would scale or
    /// rotate is refused rather than misread.
    fn offset(a: roxmltree::Node) -> (f64, f64) {
        let num = |name: &str| a.attribute(name).map_or(0.0, |v| length(v, 16.0));
        let mut t = (0.0, 0.0);
        if a.has_tag_name("svg") {
            if let Some(vb) = a.attribute("viewBox") {
                let vb: Vec<f64> = vb.split_whitespace().map(|v| v.parse().unwrap()).collect();
                assert_eq!(
                    vb,
                    [0.0, 0.0, num("width"), num("height")],
                    "a scaling viewBox is not read"
                );
            }
            if a.parent_element().is_some() {
                t = (num("x"), num("y"));
            }
        }
        if let Some(tf) = a.attribute("transform") {
            let args = tf
                .trim()
                .strip_prefix("translate(")
                .and_then(|r| r.strip_suffix(')'))
                .unwrap_or_else(|| panic!("only translate() is read: {tf:?}"));
            let v: Vec<f64> = args
                .split([',', ' '])
                .filter(|s| !s.is_empty())
                .map(|s| s.parse().unwrap())
                .collect();
            t.0 += v[0];
            t.1 += v.get(1).copied().unwrap_or(0.0);
        }
        t
    }

    /// `-` for v2's row, `+` for daydumb's, row by row.
    fn text_diff(want: &[String], got: &[String]) -> String {
        let mut out = String::new();
        for i in 0..want.len().max(got.len()) {
            if want.get(i) != got.get(i) {
                want.get(i)
                    .iter()
                    .for_each(|w| out.push_str(&format!("-{w}\n")));
                got.get(i)
                    .iter()
                    .for_each(|g| out.push_str(&format!("+{g}\n")));
            }
        }
        out
    }

    /// `-` for a shape only v2 draws, `+` for one only daydumb draws, sorted:
    /// the two draw in a different order through a different tree.
    fn svg_diff(want: Vec<Shape>, mut got: Vec<Shape>) -> String {
        let mut lines = Vec::new();
        for w in want {
            match got.iter().position(|g| g.matches(&w)) {
                Some(i) => drop(got.remove(i)),
                None => lines.push(format!("-{w}")),
            }
        }
        lines.extend(got.iter().map(|g| format!("+{g}")));
        lines.sort_by(|a, b| a[1..].cmp(&b[1..]).then(a.cmp(b)));
        lines.iter().map(|l| format!("{l}\n")).collect()
    }

    /// What to record for `name`, once its rules are named.
    fn entry(name: &str, diff: &str) -> String {
        format!(
            "[[fixture]]\nname = \"{name}\"\nrules = []\n\nand in {}:\n{diff}",
            diff_path(name).display()
        )
    }

    #[test]
    fn daydumb_matches_v2_snapshots() {
        let bundle = load_bundle();
        let div = load_divergences();
        let mut failures = Vec::new();

        for r in &div.rule {
            if !matches!(r.status.as_str(), "open" | "accepted")
                || r.what.trim().is_empty()
                || r.why.trim().is_empty()
            {
                failures.push(format!(
                    "rule {}: needs status open|accepted, a what and a why",
                    r.id
                ));
            }
            if !div.fixture.iter().any(|d| d.rules.contains(&r.id)) {
                failures.push(format!("rule {} explains no fixture: delete it", r.id));
            }
        }
        let mut recorded: BTreeMap<&str, &Diverging> = BTreeMap::new();
        for d in &div.fixture {
            if d.rules.is_empty() {
                failures.push(format!("[{}] names no rule explaining it", d.name));
            }
            for id in d
                .rules
                .iter()
                .filter(|id| !div.rule.iter().any(|r| &r.id == *id))
            {
                failures.push(format!(
                    "[{}] names rule {id}, which is not defined",
                    d.name
                ));
            }
            if !bundle
                .fixture
                .iter()
                .any(|f| f.name == d.name && f.uncapturable.is_none())
            {
                failures.push(format!("[{}] is not a compared fixture", d.name));
            }
            if recorded.insert(&d.name, d).is_some() {
                failures.push(format!("[{}] is recorded twice", d.name));
            }
        }
        for file in fs::read_dir(fixtures_root().join("divergences")).unwrap() {
            let file = file.unwrap().file_name().to_string_lossy().into_owned();
            if !file
                .strip_suffix(".diff")
                .is_some_and(|n| recorded.contains_key(n))
            {
                failures.push(format!("divergences/{file} belongs to no recorded fixture"));
            }
        }

        let mut report = Vec::new();
        for f in &bundle.fixture {
            if let Some(why) = &f.uncapturable {
                report.push(format!("skipped  {}: {why}", f.name));
            }
        }
        let outputs: Vec<_> = std::thread::scope(|s| {
            let runs: Vec<_> = bundle
                .fixture
                .iter()
                .filter(|f| f.uncapturable.is_none())
                .map(|f| (f, s.spawn(|| run_fixture(f))))
                .collect();
            runs.into_iter()
                .map(|(f, run)| (f, run.join().unwrap()))
                .collect()
        });
        for (f, out) in outputs {
            let got = match out {
                Ok(out) => out,
                Err(e) => {
                    failures.push(format!("[{}] {e}", f.name));
                    continue;
                }
            };
            let want = fs::read_to_string(expected_path(&f.name)).unwrap();
            // Output the readers cannot parse fails this fixture, named,
            // and leaves the others' report standing.
            let delta = std::panic::catch_unwind(|| {
                if renders_svg(f) {
                    svg_diff(svg_view(&want), svg_view(&got))
                } else {
                    let lines = |s: &str| s.lines().map(ansi_line).collect::<Vec<_>>();
                    text_diff(&lines(&want), &mask_hids(f, lines(&got)))
                }
            });
            let delta = match delta {
                Ok(delta) => delta,
                Err(panic) => {
                    let why = panic
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or(panic.downcast_ref::<&str>().copied())
                        .unwrap_or("a panic");
                    failures.push(format!("[{}] output not readable: {why}", f.name));
                    continue;
                }
            };
            let was = recorded.get(f.name.as_str()).map(|d| {
                let path = diff_path(&f.name);
                let was = fs::read_to_string(&path)
                    .unwrap_or_else(|e| format!("({} unreadable: {e})\n", path.display()));
                (d, was)
            });
            match was {
                None if delta.is_empty() => report.push(format!("matches  {}", f.name)),
                Some((d, was)) if was == delta && !delta.is_empty() => {
                    report.push(format!("diverges {}: {}", f.name, d.rules.join(", ")))
                }
                None => failures.push(format!(
                    "[{}] differs from v2 and is not in {DIVERGENCES_FILE}. Fix daydumb, or \
                     record the difference for vividn's walk; never edit the snapshot.\n{}",
                    f.name,
                    entry(&f.name, &delta)
                )),
                Some(_) if delta.is_empty() => failures.push(format!(
                    "[{}] matches v2 now: delete its entry from {DIVERGENCES_FILE} and its .diff",
                    f.name
                )),
                Some((_, was)) => failures.push(format!(
                    "[{}] differs from v2, but not as recorded.\nrecorded:\n{was}\nnow:\n{}",
                    f.name,
                    entry(&f.name, &delta)
                )),
            }
        }

        report.sort();
        let rules: Vec<String> = div
            .rule
            .iter()
            .map(|r| format!("{} [{}]: {}", r.id, r.status, r.what.trim()))
            .collect();
        eprintln!(
            "daydumb against v2's snapshots:\n  {}\n\ndivergence rules ({DIVERGENCES_FILE}):\n  {}",
            report.join("\n  "),
            rules.join("\n  ")
        );
        assert!(
            failures.is_empty(),
            "{} failure(s):\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}

// ─── Live runtime tests (gated) ──────────────────────────────────────────

#[cfg(feature = "parity-runtime")]
mod runtime_tests {
    use super::*;
    use datum_parity::runtime::DatumRuntime;
    use tempfile::TempDir;

    /// Normalize datum output so deterministic-input → deterministic-output.
    ///
    /// Datum bakes wall-clock timestamps and random hids into its
    /// stdout. Each run produces fresh values; without scrubbing, no two
    /// runs of a fixture produce identical output.
    ///
    /// What we scrub:
    /// * `CREATE <hid> <slug>` log lines — dropped entirely.
    /// * `EXISTS \x1b[m<hid>\x1b[m` headers in `get` output — replaced
    ///   with `EXISTS <HID>`.
    /// * Trailing `<hid>` in `tail` rows (the rightmost column) —
    ///   replaced with `<HID>`.
    ///
    /// What we don't scrub: the visible-text rendering (the timestamp
    /// derived from the entry's pinned `-d/-t` flags, field/state
    /// names, ANSI color codes). Those ARE the parity surface.
    ///
    /// SVG output (nowview/dayview) doesn't carry hids in its content,
    /// but the normalizer still runs over it harmlessly.
    fn normalize(raw: &str) -> String {
        let create_drop = regex_lite::Regex::new(r"^CREATE \S+ \S+$").unwrap();
        let exists_line = regex_lite::Regex::new(
            r"^\x1b\[33mEXISTS\x1b\[39m\s+\x1b\[33m[a-z0-9]+\x1b\[39m(?P<rest>.*)$",
        )
        .unwrap();
        // Tail rows end in `<spaces><hid>` (5 lowercase-alphanumeric).
        // Anchored to end-of-line. Avoid eating dates like "2024" by
        // requiring the column to be lowercase letters at least once.
        let tail_hid =
            regex_lite::Regex::new(r"(?P<lead>\s{2,})(?:[a-z][a-z0-9]{4}|[a-z0-9]{4,6})$").unwrap();

        let mut lines: Vec<String> = Vec::new();
        for line in raw.lines() {
            if create_drop.is_match(line) {
                continue;
            }
            let mut out = line.to_owned();
            out = exists_line
                .replace_all(&out, "\x1b[33mEXISTS\x1b[39m \x1b[33m<HID>\x1b[39m$rest")
                .into_owned();
            out = tail_hid.replace_all(&out, "${lead}<HID>").into_owned();
            lines.push(out);
        }

        let mut joined = lines.join("\n");
        if !joined.is_empty() {
            joined.push('\n');
        }
        joined
    }

    /// Run one fixture under a fresh PouchDB. Returns `Ok(captured)` on
    /// success or `Err(message)` describing the failure. Failures are
    /// accumulated by the caller so one bad fixture doesn't block the
    /// whole batch (especially under PARITY_REGEN=1).
    ///
    /// `nowview` / `dayview` render to a file (datum doesn't expose an
    /// stdout option for SVG); for those surfaces the harness substitutes
    /// `OUTPUT_FILE` in the query with a temp path, runs the command,
    /// and reads the file back as the captured output. SVG bytes are
    /// the text we pin.
    fn run_fixture(runtime: &DatumRuntime, f: &Fixture) -> Result<String, String> {
        let temp = TempDir::new().map_err(|e| format!("tempdir: {e}"))?;
        let db = format!("parity_{}", f.name);

        runtime
            .setup_db(temp.path(), &db)
            .map_err(|e| format!("setup_db: {e}"))?;

        for cmd in &f.setup {
            let mut full: Vec<&str> = vec!["--db", &db];
            full.extend(cmd.iter().map(|s| s.as_str()));
            runtime
                .run(temp.path(), &full)
                .map_err(|e| format!("setup {cmd:?}: {e}"))?;
        }

        let needs_file = matches!(f.surface, Surface::Nowview | Surface::Dayview);
        if needs_file {
            let out_file = temp.path().join(format!("{}.svg", f.name));
            let out_str = out_file.to_string_lossy().into_owned();
            let mut q_full: Vec<String> = vec!["--db".into(), db.clone()];
            for arg in &f.query {
                if arg == "OUTPUT_FILE" {
                    q_full.push(out_str.clone());
                } else {
                    q_full.push(arg.clone());
                }
            }
            // If the fixture didn't include OUTPUT_FILE, append `-o file`
            // so we always have somewhere for the renderer to write.
            if !f.query.iter().any(|a| a == "OUTPUT_FILE") {
                q_full.push("-o".into());
                q_full.push(out_str.clone());
            }
            let q_refs: Vec<&str> = q_full.iter().map(|s| s.as_str()).collect();
            runtime
                .run(temp.path(), &q_refs)
                .map_err(|e| format!("query {:?}: {e}", f.query))?;
            let svg = fs::read_to_string(&out_file)
                .map_err(|e| format!("read svg {}: {e}", out_file.display()))?;
            return Ok(normalize(&svg));
        }

        let mut q_full: Vec<&str> = vec!["--db", &db];
        q_full.extend(f.query.iter().map(|s| s.as_str()));
        let raw = runtime
            .run_stdout(temp.path(), &q_full)
            .map_err(|e| format!("query {:?}: {e}", f.query))?;
        Ok(normalize(&raw))
    }

    /// Whether `PARITY_REGEN` asks for the goldens to be rewritten.
    ///
    /// Presence alone is the wrong question: `PARITY_REGEN=0` reads as
    /// "off" to anyone typing it, and this test *overwrites the
    /// reference* when it is on. An off-switch that turns the thing on
    /// is the one mistake a golden harness cannot afford.
    fn regen_requested() -> bool {
        std::env::var("PARITY_REGEN")
            .is_ok_and(|v| !matches!(v.trim(), "" | "0" | "false" | "no" | "off"))
    }

    #[test]
    fn text_goldens_match_live_datum() {
        let runtime = DatumRuntime::new().unwrap_or_else(|e| panic!("DatumRuntime::new: {e}"));
        let bundle = load_bundle();
        let regen = regen_requested();

        let mut failures = Vec::new();
        for f in &bundle.fixture {
            if let Some(reason) = &f.uncapturable {
                eprintln!("[{}] not compared: {reason}", f.name);
                continue;
            }
            let got = match run_fixture(&runtime, f) {
                Ok(s) => s,
                Err(msg) => {
                    failures.push(format!("[{}] {msg}", f.name));
                    continue;
                }
            };

            let exp_path = expected_path(&f.name);
            if regen {
                if let Err(e) = fs::write(&exp_path, &got) {
                    failures.push(format!("[{}] write expected: {e}", f.name));
                } else {
                    eprintln!("[{}] regenerated {}", f.name, exp_path.display());
                }
                continue;
            }

            let want = match fs::read_to_string(&exp_path) {
                Ok(s) => s,
                Err(e) => {
                    failures.push(format!("[{}] read expected: {e}", f.name));
                    continue;
                }
            };

            // An empty expected file used to mean "skip me", which made
            // the suite green over fixtures it never looked at. A
            // fixture that can't be compared must say so out loud via
            // `uncapturable`; anything else with an empty expected is a
            // failure, not a shrug.
            if want.is_empty() {
                failures.push(format!(
                    "[{}] expected.txt is empty and the fixture is not marked \
                     `uncapturable`. Populate it with PARITY_REGEN=1, or declare \
                     why it can't be pinned.",
                    f.name
                ));
                continue;
            }

            if got != want {
                failures.push(format!(
                    "[{}] {}\n--- got ---\n{got}--- want ---\n{want}",
                    f.name, f.description
                ));
            }
        }

        if !failures.is_empty() {
            panic!(
                "{} fixture(s) failed:\n{}\n\n\
                 Run with PARITY_REGEN=1 to update expected outputs after a setup-cmd fix.",
                failures.len(),
                failures.join("\n\n")
            );
        }
    }
}
