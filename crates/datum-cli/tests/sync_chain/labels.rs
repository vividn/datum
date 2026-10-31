//! What a device is called: its label, `rename`, and its client id
//! (da-rcb5m, da-s4erq).

use super::*;

#[test]
fn a_copy_describes_itself_at_its_first_invite_and_renames_itself_without_an_admin() {
    let (a, b, _) = locked();
    let me = member_of(&a, &a.id());
    assert_eq!(me["client"], client_of(&a).as_str(), "{me}");
    assert_eq!(me["kind"], "cli", "{me}");
    // Alone, its host (da-xijlw); its user is said all the same.
    assert_eq!(
        (&me["label"], &me["user"]),
        (&me["host"], &serde_json::json!("ann")),
        "{me}"
    );
    assert_eq!(me["this"], true);

    // b was admitted at a handshake: it names itself, no key needed.
    assert!(member_of(&b, &b.id())["client"].is_null());
    assert!(b
        .ok(&["sync", "chain", "rename", "phone"])
        .contains("renamed b to phone"));
    assert!(b.send(&a).0);
    let seen = member_of(&a, &b.id());
    assert_eq!(seen["label"], "phone", "{seen}");
    assert_eq!(seen["client"], client_of(&b).as_str(), "{seen}");
    assert!(a.ok(&["sync", "chain", "list"]).contains("phone"));
}

/// `rename --unset` (da-xijlw) hands a copy's label back to the chain to
/// assemble — its host, alone in saying what it runs on — and its
/// describe keeps what it runs on while it carries a label; datum.day joins
/// so. Another member that never said what it runs on is refused it.
/// `--check` refuses what a member may not carry, in no chain and writing
/// nothing — before a join (da-uyyyg).
#[test]
fn rename_unset_goes_back_to_the_assembled_label_and_check_needs_no_chain() {
    let (a, b) = chained();
    let me = || member_of(&a, &a.id());
    let host = me()["host"].as_str().unwrap().to_string();
    assert_eq!(me()["label"], host.as_str());
    a.ok(&["sync", "chain", "rename", "kitchen"]);
    assert_eq!(
        (me()["label"].as_str(), me()["host"].as_str()),
        (Some("kitchen"), Some(host.as_str()))
    );
    a.ok(&["sync", "chain", "rename", "--unset"]);
    assert_eq!(me()["label"], host.as_str());
    assert!(a
        .fails(&["sync", "chain", "rename", "--unset", &b.id(), "x"], "")
        .contains("rename takes [<WHO>] <LABEL>, or [<WHO>] --unset"));
    // b never described itself: nothing to assemble, and no `auto` record written.
    let why = a.fails(&["sync", "chain", "rename", "--unset", &b.id()], "");
    assert!(why.contains("b has not said what it runs on"), "{why}");

    let c = Copy::new();
    c.ok(&["sync", "chain", "rename", "--check", "datum.day (work)"]);
    let why = c.fails(&["sync", "chain", "rename", "--check", "a\u{1b}[2J"], "");
    assert!(why.contains("a control character"), "{why}");
    // No endpoint key minted: a copy that checked a label is in no chain.
    assert_eq!(c.query("SELECT count(*) FROM sync_local"), "0");
}

/// datum.day's client is `DATUM_CLIENT_ID`, `web-` and eight more
/// (da-6kpj5): the copy starts a chain, is shown under it, and renames itself.
#[test]
fn a_client_named_by_the_environment_starts_a_chain_and_renames() {
    let a = Copy::new();
    let run = |args: &[&str]| {
        let out = a
            .cmd(args)
            .env("DATUM_CLIENT_ID", "web-abc12345")
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "`datum {}`: {err}", args.join(" "));
        String::from_utf8(out.stdout).unwrap()
    };
    // Every command under the one client: another would be another endpoint.
    let me = || {
        let all = json(&run(&["sync", "chain", "list", "--json"]));
        all["members"][0].clone()
    };
    run(&["sync", "new-invite"]);
    assert_eq!(me()["client"], "web-abc12345", "{}", me());
    assert_eq!(me()["this"], true, "{}", me());
    run(&["sync", "chain", "rename", "kitchen"]);
    assert_eq!(me()["label"], "kitchen", "{}", me());
    assert_eq!(me()["client"], "web-abc12345", "{}", me());
    let status = json(&run(&["sync", "status", "--json"]));
    assert_eq!(status["this"]["client"], "web-abc12345", "{status}");
}

