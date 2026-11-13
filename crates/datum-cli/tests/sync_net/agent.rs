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

/// A restore names the database it restores into, beside the inviter's
/// (da-kui6j): told only «database data», an agent wrote `--db data` into a
/// new database in no chain.
#[test]
fn a_restore_names_the_database_it_restores_into() {
    let a = alone();
    let code = code(&a, &["--level", "full", "--label", "claude"]);
    let _runs = a.serve(&["sync", "run"]);
    let mine = Copy::of("bob", "mine.db");
    let out = mine.run(&["sync", "restore", &code]);
    let told = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success()
            && told.contains("(its database «data») into this install's database «mine»"),
        "{told}"
    );
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

/// A code is its device's secret, so with no `--expires` it lapses 30 days
/// on, said as a date beside it; `--expires never` makes one that does not,
/// and says so (da-4cw3v).
#[test]
fn a_code_expires_in_30_days_unless_never_is_asked() {
    let a = alone();
    let made = |args: &[&str]| {
        let argv = [&["sync", "chain", "agent", "--level", "full"], args].concat();
        let out: serde_json::Value =
            serde_json::from_str(&a.ok(&[&argv[..], &["--json"]].concat())).unwrap();
        (a.ok(&argv), out["expires"].clone())
    };
    let (said, expires) = made(&[]);
    let expires = expires.as_str();
    assert!(
        expires.is_some(),
        "a code made with no --expires expires: {said}"
    );
    let expires = chrono::DateTime::parse_from_rfc3339(expires.unwrap()).unwrap();
    let days = (expires.to_utc() - chrono::Utc::now()).num_hours() as f64 / 24.0;
    assert!((29.9..=30.0).contains(&days), "{days}");
    let on = expires
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d")
        .to_string();
    assert!(
        said.contains("Anyone with this code is this agent until it expires or you remove it")
            && said.contains(&format!("It expires {on}")),
        "{said}"
    );
    let (said, expires) = made(&["--expires", "never"]);
    assert!(expires.is_null(), "{expires}");
    assert!(said.contains("it never expires"), "{said}");
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

/// The member's notices of a device joining, in `sync status` and `datum check`.
fn joined(a: &Copy) -> [usize; 2] {
    [a.ok(&["sync", "status"]), a.ok(&["check"])]
        .map(|out| out.matches("⚠ another device joined: ").count())
}

/// The member's notices of an agent's code in use in two places (da-wtdw8).
fn two_places(a: &Copy, label: &str) -> [usize; 2] {
    let said = format!("⚠ agent {label}'s code is in use in two places: ");
    [a.ok(&["sync", "status"]), a.ok(&["check"])].map(|out| out.matches(&said).count())
}

/// Two restores from one code are one device, told once on the member;
/// another code's restore is another device, told too (da-w27bp). Neither
/// is a code in use in two places (da-wtdw8).
#[test]
fn a_code_restored_again_tells_the_member_of_no_new_device() {
    let a = alone();
    let claude = code(&a, &["--level", "full", "--label", "claude"]);
    let bot = code(&a, &["--level", "full", "--label", "bot"]);
    let _runs = a.serve(&["sync", "run"]);

    Copy::new().ok(&["sync", "restore", &claude]);
    assert_eq!(joined(&a), [1, 1]);
    Copy::new().ok(&["sync", "restore", &claude]);
    assert_eq!(joined(&a), [1, 1], "the same device, again");
    Copy::new().ok(&["sync", "restore", &bot]);
    assert_eq!(joined(&a), [2, 2], "another device");
    assert_eq!(two_places(&a, "claude"), [0, 0]);
}

/// One home a session, restoring at each one's start: a session a minute
/// after the last, which logged and synced, tells the member nothing
/// (vremya, on #2069: «a leak alarm that fires when nothing leaked is how
/// a real one gets ignored»).
#[test]
fn back_to_back_sessions_restoring_one_code_tell_of_no_two_places() {
    let a = alone();
    let bot = code(
        &a,
        &["--level", "write", "--fields", "tea", "--label", "bot"],
    );
    let _runs = a.serve(&["sync", "run"]);
    for _ in 0..2 {
        let session = Copy::new();
        session.ok(&["sync", "restore", &bot]);
        session.ok(&["occur", "tea"]);
        session.ok(&["sync"]);
    }
    assert_eq!(count(&a, "tea"), "2");
    assert_eq!(two_places(&a, "bot"), [0, 0]);
}

/// Two homes restoring from one code, the first logging after the
/// second's restore: the first's sync tells the member, once, that the
/// agent's code is in use in two places, naming the agent and how to
/// remove it (da-wtdw8).
#[test]
fn a_replaced_copy_logging_after_tells_the_member_its_code_is_in_two_places() {
    let a = alone();
    let bot = code(
        &a,
        &["--level", "write", "--fields", "tea", "--label", "bot"],
    );
    let _runs = a.serve(&["sync", "run"]);
    let first = Copy::new();
    first.ok(&["sync", "restore", &bot]);
    Copy::new().ok(&["sync", "restore", &bot]);
    assert_eq!(two_places(&a, "bot"), [0, 0], "a restore alone");

    first.ok(&["occur", "tea"]);
    first.run(&["sync"]);
    first.run(&["sync"]);
    assert_eq!(two_places(&a, "bot"), [1, 1]);
    let status = a.ok(&["sync", "status"]);
    assert!(
        status.contains("not yours? remove the agent, `datum sync chain expel bot`"),
        "{status}"
    );
    assert_eq!(count(&a, "tea"), "0");
}

/// An agent is never an admin (da-ebxg5; vividn, 2026-11-13): a `full
/// --write` agent made in a locked chain stays a member once the chain is
/// unlocked, and its copy refuses the `add_admin` it would write there, so
/// neither copy is locked (`standing.rs` tests the fold refusing one it
/// receives); nor
/// is a `full` agent promoted to a member of the open chain.
#[test]
fn an_agent_locks_no_unlocked_chain_and_is_promoted_to_no_member() {
    let a = alone();
    let key = a.tmp.path().join("admin.key");
    std::fs::write(&key, "11".repeat(32)).unwrap();
    let key = key.to_str().unwrap();
    a.ok(&["sync", "admin", "add", key]);
    let claude = code(
        &a,
        &[
            "--level",
            "full",
            "--write",
            "--label",
            "claude",
            "--admin-key",
            key,
        ],
    );
    let bot = code(
        &a,
        &["--level", "full", "--label", "bot", "--admin-key", key],
    );
    let _runs = a.serve(&["sync", "run"]);
    let agent = Copy::new();
    agent.ok(&["sync", "restore", &claude]);
    Copy::new().ok(&["sync", "restore", &bot]);
    a.ok(&[
        "sync",
        "admin",
        "remove",
        "admin.key",
        "--allow-unlock",
        "--admin-key",
        key,
    ]);

    agent.ok(&["sync"]);
    let mine = agent.tmp.path().join("mine.key");
    std::fs::write(&mine, "22".repeat(32)).unwrap();
    let said = agent.fails(&["sync", "admin", "add", mine.to_str().unwrap()], "");
    assert!(said.contains("an agent is never an admin"), "{said}");
    agent.run(&["sync"]);
    let admins = |c: &Copy| c.ok(&["sync", "admin", "list", "--json"]);
    for copy in [&a, &agent] {
        let admins: serde_json::Value = serde_json::from_str(&admins(copy)).unwrap();
        assert_eq!(admins["admins"], serde_json::json!([]), "{admins}");
    }

    let said = a.fails(&["sync", "chain", "role", "bot", "member"], "");
    assert!(
        said.contains("in a chain with no admin, an agent can be at most a writer"),
        "{said}"
    );
    let list = a.ok(&["sync", "chain", "list", "--json"]);
    let list: serde_json::Value = serde_json::from_str(&list).unwrap();
    let bot = list["members"].as_array().unwrap().iter();
    let bot: Vec<_> = bot
        .filter(|m| m["label"] == "bot")
        .map(|m| &m["role"])
        .collect();
    assert_eq!(bot, [&serde_json::json!("reader")], "{list}");
}
