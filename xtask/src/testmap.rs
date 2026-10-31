//! `cargo xtask testmap` (da-fnt3v): PR CI runs only the tests a change can
//! reach, read off a map of which tests ran which functions.
//!
//! `index` turns one instrumented run (`scripts/testmap-build.sh`, a profile
//! directory per test) into the map; `select` reads a PR's diff against it
//! and writes the test names CI runs, or says why it runs everything.
//!
//! The selection diffs the MAP's commit against HEAD, so a changed line is
//! read in the map's own coordinates whatever moved since; the commits
//! between only make it select more.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// A map further behind the base than this is too old to trust. 25 is past the
/// worst lag vremya measured on the two-hour timer, 21 commits on 10-19, where
/// 10 sent 18 of 94 long PR runs to the whole suite (da-eqjzg, da-reck6). Keep
/// web/scripts/testmap.mjs's equal.
const MAX_BEHIND: usize = 25;
/// The branch the map is made on. An integration branch's freshness is
/// measured at the staging commit it was last synced from, so its own
/// commits are not staleness (da-47l15).
const STAGING: &str = "origin/staging";
/// The cohorts: ci.yml's two nextest steps.
const COHORTS: [&str; 2] = ["workspace", "e2e-full"];
/// The tests that read README.md, docs/ and datum.day's guide for agents as
/// data, which coverage cannot see: a change to a doc runs them (seconds) where
/// it would run none, so a drift between the docs and the binary is caught on
/// the PR (da-1nqv4).
const DOC_READERS: [&str; 3] = [
    "readme_anti_drift::readme_autogen_sections_in_sync",
    "readme_anti_drift::every_generated_block_has_markers_in_its_file",
    "readme_snippets::markdown_bash_snippets_run_against_datum_binary",
];
/// The test holding each published copy of the release key to the one built
/// in, and the files it reads, install.sh among them (da-mc8b1).
const RELEASE_KEY_READER: &str =
    "commands::install::tests::the_release_key_is_quoted_where_it_is_published";
const RELEASE_KEY_QUOTES: [&str; 3] =
    ["README.md", "web/static/install.sh", "web/static/agents.md"];

#[derive(Serialize, Deserialize)]
struct Map {
    commit: String,
    /// `[cohort, binary id, test name]`.
    tests: Vec<[String; 3]>,
    /// The distinct sets of tests that reach a function, by index.
    sets: Vec<Vec<u32>>,
    /// Per file, each function as `[first line, last line, set]`.
    files: BTreeMap<String, Vec<[u32; 3]>>,
    /// Tests that read the source tree as data, which coverage cannot see:
    /// those reaching a line [`tree_walks`] names. Every selection runs them.
    always: Vec<u32>,
}

pub fn run(args: Vec<String>) -> Result<()> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["index", prof, bin, out] => index(Path::new(prof), Path::new(bin), Path::new(out)),
        ["select", map, base, out] => select(Path::new(map), base, Path::new(out)),
        _ => bail!(
            "usage: cargo xtask testmap index PROF_DIR BIN_DIR OUT.json\n       \
             cargo xtask testmap select MAP.json BASE OUT_DIR"
        ),
    }
}

// ── index ───────────────────────────────────────────────────────────────

