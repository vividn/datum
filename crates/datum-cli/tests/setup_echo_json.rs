//! A setup's `--show json` document is what the terminal prints for it
//! (da-a3u8x): the field, its kind and its states, and no epoch time. The
//! spec entry under it is timed at 1970 and holds the spec's raw keys,
//! which is what datum.day drew before.

use std::process::Command;

#[test]
fn a_setup_echoes_its_summary_not_its_spec_entry() {
    let tmp = tempfile::TempDir::new().unwrap();
    let datum = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", tmp.path().join("t.db"))
            .env("DATUM_HOME", tmp.path())
            .env("DATUM_CONFIG_HOME", tmp.path())
            .env("NO_COLOR", "1")
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };
    let echo: serde_json::Value = serde_json::from_str(&datum(&[
        "setup",
        "mood",
        "--kind",
        "change",
        "--states",
        "happy,sad",
        "--show",
        "json",
    ]))
    .unwrap();
    let doc = &echo["docs"][0];
    assert_eq!(doc["info"], "mood  change  tracked", "{doc}");
    assert_eq!(doc["info_time"], "", "{doc}");
    assert_eq!(
        doc["data"],
        serde_json::json!(["states  happy · sad"]),
        "{doc}"
    );
    assert!(!doc.to_string().contains("1970"), "{doc}");

    // An update's lines carry the terminal's marks.
    let printed = datum(&["setup", "mood", "--color", "red"]);
    let echo: serde_json::Value = serde_json::from_str(&datum(&[
        "setup", "mood", "--color", "blue", "--show", "json",
    ]))
    .unwrap();
    let marked = echo["docs"][0]["data"][0].as_str().unwrap();
    assert!(
        marked.starts_with("~ color"),
        "{marked:?} beside {printed:?}"
    );
}
