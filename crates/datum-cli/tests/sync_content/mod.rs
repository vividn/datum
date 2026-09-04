//! Convergence assertions (da-0kmn) — the shared answer to "do these two
//! databases hold the same entries?", so every sync suite with two DBs to
//! compare asks it the same way and gets the same field-level diagnostic
//! when they do not.
//!
//! Kept apart from `sync_support` (the in-process relay harness) because
//! the comparison needs no server: a suite that only packs and applies
//! changesets locally can take this without spawning one.

use datum_core::db::Db;

#[allow(dead_code)]
pub fn live_fields(db: &Db) -> Vec<String> {
    let mut stmt = db
        .conn()
        .prepare("SELECT field FROM entries WHERE deleted_at IS NULL ORDER BY field")
        .unwrap();
    stmt.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
}

/// Every live row in full, as the canonical JSON projection of the
/// `Entry` it maps to, ordered by `hid` — the identity the merge keys on.
///
/// `live_rows` proves two devices hold the same NUMBER of rows; this
/// proves they hold the same ROWS. Loss shows up in a count; corruption
/// does not — a row that arrives with a mangled state, a lost `dur`, a
/// dropped data key or a wrong occur_time keeps the count exactly where
/// it was — and corruption is the half of the question this suite exists
/// to answer.
///
/// `live_hashes` is not enough either, and the gap is not incidental:
/// the `content_hash` preimage is seven fields, and `lat`/`lon`/
/// `location`, `occur_time_offset`/`occur_time_tz`, both uncertainties,
/// `verb`, `raw_input`, `alias_chain`, `parser_version`,
/// `user_asserted_last_state`, `extra_metadata` and the identity columns
/// are all deliberately outside it (docs/sync-protocol.md §Content hash).
/// Two rows with equal hashes can still differ in substance.
///
/// Projecting through `row_to_entry_public` rather than reading raw
/// columns is what makes the comparison total *and* fair: total because
/// every stored column is an `Entry` field, so a column added later is
/// compared without anyone having to remember this helper; fair because
/// the mapper applies the column-level normalisations (a `raw_input` of
/// `""` reads back as `None`) that the local insert path and the sync
/// apply path spell differently for the same fact.
#[allow(dead_code)]
pub fn live_content(db: &Db) -> Vec<serde_json::Value> {
    let mut stmt = db
        .conn()
        .prepare("SELECT * FROM entries WHERE deleted_at IS NULL ORDER BY hid")
        .unwrap();
    let rows: Vec<_> = stmt
        .query_map([], datum_core::db::row_to_entry_public)
        .unwrap()
        .map(|e| serde_json::to_value(e.unwrap()).unwrap())
        .collect();
    rows
}

/// Assert two devices converged: same rows, keyed on `hid`, equal in
/// every column.
///
/// Reports the first divergence precisely — which entry, which columns,
/// both values — because a bare `assert_eq!` on two row vectors prints
/// two walls of JSON and leaves the reader to diff them by eye.
#[track_caller]
#[allow(dead_code)]
pub fn assert_same_content(label_a: &str, db_a: &Db, label_b: &str, db_b: &Db) {
    let rows_a = live_content(db_a);
    let rows_b = live_content(db_b);
    let hids = |rows: &[serde_json::Value]| -> Vec<String> {
        rows.iter()
            .map(|r| r["hid"].as_str().unwrap_or("<no hid>").to_string())
            .collect()
    };
    assert_eq!(
        hids(&rows_a),
        hids(&rows_b),
        "{label_a} and {label_b} hold different entries \
         ({label_a} fields {:?}, {label_b} fields {:?})",
        live_fields(db_a),
        live_fields(db_b),
    );
    for (a, b) in rows_a.iter().zip(&rows_b) {
        if a == b {
            continue;
        }
        let (obj_a, obj_b) = (a.as_object().unwrap(), b.as_object().unwrap());
        let mut keys: Vec<&String> = obj_a.keys().chain(obj_b.keys()).collect();
        keys.sort_unstable();
        keys.dedup();
        let null = serde_json::Value::Null;
        let diffs: Vec<String> = keys
            .into_iter()
            .filter(|k| obj_a.get(*k) != obj_b.get(*k))
            .map(|k| {
                format!(
                    "  {k}: {label_a}={} {label_b}={}",
                    obj_a.get(k).unwrap_or(&null),
                    obj_b.get(k).unwrap_or(&null),
                )
            })
            .collect();
        panic!(
            "{label_a} and {label_b} disagree on entry {}:\n{}",
            a["hid"].as_str().unwrap_or("<no hid>"),
            diffs.join("\n"),
        );
    }
}
