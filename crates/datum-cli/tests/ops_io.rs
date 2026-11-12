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
