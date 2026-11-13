//! An agent's code (da-bdyor): `sync chain agent` makes it, and `sync
//! restore <code>` in a fresh home rebuilds a copy at its level, each one
//! retiring the copy before.

use super::*;

/// `a`, alone in the open chain its invite started.
fn alone() -> Copy {
    let a = Copy::new();
    a.ok(&["occur", "coffee", "-t", "-4400"]);
    a.ok(&["occur", "coffee", "-t", "-2880"]);
    a.ok(&["occur", "coffee"]);
    let (inviting, _) = a.invite(&[]);
    drop(inviting);
    a
}

/// The code `sync chain agent <args>` prints on `a`.
fn code(a: &Copy, args: &[&str]) -> String {
    let out = a.ok(&[&["sync", "chain", "agent"], args].concat());
    let code = out.lines().find(|l| l.starts_with("datum-agent:"));
    code.unwrap_or_else(|| panic!("{out}")).to_string()
}

fn count(copy: &Copy, field: &str) -> String {
    copy.query(&format!(
        "SELECT COUNT(*) FROM entries WHERE field = '{field}'"
    ))
}

fn grid(a: &Copy) -> serde_json::Value {
    serde_json::from_str(&a.ok(&["sync", "grid", "--json"])).unwrap()
}

/// A full code restores everything into a fresh home, as a reader; a
/// second fresh home from the same code is the same device, one row of the
/// grid, its first copy retired; and the code in the env does the same.
#[test]
fn a_full_code_restores_every_entry_and_a_second_restore_retires_the_first() {
    let a = alone();
    let code = code(&a, &["--level", "full", "--label", "claude"]);
    let _runs = a.serve(&["sync", "run"]);

    let first = Copy::new();
    let out = first.ok(&["sync", "restore", &code]);
    assert!(out.trim_end().ends_with("lost with this sandbox"), "{out}");
    assert_eq!(count(&first, "coffee"), "3");
    let refused = first.fails(&["occur", "tea"], "");
    assert!(refused.contains("reader"), "{refused}");

    let mut second = Copy::new();
    second
        .env
        .push(("DATUM_AGENT_CODE", Box::leak(code.into_boxed_str())));
    second.ok(&["sync", "restore"]);
    assert_eq!(count(&second, "coffee"), "3");
    let rows = grid(&a);
    let agents: Vec<&serde_json::Value> = rows["devices"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["agent"] == true)
        .collect();
    assert_eq!(agents.len(), 1, "{rows}");
    let cell = agents[0]["cells"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    assert_eq!(
        (&cell["state"], &cell["endpoint"]),
        (&"reader".into(), &second.id().into())
    );
    let list: serde_json::Value =
        serde_json::from_str(&a.ok(&["sync", "chain", "list", "--json"])).unwrap();
    let revoked = |id: &str| {
        let m = list["members"].as_array().unwrap().iter();
        m.into_iter().find(|m| m["endpoint"] == id).unwrap()["revoked"].is_null()
    };
    assert!(!revoked(&first.id()) && revoked(&second.id()), "{list}");
}

/// A light code holds a day of entries and the state at its start, every
/// spec and alias; a write code holds none and writes its own fields alone,
/// which `datum sync` brings to the member.
#[test]
fn a_light_code_holds_a_day_and_a_write_code_writes_its_fields() {
    let a = alone();
    a.ok(&["alias", "set", "cup", "occur coffee"]);
    let light = code(&a, &["--level", "light"]);
    let write = code(
        &a,
        &["--level", "write", "--fields", "tea", "--label", "bot"],
    );
    let _runs = a.serve(&["sync", "run"]);

    let l = Copy::new();
    l.ok(&["sync", "restore", &light]);
    assert_eq!(count(&l, "coffee"), "2", "two days old, the second is out");
    assert_eq!(count(&l, "alias"), "1");
    assert!(l.fails(&["occur", "tea"], "").contains("reader"));

    let w = Copy::new();
    w.ok(&["sync", "restore", &write]);
    assert_eq!(count(&w, "coffee"), "0");
    let refused = w.fails(&["occur", "coffee"], "");
    assert!(refused.contains("tea"), "{refused}");
    w.ok(&["occur", "tea"]);
    w.ok(&["sync"]);
    assert_eq!(count(&a, "tea"), "1");
}

/// A code expelled or past its expiry restores nothing, saying why.
#[test]
fn a_revoked_or_expired_code_restores_nothing() {
    let a = alone();
    let blank = a.fails(
        &["sync", "chain", "agent", "--level", "full", "--label", " "],
        "",
    );
    assert!(blank.contains("cannot be blank"), "{blank}");
    let expiring = code(&a, &["--level", "full", "--expires", "1s"]);
    let revoked = code(&a, &["--level", "full", "--label", "gone"]);
    let _runs = a.serve(&["sync", "run"]);
    let copy = Copy::new();
    copy.ok(&["sync", "restore", &revoked]);
    a.ok(&["sync", "chain", "expel", &copy.id()]);

    std::thread::sleep(Duration::from_millis(1100));
    for (code, why) in [
        (&expiring, "this agent's code expired"),
        (&revoked, "standing admission was revoked"),
    ] {
        let fresh = Copy::new();
        let said = fresh.fails(&["sync", "restore", code], "");
        assert!(said.contains(why), "{said}");
        let chain = fresh.run(&["sync", "chain", "list", "--json"]);
        let chain: serde_json::Value = serde_json::from_slice(&chain.stdout).unwrap();
        assert!(chain["chain"].is_null(), "{chain}");
    }

    // A database in another chain is refused before this install takes the
    // agent's device key, which stays the one it had.
    let other = Copy::new();
    let (inviting, _) = other.invite(&[]);
    drop(inviting);
    let key = || std::fs::read_to_string(other.tmp.path().join("home/device_key")).ok();
    let before = key();
    let said = other.fails(&["sync", "restore", &code(&a, &["--level", "full"])], "");
    assert!(said.contains("in another sync chain"), "{said}");
    assert_eq!(key(), before);
}

/// In an open chain a member's caps would let the agent add its own key as
/// the first admin and lock the person out (ístina's F19): `full --write`
/// is refused there, saying how, and writes no standing record. Locked, an
/// `add_admin` takes an admin's signature, so the chain makes it.
#[test]
fn a_member_agent_is_refused_in_an_open_chain_and_made_in_a_locked_one() {
    let a = alone();
    let standings =
        || a.query("SELECT COUNT(*) FROM ops WHERE body LIKE '%\"type\":\"standing\"%'");
    let full_write = ["sync", "chain", "agent", "--level", "full", "--write"];
    let said = a.fails(&full_write, "");
    assert!(
        said.contains("in a chain with no admin, an agent can be at most a writer")
            && said.contains("`--level write` / `light --write`"),
        "{said}"
    );
    assert_eq!(standings(), "0");
    code(&a, &["--level", "light", "--write"]);
    assert_eq!(standings(), "1", "a writer is made in an open chain");

    let key = a.tmp.path().join("admin.key");
    std::fs::write(&key, "11".repeat(32)).unwrap();
    let key = key.to_str().unwrap();
    a.ok(&["sync", "admin", "add", key]);
    code(&a, &["--level", "full", "--write", "--admin-key", key]);
    assert_eq!(standings(), "2");
}
