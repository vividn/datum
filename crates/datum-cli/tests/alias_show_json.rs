//! A call-site `--show json` after an alias's words is a flag with its
//! value, not two more words (da-tckse). datum.day appends it to every
//! line it runs, so the alias walk taking `json` for a positional put the
//! first keyed slot after `--show`: `feel 3 --show json` was refused as
//! `unknown --show value: feel=3`.

use std::path::Path;
use std::process::Command;

fn datum(db: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_COLOR", "0")
        .args(args)
        .output()
        .expect("run datum");
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn key_slot_aliases_take_a_trailing_show_json() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("x.db");
    datum(
        &db,
        &["alias", "set", "feel", "occur mood -k feel -k energy= -C"],
    );
    datum(
        &db,
        &[
            "alias",
            "set",
            "spent",
            "occur spent -k amount -k category -C",
        ],
    );

    let cases: [(&[&str], &str); 3] = [
        (
            &["feel", "3", "--show", "json"],
            r#""data_inline":"feel: 3""#,
        ),
        (
            &["spent", "12.50", "food", "lunch", "--show", "json"],
            r#""data_inline":"amount: 12.5, category: \"food\", comment: \"lunch\"""#,
        ),
        (
            &[
                "spent",
                "4",
                "tea",
                "--db",
                db.to_str().unwrap(),
                "--show",
                "json",
            ],
            r#""data_inline":"amount: 4, category: \"tea\"""#,
        ),
    ];
    for (argv, want) in cases {
        let out = datum(&db, argv);
        assert!(out.contains(want), "{argv:?} printed {out}");
    }
}
