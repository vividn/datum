//! `datum kit install` carries what a kit says, or refuses it by name
//! (da-uokdb, from da-g1bbi's catalog).

use crate::common;

use common::run_cli;
use datum_core::db::Db;
use datum_core::field_spec::Render;
use datum_core::kit_color::resolve_color;
use datum_core::test_util::fresh_db;

/// Write `toml` as a kit file and install it with `extra` flags.
fn install(db: &Db, toml: &str, extra: &[&str]) -> anyhow::Result<()> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("k.toml");
    std::fs::write(&path, toml).unwrap();
    let path = path.to_string_lossy().to_string();
    run_cli(&[&["kit", "install", &path][..], extra].concat(), db)
}

/// `hidden = true` installs a hidden spec — the `render = false` that
/// `spec set` stores — and export writes it back.
#[test]
fn hidden_installs_a_hidden_spec() {
    let db = fresh_db();
    let kit =
        "[kit]\nname = \"k\"\n\n[[field]]\nname = \"mood\"\nverb = \"switch\"\nhidden = true\n";
    install(&db, kit, &[]).unwrap();
    let p = db.load_spec("mood").unwrap().unwrap().presentation;
    assert_eq!(p.render, Some(Render::All(false)));
    assert!(!p.renders_on("dayview"), "hidden = true hides the field");

    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.toml");
    run_cli(&["kit", "export", "k", &out.to_string_lossy()], &db).unwrap();
    let exported = std::fs::read_to_string(&out).unwrap();
    assert!(exported.contains("render = false"), "{exported}");
}

/// A state's colour word resolves as the field's does — to a hue, seeded by
/// its own name — and one that does not resolve refuses the kit, by name.
#[test]
fn state_colours_resolve_like_the_fields() {
    let db = fresh_db();
    let kit = "[kit]\nname = \"k\"\n\n[[field]]\nname = \"mood\"\nverb = \"switch\"\ncolor = \"yellow\"\nstates.a = { color = \"yellow\" }\nstates.b = { color = \"#ABCDEF\" }\n";
    install(&db, kit, &[]).unwrap();
    let spec = db.load_spec("mood").unwrap().unwrap();
    let hue = |name| resolve_color(Some("yellow"), name).unwrap();
    assert_eq!(spec.color, hue("mood"));
    assert_eq!(spec.state_colors.get("a").cloned(), hue("a"));
    assert_eq!(spec.state_colors["b"], "#abcdef");

    let db = fresh_db();
    let bad = kit.replace("#ABCDEF", "aubergine");
    let err = format!("{:#}", install(&db, &bad, &[]).unwrap_err());
    assert!(
        err.contains("'mood' state 'b'") && err.contains("aubergine"),
        "{err}"
    );
    assert!(db.load_spec("mood").unwrap().is_none(), "nothing installed");
}

/// `--force` over a hand-set spec remembers it, and uninstalling the kit
/// writes it back rather than leaving the field with none.
#[test]
fn uninstall_after_force_gives_back_the_hand_set_spec() {
    let db = fresh_db();
    run_cli(
        &[
            "spec", "set", "mood", "--verb", "switch", "--color", "#123456",
        ],
        &db,
    )
    .unwrap();
    let hand_set = serde_json::to_value(db.load_spec("mood").unwrap().unwrap()).unwrap();
    let kit = "[kit]\nname = \"k\"\n\n[[field]]\nname = \"mood\"\nverb = \"switch\"\ncolor = \"#abcdef\"\n";
    install(&db, kit, &["--force"]).unwrap();
    let forced = db.load_spec("mood").unwrap().unwrap();
    assert_eq!(forced.color.as_deref(), Some("#abcdef"));

    run_cli(&["kit", "uninstall", "k"], &db).unwrap();
    let back = db
        .load_spec("mood")
        .unwrap()
        .expect("the hand-set spec is back");
    assert_eq!(serde_json::to_value(back).unwrap(), hand_set);
    assert!(db.list_kits().unwrap().is_empty());
}

