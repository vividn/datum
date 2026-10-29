//! `collect` (da-qhm51): a field's record, `get <field>::`. And what a key
//! operator (`key+=x`, `key-=x`, da-p9gf5) reads of it, and the drift
//! `check` finds when the merged history contradicts what one assumed.

use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value as JsonValue};

use crate::doc_ref::{data_refs, state_refs};
use crate::entry::Entry;
use crate::error::Result;

use super::types::LIVE_SQL;
use super::{is_meta_field, Db};

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

    /// The records that refer to `field`'s (da-xxqdb): each field whose
    /// record ([`Db::collect`]) holds the reference `<field>::`, in its state
    /// or under a data key by [`state_refs`] and [`data_refs`]' rules, with
    /// those keys, alphabetical. Read from the fields whose live entries
    /// spell the reference, and then their records, so a reference taken out
    /// since is none.
    pub fn backlinks(&self, field: &str) -> Result<Vec<(String, Vec<String>)>> {
        let target = format!("{field}::");
        // As the JSON text of a state or data spells it: the columns are
        // written by serde_json, so the string matches as stored, escapes
        // and all. A match only names a candidate; its record decides.
        let spelled = serde_json::to_string(&target)?;
        let spelled = &spelled[1..spelled.len() - 1];
        let mut stmt = self.conn.prepare(&format!(
            "SELECT DISTINCT field FROM entries WHERE {LIVE_SQL}
               AND (instr(state, ?1) OR instr(following_state, ?1) OR instr(data, ?1))
             ORDER BY field"
        ))?;
        let fields = stmt
            .query_map(params![spelled], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut links = Vec::new();
        for f in fields.into_iter().filter(|f| !is_meta_field(f)) {
            let spec = self.load_spec(&f)?;
            let mut keys: Vec<String> = self
                .collect(&f)?
                .into_iter()
                .filter(|(k, v)| {
                    match k.as_str() {
                        "state" => state_refs(spec.as_ref(), v),
                        k => data_refs(spec.as_ref(), k, v),
                    }
                    .contains(&target)
                })
                .map(|(k, _)| k)
                .collect();
            if !keys.is_empty() {
                keys.sort();
                links.push((f, keys));
            }
        }
        Ok(links)
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

/// The newest live entry of `field` ordered before `?3, ?4` (its time and
/// slug) that is not `?2` and whose data holds the key `?5`, whatever its
/// value. Walked back on `entries_live_field_hybrid`, so it reads up to the
/// first holder and no further; `json_each` names the key with no JSON path
/// to quote, and data that does not parse holds no key.
fn key_before_sql() -> String {
    format!(
        "{IN_ORDER} AND hid != ?2 AND (COALESCE(occur_time, create_time), slug) < (?3, ?4)
           AND EXISTS (SELECT 1 FROM json_each(CASE WHEN json_valid(data) THEN data END)
                       WHERE key = ?5)
         ORDER BY COALESCE(occur_time, create_time) DESC, slug DESC LIMIT 1"
    )
}

/// What `key` reads in `e`'s field's record just before `e`: the fold
/// [`Db::collect`] makes of the live entries ordered before it, which is the
/// newest of them holding the key. What a key operator changes, read in the
/// transaction that writes it.
pub fn key_before(conn: &Connection, e: &Entry, key: &str) -> Result<Option<JsonValue>> {
    // Compared as text: `to_rfc3339` is how both columns are written (`try_insert`).
    let at = e.occur_time.as_ref().map_or(e.create_time, |t| t.utc);
    let rows = rows(
        conn,
        &key_before_sql(),
        params![e.field, e.hid, at.to_rfc3339(), e.slug.0, key],
    )?;
    Ok(rows
        .into_iter()
        .next()
        .and_then(|(_, mut data, _)| data.remove(key))
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
            "SELECT DISTINCT e.field, k.key FROM entries e INDEXED BY entries_asserted_base,
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

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};
    use serde_json::json;

    use super::*;
    use crate::dur::Dur;
    use crate::entry::Verb;
    use crate::time::DatumTime;

    /// An occur of `issue` `minutes` past an hour, holding `data`.
    fn log(db: &Db, minutes: i64, data: JsonValue) -> Entry {
        let utc = Utc.with_ymd_and_hms(2026, 10, 26, 9, 0, 0).unwrap() + Duration::minutes(minutes);
        let time = DatumTime {
            utc,
            offset_minutes: 0,
            timezone: Some("UTC".into()),
            uncertainty: None,
        };
        let e = Entry::new(
            "issue",
            Verb::Point,
            None,
            Some(data),
            Some(time),
            Some(Dur::moment()),
            "t",
        );
        db.insert_entry(&e).unwrap();
        e
    }

    /// da-mzhxw: the newest holder before the entry gives the key, whatever
    /// its name, null reading as absent, and data that does not parse
    /// holding nothing.
    #[test]
    fn key_before_reads_the_newest_holder() {
        let db = Db::open_in_memory().unwrap();
        log(
            &db,
            0,
            json!({"blocked_by": ["x"], "odd.\"key": 1, "tag": "a", "0": "zero"}),
        );
        for i in 1..300 {
            log(&db, i, json!({"n": i}));
        }
        log(&db, 300, json!({"tag": null}));
        let broken = log(&db, 301, json!({"n": "hidden"}));
        db.conn
            .execute(
                "UPDATE entries SET data = '{' WHERE hid = ?1",
                [&broken.hid.0],
            )
            .unwrap();
        // Data that is no object holds no key: not "0" of an array.
        log(&db, 302, json!(["array"]));
        log(&db, 303, json!(5));
        let e = log(&db, 304, json!({}));
        log(&db, 305, json!({"n": "after"}));

        let read = |key| key_before(&db.conn, &e, key).unwrap();
        assert_eq!(read("blocked_by"), Some(json!(["x"])));
        assert_eq!(read("odd.\"key"), Some(json!(1)));
        assert_eq!(read("n"), Some(json!(299)));
        assert_eq!(read("tag"), None);
        assert_eq!(read("absent"), None);
        assert_eq!(read("0"), Some(json!("zero")));
    }
}
