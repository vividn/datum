//! Workspace task runner — invoked via `cargo xtask <task>`.
//!
//! Tasks live here rather than in `build.rs` files so that they don't
//! run on every `cargo build` and so that they can depend on workspace
//! crates (a build script can't).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use datum_parse::help::{render_markdown, render_roff, COMMANDS};

/// Marker bracketing the auto-generated command list inside `README.md`.
/// Kept in sync with `crates/datum-cli/tests/readme_anti_drift.rs`.
const README_BEGIN: &str = "<!-- BEGIN AUTOGEN: COMMANDS -->";
const README_END: &str = "<!-- END AUTOGEN: COMMANDS -->";

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let task = args.next().unwrap_or_default();
    match task.as_str() {
        "manpages" => manpages(args.collect()),
        "readme" => readme(),
        "check" => check(args.collect()),
        "" => {
            print_usage();
            bail!("missing task name");
        }
        other => {
            print_usage();
            bail!("unknown task: {other}");
        }
    }
}

fn print_usage() {
    eprintln!("usage: cargo xtask <task>");
    eprintln!();
    eprintln!("tasks:");
    eprintln!("  manpages [DIR]      write `man/datum.1` and `man/datum-NAME.1` files");
    eprintln!("  readme              rewrite the autogen Commands section in README.md");
    eprintln!("  check <which>       run a pre-v1 honesty gate (da-8y2q.7)");
    eprintln!("                        which: all | no-cfg-any | no-r2-stubs |");
    eprintln!("                               no-legacy-verbs | max-file-size");
}

fn readme() -> Result<()> {
    let root = workspace_root()?;
    let readme_path = root.join("README.md");
    let original = fs::read_to_string(&readme_path)
        .with_context(|| format!("reading {}", readme_path.display()))?;
    let body = autogen_commands_section();
    let updated = splice_autogen(&original, &body)?;
    if updated == original {
        eprintln!("README.md already in sync");
        return Ok(());
    }
    fs::write(&readme_path, updated)
        .with_context(|| format!("writing {}", readme_path.display()))?;
    eprintln!("wrote {}", readme_path.display());
    Ok(())
}

/// The Markdown chunk that lives between the autogen markers in README.md.
///
/// `render_markdown(None)` emits a self-contained document beginning
/// `# datum\n\nPersonal data tracker.\n\n## Usage…` — for the README we
/// strip the title/tagline so the embedded section starts at `## Usage`.
pub fn autogen_commands_section() -> String {
    let full = render_markdown(None);
    let needle = "## Usage";
    match full.find(needle) {
        Some(idx) => full[idx..].to_string(),
        None => full,
    }
}

/// Replace the body between `README_BEGIN` and `README_END` with `body`.
/// Errors if either marker is missing or out of order.
pub fn splice_autogen(readme: &str, body: &str) -> Result<String> {
    let begin = readme
        .find(README_BEGIN)
        .with_context(|| format!("README missing marker `{README_BEGIN}`"))?;
    let end = readme
        .find(README_END)
        .with_context(|| format!("README missing marker `{README_END}`"))?;
    if end < begin {
        bail!("README autogen markers are out of order");
    }
    let after_begin = begin + README_BEGIN.len();
    let mut out = String::with_capacity(readme.len() + body.len());
    out.push_str(&readme[..after_begin]);
    out.push('\n');
    out.push_str(body);
    if !body.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&readme[end..]);
    Ok(out)
}

fn manpages(args: Vec<String>) -> Result<()> {
    let dir: PathBuf = match args.first() {
        Some(p) => PathBuf::from(p),
        None => workspace_root()?.join("man"),
    };
    fs::create_dir_all(&dir)
        .with_context(|| format!("creating output directory {}", dir.display()))?;

    write_page(&dir, "datum", &render_roff(None))?;
    for spec in COMMANDS {
        let stem = format!("datum-{}", spec.name);
        write_page(&dir, &stem, &render_roff(Some(spec.name)))?;
    }

    eprintln!(
        "wrote {} man pages to {}",
        1 + COMMANDS.len(),
        dir.display()
    );
    Ok(())
}

