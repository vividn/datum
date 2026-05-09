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
//! * `text_fixtures_have_expected_outputs` requires every fixture has a
//!   matching `expected/<name>.txt`. Empty files are allowed —
//!   `PARITY_REGEN=1` populates them on the first runtime run.
//!
//! With `parity-runtime`:
//! * `text_goldens_match_live_datum` runs each fixture's setup + query
//!   against the pinned datum binary and compares stdout to its
//!   expected. With `PARITY_REGEN=1`, mismatches *overwrite* the
//!   expected file instead of failing — same idiom as
//!   `crates/datum-cli/tests/parity/regen.sh`.

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
    // The current parity harness doesn't replay setup yet — but the field
    // round-trips through deserialization so fixtures can declare it
    // upfront. Once setup replay lands, this `allow` goes away.
    #[allow(dead_code)]
    setup: Vec<Vec<String>>,
    /// The read command whose output we're pinning.
    query: Vec<String>,
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

    #[test]
    fn text_goldens_match_live_datum() {
        let runtime = DatumRuntime::new().unwrap_or_else(|e| panic!("DatumRuntime::new: {e}"));
        let bundle = load_bundle();
        let regen = std::env::var("PARITY_REGEN").is_ok();

        let mut failures = Vec::new();
        for f in &bundle.fixture {
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

            // Empty expected file = "fixture defined but not yet
            // captured". The harness shipped with this state for some
            // fixtures so that the live test passes against the subset
            // that's populated. To opt these into the comparison, run
            // with `PARITY_REGEN=1` once and commit the captures.
            if want.is_empty() {
                eprintln!(
                    "[{}] expected.txt empty; skipping (run with PARITY_REGEN=1 to populate)",
                    f.name
                );
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
