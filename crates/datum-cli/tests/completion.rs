//! Shell tab completion (da-bu09u): `datum __complete` answers each
//! position from the grammar and the db, writes nothing, never errors; and
//! the scripts `datum completions <shell>` prints parse.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use tempfile::TempDir;

struct Env {
    tmp: TempDir,
}

impl Env {
    /// A db with a switch field, a field with a declared key, a field with
    /// a space in its name, and a user alias.
    fn new() -> Self {
        let env = Self {
            tmp: tempfile::tempdir().expect("create tmpdir"),
        };
        env.ok(&["track", "mood", "--verb", "switch", "--state", "calm"]);
        env.ok(&["switch", "mood", "happy"]);
        env.ok(&["track", "tea", "--verb", "occur", "--key", "cups="]);
        env.ok(&["occur", "tea", "cups=2"]);
        env.ok(&["occur", "my field"]);
        env.ok(&["alias", "set", "vbp", "note word pos=verb"]);
        env
    }

    fn db(&self) -> PathBuf {
        self.tmp.path().join("t.db")
    }

    fn cmd(&self) -> Command {
        let p = self.tmp.path();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_datum"));
        cmd.env("DATUM_DB", self.db())
            .env("DATUM_HOME", p.join("data"))
            .env("DATUM_CONFIG_HOME", p.join("config"))
            .env("DATUM_COLOR", "0");
        cmd
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.cmd().args(args).output().expect("spawn datum");
        assert!(
            out.status.success(),
            "datum {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// `__complete` for the last word of `line` (a trailing space is an
    /// empty word): its values, having checked it exits 0 and is silent
    /// on stderr.
    fn complete(&self, line: &str) -> Vec<String> {
        complete_with(self.cmd(), line)
            .lines()
            .map(|l| l.split('\t').next().unwrap().to_string())
            .collect()
    }
}

fn complete_with(mut cmd: Command, line: &str) -> String {
    let mut words: Vec<&str> = line.split(' ').collect();
    words.insert(0, "datum");
    let cword = (words.len() - 1).to_string();
    let out: Output = cmd
        .arg("__complete")
        .arg(&cword)
        .arg("--")
        .args(&words)
        .output()
        .expect("spawn datum");
    assert!(
        out.status.success(),
        "__complete {line:?} exited {:?}",
        out.status
    );
    assert!(
        out.stderr.is_empty(),
        "__complete {line:?} printed to stderr"
    );
    String::from_utf8(out.stdout).unwrap()
}

fn has(found: &[String], want: &str) -> bool {
    found.iter().any(|f| f == want)
}

#[test]
fn the_first_word_offers_commands_aliases_and_fields() {
    let env = Env::new();
    let found = env.complete("");
    for want in ["occur", "end", "mood", "tea", "vbp", "completions"] {
        assert!(has(&found, want), "{want} missing from {found:?}");
    }
    assert!(!has(&found, "__complete"), "the reader is hidden");
    assert_eq!(env.complete("occ"), ["occur"], "narrowed to the prefix");
}

#[test]
fn a_command_with_subcommands_offers_them() {
    let found = Env::new().complete("spec ");
    assert!(has(&found, "show") && has(&found, "set"), "{found:?}");
}

#[test]
fn a_dash_offers_the_commands_flags() {
    let found = Env::new().complete("occur -");
    assert!(has(&found, "--time") && has(&found, "-t"), "{found:?}");
    assert!(has(&found, "--db"), "the global flags too: {found:?}");
}

#[test]
fn db_offers_the_known_databases() {
    let env = Env::new();
    env.ok(&["--db", "other", "occur", "x"]);
    assert!(has(&env.complete("--db "), "other"));
    assert!(has(&env.complete("tail --db "), "other"));
}

#[test]
fn switch_offers_the_fields_states_then_its_keys() {
    let env = Env::new();
    let found = env.complete("switch mood ");
    assert!(has(&found, "calm"), "the spec's state: {found:?}");
    assert!(has(&found, "happy"), "a logged state: {found:?}");
    // The field-first form is the same line.
    assert_eq!(env.complete("mood switch "), found);
    // Past the state there are no more states.
    assert!(!has(&env.complete("switch mood happy "), "calm"));
}

#[test]
fn a_field_position_offers_the_fields_quoted_for_the_shell() {
    let env = Env::new();
    let found = env.complete("occur ");
    assert!(has(&found, "tea") && has(&found, "'my field'"), "{found:?}");
    assert_eq!(env.complete("tail my"), ["'my field'"]);
}

#[test]
fn a_lone_field_offers_the_commands_that_take_one() {
    let found = Env::new().complete("mood ");
    assert!(has(&found, "tail") && has(&found, "happy"), "{found:?}");
    assert!(
        !has(&found, "spec"),
        "spec takes a subcommand first: {found:?}"
    );
}

#[test]
fn a_key_position_offers_the_declared_keys_not_on_the_line() {
    let env = Env::new();
    assert_eq!(env.complete("occur tea "), ["cups="]);
    assert!(env.complete("occur tea cups=1 ").is_empty());
}

#[test]
fn an_entry_position_offers_last_and_recent_hids() {
    let env = Env::new();
    let hid = env.ok(&["get", "_1::hid"]);
    let hid = hid.trim();
    let found = env.complete("get ");
    assert_eq!(found.first().map(String::as_str), Some("_LAST"));
    assert!(has(&found, hid), "{hid} missing from {found:?}");
    assert_eq!(env.complete(&format!("delete {}", &hid[..5])), [hid]);
    assert!(
        has(&env.complete("undo _"), "_1"),
        "the quick ids once `_` is typed"
    );
    let columns = env.complete(&format!("get {hid}::"));
    assert!(has(&columns, &format!("{hid}::field")), "{columns:?}");
}

#[test]
fn it_writes_nothing() {
    let env = Env::new();
    let before = snapshot(env.tmp.path());
    for line in ["", "occur ", "switch mood ", "get ", "occur tea ", "--db "] {
        env.complete(line);
    }
    let after = snapshot(env.tmp.path());
    let changed: Vec<_> = after
        .iter()
        .filter(|f| !before.contains(f))
        .map(|f| &f.0)
        .collect();
    assert!(
        changed.is_empty() && before.len() == after.len(),
        "changed: {changed:?}"
    );
}

/// With a writer holding the db open, the newest rows are still in the
/// WAL: they are read, and neither file is written.
#[test]
fn it_reads_a_live_wal_and_writes_nothing() {
    let env = Env::new();
    let conn = rusqlite::Connection::open(env.db()).unwrap();
    // Reading joins the WAL, so the writer's close is not the last one.
    let _: i64 = conn
        .query_row("SELECT count(*) FROM entries", [], |r| r.get(0))
        .unwrap();
    env.ok(&["occur", "fresh"]);
    let wal = env.tmp.path().join("t.db-wal");
    assert!(
        std::fs::metadata(&wal).unwrap().len() > 0,
        "the row is in the WAL"
    );
    let data = |p: &Path| std::fs::read(p).unwrap();
    let (db_before, wal_before) = (data(&env.db()), data(&wal));
    assert!(has(&env.complete("tail "), "fresh"));
    assert_eq!(data(&env.db()), db_before);
    assert_eq!(data(&wal), wal_before);
    drop(conn);
}

#[test]
fn with_no_db_it_offers_the_grammar_and_creates_nothing() {
    let env = Env::new();
    let missing = env.tmp.path().join("nope.db");
    let mut cmd = env.cmd();
    cmd.env("DATUM_DB", &missing);
    let found = complete_with(cmd, "");
    assert!(found.lines().any(|l| l.starts_with("occur\t")), "{found}");
    assert!(!found.lines().any(|l| l.starts_with("mood\t")), "{found}");
    assert!(!missing.exists());
}

#[test]
fn nonsense_is_an_empty_answer_never_an_error() {
    let env = Env::new();
    for args in [
        &[][..],
        &["x"][..],
        &["--raw"][..],
        &["9", "--", "datum"][..],
    ] {
        let out = env.cmd().arg("__complete").args(args).output().unwrap();
        assert!(out.status.success() && out.stderr.is_empty(), "{args:?}");
    }
}

/// A value the one-candidate-per-line answer cannot carry — a field may
/// hold a tab — is left out rather than split in two. A state carries it
/// in: a new field name may not (da-fi2vu).
#[test]
fn a_value_with_a_control_character_is_not_offered() {
    let env = Env::new();
    env.ok(&["switch", "f", "a\tb"]);
    env.ok(&["switch", "f", "az"]);
    // Split at the tab it would read as `a`.
    assert_eq!(env.complete("switch f a"), ["az"]);
}

/// The scripts complete a line in the shell itself, wherever the shell is
/// installed: bash through `_datum` with the `COMP_*` it would set, fish
/// through `complete -C`.
#[test]
fn the_scripts_complete_a_line() {
    let env = Env::new();
    let bin = Path::new(env!("CARGO_BIN_EXE_datum")).parent().unwrap();
    // The inherited PATH stays: `Command` finds the shell on the PATH it
    // is given.
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    for (shell, body) in [
        (
            "bash",
            "source <(datum completions bash)\n\
             COMP_WORDS=(datum switch mood ''); COMP_CWORD=3\n\
             _datum; printf '%s\\n' \"${COMPREPLY[@]}\"",
        ),
        (
            "fish",
            "datum completions fish | source\ncomplete -C 'datum switch mood '",
        ),
    ] {
        let cmd = env.cmd();
        let out = Command::new(shell)
            .args(["-c", body])
            .envs(cmd.get_envs().filter_map(|(k, v)| Some((k, v?))))
            .env("PATH", &path)
            .output();
        let Ok(out) = out else {
            eprintln!("{shell} not installed: skipped");
            continue;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.lines().any(|l| l.starts_with("calm")),
            "{shell}: {text:?}"
        );
        assert!(
            text.lines().any(|l| l.starts_with("happy")),
            "{shell}: {text:?}"
        );
    }
}

/// With `datum` only an alias and nothing of that name on PATH the scripts
/// still complete (da-1f3z5): zsh under compsys's own options, which turn
/// aliases off, and interactive bash, which expands one at the TAB. With no
/// `datum` at all, zsh says so rather than offering nothing.
#[test]
fn an_alias_only_datum_completes() {
    let env = Env::new();
    let bin = env!("CARGO_BIN_EXE_datum");
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::env::split_paths(&inherited).filter(|d| !d.join("datum").exists()),
    )
    .unwrap();
    let fpath = env.tmp.path().join("fpath");
    std::fs::create_dir_all(&fpath).unwrap();
    std::fs::write(fpath.join("_datum"), env.ok(&["completions", "zsh"])).unwrap();
    // Several words, so a script that ran only the first would run `env`.
    let alias = format!("'env DATUM_COLOR=0 {bin}'");
    // compsys's `_describe` and `_message` need a live completion widget:
    // these print what they are handed.
    let zsh = format!(
        "fpath=({} $fpath); autoload -Uz compinit _datum; compinit -u -D\n\
         _describe() {{ print -rl -- ${{(P)4}} }}; _message() {{ print -r -- \"message: ${{@[-1]}}\" }}\n\
         setopt $_comp_options; words=(datum switch mood ''); CURRENT=4\n\
         alias datum={alias}; _datum; unalias datum; _datum\n\
         alias datum='env /missing/datum'; _datum",
        fpath.display()
    );
    let bash = format!(
        "alias datum={alias}\nsource <({bin} completions bash)\n\
         COMP_WORDS=(datum switch mood ''); COMP_CWORD=3\n\
         _datum; printf '%s\\n' \"${{COMPREPLY[@]}}\""
    );
    for (shell, args) in [
        ("zsh", ["-f", "-c", &zsh]),
        ("bash", ["--norc", "-ic", &bash]),
    ] {
        let cmd = env.cmd();
        let out = Command::new(shell)
            .args(args)
            .envs(cmd.get_envs().filter_map(|(k, v)| Some((k, v?))))
            .env("PATH", &path)
            .output();
        let Ok(out) = out else {
            eprintln!("{shell} not installed: skipped");
            continue;
        };
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.lines().any(|l| l.starts_with("happy")),
            "{shell}: {text:?} {}",
            String::from_utf8_lossy(&out.stderr)
        );
        if shell == "zsh" {
            assert!(
                text.contains("message: datum completion: `datum` not found on PATH"),
                "{text:?}"
            );
            // `env` exits 127 for its missing target: the whole alias is named.
            assert!(
                text.contains("`env /missing/datum` not found (alias datum)"),
                "{text:?}"
            );
        }
    }
}