/// An entry, its copy's endpoint, the chain and `config show` name one
/// client, resolved one way: `DATUM_CLIENT_ID`, then the toml, then the file
/// (da-mmkre — entries read the file alone while sync read the env).
#[test]
fn an_entry_the_chain_and_config_show_name_one_client() {
    for (env, toml) in [
        (Some("web-abc12345"), None),
        (None, Some("toml-dev")),
        (None, None),
    ] {
        let a = Copy::new();
        if let Some(t) = toml {
            std::fs::create_dir_all(a.tmp.path().join("config")).unwrap();
            let body = format!("client_id = \"{t}\"\n");
            std::fs::write(a.tmp.path().join("config/datumrc.toml"), body).unwrap();
        }
        let run = |args: &[&str]| {
            let mut c = a.cmd(args);
            match env {
                Some(d) => c.env("DATUM_CLIENT_ID", d),
                None => c.env_remove("DATUM_CLIENT_ID"),
            };
            let out = c.output().unwrap();
            let err = String::from_utf8_lossy(&out.stderr);
            assert!(out.status.success(), "`datum {}`: {err}", args.join(" "));
            String::from_utf8(out.stdout).unwrap()
        };
        run(&["sync", "new-invite"]);
        run(&["occur", "coffee"]);
        let want = env.or(toml).map(String::from).unwrap_or_else(|| {
            let file = std::fs::read_to_string(a.tmp.path().join("home/client_id"));
            file.unwrap().trim().to_string()
        });
        assert_eq!(a.query("SELECT client_id FROM entries"), want);
        assert_eq!(a.query("SELECT client_id FROM sync_local"), want);
        let status = json(&run(&["sync", "status", "--json"]));
        assert_eq!(status["this"]["client"], want.as_str(), "{status}");
        let described = json(&run(&["sync", "chain", "list", "--json"]));
        assert_eq!(
            described["members"][0]["client"],
            want.as_str(),
            "{described}"
        );
        let show = run(&["config", "show"]);
        let line = show.lines().find(|l| l.starts_with("client_id")).unwrap();
        assert_eq!(
            line.split_whitespace().nth(1),
            Some(want.as_str()),
            "{show}"
        );
    }
}

/// An install from before da-lfsei keeps its id: the data directory's
/// `device_id` file is moved to `client_id` on first read.
#[test]
fn an_install_from_before_the_rename_keeps_its_client_id() {
    let a = Copy::new();
    let home = a.tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("device_id"), "01OLDINSTALL\n").unwrap();
    a.ok(&["occur", "coffee"]);
    assert_eq!(a.query("SELECT client_id FROM entries"), "01OLDINSTALL");
    let file = std::fs::read_to_string(home.join("client_id")).unwrap();
    assert_eq!(file.trim(), "01OLDINSTALL");
    assert!(!home.join("device_id").exists());
}

#[test]
fn another_member_is_renamed_by_an_admin_and_by_no_one_else() {
    let (a, b, admin) = locked();
    b.ok(&["sync", "chain", "rename", "phone"]);
    assert!(b.send(&a).0);
    a.ok(&[
        "sync",
        "chain",
        "rename",
        "phone",
        "tablet",
        "--admin-key",
        &admin,
    ]);
    let renamed = member_of(&a, &b.id());
    assert_eq!(renamed["label"], "tablet");
    assert_eq!(
        renamed["client"],
        client_of(&b).as_str(),
        "the client is kept"
    );
    assert!(a.send(&b).0);
    assert_eq!(member_of(&b, &b.id())["label"], "tablet");

    // b is no admin: its describe of a is refused by the fold, and nothing
    // of it is kept.
    let a_label = member_of(&b, &a.id())["label"]
        .as_str()
        .unwrap()
        .to_string();
    let out = b.fails(
        &["sync", "chain", "rename", &a.id()[..10], "evil"],
        "not-a-signature\n",
    );
    assert!(out.contains("https://datum.day/sync/approve#"), "{out}");
    assert!(
        out.contains("the sync chain refuses it: not signed by an admin"),
        "{out}"
    );
    assert_eq!(member_of(&b, &a.id())["label"], a_label.as_str());
    assert!(b.send(&a).0);
    assert_eq!(member_of(&a, &a.id())["label"], a_label.as_str());
}

#[test]
fn two_databases_in_one_data_directory_are_one_client() {
    let a = Copy::new();
    let other = a.key("other.db");
    a.ok(&["sync", "new-invite"]);
    a.ok(&["--db", &other, "sync", "new-invite"]);
    let one = json(&a.ok(&["sync", "status", "--json"]));
    let two = json(&a.ok(&["--db", &other, "sync", "status", "--json"]));
    assert_ne!(one["this"]["endpoint"], two["this"]["endpoint"]);
    assert_eq!(one["this"]["client"], two["this"]["client"]);
    assert_eq!(
        (one["db"].as_str(), two["db"].as_str()),
        (Some("data"), Some("other"))
    );
    let first = member_of(&a, &a.id());
    assert_eq!(first["client"], one["this"]["client"], "{first}");
}