fn index(prof: &Path, bin_dir: &Path, out: &Path) -> Result<()> {
    let root = PathBuf::from(git(&["rev-parse", "--show-toplevel"])?.trim());
    let tools = llvm_tools()?;

    let mut dirs = Vec::new();
    for entry in fs::read_dir(prof).with_context(|| format!("reading {}", prof.display()))? {
        let dir = entry?.path();
        if let Ok(line) = fs::read_to_string(dir.join("test")) {
            let mut it = line.trim_end().splitn(3, '\t').map(str::to_string);
            let test = [(); 3].map(|_| it.next().unwrap_or_default());
            dirs.push((test, dir));
        }
    }
    dirs.sort();
    let tests: Vec<[String; 3]> = dirs.iter().map(|(t, _)| t.clone()).collect();
    if tests.is_empty() {
        bail!("no test profiles under {}", prof.display());
    }

    // Each test's functions, from its own directory's profiles: a function
    // ran when any of its counters is non-zero.
    let mut reached: HashMap<String, Vec<u32>> = HashMap::new();
    let mut profraws = Vec::new();
    for (_, dir) in &dirs {
        profraws.extend(raws(dir)?);
    }
    let next = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel();
    std::thread::scope(|s| -> Result<()> {
        let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
        for _ in 0..workers {
            let (tx, next, dirs, tools) = (tx.clone(), &next, &dirs, &tools);
            s.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some((_, dir)) = dirs.get(i) else { break };
                let _ = tx.send((i, ran(tools, dir)));
            });
        }
        drop(tx);
        for (i, names) in rx {
            for name in names? {
                reached.entry(name).or_default().push(i as u32);
            }
        }
        Ok(())
    })?;

    // Where each function is: one export over every binary, its ranges.
    let list = prof.join(".profraws");
    fs::write(
        &list,
        profraws
            .iter()
            .map(|p| format!("{}\n", p.display()))
            .collect::<String>(),
    )?;
    let all = prof.join(".all.profdata");
    let status = Command::new(tools.join("llvm-profdata"))
        .args(["merge", "--sparse", "-f"])
        .arg(&list)
        .arg("-o")
        .arg(&all)
        .status()?;
    if !status.success() {
        bail!("llvm-profdata merge of every test: {status}");
    }
    let objects = objects(prof, bin_dir)?;
    let mut cmd = Command::new(tools.join("llvm-cov"));
    cmd.args([
        "export",
        "--format=text",
        "--skip-expansions",
        "--skip-branches",
    ])
    .arg(format!("--instr-profile={}", all.display()))
    .arg(&objects[0]);
    for o in &objects[1..] {
        cmd.arg(format!("-object={}", o.display()));
    }
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let export: Export = serde_json::from_reader(BufReader::new(child.stdout.take().unwrap()))
        .context("reading llvm-cov export")?;
    let status = child.wait()?;
    if !status.success() {
        bail!("llvm-cov export: {status}");
    }

    let prefix = format!("{}/", root.display());
    let mut functions: BTreeMap<(String, u32, u32), BTreeSet<u32>> = BTreeMap::new();
    for f in export.data.into_iter().flat_map(|d| d.functions) {
        let Some(file) = f.filenames.first().and_then(|p| p.strip_prefix(&prefix)) else {
            continue;
        };
        let own = f.regions.iter().filter(|r| r.get(5) == Some(&0));
        let (Some(first), Some(last)) = (
            own.clone().filter_map(|r| r.first()).min(),
            own.filter_map(|r| r.get(2)).max(),
        ) else {
            continue;
        };
        let set = functions
            .entry((file.to_string(), *first as u32, *last as u32))
            .or_default();
        set.extend(reached.get(&f.name).into_iter().flatten());
    }

    let mut map = Map {
        commit: git(&["rev-parse", "HEAD"])?.trim().to_string(),
        tests,
        sets: Vec::new(),
        files: BTreeMap::new(),
        always: Vec::new(),
    };
    let mut ids: HashMap<Vec<u32>, u32> = HashMap::new();
    for ((file, first, last), set) in functions {
        let set: Vec<u32> = set.into_iter().collect();
        let id = *ids.entry(set.clone()).or_insert_with(|| {
            map.sets.push(set);
            map.sets.len() as u32 - 1
        });
        map.files.entry(file).or_default().push([first, last, id]);
    }
    let mut always = BTreeSet::new();
    for (file, fns) in &map.files {
        let text = fs::read_to_string(root.join(file)).unwrap_or_default();
        for line in tree_walks(file, &text) {
            for f in fns.iter().filter(|f| f[0] <= line && line <= f[1]) {
                let n = always.len();
                always.extend(&map.sets[f[2] as usize]);
                println!(
                    "always run: {} tests reach {file}:{line}, a walk of the tree",
                    always.len() - n
                );
            }
        }
    }
    map.always = always.into_iter().collect();

    let json = serde_json::to_string(&map)?;
    fs::write(out, &json).with_context(|| format!("writing {}", out.display()))?;
    let covered = map
        .files
        .values()
        .flatten()
        .filter(|f| !map.sets[f[2] as usize].is_empty());
    println!(
        "{}: {} tests, {} functions in {} files ({} reached by a test), {} sets, {} KiB, at {}",
        out.display(),
        map.tests.len(),
        map.files.values().map(Vec::len).sum::<usize>(),
        map.files.len(),
        covered.count(),
        map.sets.len(),
        json.len() / 1024,
        map.commit,
    );
    Ok(())
}

