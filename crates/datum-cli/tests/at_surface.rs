//! da-o5bww: what a person types and reads calls the entry's time `at`, its
//! old names are still read, and `at` is no data key. The column and the ops
//! keep `occur_time` until da-95avu.

use serde_json::{json, Value};
use std::path::Path;
use std::process::{Command, Output};

const TIME: &str = "2026-05-01T10:00:00Z";

fn datum(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_datum"))
        .args(args)
        .env("HOME", dir)
        .env("DATUM_HOME", dir.join("home"))
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("XDG_STATE_HOME", dir.join("state"))
        .env("DATUM_DB", dir.join("t.db"))
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("TZ", "UTC")
        .output()
        .unwrap()
}

/// `args`' stdout, the command having succeeded.
fn ok(dir: &Path, args: &[&str]) -> String {
    let out = datum(dir, args);
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{args:?}: {stdout}\n{stderr}");
    stdout
}

/// `args`' stderr, the command having been refused, and nothing written.
fn refused(dir: &Path, args: &[&str]) -> String {
    let before = ok(dir, &["tail", "--json"]);
    let out = datum(dir, args);
    assert!(!out.status.success(), "{args:?} was not refused");
    assert_eq!(ok(dir, &["tail", "--json"]), before, "{args:?} wrote");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn pretty(dir: &Path, id: &str) -> Value {
    serde_json::from_str(&ok(dir, &["get", id, "--pretty"])).unwrap()
}

fn dir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn at_and_occur_time_write_the_same_entry() {
    let same = |e: &Value| {
        json!([
            e["slug"],
            e["at"],
            e["data"],
            e["verb"],
            e["dur"],
            e["content_hash"]
        ])
    };
    let (a, b) = (dir(), dir());
    ok(
        a.path(),
        &["occur", "teapot", &format!("at={TIME}"), "cups=1"],
    );
    ok(
        b.path(),
        &["occur", "teapot", &format!("occur_time={TIME}"), "cups=1"],
    );
    let (ea, eb) = (pretty(a.path(), "teapot"), pretty(b.path(), "teapot"));
    assert_eq!(same(&ea), same(&eb));
    assert_eq!(ea["slug"], "teapot:2026-05-01T10:00:00.000Z");
    assert_eq!(ea["data"], json!({"cups": 1}), "the time left in data");

    // The same on an update, and a relative one anchors at the entry.
    ok(a.path(), &["update", "teapot", "at=+5"]);
    ok(b.path(), &["update", "teapot", "occur_time=+5"]);
    let (ea, eb) = (pretty(a.path(), "_LAST"), pretty(b.path(), "_LAST"));
    assert_eq!(ea["slug"], "teapot:2026-05-01T10:05:00.000Z");
    assert_eq!(same(&ea), same(&eb));

    // And the uncertainty, by either name.
    let (a, b) = (dir(), dir());
    ok(
        a.path(),
        &[
            "occur",
            "teapot",
            &format!("at={TIME}"),
            "at_uncertainty=5m",
        ],
    );
    ok(
        b.path(),
        &[
            "occur",
            "teapot",
            &format!("at={TIME}"),
            "occur_time_uncertainty=5m",
        ],
    );
    let (ea, eb) = (pretty(a.path(), "teapot"), pretty(b.path(), "teapot"));
    assert_eq!(ea["at"]["uncertainty"], "5m");
    assert_eq!(ea["at"], eb["at"]);
}

/// `at=` makes `add`'s first bare word no id, as `occur_time=` does.
#[test]
fn at_is_a_time_to_add_as_the_old_name_is() {
    let d = dir();
    ok(d.path(), &["add", "teapot", &format!("at={TIME}")]);
    assert_eq!(
        pretty(d.path(), "teapot")["slug"],
        "teapot:2026-05-01T10:00:00.000Z"
    );
}

#[test]
fn what_a_person_reads_says_at() {
    let d = dir();
    let d = d.path();
    ok(d, &["occur", "teapot", &format!("at={TIME}"), "cups=1"]);
    ok(d, &["update", "teapot", "at=+5"]);

    let entry = pretty(d, "teapot");
    assert_eq!(entry["at"]["utc"], "2026-05-01T10:05:00Z");
    assert!(entry.get("occur_time").is_none(), "{entry}");
    // `get --pretty` keeps the columns' order: the time after `data`.
    let text = ok(d, &["get", "teapot", "--pretty"]);
    let (data, at) = (text.find("\n  \"data\":"), text.find("\n  \"at\":"));
    assert!(data < at && data.is_some(), "{text}");

    // A column by name, by either name, and the search's key.
    assert_eq!(ok(d, &["get", "teapot::at"]).trim(), "2026-05-01T10:05:00Z");
    assert_eq!(
        ok(d, &["get", "teapot::occur_time"]).trim(),
        "2026-05-01T10:05:00Z"
    );

    // `--json`: the history names `at` in what each version did.
    let history: Value = serde_json::from_str(&ok(d, &["history", "teapot", "--json"])).unwrap();
    assert_eq!(
        history[0]["effect"]["set"]["at"]["utc"],
        "2026-05-01T10:05:00Z"
    );
    assert!(
        history[1]["effect"]["create"].get("at").is_some(),
        "{history}"
    );
    // Naming the keys adds none: a create unset nothing.
    assert!(history[1]["effect"].get("unset").is_none(), "{history}");
    let text = history.to_string();
    assert!(!text.contains("occur_time\":"), "{text}");
    assert!(ok(d, &["why", "teapot", "--full"]).contains("set at="));
    assert!(ok(d, &["history", "teapot", "--diff", "1", "2"]).contains("at.utc"));
    let snapshot: Value =
        serde_json::from_str(&ok(d, &["history", "teapot", "--version", "1"])).unwrap();
    assert!(snapshot.get("at").is_some(), "{snapshot}");

    // The show-all labels.
    let all = ok(d, &["get", "teapot", "--show", "all"]);
    for label in ["  at ", "  at_offset ", "  at_tz ", "  at_uncertainty "] {
        assert!(all.contains(label), "{label:?} in {all}");
    }
    assert!(!all.contains("  occur_time"), "{all}");

    // Help.
    assert!(ok(d, &["help", "retime"]).contains("`at`"));
    assert!(!ok(d, &["help", "retime"]).contains("occur_time"));
}

/// As stored, the time keeps its old name until da-95avu: a backup's lines
/// are what a sync, a restore and datum.day read.
#[test]
fn a_backup_still_says_occur_time() {
    let d = dir();
    ok(d.path(), &["occur", "teapot", &format!("at={TIME}")]);
    let backup = ok(d.path(), &["db", "backup", "-"]);
    assert!(backup.contains("\"occur_time\":"), "{backup}");
}

#[test]
fn a_template_reads_at_and_its_old_names_alike() {
    let d = dir();
    let d = d.path();
    let at = format!("at={TIME}");
    for (field, id) in [
        ("aspen", "%at%"),
        ("birch", "%occur_time%"),
        ("cedar", "%occurTime%"),
    ] {
        ok(d, &["occur", field, &at, "--id", id]);
        assert_eq!(
            pretty(d, field)["slug"],
            format!("{field}:2026-05-01T10:00:00.000Z"),
            "{id}"
        );
    }
    // A slug on `%at%` follows the time as the default one does.
    ok(d, &["update", "aspen", "at=+5"]);
    assert_eq!(pretty(d, "aspen")["slug"], "aspen:2026-05-01T10:05:00.000Z");
    // And `retime -T` takes it away from a `%at%` id as from the default's.
    ok(d, &["retime", "birch:2026-05-01T10:00:00.000Z", "-T"]);
    assert_eq!(pretty(d, "birch")["id_structure"], "%create_time%");
    ok(d, &["retime", "aspen:2026-05-01T10:05:00.000Z", "-T"]);
    assert_eq!(pretty(d, "aspen")["id_structure"], "%create_time%");
}

#[test]
fn at_as_a_data_key_is_refused_on_a_new_write_saying_why() {
    let d = dir();
    let d = d.path();
    ok(d, &["occur", "teapot", &format!("at={TIME}"), "where=home"]);
    let why = "no key of your own can be named at";

    for line in [
        &["occur", "teapot", "at=home"][..],
        &["occur", "teapot", "at+=home"],
        &["occur", "teapot", "at-=home"],
        &["occur", "teapot", "at.place=home"],
        &["update", "teapot", "at=home"],
        &["update", "teapot", "at+=home"],
        &["merge", "teapot", "at=home"],
        &["rekey", "teapot", "where=at"],
    ] {
        let said = refused(d, line);
        assert!(said.contains(why), "{line:?}: {said}");
    }
    // A key under another, and a name that only starts so, are data.
    ok(d, &["update", "teapot", "place.at=home", "attic=1"]);

    // An editor can't add it either, and the old row's edit stays open.
    let script = d.join("editor.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nsed -i 's/\"data\": {/\"data\": {\"at\": 1,/' \"$1\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .args(["edit", "teapot"])
        .env("HOME", d)
        .env("DATUM_HOME", d.join("home"))
        .env("XDG_CONFIG_HOME", d.join("config"))
        .env("XDG_DATA_HOME", d.join("data"))
        .env("XDG_STATE_HOME", d.join("state"))
        .env("DATUM_DB", d.join("t.db"))
        .env("EDITOR", &script)
        .env("VISUAL", &script)
        .env("TZ", "UTC")
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && said.contains(why), "{said}");
    assert!(pretty(d, "teapot")["data"].get("at").is_none());
}

/// The import keeps a v2 doc's key `at` as data and says so; `check` names
/// it from then on; `rekey` clears it, and a row holding one stays editable.
#[test]
fn an_existing_key_is_reported_by_the_import_and_check() {
    let d = dir();
    let d = d.path();
    let backup = d.join("backup.json");
    let doc = |id: &str, data: Value| json!({ "_id": id, "data": data, "meta": { "createTime": { "utc": TIME } } });
    let time = json!({ "utc": TIME, "o": 0 });
    let docs = json!({
        "backupTime": TIME,
        "docs": [
            doc("plain:1", json!({ "field": "plain", "occurTime": time })),
            doc("visit:1", json!({ "field": "visit", "occurTime": time, "at": "home" })),
            doc("ideas:1", json!({ "field": "ideas", "at": "desk" })),
        ],
    });
    std::fs::write(&backup, docs.to_string()).unwrap();
    let said = ok(d, &["db", "v2import", "--from", backup.to_str().unwrap()]);
    assert!(said.contains("3 imported"), "{said}");
    assert!(said.contains("visit:1") && said.contains("`at`"), "{said}");
    assert!(!said.contains("plain:1"), "{said}");

    // Kept as data, and not read as the time: that is v2's `occurTime`.
    let visit = pretty(d, "visit");
    assert_eq!(visit["data"]["at"], "home");
    assert_eq!(visit["at"]["utc"], "2026-05-01T10:00:00Z");
    assert_eq!(pretty(d, "plain")["at"]["utc"], "2026-05-01T10:00:00Z");
    // A column wins a name, so `::at` is the time; with no time, the key.
    assert_eq!(ok(d, &["get", "visit::at"]).trim(), "2026-05-01T10:00:00Z");
    assert_eq!(ok(d, &["get", "ideas::at"]).trim(), "desk");

    let found = |d: &Path| -> Vec<Value> {
        let out = datum(d, &["check", "entries", "--show", "json"]);
        let report: Value = serde_json::from_slice(&out.stdout).unwrap();
        let text = report.to_string();
        let mut hits = Vec::new();
        collect(&report, &mut hits);
        assert_eq!(hits.is_empty(), !text.contains("reserved_data_key"));
        hits
    };
    let hits = found(d);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0]["count"], 2, "{hits:?}");
    assert_eq!(hits[0]["severity"], "warning", "a warning, not an error");
    let fields: Vec<_> = hits[0]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| &e["field"])
        .collect();
    assert!(
        fields.contains(&&json!("visit")) && fields.contains(&&json!("ideas")),
        "{hits:?}"
    );
    assert!(hits[0]["message"]
        .as_str()
        .unwrap()
        .starts_with("2 entries hold a"));
    assert!(hits[0]["message"].as_str().unwrap().contains("rekey"));

    // A row holding one is still edited, and the time set beside it.
    ok(d, &["update", "visit", "cups=2", "at=+5"]);
    let visit = pretty(d, "_LAST");
    assert_eq!(visit["data"]["at"], "home");
    assert_eq!(visit["at"]["utc"], "2026-05-01T10:05:00Z");

    // And what the finding says clears it.
    ok(d, &["rekey", "_LAST", "at=place"]);
    assert_eq!(pretty(d, "_LAST")["data"]["place"], "home");
    let hits = found(d);
    assert!(hits[0]["message"]
        .as_str()
        .unwrap()
        .starts_with("1 entry holds a"));
    ok(d, &["rekey", "ideas", "at=place"]);
    assert!(found(d).is_empty());
}

fn collect(v: &Value, hits: &mut Vec<Value>) {
    match v {
        Value::Object(o) => {
            if o.get("category").is_some_and(|c| c == "reserved_data_key") {
                hits.push(v.clone());
            }
            o.values().for_each(|c| collect(c, hits));
        }
        Value::Array(a) => a.iter().for_each(|c| collect(c, hits)),
        _ => {}
    }
}
