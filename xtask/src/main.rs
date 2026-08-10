//! Workspace task runner — invoked via `cargo xtask <task>`.
//!
//! Tasks live here rather than in `build.rs` files so that they don't
//! run on every `cargo build` and so that they can depend on workspace
//! crates (a build script can't).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
        "bump-version" => bump_version(args.collect()),
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
    eprintln!("  check <which>       run a check battery, or one gate by name");
    eprintln!("                        all   full CI-parity battery: the lint gates,");
    eprintln!("                              then fmt, clippy, build, tests, doc tests");
    eprintln!("                              (slow — it compiles the workspace)");
    eprintln!("                        lint  the repo-hygiene gates only (fast)");
    eprintln!("                        one gate: no-cfg-any | no-r2-stubs |");
    eprintln!("                                  no-legacy-verbs | max-file-size |");
    eprintln!("                                  toolchain-pin");
    eprintln!("  bump-version        bump the workspace version from HEAD's commit type");
    eprintln!("                        flags: --push --dry-run --subject <msg>");
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

// ─── da-qlfk: auto-bump the workspace version in the merge path ───────────────

/// The version bump implied by a merged commit's subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bump {
    /// A `feat:` merge — bump the minor, zero the patch.
    Minor,
    /// Any other merge (`fix:`, `docs:`, unprefixed, …) — bump the patch.
    Patch,
    /// The merge is our own `chore(release):` bump — do nothing, so a
    /// re-run (or a push that re-triggers the hook) never chains bumps.
    Skip,
}

/// Auto-bump the `[workspace.package]` version so it tracks main, then
/// commit (and optionally push) the change. Pairs with the on-merge
/// auto-upgrade (`datum-upgrade`); wire it into the merge-queue
/// `post_merge_cmd` — see `docs/version-bumping.md`.
///
/// Policy (conventional-commit): a `feat:` merge → **minor**; anything
/// else → **patch**. The version stays **0.x** until production — the
/// major is never bumped automatically. The bump kind is read from
/// HEAD's commit subject (override with `--subject "<msg>"`).
fn bump_version(args: Vec<String>) -> Result<()> {
    let mut push = false;
    let mut dry_run = false;
    let mut subject: Option<String> = None;
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--push" => push = true,
            "--dry-run" => dry_run = true,
            "--subject" => subject = Some(it.next().context("--subject needs a value")?),
            other => bail!("unknown bump-version flag: {other}"),
        }
    }

    let root = workspace_root()?;
    let subject = match subject {
        Some(s) => s,
        None => git_head_subject(&root)?,
    };

    let kind = bump_kind(&subject);
    if kind == Bump::Skip {
        eprintln!("bump-version: HEAD is a release commit — nothing to bump");
        return Ok(());
    }

    let manifest_path = root.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let old = read_workspace_version(&manifest)?;
    let new = apply_bump(&old, kind)?;
    eprintln!("bump-version: {old} → {new}  ({kind:?}, from {subject:?})");
    if dry_run {
        return Ok(());
    }

    let (_, updated_manifest) = set_workspace_version(&manifest, &new)?;
    fs::write(&manifest_path, &updated_manifest)
        .with_context(|| format!("writing {}", manifest_path.display()))?;

    // Keep Cargo.lock in step so the merge host's tree stays clean and a
    // `--locked` build never trips. All local crates share the workspace
    // version, so they move together.
    let lock_path = root.join("Cargo.lock");
    if lock_path.is_file() {
        let lock = fs::read_to_string(&lock_path)
            .with_context(|| format!("reading {}", lock_path.display()))?;
        fs::write(&lock_path, set_lockfile_versions(&lock, &new))
            .with_context(|| format!("writing {}", lock_path.display()))?;
    }

    run_git(&root, &["add", "Cargo.toml", "Cargo.lock"])?;
    run_git(&root, &["commit", "-m", &format!("chore(release): v{new}")])?;
    eprintln!("bump-version: committed chore(release): v{new}");
    if push {
        run_git(&root, &["push"])?;
        eprintln!("bump-version: pushed");
    }
    Ok(())
}

