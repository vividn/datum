//! Workspace task runner — invoked via `cargo xtask <task>`.
//!
//! Tasks live here rather than in `build.rs` files so that they don't
//! run on every `cargo build` and so that they can depend on workspace
//! crates (a build script can't).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use datum_parse::help::{render_roff, COMMANDS};
use xtask::{autogen_section, splice_autogen, README_BLOCKS};

mod testmap;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let task = args.next().unwrap_or_default();
    match task.as_str() {
        "manpages" => manpages(args.collect()),
        "readme" => readme(),
        "check" => check(args.collect()),
        "testmap" => testmap::run(args.collect()),
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
    eprintln!("  testmap index|select  per-test coverage map, and the PR tests it selects");
}

fn readme() -> Result<()> {
    let root = workspace_root()?;
    let readme_path = root.join("README.md");
    let original = fs::read_to_string(&readme_path)
        .with_context(|| format!("reading {}", readme_path.display()))?;
    let mut updated = original.clone();
    for block in README_BLOCKS {
        updated = splice_autogen(&updated, block, &autogen_section(block)?)?;
    }
    if updated == original {
        eprintln!("README.md already in sync");
        return Ok(());
    }
    fs::write(&readme_path, updated)
        .with_context(|| format!("writing {}", readme_path.display()))?;
    eprintln!("wrote {}", readme_path.display());
    Ok(())
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
/// - 4000 (2026-09-02, vremya, vividn's decision) — the gate had been RED
///   ON MAIN since `148c729` (2026-08-13), where `migrations.rs` already
///   stood at 3598 LOC. Every PR opened since then failed `Honesty gates`
///   through no fault of its own, including six branches of real work on
///   2026-09-02. A gate that fails on `main` is not a gate; it is a tax
///   on everyone downstream of it, and it is invisible to the author who
///   trips it because their own diff is innocent.
///   vividn's call, made explicitly rather than casually: bump now to
///   unblock, carve separately. 4000 rather than 3600 because
///   `migrations.rs` gains a block per schema version by construction —
///   a cap set just above today's size would be red again on the very
///   next migration, and one is imminent (the Precision→Uncertainty
///   rename). Carving is tracked as its own bead so this bump does not
///   quietly become the resolution.
/// - 3500 (2026-09-03, `da-tnbn`) — back down, because the carve landed.
///   `migrations.rs` is now `migrations/`: a registry module, a runner,
///   the shared rebuild helpers, and one body module per decade of
///   schema versions. A new migration adds a function to the current era
///   module and an entry to the registry, and crossing the next multiple
///   of ten starts a new file — so no file in that tree grows without
///   bound and the cap is no longer counting down towards a known
///   collision. 3500 restores the pre-#92 number; the largest remaining
///   file is `commands/check.rs` at ~3.4k, which is the next real
///   conversation rather than a reason to keep the headroom.
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
        "no-slug-keyed-writes" => check_no_slug_keyed_writes(),
        "toolchain-pin" => check_toolchain_pin(),
        "" => bail!(
            "check requires a subcommand: all | lint | no-cfg-any | no-r2-stubs | \
             no-legacy-verbs | max-file-size | no-slug-keyed-writes | toolchain-pin"
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
/// The first four are the pre-v1 honesty gates (`da-8y2q.7`), then
/// `no-slug-keyed-writes` (da-anmaz); the last keeps the Rust version
/// single-sourced.
const LINT_STEPS: &[Step] = &[
    Step::Gate("no-cfg-any", "crates/", check_no_cfg_any),
    Step::Gate("no-r2-stubs", "crates/", check_no_r2_stubs),
    Step::Gate("no-legacy-verbs", "crates/", check_no_legacy_verbs),
    Step::Gate("max-file-size", "crates/", check_max_file_size),
    Step::Gate(
        "no-slug-keyed-writes",
        "crates/*/src outside tests and migrations",
        check_no_slug_keyed_writes,
    ),
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
    "wasm-check — the wasm32-wasip1 CLI module, web/scripts/build-cli-wasm.mjs \
     (needs the pinned wasi-sdk and binaryen)",
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
        if path.iter().any(|p| p == "migrations") {
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
/// pre-existing offenders (parse.rs, check.rs, …) sit just
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

/// Locks in da-anmaz: an `UPDATE entries` or `DELETE FROM entries` whose
/// `WHERE` names `slug`. A keep-both pair shares its slug, so such a write
/// lands on both rows — found and fixed one at a time in undo (#442),
/// `raw_overwrite_mutables` (da-sy8ml) and `check --fix` (da-3qvnq). Test
/// code may seed rows by slug, so test-gated items, `tests.rs` files and
/// `migrations/` (which rewrite every row) are carved out.
///
/// The scan reads string literals: a `format!` fragment after `WHERE` is
/// caught by its NAME (`{BY_SLUG_OR_HID_SQL}`), and SQL assembled any other
/// way is out of its reach.
fn check_no_slug_keyed_writes() -> Result<()> {
    let hits = scan_rs_files_with_path(|path, content| {
        let test_or_migration = path.iter().any(|c| c == "migrations")
            || path.file_name().is_some_and(|n| n == "tests.rs");
        if test_or_migration || !path.iter().any(|c| c == "src") {
            return Vec::new();
        }
        let p = path.display().to_string();
        match slug_keyed_writes(content) {
            Ok(lines) => lines.into_iter().map(|l| (format!("{p}:{l}"), l)).collect(),
            Err(l) => vec![(
                format!("{p}:{l}: the test item opened here never closes, so the rest of the file went unread — fix the brace count in `xtask/src/main.rs`"),
                l,
            )],
        }
    })?;
    if hits.is_empty() {
        return Ok(());
    }
    let lines: Vec<String> = hits.iter().map(|(h, _)| format!("  {h}")).collect();
    bail!(
        "found {} UPDATE/DELETE on entries keyed on slug:\n{}\n\
         a keep-both pair shares its slug, so the write lands on both rows; \
         address the row by hid.",
        hits.len(),
        lines.join("\n")
    );
}

/// The 1-based lines, outside test-gated items, where an `UPDATE entries`
/// or `DELETE FROM entries` string literal names `slug` after its `WHERE`.
/// The literal is read to its closing quote, so a statement split over
/// lines with `\` is read whole, and its arguments are not. `Err(line)`
/// when the test item opened at `line` never closes: a miscounted brace
/// must not pass the rest of the file unread.
fn slug_keyed_writes(content: &str) -> std::result::Result<Vec<usize>, usize> {
    let mut out: Vec<usize> = Vec::new();
    let mut depth = 0i32; // brace depth inside a test-gated item
    let mut gated = None; // the line of a test cfg whose item is still open
    let mut offset = 0usize;
    let masked = code_only(content);
    for (i, line) in content.split_inclusive('\n').enumerate() {
        let start = offset;
        offset += line.len();
        let code = strip_line_comment(line);
        // Attributes and braces read with literals and comments blanked: a
        // `"{"` in a test must not keep its module open to the end of the file.
        let bare = &masked[start..offset];
        let t = bare.trim_start();
        if depth == 0
            && ["#[cfg(test)]", "#[cfg(all(test", "#[cfg(any(test"]
                .iter()
                .any(|g| t.starts_with(g))
        {
            gated = Some(i + 1);
        }
        if gated.is_some() {
            for c in bare.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                if depth <= 0 && (c == '}' || c == ';') {
                    depth = 0;
                    gated = None;
                    break;
                }
            }
            continue;
        }
        let upper = code.to_ascii_uppercase();
        let found = ["UPDATE ENTRIES", "DELETE FROM ENTRIES"].iter().any(|n| {
            upper.match_indices(n).any(|(at, _)| {
                let end = at + n.len();
                !upper[end..].starts_with(is_word)
                    && where_names_slug(content, start + at, start + end)
            })
        });
        if found {
            out.push(i + 1);
        }
    }
    match gated {
        Some(line) => Err(line),
        None => Ok(out),
    }
}

/// `content` with the insides of every string and char literal, and every
/// comment, blanked to spaces — the same length, newlines kept, so a byte
/// offset means the same place in both.
fn code_only(content: &str) -> String {
    let b = content.as_bytes();
    let mut out = b.to_vec();
    let mut blank = |from: usize, to: usize| {
        for c in &mut out[from..to.min(b.len())] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    };
    let find = |from: usize, pat: &str| content[from..].find(pat).map_or(b.len(), |k| from + k);
    let mut i = 0;
    while i < b.len() {
        let end = match (b[i], b.get(i + 1)) {
            (b'/', Some(b'/')) => find(i, "\n"),
            (b'/', Some(b'*')) => find(i + 2, "*/") + 2,
            (b'"', _) => {
                let hashes = content[..i].len() - content[..i].trim_end_matches('#').len();
                if content[..i - hashes].ends_with('r') {
                    find(i + 1, &format!("\"{}", "#".repeat(hashes))) + 1 + hashes
                } else {
                    let mut j = i + 1;
                    while j < b.len() && b[j] != b'"' {
                        j += if b[j] == b'\\' { 2 } else { 1 };
                    }
                    j + 1
                }
            }
            (b'\'', Some(b'\\')) => find(i + 3, "'") + 1,
            (b'\'', _) => {
                // `'x'` is a char; `'a` with no closing quote is a lifetime.
                let len = content[i + 1..].chars().next().map_or(1, char::len_utf8);
                if b.get(i + 1 + len) == Some(&b'\'') {
                    i + 2 + len
                } else {
                    i + 1
                }
            }
            _ => i + 1,
        };
        if end > i + 1 {
            blank(i, end);
        }
        i = end.max(i + 1);
    }
    String::from_utf8(out).expect("only ASCII bytes and whole chars are replaced")
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether the SQL from `from` to the end of the string literal that opens
/// before `at` (or to its statement's `;`) names `slug` after a `WHERE`: as
/// a column, or as the name of a `{…}` fragment a `format!` puts there.
fn where_names_slug(content: &str, at: usize, from: usize) -> bool {
    let rest = &content[from..];
    // A raw string ends at `"` and its hashes, and has no escapes.
    let open = content[..at].rfind('"').unwrap_or(0);
    let hashes = content[..open].len() - content[..open].trim_end_matches('#').len();
    let raw = content[..open - hashes].ends_with('r');
    let close = format!("\"{}", "#".repeat(hashes));
    let mut escaped = false;
    let mut end = rest.len();
    for (i, c) in rest.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' && !raw {
            escaped = true;
        } else if c == ';' || (c == '"' && (!raw || rest[i..].starts_with(&close))) {
            end = i;
            break;
        }
    }
    let sql = rest[..end].to_ascii_uppercase();
    let Some(w) = sql
        .match_indices("WHERE")
        .map(|(i, _)| i)
        .find(|&i| !sql[..i].ends_with(is_word) && !sql[i + 5..].starts_with(is_word))
    else {
        return false;
    };
    let clause = &sql[w..];
    clause.split(|c: char| !is_word(c)).any(|t| t == "SLUG")
        || clause.split('{').skip(1).any(|f| {
            f.split('}')
                .next()
                .is_some_and(|name| name.contains("SLUG"))
        })
}

/// Keeps the Rust version single-sourced. `rust-toolchain.toml` decides
/// it; a Dockerfile's `FROM rust:<tag>` is only a cache hint, because
/// rustup obeys the pin whatever the tag says. That is exactly what makes
/// a stale tag dangerous rather than loud: the image still builds, so
/// nothing goes red, and the tag sat at `1.85` long after the workspace
/// moved on. The gate is the price of letting the version appear twice —
/// remove it and the second copy is free to drift again.
///
/// Fails if the pin is unreadable, if Dockerfiles exist but none names a
/// `rust:` base (a gate that finds nothing must not pass), or if any tag
/// disagrees. With no Dockerfile at all the pin is the only copy, so there
/// is nothing to drift (the last one went with the relay, da-i7hig).
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
    if dockerfiles.is_empty() {
        return Ok(());
    }

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

    /// The lines `slug_keyed_writes` refuses in `src`, as one would find them.
    fn hits(src: &str) -> Vec<usize> {
        slug_keyed_writes(src).expect("every test item closes")
    }

    #[test]
    fn slug_keyed_writes_red_in_production_green_in_tests() {
        // A planted write in a non-test function is refused, on its line.
        let planted = "fn f(c: &Connection) {\n    \
                       c.execute(\"UPDATE entries SET verb = ?2 WHERE slug = ?1\", p)?;\n}\n";
        assert_eq!(hits(planted), vec![2]);
        // A DELETE, `OR hid` beside it, and a WHERE on the literal's next line.
        assert_eq!(
            hits("x(\"DELETE FROM entries WHERE slug = ?1 OR hid = ?1\");\n"),
            vec![1]
        );
        assert_eq!(
            hits("x(\"UPDATE entries SET a = 1 \\\n   WHERE slug = ?1\");\n"),
            vec![1]
        );
        // A `format!` fragment named for the slug (lookup.rs's set_metadata, deleted).
        assert_eq!(
            hits("x(&format!(\"UPDATE entries SET a = ?2 WHERE {BY_SLUG_OR_HID_SQL}\"));\n"),
            vec![1]
        );
        // A raw string reads past a quoted identifier to its own end.
        assert_eq!(
            hits("x(r#\"UPDATE entries SET \"data\" = ?2 WHERE slug = ?1\"#);\n"),
            vec![1]
        );
        // The same write in a test-gated module or fn is test code.
        let in_tests = format!("#[cfg(test)]\nmod tests {{\n{planted}}}\n");
        assert_eq!(hits(&in_tests), Vec::<usize>::new());
        for cfg in [
            "all(test, feature = \"native\")",
            "any(test, feature = \"test-util\")",
        ] {
            assert_eq!(
                hits(&format!("#[cfg({cfg})]\n{planted}")),
                Vec::<usize>::new()
            );
        }
        // ...and the test region ends with its item.
        assert_eq!(hits(&format!("{in_tests}{planted}")), vec![8]);
        assert_eq!(hits(&format!("#[cfg(test)]\nuse x;\n{planted}")), vec![4]);
        // Braces in a test's literals and comments don't hold its module open.
        let noisy = "#[cfg(test)]\nmod tests {\n    let s = (\"{\", '{', '\\'', r#\"\"{\"#); // {\n    /* { */ fn g<'a>() {}\n}\n";
        assert_eq!(hits(&format!("{noisy}{planted}")), vec![7]);
        // A test item whose braces never close is an error, not a pass.
        assert_eq!(
            slug_keyed_writes(&format!("#[cfg(test)]\nmod tests {{\n{planted}")),
            Err(1)
        );
    }

    #[test]
    fn slug_keyed_writes_reads_only_the_where_of_the_literal() {
        // Writing the slug, keyed by hid, is what the fixes do.
        assert!(hits("x(\"UPDATE entries SET slug = ?2 WHERE hid = ?1\");\n").is_empty());
        // The slug as an argument after the literal is not its key.
        assert!(
            hits("x(\"UPDATE entries SET a = ?1 WHERE hid = ?2\", params![slug, hid]);\n")
                .is_empty()
        );
        // Another table, a column merely containing `slug`, a comment, and
        // `where` inside a word before the real WHERE.
        assert!(hits("x(\"DELETE FROM entries_fts WHERE slug = ?1\");\n").is_empty());
        assert!(hits("x(\"UPDATE entries SET a = 1 WHERE old_slug = ?1\");\n").is_empty());
        assert!(hits("// UPDATE entries SET a = 1 WHERE slug = ?1\n").is_empty());
        assert!(
            hits("x(\"UPDATE entries SET n = 'elsewhere slug' WHERE hid = ?1\");\n").is_empty()
        );
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
}
