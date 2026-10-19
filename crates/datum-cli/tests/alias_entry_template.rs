//! An alias fills its templates in `update` / `merge`'s entry argument, as a
//! log fills its field (da-61cne): `update issue__%issue_id%:doc -k issue_id`
//! names the doc its positional fills (`%-issue_id%` without storing the key),
//! and a template left unfilled is refused by name rather than read as an
//! entry that is not there.

use std::process::{Command, Output};

use tempfile::TempDir;

fn datum(tmp: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", tmp.path().join("data.db"))
        .env("DATUM_HOME", tmp.path().join("home"))
        .env("DATUM_CONFIG_HOME", tmp.path().join("config"))
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("spawn datum")
}

fn ok(tmp: &TempDir, args: &[&str]) -> String {
    let out = datum(tmp, args);
    assert!(
        out.status.success(),
        "`datum {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn update_and_merge_aliases_fill_the_entry_template() {
    let tmp = tempfile::tempdir().unwrap();
    let doc = "issue__abcd1234:doc";
    ok(
        &tmp,
        &["add", "issue__abcd1234", "--id", "doc", "-k", "title=first"],
    );
    ok(
        &tmp,
        &[
            "alias",
            "set",
            "ititle",
            "update issue__%-issue_id%:doc -k issue_id -k title",
        ],
    );
    ok(
        &tmp,
        &[
            "alias",
            "set",
            "iblock",
            "merge issue__%issue_id%:doc -k issue_id -k blocked_by",
        ],
    );

    // `%-issue_id%` fills the name only (da-n1rcs): the doc gains no key.
    ok(&tmp, &["ititle", "abcd1234", "second"]);
    assert!(!ok(&tmp, &["get", doc]).contains("issue_id"));
    // A plain `%issue_id%` keeps it, as a log's field template does.
    ok(&tmp, &["iblock", "abcd1234", "x"]);
    ok(&tmp, &["iblock", "abcd1234", "y"]);

    let got = ok(&tmp, &["get", doc]);
    assert!(got.contains(r#"title: "second""#), "{got}");
    assert!(got.contains(r#"blocked_by: ["x","y"]"#), "{got}");
    let history = ok(&tmp, &["history", doc]);
    assert!(history.contains("title=first"), "{history}");
    assert!(history.contains(r#"set data.title="second""#), "{history}");

    ok(
        &tmp,
        &["alias", "set", "inope", "update issue__%nope%:doc -k title"],
    );
    let out = datum(&tmp, &["inope", "third"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("%nope% has no value"), "{err}");
    // A key given empty has no value either.
    let out = datum(
        &tmp,
        &["update", "issue__%issue_id%:doc", "issue_id=", "title=x"],
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("%issue_id% has no value"), "{err}");
    assert!(ok(&tmp, &["get", doc]).contains(r#"title: "second""#));
}
