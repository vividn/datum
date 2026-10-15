//! Shared `RenderContext` construction for the read-side commands.
//!
//! Extracted from `cmd.rs` so tail / get / edit / view / nowview / dayview /
//! update_core / log_entry can share the per-batch state-probe cache without
//! pulling in the dispatch shell.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use datum_core::{
    db::{Db, ListParams, TimeMetric},
    entry::Entry,
    field_spec::{resolve_positions, FieldSpec, ResolvedPosition},
};

use crate::render::RenderContext;

/// Every field's `(y, height, z_index)` with `relative_to` anchors honoured
/// (da-btyvj): the single-spec resolver cannot see an anchor, so a surface
/// using it drew an anchored field at its hash. An anchor need not have
/// marks in the window, so the specs its chain names are read in.
pub(crate) fn field_positions(
    db: &Db,
    specs: &HashMap<String, FieldSpec>,
    fields: &[&str],
) -> datum_core::error::Result<HashMap<String, ResolvedPosition>> {
    let mut all = specs.clone();
    let mut asked: HashSet<String> = all.keys().cloned().collect();
    loop {
        let next: Vec<String> = all
            .values()
            .filter_map(|s| s.presentation.relative_to.clone())
            .filter(|a| asked.insert(a.clone()))
            .collect();
        if next.is_empty() {
            break;
        }
        let names: Vec<&str> = next.iter().map(String::as_str).collect();
        all.extend(db.batch_load_specs(&names)?);
    }
    Ok(resolve_positions(&all, fields.iter().map(|f| f.to_string())).positions)
}

/// Query window shared by the live render surfaces — dayview's day builder
/// and both nowview field builders (da-v0uy). One constructor so the three
/// mirrors can't drift: every surface clamps the window's right edge to the
/// caller's exact `end` instant. The bound is sub-ms precise so entries
/// stamped at "now" stay included, while future-dated entries stay out —
/// the now-line is the visual promise; rendering the future deserves a
/// deliberate feature, not an accident of an unset bound. The leading
/// `fetch_start` buffer is the caller's choice; the layout clips to the
/// visible window.
pub(crate) fn live_window_params(fetch_start: DateTime<Utc>, end: DateTime<Utc>) -> ListParams {
    ListParams {
        fields: Vec::new(),
        metric: TimeMetric::Hybrid,
        start_utc: Some(fetch_start),
        end_utc: Some(end),
        limit: None,
        head: false,
        reverse: false,
        // da-ghlx: dayview/nowview are live-data surfaces; review markers /
        // protocol notes don't belong in the visual render.
        include_meta: false,
        include_notes: false,
        before: None,
    }
}

/// Vector form of `RenderContext::from_rows` — one context per entry, in
/// the same order. Used by the table path so the canonical state-cell
/// renderer can emit transition arrows for tail rows.
///
/// Per da-uuu: reads each field's state rows once up front so the lead /
/// trail / ongoing probes run in memory instead of issuing one query per
/// entry. What is read is the span the batch's rows of that field probe
/// ([`probed_state_rows`](crate::render::action::probed_state_rows)), so
/// ten rows cost ten rows' worth however long the field's history
/// (da-asgt4). The cache is scoped to this batch.
pub(crate) fn build_render_contexts(entries: &[Entry], db: &Db) -> Vec<RenderContext> {
    let mut cache: HashMap<&str, datum_core::state::FieldStateRows> = HashMap::new();
    for e in entries {
        cache.entry(&e.field).or_insert_with(|| {
            let of_field = entries.iter().filter(|x| x.field == e.field);
            crate::render::action::probed_state_rows(db, &e.field, of_field)
        });
    }
    entries
        .iter()
        .map(|e| RenderContext::from_rows(cache.get(e.field.as_str()), e))
        .collect()
}

