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
            .env("NO_COLOR", "1")
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

/// da-pimix: `::` after a name is a hop, so a data key holding one is no
/// longer read whole — the first piece is the name.
#[test]
fn a_further_double_colon_is_a_hop_not_part_of_a_key() {
    let env = Env::new();
    env.ok(&["occur", "coffee", "a::b=1", "a=2"]);
    let stderr = env.err(&["get", "_::a::b"]);
    assert!(
        stderr.contains("_::a is 2, which refers to nothing"),
        "{stderr}"
    );
}

/// An `issue` spec whose bare states refer through `issue_state`, one
/// document `issue_state:queued` and an issue in it; return the issue's hid.
fn tracker() -> (Env, String) {
    let env = Env::new();
    env.ok(&[
        "setup",
        "issue",
        "--verb",
        "switch",
        "--state-lookup-field",
        "issue_state",
    ]);
    env.ok(&["add", "issue_state", "--id", "queued", "class=waiting"]);
    env.ok(&["switch", "issue", "queued"]);
    let hid = env.ok(&["get", "issue:_::hid"]).trim_end().to_string();
    (env, hid)
}

#[test]
fn state_hops_into_the_doc_a_bare_state_names_through_the_lookup_field() {
    let (env, hid) = tracker();
    let id = &hid[..5];
    assert_eq!(
        env.ok(&["get", &format!("{id}::state::class")]),
        "waiting
"
    );
    // Every name reads by the same order on the far side: a column…
    assert_eq!(
        env.ok(&["get", &format!("{id}::state::slug")]),
        "issue_state:queued
"
    );
    // Control: no further hop prints the state itself, not the doc.
    assert_eq!(
        env.ok(&["get", &format!("{id}::state")]),
        "queued
"
    );
    assert_eq!(
        env.ok(&["get", &format!("{id}::field")]),
        "issue
"
    );
}

#[test]
fn a_full_slug_state_hops_and_hops_chain() {
    let (env, _) = tracker();
    env.ok(&["add", "book", "--id", "dune", "title=Dune"]);
    env.ok(&["switch", "media", "book:dune"]);
    // One hop through a `field:id` state, on a field with no lookup field.
    assert_eq!(
        env.ok(&["get", "media:_::state::title"]),
        "Dune
"
    );
    // Two: media → book:dune → the issue, named by its real slug (a state
    // is a slug; the `issue:_` shorthand is an id, not a reference).
    let issue = env.ok(&["get", "issue:_::slug"]).trim_end().to_string();
    env.ok(&["update", "book:dune", &format!("state={issue}")]);
    assert_eq!(
        env.ok(&["get", "media:_::state::state::state::class"]),
        "waiting
"
    );
}

#[test]
fn a_dangling_reference_names_the_missing_slug() {
    let (env, hid) = tracker();
    env.ok(&["switch", "issue", "shipped"]);
    let stderr = env.err(&["get", "issue:_::state::class"]);
    assert!(
        stderr.contains("issue:_::state refers to issue_state:shipped, which does not exist"),
        "{stderr}"
    );
    // Control: the entry that exists still reads.
    assert_eq!(
        env.ok(&["get", &format!("{}::state::class", &hid[..5])]),
        "waiting
"
    );
    // A missing value on the far side is still "has no".
    let stderr = env.err(&["get", &format!("{}::state::nope", &hid[..5])]);
    assert!(stderr.contains("::state has no nope"), "{stderr}");
}

