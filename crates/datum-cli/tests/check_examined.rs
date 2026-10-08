//! `datum check`'s all-clear names what it examined (da-qd9ao).
//!
//! On the main this bead branched from, an empty database and a populated
//! one printed the same «✓ No consistency issues found.», so «nothing
//! wrong» and «nothing looked at» read identically. The line now carries
//! its denominator — entries, field specs, aliases and the checkers that
//! ran — counted from the walk the checkers made.

use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn datum(db: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.parent().unwrap())
        .env("DATUM_CONFIG_HOME", db.parent().unwrap())
        .env("DATUM_COLOR", "0")
        .args(args)
        .output()
        .expect("spawn datum");
    assert!(
        out.status.success(),
        "datum {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn all_clear_over_an_empty_db_differs_from_one_over_entries() {
    let dir = TempDir::new().unwrap();
    let empty = dir.path().join("empty.db");
    let seeded = dir.path().join("seeded.db");
    datum(&seeded, &["occur", "coffee"]);
    datum(&seeded, &["occur", "coffee", "-q"]);
    datum(&seeded, &["alias", "set", "cf", "occur coffee"]);

    let all = "checkers: base, verbs, aliases, slugs, integrity, flags, tz, refs, stats";
    let on_empty = datum(&empty, &["check"]);
    let on_seeded = datum(&seeded, &["check"]);
    assert_ne!(
        on_empty, on_seeded,
        "a green over nothing must not look like one over data"
    );
    assert_eq!(
        on_empty,
        format!("✓ No consistency issues found: 0 entries, 0 field specs, 0 aliases examined, 0 cached days recomputed, 0 differ ({all}).\n")
    );
    // Two coffees, the spec the first one wrote and the alias: specs and
    // aliases are entries too, and the checkers walk them.
    assert_eq!(
        on_seeded,
        format!(
            "✓ No consistency issues found: 4 entries, 1 field spec, 1 alias examined, 0 cached days recomputed, 0 differ ({all}).\n"
        )
    );

    // A filter names only the checkers that ran, and what they looked at.
    assert_eq!(
        datum(&seeded, &["check", "aliases"]),
        "✓ No consistency issues found: 0 entries, 0 field specs, 1 alias examined (checkers: aliases).\n"
    );

    let json: Value = serde_json::from_str(&datum(&seeded, &["check", "--json"])).unwrap();
    assert_eq!(json["examined"]["entries"], 4);
    assert_eq!(json["examined"]["specs"], 1);
    assert_eq!(json["examined"]["aliases"], 1);
    assert_eq!(json["examined"]["checkers"].as_array().unwrap().len(), 9);
}

/// da-1i6mi: `check stats` says how many cached days it recomputed, so a
/// clean run over an empty cache does not read like one over a full one.
#[test]
fn check_stats_names_the_cached_days_it_recomputed() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("stats.db");
    datum(&db, &["switch", "mood", "calm", "-yyy"]);
    let only = "(checkers: stats).\n";
    let clean = |days: &str| {
        format!("✓ No consistency issues found: 0 entries, 0 field specs, 0 aliases examined, {days} recomputed, 0 differ {only}")
    };
    assert_eq!(datum(&db, &["check", "stats"]), clean("0 cached days"));
    datum(&db, &["stats"]);
    // Three days ago, the two after it: the closed days the span covers.
    assert_eq!(datum(&db, &["check", "stats"]), clean("3 cached days"));

    let json: Value = serde_json::from_str(&datum(&db, &["check", "stats", "--json"])).unwrap();
    assert_eq!(json["examined"]["stats_days"], 3);
    let json: Value = serde_json::from_str(&datum(&db, &["check", "aliases", "--json"])).unwrap();
    assert!(json["examined"].get("stats_days").is_none(), "{json}");
}
