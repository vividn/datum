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
    let body = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
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
    for required in [Surface::Get, Surface::Tail, Surface::Dayview, Surface::Nowview] {
        let count = bundle.fixture.iter().filter(|f| f.surface == required).count();
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
        assert!(!f.query.is_empty(), "fixture {}: query must be non-empty", f.name);
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
    /// Datum's `CREATE <hid> <slug>` log lines and any line with a `hid`
    /// trailing column are stripped; the visible-text body is what we
    /// pin. If a future fixture needs hid pinning, add a `--show=none`
    /// flag to the query rather than relaxing this normalizer.
    fn normalize(raw: &str) -> String {
        let mut out = Vec::new();
        for line in raw.lines() {
            if line.starts_with("CREATE ") {
                continue;
            }
            out.push(line.to_owned());
        }
        let mut joined = out.join("\n");
        if !joined.is_empty() {
            joined.push('\n');
        }
        joined
    }

    /// Run one fixture under a fresh PouchDB. Returns `Ok(captured)` on
    /// success or `Err(message)` describing the failure. Failures are
    /// accumulated by the caller so one bad fixture doesn't block the
    /// whole batch (especially under PARITY_REGEN=1).
    fn run_fixture(
        runtime: &DatumRuntime,
        f: &Fixture,
    ) -> Result<String, String> {
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

        let mut q_full: Vec<&str> = vec!["--db", &db];
        q_full.extend(f.query.iter().map(|s| s.as_str()));
        let raw = runtime
            .run_stdout(temp.path(), &q_full)
            .map_err(|e| format!("query {:?}: {e}", f.query))?;
        Ok(normalize(&raw))
    }

    #[test]
    fn text_goldens_match_live_datum() {
        let runtime = DatumRuntime::new()
            .unwrap_or_else(|e| panic!("DatumRuntime::new: {e}"));
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