/// A key the format does not read refuses the kit — at install and at
/// `--dry-run` — naming it and its table; the kits in `kits/` still install.
#[test]
fn an_unknown_key_refuses_the_kit_by_name() {
    let kit =
        "[kit]\nname = \"k\"\n\n[[field]]\nname = \"water\"\nverb = \"occur\"\nunti = \"ml\"\n";
    for extra in [&[][..], &["--dry-run"]] {
        let db = fresh_db();
        let err = format!("{:#}", install(&db, kit, extra).unwrap_err());
        assert!(err.contains("[[field]] water: unknown key «unti»"), "{err}");
        assert!(
            db.load_spec("water").unwrap().is_none(),
            "nothing installed"
        );
    }
    let db = fresh_db();
    let meta = "[kit]\nname = \"k\"\nvresion = \"1\"\n";
    let err = format!("{:#}", install(&db, meta, &[]).unwrap_err());
    assert!(err.contains("[kit]: unknown key «vresion»"), "{err}");
    let state = kit.replace("unti = \"ml\"", "states.a = { colr = \"red\" }");
    let err = format!("{:#}", install(&db, &state, &[]).unwrap_err());
    assert!(
        err.contains("[[field]] water: unknown key «states.a.colr»"),
        "{err}"
    );
    // Keys the format once parsed and dropped: nothing draws them, so they
    // refuse like any other (da-uokdb).
    for (key, named) in [
        ("kind = \"block\"", "«kind»"),
        ("state_pos = 0.1", "«state_pos»"),
        ("state_size = 0.5", "«state_size»"),
        ("states.a = { pos = 0.2 }", "«states.a.pos»"),
        ("states.a = { size = 0.3 }", "«states.a.size»"),
    ] {
        let err = format!(
            "{:#}",
            install(&db, &kit.replace("unti = \"ml\"", key), &[]).unwrap_err()
        );
        assert!(
            err.contains(&format!("[[field]] water: unknown key {named}")),
            "{err}"
        );
    }

    for shipped in ["sample", "chores"] {
        let path = format!("{}/../../kits/{shipped}.toml", env!("CARGO_MANIFEST_DIR"));
        run_cli(&["kit", "install", &path, "--yes"], &fresh_db()).unwrap();
    }
}

/// Kit `a` forced over kit `b`, which was forced over a hand-set spec: the
/// hand-set spec comes back whichever kit is uninstalled first, and neither
/// kit outlives its own uninstall.
#[test]
fn a_kit_forced_over_a_kit_unwinds_in_either_order() {
    let kit = |name: &str, color: &str| {
        format!("[kit]\nname = \"{name}\"\n\n[[field]]\nname = \"mood\"\nverb = \"switch\"\ncolor = \"{color}\"\n")
    };
    for order in [["a", "b"], ["b", "a"]] {
        let db = fresh_db();
        run_cli(
            &[
                "spec", "set", "mood", "--verb", "switch", "--color", "#123456",
            ],
            &db,
        )
        .unwrap();
        let hand_set = serde_json::to_value(db.load_spec("mood").unwrap().unwrap()).unwrap();
        install(&db, &kit("b", "#bbbbbb"), &["--force"]).unwrap();
        install(&db, &kit("a", "#aaaaaa"), &["--force"]).unwrap();

        run_cli(&["kit", "uninstall", order[0]], &db).unwrap();
        let color = db.load_spec("mood").unwrap().unwrap().color;
        let left = if order[0] == "a" {
            "#bbbbbb"
        } else {
            "#aaaaaa"
        };
        assert_eq!(color.as_deref(), Some(left), "{order:?}");
        run_cli(&["kit", "uninstall", order[1]], &db).unwrap();
        let back = db.load_spec("mood").unwrap().unwrap();
        assert_eq!(serde_json::to_value(back).unwrap(), hand_set, "{order:?}");
        assert!(db.list_kits().unwrap().is_empty(), "{order:?}");
    }
}
