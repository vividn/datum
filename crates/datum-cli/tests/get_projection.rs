//! `datum get <id>::<name>` (da-u7mgc): one value of an entry, alone on
//! stdout, for scripts — and `::` refused in the ids it would split.

use std::process::{Command, Output};

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
            .env("DATUM_COLOR", "0")
            .args(args)
            .output()
            .expect("spawn datum")
    }

    /// stdout of a command that must succeed.
    fn ok(&self, args: &[&str]) -> String {
        let out = self.out(args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "datum {args:?} failed: {stderr}");
        String::from_utf8(out.stdout).expect("utf-8 stdout")
    }

    /// stderr of a command that must fail, with nothing on stdout.
    fn err(&self, args: &[&str]) -> String {
        let out = self.out(args);
        assert!(!out.status.success(), "datum {args:?} must fail");
        assert!(out.stdout.is_empty(), "datum {args:?} printed on stdout");
        String::from_utf8(out.stderr).expect("utf-8 stderr")
    }
}

/// Seed one entry with a structured state and two data keys, one of them
/// an empty string; return its full hid.
fn seeded() -> (Env, String) {
    let env = Env::new();
    env.ok(&[
        "switch",
        "issue",
        r#"{"title":"Dune","by":"vividn"}"#,
        "priority=2",
        r#"note="""#,
    ]);
    let hid = env.ok(&["get", "_::hid"]).trim_end().to_string();
    (env, hid)
}

#[test]
fn hid_from_a_prefix_is_the_full_twelve_chars() {
    let (env, hid) = seeded();
    assert_eq!(hid.len(), 12);
    assert_eq!(
        env.ok(&["get", &format!("{}::hid", &hid[..5])]),
        format!("{hid}\n")
    );
}

/// An empty id is `_LAST`, as a bare `datum get` is.
#[test]
fn an_empty_id_reads_the_last_entry() {
    let (env, hid) = seeded();
    assert_eq!(env.ok(&["get", "::hid"]), format!("{hid}\n"));
    assert_eq!(env.ok(&["get", "::.title"]), "Dune\n");
}

/// A list of values is all `<id>::<name>`: the pieces before a plain id
/// print, the plain id is named, and the command fails.
#[test]
fn a_plain_id_in_a_list_of_values_is_refused_by_name() {
    let (env, hid) = seeded();
    let out = env.out(&["get", &format!("{}::hid,{}", &hid[..5], &hid[..5])]);
    assert!(!out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), format!("{hid}\n"));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("is not <id>::<name>"), "{stderr}");
}

#[test]
fn state_prints_as_compact_json_and_dot_keys_walk_it() {
    let (env, hid) = seeded();
    let id = &hid[..5];
    assert_eq!(
        env.ok(&["get", &format!("{id}::state")]),
        "{\"by\":\"vividn\",\"title\":\"Dune\"}\n"
    );
    assert_eq!(env.ok(&["get", &format!("{id}::.title")]), "Dune\n");
    // The `<field>:_` shorthand is an id like any other.
    assert_eq!(env.ok(&["get", "issue:_::.by"]), "vividn\n");
}

#[test]
fn a_column_name_means_the_column_and_anything_else_is_data() {
    let (env, hid) = seeded();
    let id = &hid[..5];
    assert_eq!(env.ok(&["get", &format!("{id}::field")]), "issue\n");
    assert_eq!(env.ok(&["get", &format!("{id}::priority")]), "2\n");
}

#[test]
fn a_comma_list_prints_one_value_per_line_in_order() {
    let (env, hid) = seeded();
    let id = &hid[..5];
    assert_eq!(
        env.ok(&["get", &format!("{id}::.title,{id}::field,{id}::priority")]),
        "Dune\nissue\n2\n"
    );
}

/// Output flags do not apply to a value, but a bad one is still refused.
#[test]
fn a_bad_output_flag_is_refused() {
    let (env, hid) = seeded();
    let stderr = env.err(&["get", &format!("{}::hid", &hid[..5]), "--show", "format"]);
    assert!(stderr.contains("--show format requires -o"), "{stderr}");
}

/// Only the first `::` splits, so a data key holding one stays reachable.
#[test]
fn a_name_may_contain_double_colon() {
    let env = Env::new();
    env.ok(&["occur", "coffee", "a::b=1"]);
    assert_eq!(env.ok(&["get", "_::a::b"]), "1\n");
}

/// The pair a script must tell apart: absent fails, empty succeeds.
#[test]
fn missing_key_fails_but_empty_string_prints_an_empty_line() {
    let (env, hid) = seeded();
    let id = &hid[..5];
    assert_eq!(env.ok(&["get", &format!("{id}::note")]), "\n");
    for name in ["nope", ".nope", "dur"] {
        let stderr = env.err(&["get", &format!("{id}::{name}")]);
        assert!(
            stderr.contains(&format!("has no {name}")),
            "{name}: {stderr}"
        );
    }
}

#[test]
fn double_colon_is_refused_in_a_slug_body_and_a_field_name() {
    let env = Env::new();
    // The default `:` delimiter collapses a run of colons, so `::` reaches
    // a slug only through another delimiter — a literal id, a `-K` key.
    let stderr = env.err(&["add", "thing", "id=x::y", "--id-delimiter", "_"]);
    assert!(stderr.contains("cannot contain '::'"), "{stderr}");
    let stderr = env.err(&[
        "add",
        "thing",
        "name=a::b",
        "-K",
        "name",
        "--id-delimiter",
        "_",
    ]);
    assert!(stderr.contains("cannot contain '::'"), "{stderr}");
    // …and a rekey that would re-derive one.
    env.ok(&[
        "add",
        "thing",
        "name=ok",
        "-K",
        "name",
        "--id-delimiter",
        "_",
    ]);
    let stderr = env.err(&["update", "thing:ok", "name=p::q", "--id-delimiter", "_"]);
    assert!(stderr.contains("cannot contain '::'"), "{stderr}");
    assert_eq!(env.ok(&["get", "thing:_::slug"]), "thing:ok\n");

    let stderr = env.err(&["occur", "a::b"]);
    assert!(stderr.contains("cannot contain ':'"), "{stderr}");
}

#[test]
fn get_help_documents_the_syntax_and_the_precedence() {
    let help = Env::new().ok(&["get", "--help"]);
    assert!(help.contains("<ID_OR_SLUG>::<NAME>"), "{help}");
    assert!(help.contains("always means the column"), "{help}");
}
