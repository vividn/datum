//! `datum db ops export` and `db ops import` (da-tllzg): the op table moved
//! whole, so a fresh copy in no chain holds the same ops and the same
//! entries; the chain's records come too, held and not applied, so it is in
//! no chain; a copy in a chain refuses the import, pointing to JSONL.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

struct Env {
    tmp: TempDir,
    home: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        Self { tmp, home }
    }

    /// `datum <args>` on database `db`, with `stdin`.
    fn datum(&self, db: &str, args: &[&str], stdin: &str) -> Output {
        use std::io::Write;
        let mut child = Command::new(env!("CARGO_BIN_EXE_datum"))
            .args(args)
            .env("DATUM_DB", self.tmp.path().join(format!("{db}.db")))
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.home)
            .env("DATUM_SYNC_NO_DAEMON", "1")
            .env("TZ", "Europe/Berlin")
            .env("NO_COLOR", "1")
            .env("RUST_BACKTRACE", "0")
            .env("RUST_LIB_BACKTRACE", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        input.write_all(stdin.as_bytes()).unwrap();
        drop(input);
        child.wait_with_output().unwrap()
    }

    fn ok(&self, db: &str, args: &[&str]) -> String {
        let out = self.datum(db, args, "");
        assert!(out.status.success(), "{args:?}: {}", text(&out.stderr));
        text(&out.stdout)
    }

    /// A history: a moment, a switch, an edit of it and a delete.
    fn logged(&self, db: &str) {
        self.ok(db, &["moment", "coffee", "-y", "-t", "9:00"]);
        self.ok(db, &["switch", "mood", "happy", "-y", "-t", "10:00"]);
        self.ok(db, &["update", "mood:_", "note=later"]);
        self.ok(db, &["moment", "tea", "-y", "-t", "11:00"]);
        self.ok(db, &["delete", "tea"]);
    }
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

#[test]
fn ops_exported_import_into_a_fresh_copy_as_the_same_ops_and_entries() {
    let env = Env::new();
    env.logged("a");
    let ops = env.ok("a", &["db", "ops", "export"]);
    // Each field's first log writes its spec too, and the delete takes both.
    assert_eq!(
        ops.lines().count(),
        9,
        "every op, the edit and the deletes too"
    );
    assert!(!ops.contains("\"skipped\""), "no seq is left out:\n{ops}");

    let out = env.datum("b", &["db", "ops", "import", "-"], &ops);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "applied 9 · already had 0 · held 0 · conflicts 0\n"
    );
    assert_eq!(
        env.ok("b", &["db", "ops", "export"]),
        ops,
        "the same ops, signed as they were"
    );
    let entries = env.ok("a", &["export", "--jsonl"]);
    assert_eq!(env.ok("b", &["export", "--jsonl"]), entries);
    assert_eq!(
        entries.lines().count(),
        2,
        "tea's delete came too:\n{entries}"
    );

    // Again: every op is held already.
    let out = env.datum("b", &["db", "ops", "import"], &ops);
    assert_eq!(
        text(&out.stdout),
        "applied 0 · already had 9 · held 0 · conflicts 0\n"
    );
}

/// A chained copy's records are held dormant by the copy importing: its
/// later ops do not wait on them, and the copy is in no chain.
#[test]
fn a_chained_copys_ops_import_without_its_chain() {
    let env = Env::new();
    env.logged("a");
    env.ok("a", &["sync", "new-invite"]);
    env.ok("a", &["moment", "juice", "-y", "-t", "12:00"]);
    let ops = env.ok("a", &["db", "ops", "export"]);
    assert!(ops.contains("\"~chain\""), "the chain's records come too");
    assert!(!ops.contains("\"skipped\""), "and fill their seqs");

    let out = env.datum("b", &["db", "ops", "import", "-"], &ops);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stdout).contains("held 0"),
        "{}",
        text(&out.stdout)
    );
    assert_eq!(
        env.ok("b", &["export", "--jsonl"]),
        env.ok("a", &["export", "--jsonl"])
    );
    assert_eq!(env.ok("b", &["db", "ops", "export"]), ops);
    let status = env.datum("b", &["sync", "chain", "list"], "");
    assert!(!status.status.success(), "b is in no chain");
}

