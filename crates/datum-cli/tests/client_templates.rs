//! `%client_id%` in id and field templates, and `--group-by client` in
//! `count` and `stats` (da-z99wr): one keyed record, or one field, per
//! client, and a field's entries split by the client that wrote them,
//! labelled by the chain's name for it when the output is printed.

use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

struct Copy {
    tmp: TempDir,
}

impl Copy {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().expect("create tmpdir"),
        }
    }

    /// `datum args` as `client`: its stdout, asserting it succeeded.
    fn ok(&self, client: &str, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_datum"))
            .env("DATUM_DB", self.tmp.path().join("data.db"))
            .env("DATUM_HOME", self.tmp.path().join("home"))
            .env("DATUM_CONFIG_HOME", self.tmp.path().join("config"))
            .env("DATUM_COLOR", "0")
            .env("DATUM_CLIENT_ID", client)
            .args(args)
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "`datum {}`: {err}", args.join(" "));
        String::from_utf8(out.stdout).unwrap()
    }

    fn json(&self, client: &str, args: &[&str]) -> Value {
        let args = [args, &["--json"]].concat();
        serde_json::from_str(&self.ok(client, &args)).unwrap()
    }
}

#[test]
fn an_id_template_keys_one_record_per_client() {
    let c = Copy::new();
    for (client, level) in [("aaaa", "80"), ("bbbb", "60")] {
        let level = format!("level={level}");
        c.ok(client, &["occur", "battery", &level, "--id", "%client_id%"]);
    }
    let ids = c.ok("aaaa", &["ids", "battery", "--slug", "--oldest-first"]);
    assert_eq!(ids, "battery:aaaa\nbattery:bbbb\n");

    // The record is the client's: updating it by that id changes it in place.
    c.ok("aaaa", &["update", "battery:aaaa", "level=75"]);
    assert_eq!(c.ok("aaaa", &["count", "battery"]), "2\n");
    let got = c.json("aaaa", &["get", "battery:aaaa"]);
    assert_eq!(got["docs"][0]["data_inline"], "level: 75", "{got}");
}

#[test]
fn a_field_template_puts_each_client_in_its_own_field() {
    let c = Copy::new();
    c.ok("aaaa", &["occur", "phone_%client_id%", "n=1"]);
    c.ok("bbbb", &["occur", "phone_%client_id%", "n=2"]);
    assert_eq!(c.ok("aaaa", &["count", "phone_aaaa"]), "1\n");
    assert_eq!(c.ok("aaaa", &["count", "phone_bbbb"]), "1\n");
}

/// Two clients log one field; `count` and `stats` split it by client, and a
/// rename of the chain's label relabels the same rows.
#[test]
fn group_by_client_splits_a_field_and_follows_a_rename() {
    let c = Copy::new();
    for client in ["aaaa", "aaaa", "bbbb"] {
        c.ok(client, &["occur", "battery", "-q"]);
    }
    let count = |label_a: &str| {
        let rows = c.json("aaaa", &["count", "battery", "--group-by", "client"]);
        let want = serde_json::json!([
            {"key": label_a, "count": 2},
            {"key": "client bbbb", "count": 1},
        ]);
        assert_eq!(rows, want);
    };
    let stats = |label_a: &str| {
        let r = c.json("aaaa", &["stats", "battery", "--group-by", "client"]);
        let mut rows: Vec<(&str, &str, u64)> = (r["clients"].as_array().unwrap().iter())
            .map(|f| {
                let (client, label) = (f["client"].as_str(), f["label"].as_str());
                (
                    client.unwrap(),
                    label.unwrap(),
                    f["count"].as_u64().unwrap(),
                )
            })
            .collect();
        rows.sort();
        assert_eq!(rows, [("aaaa", label_a, 2), ("bbbb", "client bbbb", 1)]);
        assert_eq!(
            (r["field"].as_str(), r["total_count"].as_u64()),
            (Some("battery"), Some(3))
        );
    };

    // No chain: this copy knows its own client's kind, and no other's.
    count("cli aaaa");
    stats("cli aaaa");

    // Known to the chain: its label, read when printed.
    c.ok("aaaa", &["sync", "new-invite"]);
    c.ok("aaaa", &["sync", "chain", "rename", "phone"]);
    count("phone");
    stats("phone");
    c.ok("aaaa", &["sync", "chain", "rename", "tablet"]);
    count("tablet");
    stats("tablet");
    let text = c.ok("aaaa", &["stats", "battery", "--group-by", "client"]);
    assert!(text.starts_with("battery by client  ·"), "{text}");
    assert!(text.contains("2×  tablet\n"), "{text}");
}

/// Every span is credited to the client whose entry began it and ends at the
/// next change, whoever made it: the clients' time adds up to the field's.
#[test]
fn stats_by_client_splits_a_fields_time() {
    let c = Copy::new();
    c.ok(
        "aaaa",
        &["switch", "room", "kitchen", "-t", "yesterday 10:00"],
    );
    c.ok("bbbb", &["switch", "room", "desk", "-t", "yesterday 10:20"]);
    c.ok("aaaa", &["switch", "room", "sofa", "-t", "yesterday 10:50"]);
    let r = c.json("aaaa", &["stats", "room", "--group-by", "client"]);
    let secs = |client: &str| {
        let rows = r["clients"].as_array().unwrap();
        let row = rows.iter().find(|f| f["client"] == client).unwrap();
        row["duration_seconds"].as_i64().unwrap()
    };
    assert_eq!(secs("bbbb"), 30 * 60, "{r}");
    let whole = c.json("aaaa", &["stats", "room"]);
    let field = whole["tracked_duration_seconds"].as_i64().unwrap();
    assert_eq!(secs("aaaa") + secs("bbbb"), field, "{r}");

    // A window that opens inside kitchen reads only its own rows (da-307e0):
    // the span in effect at its start is still the client's that began it.
    let window = ["--since", "yesterday 10:10", "--until", "yesterday 11:00"];
    let r = c.json(
        "aaaa",
        &[&["stats", "room", "--group-by", "client"], &window[..]].concat(),
    );
    let secs = |client: &str| {
        let rows = r["clients"].as_array().unwrap();
        let row = rows.iter().find(|f| f["client"] == client).unwrap();
        row["duration_seconds"].as_i64().unwrap()
    };
    assert_eq!((secs("aaaa"), secs("bbbb")), (20 * 60, 30 * 60), "{r}");
}

/// An entry written with `-M` carries no client, and its row says so rather
/// than printing an empty id.
#[test]
fn an_entry_with_no_client_is_labelled_so() {
    let c = Copy::new();
    c.ok("aaaa", &["occur", "battery", "-M"]);
    let rows = c.json("aaaa", &["count", "battery", "--group-by", "client"]);
    assert_eq!(rows, serde_json::json!([{"key": "no client", "count": 1}]));
    let r = c.json("aaaa", &["stats", "battery", "--group-by", "client"]);
    assert_eq!(r["clients"][0]["label"], "no client", "{r}");
}
