//! da-kjikt: one `ref` column, an entry's `_n` where it has one, else its
//! hid (vividn, 2026-11-03: «if something has a _n id, use that, otherwise
//! use the hid … keep hid in the json and database»).

use serde_json::Value;
use std::process::Command;
use tempfile::TempDir;

fn datum(dir: &TempDir, args: &[&str]) -> String {
    let d = dir.path();
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .args(args)
        .env("HOME", d)
        .env("DATUM_HOME", d.join("home"))
        .env("DATUM_CONFIG_HOME", d.join("config"))
        .env("XDG_CONFIG_HOME", d.join("xc"))
        .env("XDG_DATA_HOME", d.join("xd"))
        .env("XDG_STATE_HOME", d.join("xs"))
        .env("DATUM_DB", d.join("data.db"))
        .env("DATUM_SYNC_NO_DAEMON", "1")
        .env("NO_COLOR", "1")
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}

/// The first cell of each line: the header word, then each row's ref.
fn refs(tail: &str) -> Vec<String> {
    tail.lines()
        .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
        .collect()
}

#[test]
fn tail_has_one_ref_column_which_moves_as_entries_come_in() {
    let dir = TempDir::new().unwrap();
    datum(&dir, &["moment", "coffee", "-t", "-2"]);
    let tail = datum(&dir, &["tail"]);
    assert_eq!(refs(&tail), ["ref", "_1"], "{tail}");
    let header = tail.lines().next().unwrap();
    assert!(!header.contains("hid") && !header.contains("_n"), "{tail}");

    datum(&dir, &["moment", "tea"]);
    assert_eq!(refs(&datum(&dir, &["tail"])), ["ref", "_2", "_1"]);

    // A meta row has no `_n`: its ref is its hid.
    let specs: Vec<String> = datum(&dir, &["ids", "spec"])
        .lines()
        .map(str::to_string)
        .collect();
    let all = datum(&dir, &["tail", "-A"]);
    let spec_row = all.lines().find(|l| l.contains(" spec ")).unwrap();
    let shown = spec_row.split_whitespace().next().unwrap();
    assert!(specs.iter().any(|h| h.starts_with(shown)), "{all}");
}

#[test]
fn json_keeps_hid_and_adds_no_ref_key() {
    let dir = TempDir::new().unwrap();
    datum(&dir, &["moment", "coffee"]);
    let doc: Value = serde_json::from_str(&datum(&dir, &["tail", "--json"])).unwrap();
    let row = doc["rows"][0].as_object().unwrap();
    assert_eq!(row["hid"].as_str().unwrap().len(), 12);
    assert_eq!(row["quick_id"], "_1");
    assert!(!row.contains_key("ref"));
    assert_eq!(doc["columns"][0]["key"], "ref");
    assert_eq!(doc["columns"][0]["cells"][0], "_1");
}

#[test]
fn ids_ref_names_each_n_and_its_hid() {
    let dir = TempDir::new().unwrap();
    datum(&dir, &["moment", "coffee", "-t", "-2"]);
    datum(&dir, &["moment", "tea"]);
    let hid = |field: &str| datum(&dir, &["ids", field]).trim().to_string();
    let map: Value = serde_json::from_str(&datum(&dir, &["ids", "--ref", "--json"])).unwrap();
    assert_eq!(
        map,
        serde_json::json!({"_1": hid("tea"), "_2": hid("coffee")})
    );
    let text = datum(&dir, &["ids", "--ref", "-n", "1"]);
    assert_eq!(text, format!("_1  {}\n", hid("tea")));
}
