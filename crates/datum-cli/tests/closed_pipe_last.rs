//! A reader closing stdout ends a command at its first print (exit 141,
//! da-9p3o), so everything the command records — the entry and the `_LAST`
//! pointer — must land before it prints (da-ki5j3). Each test runs a command
//! with its stdout on a pipe whose read end is already closed, then an
//! `undo`: it must revert what that command touched. A write is what a bare
//! `undo` reverts; a `get` moves only `_LAST`, so after one it is
//! `undo _LAST` (da-z0zh3).
//! A closed read end is the strict form of `| head`: the first write fails.

use std::path::PathBuf;
use std::process::{Command, Output};

use tempfile::TempDir;

struct Env {
    _tmp: TempDir,
    db: PathBuf,
    home: PathBuf,
}

impl Env {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        Self {
            db: tmp.path().join("t.db"),
            home,
            _tmp: tmp,
        }
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_datum"));
        c.env("DATUM_DB", &self.db)
            .env("DATUM_HOME", &self.home)
            .env("DATUM_CONFIG_HOME", &self.home)
            .env("HOME", &self.home)
            .env("NO_COLOR", "1")
            .args(args);
        c
    }

    fn ok(&self, args: &[&str]) -> Output {
        let out = self.cmd(args).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", stderr(&out));
        out
    }

    /// Log `field` at `time` and return its hid.
    fn occur(&self, field: &str, time: &str) -> String {
        let out = self.ok(&["occur", field, "-t", time, "--show", "json"]);
        let docs: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        docs["docs"][0]["hid"].as_str().unwrap().to_string()
    }

    /// Run `args` with its stdout closed: it must be cut off (141).
    fn cut_off(&self, args: &[&str]) {
        let (reader, writer) = std::io::pipe().unwrap();
        drop(reader);
        let out = self.cmd(args).stdout(writer).output().unwrap();
        assert_eq!(out.status.code(), Some(141), "{args:?}: {}", stderr(&out));
    }

    /// The live entries as `field=state`, oldest first. `tail` leaves
    /// `_LAST` alone, so reading this never moves what `undo` acts on.
    fn live(&self) -> Vec<String> {
        let out = self.ok(&["tail", "--json"]);
        let rows: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        rows["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| format!("{}={}", r["field"].as_str().unwrap(), r["state"]))
            .collect()
    }
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn a_cut_off_log_is_what_a_bare_undo_reverts() {
    let env = Env::new();
    env.occur("tea", "0900");
    env.cut_off(&["occur", "coffee", "-t", "0910"]);
    assert_eq!(env.live(), ["tea=null", "coffee=null"]);
    env.ok(&["undo"]);
    assert_eq!(env.live(), ["tea=null"]);
}

#[test]
fn a_cut_off_get_is_what_undo_last_reverts() {
    let env = Env::new();
    let tea = env.occur("tea", "0900");
    let coffee = env.occur("coffee", "0910");
    env.occur("water", "0920");
    env.cut_off(&["get", &format!("{tea},{coffee}")]);
    env.ok(&["undo", "_LAST"]);
    assert_eq!(env.live(), ["water=null"]);
}

#[test]
fn a_cut_off_get_search_is_what_undo_last_reverts() {
    let env = Env::new();
    env.occur("tea", "0900");
    env.occur("water", "0920");
    env.cut_off(&["get", "tea", "field=tea"]);
    env.ok(&["undo", "_LAST"]);
    assert_eq!(env.live(), ["water=null"]);
}

#[test]
fn a_cut_off_delete_is_what_a_bare_undo_reverts() {
    let env = Env::new();
    let tea = env.occur("tea", "0900");
    env.occur("coffee", "0910");
    env.cut_off(&["delete", &tea]);
    assert_eq!(env.live(), ["coffee=null"]);
    env.ok(&["undo"]);
    assert_eq!(env.live(), ["tea=null", "coffee=null"]);
}

#[test]
fn a_cut_off_update_is_what_a_bare_undo_reverts() {
    let env = Env::new();
    let tea = env.occur("tea", "0900");
    env.occur("coffee", "0910");
    env.cut_off(&["update", &tea, "state=green"]);
    assert_eq!(env.live(), ["tea=\"green\"", "coffee=null"]);
    env.ok(&["undo"]);
    assert_eq!(env.live(), ["tea=null", "coffee=null"]);
}

#[test]
fn a_cut_off_retime_is_what_a_bare_undo_reverts() {
    let env = Env::new();
    let tea = env.occur("tea", "0900");
    env.occur("coffee", "0910");
    env.cut_off(&["retime", &tea, "-t", "0930"]);
    assert_eq!(env.live(), ["coffee=null", "tea=null"]);
    env.ok(&["undo"]);
    assert_eq!(env.live(), ["tea=null", "coffee=null"]);
}

#[test]
fn a_cut_off_kit_install_is_what_a_bare_undo_reverts() {
    let env = Env::new();
    env.occur("tea", "0900");
    let kit = env.db.with_extension("toml");
    std::fs::write(
        &kit,
        "[kit]\nname = \"k\"\n\n[[field]]\nname = \"coffee\"\nverb = \"occur\"\n",
    )
    .unwrap();
    env.cut_off(&["kit", "install", kit.to_str().unwrap()]);
    let coffee = || String::from_utf8(env.ok(&["setup", "coffee", "--json"]).stdout).unwrap();
    assert!(!coffee().contains("No spec defined"), "{}", coffee());
    env.ok(&["undo"]);
    assert_eq!(env.live(), ["tea=null"]);
    assert!(coffee().contains("No spec defined"), "{}", coffee());
}

// `undo <hid>` takes the step a bare `undo` takes (da-ksxkx): a create is
// deleted, spec and all, and the next `undo` brings it back.
#[test]
fn a_named_undo_of_a_create_deletes_it() {
    let env = Env::new();
    let tea = env.occur("tea", "0900");
    env.ok(&["undo", &tea]);
    assert_eq!(env.live(), Vec::<String>::new());
    let spec = || String::from_utf8(env.ok(&["setup", "tea", "--json"]).stdout).unwrap();
    assert!(spec().contains("No spec defined"), "{}", spec());
    env.ok(&["undo", &tea]);
    assert_eq!(env.live(), ["tea=null"]);
    let coffee = env.occur("coffee", "0910");
    let water = env.occur("water", "0920");
    env.ok(&["undo", &format!("{coffee},{water}")]);
    assert_eq!(env.live(), ["tea=null"]);
}

#[test]
fn a_named_undo_steps_an_edit_back_and_a_delete_back() {
    let env = Env::new();
    let tea = env.occur("tea", "0900");
    let coffee = env.occur("coffee", "0910");
    env.ok(&["update", &tea, "state=green"]);
    env.ok(&["delete", &coffee]);
    env.ok(&["undo", &format!("{tea},{coffee}")]);
    assert_eq!(env.live(), ["tea=null", "coffee=null"]);
}

// A list records every step before it echoes any (da-ki5j3).
#[test]
fn a_cut_off_named_undo_list_undoes_every_entry() {
    let env = Env::new();
    let tea = env.occur("tea", "0900");
    let coffee = env.occur("coffee", "0910");
    env.ok(&["update", &format!("{tea},{coffee}"), "state=green"]);
    env.cut_off(&["undo", &format!("{tea},{coffee}")]);
    assert_eq!(env.live(), ["tea=null", "coffee=null"]);
}