fn write_page(dir: &Path, stem: &str, contents: &str) -> Result<()> {
    let path = dir.join(format!("{stem}.1"));
    fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

// ─── da-8y2q.7: pre-v1 honesty gates ─────────────────────────────────────────

/// Maximum LOC a single `.rs` file under `crates/` is allowed to
/// have. Bumped only by a deliberate workspace-wide decision; the
/// gate exists to keep the few existing mega-files from quietly
/// gaining roommates.
const MAX_FILE_LOC: usize = 3000;

/// Top-level entry for `cargo xtask check ...`. Each subcommand
/// scans the workspace tree under `crates/` for a specific
/// regression pattern and exits non-zero (via `bail!`) when it finds
/// one. `all` runs every gate.
fn check(args: Vec<String>) -> Result<()> {
    let which = args.first().map(String::as_str).unwrap_or("");
    match which {
        "all" => check_all(),
        "no-cfg-any" => check_no_cfg_any(),
        "no-r2-stubs" => check_no_r2_stubs(),
        "no-legacy-verbs" => check_no_legacy_verbs(),
        "max-file-size" => check_max_file_size(),
        "" => bail!(
            "check requires a subcommand: all | no-cfg-any | no-r2-stubs | \
             no-legacy-verbs | max-file-size"
        ),
        other => bail!("unknown check: {other}"),
    }
}

fn check_all() -> Result<()> {
    // Run every gate, collecting failures so a single bad commit
    // doesn't hide adjacent ones. Bails with a joined message at the
    // end if anything failed.
    let mut failures: Vec<String> = Vec::new();
    for (name, run) in CHECKS {
        match run() {
            Ok(()) => eprintln!("  ✓ {name}"),
            Err(e) => failures.push(format!("  ✗ {name}: {e}")),
        }
    }
    if !failures.is_empty() {
        bail!(
            "{} check(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
    Ok(())
}

type CheckFn = fn() -> Result<()>;

const CHECKS: &[(&str, CheckFn)] = &[
    ("no-cfg-any", check_no_cfg_any),
    ("no-r2-stubs", check_no_r2_stubs),
    ("no-legacy-verbs", check_no_legacy_verbs),
    ("max-file-size", check_max_file_size),
];

/// Locks in da-8y2q.1: the `#[cfg(any())]` attribute was the
/// "disabled module" hack — every reachable use was deleted in that
/// bead. Re-introducing one is an attempt to ship dead code; fail.
fn check_no_cfg_any() -> Result<()> {
    let hits = scan_rs_files(|content| {
        let mut out: Vec<usize> = Vec::new();
        for (i, line) in content.lines().enumerate() {
            if strip_line_comment(line).contains("#[cfg(any())]") {
                out.push(i + 1);
            }
        }
        out
    })?;
    if hits.is_empty() {
        return Ok(());
    }
    bail!(
        "found {} `#[cfg(any())]` site(s):\n{}",
        hits.len(),
        format_hits(&hits)
    );
}

/// Locks in da-8y2q.3: the "stub — R2 extracts will route through
/// X" bail!() pattern was the temporary scaffolding for the R2
/// extraction series. With R2 done, those stubs are gone; re-adding
/// one signals incomplete work shipped behind a bail.
fn check_no_r2_stubs() -> Result<()> {
    let hits = scan_rs_files(|content| {
        let mut out: Vec<usize> = Vec::new();
        for (i, line) in content.lines().enumerate() {
            let code = strip_line_comment(line);
            if code.contains("bail!") && code.contains("R2 extract") {
                out.push(i + 1);
            }
        }
        out
    })?;
    if hits.is_empty() {
        return Ok(());
    }
    bail!(
        "found {} R2-stub bail!() site(s):\n{}",
        hits.len(),
        format_hits(&hits)
    );
}

/// Locks in da-8y2q.2: the legacy verb-family (`Verb::Occur`,
/// `Verb::Start`, `Verb::Stop`, `Verb::Add`) collapsed into the
/// canonical (Point/Switch + state/dur) shape. Migrations still
/// reference these by name because they decode persisted strings;
/// every other reference is dead. The gate carves out `migrations`
/// modules and otherwise fails.
fn check_no_legacy_verbs() -> Result<()> {
    let needles = ["Verb::Occur", "Verb::Start", "Verb::Stop", "Verb::Add"];
    let hits = scan_rs_files_with_path(|path, content| {
        let mut out: Vec<(String, usize)> = Vec::new();
        // Migrations decode persisted-verb strings → carve them out.
        if path
            .iter()
            .any(|p| p == "migrations" || p == "migrations.rs")
        {
            return out;
        }
        if path.to_string_lossy().ends_with("migrations.rs") {
            return out;
        }
        for (i, line) in content.lines().enumerate() {
            let code = strip_line_comment(line);
            if needles.iter().any(|n| code.contains(n)) {
                out.push((path.to_string_lossy().to_string(), i + 1));
            }
        }
        out
    })?;
    if hits.is_empty() {
        return Ok(());
    }
    let lines: Vec<String> = hits.iter().map(|(p, l)| format!("  {p}:{l}")).collect();
    bail!(
        "found {} legacy-Verb reference(s) outside migrations:\n{}",
        hits.len(),
        lines.join("\n")
    );
}

/// Gate further growth in monolithic source files. A handful of
/// pre-existing offenders (parse.rs, migrations.rs, …) sit just
/// under the cap; the gate exists to catch the next file that
/// crosses the line, not to retroactively split anything. Crossing
/// the cap → fail; the offender must either carve up or bump the cap
/// in a deliberate commit.
fn check_max_file_size() -> Result<()> {
    let mut offenders: Vec<(String, usize)> = Vec::new();
    let crates = workspace_root()?.join("crates");
    for entry in walk_rs(&crates)? {
        let content =
            fs::read_to_string(&entry).with_context(|| format!("reading {}", entry.display()))?;
        // Count lines the same way `wc -l` does: number of '\n' bytes,
        // matching the unit users will reach for to diagnose.
        let loc = content.bytes().filter(|b| *b == b'\n').count();
        if loc > MAX_FILE_LOC {
            offenders.push((entry.display().to_string(), loc));
        }
    }
    if offenders.is_empty() {
        return Ok(());
    }
    offenders.sort_by(|a, b| b.1.cmp(&a.1));
    let lines: Vec<String> = offenders
        .iter()
        .map(|(p, n)| format!("  {p}: {n} LOC (cap = {MAX_FILE_LOC})"))
        .collect();
    bail!(
        "{} file(s) over the {}-LOC cap:\n{}\n\
         carve the file up or bump MAX_FILE_LOC in `xtask/src/main.rs` \
         in a deliberate commit.",
        offenders.len(),
        MAX_FILE_LOC,
        lines.join("\n")
    );
}

/// Strip everything from the first `//` onward so comment text
/// doesn't trip the substring checks. Doesn't handle block comments
/// `/* … */`; they're rare enough in this codebase that we accept
/// the false-positive risk over the complexity of a tokenizer.
fn strip_line_comment(line: &str) -> &str {
    match line.find("//") {
        Some(i) => &line[..i],
        None => line,
    }
}

/// Walk every `.rs` file under `crates/` and call `f` per file. The
/// closure returns line numbers (1-based) the caller considers hits;
/// the wrapper builds `path:line` strings.
fn scan_rs_files<F>(mut f: F) -> Result<Vec<String>>
where
    F: FnMut(&str) -> Vec<usize>,
{
    let crates = workspace_root()?.join("crates");
    let mut hits: Vec<String> = Vec::new();
    for entry in walk_rs(&crates)? {
        let content =
            fs::read_to_string(&entry).with_context(|| format!("reading {}", entry.display()))?;
        for line in f(&content) {
            hits.push(format!("{}:{line}", entry.display()));
        }
    }
    Ok(hits)
}

/// Variant of `scan_rs_files` that exposes the file's path to the
/// closure, for checks (like legacy-verb) that need per-file
/// carve-outs.
fn scan_rs_files_with_path<F>(mut f: F) -> Result<Vec<(String, usize)>>
where
    F: FnMut(&Path, &str) -> Vec<(String, usize)>,
{
    let crates = workspace_root()?.join("crates");
    let mut hits: Vec<(String, usize)> = Vec::new();
    for entry in walk_rs(&crates)? {
        let content =
            fs::read_to_string(&entry).with_context(|| format!("reading {}", entry.display()))?;
        hits.extend(f(&entry, &content));
    }
    Ok(hits)
}

fn format_hits(hits: &[String]) -> String {
    hits.iter()
        .map(|h| format!("  {h}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Recursively collect every `*.rs` file under `dir`. Hidden
/// directories (e.g. `.git`) and `target/` are skipped — neither
/// owns source code we want to lint.
fn walk_rs(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    walk_rs_into(dir, &mut out)?;
    Ok(out)
}

fn walk_rs_into(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("reading dir {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if name.starts_with('.') || name == "target" {
            continue;
        }
        if path.is_dir() {
            walk_rs_into(&path, out)?;
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Locate the workspace root by walking up from `CARGO_MANIFEST_DIR`
/// until we find a `Cargo.toml` whose first non-comment line declares
/// `[workspace]`.
fn workspace_root() -> Result<PathBuf> {
    let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in start.ancestors() {
        let candidate = ancestor.join("Cargo.toml");
        if candidate.is_file() {
            let text = fs::read_to_string(&candidate)?;
            if text
                .lines()
                .any(|l| l.trim_start().starts_with("[workspace]"))
            {
                return Ok(ancestor.to_path_buf());
            }
        }
    }
    bail!("could not locate workspace root from {}", start.display())
}
