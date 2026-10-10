//! A keep-both (sync, or `restore --merge`) leaves two rows on one slug, the
//! loser marked `superseded_by` the winner. `undo` and `redo` of the winner
//! move the winner and nothing else: they address the row by hid, the
//! identity, never by the slug the pair shares (da-4ptyy). So do `del` and a
//! log's `-u` (da-1uqij).

use std::path::Path;
use std::process::Command;

fn datum(db: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_datum"))
        .env("DATUM_DB", db)
        .env("DATUM_HOME", db.with_extension("home"))
        .env("DATUM_CONFIG_HOME", db.with_extension("config"))
        .args(args)
        .output()
        .expect("spawn datum");
    assert!(
        out.status.success(),
        "`datum {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8(out.stdout).unwrap()
}

/// `(hid, slug, note, deleted, superseded_by)` for every habit row, oldest first.
type Row = (String, String, String, bool, Option<String>);

fn habits(db: &Path) -> Vec<Row> {
    let conn = rusqlite::Connection::open(db).unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT hid, slug, json_extract(data, '$.note'), deleted_at IS NOT NULL, superseded_by
             FROM entries WHERE field = 'habit' ORDER BY rowid",
        )
        .unwrap();
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    rows
}

/// The keep-both shape, as `[w, x, y]`: y wins, x is set aside under the same
/// slug. x is the older row, so a lookup by slug finds it first.
fn pair(db: &Path) -> [Row; 3] {
    // The first log declares the field, so x and y differ in nothing y
    // does not win: y already shows the two merged (da-04glw).
    datum(db, &["occur", "habit", "-t", "2026-10-03 09:00", "note=w"]);
    datum(db, &["occur", "habit", "-t", "2026-10-04 09:00", "note=x"]);
    datum(db, &["occur", "habit", "-t", "2026-10-04 10:00", "note=y"]);
    let [w, x, y] = <[Row; 3]>::try_from(habits(db)).unwrap();
    rusqlite::Connection::open(db)
        .unwrap()
        .execute(
            "UPDATE entries SET slug = ?2, superseded_by = ?3 WHERE hid = ?1",
            rusqlite::params![x.0, y.1, y.0],
        )
        .unwrap();
    let x = (x.0, y.1.clone(), x.2, false, Some(y.0.clone()));
    [w, x, y]
}

#[test]
fn undo_and_redo_of_the_winner_leave_the_loser_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("data.db");
    let [w, x, y] = pair(&db);

    datum(&db, &["undo"]);
    let after_undo = habits(&db);
    assert_eq!(after_undo[1], x, "undo of y touched x");
    assert_eq!(after_undo[2].0, y.0);
    assert_eq!(after_undo[2].2, "y", "undo wrote another row's data onto y");
    assert!(after_undo[2].3, "undo did not delete y");

    datum(&db, &["redo"]);
    assert_eq!(habits(&db), vec![w, x, y], "redo after undo");
}

/// `del <winner's hid>` and `occur habit -u` each delete the winner, and say so.
#[test]
fn a_delete_of_the_winner_is_the_winner_s() {
    for undo in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("data.db");
        let [w, x, y] = pair(&db);
        let argv: &[&str] = if undo {
            &["occur", "habit", "-u"]
        } else {
            &["del", &y.0]
        };

        let out = datum(&db, argv);
        let named = out.split_whitespace().nth(1).unwrap();
        assert!(y.0.starts_with(named), "{argv:?} printed {out}");
        let dead = (y.0.clone(), y.1, y.2, true, None);
        assert_eq!(habits(&db), vec![w, x, dead], "{argv:?}");
    }
}