/// The lines of `file` holding a `read_dir(` in test code, in a file that
/// names `CARGO_MANIFEST_DIR`: a test file, or a source file past its first
/// `#[cfg(test)]`, as `db::write`'s guards are (da-o2vna).
fn tree_walks(file: &str, text: &str) -> Vec<u32> {
    if !text.contains("CARGO_MANIFEST_DIR") {
        return Vec::new();
    }
    let test_file = file.contains("/tests/") || file.ends_with("/tests.rs");
    let lines = text.lines().enumerate();
    lines
        .skip_while(|(_, l)| !test_file && !l.trim_start().starts_with("#[cfg(test)]"))
        .filter(|(_, l)| l.contains("read_dir("))
        .map(|(i, _)| i as u32 + 1)
        .collect()
}

#[derive(Deserialize)]
struct Export {
    data: Vec<ExportData>,
}

#[derive(Deserialize)]
struct ExportData {
    functions: Vec<ExportFn>,
}

#[derive(Deserialize)]
struct ExportFn {
    name: String,
    filenames: Vec<String>,
    /// `[line start, col start, line end, col end, count, file id, …]`.
    regions: Vec<Vec<u64>>,
}

fn raws(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let p = entry?.path();
        if p.extension().is_some_and(|e| e == "profraw") {
            out.push(p);
        }
    }
    Ok(out)
}

/// The functions one test's processes ran.
fn ran(tools: &Path, dir: &Path) -> Result<Vec<String>> {
    let raws = raws(dir)?;
    if raws.is_empty() {
        return Ok(Vec::new());
    }
    let out = Command::new(tools.join("llvm-profdata"))
        .args(["merge", "--sparse", "--text", "-o", "-"])
        .args(&raws)
        .stderr(Stdio::null())
        .output()?;
    if !out.status.success() {
        bail!("llvm-profdata merge {}: {}", dir.display(), out.status);
    }
    Ok(parse_text_profile(&String::from_utf8_lossy(&out.stdout)))
}

/// The names in `llvm-profdata merge --text` output with a non-zero counter.
/// A record is its name, then `# Func Hash:`, `# Num Counters:` and
/// `# Counter Values:`, each followed by its value(s), and a blank line.
fn parse_text_profile(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for record in text.split("\n\n") {
        let mut lines = record
            .lines()
            .skip_while(|l| l.starts_with(':') || l.is_empty());
        let Some(name) = lines.next().filter(|l| !l.starts_with('#')) else {
            continue;
        };
        let mut lines = lines.skip_while(|l| *l != "# Num Counters:").skip(1);
        let n: usize = lines.next().and_then(|l| l.parse().ok()).unwrap_or(0);
        let mut counters = lines
            .skip_while(|l| *l != "# Counter Values:")
            .skip(1)
            .take(n);
        if counters.any(|c| c.parse::<u64>().is_ok_and(|c| c > 0)) {
            out.push(name.to_string());
        }
    }
    out
}

/// Every binary a test ran: the test binaries nextest listed, and the
/// workspace's executables, which datum-cli's tests spawn.
fn objects(prof: &Path, bin_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = BTreeSet::new();
    for entry in fs::read_dir(prof)? {
        let p = entry?.path();
        if p.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with(".list-"))
        {
            let list: serde_json::Value = serde_json::from_str(&fs::read_to_string(&p)?)?;
            let suites = list["rust-suites"]
                .as_object()
                .context("nextest list: no rust-suites")?;
            out.extend(
                suites
                    .values()
                    .filter_map(|s| s["binary-path"].as_str())
                    .map(PathBuf::from),
            );
        }
    }
    for entry in fs::read_dir(bin_dir)? {
        let p = entry?.path();
        let exec = p.metadata().is_ok_and(|m| {
            use std::os::unix::fs::PermissionsExt;
            m.is_file() && m.permissions().mode() & 0o111 != 0
        });
        if exec && p.extension().is_none() {
            out.insert(p);
        }
    }
    if out.is_empty() {
        bail!("no binaries to export coverage from");
    }
    Ok(out.into_iter().collect())
}

