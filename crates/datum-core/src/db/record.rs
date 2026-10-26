//! `collect` (da-qhm51): a field's record, `get <field>::`. And what a key
//! operator (`key+=x`, `key-=x`, da-p9gf5) reads of it, and the drift
//! `check` finds when the merged history contradicts what one assumed.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value as JsonValue};

use crate::entry::Entry;
use crate::error::Result;

use super::types::LIVE_SQL;
use super::Db;

impl Db {
    /// The record of `field`: its state now ([`Db::state_at`]) and the fold
    /// of its live entries' data in the listing's order, `occur_time ??
    /// create_time` then slug, so a timeless entry folds at its create time.
    /// Each key holds the latest entry carrying it, and `null` clears it:
    /// JSON Merge Patch (RFC 7396), an array replaced whole. `comment` is
    /// gathered instead, each comment with its entry's time.
    pub fn collect(&self, field: &str) -> Result<Map<String, JsonValue>> {
        let mut stmt = self.conn.prepare_cached(&format!(
            "SELECT data, COALESCE(occur_time, create_time) FROM entries
             WHERE field = ?1 AND {LIVE_SQL} AND data IS NOT NULL
             ORDER BY COALESCE(occur_time, create_time), slug"
        ))?;
        let rows = stmt.query_map(params![field], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut record = Map::new();
        let mut comments = Vec::new();
        for row in rows {
            let (data, time) = row?;
            // Data that is no object holds no keys, and data that does not
            // parse is none, as every row read takes it (`row_mapper.rs`).
            let Ok(JsonValue::Object(data)) = serde_json::from_str(&data) else {
                continue;
            };
            for (key, value) in data {
                match (key.as_str(), value) {
                    ("comment", JsonValue::Array(said)) => comments.extend(
                        said.into_iter()
                            .map(|text| json!({"time": time, "text": text})),
                    ),
                    // Gathered, so a null clears none of them.
                    ("comment", JsonValue::Null) => {}
                    // A row from before comments were arrays holds a string.
                    ("comment", text) => comments.push(json!({"time": time, "text": text})),
                    (_, JsonValue::Null) => {
                        record.remove(&key);
                    }
                    (_, value) => {
                        record.insert(key, value);
                    }
                }
            }
        }
        if !comments.is_empty() {
            record.insert("comment".into(), comments.into());
        }
        match self.state_at(field, Utc::now())? {
            Some(state) if !state.is_null() => record.insert("state".into(), state),
            _ => record.remove("state"),
        };
        Ok(record)
    }
}

/// The live entries of `field` in the record's order, `occur_time ??
/// create_time` then slug: each one's hid, data and `asserted_base`.
const IN_ORDER: &str = "SELECT hid, data, json_extract(extra_metadata, '$.asserted_base')
     FROM entries WHERE field = ?1 AND deleted_at IS NULL AND superseded_by IS NULL
       AND data IS NOT NULL";
const ORDER: &str = "ORDER BY COALESCE(occur_time, create_time), slug";

type Row = (String, Map<String, JsonValue>, Map<String, JsonValue>);

fn rows(conn: &Connection, sql: &str, args: impl rusqlite::Params) -> Result<Vec<Row>> {
    let object = |s: Option<String>| match s.and_then(|s| serde_json::from_str(&s).ok()) {
        Some(JsonValue::Object(m)) => m,
        _ => Map::new(),
    };
    let mut stmt = conn.prepare_cached(sql)?;
    let rows = stmt.query_map(args, |r| {
        Ok((r.get(0)?, object(r.get(1)?), object(r.get(2)?)))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// What `key` reads in `e`'s field's record just before `e`: the fold
/// [`Db::collect`] makes of the live entries ordered before it. What a key
/// operator changes, read in the transaction that writes it.
pub fn key_before(conn: &Connection, e: &Entry, key: &str) -> Result<Option<JsonValue>> {
    // Compared as text: `to_rfc3339` is how both columns are written (`try_insert`).
    let at = e.occur_time.as_ref().map_or(e.create_time, |t| t.utc);
    let sql = format!(
        "{IN_ORDER} AND hid != ?2 AND (COALESCE(occur_time, create_time), slug) < (?3, ?4) {ORDER}"
    );
    let rows = rows(
        conn,
        &sql,
        params![e.field, e.hid, at.to_rfc3339(), e.slug.0],
    )?;
    Ok(rows
        .into_iter()
        .rev()
        .find_map(|(_, mut data, _)| data.remove(key))
        .filter(|v| !v.is_null()))
}

/// A value's members, as a key operator reads it: none for no value or
/// null, an array's items, and otherwise the value alone.
pub fn members(v: Option<&JsonValue>) -> Vec<JsonValue> {
    match v {
        None | Some(JsonValue::Null) => vec![],
        Some(JsonValue::Array(items)) => items.clone(),
        Some(v) => vec![v.clone()],
    }
}

fn same_members(a: &[JsonValue], b: &[JsonValue]) -> bool {
    a.iter().all(|x| b.contains(x)) && b.iter().all(|x| a.contains(x))
}

/// A key a `+=` / `-=` edited whose value the merged history now gives
/// otherwise (da-p9gf5).
#[derive(Debug, Clone)]
pub struct KeyDrift {
    pub field: String,
    pub key: String,
    /// What the record holds.
    pub held: JsonValue,
    /// What the history's changes to it give, replayed: what `check --fix`
    /// writes.
    pub replayed: JsonValue,
    /// The edits whose `asserted_base` the history before them contradicts.
    pub contradicted: Vec<String>,
}

impl Db {
    /// Every key an edit's `asserted_base` names whose record the replay of
    /// its history reads otherwise.
    pub fn key_drift(&self) -> Result<Vec<KeyDrift>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT e.field, k.key FROM entries e,
               json_each(e.extra_metadata, '$.asserted_base') k
             WHERE e.deleted_at IS NULL AND e.superseded_by IS NULL
               AND json_type(e.extra_metadata, '$.asserted_base') = 'object'
             ORDER BY e.field, k.key",
        )?;
        let keys = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut drift = Vec::new();
        for (field, key) in keys {
            drift.extend(replay(&self.conn, &field, &key)?);
        }
        Ok(drift)
    }
}

/// `key` of `field` as its history makes it, the observed-remove set
/// (vividn, 2026-10-26: add-wins): each entry in the record's order changes
/// it by what it wrote against what it read, an edit's read being its
/// `asserted_base` and a plain value's the key as it stood. An element added
/// is tagged by the entry adding it; one taken out loses only the tags its
/// entry had seen (its create's `deps`), so an add the remover never saw
/// stands. A pure function of the merged history, so every copy reads the
/// same. `None` when the record already holds it.
fn replay(conn: &Connection, field: &str, key: &str) -> Result<Option<KeyDrift>> {
    let mut held: Option<JsonValue> = None;
    let mut set: Vec<(JsonValue, Vec<String>)> = Vec::new();
    let mut contradicted = Vec::new();
    for (hid, mut data, asserted) in rows(conn, &format!("{IN_ORDER} {ORDER}"), [field])? {
        let Some(value) = data.remove(key) else {
            continue;
        };
        let now = members(Some(&value));
        let was = match asserted.get(key) {
            Some(base) => {
                let base = members(Some(base));
                if !same_members(&base, &members(held.as_ref())) {
                    contradicted.push(hid.clone());
                }
                base
            }
            // A plain value read the key as the history had made it so far.
            None => set.iter().map(|(x, _)| x.clone()).collect(),
        };
        for (_, tags) in set
            .iter_mut()
            .filter(|(x, _)| was.contains(x) && !now.contains(x))
        {
            let mut unseen = Vec::new();
            for tag in tags.drain(..) {
                // An entry with no create op to tell by (`None`) counts as
                // having seen everything before it.
                if !super::sync::made_seeing(conn, &hid, &tag)?.unwrap_or(true) {
                    unseen.push(tag);
                }
            }
            *tags = unseen;
        }
        set.retain(|(_, tags)| !tags.is_empty());
        for x in now.into_iter().filter(|x| !was.contains(x)) {
            match set.iter_mut().find(|(y, _)| *y == x) {
                Some((_, tags)) => tags.push(hid.clone()),
                None => set.push((x, vec![hid.clone()])),
            }
        }
        held = Some(value).filter(|v| !v.is_null());
    }
    let replayed: Vec<JsonValue> = set.into_iter().map(|(x, _)| x).collect();
    if same_members(&replayed, &members(held.as_ref())) {
        return Ok(None);
    }
    Ok(Some(KeyDrift {
        field: field.into(),
        key: key.into(),
        held: held.unwrap_or(JsonValue::Null),
        replayed: replayed.into(),
        contradicted,
    }))
}
