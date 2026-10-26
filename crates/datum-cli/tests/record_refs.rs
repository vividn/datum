//! References to records (da-xxqdb): `ref://<field>::`, or `<field>::` on
//! a key typed `ref`, names a field's record. Written, its name is the
//! field it names as a read's would be, so the stored reference never
//! depends on which prefixes are unique later; `check refs` asks the field;
//! a `::` hop reads the record; `get <field> --backlinks` lists the records
//! that refer to it.

use std::process::{Command, Output};

use tempfile::TempDir;

struct Env {
    tmp: TempDir,
}

impl Env {
    fn out(&self, args: &[&str]) -> Output {
        let p = self.tmp.path();
        Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", p.join("t.db"))
            .env("DATUM_HOME", p)
            .env("DATUM_CONFIG_HOME", p)
            .env("DATUM_SYNC_NO_DAEMON", "1")
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

    fn err(&self, args: &[&str]) -> String {
        let out = self.out(args);
        assert!(!out.status.success(), "datum {args:?} must fail");
        String::from_utf8(out.stderr).expect("utf-8 stderr")
    }
}

const A: &str = "issue__k7f2a9x3";
const B: &str = "issue__k7g00000";

/// The tracker's shape: a strict issue template whose `blocked_by` is
/// typed `ref[]`, and two issues, `A` and `B`, starting `issue__k7`.
fn tracker() -> Env {
    let env = Env {
        tmp: tempfile::tempdir().expect("create tmpdir"),
    };
    let spec = r#"{"verb":"switch","subfields":{"blocked_by":{"value_type":"ref[]"}}}"#;
    env.ok(&[
        "setup",
        "issue__%hid:8%",
        "--auto-setup",
        "false",
        "--from-json",
        spec,
    ]);
    for (f, t) in [(A, "a"), (B, "b")] {
        env.ok(&["switch", f, "queued", "--setup", &format!("title={t}")]);
    }
    env
}

#[test]
fn a_prefix_is_stored_as_the_field_it_names_and_stays_so() {
    let env = tracker();
    let c = "issue__c0000000";
    env.ok(&["switch", c, "queued", "--setup", "blocked_by=issue__k7f::"]);
    env.ok(&["occur", "x", "n=ref://issue__k7g::"]);
    let stored = |at: &str| env.ok(&["get", at]).trim().to_string();
    assert_eq!(stored(&format!("{c}::blocked_by")), format!(r#"["{A}::"]"#));
    assert_eq!(stored("x::n"), format!("ref://{B}::"));

    // `issue__k7f` starts two issues now: what was stored is unchanged,
    // and still reads A's record, where the same prefix written now is
    // refused, naming both.
    let a2 = "issue__k7f2zzzz";
    env.ok(&["switch", a2, "queued", "--setup", "title=a2"]);
    assert_eq!(stored(&format!("{c}::blocked_by")), format!(r#"["{A}::"]"#));
    let refused = env.err(&["occur", "z", "n=ref://issue__k7f2::"]);
    assert!(
        refused.contains(A) && refused.contains(a2),
        "names both: {refused}"
    );
}

#[test]
fn a_prefix_starting_several_or_none_is_refused_where_none_may_be_made() {
    let env = tracker();
    let both = env.err(&[
        "switch",
        "issue__c0000000",
        "queued",
        "--setup",
        "blocked_by=issue__k7::",
    ]);
    assert!(both.contains(A) && both.contains(B), "{both}");
    let none = env.err(&[
        "switch",
        "issue__c0000000",
        "queued",
        "--setup",
        "blocked_by=issue__zbx::",
    ]);
    assert!(none.contains("issue__zbx"), "{none}");
    // Nothing was written by either.
    assert!(env.err(&["get", "issue__c0000000::"]).contains("not found"));

    // Control: where a write could make the field, a reference to none
    // is kept as typed, to dangle until it is logged.
    env.ok(&["occur", "x", "n=ref://later::"]);
    assert_eq!(env.ok(&["get", "x::n"]).trim(), "ref://later::");
}

#[test]
fn an_update_resolves_what_it_writes_and_keeps_what_was_held() {
    let env = tracker();
    // Held while no field started `soon`, so stored as typed.
    env.ok(&["occur", "x", "held=ref://soon::"]);
    env.ok(&["occur", "soonest"]);
    env.ok(&["update", "x", "n=ref://issue__k7g::"]);
    assert_eq!(env.ok(&["get", "x::held"]).trim(), "ref://soon::");
    assert_eq!(env.ok(&["get", "x::n"]).trim(), format!("ref://{B}::"));
}

#[test]
fn check_asks_the_field_for_a_record_reference() {
    let env = tracker();
    env.ok(&[
        "occur",
        "x",
        "gone=ref://nowhere::",
        "live=ref://issue__k7f2a9x3::",
    ]);
    env.ok(&["setup", "specced", "--verb", "occur"]);
    env.ok(&["occur", "y", "s=ref://specced::", "-t", "-5"]);
    let out = env.out(&["check", "refs"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("refers to nowhere::, which is no field (no entries, no spec)"),
        "{text}"
    );
    assert!(!text.contains(&format!("{A}::")), "a live record: {text}");
    assert!(!text.contains("specced::"), "a spec of its own: {text}");
}

#[test]
fn a_hop_through_a_record_reference_reads_the_record() {
    let env = tracker();
    env.ok(&["occur", "x", "n=ref://issue__k7f::"]);
    env.ok(&["note", A, "title=a, renamed"]);
    assert_eq!(env.ok(&["get", "x::n::title"]).trim(), "a, renamed");
    assert_eq!(env.ok(&["get", "x::n::state"]).trim(), "queued");
}

#[test]
fn backlinks_are_the_records_that_refer_to_it() {
    let env = tracker();
    let (c, d) = ("issue__c0000000", "issue__d0000000");
    env.ok(&[
        "switch",
        c,
        "queued",
        "--setup",
        &format!("blocked_by={A}::,{B}::"),
    ]);
    env.ok(&[
        "switch",
        d,
        "queued",
        "--setup",
        &format!("blocked_by={A}::"),
    ]);
    env.ok(&["occur", "x", "n=ref://issue__k7f::", "m=ref://issue__k7f::"]);
    // `d` no longer blocks on A: its record does not refer to it.
    env.ok(&["note", d, "blocked_by=null"]);
    // Controls: a literal, and an untyped key's bare name, refer to nothing.
    env.ok(&[
        "occur",
        "y",
        &format!("lit=str://ref://{A}::"),
        &format!("bare={A}::"),
    ]);
    let links = env.ok(&["get", "issue__k7f", "--backlinks"]);
    assert_eq!(links, format!("{c} blocked_by\nx m,n\n"));
    assert_eq!(
        env.ok(&["get", &format!("{B}::"), "--backlinks"]),
        format!("{c} blocked_by\n")
    );
}

/// A key operator's operand is resolved as a value is (da-p9gf5's
/// `key+=` / `key-=`), so `-=` takes out the full name `+=` stored.
#[test]
fn a_key_operator_names_the_record_its_prefix_names() {
    let env = tracker();
    let c = "issue__c0000000";
    env.ok(&["switch", c, "queued", "--setup", "blocked_by=issue__k7g::"]);
    env.ok(&["note", c, "blocked_by+=issue__k7f::"]);
    let blocked = || {
        env.ok(&["get", &format!("{c}::blocked_by")])
            .trim()
            .to_string()
    };
    assert_eq!(blocked(), format!(r#"["{B}::","{A}::"]"#));
    env.ok(&["note", c, "blocked_by-=issue__k7g::"]);
    assert_eq!(blocked(), format!(r#"["{A}::"]"#));
    let both = env.err(&["note", c, "blocked_by+=issue__k7::"]);
    assert!(both.contains(A) && both.contains(B), "{both}");
}
