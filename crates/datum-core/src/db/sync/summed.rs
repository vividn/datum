//! The summed state hash a verify compares (da-9ln3s): each row's part
//! kept as rows change, and what decides the kept parts are still this
//! build's (da-vpwkw, da-g9wph).

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::chain::Filter;
use super::{belongs_to, canonical_json, hex, sha256_hex, state_row, unhex};
use crate::db::{Seen, WriteBatch};
use crate::error::{DatumError, Result};
use crate::migrations::CURRENT_SCHEMA_VERSION;

/// The summed state hash of the rows passing `filters`, and what keeps
/// it ([`Db::sync_state_hash`]). The rows read are those a write touched
/// since the parts were kept, or every one under parts another
/// [`row_hash_rule`] wrote or a sampled row disagrees with
/// ([`sample_agrees`]), one row at a time (da-vpwkw).
pub(super) fn summed(
    conn: &Connection,
    seen: Seen,
    key: &str,
    filters: &[Filter],
) -> Result<([u8; 32], WriteBatch)> {
    let passes = |belongs: &Option<String>| filters.iter().all(|f| f.admits(belongs.as_deref()));
    let parts = |sql: &str| -> Result<Vec<([u8; 32], Option<String>)>> {
        let rows: Vec<(String, Option<String>)> = conn
            .prepare(sql)?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        rows.into_iter()
            .map(|(h, b)| {
                let h = unhex(&h).ok_or_else(|| DatumError::Other(format!("row hash {h}")))?;
                Ok((h, b))
            })
            .collect()
    };
    let stamp: Option<String> = conn
        .query_row(
            "SELECT value FROM _meta WHERE key = 'sync_row_hashes'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let rule = row_hash_rule(conn)?;
    let fresh = stamp.as_deref() == Some(rule.as_str()) && sample_agrees(conn)?;
    let mut keep = WriteBatch::on(seen);
    let touched = "IN (SELECT hid FROM sync_rows_changed)";
    let (mut sum, gone, now) = match fresh {
        true => {
            let kept: Option<String> = conn
                .query_row(
                    "SELECT sum FROM sync_state_sums WHERE key = ?1",
                    [key],
                    |r| r.get(0),
                )
                .optional()?;
            let sum = match kept.as_deref().map(unhex) {
                Some(sum) => sum.ok_or_else(|| DatumError::Other("state sum".into()))?,
                None => total(
                    parts("SELECT hash, belongs FROM sync_row_hashes")?
                        .iter()
                        .filter(|(_, b)| passes(b)),
                ),
            };
            keep.push(
                "DELETE FROM sync_row_hashes WHERE hid IN (SELECT hid FROM sync_rows_changed)",
                vec![],
            );
            (
                sum,
                parts(&format!(
                    "SELECT hash, belongs FROM sync_row_hashes WHERE hid {touched}"
                ))?,
                format!("SELECT * FROM entries WHERE hid {touched}"),
            )
        }
        false => {
            keep.push("DELETE FROM sync_row_hashes", vec![]);
            keep.push(
                "INSERT OR REPLACE INTO _meta (key, value) VALUES ('sync_row_hashes', ?1)",
                vec![rule.into()],
            );
            ([0; 32], Vec::new(), "SELECT * FROM entries".to_string())
        }
    };
    for (h, _) in gone.iter().filter(|(_, b)| passes(b)) {
        add(&mut sum, h, -1);
    }
    // Each row hashed as it is read, and its part kept as one JSON array:
    // every row of a database at once, read first, was a GiB (da-vpwkw).
    let mut kept = String::from("[");
    let mut stmt = conn.prepare(&now)?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let (h, hid, belongs) = row_part(state_row(r)?);
        if passes(&belongs) {
            add(&mut sum, &h, 1);
        }
        if kept.len() > 1 {
            kept.push(',');
        }
        kept.push_str(&json!([hid, hex(&h), belongs]).to_string());
    }
    kept.push(']');
    // Another filter's sum is kept only while no part moves under it.
    if !fresh || !gone.is_empty() || kept.len() > 2 {
        keep.push("DELETE FROM sync_state_sums", vec![]);
    }
    keep.push(
        "INSERT INTO sync_row_hashes (hid, hash, belongs)
         SELECT value ->> 0, value ->> 1, value ->> 2 FROM json_each(?1)",
        vec![kept.into()],
    );
    keep.push("DELETE FROM sync_rows_changed", vec![]);
    keep.push(
        "INSERT OR REPLACE INTO sync_state_sums (key, sum) VALUES (?1, ?2)",
        vec![key.to_string().into(), hex(&sum).into()],
    );
    Ok((sum, keep))
}

/// The oldest and newest rows no write touched since their parts were
/// kept, beside those parts: [`sample_agrees`] hashes them again.
const ROW_HASH_SAMPLE: &str = "SELECT e.*, k.hash AS kept_hash, k.belongs AS kept_belongs
      FROM entries e LEFT JOIN sync_row_hashes k ON k.hid = e.hid
     WHERE e.hid NOT IN (SELECT hid FROM sync_rows_changed)
       AND e.rowid IN (SELECT rowid FROM (SELECT rowid FROM entries ORDER BY rowid LIMIT 3)
                       UNION SELECT rowid FROM (SELECT rowid FROM entries ORDER BY rowid DESC LIMIT 3))";