/// The toolchain's own llvm-tools (`rustup component add llvm-tools`): a
/// system LLVM older than rustc's cannot read its profiles.
fn llvm_tools() -> Result<PathBuf> {
    let sysroot = output(Command::new("rustc").args(["--print", "sysroot"]))?;
    let vv = output(Command::new("rustc").arg("-vV"))?;
    let host = vv
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .context("rustc -vV: no host")?;
    let dir = Path::new(sysroot.trim())
        .join("lib/rustlib")
        .join(host)
        .join("bin");
    if !dir.join("llvm-profdata").exists() {
        bail!(
            "no llvm-profdata in {}: rustup component add llvm-tools",
            dir.display()
        );
    }
    Ok(dir)
}

// ── select ──────────────────────────────────────────────────────────────

fn select(map_path: &Path, base: &str, out: &Path) -> Result<()> {
    fs::create_dir_all(out)?;
    for f in ["FULL", "workspace.tests", "e2e-full.tests", "new.tests"] {
        let _ = fs::remove_file(out.join(f));
    }
    let full = |reasons: &[String]| -> Result<()> {
        println!("FULL SUITE:");
        for r in reasons {
            println!("  {r}");
        }
        fs::write(out.join("FULL"), reasons.join("\n") + "\n")?;
        // CI runs everything here; a can's gate runs these (scripts/mq-check.sh).
        fs::write(
            out.join("new.tests"),
            touched_tests(base)?
                .iter()
                .map(|n| format!("{n}\n"))
                .collect::<String>(),
        )?;
        Ok(())
    };

    let map: Map = match fs::read_to_string(map_path) {
        Ok(text) => serde_json::from_str(&text).context("reading the test map")?,
        Err(e) => return full(&[format!("no test map ({}: {e})", map_path.display())]),
    };
    println!("test map of {}: {} tests", map.commit, map.tests.len());

    let mut reasons = Vec::new();
    let synced = Command::new("git")
        .args(["merge-base", base, STAGING])
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let at = match synced {
        Some(sha) if sha != git(&["rev-parse", base])?.trim() => {
            println!("{base} left {STAGING} at {sha}: the map is measured there");
            sha
        }
        Some(_) => base.to_string(),
        None => {
            println!("no merge-base of {base} and {STAGING}: the map is measured at {base}");
            base.to_string()
        }
    };
    if !git_ok(&["merge-base", "--is-ancestor", &map.commit, &at]) {
        reasons.push(format!(
            "the map's commit {} is not an ancestor of {at}",
            map.commit
        ));
    } else {
        let behind: usize = git(&["rev-list", "--count", &format!("{}..{at}", map.commit)])?
            .trim()
            .parse()?;
        if behind > MAX_BEHIND {
            reasons.push(format!(
                "the map is {behind} commits behind {at} (more than {MAX_BEHIND})"
            ));
        }
    }
    let changed: Vec<String> = git(&[
        "diff",
        "--name-only",
        "--no-renames",
        &format!("{base}...HEAD"),
    ])?
    .lines()
    .map(str::to_string)
    .collect();
    if changed.is_empty() {
        reasons.push(format!("no file differs from {base}"));
    }
    for path in &changed {
        if let Some(why) = trigger(path) {
            reasons.push(format!("{path}: {why}"));
        } else if path.ends_with(".rs")
            && !path.contains("/tests/")
            && !map.files.contains_key(path.as_str())
        {
            // A source file new since the map, or one with no function in it,
            // reaches tests no coverage of it names; a test file's own tests
            // are found by name below.
            reasons.push(format!("{path}: no function of it is in the map"));
        } else if readers_in(&map, path).len() < readers_of(path).len() {
            reasons.push(format!(
                "{path}: a test that reads it is not in the map (one of {})",
                readers_of(path).join(", ")
            ));
        }
    }
    if !reasons.is_empty() {
        return full(&reasons);
    }

    let rust: Vec<&String> = changed.iter().filter(|p| p.ends_with(".rs")).collect();
    let mut picked: BTreeSet<u32> = BTreeSet::new();
    let mut take = |why: String, tests: &mut dyn Iterator<Item = u32>| {
        let n = picked.len();
        picked.extend(tests);
        println!("{why}: +{} tests", picked.len() - n);
    };
    for p in changed.iter().filter(|p| !p.ends_with(".rs")) {
        let readers = readers_in(&map, p);
        if readers.is_empty() {
            println!("{p}: no Rust test reads it");
        } else {
            take(
                format!("{p}: the tests that read it"),
                &mut readers.into_iter(),
            );
        }
    }
    let whole = |file: &str| -> Vec<u32> {
        let fns = map.files.get(file).map(Vec::as_slice).unwrap_or_default();
        fns.iter()
            .flat_map(|f| map.sets[f[2] as usize].clone())
            .collect()
    };

    // A file a test or source include_str!s is part of the includer.
    let root = PathBuf::from(git(&["rev-parse", "--show-toplevel"])?.trim());
    for includer in crate::walk_rs(&root.join("crates"))? {
        let text = fs::read_to_string(&includer)?;
        let rel = includer.strip_prefix(&root)?.to_string_lossy().into_owned();
        for target in includes(&rel, &text) {
            if rust.iter().any(|p| **p == target) {
                take(
                    format!("{rel} includes {target}"),
                    &mut whole(&rel).into_iter(),
                );
            }
        }
    }

    // Each changed line, in the map's coordinates: the functions around it,
    // or the whole file's tests for a line outside every function.
    if rust.is_empty() {
        println!("no Rust file changed");
    }
    let mut args = vec![
        "diff".to_string(),
        "-U0".into(),
        "--no-renames".into(),
        map.commit.clone(),
        "HEAD".into(),
        "--".into(),
    ];
    args.extend(rust.iter().map(|p| p.to_string()));
    let diff = if rust.is_empty() {
        String::new()
    } else {
        git(&args.iter().map(String::as_str).collect::<Vec<_>>())?
    };
    for (file, lines) in changed_lines(&diff, Side::Old) {
        let fns = map.files.get(&file).map(Vec::as_slice).unwrap_or_default();
        let source = git(&["show", &format!("{}:{file}", map.commit)]).unwrap_or_default();
        let mut seen = BTreeSet::new();
        let mut outside = None;
        for line in lines {
            let around: Vec<&[u32; 3]> = fns
                .iter()
                .filter(|f| f[0] <= line && line <= f[1])
                .collect();
            if around.is_empty() {
                outside.get_or_insert(line);
            }
            for f in around {
                if seen.insert(*f) {
                    let sig = source.lines().nth(f[0] as usize - 1).unwrap_or("").trim();
                    take(
                        format!("{file}:{line} in `{sig}` ({}–{})", f[0], f[1]),
                        &mut map.sets[f[2] as usize].iter().copied(),
                    );
                }
            }
        }
        if let Some(line) = outside {
            take(
                format!("{file}:{line} is outside every function: the whole file's tests"),
                &mut whole(&file).into_iter(),
            );
        }
    }
    take(
        "tests that read the tree".into(),
        &mut map.always.iter().copied(),
    );

    let named = touched_tests(base)?;

    // nextest's positional filters, one argument a test (a filterset is ONE
    // argument, and Linux caps one at 128 KiB): `<cohort>.tests` go after
    // `-- --exact`, `new.tests` before it, where a bare name matches as a
    // substring wherever the new test sits. Either matching selects.
    let mut names: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for i in &picked {
        let [cohort, _, name] = &map.tests[*i as usize];
        names.entry(cohort).or_default().insert(name);
    }
    let lines = |names: &mut dyn Iterator<Item = &str>| -> String {
        names.map(|n| format!("{n}\n")).collect()
    };
    for cohort in COHORTS {
        let names = names.remove(cohort).unwrap_or_default();
        println!("{cohort}: {} tests", names.len());
        let list = lines(&mut names.into_iter());
        print!(
            "{}",
            list.lines().map(|n| format!("  {n}\n")).collect::<String>()
        );
        fs::write(out.join(format!("{cohort}.tests")), list)?;
    }
    fs::write(
        out.join("new.tests"),
        lines(&mut named.iter().map(String::as_str)),
    )?;
    Ok(())
}

