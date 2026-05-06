//! Spawn the original-datum (TS) binary to capture its output.
//!
//! Feature-gated behind `parity-runtime` so plain `cargo test` doesn't
//! pull node into a Rust-only build. CI must opt in:
//!
//! ```text
//! cargo test --features parity-runtime -p datum-parity
//! ```
//!
//! ## Setup contract
//!
//! `DatumRuntime::new()` checks that `target/parity-cache/datum/` exists
//! with `node_modules` populated. If not, it returns an error pointing
//! at `scripts/setup_datum.sh`. Tests must surface that error rather
//! than silently passing.
//!
//! ## Database isolation
//!
//! Datum stores PouchDB-on-disk under `DATUM_HOST=<dir>`. Each runtime
//! call accepts a `host_dir` so callers can create a temp directory
//! per-test (see `tempfile::TempDir`) and have it cleaned up
//! automatically.
//!
//! ## Time pinning
//!
//! Datum doesn't expose a "fix-now" environment variable, but commands
//! that take `-t HHMM -d YYYY-MM-DD` use the pinned absolute time
//! deterministically. Callers must always pass these flags rather than
//! relying on `now`. Output snapshots that mention "now" (e.g.,
//! `nowview` headers) substitute the pinned time before comparison.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Errors surfaced by the runtime.
#[derive(Debug)]
pub enum RuntimeError {
    /// `target/parity-cache/datum` is missing or unbuilt; user should run
    /// `scripts/setup_datum.sh`.
    NotInstalled(String),
    /// The spawned `pnpm dev` command exited non-zero. Stderr text is
    /// included verbatim.
    Failed {
        /// Argument vector passed to `datum` (after `pnpm dev --`).
        args: Vec<String>,
        /// Process exit status as printed by the OS (`exited with N` or `signal N`).
        status: String,
        /// Captured stderr from the failing process.
        stderr: String,
    },
    /// The `pnpm` binary couldn't be spawned at all.
    SpawnFailed(std::io::Error),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInstalled(s) => write!(f, "{s}"),
            Self::Failed {
                args,
                status,
                stderr,
            } => write!(
                f,
                "datum {args:?} failed ({status}):\n{stderr}",
            ),
            Self::SpawnFailed(e) => write!(f, "spawn pnpm: {e}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

/// Wrapper around the `pnpm dev` invocation in `target/parity-cache/datum`.
///
/// Cheap to construct (just validates paths); spawning a subprocess
/// happens on each `run` call.
pub struct DatumRuntime {
    repo: PathBuf,
}

impl DatumRuntime {
    /// Validate the clone exists and dependencies are installed. Returns
    /// `NotInstalled` with a diagnostic message otherwise.
    pub fn new() -> Result<Self, RuntimeError> {
        let repo = crate::datum_clone_path();
        if !repo.exists() {
            return Err(RuntimeError::NotInstalled(format!(
                "datum clone missing at {}\n\
                 Run: bash crates/datum-parity/scripts/setup_datum.sh",
                repo.display()
            )));
        }
        if !repo.join("node_modules").exists() {
            return Err(RuntimeError::NotInstalled(format!(
                "datum clone at {} is missing node_modules\n\
                 Run: bash crates/datum-parity/scripts/setup_datum.sh",
                repo.display()
            )));
        }
        if !repo.join("src").join("index.ts").exists() {
            return Err(RuntimeError::NotInstalled(format!(
                "datum clone at {} doesn't look like vividn/datum (no src/index.ts)",
                repo.display()
            )));
        }
        Ok(Self { repo })
    }

    /// Run `datum <args>` against the given on-disk PouchDB host
    /// directory. Returns the full `Output` so callers can inspect
    /// stdout, stderr, and status separately.
    ///
    /// `host_dir` is passed via `DATUM_HOST` and must already exist
    /// (callers typically use `tempfile::TempDir`). The `--db` flag is
    /// NOT injected — pass it explicitly in `args` so callers control
    /// the db namespace.
    pub fn run(
        &self,
        host_dir: &Path,
        args: &[&str],
    ) -> Result<Output, RuntimeError> {
        let owned_args: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();

        let mut cmd = Command::new("pnpm");
        cmd.arg("dev")
            .arg("--")
            .args(args)
            .current_dir(&self.repo)
            .env("DATUM_HOST", host_dir)
            // Suppress pnpm's "Run X" preamble — keep stdout pure datum output.
            .env("npm_config_silent", "1")
            // Force chalk into truecolor mode (`[48;2;R;G;Bm`) so datum's
            // ANSI matches daydumb's. Without this, chalk auto-detects
            // 256-color (`[48;5;Nm`) when stdout is piped, and goldens
            // never line up. See chalk@4 supports-color@7 logic.
            .env("FORCE_COLOR", "3")
            .env("COLORTERM", "truecolor")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().map_err(RuntimeError::SpawnFailed)?;
        if !output.status.success() {
            return Err(RuntimeError::Failed {
                args: owned_args,
                status: format!("{:?}", output.status),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }
        Ok(output)
    }

    /// Convenience: capture stdout as UTF-8, after stripping pnpm's
    /// preamble lines (the `> @vividn/datum@... dev` and
    /// `> NODE_ENV=development ts-node ...` headers that pnpm adds before
    /// the actual datum output).
    pub fn run_stdout(
        &self,
        host_dir: &Path,
        args: &[&str],
    ) -> Result<String, RuntimeError> {
        let out = self.run(host_dir, args)?;
        let raw = String::from_utf8_lossy(&out.stdout);
        Ok(strip_pnpm_preamble(&raw))
    }

    /// Initialize a fresh DB. Needed before any non-trivial command —
    /// datum requires its design documents to be present.
    pub fn setup_db(&self, host_dir: &Path, db_name: &str) -> Result<(), RuntimeError> {
        let _ = self.run(host_dir, &["--create-db", "--db", db_name, "setup"])?;
        Ok(())
    }
}

/// Strip the `> @vividn/datum@2.1.0-alpha.0 dev /path` and
/// `> NODE_ENV=development ts-node ./src/index.ts ...` lines that pnpm
/// echoes before the actual subcommand output.
///
/// Datum's own output never starts with `> ` because `>` isn't a leading
/// character it ever uses (it uses ANSI escapes, hex colors, etc.); so
/// dropping every leading `> `-prefixed line is safe.
pub(crate) fn strip_pnpm_preamble(raw: &str) -> String {
    raw.lines()
        .skip_while(|l| l.is_empty() || l.starts_with("> "))
        .collect::<Vec<_>>()
        .join("\n")
        + if raw.ends_with('\n') { "\n" } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_preamble_drops_pnpm_headers() {
        let raw = "\n> @vividn/datum@2.1.0-alpha.0 dev /path\n> NODE_ENV=development ts-node ...\n\nactual output\nmore output\n";
        assert_eq!(strip_pnpm_preamble(raw), "actual output\nmore output\n");
    }

    #[test]
    fn strip_preamble_passes_clean_output_through() {
        let raw = "00:42:57+2 coffee ●\n";
        assert_eq!(strip_pnpm_preamble(raw), raw);
    }
}