#[test]
fn a_chained_copy_refuses_an_ops_import_pointing_to_jsonl() {
    let env = Env::new();
    env.logged("a");
    let ops = env.ok("a", &["db", "ops", "export"]);
    env.ok("b", &["moment", "water", "-y", "-t", "8:00"]);
    env.ok("b", &["sync", "new-invite"]);
    let before = env.ok("b", &["export", "--jsonl"]);

    let out = env.datum("b", &["db", "ops", "import", "-"], &ops);
    assert!(!out.status.success());
    let err = text(&out.stderr);
    assert!(err.contains("is in sync chain"), "{err}");
    assert!(err.contains("`datum import`"), "{err}");
    assert_eq!(
        env.ok("b", &["export", "--jsonl"]),
        before,
        "nothing written"
    );
}

/// An entry no op covers yet (`datum import`'s) gets its baseline from the
/// export; a paused copy signs none, so it refuses rather than leave it out.
#[test]
fn a_paused_copy_refuses_an_export_that_would_leave_entries_out() {
    let env = Env::new();
    env.ok("a", &["sync", "new-invite"]);
    let row = r#"{"field": "coffee", "at": "2026-11-10T08:30:00+01:00", "dur": "MOMENT"}"#;
    let out = env.datum("a", &["import", "-"], row);
    assert!(out.status.success(), "{}", text(&out.stderr));
    env.ok("a", &["sync", "pause"]);

    let out = env.datum("a", &["db", "ops", "export"], "");
    assert!(!out.status.success());
    let err = text(&out.stderr);
    assert!(err.contains("1 entries have no op yet"), "{err}");
    assert!(err.contains("`datum sync resume`"), "{err}");

    env.ok("a", &["sync", "resume"]);
    let ops = env.ok("a", &["db", "ops", "export"]);
    let out = env.datum("b", &["db", "ops", "import"], &ops);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        env.ok("b", &["export", "--jsonl"]),
        env.ok("a", &["export", "--jsonl"])
    );
    assert!(env.ok("b", &["export", "--jsonl"]).contains("coffee"));
}

/// The endpoint and seq of the op in `ops` whose slug starts `slug`, and
/// the highest seq of that endpoint there.
fn op_at(ops: &str, slug: &str) -> (String, i64, i64) {
    let ops: Vec<serde_json::Value> = ops
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let op = ops
        .iter()
        .find(|o| o["slug"].as_str().is_some_and(|s| s.starts_with(slug)))
        .unwrap();
    let e = op["endpoint"].as_str().unwrap().to_string();
    let head = ops
        .iter()
        .filter(|o| o["endpoint"] == e.as_str())
        .map(|o| o["seq"].as_i64().unwrap());
    (e.clone(), op["seq"].as_i64().unwrap(), head.max().unwrap())
}

/// `{"skipped": {e: [[from, to]]}}` before `ops`.
fn skipping(e: &str, from: i64, to: i64, ops: &str) -> String {
    format!("{{\"skipped\":{{\"{e}\":[[{from},{to}]]}}}}\n{ops}")
}

/// The skipped line is unsigned (da-9jdsf): one naming the seq of a signed op
/// the file holds silenced it, said «already had» it, and the copy's vector
/// then kept it out of every sync, its export carrying the stub on. Now the
/// stub is refused by name and the op applies. The mutant that holds a run
/// whole, not split around the file's signed ops, fails this test.
#[test]
fn a_skipped_seq_a_signed_op_in_the_file_fills_is_refused_and_the_op_applies() {
    let env = Env::new();
    for f in ["one", "two", "three"] {
        env.ok("a", &["moment", f, "-y", "-t", "9:00"]);
    }
    let ops = env.ok("a", &["db", "ops", "export"]);
    let (e, n, _) = op_at(&ops, "two:");

    let out = env.datum("b", &["db", "ops", "import"], &skipping(&e, n, n, &ops));
    assert!(!out.status.success(), "the stub is refused");
    let err = text(&out.stderr);
    assert!(
        err.contains(&format!("refused: {e}/{n}: the skipped line")),
        "{err}"
    );
    assert!(
        text(&out.stdout).contains("already had 0"),
        "{}",
        text(&out.stdout)
    );
    assert!(
        !text(&out.stdout).contains("stubbed"),
        "{}",
        text(&out.stdout)
    );
    let entries = env.ok("a", &["export", "--jsonl"]);
    assert_eq!(
        env.ok("b", &["export", "--jsonl"]),
        entries,
        "one, two and three"
    );

    // A sync with two's own author has nothing more to bring.
    let vector = env.ok("b", &["sync", "pipe", "vector"]);
    let sent = env.ok("a", &["sync", "pipe", "ops", "--since", vector.trim()]);
    let out = env.datum("b", &["sync", "pipe", "apply"], &sent);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(env.ok("b", &["export", "--jsonl"]), entries);
    assert_eq!(
        env.ok("b", &["db", "ops", "export"]),
        ops,
        "no stub carried on"
    );
}