/// The tests the change adds or edits, by name: a new one is in no map.
fn touched_tests(base: &str) -> Result<BTreeSet<String>> {
    let root = PathBuf::from(git(&["rev-parse", "--show-toplevel"])?.trim());
    let diff = git(&[
        "diff",
        "-U0",
        "--no-renames",
        &format!("{base}...HEAD"),
        "--",
        "*.rs",
    ])?;
    let mut named = BTreeSet::new();
    for (file, lines) in changed_lines(&diff, Side::New) {
        let text = fs::read_to_string(root.join(&file)).unwrap_or_default();
        for name in tests_touched(&text, &lines) {
            println!("{file}: the PR adds or changes test `{name}`");
            named.insert(name);
        }
    }
    Ok(named)
}

/// Why a changed path runs the whole suite, if it does.
fn trigger(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if matches!(name, "Cargo.toml" | "Cargo.lock") {
        return Some("a manifest: every crate's build may change");
    }
    if name == "build.rs" {
        return Some("a build script");
    }
    if path.starts_with(".forgejo/") || path.starts_with(".cargo/") || path == "rust-toolchain.toml"
    {
        return Some("CI or the toolchain");
    }
    if path.starts_with("xtask/")
        || path.starts_with("scripts/testmap-")
        || path == "scripts/mq-check.sh"
    {
        return Some("the gate or this selection itself");
    }
    if let Some(rest) = path.strip_prefix("crates/") {
        if rest.contains("/tests/support/") {
            return Some("shared test support");
        }
        if path.starts_with("crates/datum-core/src/migrations/") {
            return Some("a migration, which every database open runs");
        }
        if !path.ends_with(".rs") {
            return Some("a file under crates/ that is not Rust, which a test may read or include");
        }
        return None;
    }
    if path.ends_with(".rs") {
        return Some("Rust outside crates/");
    }
    let unread = ["web/", "docs/", ".github/"]
        .iter()
        .any(|d| path.starts_with(d))
        || path.ends_with(".md")
        || path == ".gitignore";
    (!unread).then_some("a file outside crates/ that a test may read (kits/, tests/, scripts/)")
}

