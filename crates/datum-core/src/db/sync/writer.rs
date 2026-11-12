//! A writer's bounds (da-mogvf, vividn, 2026-11-12: «Both limits»): a
//! writer (da-o5o6m) writes only its filter's fields, where it names any,
//! and a spec of one of them, since a first log writes its field's spec; no
//! alias or other entry about the database; and changes only an entry whose
//! create is its own. Every copy's fold refuses the rest as it refuses a
//! reader's op, and the writer's own copy refuses it as it is typed.

use rusqlite::Connection;

use super::chain::{Chain, Role};
use super::{belongs_to, endpoint, own_alone, read_only, row, Endpoint, Op};
use crate::db::{is_meta_field, Db, SPEC_FIELD};
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
/// it specifies), `meta` whether its row is an alias or the like, and `own()`
/// whether the entry is the writer's to change ([`owns`]). Anyone else's op
/// is no writer's, and passes.
fn bounds(
    chain: &Chain,
    endpoint: &str,
    hlc: &str,
    field: Option<&str>,
    meta: bool,
    own: impl FnOnce() -> Result<bool>,
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
        _ if !own()? => Err(WRITES_OWN.into()),
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
        owns(chain, op, &creates_of(conn, &op.hid)?)
    })
}

/// Every create of `hid` this copy holds, applied, waiting or refused: the
/// set ownership is judged from, which only grows, so every copy holding
/// the same creates judges alike, in whatever order they came (da-lgvu8).
pub(super) fn creates_of(conn: &Connection, hid: &str) -> Result<Vec<Op>> {
    let mut ops: Vec<Op> = conn
        .prepare_cached("SELECT body FROM ops WHERE hid = ?1 AND applied IN (0, 1, 2)")?
        .query_map([hid], |r| r.get::<_, String>(0))?
        .map(|b| Op::parse(&b?))
        .collect::<Result<_>>()?;
    ops.retain(|o| o.effect.get("create").is_some());
    Ok(ops)
}

/// Whether writer op `op` keeps to its own entry, judged from `creates`, the
/// entry's, and never by a clock, which a writer sets for itself (da-lgvu8).
/// Its create stands unless a member created the entry too, or it saw
/// another's create of it: a member's entry is never a writer's, and a
/// create made over another's is no create of its own. Any other op stands
/// only where every create is its own. So a writer forging a create of
/// another writer's entry, saying it saw none, freezes it — neither's edits
/// stand — and is a conflict on each key it gives another value: it takes
/// nothing silently and erases nothing.
fn owns(chain: &Chain, op: &Op, creates: &[Op]) -> Result<bool> {
    let writer = |o: &Op| {
        chain
            .member(&o.endpoint)
            .is_some_and(|m| m.role_at(&o.hlc) == Role::Writer)
    };
    let mine = |c: &Op| c.endpoint == op.endpoint;
    Ok(if op.effect.get("create").is_some() {
        !creates
            .iter()
            .any(|c| !mine(c) && (!writer(c) || op.saw(c)))
    } else {
        !creates.is_empty() && creates.iter().all(mine)
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
    let chain = Chain::load(conn)?;
    judged(conn, &chain, &ep.id, post, hid)?.map_err(DatumError::Other)
}

/// Why writer `me` may not sign `post`, in `may_sign`'s words, if it may not.
fn judged(
    conn: &Connection,
    chain: &Chain,
    me: &str,
    post: &Entry,
    hid: &Hid,
) -> Result<std::result::Result<(), String>> {
    // An entry with no create here is this copy's own.
    let own = || {
        let creates = creates_of(conn, hid.as_str())?;
        Ok(creates.iter().all(|c| c.endpoint == me))
    };
    let field = belongs_to(&row(post));
    Ok(
        bounds(chain, me, "~", field.as_deref(), meta(&post.field), own)?
            .map_err(|why| format!("this copy is {why}")),
    )
}

impl Db {
    /// Where this copy is a writer in its sync chain, why it may not sign
    /// an entry, in `may_sign`'s words, if it may not (da-vspq5): a path
    /// that writes rows with no op, as `import` does, judges each before it
    /// writes, since the baseline signed for it later is refused by every
    /// other copy. None for a copy that is no writer, or in no chain. A
    /// reader's or a departing copy's is refused whole, as a log of it is,
    /// since its rows would sync nowhere (da-xe39u).
    #[allow(clippy::type_complexity)]
    pub fn sync_writer_bounds(
        &self,
    ) -> Result<Option<Box<dyn Fn(&Entry) -> Result<Option<String>> + '_>>> {
        let conn = self.conn();
        let chain = Chain::load(conn)?;
        if !chain.exists() {
            return Ok(None);
        }
        let ep = endpoint(conn)?;
        if let Some(why) = read_only(conn, &ep)? {
            return Err(DatumError::Other(why.into()));
        }
        let me = ep.id;
        let writer = chain
            .member(&me)
            .is_some_and(|m| m.role_at("~") == Role::Writer);
        Ok(writer.then(|| {
            Box::new(move |e: &Entry| Ok(judged(conn, &chain, &me, e, &e.hid)?.err()))
                as Box<dyn Fn(&Entry) -> Result<Option<String>>>
        }))
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