#[test]
fn each_script_parses() {
    let env = Env::new();
    for (shell, check) in [("zsh", "-n"), ("bash", "-n"), ("fish", "-n")] {
        let script = env.ok(&["completions", shell]);
        assert!(script.contains("datum __complete"), "{shell}");
        let path = env.tmp.path().join(format!("datum.{shell}"));
        std::fs::write(&path, script).unwrap();
        match Command::new(shell).arg(check).arg(&path).output() {
            Ok(out) => assert!(
                out.status.success(),
                "{shell} -n: {}",
                String::from_utf8_lossy(&out.stderr)
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("{shell} not installed: skipped");
            }
            Err(e) => panic!("{shell}: {e}"),
        }
    }
}

/// On a lived-in db of ~30k entries an entry position — the heaviest ask —
/// stays well inside a TAB's budget.
#[test]
fn it_is_fast_on_a_lived_in_db() {
    let env = Env::new();
    let conn = rusqlite::Connection::open(env.db()).unwrap();
    conn.execute_batch(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 30000)
         INSERT INTO entries (hid, slug, content_hash, field, state, verb, occur_time,
                              create_time, modify_time, device_id)
         SELECT printf('gen%09d', i), printf('f%d:%d', i % 40, i), 'h', printf('f%d', i % 40),
                '\"on\"', 'switch', strftime('%Y-%m-%dT%H:%M:%SZ', 1600000000 + i * 600, 'unixepoch'),
                '2024-01-01T00:00:00Z', strftime('%Y-%m-%dT%H:%M:%SZ', 1600000000 + i * 600, 'unixepoch'),
                'dev'
         FROM n;",
    )
    .unwrap();
    drop(conn);
    for line in ["get ", "", "switch f3 "] {
        complete_with(env.cmd(), line); // warm the page cache
        let start = Instant::now();
        let found = complete_with(env.cmd(), line);
        let took = start.elapsed();
        assert!(!found.is_empty(), "{line:?}");
        assert!(took < Duration::from_millis(300), "{line:?} took {took:?}");
    }
}