/// What [`DOC_READERS`] read.
fn is_doc(path: &str) -> bool {
    path == "README.md"
        || path == "web/static/agents.md"
        || (path.starts_with("docs/") && path.ends_with(".md"))
}

/// The tests that read a file outside crates/ as data.
fn readers_of(path: &str) -> Vec<&'static str> {
    let mut names = Vec::new();
    if is_doc(path) {
        names.extend(DOC_READERS);
    }
    if RELEASE_KEY_QUOTES.contains(&path) {
        names.push(RELEASE_KEY_READER);
    }
    names
}

/// The map's indices of [`readers_of`] a path.
fn readers_in(map: &Map, path: &str) -> Vec<u32> {
    let names = readers_of(path);
    (0..map.tests.len() as u32)
        .filter(|&i| names.contains(&map.tests[i as usize][2].as_str()))
        .collect()
}

#[derive(Clone, Copy)]
enum Side {
    Old,
    New,
}

/// Per file, the changed lines of a `git diff -U0` on one side. A pure
/// insertion changes no old line, so it marks the two old lines around it.
fn changed_lines(diff: &str, side: Side) -> BTreeMap<String, Vec<u32>> {
    let mut out: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    let mut file = None;
    for line in diff.lines() {
        if let Some(p) = line
            .strip_prefix("--- a/")
            .filter(|_| matches!(side, Side::Old))
        {
            file = Some(p.to_string());
        } else if let Some(p) = line
            .strip_prefix("+++ b/")
            .filter(|_| matches!(side, Side::New))
        {
            file = Some(p.to_string());
        } else if line.starts_with("--- ") || line.starts_with("+++ ") {
            if line.ends_with("/dev/null")
                && line.starts_with(if matches!(side, Side::Old) {
                    "--- "
                } else {
                    "+++ "
                })
            {
                file = None;
            }
        } else if let (Some(f), Some(hunk)) = (&file, line.strip_prefix("@@ ")) {
            let mark = if matches!(side, Side::Old) { '-' } else { '+' };
            let Some(range) = hunk.split(' ').take(2).find_map(|w| w.strip_prefix(mark)) else {
                continue;
            };
            let (start, len) = match range.split_once(',') {
                Some((s, l)) => (s.parse().unwrap_or(0), l.parse().unwrap_or(0)),
                None => (range.parse().unwrap_or(0), 1),
            };
            let lines = out.entry(f.clone()).or_default();
            if len == 0 {
                lines.extend([start.max(1), start + 1]);
            } else {
                lines.extend(start..start + len);
            }
        }
    }
    out
}

