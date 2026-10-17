//! `sync chain list` and `sync admin list` print each instant on the
//! reader's clock, as `sync status` does (da-awd9c); `--json` keeps the
//! stored instants.

use super::*;

/// `at` as a reader at +02 reads it: the time alone today.
fn at_plus_two(at: &str) -> String {
    let zone = chrono::FixedOffset::east_opt(2 * 3600).unwrap();
    let t = chrono::DateTime::parse_from_rfc3339(at)
        .unwrap()
        .with_timezone(&zone);
    let today = chrono::Utc::now().with_timezone(&zone).date_naive();
    let format = if t.date_naive() == today {
        "%H:%M"
    } else {
        "%Y-%m-%d %H:%M"
    };
    t.format(format).to_string()
}

#[test]
fn chain_list_prints_its_instants_on_the_readers_clock() {
    let (a, b) = chained();
    let admin = a.key("admin.key");
    a.ok(&["sync", "admin", "add", &admin]);
    a.ok(&["sync", "chain", "revoke", "b", "--admin-key", &admin]);
    let at_two = |args: &[&str]| {
        let out = a.cmd(args).env("TZ", "Etc/GMT-2").output().unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    let list = at_two(&["sync", "chain", "list", "--show-revoked"]);
    let all = json(&a.ok(&["sync", "chain", "list", "--json"]));
    let raw = |v: &serde_json::Value| v.as_str().unwrap().to_string();
    let added = raw(&all["admins"][0]["added"]);
    let b_row = member_of(&a, &b.id());
    let shown = [
        ("admitted", raw(&member_of(&a, &a.id())["admitted"])),
        ("admitted", raw(&b_row["admitted"])),
        ("REVOKED", raw(&b_row["revoked"])),
        ("added", added.clone()),
    ];
    for (word, instant) in shown {
        assert!(instant.ends_with('Z'), "--json keeps {instant}");
        assert!(!list.contains(&instant), "{list}");
        let local = format!("{word} {}", at_plus_two(&instant));
        assert!(list.contains(&local), "{local} in {list}");
    }
    let admins = at_two(&["sync", "admin", "list"]);
    assert!(
        admins.contains(&format!("added {}", at_plus_two(&added))),
        "{admins}"
    );
}