/// A run past an endpoint's last signed op in the file holds no stub: a
/// forged one would make its next op read as a second at its seq.
#[test]
fn a_skipped_run_past_its_endpoints_last_op_in_the_file_holds_no_stub() {
    let env = Env::new();
    env.ok("a", &["moment", "one", "-y", "-t", "9:00"]);
    let ops = env.ok("a", &["db", "ops", "export"]);
    let (e, _, head) = op_at(&ops, "one:");

    let out = env.datum(
        "b",
        &["db", "ops", "import"],
        &skipping(&e, head + 1, head + 3, &ops),
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let err = text(&out.stderr);
    let past = format!(
        "not stubbed: {e}/{}..{}: no stub past its last op",
        head + 1,
        head + 3
    );
    assert!(err.contains(&past), "{err}");
    assert!(
        !text(&out.stdout).contains("stubbed"),
        "{}",
        text(&out.stdout)
    );

    env.ok("a", &["moment", "two", "-y", "-t", "10:00"]);
    let out = env.datum(
        "b",
        &["db", "ops", "import"],
        &env.ok("a", &["db", "ops", "export"]),
    );
    assert!(
        out.status.success(),
        "a's next op applies: {}",
        text(&out.stderr)
    );
    assert_eq!(
        env.ok("b", &["export", "--jsonl"]),
        env.ok("a", &["export", "--jsonl"])
    );
}

/// A purge's stubs come back said as stubs, never as ops already held.
#[test]
fn a_purges_skipped_line_imports_said_as_stubs() {
    let env = Env::new();
    env.ok("a", &["moment", "one", "-y", "-t", "9:00"]);
    env.ok("a", &["sync", "new-invite"]);
    env.ok("a", &["sync", "chain", "purge", "--data", "--yes"]);
    env.ok("a", &["moment", "two", "-y", "-t", "10:00"]);
    let ops = env.ok("a", &["db", "ops", "export"]);
    assert!(ops.starts_with("{\"skipped\":"), "{ops}");

    let out = env.datum("b", &["db", "ops", "import"], &ops);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let said = text(&out.stdout);
    assert!(said.contains("already had 0"), "{said}");
    assert!(
        said.contains("stubbed 2: seqs the skipped line names"),
        "{said}"
    );
    assert_eq!(env.ok("b", &["db", "ops", "export"]), ops);
    assert_eq!(
        env.ok("b", &["export", "--jsonl"]),
        env.ok("a", &["export", "--jsonl"])
    );
}

/// The skipped runs are bounded: a file naming more seqs than a purge
/// leaves, or no run of seqs, is refused by its line, holding nothing.
#[test]
fn a_skipped_line_past_the_bound_is_refused_naming_its_line() {
    let env = Env::new();
    env.ok("b", &["moment", "water", "-y", "-t", "8:00"]);
    let before = env.ok("b", &["db", "ops", "export"]);
    let e = "7".repeat(64);
    for (line, why) in [
        (format!("[[1,{}],[1,1]]", 1 << 22), "more than 4194304 seqs"),
        ("[[0,5]]".into(), "[0, 5] is no run"),
        ("[[5,3]]".into(), "[5, 3] is no run"),
        (format!("[[1,{}]]", u64::MAX), "two seqs"),
    ] {
        let file = format!("\n{{\"skipped\":{{\"{e}\":{line}}}}}\n");
        let out = env.datum("b", &["db", "ops", "import"], &file);
        assert!(!out.status.success(), "{line}");
        let err = text(&out.stderr);
        assert!(
            err.contains("line 2, the skipped line") && err.contains(why),
            "{err}"
        );
        assert_eq!(
            env.ok("b", &["db", "ops", "export"]),
            before,
            "nothing held"
        );
    }
}