/// [`build_render_contexts`] plus the `_N` quickId ordinals (da-8sx8) and
/// the sync-conflict marker (da-uxgv).
///
/// Used by the listing surfaces — `tail` / `head` / `grep` / the tail TUI —
/// where `_N` is a navigational aid: the reader sees which row `_3` names
/// without counting. The single-row preview surfaces (`get`, `del`,
/// `update`) deliberately stay on the plain builder; an ordinal is a
/// property of the current listing, not of the entry.
pub(crate) fn build_table_contexts(entries: &[Entry], db: &Db) -> Vec<RenderContext> {
    let mut ctxs = build_render_contexts(entries, db);
    // The ordinal universe is the whole database, not this listing — a
    // filtered `tail mood` shows gaps rather than a renumbered 1..n that
    // wouldn't resolve. Attaching it is the shared step (da-6ec2): the
    // browser computes the same map over the rows it holds
    // (`render::quick_ordinal_map`) and hangs it on with this function.
    let ordinals = datum_core::ids::quick_ordinals(db).unwrap_or_default();
    crate::render::attach_quick_ordinals(&mut ctxs, entries, &ordinals);
    // One query for the batch, served by a partial index that holds only
    // conflicted rows — empty whenever there is no conflict to mark.
    let conflicted = db.hids_with_supersedees().unwrap_or_default();
    for (ctx, e) in ctxs.iter_mut().zip(entries) {
        ctx.has_supersedees = conflicted.contains(&e.hid);
    }
    ctxs
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::test_util::{entry_builder, fresh_db};

    /// The wiring proof for da-8sx8: what a listing prints as `_N` is
    /// what `_N` resolves to. The renderer only ever sees the ordinal
    /// this helper attaches, so agreement here is agreement on screen.
    #[test]
    fn table_contexts_carry_resolvable_quick_ordinals() {
        let db = fresh_db();
        let base: chrono::DateTime<Utc> = "2026-05-01T12:00:00Z".parse().unwrap();
        let entries: Vec<Entry> = (0..4)
            .map(|i| {
                entry_builder("mood")
                    .time_start_utc(base + chrono::Duration::minutes(i))
                    .insert(&db)
            })
            .collect();

        // Newest-first, the order `tail` renders in reverse of.
        let listed: Vec<Entry> = entries.iter().rev().cloned().collect();
        let ctxs = build_table_contexts(&listed, &db);

        for (e, ctx) in listed.iter().zip(&ctxs) {
            let n = ctx
                .quick_ordinal
                .expect("every seeded row is within the cap");
            let resolved =
                datum_core::ids::resolve_quickid_single(&db, &format!("_{n}")).expect("resolve");
            assert_eq!(resolved.hid, e.hid, "_{n} must name the row it labels");
        }
        assert_eq!(ctxs[0].quick_ordinal, Some(1));
        assert_eq!(ctxs[3].quick_ordinal, Some(4));
    }

    /// da-uxgv: the tail marks the row that *won* a contest, not the row
    /// that lost it — the loser is out of the stream, so the only place a
    /// conflict can be shown is on the row the reader can see.
    #[test]
    fn table_contexts_mark_a_row_with_supersedees() {
        let db = fresh_db();
        let base: chrono::DateTime<Utc> = "2026-05-01T12:00:00Z".parse().unwrap();
        let winner = entry_builder("mood").time_start_utc(base).insert(&db);
        let bystander = entry_builder("mood")
            .time_start_utc(base + chrono::Duration::minutes(5))
            .insert(&db);
        let loser = entry_builder("mood")
            .time_start_utc(base + chrono::Duration::minutes(9))
            .insert(&db);
        db.conn()
            .execute(
                "UPDATE entries SET superseded_by = ?2 WHERE hid = ?1",
                rusqlite::params![loser.hid, winner.hid],
            )
            .unwrap();

        let listed = vec![winner.clone(), bystander.clone()];
        let ctxs = build_table_contexts(&listed, &db);
        assert!(ctxs[0].has_supersedees, "the winner carries the marker");
        assert!(
            !ctxs[1].has_supersedees,
            "an uninvolved row carries nothing",
        );
    }

    /// da-6ec2: the pure ordering and the SQL one are the same ordering.
    /// The browser cannot run `ORDER BY occur_time DESC, slug DESC`, so it
    /// spells that rule in Rust — and this is what holds the spelling to
    /// the query it stands in for.
    #[test]
    fn the_pure_ordinal_map_agrees_with_the_query() {
        let db = fresh_db();
        let base: chrono::DateTime<Utc> = "2026-05-01T12:00:00Z".parse().unwrap();
        let mut entries: Vec<Entry> = (0..6)
            .map(|i| {
                entry_builder(if i % 2 == 0 { "mood" } else { "work" })
                    .time_start_utc(base + chrono::Duration::minutes(i))
                    .insert(&db)
            })
            .collect();
        // Two rows at the same instant, so the `slug DESC` tiebreak is
        // exercised rather than assumed.
        entries.push(
            entry_builder("aaa")
                .time_start_utc(base + chrono::Duration::minutes(5))
                .insert(&db),
        );
        assert_eq!(
            crate::render::quick_ordinal_map(&entries),
            datum_core::ids::quick_ordinals(&db).unwrap()
        );
    }

    /// The plain builder leaves the ordinal unset, so the preview
    /// surfaces (`get` / `del` / `update`) keep their column-free table.
    #[test]
    fn plain_contexts_have_no_quick_ordinal() {
        let db = fresh_db();
        let e = entry_builder("mood").insert(&db);
        let ctxs = build_render_contexts(std::slice::from_ref(&e), &db);
        assert_eq!(ctxs[0].quick_ordinal, None);
    }
}
