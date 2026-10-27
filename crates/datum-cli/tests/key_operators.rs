//! Key operators (da-p9gf5): `key+=x` / `key-=x`, or a slot named `key+` /
//! `key-`, add to a key's list and take out of it — beside the key on the
//! line, that value; otherwise the field's record where the entry lands,
//! recording the value read as `asserted_base`, which `check keys` holds the
//! history to. And `%::key%` / `%::%` (da-3se7a), which copy a value from the
//! field's record into the line where the entry lands.

use std::process::{Command, Output};

use serde_json::{json, Value};
use tempfile::TempDir;

struct Env {
    tmp: TempDir,
}

impl Env {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().expect("create tmpdir"),
        }
    }

    fn out(&self, args: &[&str]) -> Output {
        let p = self.tmp.path();
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", p.join("t.db"))
            .env("DATUM_HOME", p)
            .env("DATUM_CONFIG_HOME", p)
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .expect("spawn datum")
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.out(args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "datum {args:?} failed: {stderr}");
        String::from_utf8(out.stdout).expect("utf-8 stdout")
    }

    fn record(&self, field: &str) -> Value {
        serde_json::from_str(&self.ok(&["get", &format!("{field}::")])).unwrap()
    }

    /// The newest entry's `asserted_base`, `Null` when it has none.
    fn asserted(&self) -> Value {
        let hid = self.ok(&["get", "_::hid"]);
        let meta: Option<String> = rusqlite::Connection::open(self.tmp.path().join("t.db"))
            .unwrap()
            .query_row(
                "SELECT json_extract(extra_metadata, '$.asserted_base') FROM entries WHERE hid = ?1",
                [hid.trim()],
                |r| r.get(0),
            )
            .unwrap();
        meta.map_or(Value::Null, |m| serde_json::from_str(&m).unwrap())
    }
}

#[test]
fn operators_beside_their_key_change_its_value_on_the_line() {
    let env = Env::new();
    env.ok(&[
        "note",
        "iss",
        "blocked_by=a",
        "blocked_by+=b",
        "blocked_by+=c",
    ]);
    assert_eq!(env.record("iss"), json!({"blocked_by": ["a", "b", "c"]}));
    assert_eq!(env.asserted(), Value::Null, "the line read no record");
    env.ok(&["note", "iss", "blocked_by=a,b", "blocked_by-=a"]);
    assert_eq!(env.record("iss"), json!({"blocked_by": ["b"]}));
}

#[test]
fn operators_against_the_record_store_the_list_and_what_they_read() {
    let env = Env::new();
    env.ok(&["note", "iss", "blocked_by+=x"]);
    assert_eq!(env.asserted(), json!({"blocked_by": null}));
    env.ok(&["note", "iss", "-k", "blocked_by+", "y"]);
    assert_eq!(env.asserted(), json!({"blocked_by": ["x"]}));
    env.ok(&["note", "iss", "blocked_by-=x", "other+=1", "other+=2"]);
    assert_eq!(
        env.asserted(),
        json!({"blocked_by": ["x", "y"], "other": null})
    );
    assert_eq!(
        env.record("iss"),
        json!({"blocked_by": ["y"], "other": [1, 2]})
    );
    assert!(env.ok(&["check", "keys"]).contains("No consistency issues"));
}

