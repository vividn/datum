//! `sync grid` (da-n0uj1.3): the devices this install's databases know of,
//! against the chains they are in.

use super::*;

/// Two databases, each in a chain, shared by two devices, and a third
/// device in one: a row a device key however many chains it is in, each
/// cell its standing there, and a copy that never bound itself a row of its
/// endpoint alone; the database it runs on is the first column. The text
/// says the same.
#[test]
fn the_grid_has_a_row_a_device_and_a_column_a_chain() {
    let (a, b, c) = (Copy::new(), Copy::new(), Copy::new());
    admit(&a, &b, "b", None);
    admit(&a, &c, "c", None);
    for (copy, db) in [(&a, "other.db"), (&b, "other.db")] {
        copy.db.set(db);
    }
    admit(&a, &b, "b", None);
    // Each binds itself to its device as it describes itself.
    for (copy, label, dbs) in [
        (&b, "bee", &["data.db", "other.db"][..]),
        (&c, "cat", &["data.db"]),
    ] {
        for db in dbs {
            copy.db.set(db);
            a.db.set(db);
            copy.ok(&["sync", "chain", "rename", label]);
            assert!(copy.send(&a).0);
        }
    }
    // Admitted, and never described: bound to no device.
    let d = Copy::new();
    admit(&a, &d, "dee", None);
    a.db.set("other.db");
    a.ok(&["sync", "chain", "role", "bee", "reader"]);
    let tmp = a.tmp.path().display();
    std::fs::create_dir_all(a.tmp.path().join("config")).unwrap();
    std::fs::write(
        a.tmp.path().join("config/dbs.toml"),
        format!("[db.notes]\npath = \"{tmp}/data.db\"\n\n[db.health]\npath = \"{tmp}/other.db\"\n"),
    )
    .unwrap();
    let device =
        |copy: &Copy| json(&copy.ok(&["sync", "status", "--json"]))["this"]["device"].clone();
    let grid = || json(&a.ok(&["sync", "grid", "--json"]));
    let chain = |db| {
        a.db.set(db);
        json(&a.ok(&["sync", "chain", "list", "--json"]))["chain"].clone()
    };
    let (health, notes) = (chain("other.db"), chain("data.db"));
    let all = grid();
    assert_eq!(all["device"], device(&a), "{all}");
    let names: Vec<_> = all["chains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["db"].clone(), c["chain"].clone()))
        .collect();
    assert_eq!(
        names,
        [
            ("notes".into(), notes.clone()),
            ("health".into(), health.clone())
        ],
        "{all}"
    );
    let rows = all["devices"].as_array().unwrap();
    let cells = |copy: &Copy| {
        let id = device(copy);
        let row = rows
            .iter()
            .find(|r| r["device"] == id)
            .unwrap_or_else(|| panic!("{id} not in {all}"));
        let at = |chain: &serde_json::Value| row["cells"][chain.as_str().unwrap()]["state"].clone();
        (row["this"].clone(), at(&notes), at(&health))
    };
    assert_eq!(rows.len(), 4, "{all}");
    assert_eq!(rows[0]["device"], device(&a), "this device first: {all}");
    assert_eq!(cells(&a), (true.into(), "member".into(), "member".into()));
    assert_eq!(cells(&b), (false.into(), "member".into(), "reader".into()));
    assert_eq!(
        cells(&c),
        (false.into(), "member".into(), serde_json::Value::Null)
    );
    assert_eq!(
        rows[2]["cells"][notes.as_str().unwrap()]["endpoint"],
        c.id()
    );
    let unbound = &rows[3];
    assert_eq!(
        (&unbound["device"], &unbound["endpoint"], &unbound["label"]),
        (&serde_json::Value::Null, &d.id().into(), &"dee".into()),
        "{all}"
    );
    assert_eq!(unbound["cells"][notes.as_str().unwrap()]["state"], "member");

    // The text: a header of names and one of chain ids, then a line a device.
    let text = a.ok(&["sync", "grid"]);
    let lines: Vec<Vec<&str>> = text
        .lines()
        .map(|l| l.split_whitespace().collect())
        .collect();
    let short = |v: &serde_json::Value| v.as_str().unwrap()[..10].to_string();
    assert_eq!(lines[0], ["notes", "health"], "{text}");
    assert_eq!(lines[1], [&notes, &health], "{text}");
    let tail = |n: usize, line: &[&str]| line[line.len() - n..].join(" ");
    assert_eq!(
        tail(5, &lines[2]),
        format!("{} member member (this device)", short(&device(&a)))
    );
    assert_eq!(
        tail(3, &lines[3]),
        format!("{} member reader", short(&device(&b)))
    );
    assert_eq!(lines[3][0], "bee", "{text}");
    assert_eq!(
        tail(3, &lines[4]),
        format!("{} member —", short(&device(&c)))
    );
    assert_eq!(
        lines[5],
        ["dee", "endpoint", &d.id()[..10], "member", "—"],
        "{text}"
    );

    // Run on the other database, its column first; then an expelled device
    // and an unjoined one.
    a.db.set("other.db");
    let first = &grid()["chains"][0]["chain"];
    assert_eq!(first, &health);
    a.db.set("data.db");
    a.ok(&["sync", "chain", "expel", "cat"]);
    assert_eq!(
        grid()["devices"][2]["cells"][notes.as_str().unwrap()]["state"],
        "expelled"
    );
    a.ok(&["sync", "chain", "expel", "cat", "--unjoin", "--yes"]);
    assert_eq!(
        grid()["devices"][2]["cells"][notes.as_str().unwrap()]["state"],
        "unjoined"
    );
}

/// In no chain: `--json` is an empty grid, and the text says why.
#[test]
fn an_install_in_no_chain_has_an_empty_grid() {
    let a = Copy::new();
    a.ok(&["occur", "coffee"]);
    let all = json(&a.ok(&["sync", "grid", "--json"]));
    assert_eq!(
        (&all["chains"], &all["devices"]),
        (&json("[]"), &json("[]")),
        "{all}"
    );
    let said = a.fails(&["sync", "grid"], "");
    assert!(
        said.contains("no database of this install is in a sync chain"),
        "{said}"
    );
}
