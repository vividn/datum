//! `datum conflicts` through the real binary (da-ttsm, da-05rkv).
//!
//! Every case here spawns the actual `datum` binary and reads its stdout /
//! exit code. A conflict itself takes two copies and a sync, which
//! `sync_ops.rs` drives; here is what the command does around it, and how
//! `resolve` finds its entry, on the conflicts `debug conflicts` makes.
//!
//! Scope note: `conflicts` lists what the sync fold leaves contested in
//! the entries' `conflicts` column, not the integrity queue. da-z46t
//! removed the queue-wide `list/resolve/auto-resolve` surface deliberately
//! — integrity findings resolve by fixing the data — and
//! `auto_resolve_is_not_a_conflicts_subcommand` keeps it removed.

use std::path::PathBuf;
use std::process::Command;

use datum_core::db::Db;
use datum_core::integrity::{IntegrityKind, ListFilter};
use serde_json::{json, Value};
use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    path: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let path = tmp.path().join("conflicts.db");
        // Materialize the schema before the binary runs so seeding and
        // the CLI look at the same file.
        drop(Db::open(path.to_str().unwrap()).expect("open db"));
        Self { _tmp: tmp, path }
    }

    fn db(&self) -> Db {
        Db::open(self.path.to_str().unwrap()).expect("reopen db")
    }

    /// Run `datum <args...>` against this env's db. Returns (ok, stdout+stderr).
    fn run(&self, args: &[&str]) -> (bool, String) {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", &self.path)
            .env("NO_COLOR", "1")
            // CI sets RUST_BACKTRACE, which puts a stack trace under an error.
            .env_remove("RUST_BACKTRACE")
            .env_remove("RUST_LIB_BACKTRACE")
            .args(args)
            .output()
            .expect("spawn datum");
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.success(), text)
    }

    fn double_open(&self) -> String {
        assert!(self.run(&["start", "work", "-t=-1"]).0);
        assert!(self.run(&["start", "work"]).0);
        self.db()
            .list_integrity_warnings(&ListFilter {
                kind: Some(IntegrityKind::DoubleOpen),
                pending: Some(true),
            })
            .expect("list")
            .first()
            .expect("double_open filed")
            .hid
            .clone()
    }

    /// `debug conflicts 2`, then the short hids `conflicts` lists them by:
    /// the two live entries', and the deleted one's. None is a whole hid.
    fn debug_conflicts(&self) -> (Vec<String>, String) {
        assert!(self.run(&["debug", "conflicts", "2"]).0);
        let (_, list) = self.run(&["conflicts"]);
        let (deleted, live): (Vec<&str>, Vec<&str>) = list
            .lines()
            .filter(|l| l.contains("  debug:"))
            .partition(|l| l.ends_with("(deleted)"));
        let short = |l: &&str| l.split_whitespace().next().unwrap().to_string();
        let (live, deleted): (Vec<_>, Vec<_>) = (
            live.iter().map(short).collect(),
            deleted.iter().map(short).collect(),
        );
        assert_eq!((live.len(), deleted.len()), (2, 1), "{list}");
        let db = self.db();
        for hid in live.iter().chain(&deleted) {
            assert!(db.get_entry_including_deleted(hid).is_err(), "{hid}");
        }
        (live, deleted[0].clone())
    }

    /// The stored `column` of the entry a short hid names, as JSON.
    fn column(&self, short: &str, column: &str) -> Value {
        let text: Option<String> = self
            .db()
            .conn()
            .query_row(
                &format!("SELECT {column} FROM entries WHERE hid LIKE ?1 || '%'"),
                [short],
                |r| r.get(0),
            )
            .unwrap();
        text.map_or(Value::Null, |t| serde_json::from_str(&t).unwrap())
    }

    /// The header lines of `conflicts`: one an entry with conflicts.
    fn conflicted(&self) -> Vec<String> {
        let (_, list) = self.run(&["conflicts"]);
        list.lines()
            .filter(|l| l.contains("  debug:"))
            .map(str::to_string)
            .collect()
    }
}

#[test]
fn bare_conflicts_says_there_are_none() {
    let env = Env::new();
    // Bare `datum conflicts` is `conflicts list`.
    let (ok, out) = env.run(&["conflicts"]);
    assert!(ok, "datum conflicts failed: {out}");
    assert!(out.contains("No sync conflicts"), "{out}");
}