#[test]
fn a_removal_of_what_the_key_does_not_hold_writes_nothing() {
    let env = Env::new();
    env.ok(&["note", "iss", "blocked_by+=x"]);
    for line in [
        &["note", "iss", "blocked_by-=z"][..],
        &["note", "iss", "blocked_by=a", "blocked_by-=z"],
    ] {
        let out = env.out(line);
        assert!(!out.status.success(), "{line:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("iss's blocked_by holds no z"),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    env.ok(&["note", "iss", "blocked_by+=y", "--dry-run"]);
    assert_eq!(env.record("iss"), json!({"blocked_by": ["x"]}));
}

/// A backdated edit reads the record at its own time, and an edit after it
/// that read the key without it is drift: `check keys` names it, and `--fix`
/// writes the value the history gives, once.
#[test]
fn a_backdated_edit_reads_the_record_at_its_time_and_check_fixes_the_one_after() {
    let env = Env::new();
    env.ok(&["occur", "iss", "blocked_by+=a", "-t", "-30"]);
    env.ok(&["occur", "iss", "blocked_by+=b", "-t", "-10"]);
    env.ok(&["occur", "iss", "blocked_by+=c", "-t", "-20"]);
    assert_eq!(env.asserted(), json!({"blocked_by": ["a"]}));
    assert_eq!(env.record("iss")["blocked_by"], json!(["a", "b"]));
    let found = env.out(&["check", "keys"]);
    let said = String::from_utf8_lossy(&found.stdout);
    assert!(found.status.success(), "drift is a warning: {said}");
    assert!(
        said.contains(
            r#"iss's blocked_by reads ["a","b"], and its history makes it ["a","c","b"]"#
        ),
        "{said}"
    );
    env.ok(&["check", "keys", "--fix"]);
    assert_eq!(env.record("iss")["blocked_by"], json!(["a", "c", "b"]));
    assert!(env.ok(&["check", "keys"]).contains("No consistency issues"));
    let notes = env.ok(&["tail", "iss", "-n", "10"]);
    env.ok(&["check", "keys", "--fix"]);
    assert_eq!(
        env.ok(&["tail", "iss", "-n", "10"]),
        notes,
        "a second fix writes nothing"
    );
}

/// An alias line's `%::title%` stores the record's title as it stands, plain;
/// in text, the text; a backdated line reads the record at its own time.
#[test]
fn a_record_copy_stores_the_records_value_where_the_entry_lands() {
    let env = Env::new();
    env.ok(&["occur", "iss", "title=first", "-t", "-30"]);
    env.ok(&["occur", "iss", "title=second", "-t", "-10"]);
    env.ok(&[
        "alias",
        "set",
        "iwas",
        "note iss was=%::title% said=on_%::title%",
    ]);
    env.ok(&["iwas"]);
    let record = env.record("iss");
    assert_eq!(
        (&record["was"], &record["said"]),
        (&json!("second"), &json!("on_second"))
    );
    assert_eq!(env.asserted(), Value::Null, "a copy alone asserts nothing");
    env.ok(&["occur", "iss", "then=%::title%", "-t", "-20"]);
    assert_eq!(env.record("iss")["then"], json!("first"));
}

/// `%::%` is the key's own record value, and an operator beside it changes
/// what it copied: the merged list is stored with the value read.
#[test]
fn a_copy_of_the_keys_own_value_under_an_operator_stores_the_list_and_its_base() {
    let env = Env::new();
    env.ok(&["note", "iss", "blocked_by=a,b"]);
    env.ok(&["note", "iss", "blocked_by=%::%", "blocked_by+=c"]);
    assert_eq!(env.record("iss")["blocked_by"], json!(["a", "b", "c"]));
    assert_eq!(env.asserted(), json!({"blocked_by": ["a", "b"]}));
    assert!(env.ok(&["check", "keys"]).contains("No consistency issues"));
}

/// A key the record does not hold refuses, naming it, and nothing is
/// written; `%?::key%` alone leaves the key out.
#[test]
fn a_copy_of_a_key_the_record_lacks_refuses_unless_optional() {
    let env = Env::new();
    env.ok(&["note", "iss", "title=t"]);
    let out = env.out(&["note", "iss", "x=%::nope%"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{said}");
    assert!(
        said.contains("%::nope%: iss's record holds no nope"),
        "{said}"
    );
    env.ok(&["note", "iss", "x=%?::nope%", "y=1"]);
    assert_eq!(env.record("iss"), json!({"title": "t", "y": 1}));
    env.ok(&["note", "iss", r"note=\%::x", r"z=50\% \%::x"]);
    let record = env.record("iss");
    assert_eq!(record["note"], json!("%::x"), "escaped, it copies nothing");
    assert_eq!(record["z"], json!(r"50\% %::x"), r"only \%:: is unescaped");
}