/// The paths `include_str!` / `include_bytes!` in `file` (repo-relative)
/// name, resolved against its directory.
fn includes(file: &str, text: &str) -> Vec<String> {
    let dir = file.rsplit_once('/').map_or("", |(d, _)| d);
    let mut out = Vec::new();
    for m in ["include_str!(\"", "include_bytes!(\""] {
        for (i, _) in text.match_indices(m) {
            let rest = &text[i + m.len()..];
            let Some(end) = rest.find('"') else { continue };
            let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
            for p in rest[..end].split('/') {
                match p {
                    ".." => {
                        parts.pop();
                    }
                    "." | "" => {}
                    p => parts.push(p),
                }
            }
            out.push(parts.join("/"));
        }
    }
    out
}

/// The test functions in `text` that hold one of `lines`: a test runs from
/// its `#[…test…]` attribute to the next test's.
fn tests_touched(text: &str, lines: &[u32]) -> Vec<String> {
    let src: Vec<&str> = text.lines().collect();
    let mut starts = Vec::new();
    for (i, l) in src.iter().enumerate() {
        let l = l.trim_start();
        if l.starts_with("#[") && (l.starts_with("#[test]") || l.contains("::test")) {
            let name = src[i + 1..].iter().find_map(|l| {
                let after = l.split("fn ").nth(1)?;
                let name: String = after
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                (!name.is_empty()).then_some(name)
            });
            if let Some(name) = name {
                starts.push((i as u32 + 1, name));
            }
        }
    }
    let mut out = Vec::new();
    for (k, (start, name)) in starts.iter().enumerate() {
        let end = starts.get(k + 1).map_or(u32::MAX, |(s, _)| s - 1);
        if lines.iter().any(|l| (*start..=end).contains(l)) {
            out.push(name.clone());
        }
    }
    out
}

fn git(args: &[&str]) -> Result<String> {
    output(Command::new("git").args(args))
}