/// Map a merged commit's subject to a bump kind. Our own
/// `chore(release):` commits are skipped so a bump never chains; a
/// `feat` type bumps the minor; everything else bumps the patch.
fn bump_kind(subject: &str) -> Bump {
    if subject.trim_start().starts_with("chore(release):") {
        return Bump::Skip;
    }
    match commit_type(subject) {
        Some("feat") => Bump::Minor,
        _ => Bump::Patch,
    }
}

/// The conventional-commit *type* token at the head of `subject`, if the
/// header is well-formed: a lowercase `<type>`, an optional `(scope)`,
/// an optional `!`, then `:`. `feat(cli)!: x` → `Some("feat")`;
/// `plain words` → `None`.
fn commit_type(subject: &str) -> Option<&str> {
    let s = subject.trim_start();
    let ty_len = s.find(|c: char| !c.is_ascii_lowercase())?;
    if ty_len == 0 {
        return None;
    }
    let (ty, mut rest) = s.split_at(ty_len);
    if let Some(after_open) = rest.strip_prefix('(') {
        rest = &after_open[after_open.find(')')? + 1..];
    }
    rest = rest.strip_prefix('!').unwrap_or(rest);
    rest.starts_with(':').then_some(ty)
}

/// Apply a bump to a `major.minor.patch` version, keeping the pre-v1
/// invariant: the **major is fixed** (0.x until production). A minor
/// bump zeroes the patch; a patch bump leaves the minor untouched.
fn apply_bump(version: &str, kind: Bump) -> Result<String> {
    let (major, minor, patch) = parse_semver(version)?;
    let (minor, patch) = match kind {
        Bump::Minor => (minor + 1, 0),
        Bump::Patch => (minor, patch + 1),
        Bump::Skip => return Ok(version.to_string()),
    };
    Ok(format!("{major}.{minor}.{patch}"))
}

fn parse_semver(v: &str) -> Result<(u64, u64, u64)> {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.len() != 3 {
        bail!("version `{v}` is not `major.minor.patch`");
    }
    let num = |s: &str| -> Result<u64> {
        s.parse::<u64>()
            .with_context(|| format!("non-numeric component in version `{v}`"))
    };
    Ok((num(parts[0])?, num(parts[1])?, num(parts[2])?))
}

/// Extract the double-quoted value from a bare `version = "..."` line.
/// Ignores `version.workspace = true` and any line whose key isn't a
/// standalone `version`. Shared by the Cargo.toml and Cargo.lock passes.
fn parse_version_line(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix("version")?;
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    Some(&rest[..rest.find('"')?])
}

/// Read the `[workspace.package]` version from a Cargo.toml.
fn read_workspace_version(cargo_toml: &str) -> Result<String> {
    let mut in_pkg = false;
    for line in cargo_toml.lines() {
        let t = line.trim_start();
        if t.starts_with('[') {
            in_pkg = t.starts_with("[workspace.package]");
        } else if in_pkg {
            if let Some(v) = parse_version_line(line) {
                return Ok(v.to_string());
            }
        }
    }
    bail!("no `[workspace.package]` version in Cargo.toml")
}

