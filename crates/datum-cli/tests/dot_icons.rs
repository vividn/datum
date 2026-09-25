//! A field's custom SVG ICON (da-45wgz): drawn in the dayview SVG, stored
//! there once and `<use>`d per dot, while `datum now`'s terminal band — text,
//! which cannot draw an image — draws the field's glyph, the standard dot
//! unless one is named.
//!
//! Runs the real binary so both surfaces are read off stdout, as a person
//! reads them.

use std::process::{Command, Output};

const BEANS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><ellipse cx="12" cy="12" rx="6" ry="9" fill="currentColor"/></svg>"#;

struct Env {
    tmp: tempfile::TempDir,
}

impl Env {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().unwrap(),
        }
    }

    fn output(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", self.tmp.path().join("i.db"))
            .env("HOME", self.tmp.path())
            .env("DATUM_HOME", self.tmp.path())
            .env("DATUM_CONFIG_HOME", self.tmp.path())
            .env("DATUM_COLOR", "0")
            .args(args)
            .output()
            .unwrap()
    }

    fn run(&self, args: &[&str]) -> String {
        let out = self.output(args);
        assert!(
            out.status.success(),
            "datum {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn fails(&self, args: &[&str]) -> String {
        let out = self.output(args);
        assert!(!out.status.success(), "datum {args:?} should fail");
        String::from_utf8_lossy(&out.stderr).into_owned()
    }

    /// Write `svg` to a file in the sandbox and return its path.
    fn file(&self, name: &str, svg: &str) -> String {
        let path = self.tmp.path().join(name);
        std::fs::write(&path, svg).unwrap();
        path.to_str().unwrap().to_string()
    }

    /// Two days, because the entries are minutes old and just after
    /// midnight some of them are yesterday's. The now-line is left out: it
    /// moves between two renders, and nothing else does.
    fn dayview(&self) -> String {
        let svg = self.run(&["dayview", "-n", "2"]);
        let (head, tail) = svg
            .split_once(r#"<line class="current-time-line""#)
            .expect("a now-line");
        format!("{head}{}", tail.split_once("/>").unwrap().1)
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

/// Acceptance 1, 2 and 4: an icon given and nothing else saves; its dots
/// draw it in the SVG from ONE `<defs>` copy, and draw the standard dot in
/// the band. A named glyph is then the band's fallback, and not the SVG's.
#[test]
fn an_icon_draws_once_in_defs_per_dot_in_the_svg_and_the_band_draws_its_fallback() {
    let env = Env::new();
    for t in ["-30", "-25", "-20"] {
        env.run(&["occur", "coffee", "-t", t]);
    }
    let plain_band = env.band_marks("coffee");
    assert_eq!(plain_band, "●●●");

    let icon = env.file("beans.svg", BEANS);
    env.run(&["spec", "set", "coffee", "--icon", &icon]);
    let svg = env.dayview();
    assert_eq!(
        svg.matches(r#"<svg id="icon-coffee""#).count(),
        1,
        "one copy of the artwork:\n{svg}"
    );
    assert_eq!(svg.matches(r##"href="#icon-coffee""##).count(), 3);
    assert_eq!(svg.matches("<ellipse").count(), 1);
    assert_eq!(env.band_marks("coffee"), plain_band, "no fallback named: ●");

    env.run(&["spec", "set", "coffee", "--glyph", "▲"]);
    assert_eq!(env.band_marks("coffee"), "▲▲▲");
    let svg = env.dayview();
    assert_eq!(svg.matches(r##"href="#icon-coffee""##).count(), 3);
    assert!(
        !svg.contains('▲'),
        "the SVG draws the icon, not the fallback"
    );
}

/// A state's own glyph is more specific than the field's icon, and wins.
#[test]
fn a_state_glyph_wins_over_the_icon() {
    let env = Env::new();
    env.run(&["switch", "mood", "happy", "-t", "-40"]);
    env.run(&["switch", "mood", "sad", "-m", "-t", "-30"]);
    env.run(&["switch", "mood", "calm", "-m", "-t", "-20"]);
    let icon = env.file("m.svg", BEANS);
    env.run(&["spec", "set", "mood", "--icon", &icon]);
    env.run(&["spec", "add-state", "mood", "sad", "-", "-", "▼"]);
    let svg = env.dayview();
    assert_eq!(svg.matches(r##"href="#icon-mood""##).count(), 1);
    assert!(svg.contains(">▼</text>"), "{svg}");
}

/// Acceptance 3: an oversize or non-SVG file is refused, naming the limit,
/// and so is anything that would run or restyle the page it is drawn in.
#[test]
fn an_oversize_non_svg_or_unsafe_icon_is_refused() {
    let env = Env::new();
    env.run(&["occur", "coffee", "-t", "-30"]);
    let refused = |name: &str, svg: &str| {
        let path = env.file(name, svg);
        env.fails(&["spec", "set", "coffee", "--icon", &path])
    };

    let big = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><path d="{}"/></svg>"#,
        "M0 0".repeat(1200)
    );
    assert!(
        refused("big.svg", &big).contains("an icon may be at most 4 kB"),
        "the limit is named"
    );
    let png = refused("dot.png", "\u{89}PNG\r\n\u{1a}\n");
    assert!(png.contains("an icon must be an .svg file"), "{png}");
    assert!(refused("x.svg", "<html><body/></html>").contains("an icon must be an .svg file"));
    assert!(refused("x.svg", "<svg viewBox='0 0 1 1'>").contains("an icon must be an .svg file"));
    assert!(refused("x.svg", "<svg/>").contains("viewBox"));
    for (bad, says) in [
        ("<script>alert(1)</script>", "<script>"),
        ("<style>body{display:none}</style>", "<style>"),
        (r#"<g onclick="x()"/>"#, "onclick"),
        (r#"<image href="https://example.com/x.png"/>"#, "href"),
        (r#"<rect fill="url(https://example.com/x)"/>"#, "url("),
        (
            r#"<rect style="fill: URL( 'https://example.com/x')"/>"#,
            "URL(",
        ),
        // A CSS escape spells any function past a match on its name:
        // `\75 rl(` IS `url(` to a browser, which fetched both of these.
        (
            r#"<rect style="fill: \75 rl(https://example.com/x)"/>"#,
            r"\75",
        ),
        (r#"<rect fill="\75 rl(https://example.com/x)"/>"#, r"\75"),
        // `image-set` fetches a bare string, with no `url(` at all — from a
        // `style` and from the `mask` attribute alike.
        (
            r#"<rect style="mask-image: image-set('https://example.com/x' 1x)"/>"#,
            "image-set(",
        ),
        (
            r#"<rect mask="-webkit-image-set('https://example.com/x' 1x)"/>"#,
            "image-set(",
        ),
        // `name()` is the local name, so a prefix hides neither.
        (
            r#"<image xmlns:xlink="http://www.w3.org/1999/xlink" xlink:href="https://example.com/x.png"/>"#,
            "href",
        ),
        (
            r#"<x:script xmlns:x="http://www.w3.org/1999/xhtml">alert(1)</x:script>"#,
            "<script>",
        ),
        (
            r##"<a href="#x"><set attributeName="href" to="javascript:x()"/></a>"##,
            "animate href",
        ),
    ] {
        let svg =
            format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1">{bad}</svg>"#);
        let err = refused("x.svg", &svg);
        assert!(err.contains(says), "{bad}: {err}");
    }
    // What an icon draws with is allowed: a `url(#id)` inside the picture,
    // a colour function, a transform.
    let gradient = env.file(
        "g.svg",
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><linearGradient id="g"/><rect style="fill:url( '#g')" fill="url(#g)" stroke="rgba(0,0,0,.5)" transform="translate(1 0) rotate (90) scale(.5)"/></svg>"#,
    );
    env.run(&["spec", "set", "coffee", "--icon", &gradient]);
    env.run(&["spec", "set", "coffee", "--icon", ""]);
    // `--from-json` is vetted the same way: the web saves through it.
    let err = env.fails(&[
        "spec",
        "set",
        "coffee",
        "--from-json",
        r#"{"field":"coffee","verb":"occur","icon":"<svg viewBox='0 0 1 1'><script/></svg>"}"#,
    ]);
    assert!(err.contains("<script>"), "{err}");
    assert!(!env.run(&["spec", "show", "coffee"]).contains("icon"));
}

/// Acceptance 5: a field with no icon renders byte for byte as before —
/// here, as before the icon was given, once it is cleared. What is stored
/// is the `<svg>` element alone, the prolog dropped.
#[test]
fn a_cleared_icon_renders_byte_identically_and_only_the_svg_element_is_stored() {
    let env = Env::new();
    env.run(&["occur", "coffee", "-t", "-30"]);
    let before = env.dayview();
    let icon = env.file(
        "beans.svg",
        &format!("<?xml version=\"1.0\"?>\n<!-- beans -->\n{BEANS}\n"),
    );
    env.run(&["spec", "set", "coffee", "--icon", &icon]);
    let shown: serde_json::Value =
        serde_json::from_str(&env.run(&["spec", "show", "coffee"])).unwrap();
    assert_eq!(shown["icon"], BEANS);
    assert_ne!(env.dayview(), before);
    env.run(&["spec", "set", "coffee", "--icon", ""]);
    assert_eq!(env.dayview(), before);
}

/// A stored icon is vetted again where it is drawn: one that reached the
/// database around `spec set` (sync, a hand edit) is not drawn — its dot is
/// the standard dot — and the dayview says so once, on stderr.
#[test]
fn an_unvetted_icon_in_the_database_is_not_drawn() {
    let env = Env::new();
    env.run(&["occur", "coffee", "-t", "-30"]);
    env.run(&["spec", "set", "coffee", "--description", "beans"]);
    let before = env.dayview();
    let conn = rusqlite::Connection::open(env.tmp.path().join("i.db")).unwrap();
    for bad in [
        r#"<svg viewBox="0 0 1 1"/><script>alert(1)</script>"#,
        r#"<svg viewBox="0 0 1 1"><script>alert(1)</script></svg>"#,
    ] {
        conn.execute(
            "UPDATE field_specs SET spec_json = json_set(spec_json, '$.icon', ?1) WHERE field = 'coffee'",
            [bad],
        )
        .unwrap();
        let out = env.output(&["dayview", "-n", "2"]);
        assert!(out.status.success());
        let svg = String::from_utf8(out.stdout).unwrap();
        assert!(
            !svg.contains("script") && !svg.contains("icon-coffee"),
            "{svg}"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(
            stderr.matches("coffee: icon not drawn").count(),
            1,
            "{stderr}"
        );
        assert_eq!(env.dayview(), before, "the standard dot, as with no icon");
    }
}
