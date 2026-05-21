//! Shared `RenderContext` construction for the read-side commands.
//!
//! Extracted from `cmd.rs` so tail / get / edit / view / nowview / dayview /
//! update_core / log_entry can share the per-batch state-probe cache without
//! pulling in the dispatch shell.

use chrono::Utc;
use datum_core::{
    db::Db,
    entry::{Entry, Verb},
};

use crate::render::RenderContext;

/// Vector form of `RenderContext::from_events` — one context per entry, in
/// the same order. Used by the table path so the canonical state-cell
/// renderer can emit transition arrows for tail rows.
///
/// Per da-uuu: builds a per-field [`FieldStateEvents`] cache up front so the
/// lead / trail / ongoing state probes run in memory instead of issuing one
/// (multi-row, all-columns) `last_state_at` query per entry. The previous
/// implementation re-fetched the same field's state history once per output
/// row, which dominated `dm tail` wall-clock — especially with `-yt`/`-d`
/// filters where the probe time is far enough from `now` to defeat the OS
/// page cache. The cache is scoped to this batch.
pub(crate) fn build_render_contexts(entries: &[Entry], db: &Db) -> Vec<RenderContext> {
    use std::collections::HashMap;
    // Bound the per-field load by the latest probe time we'll issue. Block
    // trail probes go to `end_time`; everything else to `start_time` (or
    // `create_time` for the rare state-affecting row that lacks a start).
    // Events strictly later than this max never affect any probe, so passing
    // it to the loader prunes the WHERE clause and skips work for histories
    // that extend far past this batch (`tail -yt 15` on a DB written to all
    // morning will skip every today-event).
    let max_probe: Option<chrono::DateTime<Utc>> = entries
        .iter()
        .filter(|e| needs_state_probe(e))
        .filter_map(probe_upper_bound)
        .max();
    let mut cache: HashMap<String, datum_core::state::FieldStateEvents> = HashMap::new();
    for e in entries {
        if !cache.contains_key(&e.field) && needs_state_probe(e) {
            let events = datum_core::state::FieldStateEvents::load_for_field_up_to(
                db.conn(),
                &e.field,
                max_probe,
            )
            .unwrap_or_default();
            cache.insert(e.field.clone(), events);
        }
    }
    entries
        .iter()
        .map(|e| RenderContext::from_events(cache.get(&e.field), e, db))
        .collect()
}

/// Whether this entry's `RenderContext` needs any state-history probe at
/// all. `Add`/`Note` rows skip the cache load entirely.
fn needs_state_probe(entry: &Entry) -> bool {
    matches!(
        entry.verb,
        Verb::Switch | Verb::Block | Verb::BlockSwitch | Verb::PointSwitch | Verb::Point
    )
}

/// The latest probe time the batch will issue for this entry. Block rows
/// probe at `end_time`; everything else at `start_time`, falling back to
/// `create_time` for state-affecting rows that lack a start.
fn probe_upper_bound(entry: &Entry) -> Option<chrono::DateTime<Utc>> {
    if matches!(entry.entry_shape, datum_core::entry::EntryShape::Block) {
        if let Some(end) = entry.end_time.as_ref() {
            return Some(end.utc);
        }
    }
    entry
        .start_time
        .as_ref()
        .map(|t| t.utc)
        .or(Some(entry.create_time))
}