/// Rewrite the `[workspace.package]` version, returning `(old, updated)`.
/// Only the single version line in that section changes; every other
/// byte is preserved so the diff stays minimal.
fn set_workspace_version(cargo_toml: &str, new: &str) -> Result<(String, String)> {
    let mut in_pkg = false;
    let mut old: Option<String> = None;
    let mut out = String::with_capacity(cargo_toml.len() + 8);
    for line in cargo_toml.lines() {
        let t = line.trim_start();
        if t.starts_with('[') {
            in_pkg = t.starts_with("[workspace.package]");
        } else if in_pkg && old.is_none() {
            if let Some(v) = parse_version_line(line) {
                old = Some(v.to_string());
                out.push_str(&format!("version = \"{new}\"\n"));
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    let old = old.context("no `[workspace.package]` version in Cargo.toml")?;
    Ok((old, out))
}

/// Bump every *local* crate's version in Cargo.lock to `new`. Local
/// (path/workspace) crates are exactly the `[[package]]` blocks with no
/// `source` field — registry crates always carry one — and they all
/// inherit the single workspace version, so they move together. The
/// top-of-file lockfile-format `version = N` sits outside any package
/// block and is left untouched.
fn set_lockfile_versions(lock: &str, new: &str) -> String {
    let mut out = String::with_capacity(lock.len());
    let mut block: Vec<&str> = Vec::new();
    let mut in_block = false;
    let flush = |out: &mut String, block: &[&str]| {
        let local = !block
            .iter()
            .any(|l| l.trim_start().starts_with("source = "));
        let mut replaced = false;
        for l in block {
            if local && !replaced && l.trim_start().starts_with("version = \"") {
                out.push_str(&format!("version = \"{new}\"\n"));
                replaced = true;
            } else {
                out.push_str(l);
                out.push('\n');
            }
        }
    };
    for line in lock.lines() {
        if line.trim_start() == "[[package]]" {
            if in_block {
                flush(&mut out, &block);
                block.clear();
            }
            in_block = true;
        }
        if in_block {
            block.push(line);
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if in_block {
        flush(&mut out, &block);
    }
    out
}

fn git_head_subject(root: &Path) -> Result<String> {
    Ok(run_git(root, &["log", "-1", "--format=%s"])?
        .trim()
        .to_string())
}

/// Run `git -C <root> <args>`, returning stdout; bails on a non-zero exit.
fn run_git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

// ─── da-8y2q.7: pre-v1 honesty gates ─────────────────────────────────────────

/// Maximum LOC a single `.rs` file under `crates/` is allowed to
/// have. Bumped only by a deliberate workspace-wide decision; the
/// gate exists to keep the few existing mega-files from quietly
/// gaining roommates.
///
/// History:
/// - 3000 (initial, `da-8y2q.7`) — locked the carved cmd.rs (~265 LOC
///   of source after `da-jflh.1`) and field_spec.rs (`da-jflh.4/.5`).
/// - 3500 (2026-06-02, vremya, this commit) — bumped to wire the gate
///   into CI; the two pre-existing offenders (`parse.rs` at 3183 LOC
///   and `migrations.rs` at 3227 LOC) re-grew past 3000 after their
///   carve-ups (`da-jflh.2` shipped `parse_command_with_hooks` cleanly,
///   `migrations.rs` accumulates each new schema version), and bumping
///   to a deliberate next-round number is preferable to leaving the
///   gate disabled. Re-carve work is tracked separately as `vremya:`
///   beads. Next bump should be a real conversation, not a casual
///   `+200`.
const MAX_FILE_LOC: usize = 3500;

/// Top-level entry for `cargo xtask check ...` — two batteries, plus
/// every gate addressable on its own by name.
///
/// * `lint` — the in-process repo-hygiene gates: the four pre-v1
///   honesty gates (`da-8y2q.7`), which scan the tree under `crates/`
///   for a regression pattern, plus `toolchain-pin`, which reads
///   `rust-toolchain.toml` and every Dockerfile. Compiles nothing, so
///   it's the fast pre-commit loop.
/// * `all` — `lint`, then the gates that actually compile the code:
///   `cargo fmt --check`, `cargo clippy -D warnings`, `cargo build`,
///   `cargo nextest run`, and the doc tests. Mirrors the `build-and-test`,
///   `lint` and `honesty` jobs of `.github/workflows/ci.yml`.
///
/// **What `all` does not cover**, stated here and printed after every
/// run, because a battery whose name outruns its contents is exactly
/// the defect this shape exists to prevent: the `e2e-full` and
/// `wasm-check` jobs, both separate jobs in that same workflow. See
/// `UNCOVERED_BY_ALL`.
///
/// A step that *cannot* run — `cargo-nextest` not installed, say — is a
/// failure, never a skip. Quietly dropping a step and still printing
/// green is the one outcome this command must never produce.
fn check(args: Vec<String>) -> Result<()> {
    let which = args.first().map(String::as_str).unwrap_or("");
    match which {
        "all" => run_battery(
            "all",
            &[LINT_STEPS, COMPILE_STEPS].concat(),
            UNCOVERED_BY_ALL,
        ),
        "lint" => run_battery("lint", LINT_STEPS, UNCOVERED_BY_LINT),
        "no-cfg-any" => check_no_cfg_any(),
        "no-r2-stubs" => check_no_r2_stubs(),
        "no-legacy-verbs" => check_no_legacy_verbs(),
        "max-file-size" => check_max_file_size(),
        "toolchain-pin" => check_toolchain_pin(),
        "" => bail!(
            "check requires a subcommand: all | lint | no-cfg-any | no-r2-stubs | \
             no-legacy-verbs | max-file-size | toolchain-pin"
        ),
        other => bail!("unknown check: {other}"),
    }
}

type CheckFn = fn() -> Result<()>;

/// One step of a battery: an in-process hygiene gate, or a `cargo`
/// invocation shelled out so it is verbatim the command CI runs (and
/// the one a reader can copy out of the output and re-run by hand).
#[derive(Clone, Copy)]
enum Step {
    /// Name, what it reads, and the gate itself. The scope is carried
    /// per-gate rather than assumed: the gates no longer all read the
    /// same tree, and a step that misreports its own reach is the defect
    /// this printing exists to prevent.
    Gate(&'static str, &'static str, CheckFn),
    Cargo(&'static str, &'static [&'static str]),
}

impl Step {
    fn name(&self) -> &'static str {
        match self {
            Step::Gate(name, ..) | Step::Cargo(name, _) => name,
        }
    }

    /// What the step actually does, printed alongside the name so the
    /// output names its own coverage rather than asking for trust.
    fn detail(&self) -> String {
        match self {
            Step::Gate(_, scope, _) => format!("in-process scan of {scope}"),
            Step::Cargo(_, args) => format!("cargo {}", args.join(" ")),
        }
    }

    fn run(&self) -> Result<()> {
        match self {
            Step::Gate(_, _, run) => run(),
            Step::Cargo(_, args) => run_cargo(args),
        }
    }
}

/// The repo-hygiene gates — `check lint`, and the head of `check all`.
/// The first four are the pre-v1 honesty gates (`da-8y2q.7`); the last
/// keeps the Rust version single-sourced.
const LINT_STEPS: &[Step] = &[
    Step::Gate("no-cfg-any", "crates/", check_no_cfg_any),
    Step::Gate("no-r2-stubs", "crates/", check_no_r2_stubs),
    Step::Gate("no-legacy-verbs", "crates/", check_no_legacy_verbs),
    Step::Gate("max-file-size", "crates/", check_max_file_size),
    Step::Gate(
        "toolchain-pin",
        "rust-toolchain.toml + every Dockerfile",
        check_toolchain_pin,
    ),
];

/// The compiling half of `check all`, ordered cheapest-first so a stray
/// comma fails in seconds instead of after the test suite.
const COMPILE_STEPS: &[Step] = &[
    Step::Cargo("fmt", &["fmt", "--all", "--", "--check"]),
    Step::Cargo(
        "clippy",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    ),
    Step::Cargo("build", &["build", "--workspace", "--all-targets"]),
    Step::Cargo(
        "nextest",
        &["nextest", "run", "--workspace", "--no-fail-fast"],
    ),
    Step::Cargo("doc-tests", &["test", "--workspace", "--doc"]),
];

/// CI jobs `check all` deliberately leaves out, printed after every run.
/// Naming the gap is the point: `check all` previously meant four
/// hygiene lints while its name implied the lot, and an unnamed gap is
/// how that happens.
const UNCOVERED_BY_ALL: &[&str] = &[
    "e2e-full   — cargo nextest run -p datum-commands --test e2e --features e2e-full \
     (feature-gated cohort; its own CI job)",
    "wasm-check — cargo check -p datum-sync-client --target wasm32-unknown-unknown \
     --no-default-features --features wasm (needs the wasm32 target installed)",
];

/// `lint` is the fast subset and must not be mistaken for the battery.
const UNCOVERED_BY_LINT: &[&str] = &[
    "everything that compiles — fmt, clippy, build, tests, doc tests: \
     run `cargo xtask check all`",
];

/// Run a battery, printing each step and its result as it goes, then
/// listing what actually executed and what was not covered.
///
/// Failures are collected rather than short-circuited — the original
/// `check all` did this deliberately so one bad commit can't hide
/// adjacent breakage, and it matters more now that a step can be a
/// twenty-minute test run.
fn run_battery(battery: &str, steps: &[Step], uncovered: &[&str]) -> Result<()> {
    eprintln!("cargo xtask check {battery} — {} step(s)", steps.len());
    let mut failures: Vec<String> = Vec::new();
    let mut ran: Vec<&str> = Vec::new();
    for step in steps {
        eprintln!("\n── {} — {}", step.name(), step.detail());
        ran.push(step.name());
        match step.run() {
            Ok(()) => eprintln!("  ✓ {}", step.name()),
            Err(e) => {
                eprintln!("  ✗ {}: {e}", step.name());
                failures.push(format!("  ✗ {}: {e}", step.name()));
            }
        }
    }

    eprintln!("\n── summary");
    eprintln!("  ran {} step(s): {}", ran.len(), ran.join(", "));
    for line in uncovered {
        eprintln!("  not covered: {line}");
    }
    if !failures.is_empty() {
        bail!(
            "{} of {} step(s) failed:\n{}",
            failures.len(),
            steps.len(),
            failures.join("\n")
        );
    }
    eprintln!("  ✓ all {} step(s) passed", steps.len());
    Ok(())
}

/// Cargo subcommands that ship as a separately-installed binary, with
/// the command that installs each. Used only to turn a missing tool
/// into a message that says so.
const CARGO_PLUGINS: &[(&str, &str)] = &[("nextest", "cargo install cargo-nextest --locked")];

/// Shell out to `cargo <args>` at the workspace root, inheriting stdio
/// so a failure is diagnosable in the terminal it ran in.
///
/// A tool that isn't installed is an error here, not a fallback.
/// `scripts/check.sh` substitutes `cargo test` when `cargo-nextest` is
/// missing, which is right for a convenience script and wrong for a
/// gate: the run would go green having tested something other than what
/// it claimed.
fn run_cargo(args: &[&str]) -> Result<()> {
    let root = workspace_root()?;
    // `CARGO` is set when we're invoked as `cargo xtask`; the fallback
    // keeps the binary usable when run directly.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(&cargo)
        .current_dir(&root)
        .args(args)
        .status()
        .with_context(|| format!("could not run `{cargo}` — is cargo on PATH?"))?;
    if status.success() {
        return Ok(());
    }
    let sub = args.first().copied().unwrap_or_default();
    if let Some((_, install)) = CARGO_PLUGINS.iter().find(|(name, _)| *name == sub) {
        if !plugin_installed(&cargo, sub) {
            bail!(
                "`cargo {sub}` is not installed — install it with `{install}`. \
                 This step fails rather than falling back to another command, so a \
                 green run always means every named gate actually ran."
            );
        }
    }
    match status.code() {
        Some(code) => bail!("`cargo {}` exited {code}", args.join(" ")),
        None => bail!("`cargo {}` was killed by a signal", args.join(" ")),
    }
}

/// Whether `cargo <sub>` resolves to an installed plugin.
fn plugin_installed(cargo: &str, sub: &str) -> bool {
    Command::new(cargo)
        .args([sub, "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

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
    offenders.sort_by_key(|b| std::cmp::Reverse(b.1));
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

/// Keeps the Rust version single-sourced. `rust-toolchain.toml` decides
/// it; a Dockerfile's `FROM rust:<tag>` is only a cache hint, because
/// rustup obeys the pin whatever the tag says. That is exactly what makes
/// a stale tag dangerous rather than loud: the image still builds, so
/// nothing goes red, and the tag sat at `1.85` long after the workspace
/// moved on. The gate is the price of letting the version appear twice —
/// remove it and the second copy is free to drift again.
///
/// Fails if the pin is unreadable, if no Dockerfile names a `rust:` base
/// (a gate that finds nothing must not pass), or if any tag disagrees.
fn check_toolchain_pin() -> Result<()> {
    let root = workspace_root()?;
    let pin_path = root.join("rust-toolchain.toml");
    let pin =
        fs::read_to_string(&pin_path).with_context(|| format!("reading {}", pin_path.display()))?;
    let channel = toolchain_channel(&pin)
        .with_context(|| format!("no `[toolchain] channel` in {}", pin_path.display()))?;

    let dockerfiles = walk_files(&root, &|p| {
        p.file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| n.starts_with("Dockerfile"))
    })?;

    let mut examined = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for path in &dockerfiles {
        let content =
            fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        for (line, tag) in rust_base_tags(&content) {
            examined += 1;
            if tag != channel {
                mismatches.push(format!(
                    "  {}:{line}: base image is rust:{tag}, pin says {channel}",
                    path.display()
                ));
            }
        }
    }

    if examined == 0 {
        bail!(
            "no `FROM rust:<tag>` line in any of the {} Dockerfile(s) under {} — \
             this gate had nothing to compare and would have passed vacuously; \
             fix the scan in `xtask/src/main.rs` rather than deleting the gate.",
            dockerfiles.len(),
            root.display()
        );
    }
    if mismatches.is_empty() {
        return Ok(());
    }
    bail!(
        "{} Dockerfile base tag(s) disagree with rust-toolchain.toml:\n{}\n\
         retag to rust:{channel} — the pin decides the version, the tag only \
         decides whether the base image's compiler can be reused.",
        mismatches.len(),
        mismatches.join("\n")
    )
}

/// The `channel` value from a rust-toolchain.toml's `[toolchain]` table.
/// A `channel` key in any other table isn't the pin, and comment lines
/// (the pin file's header talks *about* `channel`) never count.
fn toolchain_channel(pin: &str) -> Option<&str> {
    let mut in_toolchain = false;
    for line in pin.lines() {
        let t = line.trim_start();
        if t.starts_with('#') {
            continue;
        }
        if t.starts_with('[') {
            in_toolchain = t.starts_with("[toolchain]");
        } else if in_toolchain {
            if let Some(v) = quoted_value(t, "channel") {
                return Some(v);
            }
        }
    }
    None
}

/// `key = "value"` → `Some("value")`. A longer key that merely starts with
/// `key` falls out on its own: the remainder won't begin with `=`.
fn quoted_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line
        .trim_start()
        .strip_prefix(key)?
        .trim_start()
        .strip_prefix('=')?
        .trim_start()
        .strip_prefix('"')?;
    Some(&rest[..rest.find('"')?])
}

/// Every `rust:` base image a Dockerfile builds on, as `(line, version)`.
/// Handles the shapes that actually occur: a lowercase instruction, a
/// `--platform` flag, a registry-qualified name. The variant suffix is
/// dropped (`1.97.1-slim` → `1.97.1`) so moving to a slim or alpine base
/// isn't a spurious failure; a tag naming no version (`rust:latest`) is
/// returned as-is and therefore matches no pin, which is the point.
fn rust_base_tags(dockerfile: &str) -> Vec<(usize, &str)> {
    let mut out: Vec<(usize, &str)> = Vec::new();
    for (i, line) in dockerfile.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with('#') || !t.get(..5).is_some_and(|h| h.eq_ignore_ascii_case("FROM ")) {
            continue;
        }
        let Some(image) = t[5..].split_whitespace().find(|tok| !tok.starts_with("--")) else {
            continue;
        };
        // `docker.io/library/rust:1.97.1` and `rust:1.97.1` alike.
        let Some(tag) = image
            .rsplit('/')
            .next()
            .and_then(|r| r.strip_prefix("rust:"))
        else {
            continue;
        };
        out.push((i + 1, tag.split('-').next().unwrap_or(tag)));
    }
    out
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

/// Recursively collect every `*.rs` file under `dir`.
fn walk_rs(dir: &Path) -> Result<Vec<PathBuf>> {
    walk_files(dir, &|p| {
        p.extension().and_then(|s| s.to_str()) == Some("rs")
    })
}

/// Recursively collect every file under `dir` that `keep` accepts. Hidden
/// directories (e.g. `.git`) and `target/` are skipped — neither owns
/// source we want to lint.
fn walk_files(dir: &Path, keep: &dyn Fn(&Path) -> bool) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    walk_files_into(dir, keep, &mut out)?;
    Ok(out)
}

fn walk_files_into(dir: &Path, keep: &dyn Fn(&Path) -> bool, out: &mut Vec<PathBuf>) -> Result<()> {
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
            walk_files_into(&path, keep, out)?;
        } else if keep(&path) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_type_parses_conventional_headers() {
        assert_eq!(commit_type("feat: add thing"), Some("feat"));
        assert_eq!(commit_type("feat(cli): add thing"), Some("feat"));
        assert_eq!(commit_type("feat(cli)!: breaking"), Some("feat"));
        assert_eq!(commit_type("fix(count): drop unimplemented"), Some("fix"));
        assert_eq!(commit_type("  chore(release): v0.1.1"), Some("chore"));
        assert_eq!(commit_type("plain words, no prefix"), None);
        assert_eq!(commit_type("Feat: capitalized isn't a type"), None);
    }

    #[test]
    fn bump_kind_maps_feat_to_minor_else_patch() {
        assert_eq!(
            bump_kind("feat(spec): safe incremental add-state"),
            Bump::Minor
        );
        assert_eq!(bump_kind("fix(retime): refresh offset"), Bump::Patch);
        assert_eq!(bump_kind("docs: tidy readme"), Bump::Patch);
        assert_eq!(bump_kind("random unprefixed merge"), Bump::Patch);
        // Our own release commit must never chain another bump.
        assert_eq!(bump_kind("chore(release): v0.2.0"), Bump::Skip);
    }

    #[test]
    fn apply_bump_keeps_major_fixed() {
        assert_eq!(apply_bump("0.1.0", Bump::Minor).unwrap(), "0.2.0");
        assert_eq!(apply_bump("0.1.5", Bump::Minor).unwrap(), "0.2.0");
        assert_eq!(apply_bump("0.1.0", Bump::Patch).unwrap(), "0.1.1");
        assert_eq!(apply_bump("0.2.9", Bump::Patch).unwrap(), "0.2.10");
        assert_eq!(apply_bump("0.1.0", Bump::Skip).unwrap(), "0.1.0");
        assert!(apply_bump("0.1", Bump::Patch).is_err());
        assert!(apply_bump("0.1.x", Bump::Patch).is_err());
    }

    #[test]
    fn parse_version_line_only_matches_bare_version_key() {
        assert_eq!(parse_version_line("version = \"0.1.0\""), Some("0.1.0"));
        assert_eq!(parse_version_line("  version=\"1.2.3\""), Some("1.2.3"));
        assert_eq!(parse_version_line("version.workspace = true"), None);
        assert_eq!(parse_version_line("serde = { version = \"1\" }"), None);
    }

    #[test]
    fn set_workspace_version_touches_only_the_package_section() {
        let toml = "[workspace]\nmembers = [\"a\"]\n\n\
                    [workspace.package]\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
                    [workspace.dependencies]\nserde = { version = \"1\" }\n";
        let (old, out) = set_workspace_version(toml, "0.2.0").unwrap();
        assert_eq!(old, "0.1.0");
        assert!(out.contains("[workspace.package]\nversion = \"0.2.0\"\nedition"));
        // The dependency's version = "1" must be untouched.
        assert!(out.contains("serde = { version = \"1\" }"));
        assert_eq!(read_workspace_version(&out).unwrap(), "0.2.0");
    }

    #[test]
    fn toolchain_channel_reads_only_the_pin() {
        let pin = "# To bump: change `channel` here, then run the gates.\n\
                   [toolchain]\nchannel = \"1.97.1\"\nprofile = \"default\"\n";
        assert_eq!(toolchain_channel(pin), Some("1.97.1"));
        // A `channel` under some other table is not the pin.
        assert_eq!(toolchain_channel("[other]\nchannel = \"nope\"\n"), None);
        assert_eq!(
            toolchain_channel("[toolchain]\nprofile = \"default\"\n"),
            None
        );
    }

    #[test]
    fn rust_base_tags_finds_the_version_it_must_compare() {
        assert_eq!(
            rust_base_tags("FROM rust:1.97.1 AS builder\n"),
            vec![(1, "1.97.1")]
        );
        // Variant suffixes compare on the version alone.
        assert_eq!(
            rust_base_tags("FROM rust:1.97.1-slim\n"),
            vec![(1, "1.97.1")]
        );
        // Lowercase instruction, --platform flag, registry-qualified name.
        assert_eq!(
            rust_base_tags("from --platform=$BUILDPLATFORM docker.io/library/rust:1.85\n"),
            vec![(1, "1.85")]
        );
        // Comments and other bases aren't ours to police.
        assert_eq!(
            rust_base_tags("# FROM rust:1.0\nFROM debian:bookworm-slim\n"),
            vec![]
        );
        // A floating tag names no version, so it can never agree with a pin.
        assert_eq!(rust_base_tags("FROM rust:latest\n"), vec![(1, "latest")]);
    }

    #[test]
    fn quoted_value_only_matches_the_whole_key() {
        assert_eq!(
            quoted_value("channel = \"1.97.1\"", "channel"),
            Some("1.97.1")
        );
        assert_eq!(
            quoted_value("  channel=\"1.97.1\"", "channel"),
            Some("1.97.1")
        );
        assert_eq!(quoted_value("channel-override = \"x\"", "channel"), None);
        assert_eq!(quoted_value("profile = \"default\"", "channel"), None);
    }

    #[test]
    fn set_lockfile_versions_bumps_locals_only() {
        let lock = "# @generated\nversion = 4\n\n\
                    [[package]]\nname = \"adler2\"\nversion = \"2.0.1\"\n\
                    source = \"registry+https://example\"\nchecksum = \"abc\"\n\n\
                    [[package]]\nname = \"datum-core\"\nversion = \"0.1.0\"\n\
                    dependencies = [\n \"serde\",\n]\n";
        let out = set_lockfile_versions(lock, "0.2.0");
        // Top-of-file lockfile format version untouched.
        assert!(out.contains("version = 4\n"));
        // Registry crate untouched.
        assert!(out.contains("name = \"adler2\"\nversion = \"2.0.1\""));
        // Local crate bumped.
        assert!(out.contains("name = \"datum-core\"\nversion = \"0.2.0\""));
    }
}
