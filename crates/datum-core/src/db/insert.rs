//! Entry insert / update paths and the in-transaction helpers they share.
//!
//! All write paths funnel through `insert_entry_in_tx` (live writes; runs
//! conflict detection + history) or `insert_entry_bulk_in_tx` (migration
//! fast path; skips both). The `Db::insert_*` methods just open a tx and
//! delegate.

use chrono::Utc;
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::entry::{new_hid, Entry, Verb};
use crate::error::{DatumError, Result};
use crate::history::{self, ChangeKind};
use crate::ids::{Hid, Slug};
use crate::verb::derive_verb;

use super::lookup::fetch_entry_any;
use super::row_mapper::row_to_entry;
use super::types::{ConflictStrategy, InsertOutcome};
use super::Db;

/// Derive the storage-layer verb string and canonical state JSON for an
/// outgoing entry write. da-5ns.3: every insert/update path runs this so
/// the `verb` column is sourced from `derive_verb(state, dur, fs)` rather
/// than the legacy stub.
///
/// One storage-verb-driven promotion remains: when the caller pre-stamps
/// the entry with a switch-style storage verb (`Switch`, `BlockSwitch`,
/// `PointSwitch`) and `state` is `None`, the state is canonicalized to
/// `true` so that direct [`EntryBuilder`](crate::test_util::EntryBuilder)
/// writes hit the same rule the live insert path uses. Action-driven
/// canonicalization (`Action::Stop` → `false`, `Action::Start` → `true`)
/// happens in `log_entry` before the entry reaches storage — see
/// `da-8y2q.2`.
fn derive_verb_for_storage(entry: &Entry) -> Result<(String, Option<String>)> {
    let state_in = match (entry.state.clone(), &entry.verb) {
        (None, Verb::Switch | Verb::BlockSwitch | Verb::PointSwitch) => {
            Some(serde_json::Value::Bool(true))
        }
        (s, _) => s,
    };
    let (verb, canonical_state) =
        derive_verb(state_in, entry.dur.as_ref(), entry.following_state.clone())?;
    let verb_str = verb.to_string();
    let state_str = canonical_state.as_ref().map(|v| v.to_string());
    Ok((verb_str, state_str))
}

impl Db {
    /// Insert an entry using the default [`ConflictStrategy::Ignore`].
    /// Returns `Err(Duplicate)` if the content_hash collides with a live row,
    /// or if the slug PK collides with an existing row (same field at the
    /// same instant — semantically a duplicate even with different content).
    /// Preserved for call sites that pre-date the strategy API.
    pub fn insert_entry(&self, entry: &Entry) -> Result<()> {
        match self.insert_entry_with_strategy(entry, ConflictStrategy::Ignore)? {
            InsertOutcome::Inserted { .. } => Ok(()),
            InsertOutcome::Ignored { .. } => Err(DatumError::Duplicate(format!(
                "Entry already exists at slug {} (or identical content with hash {})",
                entry.slug,
                &entry.content_hash[..entry.content_hash.len().min(12)]
            ))),
            // Ignore strategy never produces the other outcomes.
            other => unreachable!("Ignore strategy produced {other:?}"),
        }
    }

