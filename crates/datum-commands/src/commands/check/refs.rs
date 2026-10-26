//! The `refs` checker: a state or data reference that names no live
//! entry, or a record reference whose field does not exist.

use anyhow::Result;
use datum_core::db::Db;
use datum_core::entry::Entry;

use super::{Domain, Issue, IssueExample, Severity};

/// da-f9jyv: state references that name no live entry — a `field:id`
/// state, and a bare state whose `F:<state>` document is missing (the
/// rules are [`state_refs`](datum_core::doc_ref::state_refs)'s). da-s5k2s: and data references, per key
/// (the rules are [`data_refs`](datum_core::doc_ref::data_refs)'s). A tombstoned document is missing. da-xxqdb:
/// a record reference (`field::`) is missing when the field has no entry and no spec of its own. One
/// warning per (field, key, slug) with a count and sample hids, since a
/// tracker has thousands of rows in one state. Never an error: the
/// document may yet arrive by sync.
pub fn collect_dangling_refs(db: &Db, entries: &[Entry]) -> Result<Vec<Issue>> {
    use datum_core::doc_ref::{data_refs, record_ref, state_refs};
    let registry = db.load_registry()?;
    let live: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.deleted_at.is_none() && e.superseded_by.is_none())
        .collect();
    let slugs: std::collections::HashSet<&str> = live.iter().map(|e| e.slug.as_str()).collect();
    // key `None` is the state; `Some(k)` the data key `k`.
    type Missing<'a> = (&'a str, Option<&'a str>, String);
    let mut missing: std::collections::BTreeMap<Missing, Vec<String>> = Default::default();
    for e in &live {
        let spec = registry.field_spec(&e.field);
        let state = [&e.state, &e.following_state]
            .into_iter()
            .flatten()
            .flat_map(|s| state_refs(spec, s))
            .map(|slug| (None, slug));
        let data = e
            .data
            .as_ref()
            .and_then(|d| d.as_object())
            .into_iter()
            .flatten()
            .flat_map(|(k, v)| {
                data_refs(spec, k, v)
                    .into_iter()
                    .map(|s| (Some(k.as_str()), s))
            });
        let mut refs: Vec<_> = state.chain(data).collect();
        refs.sort();
        refs.dedup();
        for (key, slug) in refs
            .into_iter()
            .filter(|(_, r)| !slugs.contains(r.as_str()))
        {
            missing
                .entry((&e.field, key, slug))
                .or_default()
                .push(e.short_uid());
        }
    }
    // A record reference names no slug: it is missing when its field is.
    let mut live = Vec::new();
    for (_, _, slug) in missing.keys() {
        if record_ref(slug).map_or(Ok(false), |f| db.field_exists(f))? {
            live.push(slug.clone());
        }
    }
    missing.retain(|(_, _, slug), _| !live.contains(slug));
    Ok(missing
        .into_iter()
        .map(|((field, key, slug), hids)| {
            let count = hids.len();
            let more = if count > 3 { ", …" } else { "" };
            let y = if count == 1 { "y refers" } else { "ies refer" };
            let record = record_ref(&slug).is_some();
            let no = ["no live entry", "no field (no entries, no spec)"][record as usize];
            let (category, message) = match key {
                None => (
                    "dangling_state_ref",
                    format!("{count} {field} entr{y} to {slug}, which is {no}"),
                ),
                Some(key) => (
                    "dangling_data_ref",
                    format!("{count} {field} entr{y} to {slug}, which is {no} (key {key})"),
                ),
            };
            Issue {
                domain: Domain::Entries,
                severity: Severity::Warning,
                category,
                message,
                count,
                example: Some(IssueExample {
                    id: Some(hids[0].clone()),
                    field: Some(field.to_string()),
                    detail: Some(format!("hids: {}{more}", hids[..count.min(3)].join(", "))),
                }),
                entries: Vec::new(),
                rendered_block: String::new(),
            }
        })
        .collect())
}