#[test]
fn a_label_with_a_control_character_is_refused_by_the_fold_and_the_command() {
    let (a, b) = chained();
    assert!(b.send(&a).0);
    let chain_id = a
        .query("SELECT json_extract(body, '$.effect.chain') FROM ops WHERE hid = '~chain' LIMIT 1");
    let describe = |label: &str, kind: &str, client: &str| {
        serde_json::json!({
            "type": "describe", "chain": chain_id, "endpoint": b.id(), "client": client,
            "label": label, "kind": kind, "created": "2026-09-29T00:00:00Z",
        })
    };
    let client = client_of(&b);
    for (record, why) in [
        (
            describe("\u{1b}[2Jevil", "cli", &client),
            "a label cannot contain a control character (U+001B)",
        ),
        (
            describe("adm\u{202E}nimda", "cli", &client),
            "a label cannot contain a bidi control (U+202E)",
        ),
        (
            describe(&"x".repeat(65), "cli", &client),
            "a label is at most 64 characters, not 65",
        ),
        (
            describe("phone", "tv", &client),
            "a describe's kind is cli or web",
        ),
        (
            describe("phone", "cli", "web-\u{1b}[2J"),
            "a describe's client is not a client id",
        ),
        (
            describe("phone", "cli", &"d".repeat(65)),
            "a describe's client is not a client id",
        ),
    ] {
        let out = a.run(&["sync", "pipe", "apply"], &forged(&b, &a, record));
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success() && text.contains(why), "{why}: {text}");
        assert_eq!(member_of(&a, &b.id())["label"], "b");
    }
    // The forgery itself is sound: a well-formed one applies.
    let ok = describe("phone", "web", &client.to_lowercase());
    a.ok_in(&["sync", "pipe", "apply"], &forged(&b, &a, ok));
    assert_eq!(member_of(&a, &b.id())["kind"], "web");

    // A describe written before da-lfsei, `{device: <id>, client: cli|web}`,
    // reads as the same member: chain records are permanent.
    let old = serde_json::json!({
        "type": "describe", "chain": chain_id, "endpoint": b.id(), "device": client,
        "label": "old", "client": "cli", "created": "2026-09-29T00:00:01Z",
    });
    a.ok_in(&["sync", "pipe", "apply"], &forged(&b, &a, old));
    let m = member_of(&a, &b.id());
    assert_eq!(
        (&m["client"], &m["kind"]),
        (&client.as_str().into(), &"cli".into()),
        "{m}"
    );
    // The old fold took a null `device` too: that is still the old shape.
    let null = serde_json::json!({
        "type": "describe", "chain": chain_id, "endpoint": b.id(), "device": null,
        "label": "old", "client": "cli", "created": "2026-09-29T00:00:02Z",
    });
    a.ok_in(&["sync", "pipe", "apply"], &forged(&b, &a, null));
    let m = member_of(&a, &b.id());
    assert_eq!(
        (&m["client"], &m["kind"]),
        (&().into(), &"cli".into()),
        "{m}"
    );

    // The command refuses first, naming the character; nothing is written.
    let out = b.fails(&["sync", "chain", "rename", "\u{1b}[2Jevil"], "");
    assert!(
        out.contains("a label cannot contain a control character (U+001B)"),
        "{out}"
    );
    let out = b.fails(&["sync", "chain", "rename", &"x".repeat(65)], "");
    assert!(out.contains("at most 64 characters"), "{out}");
    assert_eq!(member_of(&b, &b.id())["label"], "b");
}

/// da-s4erq: in a chain a history names each version's device by its label
/// — the one `sync chain list` gives it — and reads the same on every member.
#[test]
fn history_names_each_versions_device_by_its_label_on_every_member() {
    let (a, b) = chained();
    a.ok(&["occur", "tea", "cups=1"]);
    assert!(a.send(&b).0);
    let hid = a.ok(&["tail", "tea", "-n", "1", "-o", "%full_hid%"]);
    let hid = hid.trim();
    b.ok(&["update", hid, "cups=2"]);
    assert!(b.send(&a).0);
    let members = json(&a.ok(&["sync", "chain", "list", "--json"]));
    let genesis = members["members"][0]["label"].as_str().unwrap();
    let shown = a.ok(&["history", hid]);
    assert_eq!(shown, b.ok(&["history", hid]));
    let versions = json(&a.ok(&["history", hid, "--json"]));
    assert_eq!(versions, json(&b.ok(&["history", hid, "--json"])));
    assert_eq!(versions[0]["device"], "b", "{versions}");
    assert_eq!(versions[1]["device"], genesis, "{versions}");
    let line = format!("update    b  'update {hid} cups=2'\n");
    assert!(shown.contains(&line), "{shown}");
    let line = format!("create    {genesis}  'occur tea cups=1'\n");
    assert!(shown.contains(&line), "{shown}");
}
