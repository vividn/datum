//! A writer's bounds (da-mogvf, vividn, 2026-11-12: «Both limits»): a
//! writer (da-o5o6m) writes only its filter's fields, where it names any,
//! and a spec of one of them, since a first log writes its field's spec; no
//! alias or other entry about the database; and changes only an entry whose
//! create is its own. Every copy's fold refuses the rest as it refuses a
//! reader's op, and the writer's own copy refuses it as it is typed.

use rusqlite::Connection;

use super::chain::{Chain, Role};
use super::{belongs_to, ops_of, own_alone, read_only, row, Endpoint, Op};
use crate::db::{is_meta_field, SPEC_FIELD};
use crate::entry::Entry;
use crate::error::{DatumError, Result};
use crate::ids::Hid;

/// Why a writer's alias or other entry about the database is refused.
const WRITES_NO_META: &str = "a writer in the sync chain: it writes entries and their fields' \
     specs, and no alias or other entry about the database";

/// Why a writer's change to an entry another copy created is refused.
const WRITES_OWN: &str = "a writer in the sync chain: it changes only the entries it created";

/// What a writer of `fields` writes, as its invite says it: «mood», its
/// own entries; with none named, its own entries of any field.
pub fn writes_said<'a>(fields: impl IntoIterator<Item = &'a String>) -> String {
    match words(fields) {
        w if w.is_empty() => "its own entries, of any field".into(),
        w => format!("{w}, its own entries"),
    }
}

/// Fields in words: «mood», «tea».
fn words<'a>(fields: impl IntoIterator<Item = &'a String>) -> String {
    let words: Vec<_> = fields.into_iter().map(|f| format!("«{f}»")).collect();
    words.join(", ")
}

/// Whether an entry of `field` describes the database, a spec aside.
fn meta(field: &str) -> bool {
    field != SPEC_FIELD && is_meta_field(field)
}

/// Why an entry op of `endpoint` at `hlc`, a writer then, is refused, if it
/// is: `field` is the one its entry belongs to after it (a spec's, the field
/// it specifies), `meta` whether its row is an alias or the like, and `by()`
/// the endpoint whose create the entry's ops open with: none, where no op
/// held creates it, is no writer's own, and refused. Anyone else's op is no
/// writer's, and passes.
fn bounds(
    chain: &Chain,
    endpoint: &str,
    hlc: &str,
    field: Option<&str>,
    meta: bool,
    by: impl FnOnce() -> Result<Option<String>>,
) -> Result<std::result::Result<(), String>> {
    let Some(m) = chain.member(endpoint) else {
        return Ok(Ok(()));
    };
    if m.role_at(hlc) != Role::Writer {
        return Ok(Ok(()));
    }
    // Sent no field (`--fields ''`, a sensor), it writes any.
    let writes = m.filter.as_ref().and_then(|f| f.fields.as_ref());
    let writes = writes.filter(|w| !w.is_empty());
    Ok(match () {
        _ if meta => Err(WRITES_NO_META.into()),
        _ if writes.is_some_and(|w| field.is_none_or(|f| !w.contains(f))) => Err(format!(
            "a writer of {} in the sync chain, and this is {}",
            writes.map_or(String::new(), words),
            field.map_or("no field".into(), |f| format!("«{f}»")),
        )),
        _ if by()?.as_deref() != Some(endpoint) => Err(WRITES_OWN.into()),
        _ => Ok(()),
    })
}

/// Why the chain refuses entry op `op`, if it does: its endpoint's
/// membership, then a writer's bounds. Never a chain record: `settle` judges
/// those by the chain's fold, and only an entry op is ever refused.
pub(super) fn allowed(
    conn: &Connection,
    chain: &Chain,
    op: &Op,
) -> Result<std::result::Result<(), String>> {
    if let Err(why) = chain.allows(&op.endpoint, &op.hlc) {
        return Ok(Err(why));
    }
    let row = op.effect.get("create").or_else(|| op.effect.get("set"));
    let meta = row.and_then(|r| r["field"].as_str()).is_some_and(meta);
    let field = op.field.as_deref();
    bounds(chain, &op.endpoint, &op.hlc, field, meta, || {
        // The create `op`'s entry opens with, `op` among its ops.
        let mut ops = ops_of(conn, &op.hid)?;
        ops.push(op.clone());
        ops.sort_by(|a, b| a.order().cmp(&b.order()));
        let create = ops.into_iter().find(|o| o.effect.get("create").is_some());
        Ok(create.map(|o| o.endpoint))
    })
}

/// Why this copy may not sign the change that left `post`, if it may not:
/// a reader's or a departing copy's, or a writer's past its bounds, judged
/// as of now, the latest role, which sorts after every HLC. An entry with no
/// op is this copy's own, which the chain never saw: the baseline `emit`
/// signs for it is the create every copy's fold reads as the writer's.
pub(super) fn may_sign(conn: &Connection, ep: &Endpoint, post: &Entry, hid: &Hid) -> Result<()> {
    if let Some(why) = read_only(conn, ep)? {
        return Err(DatumError::Other(why.into()));
    }
    if !own_alone(conn, ep)? {
        return Ok(());
    }
    let by = || {
        let ops = ops_of(conn, hid.as_str())?;
        let create = ops.into_iter().find(|o| o.effect.get("create").is_some());
        Ok(Some(create.map_or(ep.id.clone(), |o| o.endpoint)))
    };
    let field = belongs_to(&row(post));
    let chain = Chain::load(conn)?;
    match bounds(&chain, &ep.id, "~", field.as_deref(), meta(&post.field), by)? {
        Err(why) => Err(DatumError::Other(format!("this copy is {why}"))),
        Ok(()) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A writer's invite says what it writes.
    #[test]
    fn a_writers_invite_says_what_it_writes() {
        let said = |f: &[&str]| writes_said(&f.iter().map(|f| f.to_string()).collect::<Vec<_>>());
        assert_eq!(said(&["mood", "tea"]), "«mood», «tea», its own entries");
        assert_eq!(said(&[]), "its own entries, of any field");
    }
}