fn git_ok(args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn output(cmd: &mut Command) -> Result<String> {
    let out = cmd
        .stderr(Stdio::inherit())
        .output()
        .with_context(|| format!("running {cmd:?}"))?;
    if !out.status.success() {
        bail!("{cmd:?}: {}", out.status);
    }
    Ok(String::from_utf8(out.stdout)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_profile_names_the_functions_that_ran() {
        let text = "_Rran\n# Func Hash:\n1\n# Num Counters:\n2\n# Counter Values:\n0\n3\n\n\
                    _Ridle\n# Func Hash:\n2\n# Num Counters:\n1\n# Counter Values:\n0\n";
        assert_eq!(parse_text_profile(text), ["_Rran"]);
    }

    #[test]
    fn a_hunk_marks_its_lines_and_an_insertion_its_neighbours() {
        let diff = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -10,2 +10,3 @@ fn f(a: -1, b: +2)\n-a\n\
                    @@ -20,0 +22 @@\n+b\n@@ -30 +32,0 @@\n-c\n";
        assert_eq!(changed_lines(diff, Side::Old)["x.rs"], [10, 11, 20, 21, 30]);
        assert_eq!(
            changed_lines(diff, Side::New)["x.rs"],
            [10, 11, 12, 22, 32, 33]
        );
    }

    #[test]
    fn a_new_file_has_no_old_lines() {
        let diff = "--- /dev/null\n+++ b/new.rs\n@@ -0,0 +1,2 @@\n+a\n+b\n";
        assert!(changed_lines(diff, Side::Old).is_empty());
        assert_eq!(changed_lines(diff, Side::New)["new.rs"], [1, 2]);
    }

    #[test]
    fn includes_resolve_against_the_including_file() {
        let text = r#"const A: &str = include_str!("../../datum-commands/src/config.rs");"#;
        assert_eq!(
            includes("crates/datum-cli/tests/env.rs", text),
            ["crates/datum-commands/src/config.rs"]
        );
    }

    #[test]
    fn a_changed_line_names_the_test_around_it() {
        let text = "#[test]\nfn one() {\n}\n\n#[tokio::test]\nasync fn two() {\n    x();\n}\n";
        assert_eq!(tests_touched(text, &[7]), ["two"]);
        assert_eq!(tests_touched(text, &[2]), ["one"]);
        let long =
            "#[test]\n#[should_panic]\n/// a\n/// b\n/// c\n/// d\n/// e\n/// f\nfn far<T>() {}\n";
        assert_eq!(tests_touched(long, &[9]), ["far"]);
    }

    #[test]
    fn a_walk_of_the_tree_is_found_in_test_code_wherever_it_sits() {
        let walk = "const R: &str = env!(\"CARGO_MANIFEST_DIR\");\nfn f() { read_dir(R); }\n";
        assert_eq!(tree_walks("crates/a/tests/t.rs", walk), [2]);
        assert_eq!(tree_walks("crates/a/src/db/tests.rs", walk), [2]);
        assert!(tree_walks("crates/a/src/lib.rs", walk).is_empty());

        let module =
            format!("fn prod() {{ read_dir(x); }}\n#[cfg(test)]\nmod tests {{\n{walk}}}\n");
        assert_eq!(tree_walks("crates/a/src/lib.rs", &module), [5]);

        let quoted = format!("const A: &str = \"#[cfg(test)]\";\n{walk}");
        assert!(tree_walks("crates/a/src/lib.rs", &quoted).is_empty());

        let tempdir = "#[cfg(test)]\nfn f() { read_dir(tmp); }\n";
        assert!(tree_walks("crates/a/src/lib.rs", tempdir).is_empty());
    }

    #[test]
    fn a_doc_selects_the_tests_that_read_it_and_no_other() {
        let test = |name: &str| ["workspace".into(), "datum-cli::main".into(), name.into()];
        let mut tests = vec![
            test("readme_snippets::parser_tests::tokenize_strips_inline_comments"),
            test("readme_anti_drift::render_adapters_round_trip_command_names"),
            test(RELEASE_KEY_READER),
        ];
        tests.extend(DOC_READERS.map(test));
        let map = Map {
            commit: String::new(),
            tests,
            sets: Vec::new(),
            files: BTreeMap::new(),
            always: Vec::new(),
        };
        let names = |p| -> Vec<String> {
            readers_in(&map, p)
                .iter()
                .map(|&i| map.tests[i as usize][2].clone())
                .collect()
        };
        for p in ["README.md", "web/static/agents.md"] {
            assert!(trigger(p).is_none(), "{p}");
            assert_eq!(names(p)[0], RELEASE_KEY_READER, "{p}");
            assert_eq!(names(p)[1..], DOC_READERS, "{p}");
        }
        assert_eq!(names("docs/grammar-tokens.md"), DOC_READERS);
        assert_eq!(names("web/static/install.sh"), [RELEASE_KEY_READER]);
        for p in ["AGENTS.md", "web/README.md", "docs/parity-audit/notes.txt"] {
            assert!(names(p).is_empty(), "{p}");
        }
    }

    /// What the two tests read, as they say it. A file a test starts reading
    /// here fails this before it goes unselected.
    #[test]
    fn the_files_named_are_the_ones_the_tests_read() {
        let install = include_str!("../../crates/datum-commands/src/commands/install.rs");
        assert!(install.contains(&format!("for file in {RELEASE_KEY_QUOTES:?}")));
        let test = RELEASE_KEY_READER.rsplit("::").next().unwrap();
        assert!(install.contains(&format!("fn {test}()")));
        let snippets = include_str!("../../crates/datum-cli/tests/readme_snippets.rs");
        let docs = snippets.split("SNIPPET_DOCS: &[&str] = &[").nth(1).unwrap();
        for doc in docs
            .split("];")
            .next()
            .unwrap()
            .split('"')
            .skip(1)
            .step_by(2)
        {
            assert!(is_doc(doc), "{doc}");
        }
    }

    #[test]
    fn the_triggers_that_run_everything() {
        for p in [
            "Cargo.lock",
            "crates/datum-cli/Cargo.toml",
            "crates/datum-cli/build.rs",
            "crates/datum-core/src/schema.sql",
            "crates/datum-core/src/migrations/v1.rs",
            "crates/datum-cli/tests/support/mod.rs",
            ".forgejo/workflows/ci.yml",
            "xtask/src/testmap.rs",
            "scripts/mq-check.sh",
            "kits/sample.toml",
            "tests/fixtures/shell-split.json",
        ] {
            assert!(trigger(p).is_some(), "{p}");
        }
        for p in [
            "crates/datum-core/src/db.rs",
            "web/src/app.ts",
            "docs/x.md",
            "README.md",
        ] {
            assert!(trigger(p).is_none(), "{p}");
        }
    }
}