/// Every file under `dir` with its bytes.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(d).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    out.sort();
    out
}

/// The tip through the binary on a real terminal (`script`): once, then
/// never; never under `DATUM_NO_TIPS=1`; never with stderr piped.
#[test]
fn the_tip_prints_once_on_a_terminal() {
    let env = Env::new();
    let p = env.tmp.path();
    let run = |mut cmd: Command| {
        cmd.env("DATUM_DB", env.db())
            .env("DATUM_HOME", p.join("data"))
            .env("DATUM_CONFIG_HOME", p.join("config"))
            .env("HOME", p.join("home"))
            .output()
    };
    let on_a_terminal = |no_tips: &str| {
        let mut cmd = Command::new("script");
        let datum = env!("CARGO_BIN_EXE_datum");
        // `script` runs the line with `$SHELL`: only datum's is fish.
        cmd.args(["-qc", &format!("SHELL=/bin/fish {datum} tail"), "/dev/null"])
            .env("SHELL", "/bin/sh")
            .env("DATUM_NO_TIPS", no_tips);
        run(cmd).map(|o| String::from_utf8_lossy(&o.stdout).contains("tab completion"))
    };
    let Ok(quiet) = on_a_terminal("1") else {
        eprintln!("`script` not installed: skipped");
        return;
    };
    assert!(!quiet, "DATUM_NO_TIPS=1");
    let mut piped = Command::new(env!("CARGO_BIN_EXE_datum"));
    piped.arg("tail").env("SHELL", "/bin/fish");
    let piped = run(piped).unwrap();
    assert!(!String::from_utf8_lossy(&piped.stderr).contains("tab completion"));
    assert!(on_a_terminal("0").unwrap(), "the first time");
    assert!(!on_a_terminal("0").unwrap(), "only once");
}