#[test]
fn a_value_that_refers_to_nothing_refuses_a_further_hop() {
    let env = Env::new();
    env.ok(&["switch", "mood", "open", "note=x"]);
    let stderr = env.err(&["get", "mood:_::state::class"]);
    assert!(
        stderr.contains(r#"mood:_::state is "open", which refers to nothing"#),
        "{stderr}"
    );
    // A data value refers only by `ref://` or a ref-typed key (da-s5k2s):
    // a bare `book:x` in data is not followed.
    env.ok(&["occur", "read", "what=book:x"]);
    let stderr = env.err(&["get", "read:_::what::title"]);
    assert!(stderr.contains("which refers to nothing"), "{stderr}");
    // Control: without the hop both read.
    assert_eq!(
        env.ok(&["get", "mood:_::state"]),
        "open
"
    );
    assert_eq!(
        env.ok(&["get", "mood:_::note"]),
        "x
"
    );
}

#[test]
fn an_array_state_asks_for_an_element() {
    let env = Env::new();
    env.ok(&["add", "book", "--id", "a"]);
    env.ok(&["switch", "media", r#"["book:a","book:a"]"#]);
    let stderr = env.err(&["get", "media:_::state::slug"]);
    assert!(stderr.contains("is an array"), "{stderr}");
    assert!(stderr.contains("get an element's own slug"), "{stderr}");
}

#[test]
fn a_cycle_errors_with_the_chain() {
    let env = Env::new();
    env.ok(&["add", "a", "--id", "1", "state=b:1"]);
    env.ok(&["add", "b", "--id", "1", "state=a:1"]);
    // Control: one hop each way reads.
    assert_eq!(
        env.ok(&["get", "a:1::state::slug"]),
        "b:1
"
    );
    let stderr = env.err(&["get", "a:1::state::state::slug"]);
    assert!(
        stderr.contains("a:1::state::state loops back to a:1: a:1 → b:1 → a:1"),
        "{stderr}"
    );
}

#[test]
fn a_ninth_hop_errors_with_the_chain() {
    let env = Env::new();
    for i in 0..10 {
        env.ok(&[
            "add",
            &format!("n{i}"),
            "--id",
            "x",
            &format!("state=n{}:x", i + 1),
        ]);
    }
    let path = |hops: usize| format!("n0:x{}::slug", "::state".repeat(hops));
    // Control: eight hops is the bound, and reads.
    assert_eq!(env.ok(&["get", &path(8)]), "n8:x\n");
    let stderr = env.err(&["get", &path(9)]);
    assert!(stderr.contains("goes past 8 hops"), "{stderr}");
    assert!(
        stderr.contains("n0:x → n1:x → n2:x → n3:x → n4:x → n5:x → n6:x → n7:x → n8:x → n9:x"),
        "{stderr}"
    );
}

/// da-s5k2s: a data key hops when its value is a reference — `ref://<slug>`
/// on any key — through the same reader as a state, and the two chain.
#[test]
fn a_data_ref_hops_and_chains_with_state_hops() {
    let env = Env::new();
    env.ok(&["add", "book", "--id", "dune", "title=Dune"]);
    env.ok(&["add", "shelf", "--id", "top", "holds=ref://book:dune"]);
    env.ok(&["occur", "read", "what=ref://book:dune"]);
    assert_eq!(env.ok(&["get", "read:_::what::title"]), "Dune\n");
    // Control: no hop prints the reference as stored.
    assert_eq!(env.ok(&["get", "read:_::what"]), "ref://book:dune\n");
    // Two data hops, and a state hop into a data hop.
    env.ok(&["occur", "visit", "where=ref://shelf:top"]);
    assert_eq!(env.ok(&["get", "visit:_::where::holds::title"]), "Dune\n");
    env.ok(&["switch", "media", "shelf:top"]);
    assert_eq!(env.ok(&["get", "media:_::state::holds::title"]), "Dune\n");
}

#[test]
fn a_bare_slug_hops_only_on_a_key_the_spec_types_ref() {
    let env = Env::new();
    env.ok(&[
        "setup",
        "task",
        "--from-json",
        r#"{"field":"task","verb":"occur","subfields":{"blocker":{"value_type":"ref"}}}"#,
    ]);
    env.ok(&["add", "issue", "--id", "k7f2a", "title=Fix it"]);
    env.ok(&["occur", "task", "blocker=issue:k7f2a"]);
    assert_eq!(env.ok(&["get", "task:_::blocker::title"]), "Fix it\n");
    // Control: the same value on a field without the spec type.
    env.ok(&["occur", "chore", "blocker=issue:k7f2a"]);
    let stderr = env.err(&["get", "chore:_::blocker::title"]);
    assert!(
        stderr.contains(r#"chore:_::blocker is "issue:k7f2a", which refers to nothing"#),
        "{stderr}"
    );
}

#[test]
fn a_dangling_or_looping_data_ref_errors() {
    let env = Env::new();
    env.ok(&["occur", "read", "what=ref://book:nope"]);
    let stderr = env.err(&["get", "read:_::what::title"]);
    assert!(
        stderr.contains("read:_::what refers to book:nope, which does not exist"),
        "{stderr}"
    );
    env.ok(&["add", "a", "--id", "1", "next=ref://b:1"]);
    env.ok(&["add", "b", "--id", "1", "next=ref://a:1"]);
    assert_eq!(env.ok(&["get", "a:1::next::slug"]), "b:1\n");
    let stderr = env.err(&["get", "a:1::next::next::slug"]);
    assert!(
        stderr.contains("a:1::next::next loops back to a:1: a:1 → b:1 → a:1"),
        "{stderr}"
    );
}

/// A literal `ref://` string — typed `str://ref://…` or JSON-quoted — is
/// stored `str://ref://…`, reads without the `str://`, and is no reference.
#[test]
fn a_literal_ref_string_reads_bare_and_does_not_hop() {
    let env = Env::new();
    env.ok(&["add", "book", "--id", "dune", "title=Dune"]);
    let echo = env.ok(&[
        "occur",
        "q",
        r#"a="ref://book:dune""#,
        "b=str://ref://book:dune",
    ]);
    assert!(echo.contains(r#"  a: "ref://book:dune""#), "{echo}");
    assert!(echo.contains(r#"  b: "ref://book:dune""#), "{echo}");
    assert!(!echo.contains("str://"), "{echo}");
    assert_eq!(env.ok(&["get", "q:_::a"]), "ref://book:dune\n");
    assert_eq!(
        env.ok(&["get", "q:_::data"]),
        r#"{"a":"str://ref://book:dune","b":"str://ref://book:dune"}"#.to_string() + "\n",
        "stored with the mark"
    );
    let stderr = env.err(&["get", "q:_::a::title"]);
    assert!(stderr.contains("which refers to nothing"), "{stderr}");
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
    let stderr = env.err(&["add", "thing", "--id", "x::y", "--id-delimiter", "_"]);
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
    assert!(
        help.contains("datum get <ID_OR_SLUG>::<REF>[::<REF>]...::<NAME>"),
        "{help}"
    );
}