#[test]
fn list_leaves_integrity_findings_to_check() {
    let env = Env::new();
    let double_open = env.double_open();
    let (ok, out) = env.run(&["conflicts", "list"]);
    assert!(ok, "conflicts list failed: {out}");
    assert!(
        !out.contains(&double_open) && !out.contains("double_open"),
        "double_open is `datum check`'s to report, not conflicts':\n{out}"
    );
    assert!(out.contains("No sync conflicts"), "{out}");
}

#[test]
fn resolve_refuses_an_entry_with_no_conflicts() {
    let env = Env::new();
    env.double_open();
    let hid: String = env
        .db()
        .conn()
        .query_row(
            "SELECT hid FROM entries WHERE field = 'work' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let (ok, out) = env.run(&["conflicts", "resolve", &hid]);
    assert!(!ok, "nothing to resolve, and nothing may be written: {out}");
    assert!(out.contains("has no conflicts"), "{out}");
}

#[test]
fn resolve_takes_the_short_hid_tail_prints() {
    // da-q4x3c: an exact-hid lookup refused the hid every listing prints.
    let env = Env::new();
    let (live, _) = env.debug_conflicts();
    let (_, tail) = env.run(&["tail"]);
    assert!(tail.contains(&live[0]), "{tail}");

    let (ok, out) = env.run(&["conflicts", "resolve", &live[0]]);
    assert!(ok, "{out}");
    assert!(out.ends_with(&format!("({})\n", live[0])), "{out}");
    let left = env.conflicted();
    assert_eq!(left.len(), 2, "{left:?}");
    assert!(!left.iter().any(|l| l.starts_with(&live[0])), "{left:?}");
}

#[test]
fn resolve_takes_the_short_hid_of_a_deleted_entry() {
    // A delete can be one side of a conflict, and `tail` never shows it.
    let env = Env::new();
    let (_, deleted) = env.debug_conflicts();
    let (ok, out) = env.run(&["conflicts", "resolve", &deleted]);
    assert!(ok, "{out}");
    assert!(out.ends_with(&format!("({deleted})\n")), "{out}");
    let left = env.conflicted();
    assert_eq!(left.len(), 2, "{left:?}");
    assert!(!left.iter().any(|l| l.contains("(deleted)")), "{left:?}");
}

#[test]
fn resolve_refuses_an_ambiguous_prefix_as_update_does() {
    let env = Env::new();
    env.debug_conflicts();
    let (ok, out) = env.run(&["conflicts", "resolve", "debug:"]);
    assert!(!ok, "{out}");
    assert!(out.contains("'debug:' is ambiguous"), "{out}");
    assert_eq!(out, env.run(&["update", "debug:", "note=y"]).1);
    assert_eq!(env.conflicted().len(), 3, "nothing was resolved");
}

#[test]
fn resolve_takes_one_entry_then_picks() {
    let env = Env::new();
    let (ok, out) = env.run(&["conflicts", "resolve", "a", "b"]);
    assert!(!ok, "{out}");
    assert!(out.contains("one entry, then KEY=VALUE picks"), "{out}");
}

// ─── da-07594: letters, strategies, picks ────────────────────────────────────
//
// `debug conflicts` makes each live entry's `state` «edited here» (this
// copy, the earlier clock: A) against «edited there» (the device: B), and
// one entry deleted here (A) that the device edited (B).

#[test]
fn the_listing_letters_each_side_in_clock_order() {
    let env = Env::new();
    env.debug_conflicts();
    let (_, list) = env.run(&["conflicts"]);
    let side = |letter: &str, what: &str| {
        list.lines()
            .filter(|l| l.starts_with(&format!("      {letter}  ")) && l.contains(what))
            .count()
    };
    assert_eq!(side("A", r#""edited here""#), 2, "{list}");
    assert_eq!(side("B", r#""edited there""#), 2, "{list}");
    assert_eq!(side("A", "deleted on "), 1, "{list}");
    assert_eq!(side("B", "kept on "), 1, "{list}");
    let sides = list.lines().filter(|l| l.starts_with("      "));
    assert_eq!(sides.count(), 6, "a letter on every side:\n{list}");
}

#[test]
fn json_is_each_entrys_column_as_stored() {
    let env = Env::new();
    env.debug_conflicts();
    let (ok, out) = env.run(&["conflicts", "--json"]);
    assert!(ok, "{out}");
    let rows: Vec<Value> = serde_json::from_str(&out).unwrap();
    assert_eq!(rows.len(), 3, "{out}");
    for row in &rows {
        let hid = row["hid"].as_str().unwrap();
        assert_eq!(row["conflicts"], env.column(hid, "conflicts"), "{row}");
        assert_eq!(row["deleted"], row["conflicts"]["deleted"].is_array());
    }
    assert_eq!(env.run(&["conflicts", "list", "--json"]).1, out);
}

#[test]
fn each_strategy_is_its_row_of_the_table_over_a_and_b() {
    let env = Env::new();
    assert!(env.run(&["debug", "conflicts", "5"]).0);
    assert!(env.run(&["check"]).1.contains("sync_conflict (x6)"));
    let (a, b) = (json!("edited here"), json!("edited there"));
    let cases = [
        ("--use-a", a.clone()),
        ("--use-b", b.clone()),
        ("--prefer-a", a.clone()),
        ("--prefer-b", b.clone()),
        ("--merge", json!([a, b])),
    ];
    let live = env.conflicted();
    let live = live.iter().filter(|l| !l.ends_with("(deleted)"));
    let mut resolved = 0;
    for (line, (how, want)) in live.zip(cases) {
        let hid = line.split_whitespace().next().unwrap();
        let (ok, out) = env.run(&["conflicts", "resolve", hid, how]);
        assert!(ok, "{how}: {out}");
        assert_eq!(env.column(hid, "state"), want, "{how}");
        assert!(out.contains(&format!("    state  {want}\n")), "{out}");
        resolved += 1;
    }
    assert_eq!(resolved, 5);
    // The delete is left: `check` counts what `conflicts` lists.
    assert!(env.run(&["check"]).1.contains("sync_conflict (x1)"));
    let deleted = env.conflicted()[0].clone();
    let deleted = deleted.split_whitespace().next().unwrap();
    assert!(env.run(&["conflicts", "resolve", deleted]).0);
    assert!(!env.run(&["check"]).1.contains("sync_conflict"));
}

#[test]
fn a_bare_resolve_keeps_what_the_entry_shows() {
    // No strategy and no pick: the conflicts are settled and no data moves.
    // The fold's winner stays (B's state), and the delete, which is A,
    // stands (vremya's review of #584: the entry must not come back).
    let env = Env::new();
    let (live, deleted) = env.debug_conflicts();
    let shown = |hid: &str| {
        (
            env.column(hid, "state"),
            env.column(hid, "json_quote(deleted_at)"),
        )
    };
    for hid in live.iter().chain([&deleted]) {
        let before = shown(hid);
        let (ok, out) = env.run(&["conflicts", "resolve", hid]);
        assert!(ok, "{out}");
        assert!(out.starts_with("Resolved "), "it names no key: {out}");
        assert_eq!(shown(hid), before, "{hid}");
    }
    assert_eq!(shown(&live[0]).0, json!("edited there"));
    assert!(!shown(&deleted).1.is_null(), "the delete stands");
    assert!(env.conflicted().is_empty());
}

#[test]
fn a_delete_follows_a_named_strategy_as_a_key_does() {
    // The delete is A, the edit that would keep the entry B.
    for (how, gone) in [
        ("--use-b", false),
        ("--prefer-b", false),
        ("--use-a", true),
        ("--prefer-a", true),
        ("deleted=a", true),
        ("deleted=B", false),
    ] {
        let env = Env::new();
        let (_, deleted) = env.debug_conflicts();
        let (ok, out) = env.run(&["conflicts", "resolve", &deleted, how]);
        assert!(ok, "{how}: {out}");
        let said = if gone { "    deleted\n" } else { "    kept\n" };
        assert!(out.starts_with(said), "{how}: {out}");
        let at = env.column(&deleted, "json_quote(deleted_at)");
        assert_eq!(!at.is_null(), gone, "{how}");
        assert_eq!(env.conflicted().len(), 2, "{how}");
    }
}

#[test]
fn a_merge_of_what_cannot_be_one_array_is_refused_and_says_to_pick() {
    let env = Env::new();
    let (_, deleted) = env.debug_conflicts();
    let (ok, out) = env.run(&["conflicts", "resolve", &deleted, "--merge"]);
    assert!(!ok, "{out}");
    assert!(out.contains("cannot merge true with false"), "{out}");
    assert!(out.contains("KEY=a, KEY=b or KEY=VALUE"), "{out}");
    assert_eq!(env.conflicted().len(), 3, "nothing was resolved");
}

#[test]
fn a_pick_is_a_sides_letter_or_a_typed_value() {
    let env = Env::new();
    assert!(env.run(&["debug", "conflicts", "4"]).0);
    let live = env.conflicted();
    let live: Vec<&str> = live
        .iter()
        .filter(|l| !l.ends_with("(deleted)"))
        .map(|l| l.split_whitespace().next().unwrap())
        .collect();
    // A letter that is no side is not quietly the text «c».
    let (ok, out) = env.run(&["conflicts", "resolve", live[0], "state=c"]);
    assert!(!ok, "{out}");
    assert!(out.contains("state has 2 sides, and c is none"), "{out}");
    assert_eq!(env.conflicted().len(), 5, "nothing was resolved");
    for (hid, (pick, want)) in live.iter().zip([
        ("state=a", json!("edited here")),
        ("state=B", json!("edited there")),
        (r#"state="c""#, json!("c")),
        (r#"state=[1,"two"]"#, json!([1, "two"])),
    ]) {
        // The strategy is for the keys no pick names: here, none.
        let (ok, out) = env.run(&["conflicts", "resolve", hid, pick, "--use-a"]);
        assert!(ok, "{pick}: {out}");
        assert_eq!(env.column(hid, "state"), want, "{pick}");
    }
    assert_eq!(env.conflicted().len(), 1);
}

#[test]
fn resolve_takes_one_strategy() {
    let env = Env::new();
    let (live, _) = env.debug_conflicts();
    let (ok, out) = env.run(&["conflicts", "resolve", &live[0], "--use-a", "--merge"]);
    assert!(!ok, "{out}");
    assert!(out.contains("one strategy: '--merge' is a second"), "{out}");
}

#[test]
fn resolve_has_no_json_and_says_so() {
    // `--json` is the listing's. On `resolve` it is refused, not swallowed.
    let env = Env::new();
    let (live, _) = env.debug_conflicts();
    let (ok, out) = env.run(&["conflicts", "resolve", &live[0], "--json"]);
    assert!(!ok, "{out}");
    assert!(
        out.contains("--show json is not a rendering conflicts resolve has"),
        "{out}"
    );
    assert_eq!(env.conflicted().len(), 3, "nothing was resolved");
}

#[test]
fn the_help_says_what_each_strategy_does_to_a_delete() {
    // The help named --merge among those that undo a delete; it refuses one.
    let env = Env::new();
    let (_, help) = env.run(&["help", "conflicts"]);
    let help = help.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        help.contains("undone by --use-b or --prefer-b. --merge refuses it"),
        "{help}"
    );
}

#[test]
fn take_local_take_remote_and_keep_both_are_gone() {
    // They meant something else on each copy; the letters do not.
    let env = Env::new();
    let (live, _) = env.debug_conflicts();
    for flag in ["--take-local", "--take-remote", "--keep-both"] {
        let (ok, out) = env.run(&["conflicts", "resolve", &live[0], flag]);
        assert!(!ok, "{flag}: {out}");
        assert!(out.contains(&format!("unexpected flag: {flag}")), "{out}");
    }
    assert_eq!(env.conflicted().len(), 3, "nothing was resolved");
    let (_, help) = env.run(&["help", "conflicts"]);
    assert!(
        !help.contains("take-") && !help.contains("keep-both"),
        "{help}"
    );
}

#[test]
fn auto_resolve_is_not_a_conflicts_subcommand() {
    // da-z46t removed the auto-resolver's CLI lever along with the rest of
    // the queue-wide surface; the double_open rule stays internal. If this
    // starts passing, that decision has been quietly reversed.
    let env = Env::new();
    let (ok, out) = env.run(&["conflicts", "auto-resolve"]);
    assert!(!ok, "conflicts auto-resolve should not exist: {out}");
    assert!(
        out.contains("unknown conflicts subcommand"),
        "expected the subcommand guard, got:\n{out}"
    );
}