/// Whether the kept parts of [`ROW_HASH_SAMPLE`]'s rows are the parts this
/// build computes for them (da-g9wph). [`row_hash_rule`]'s probe sees a
/// change to how a row is hashed only on the values it holds; this sees
/// one on values the database holds, and a part that differs, or none
/// kept, reads every row again. At most six rows a verify, whatever the
/// history: fewer where there are fewer, or a write touched an edge row,
/// which the verify reads again anyway.
fn sample_agrees(conn: &Connection) -> Result<bool> {
    let mut stmt = conn.prepare(ROW_HASH_SAMPLE)?;
    let mut rows = stmt.query([])?;
    while let Some(r) = rows.next()? {
        let kept: (Option<String>, Option<String>) = (r.get("kept_hash")?, r.get("kept_belongs")?);
        let (h, _, belongs) = row_part(state_row(r)?);
        if kept != (Some(hex(&h)), belongs) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A row's part of the summed state hash: the SHA-256 of its canonical
/// JSON, its hid, and the field it belongs to.
fn row_part(r: Map<String, Value>) -> ([u8; 32], Option<String>, Option<String>) {
    let (hid, belongs) = (r["hid"].as_str().map(String::from), belongs_to(&r));
    (
        Sha256::digest(canonical_json(&Value::Object(r))).into(),
        hid,
        belongs,
    )
}

/// The rows [`row_hash_rule`] hashes: between them every column of
/// `entries` a row's hash reads, and every key it writes, set
/// (`the_row_hash_probe_sets_every_column_and_key`).
pub(super) const ROW_HASH_PROBE: &str = r#"SELECT 'probe0000001' AS hid, 'pröbe:2026-01-02T03:04:05.678Z' AS slug,
           'c0' AS content_hash, 'pröbe' AS field, '"on"' AS state,
           'PT30M' AS dur, '"off"' AS following_state, 'block-switch' AS verb,
           '{"b":[1,2.5,-0.0,1e300,{"z":null,"a":"ü\n"}],"a":true}' AS data,
           '2026-01-02T03:04:05.678+00:00' AS at, 60 AS at_offset,
           'Europe/Berlin' AS at_tz, '5m' AS at_uncertainty, '2m' AS dur_uncertainty,
           52.52 AS lat, -13.405 AS lon, 'here' AS location,
           '2026-01-02T03:04:05.678901234+00:00' AS create_time,
           '2026-01-02T04:04:05+01:00' AS modify_time, 'd0' AS client_id,
           NULL AS deleted_at, 'probe0000002' AS superseded_by,
           '%at%' AS id_structure, '%f%' AS field_structure,
           'switch pröbe on' AS raw_input, '[{"alias":"p"}]' AS alias_chain,
           2 AS parser_version, '"off"' AS user_asserted_last_state,
           '{"m":1}' AS extra_metadata,
           '{"state":[{"device":"x","hlc":"h","op":"o/1","value":"on"}]}' AS conflicts
    UNION ALL
    SELECT 'probe0000002', 'spec:q', 'c1', 'spec', NULL, 'UNKNOWN', NULL, 'point',
           '{"field":"q"}', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL,
           '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z', 'd1',
           '2026-01-03T00:00:00Z', NULL, NULL, NULL, '', NULL, 1, NULL, NULL, NULL
    UNION ALL
    SELECT 'probe0000003', 'm:x', 'c2', 'm', '7', 'MOMENT', NULL, 'point',
           'null', '2026-01-02T03:04:05Z', -330, NULL, NULL, NULL, 0, 0, NULL,
           '2026-01-02T03:04:05Z', '2026-01-02T03:04:05Z', 'd2',
           NULL, NULL, NULL, NULL, NULL, 'null', 2, NULL, NULL, '{}'"#;

/// What the kept parts of the summed state hash were computed by, as
/// `_meta` `sync_row_hashes` stamps them (da-vpwkw): the schema, and the
/// parts this build computes for rows that set every column `state_row`
/// reads, one way and the other. A build that reads or writes a row
/// differently computes other parts for them and so reads every row
/// again; a deploy that does not, the most of them, reads none. Stamped
/// by the whole build before, every deploy of datum.day hashed every row
/// at its first verify: 14 s and a GiB of wasm memory at 290k rows.
pub(super) fn row_hash_rule(conn: &Connection) -> Result<String> {
    let probe = conn
        .prepare(ROW_HASH_PROBE)?
        .query_map([], state_row)?
        .map(|r| {
            let (h, _, belongs) = row_part(r?);
            Ok(format!("{} {}", hex(&h), belongs.unwrap_or_default()))
        })
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(format!(
        "v{CURRENT_SCHEMA_VERSION} {}",
        sha256_hex(&probe.join("\n"))
    ))
}

/// `sum` plus `sign` times `part`, as 256-bit little-endian numbers that wrap.
pub(super) fn add(sum: &mut [u8; 32], part: &[u8; 32], sign: i16) {
    let mut carry = 0;
    for (s, p) in sum.iter_mut().zip(part) {
        let v = *s as i16 + sign * *p as i16 + carry;
        *s = v.rem_euclid(256) as u8;
        carry = v.div_euclid(256);
    }
}

/// The sum of `parts`, as [`add`] adds them.
fn total<'a>(parts: impl Iterator<Item = &'a ([u8; 32], Option<String>)>) -> [u8; 32] {
    parts.fold([0; 32], |mut sum, (h, _)| {
        add(&mut sum, h, 1);
        sum
    })
}