    /// Rewrite just the `verb` column of the row identified by `slug`.
    /// `dm check verbs --fix` (da-5ns sub-bead 7) uses this to repair
    /// drift between the stored verb and `derive_verb`. Touches no other
    /// columns and does not write a history row — verb is a derived
    /// classifier, so a repair is not a user-visible edit. Errors
    /// [`DatumError::NotFound`] when the slug doesn't match.
    pub fn set_entry_verb(&self, slug: &str, verb: &str) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE entries SET verb = ?2 WHERE slug = ?1",
            params![slug, verb],
        )?;
        if affected == 0 {
            return Err(DatumError::NotFound(slug.to_string()));
        }
        Ok(())
    }

    /// Insert an entry and dispatch on [`ConflictStrategy`] when the incoming
    /// `content_hash` already matches a live row. Tombstoned rows are ignored
    /// for collision detection — re-inserting a deleted-then-identical payload
    /// behaves like a fresh insert.
    ///
    /// Both the entries mutation and the companion `entry_history` row land in
    /// the same transaction: `Create` is recorded for new inserts and Rekey,
    /// `UserEdit` for Update / Merge, and `Ignored` writes no history at all.
    pub fn insert_entry_with_strategy(
        &self,
        entry: &Entry,
        strategy: ConflictStrategy,
    ) -> Result<InsertOutcome> {
        let tx = self.conn.unchecked_transaction()?;
        let outcome = insert_entry_in_tx(&tx, entry, strategy)?;
        tx.commit()?;
        Ok(outcome)
    }

    /// Bulk-insert entries inside a single SQLite transaction. Uses the
    /// supplied [`ConflictStrategy`] for every row. One commit covers the
    /// whole batch — at ~100k rows this is ~50-100x faster than calling
    /// [`Self::insert_entry_with_strategy`] per row, which opens its own
    /// transaction per call.
    ///
    /// The whole batch is atomic: if any row errors (other than a
    /// duplicate-handled-as-`Ignored` outcome), the transaction is rolled
    /// back and no rows are persisted.
    pub fn insert_entries_batch(
        &self,
        entries: &[Entry],
        strategy: ConflictStrategy,
    ) -> Result<Vec<InsertOutcome>> {
        let tx = self.conn.unchecked_transaction()?;
        let mut outcomes = Vec::with_capacity(entries.len());
        for entry in entries {
            outcomes.push(insert_entry_in_tx(&tx, entry, strategy)?);
        }
        tx.commit()?;
        Ok(outcomes)
    }

    /// Bulk-load fast path for one-shot ingestion (e.g. `datum db import`).
    ///
    /// Like [`Self::insert_entries_batch`], one transaction wraps the whole
    /// batch. Unlike it, this method **skips** per-row work that's only useful
    /// for live writes from a user session:
    ///
    /// - No `entry_history` rows are written (the migration itself is the
    ///   audit event; a 100k-row import doesn't need 100k matching `Create`
    ///   history rows).
    /// - No per-row conflict detection. `conflicts::detect_for_entry` runs
    ///   time-range scans against `entries`; at 100k rows this dominates the
    ///   wall-clock. Source data for migrations is already self-consistent —
    ///   if you want belt-and-braces verification, run `datum conflicts list`
    ///   once after the import completes.
    ///
    /// Still does: content_hash dedup, slug/hid collision retry. Honors
    /// [`ConflictStrategy::Ignore`] only (the supported db-import mode).
    /// Other strategies error — the fast path is deliberately narrow.
    ///
    /// **OPT-IN.** Existing per-row callers (`switch`, `block`, sync apply)
    /// must keep using `insert_entry_with_strategy` / `insert_entries_batch`
    /// so they get full conflict detection and history.
    pub fn insert_entries_bulk(
        &self,
        entries: &[Entry],
        strategy: ConflictStrategy,
    ) -> Result<Vec<InsertOutcome>> {
        if !matches!(strategy, ConflictStrategy::Ignore) {
            return Err(DatumError::Other(format!(
                "insert_entries_bulk supports only ConflictStrategy::Ignore, got {strategy:?}",
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        let mut outcomes = Vec::with_capacity(entries.len());
        for entry in entries {
            outcomes.push(insert_entry_bulk_in_tx(&tx, entry)?);
        }
        tx.commit()?;
        Ok(outcomes)
    }

    /// Persist a mutation to an existing entry, matched by `entry.slug`.
    /// Errors [`DatumError::NotFound`] if the slug is unknown and
    /// [`DatumError::Tombstoned`] if the row is soft-deleted. Refreshes
    /// `modify_time` and rewrites all mutable fields (including
    /// `content_hash`, `data`) from the incoming payload. Records a
    /// `user_edit` history row for the pre-edit state in the same transaction.
    pub fn save_existing_entry(&self, entry: &Entry) -> Result<()> {
        self.save_existing_entry_with_kind(entry, ChangeKind::UserEdit)
    }

    /// As [`Self::save_existing_entry`] but lets the caller stamp a
    /// non-default [`ChangeKind`] on the pre-mutation history row. Used by
    /// the retime path to preserve `ChangeKind::Retime` even when the
    /// shared auto-reinterp helper is doing the persist.
    pub fn save_existing_entry_with_kind(
        &self,
        entry: &Entry,
        change_kind: ChangeKind,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let state: Option<Option<String>> = tx
            .query_row(
                "SELECT deleted_at FROM entries WHERE slug = ?1",
                params![entry.slug],
                |row| row.get(0),
            )
            .optional()?;
        match state {
            None => return Err(DatumError::NotFound(entry.slug.to_string())),
            Some(Some(_)) => return Err(DatumError::Tombstoned(entry.slug.to_string())),
            Some(None) => {
                let pre = fetch_entry_any(&tx, &entry.slug)?;
                history::record(&tx, &pre, change_kind)?;
                raw_overwrite_mutables(&tx, &entry.slug, entry, None)?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Persist `updated` after re-interpolating its `field_structure` /
    /// `id_structure` against the post-mutation state. If the interpolated
    /// slug differs from the row's existing slug (matched by `existing_slug`),
    /// the row is rekeyed in place via [`Self::save_existing_entry_with_rekey`];
    /// otherwise it goes through [`Self::save_existing_entry`].
    ///
    /// Returns the freshly-loaded entry plus the old slug iff a rekey
    /// happened. The caller is responsible for any user-facing rendering.
    ///
    /// da-5ns.4: extends the da-4ud auto-rekey trigger so column mutations
    /// (`occur_time`, `state`, `dur`, `following_state`) cascade into the
    /// templated slug just like data-blob mutations did. The mutator
    /// (insert.rs's update path, time.rs's retime path, …) computes the new
    /// column values on `updated`; this helper handles the re-interpolate +
    /// persist + maybe-rekey work in one place so each mutator doesn't
    /// reinvent the auto-rekey logic.
    pub fn save_existing_entry_with_auto_reinterp(
        &self,
        existing_slug: &Slug,
        updated: Entry,
        slug_delimiter: &str,
    ) -> Result<(Entry, Option<Slug>)> {
        self.save_existing_entry_with_auto_reinterp_kind(
            existing_slug,
            updated,
            slug_delimiter,
            ChangeKind::UserEdit,
        )
    }

    /// As [`Self::save_existing_entry_with_auto_reinterp`] but the caller
    /// picks the [`ChangeKind`] stamped on the pre-mutation history row.
    /// `retime` passes [`ChangeKind::Retime`] so a templated rekey driven by
    /// an occur_time change is still surfaced as a retime in
    /// `entry_history`.
    pub fn save_existing_entry_with_auto_reinterp_kind(
        &self,
        existing_slug: &Slug,
        mut updated: Entry,
        slug_delimiter: &str,
        change_kind: ChangeKind,
    ) -> Result<(Entry, Option<Slug>)> {
        use crate::field_spec::{self, InterpolationContext};
        if let Some(tmpl) = updated.field_structure.clone() {
            let uid = updated.short_uid();
            let ctx = InterpolationContext::from_entry(&updated, &uid);
            updated.field = field_spec::interpolate(&tmpl, &ctx, slug_delimiter);
        }
        // da-ivld: slug = `<field>:<body>` where body is
        // interpolate(id_structure). The `<field>:` prefix is implicit
        // and never lives in id_structure; see cmd.rs's slug-precedence
        // block for write-time derivation.
        let new_slug = if let Some(tmpl) = updated.id_structure.clone() {
            let uid = updated.short_uid();
            let ctx = InterpolationContext::from_entry(&updated, &uid);
            let body = field_spec::interpolate(&tmpl, &ctx, slug_delimiter);
            Some(Slug(format!("{}:{}", updated.field, body)))
        } else {
            None
        };
        updated.content_hash = updated.expected_content_hash();

        let slug_changed = matches!(&new_slug, Some(s) if s != existing_slug);

        if slug_changed {
            let target = new_slug.unwrap();
            updated.slug = target.clone();
            self.save_existing_entry_with_rekey_kind(existing_slug, &updated, change_kind)?;
            let fresh = self.get_entry(&target)?;
            Ok((fresh, Some(existing_slug.clone())))
        } else {
            // updated.slug already equals existing_slug for the in-place
            // path; save_existing_entry matches on entry.slug so make sure
            // it isn't out of sync.
            updated.slug = existing_slug.clone();
            self.save_existing_entry_with_kind(&updated, change_kind)?;
            let fresh = self.get_entry(existing_slug)?;
            Ok((fresh, None))
        }
    }

    /// Rekey an existing entry in place: rewrite mutable columns AND change
    /// `slug` / `field` / `id_structure` / `field_structure` while preserving
    /// the row's `hid`. Used by the update path's auto-reinterpolation when
    /// a templated field/slug re-derives to a different value after data
    /// changes (port of datum's `idStructure` / `fieldStructure` semantics).
    ///
    /// Looks the row up by `old_slug`; uses `entry.hid` as the identity for
    /// the actual UPDATE so the slug column can change in the same statement
    /// without losing the row. Records a `user_edit` history row for the
    /// pre-rekey state in the same transaction. Returns
    /// [`DatumError::Duplicate`] if the new slug collides with another live
    /// row.
    pub fn save_existing_entry_with_rekey(&self, old_slug: &Slug, entry: &Entry) -> Result<()> {
        self.save_existing_entry_with_rekey_kind(old_slug, entry, ChangeKind::UserEdit)
    }

    /// As [`Self::save_existing_entry_with_rekey`] but lets the caller pick
    /// the [`ChangeKind`] for the pre-rekey history row.
    pub fn save_existing_entry_with_rekey_kind(
        &self,
        old_slug: &Slug,
        entry: &Entry,
        change_kind: ChangeKind,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let pre = fetch_entry_any(&tx, old_slug)?;
        if pre.deleted_at.is_some() {
            return Err(DatumError::Tombstoned(old_slug.to_string()));
        }
        if pre.hid != entry.hid {
            return Err(DatumError::Other(format!(
                "save_existing_entry_with_rekey: hid mismatch — pre={} new={}",
                pre.hid, entry.hid,
            )));
        }
        history::record(&tx, &pre, change_kind)?;
        match raw_overwrite_full_by_hid(&tx, entry) {
            Ok(()) => {}
            Err(DatumError::Db(e)) if collision_kind(&e) == Some(CollisionKind::Slug) => {
                return Err(DatumError::Duplicate(format!(
                    "Cannot rekey {old_slug} → {}: slug already taken",
                    entry.slug,
                )));
            }
            Err(e) => return Err(e),
        }
        tx.commit()?;
        Ok(())
    }
}

/// Append a short random base36 suffix to a slug. Used by the
/// [`ConflictStrategy::Rekey`] path to derive a distinct slug for the new
/// row when the natural slug already belongs to the existing one.
fn suffixed_slug(base: &str) -> Slug {
    let mut rng = rand::thread_rng();
    let suffix: String = (0..4)
        .map(|_| {
            let n = rng.gen_range(0..36u8);
            if n < 10 {
                (b'0' + n) as char
            } else {
                (b'a' + (n - 10)) as char
            }
        })
        .collect();
    Slug(format!("{base}_{suffix}"))
}

/// Which uniqueness constraint a SQLite ConstraintViolation tripped: the
/// `entries.slug` UNIQUE index, the `entries.hid` PRIMARY KEY, or the
/// `entries_field_occurtime` partial UNIQUE index over `(field, occur_time)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CollisionKind {
    Slug,
    Hid,
    FieldOccurTime,
}

/// Inspect a rusqlite error and decide which collision it represents.
/// SQLite's constraint-violation message names the offending column or
/// index (e.g. `UNIQUE constraint failed: entries.slug` for column-level
/// indexes, or `UNIQUE constraint failed: entries.field, entries.occur_time`
/// for the multi-column partial index), which is the cheapest way to tell
/// them apart without a second query.
fn collision_kind(err: &rusqlite::Error) -> Option<CollisionKind> {
    let rusqlite::Error::SqliteFailure(e, msg) = err else {
        return None;
    };
    if e.code != rusqlite::ErrorCode::ConstraintViolation {
        return None;
    }
    let msg = msg.as_deref().unwrap_or("");
    // Order matters: the (field, occur_time) message contains "entries.field"
    // and "entries.occur_time" together — check the multi-column form first
    // so a same-instant collision is not misclassified as a slug collision.
    if msg.contains("entries.field") && msg.contains("entries.occur_time") {
        Some(CollisionKind::FieldOccurTime)
    } else if msg.contains("entries.slug") {
        Some(CollisionKind::Slug)
    } else if msg.contains("entries.hid") {
        Some(CollisionKind::Hid)
    } else {
        None
    }
}

/// Look up the `hid` of the live row that currently occupies
/// `(field, occur_time)` for diagnostic messages on a partial-unique
/// collision. Returns `None` if no live row matches (which would mean
/// the index was tripped by a race the surrounding tx already rolled
/// back — surface the diagnostic without a hid in that case).
fn fetch_field_occur_collider(
    conn: &Connection,
    field: &str,
    occur_time: &str,
) -> Result<Option<String>> {
    let hid: Option<String> = conn
        .query_row(
            "SELECT hid FROM entries \
             WHERE field = ?1 AND occur_time = ?2 AND deleted_at IS NULL \
             LIMIT 1",
            params![field, occur_time],
            |row| row.get(0),
        )
        .optional()?;
    Ok(hid)
}

/// Deep-merge `incoming` into `existing` for use by [`ConflictStrategy::Merge`].
///
/// - If either side is `None`, the other is returned.
/// - Object + object: keys are unioned; nested objects recurse; scalar
///   conflicts → incoming wins (LWW).
/// - Any non-object pair: incoming replaces existing.
fn merge_extra_data(
    existing: Option<&JsonValue>,
    incoming: Option<&JsonValue>,
) -> Option<JsonValue> {
    match (existing, incoming) {
        (None, None) => None,
        (Some(e), None) => Some(e.clone()),
        (None, Some(n)) => Some(n.clone()),
        (Some(e), Some(n)) => Some(merge_json(e.clone(), n.clone())),
    }
}

fn merge_json(existing: JsonValue, incoming: JsonValue) -> JsonValue {
    match (existing, incoming) {
        (JsonValue::Object(mut e_map), JsonValue::Object(n_map)) => {
            for (k, v) in n_map {
                match e_map.remove(&k) {
                    Some(existing_v) => {
                        e_map.insert(k, merge_json(existing_v, v));
                    }
                    None => {
                        e_map.insert(k, v);
                    }
                }
            }
            JsonValue::Object(e_map)
        }
        (_, incoming) => incoming,
    }
}

// --- Free-function mutators ---------------------------------------------------
//
// These take an explicit `&Connection` so they can run inside caller-owned
// transactions alongside `entry_history` writes. The public `Db` methods above
// wrap them with `unchecked_transaction` to keep the entries mutation and the
// history row atomic.

/// Sibling-visible re-export of [`insert_entry_in_tx`] so other db
/// submodules (notably [`super::multi`]) can compose it inside a
/// caller-owned transaction.
pub(super) fn insert_entry_in_tx_pub(
    conn: &Connection,
    entry: &Entry,
    strategy: ConflictStrategy,
) -> Result<InsertOutcome> {
    insert_entry_in_tx(conn, entry, strategy)
}

/// Per-entry insert logic shared by `insert_entry_with_strategy` (one tx per
/// call) and `insert_entries_batch` (one tx for the whole batch).
fn insert_entry_in_tx(
    conn: &Connection,
    entry: &Entry,
    strategy: ConflictStrategy,
) -> Result<InsertOutcome> {
    let existing: Option<(Slug, Option<String>)> = conn
        .query_row(
            "SELECT slug, data FROM entries
             WHERE content_hash = ?1 AND deleted_at IS NULL
             LIMIT 1",
            params![entry.content_hash],
            |row| Ok((row.get::<_, Slug>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()?;

    let outcome = match (existing, strategy) {
        (None, _) => {
            // No content_hash match — distinct content. A slug collision
            // here means the user is asserting a *different* thing at the
            // same identity (same field, same instant, distinct content),
            // which is semantically a duplicate-identity conflict. We let
            // raw_insert's `Duplicate` propagate so the caller sees an
            // error rather than silently dropping the second write — this
            // applies even under `Ignore`, which is meant to skip true
            // content duplicates, not different-content same-slug writes.
            // (The bulk-import fast path takes the opposite stance — see
            // `insert_entry_bulk_in_tx` — because backups commonly carry
            // legitimate same-instant rows that the user has already
            // accepted as historical.)
            let (stored_slug, stored_hid) = raw_insert(conn, entry)?;
            let inserted = fetch_entry_any(conn, &stored_slug)?;
            history::record(conn, &inserted, ChangeKind::Create)?;
            // D5: post-write conflict detection. Runs inside the same
            // transaction so the conflict row is atomic with the insert.
            crate::conflicts::detect_for_entry(
                conn,
                &inserted,
                crate::conflicts::DetectedBy::LocalValidator,
            )?;
            InsertOutcome::Inserted {
                slug: stored_slug,
                hid: stored_hid,
            }
        }
        (Some((slug, _)), ConflictStrategy::Ignore) => InsertOutcome::Ignored { slug },
        (Some((slug, _)), ConflictStrategy::Update) => {
            let pre = fetch_entry_any(conn, &slug)?;
            history::record(conn, &pre, ChangeKind::UserEdit)?;
            raw_overwrite_mutables(conn, &slug, entry, None)?;
            InsertOutcome::Updated { slug }
        }
        (Some((slug, existing_data_json)), ConflictStrategy::Merge) => {
            let pre = fetch_entry_any(conn, &slug)?;
            history::record(conn, &pre, ChangeKind::UserEdit)?;
            let existing_data: Option<JsonValue> = existing_data_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok());
            let merged = merge_extra_data(existing_data.as_ref(), entry.data.as_ref());
            raw_overwrite_mutables(conn, &slug, entry, Some(merged))?;
            InsertOutcome::Merged { slug }
        }
        (Some((old_slug, _)), ConflictStrategy::Rekey) => {
            // Rekey: insert the incoming row with its own slug. Both rows
            // share a content_hash — the content_hash column is not UNIQUE.
            // The new row gets its own `Create` history entry; the original
            // row is untouched.
            //
            // Default slug derives from field+timestamp; if it collides with
            // the existing row, retry with a suffixed slug. raw_insert no
            // longer auto-suffixes — that retry is a Rekey-strategy concern,
            // not a slug-collision-must-error concern.
            const REKEY_MAX_ATTEMPTS: u8 = 8;
            let mut entry_for_rekey = entry.clone();
            let mut attempts = 0u8;
            let (new_slug, new_hid) = loop {
                match raw_insert(conn, &entry_for_rekey) {
                    Ok(pair) => break pair,
                    Err(DatumError::Duplicate(_)) if attempts < REKEY_MAX_ATTEMPTS => {
                        entry_for_rekey.slug = suffixed_slug(&entry.slug);
                        attempts += 1;
                    }
                    Err(e) => return Err(e),
                }
            };
            let inserted = fetch_entry_any(conn, &new_slug)?;
            history::record(conn, &inserted, ChangeKind::Create)?;
            crate::conflicts::detect_for_entry(
                conn,
                &inserted,
                crate::conflicts::DetectedBy::LocalValidator,
            )?;
            InsertOutcome::Rekeyed {
                old_slug,
                new_slug,
                new_hid,
            }
        }
    };

    Ok(outcome)
}

/// Bulk-load fast path: dedup-by-content-hash + raw_insert only. Skips
/// `history::record` and `conflicts::detect_for_entry` — see
/// [`Db::insert_entries_bulk`] for the rationale.
///
/// Uses `prepare_cached` for the dedup SELECT so the bulk loop reuses one
/// prepared statement rather than re-parsing SQL per row. The INSERT side
/// (`try_insert`) is also cached.
fn insert_entry_bulk_in_tx(conn: &Connection, entry: &Entry) -> Result<InsertOutcome> {
    let existing_slug: Option<Slug> = {
        let mut stmt = conn.prepare_cached(
            "SELECT slug FROM entries
             WHERE content_hash = ?1 AND deleted_at IS NULL
             LIMIT 1",
        )?;
        stmt.query_row(params![entry.content_hash], |row| row.get::<_, Slug>(0))
            .optional()?
    };
    match existing_slug {
        Some(slug) => Ok(InsertOutcome::Ignored { slug }),
        None => match raw_insert(conn, entry) {
            Ok((stored_slug, stored_hid)) => Ok(InsertOutcome::Inserted {
                slug: stored_slug,
                hid: stored_hid,
            }),
            // Bulk import is always Ignore-equivalent (see
            // `Db::insert_entries_bulk` precondition). Slug collision on
            // distinct content is a same-field-same-instant duplicate from
            // the source backup — skip silently rather than abort the batch.
            Err(DatumError::Duplicate(_)) => Ok(InsertOutcome::Ignored {
                slug: entry.slug.clone(),
            }),
            Err(e) => Err(e),
        },
    }
}

/// Insert an entry. Returns the `(slug, hid)` actually stored.
///
/// Slug uniqueness exists to prevent essentially-duplicate data — two rows
/// asserting the same field at the same instant. A slug collision is a
/// semantic conflict, not a transient retryable failure, so this returns
/// [`DatumError::Duplicate`] and lets the caller decide how to surface it
/// (the `Ignore` strategy maps it to `Ignored`; live writers surface it to
/// the user; the `Rekey` strategy retries with a suffixed slug).
///
/// Hid collisions, by contrast, are astronomical (12-char base36 = ~4.7
/// quintillion values). We retry once with a fresh hid; a second collision
/// almost certainly indicates a bug or environmental problem and is
/// returned as [`DatumError::Other`].
pub(crate) fn raw_insert(conn: &Connection, entry: &Entry) -> Result<(Slug, Hid)> {
    let mut hid = entry.hid.clone();
    let mut hid_attempt = 0u8;

    loop {
        match try_insert(conn, &entry.slug, &hid, entry) {
            Ok(()) => return Ok((entry.slug.clone(), hid)),
            Err(e) => {
                let kind = match &e {
                    DatumError::Db(sql_err) => collision_kind(sql_err),
                    _ => None,
                };
                match kind {
                    Some(CollisionKind::Slug) => {
                        return Err(DatumError::Duplicate(format!(
                            "Entry already exists with slug {}",
                            entry.slug
                        )));
                    }
                    Some(CollisionKind::FieldOccurTime) => {
                        // da-ga8b: a different slug already occupies
                        // (field, occur_time) for a live row. Two
                        // state-changes at the same instant for the same
                        // field are semantically undefined, so surface a
                        // clear diagnostic naming the existing hid.
                        let occur_str = entry
                            .occur_time
                            .as_ref()
                            .map(|t| t.utc.to_rfc3339())
                            .unwrap_or_else(|| entry.create_time.to_rfc3339());
                        let collider = fetch_field_occur_collider(conn, &entry.field, &occur_str)?
                            .unwrap_or_else(|| "<unknown>".to_string());
                        return Err(DatumError::Duplicate(format!(
                            "entry collides with {collider} on field={}, occur_time={occur_str}",
                            entry.field,
                        )));
                    }
                    Some(CollisionKind::Hid) => {
                        if hid_attempt >= 1 {
                            return Err(DatumError::Other(format!(
                                "hid collision persisted after retry; second collision \
                                 on {hid} indicates a bug or RNG failure",
                            )));
                        }
                        hid = new_hid();
                        hid_attempt += 1;
                    }
                    None => return Err(e),
                }
            }
        }
    }
}

pub(crate) fn try_insert(conn: &Connection, slug: &Slug, hid: &Hid, entry: &Entry) -> Result<()> {
    // da-5ns sub-bead 1 stub: occur_time is required at the schema
    // level. The CLI parser hasn't been updated yet (sub-bead 3) so
    // call sites may still produce entries with no time set; default
    // to create_time so the insert doesn't violate NOT NULL. Sub-bead
    // 3 promotes this to a parser-level requirement.
    let occur_str = entry
        .occur_time
        .as_ref()
        .map(|t| t.utc.to_rfc3339())
        .unwrap_or_else(|| entry.create_time.to_rfc3339());
    let occur_offset = entry.occur_time.as_ref().map(|t| t.offset_minutes);
    let occur_tz = entry
        .occur_time
        .as_ref()
        .map(|t| t.timezone.as_str().to_string());
    let (verb_str, state_str) = derive_verb_for_storage(entry)?;

    let mut stmt = conn.prepare_cached(
        r#"INSERT INTO entries (
            hid, slug, content_hash, field,
            verb, state, dur, following_state, data,
            occur_time, occur_time_offset, occur_time_tz, occur_time_precision,
            dur_precision,
            lat, lon, location,
            create_time, modify_time, device_id,
            deleted_at, id_structure, field_structure,
            raw_input, alias_chain, parser_version,
            user_asserted_last_state, extra_metadata
        ) VALUES (
            ?1, ?2, ?3, ?4,
            ?5, ?6, ?7, ?8, ?9,
            ?10, ?11, ?12, ?13,
            ?14,
            ?15, ?16, ?17,
            ?18, ?19, ?20,
            ?21, ?22, ?23,
            ?24, ?25, ?26,
            ?27, ?28
        )"#,
    )?;
    stmt.execute(params![
        hid,
        slug,
        entry.content_hash,
        entry.field,
        verb_str,
        state_str,
        entry.dur.as_ref().map(crate::dur::Dur::to_storage_string),
        entry.following_state.as_ref().map(|v| v.to_string()),
        entry.data.as_ref().map(|v| v.to_string()),
        occur_str,
        occur_offset,
        occur_tz,
        entry.occur_time_precision,
        entry.dur_precision,
        entry.lat,
        entry.lon,
        entry.location.as_deref(),
        entry.create_time.to_rfc3339(),
        entry.modify_time.to_rfc3339(),
        entry.device_id,
        entry.deleted_at.map(|t| t.to_rfc3339()),
        entry.id_structure.as_deref(),
        entry.field_structure.as_deref(),
        entry.raw_input,
        entry.alias_chain.as_ref().map(|v| v.to_string()),
        entry.parser_version,
        entry
            .user_asserted_last_state
            .as_ref()
            .map(|v| v.to_string()),
        entry.extra_metadata.as_ref().map(|v| v.to_string()),
    ])?;
    Ok(())
}

/// Rewrite the `entries` row matching `snapshot.slug` to match the
/// supplied snapshot. Used by conflict resolution paths
/// (`take_local`, `take_remote`, `edit`) to apply the user's chosen
/// version. Records a `user_edit` history row before the overwrite.
///
/// Lenient: if no row matches the slug (e.g., the snapshot describes an
/// entry that was never written locally), this returns `Ok(())` without
/// touching the table. That keeps the conflict-resolution surface usable
/// even in the rare case where the queue's local/remote snapshots
/// reference a deleted or never-applied row.
pub(super) fn rewrite_entry_to_snapshot(conn: &Connection, snapshot: &Entry) -> Result<()> {
    let pre: Option<Entry> = conn
        .query_row(
            "SELECT * FROM entries WHERE slug = ?1 AND deleted_at IS NULL",
            params![snapshot.slug],
            row_to_entry,
        )
        .optional()?;
    let Some(pre) = pre else {
        return Ok(());
    };
    history::record(conn, &pre, ChangeKind::UserEdit)?;
    raw_overwrite_mutables(conn, &pre.slug, snapshot, None)?;
    Ok(())
}

/// Overwrite an existing row identified by `slug` with the mutable fields
/// from `entry`. When `override_data` is `Some`, it replaces the stored
/// `data`; otherwise the incoming `entry.data` wins. Refreshes `modify_time`
/// to now. The slug is immutable — this never renames the row.
fn raw_overwrite_mutables(
    conn: &Connection,
    slug: &str,
    entry: &Entry,
    override_data: Option<Option<JsonValue>>,
) -> Result<()> {
    let data_value: Option<&JsonValue> = match override_data.as_ref() {
        Some(o) => o.as_ref(),
        None => entry.data.as_ref(),
    };
    let data_json = data_value.map(|v| v.to_string());
    let now = Utc::now().to_rfc3339();

    let occur_str = entry
        .occur_time
        .as_ref()
        .map(|t| t.utc.to_rfc3339())
        .unwrap_or_else(|| entry.create_time.to_rfc3339());
    let occur_offset = entry.occur_time.as_ref().map(|t| t.offset_minutes);
    let occur_tz = entry
        .occur_time
        .as_ref()
        .map(|t| t.timezone.as_str().to_string());
    let (verb_str, state_str) = derive_verb_for_storage(entry)?;

    conn.execute(
        r#"UPDATE entries SET
            content_hash = ?2,
            field = ?3,
            verb = ?4,
            state = ?5,
            dur = ?6,
            following_state = ?7,
            data = ?8,
            occur_time = ?9,
            occur_time_offset = ?10,
            occur_time_tz = ?11,
            occur_time_precision = ?12,
            dur_precision = ?13,
            lat = ?14,
            lon = ?15,
            location = ?16,
            modify_time = ?17,
            device_id = ?18,
            raw_input = ?19,
            alias_chain = ?20,
            parser_version = ?21,
            user_asserted_last_state = ?22,
            id_structure = ?23,
            field_structure = ?24
           WHERE slug = ?1"#,
        params![
            slug,
            entry.content_hash,
            entry.field,
            verb_str,
            state_str,
            entry.dur.as_ref().map(crate::dur::Dur::to_storage_string),
            entry.following_state.as_ref().map(|v| v.to_string()),
            data_json,
            occur_str,
            occur_offset,
            occur_tz,
            entry.occur_time_precision,
            entry.dur_precision,
            entry.lat,
            entry.lon,
            entry.location.as_deref(),
            now,
            entry.device_id,
            entry.raw_input,
            entry.alias_chain.as_ref().map(|v| v.to_string()),
            entry.parser_version,
            entry
                .user_asserted_last_state
                .as_ref()
                .map(|v| v.to_string()),
            entry.id_structure.as_deref(),
            entry.field_structure.as_deref(),
        ],
    )?;
    Ok(())
}

/// Rewrite an existing live row identified by `entry.hid` with every mutable
/// column AND the templated identity columns (`slug`, `field`, `id_structure`,
/// `field_structure`). Refreshes `modify_time` to now. Returns whatever
/// SQLite returns for a slug UNIQUE collision so the caller can map it to
/// [`DatumError::Duplicate`].
///
/// `deleted_at` is intentionally NOT touched — a rekey of a live row keeps
/// it live; tombstoned rows must be filtered out by the caller.
fn raw_overwrite_full_by_hid(conn: &Connection, entry: &Entry) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let occur_str = entry
        .occur_time
        .as_ref()
        .map(|t| t.utc.to_rfc3339())
        .unwrap_or_else(|| entry.create_time.to_rfc3339());
    let occur_offset = entry.occur_time.as_ref().map(|t| t.offset_minutes);
    let occur_tz = entry
        .occur_time
        .as_ref()
        .map(|t| t.timezone.as_str().to_string());
    let (verb_str, state_str) = derive_verb_for_storage(entry)?;

    let mut stmt = conn.prepare_cached(
        r#"UPDATE entries SET
            slug = ?2,
            content_hash = ?3,
            field = ?4,
            verb = ?5,
            state = ?6,
            dur = ?7,
            following_state = ?8,
            data = ?9,
            occur_time = ?10,
            occur_time_offset = ?11,
            occur_time_tz = ?12,
            occur_time_precision = ?13,
            dur_precision = ?14,
            lat = ?15,
            lon = ?16,
            location = ?17,
            modify_time = ?18,
            device_id = ?19,
            id_structure = ?20,
            field_structure = ?21,
            raw_input = ?22,
            alias_chain = ?23,
            parser_version = ?24,
            user_asserted_last_state = ?25
           WHERE hid = ?1"#,
    )?;
    let affected = stmt.execute(params![
        entry.hid,
        entry.slug,
        entry.content_hash,
        entry.field,
        verb_str,
        state_str,
        entry.dur.as_ref().map(crate::dur::Dur::to_storage_string),
        entry.following_state.as_ref().map(|v| v.to_string()),
        entry.data.as_ref().map(|v| v.to_string()),
        occur_str,
        occur_offset,
        occur_tz,
        entry.occur_time_precision,
        entry.dur_precision,
        entry.lat,
        entry.lon,
        entry.location.as_deref(),
        now,
        entry.device_id,
        entry.id_structure.as_deref(),
        entry.field_structure.as_deref(),
        entry.raw_input,
        entry.alias_chain.as_ref().map(|v| v.to_string()),
        entry.parser_version,
        entry
            .user_asserted_last_state
            .as_ref()
            .map(|v| v.to_string()),
    ])?;
    if affected == 0 {
        return Err(DatumError::NotFound(entry.hid.to_string()));
    }
    Ok(())
}

/// Apply `entry` wholesale to the `entries` table (live or tombstoned) — the
/// hid/slug are treated as the identifying target, every other column takes
/// the snapshot's value. Used by `revert_entry_to` so a revert can flip the
/// `deleted_at` column back to NULL (or set it) in lockstep with the rest of
/// the row. `modify_time` is taken from the snapshot verbatim; the caller has
/// already persisted a history row capturing the pre-revert state.
pub(super) fn apply_entry_state(conn: &Connection, entry: &Entry) -> Result<()> {
    let occur_str = entry
        .occur_time
        .as_ref()
        .map(|t| t.utc.to_rfc3339())
        .unwrap_or_else(|| entry.create_time.to_rfc3339());
    let occur_offset = entry.occur_time.as_ref().map(|t| t.offset_minutes);
    let occur_tz = entry
        .occur_time
        .as_ref()
        .map(|t| t.timezone.as_str().to_string());
    let (verb_str, state_str) = derive_verb_for_storage(entry)?;

    let affected = conn.execute(
        r#"UPDATE entries SET
            content_hash = ?2,
            field = ?3,
            verb = ?4,
            state = ?5,
            dur = ?6,
            following_state = ?7,
            data = ?8,
            occur_time = ?9,
            occur_time_offset = ?10,
            occur_time_tz = ?11,
            occur_time_precision = ?12,
            dur_precision = ?13,
            lat = ?14,
            lon = ?15,
            location = ?16,
            modify_time = ?17,
            device_id = ?18,
            deleted_at = ?19,
            id_structure = ?20,
            field_structure = ?21,
            raw_input = ?22,
            alias_chain = ?23,
            parser_version = ?24,
            user_asserted_last_state = ?25,
            extra_metadata = ?26
           WHERE hid = ?1"#,
        params![
            entry.hid,
            entry.content_hash,
            entry.field,
            verb_str,
            state_str,
            entry.dur.as_ref().map(crate::dur::Dur::to_storage_string),
            entry.following_state.as_ref().map(|v| v.to_string()),
            entry.data.as_ref().map(|v| v.to_string()),
            occur_str,
            occur_offset,
            occur_tz,
            entry.occur_time_precision,
            entry.dur_precision,
            entry.lat,
            entry.lon,
            entry.location.as_deref(),
            entry.modify_time.to_rfc3339(),
            entry.device_id,
            entry.deleted_at.map(|t| t.to_rfc3339()),
            entry.id_structure.as_deref(),
            entry.field_structure.as_deref(),
            entry.raw_input,
            entry.alias_chain.as_ref().map(|v| v.to_string()),
            entry.parser_version,
            entry
                .user_asserted_last_state
                .as_ref()
                .map(|v| v.to_string()),
            entry.extra_metadata.as_ref().map(|v| v.to_string()),
        ],
    )?;
    if affected == 0 {
        return Err(DatumError::NotFound(entry.hid.to_string()));
    }
    Ok(())
}
