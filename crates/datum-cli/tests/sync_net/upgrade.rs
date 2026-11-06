//! A chain upgrades itself (da-w8zp5): the copy below a member's sync floor
//! upgrades, every member follows a member's ask, between floors nothing
//! moves, and nothing goes past the chain's pin. A datum `datum install` did
//! not set up, a checkout's build, warns instead.
#![cfg(target_os = "linux")]

use super::schema::{up, OVER};
use super::*;
use crate::install_cmd::{minisign_key, sign_release, write_release, TEST_KEY};

/// The release an installed copy upgrades to: not a datum, so it says it is.
const RELEASE: &[u8] = b"#!/bin/sh\necho 'datum 9.9.9-beta (abcdef1)'\n";

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

impl Copy {
    /// This copy runs a datum `datum install` set up, a copy of its own, and
    /// upgrades from a release beside it holding `release`, signed by the key
    /// the test build trusts. Its systemd units are its own too.
    fn installed(&mut self, release: &[u8]) {
        let dir = self.tmp.path();
        let exe = dir.join("bin/datum");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::copy(env!("CARGO_BIN_EXE_datum"), &exe).unwrap();
        std::fs::create_dir_all(dir.join("home")).unwrap();
        let record = format!("exe = {:?}\n", exe.display().to_string());
        std::fs::write(dir.join("home/installed.toml"), record).unwrap();
        write_release(&dir.join("release"), release, None);
        sign_release(&dir.join("release"), &TEST_KEY);
        let url = format!("file://{}", dir.join("release").display());
        self.env.extend([
            ("DATUM_RELEASE_URL", leak(url)),
            ("DATUM_TEST_RELEASE_KEY", leak(minisign_key(&TEST_KEY))),
            ("HOME", leak(dir.join("h").display().to_string())),
            (
                "XDG_CONFIG_HOME",
                leak(dir.join("xdg").display().to_string()),
            ),
        ]);
        self.exe = Some(exe);
    }

    fn exe_holds(&self, bytes: &[u8]) -> bool {
        std::fs::read(self.exe.as_ref().unwrap()).unwrap() == bytes
    }
}

fn datum() -> Vec<u8> {
    std::fs::read(env!("CARGO_BIN_EXE_datum")).unwrap()
}

/// Across a sync floor the copy below upgrades, to the release it fetches
/// and verifies itself; a checkout's build says the chain has moved on, once.
#[test]
fn across_a_sync_floor_the_copy_below_upgrades_and_a_checkout_warns() {
    let (mut a, mut b) = paired();
    a.env.extend(OVER);
    b.installed(RELEASE);
    let (a_runs, _) = up(&a);
    let out = b.run(&["sync"]);
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(said.contains("to datum 9.9.9-beta (abcdef1)"), "{said}");
    assert!(b.exe_holds(RELEASE));
    drop(a_runs);

    let (mut a, c) = paired();
    a.env.extend(OVER);
    let (_a_runs, _) = up(&a);
    let warned = |c: &Copy| {
        let err = String::from_utf8(c.run(&["sync"]).stderr).unwrap();
        err.contains("the sync chain has moved on, but this datum")
    };
    assert!(warned(&c));
    assert!(!warned(&c), "once");
}

/// Between floors nothing moves: a member's newer schema is a warning alone.
#[test]
fn between_sync_floors_nothing_moves() {
    let (mut a, mut b) = paired();
    a.env.push(("DATUM_SYNC_TEST_SCHEMA", "999"));
    b.installed(RELEASE);
    let (_a_runs, _) = up(&a);
    let said = b.ok(&["sync"]);
    assert!(
        said.contains("verify ok") && !said.contains("upgraded"),
        "{said}"
    );
    assert!(b.exe_holds(&datum()));
}

/// The chain's pin holds both an upgrade past it and a floor above it: the
/// copy below says it is held, `datum upgrade` does nothing and refuses a
/// version past it, a copy past the pin stays, and once unpinned the copy
/// upgrades.
#[test]
fn a_pin_holds_an_upgrade_past_it_and_a_floor_above_it() {
    let (mut a, mut b) = paired();
    a.env.extend(OVER);
    b.installed(RELEASE);
    b.env
        .push(("DATUM_SYNC_TEST_BUILD", "3.2.5-beta (abcdef1)"));
    b.ok(&["sync", "pin", "3.2.5"]);
    assert!(b.ok(&["sync", "status"]).contains("pinned at datum 3.2.5"));
    let (_a_runs, _) = up(&a);
    let err = String::from_utf8(b.run(&["sync"]).stderr).unwrap();
    assert!(
        err.contains("held at the sync chain's pin (datum 3.2.5)"),
        "{err}"
    );
    assert!(b.ok(&["upgrade"]).contains("datum is held at 3.2.5"));
    let past = b.fails(&["upgrade", "--to", "9.9"], "");
    assert!(past.contains("pinned at datum 3.2.5"), "{past}");
    assert!(b.exe_holds(&datum()));

    // A copy past the pin is no copy held: it stays where it is.
    b.ok(&["sync", "pin", "3.1"]);
    let said = b.ok(&["upgrade"]);
    assert!(said.contains("is past the pin"), "{said}");
    let err = String::from_utf8(b.run(&["sync"]).stderr).unwrap();
    assert!(
        err.contains("past the sync chain's pin (datum 3.1): it stays where it is"),
        "{err}"
    );
    assert!(b.exe_holds(&datum()));

    b.ok(&["sync", "unpin"]);
    let said = String::from_utf8(b.run(&["sync"]).stdout).unwrap();
    assert!(said.contains("to datum 9.9.9-beta (abcdef1)"), "{said}");
    assert!(b.exe_holds(RELEASE));
}

/// `sync chain upgrade`: this copy upgrades, and asks every member, floor or
/// no floor; a member's `sync run` follows, and `sync status` says who has.
#[test]
fn chain_upgrade_asks_every_member_and_status_says_who_followed() {
    let (mut a, mut b) = paired();
    a.installed(&datum());
    b.installed(RELEASE);
    b.env
        .push(("DATUM_SYNC_TEST_BUILD", "3.1.0-beta (0000000)"));
    let (_b_runs, _) = up(&b);
    let said = a.ok(&["sync", "chain", "upgrade"]);
    assert!(said.contains("asked every member of sync chain"), "{said}");
    eventually("b upgrades", || b.exe_holds(RELEASE));

    let status: serde_json::Value =
        serde_json::from_str(&a.ok(&["sync", "status", "--json"])).unwrap();
    let asked = &status["asked"];
    let mine = status["this"]["label"].clone();
    assert_eq!(asked["datum"], status["this"]["datum"], "{status}");
    assert_eq!(asked["followed"], serde_json::json!([mine]), "{status}");
    assert_eq!(
        asked["behind"][0]["datum"], "3.1.0-beta (0000000)",
        "{status}"
    );
    assert!(a
        .ok(&["sync", "status"])
        .contains("asked every member to upgrade to datum"));
}
