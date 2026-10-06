//! Entry insert / update paths and the in-transaction helpers they share.
//!
//! All write paths funnel through `insert_entry_in_tx` (live writes; runs
//! conflict detection + history) or `insert_entry_bulk_in_tx` (migration
//! fast path; skips both). The `Db::insert_*` methods just open a tx and
//! delegate.

use std::collections::HashSet;

use chrono::Utc;
use rand::Rng;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;

use crate::entry::{new_hid, Entry};
use crate::error::{DatumError, Result};
use crate::history::{self, ChangeKind};
use crate::ids::{Hid, Slug};
use crate::verb::derive_verb;

use super::lookup::fetch_entry_any;
use super::row_mapper::row_to_entry;
use super::tombstone::soft_delete_in_tx;
use super::types::{ConflictStrategy, HardReplaceCounts, InsertOutcome, ReplaceCounts, LIVE_SQL};
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
fn derive_verb_for_storage(
    entry: &Entry,
) -> Result<(String, Option<String>, Option<serde_json::Value>)> {
    // The promotable verb is the one whose own state IS the switch —
    // shape.rs's `state_is_the_switch` owns the set, so a new verb forces
    // a decision there. da-gvcen narrowed it from the whole switching
    // family: a compound verb carries its switch in `following_state`,
    // and `occur newfield` is compound only because it declares its
    // field. Promoting its leading state would log a `true` nobody typed
    // and put it in the field's list of logged states.
    let state_in = match entry.state.clone() {
        None if crate::shape::state_is_the_switch(entry.verb) => {
            Some(serde_json::Value::Bool(true))
        }
        s => s,
    };
    let (verb, canonical_state) =
        derive_verb(state_in, entry.dur.as_ref(), entry.following_state.clone())?;
    let verb_str = verb.to_string();
    let state_str = canonical_state.as_ref().map(|v| v.to_string());
    Ok((verb_str, state_str, canonical_state))
}

/// True when `incoming` is a re-log of the row that DECLARED its field —
/// the same event, differing only in that the stored row also brought the
/// field into being.
///
/// da-gvcen puts a field's declaration in its first log's own
/// `following_state`, and `following_state` is content (da-4vyw). So the
/// SECOND, identical command hashes differently from the row it is a
/// re-log of, and would reach the duplicate-IDENTITY path — the loud
/// refusal da-2fkr wrote for two different events at one instant —
/// instead of the benign no-op da-9sfm reserves exit 11 for. A
/// declaration is a fact about the FIELD, not about the event: two rows
/// that agree on the event and differ only in whether one of them also
/// declared the field are the same event.
///
/// The guard is deliberately narrow, so that a row which really does
/// switch at its trailing edge still collides loudly: the stored row must
/// be the field's FIRST-WRITTEN row (only a first log declares), its
/// trailing state must be one of the two values `track::declared_state`
/// produces ([`crate::shape::is_off_state`]), the incoming row must carry
/// none, and everything else the hash covers must already agree.
///
/// ⚠ FIRST-WRITTEN, by `create_time`, and not first-HAPPENED by
/// `occur_time`. Backfill separates them: `occur foo -t 0900` declares
/// the field, `occur foo -t 0800` then sorts earlier by occur_time
/// without being a declaration, and re-running the 0900 command would
/// stop being recognised as the re-log it is. What "only a first log
/// declares" is about is the order the rows were LOGGED in, which is the
/// clock `create_time` keeps.
///
/// Runs only where the insert is about to fail, so it costs the hot path
/// nothing.
fn is_relog_of_a_declaring_row(conn: &Connection, incoming: &Entry) -> bool {
    if incoming.following_state.is_some() {
        return false;
    }
    let Ok(stored) = fetch_entry_any(conn, &incoming.slug) else {
        return false;
    };
    if stored.deleted_at.is_some() {
        return false;
    }
    if !stored
        .following_state
        .as_ref()
        .is_some_and(crate::shape::is_off_state)
    {
        return false;
    }
    let first_written: std::result::Result<String, _> = conn.query_row(
        "SELECT slug FROM entries
         WHERE field = ?1 AND deleted_at IS NULL AND superseded_by IS NULL
         ORDER BY create_time LIMIT 1",
        params![stored.field],
        |row| row.get(0),
    );
    if first_written.ok().as_deref() != Some(stored.slug.as_str()) {
        return false;
    }
    let mut undeclared = stored.clone();
    undeclared.following_state = None;
    undeclared.expected_content_hash() == incoming.expected_content_hash()
}

/// Recompute the content_hash from the columns about to be persisted so
/// the stored value always agrees with `Entry::expected_content_hash`
/// applied to the row after load. da-r569: callers like `log_entry` mutate
/// the entry post-`Entry::new` (`Dur::moment()` for `occur`, state
/// canonicalization for switch family); without this, the precomputed
/// `entry.content_hash` is stale by the time the row reaches the bind.
///
/// `occur_str`, `canonical_state` and `data` are passed separately because
/// the write paths derive them rather than reading them off `entry`
/// (timeless rows hash an empty occur string, the switch family
/// canonicalizes `state`, and `raw_overwrite_mutables` can override
/// `data`). Everything else — including `following_state` and the
/// deleted-ness flag added in da-4vyw — comes straight from `entry`.
fn storage_content_hash(
    entry: &Entry,
    occur_str: &str,
    canonical_state: Option<&serde_json::Value>,
    data: Option<&JsonValue>,
) -> String {
    let dur_str = entry
        .dur
        .as_ref()
        .map(crate::dur::Dur::to_storage_string)
        .unwrap_or_default();
    let state_str = canonical_state.map(|v| v.to_string()).unwrap_or_default();
    let data_str = data.map(|v| v.to_string()).unwrap_or_default();
    let following_str = entry
        .following_state
        .as_ref()
        .map(|v| v.to_string())
        .unwrap_or_default();
    crate::entry::compute_hash(&crate::entry::HashInputs {
        field: &entry.field,
        occur: occur_str,
        dur: &dur_str,
        state: &state_str,
        data: &data_str,
        following_state: &following_str,
        deleted: entry.deleted_at.is_some(),
    })
}

/// The occur_time string used for content-hash computation. Mirrors
/// [`Entry::new`]'s `unwrap_or_default()`: a timeless entry (no
/// `occur_time`, e.g. a bare `add`) hashes with an empty occur string —
/// NOT `create_time`. Keeping this in lockstep with the stored value
/// (NULL for timeless) is what makes dedup-by-content-hash see two
/// timeless adds of the same content as one (da-lxyw / da-r569).
fn hash_occur_str(entry: &Entry) -> String {
    entry
        .occur_time
        .as_ref()
        .map(|t| t.utc.to_rfc3339())
        .unwrap_or_default()
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

    /// Rewrite just the `verb` column of the row identified by `hid`.
    /// `dm check verbs --fix` (da-5ns sub-bead 7) uses this to repair
    /// drift between the stored verb and `derive_verb`. Touches no other
    /// columns and does not write a history row — verb is a derived
    /// classifier, so a repair is not a user-visible edit. Keyed on hid,
    /// not slug: a keep-both pair shares its slug (da-3qvnq). Errors
    /// [`DatumError::NotFound`] when the hid doesn't match.
    pub fn set_entry_verb(&self, hid: &str, verb: &str) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE entries SET verb = ?2 WHERE hid = ?1",
            params![hid, verb],
        )?;
        if affected == 0 {
            return Err(DatumError::NotFound(hid.to_string()));
        }
        Ok(())
    }

    /// Rewrite just the `content_hash` column of the row identified by
    /// `hid`. `datum check --fix` (da-adyj) uses this to repair legacy
    /// rows whose stored hash diverges from `Entry::expected_content_hash`
    /// — typically because they were written before da-r569 fixed the
    /// canonicalization rule and never re-saved. Like [`set_entry_verb`],
    /// this is an integrity housekeeping rewrite and does not record a
    /// history row: `content_hash` is a derived dedup key, not user-
    /// editable data. Errors [`DatumError::NotFound`] when the hid
    /// doesn't match a row.
    pub fn set_entry_content_hash(&self, hid: &str, content_hash: &str) -> Result<()> {
        let affected = self.conn.execute(
            "UPDATE entries SET content_hash = ?2 WHERE hid = ?1",
            params![hid, content_hash],
        )?;
        if affected == 0 {
            return Err(DatumError::NotFound(hid.to_string()));
        }
        Ok(())
    }

    /// Insert an entry and dispatch on [`ConflictStrategy`] when the incoming
    /// `content_hash` already matches a live row — or, for Update / Merge,
    /// its slug does (da-2mn3k). Tombstoned rows are ignored for collision
    /// detection — re-inserting a deleted-then-identical payload behaves
    /// like a fresh insert.
    ///
    /// Both the entries mutation and the companion `entry_history` row land in
    /// the same transaction: `Create` is recorded for new inserts and Rekey,
    /// `UserEdit` for Update / Merge, and `Ignored` writes no history at all.
    pub fn insert_entry_with_strategy(
        &self,
        entry: &Entry,
        strategy: ConflictStrategy,
    ) -> Result<InsertOutcome> {
        self.insert_entry_declaring(entry, strategy, None)
    }

    /// [`Self::insert_entry_with_strategy`], and — when the entry lands —
    /// `declare` as its field's spec entry, in the SAME transaction
    /// (da-lc26c): a first log and the spec it writes are both-or-neither,
    /// so a failure (a full disk) leaves the database as it was.
    pub fn insert_entry_declaring(
        &self,
        entry: &Entry,
        strategy: ConflictStrategy,
        declare: Option<&crate::field_spec::FieldSpec>,
    ) -> Result<InsertOutcome> {
        let tx = self.conn.unchecked_transaction()?;
        let outcome = insert_entry_in_tx(&tx, entry, strategy)?;
        let landed = matches!(
            outcome,
            InsertOutcome::Inserted { .. } | InsertOutcome::Rekeyed { .. }
        );
        if let Some(spec) = declare.filter(|_| landed) {
            let row = super::spec::new_spec_entry(
                &spec.field,
                serde_json::to_value(spec)?,
                &entry.client_id,
            );
            insert_entry_in_tx(&tx, &row, ConflictStrategy::Ignore)?;
        }
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
    ///
    /// Conflict detection runs once, after every row is written, through
    /// [`crate::integrity::detect_for_entries`] (da-ij3x): per row it reads
    /// the field's whole history, O(N²) over a restore. So each row is
    /// checked against the whole batch, not only the rows before it.
    ///
    /// `on_row` is told how many rows are written so far, after each one —
    /// a count to show, still inside the one transaction (da-h9vj).
    pub fn insert_entries_batch(
        &self,
        entries: &[Entry],
        strategy: ConflictStrategy,
        on_row: &mut dyn FnMut(usize),
    ) -> Result<Vec<InsertOutcome>> {
        let tx = self.conn.unchecked_transaction()?;
        let mut outcomes = Vec::with_capacity(entries.len());
        let mut written = Vec::new();
        for entry in entries {
            let (outcome, row) = write_entry_in_tx(&tx, entry, strategy)?;
            outcomes.push(outcome);
            written.extend(row);
            on_row(outcomes.len());
        }
        crate::integrity::detect_for_entries(
            &tx,
            &written,
            crate::integrity::DetectedBy::LocalValidator,
        )?;
        tx.commit()?;
        Ok(outcomes)
    }

    /// Make `entries` the live set, in ONE transaction (da-5fooh): the
    /// replace behind `db restore`. Every live row whose hid `entries` lacks
    /// is tombstoned — a deletion like any other, with its history row,
    /// never a dropped row. Then each entry goes back onto the row that
    /// holds its hid, live or tombstoned, slug included; one no row holds is
    /// written as [`Self::insert_entries_batch`] writes it, `strategy` and
    /// all. With `commit` false the transaction is rolled back, so a dry
    /// run's counts are what the real run would do, by construction.
    ///
    /// The counts carry a fingerprint of the live set they were taken from.
    /// With `if_unchanged` (da-2zqad) the replace goes ahead only on that
    /// same live set, checked inside its own transaction, and is otherwise
    /// [`DatumError::Changed`] with nothing written: a yes to a dry run's
    /// counts acts on what they counted, never on a write since.
    pub fn replace_entries(
        &self,
        entries: &[Entry],
        strategy: ConflictStrategy,
        commit: bool,
        if_unchanged: Option<u64>,
        on_row: &mut dyn FnMut(usize),
    ) -> Result<ReplaceCounts> {
        let tx = self.conn.unchecked_transaction()?;
        let keep: HashSet<&str> = entries.iter().map(|e| e.hid.0.as_str()).collect();
        let (live, fingerprint) = live_fingerprint(&tx, if_unchanged)?;
        let mut counts = ReplaceCounts {
            fingerprint,
            ..Default::default()
        };
        let mut deleted = HashSet::new();
        for hid in live.into_iter().filter(|h| !keep.contains(h.0.as_str())) {
            soft_delete_in_tx(&tx, &hid.0, ChangeKind::UserEdit, None)?;
            deleted.insert(hid);
        }
        let mut written = Vec::new();
        for (n, entry) in entries.iter().enumerate() {
            let held = tx
                .query_row(
                    "SELECT * FROM entries WHERE hid = ?1",
                    params![entry.hid],
                    row_to_entry,
                )
                .optional()?;
            match held {
                Some(pre)
                    if pre.deleted_at.is_none()
                        && pre.slug == entry.slug
                        && pre.content_hash == entry.content_hash =>
                {
                    counts.unchanged += 1
                }
                Some(pre) => {
                    history::record(&tx, &pre, ChangeKind::UserEdit)?;
                    tx.execute(
                        "UPDATE entries SET deleted_at = NULL, slug = ?2 WHERE hid = ?1",
                        params![entry.hid, entry.slug],
                    )
                    .map_err(|e| match collision_kind(&e) {
                        Some(CollisionKind::Slug) => DatumError::Duplicate(format!(
                            "Cannot restore {} to {}: another entry holds that slug",
                            entry.hid, entry.slug
                        )),
                        _ => DatumError::Db(e),
                    })?;
                    raw_overwrite_mutables(&tx, &pre.hid, &entry.slug, entry, None)?;
                    super::sync::emit(&tx, Some(&pre), &pre.hid, ChangeKind::UserEdit)?;
                    written.push(fetch_entry_any(&tx, &pre.hid.0)?);
                    if pre.deleted_at.is_none() {
                        counts.changed += 1
                    } else {
                        counts.added += 1
                    }
                }
                None => {
                    let (outcome, row) = write_entry_in_tx(&tx, entry, strategy)?;
                    written.extend(row);
                    match outcome {
                        // A tombstone at the slug revives in place, keeping
                        // its hid: one this replace just deleted was live.
                        InsertOutcome::Inserted { hid, .. } if deleted.remove(&hid) => {
                            counts.changed += 1
                        }
                        InsertOutcome::Inserted { .. } | InsertOutcome::Rekeyed { .. } => {
                            counts.added += 1
                        }
                        InsertOutcome::Updated { .. } | InsertOutcome::Merged { .. } => {
                            counts.changed += 1
                        }
                        InsertOutcome::Ignored { .. } => counts.unchanged += 1,
                    }
                }
            }
            on_row(n + 1);
        }
        counts.deleted = deleted.len();
        crate::integrity::detect_for_entries(
            &tx,
            &written,
            crate::integrity::DetectedBy::LocalValidator,
        )?;
        if commit {
            tx.commit()?;
        }
        Ok(counts)
    }

    /// Make the database exactly `entries` (da-rle8g, `db restore --hard`):
    /// every entry row, tombstone and `entry_history` row is DROPPED, not
    /// tombstoned, and each entry is written back verbatim, hid and slug
    /// included, with no history. The integrity findings are derived again
    /// from what was written, and the sync pull watermark goes back to 0,
    /// so the next pull brings what the other devices pushed back in on
    /// top. One transaction; with `commit` false it is rolled back, so a
    /// dry run's counts are the real run's. `if_unchanged` pins it to the
    /// live set a dry run counted, as [`Self::replace_entries`]'s does.
    pub fn hard_replace_entries(
        &self,
        entries: &[Entry],
        commit: bool,
        if_unchanged: Option<u64>,
        on_row: &mut dyn FnMut(usize),
    ) -> Result<HardReplaceCounts> {
        let tx = self.conn.unchecked_transaction()?;
        let (live, fingerprint) = live_fingerprint(&tx, if_unchanged)?;
        let live = live.len();
        let count = |sql: &str| tx.query_row(sql, [], |row| row.get::<_, usize>(0));
        let counts = HardReplaceCounts {
            fingerprint,
            written: entries.len(),
            entries: live,
            tombstones: count("SELECT COUNT(*) FROM entries")? - live,
            history: count("SELECT COUNT(*) FROM entry_history")?,
        };
        tx.execute_batch(
            "DELETE FROM entries; DELETE FROM entry_history; DELETE FROM undo_actions;
             DELETE FROM integrity_warnings; DELETE FROM last_docs;",
        )?;
        let mut written = Vec::with_capacity(entries.len());
        for (n, entry) in entries.iter().enumerate() {
            try_insert(&tx, &entry.slug, &entry.hid, entry)?;
            written.push(fetch_entry_any(&tx, &entry.slug)?);
            on_row(n + 1);
        }
        crate::integrity::detect_for_entries(
            &tx,
            &written,
            crate::integrity::DetectedBy::LocalValidator,
        )?;
        if commit {
            tx.commit()?;
        }
        Ok(counts)
    }

    /// Bulk-load fast path for one-shot ingestion (e.g. `datum db v2import`).
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
    ///
    /// `on_row` is told the count written so far, as in
    /// [`Self::insert_entries_batch`].
    pub fn insert_entries_bulk(
        &self,
        entries: &[Entry],
        strategy: ConflictStrategy,
        on_row: &mut dyn FnMut(usize),
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
            on_row(outcomes.len());
        }
        tx.commit()?;
        Ok(outcomes)
    }

    /// Persist a mutation to an existing entry, matched by `entry.hid` (a
    /// keep-both pair shares its slug, so a slug cannot address one row).
    /// Errors [`DatumError::NotFound`] if the hid is unknown and
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
        save_existing_entry_with_kind_in_tx(&tx, entry, change_kind)?;
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
        updated: Entry,
        slug_delimiter: &str,
        change_kind: ChangeKind,
    ) -> Result<(Entry, Option<Slug>)> {
        let tx = self.conn.unchecked_transaction()?;
        let out = self.save_existing_entry_with_auto_reinterp_kind_in_tx(
            existing_slug,
            updated,
            slug_delimiter,
            change_kind,
        )?;
        tx.commit()?;
        Ok(out)
    }

    /// As [`Self::save_existing_entry_with_auto_reinterp_kind`] but runs
    /// inside a transaction the caller has already opened on `self.conn`
    /// — nothing here begins or commits. Used by the bulk retime paths
    /// (db/time.rs) to make an N-row batch all-or-nothing (da-sunq): a
    /// mid-batch failure (rekey collision, disk error) rolls back every
    /// already-shifted row instead of leaving a partial shift.
    pub(crate) fn save_existing_entry_with_auto_reinterp_kind_in_tx(
        &self,
        existing_slug: &Slug,
        mut updated: Entry,
        slug_delimiter: &str,
        change_kind: ChangeKind,
    ) -> Result<(Entry, Option<Slug>)> {
        use crate::field_spec::{self, InterpolationContext};
        if let Some(tmpl) = updated.field_structure.clone() {
            let ctx = InterpolationContext::from_entry(&updated);
            updated.field = field_spec::interpolate_field(&tmpl, &ctx, slug_delimiter)?;
        }
        // A field this save moves the entry to — a template re-derived,
        // `update field=`, `edit` — is a name being chosen; the stored one is
        // the slug's prefix, as a field holds no ':'.
        if existing_slug.as_str().split_once(':').map(|(f, _)| f) != Some(updated.field.as_str()) {
            crate::slug::validate_new_field_name(&updated.field)?;
        }
        // da-ivld: slug = `<field>:<body>` where body is
        // interpolate(id_structure). The `<field>:` prefix is implicit
        // and never lives in id_structure; see cmd.rs's slug-precedence
        // block for write-time derivation.
        let new_slug = if let Some(tmpl) = updated.id_structure.clone() {
            let ctx = InterpolationContext::from_entry(&updated);
            let body = field_spec::interpolate(&tmpl, &ctx, slug_delimiter);
            Some(Slug(format!("{}:{}", updated.field, body)))
        } else {
            None
        };
        updated.content_hash = updated.expected_content_hash();

        // da-uxgv: nothing to un-derive here. A row a sync merge set aside
        // keeps its own naturally-derived slug — the merge state lives in
        // `superseded_by`, not in this column — so re-deriving is just
        // re-deriving, on every row, with no exception to remember.
        let slug_changed = matches!(&new_slug, Some(s) if s != existing_slug);

        if slug_changed {
            let target = new_slug.unwrap();
            updated.slug = target.clone();
            save_existing_entry_with_rekey_kind_in_tx(
                &self.conn,
                existing_slug,
                &updated,
                change_kind,
            )?;
            let fresh = self.get_entry(&updated.hid.0)?;
            Ok((fresh, Some(existing_slug.clone())))
        } else {
            // updated.slug already equals existing_slug for the in-place
            // path; the row is matched by hid, but the slug is what the
            // meta-entry checks read, so make sure it isn't out of sync.
            updated.slug = existing_slug.clone();
            save_existing_entry_with_kind_in_tx(&self.conn, &updated, change_kind)?;
            let fresh = self.get_entry(&updated.hid.0)?;
            Ok((fresh, None))
        }
    }

    /// Rekey an existing entry in place: rewrite mutable columns AND change
    /// `slug` / `field` / `id_structure` / `field_structure` while preserving
    /// the row's `hid`. Used by the update path's auto-reinterpolation when
    /// a templated field/slug re-derives to a different value after data
    /// changes (port of datum's `idStructure` / `fieldStructure` semantics).
    ///
    /// Addresses the row by `entry.hid`, so the slug column can change in
    /// the same statement without losing the row, and a keep-both pair's
    /// other row on `old_slug` is never the one written. Records a
    /// `user_edit` history row for the pre-rekey state in the same
    /// transaction. Returns
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
        save_existing_entry_with_rekey_kind_in_tx(&tx, old_slug, entry, change_kind)?;
        tx.commit()?;
        Ok(())
    }
}

/// Transaction-free body of [`Db::save_existing_entry_with_kind`]: runs
/// inside whatever transaction is already open on `conn` and never
/// begins or commits one itself. Exists so bulk mutators (db/time.rs's
/// retime batch, da-sunq) can wrap N per-row saves in a single
/// all-or-nothing transaction — the same posture as
/// [`Db::insert_entries_atomic`].
fn save_existing_entry_with_kind_in_tx(
    conn: &Connection,
    entry: &Entry,
    change_kind: ChangeKind,
) -> Result<()> {
    let state: Option<Option<String>> = conn
        .query_row(
            "SELECT deleted_at FROM entries WHERE hid = ?1",
            params![entry.hid],
            |row| row.get(0),
        )
        .optional()?;
    match state {
        None => Err(DatumError::NotFound(entry.slug.to_string())),
        Some(Some(_)) => Err(DatumError::Tombstoned(entry.slug.to_string())),
        Some(None) => {
            let pre = fetch_entry_any(conn, &entry.hid.0)?;
            history::record(conn, &pre, change_kind)?;
            raw_overwrite_mutables(conn, &pre.hid, &entry.slug, entry, None)?;
            super::sync::emit(conn, Some(&pre), &pre.hid, change_kind)
        }
    }
}

/// Transaction-free body of [`Db::save_existing_entry_with_rekey_kind`];
/// see [`save_existing_entry_with_kind_in_tx`] for the rationale.
fn save_existing_entry_with_rekey_kind_in_tx(
    conn: &Connection,
    old_slug: &Slug,
    entry: &Entry,
    change_kind: ChangeKind,
) -> Result<()> {
    crate::slug::validate_slug(&entry.slug.0)?;
    let pre = fetch_entry_any(conn, &entry.hid.0)?;
    if pre.deleted_at.is_some() {
        return Err(DatumError::Tombstoned(old_slug.to_string()));
    }
    history::record(conn, &pre, change_kind)?;
    match raw_overwrite_full_by_hid(conn, entry) {
        Ok(()) => super::sync::emit(conn, Some(&pre), &pre.hid, change_kind),
        Err(DatumError::Db(e)) if collision_kind(&e) == Some(CollisionKind::Slug) => {
            Err(DatumError::Duplicate(format!(
                "Cannot rekey {old_slug} → {}: slug already taken",
                entry.slug,
            )))
        }
        Err(e) => Err(e),
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
///
/// The terms are the index predicate's, `state IS NOT NULL` included
/// (da-2fkr): a stateless row at that instant is not the collider, because
/// it is not what the index refused.
fn fetch_field_occur_collider(
    conn: &Connection,
    field: &str,
    occur_time: &str,
) -> Result<Option<String>> {
    let hid: Option<String> = conn
        .query_row(
            "SELECT hid FROM entries \
             WHERE field = ?1 AND occur_time = ?2 \
               AND deleted_at IS NULL AND superseded_by IS NULL \
               AND state IS NOT NULL \
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

/// Per-entry insert logic for `insert_entry_with_strategy` (one tx per
/// call) and the atomic multi-row insert.
fn insert_entry_in_tx(
    conn: &Connection,
    entry: &Entry,
    strategy: ConflictStrategy,
) -> Result<InsertOutcome> {
    let (outcome, written) = write_entry_in_tx(conn, entry, strategy)?;
    // D5: post-write conflict detection. Runs inside the same
    // transaction so the conflict row is atomic with the insert.
    if let Some(written) = written {
        crate::integrity::detect_for_entry(
            conn,
            &written,
            crate::integrity::DetectedBy::LocalValidator,
        )?;
    }
    Ok(outcome)
}

/// [`insert_entry_in_tx`] minus the conflict detection: returns the row it
/// created or revived, which the caller must hand to
/// `integrity::detect_for_entry` / `detect_for_entries` once it is in the
/// table (Update, Merge and Ignore create nothing and return `None`).
fn write_entry_in_tx(
    conn: &Connection,
    entry: &Entry,
    strategy: ConflictStrategy,
) -> Result<(InsertOutcome, Option<Entry>)> {
    crate::slug::validate_field_name(&entry.field)?;
    crate::slug::validate_slug(&entry.slug.0)?;
    // da-r569: canonicalize content_hash so the dedup lookup uses the same
    // value `try_insert` / `raw_overwrite_mutables` will persist. Live
    // writes mutate the entry post-`Entry::new` (`Dur::moment()` for occur,
    // state canonicalization for switch family); without this the in-memory
    // `entry.content_hash` is stale and the dedup misses a true content
    // duplicate, routing the write into a slug-UNIQUE collision rather than
    // through Update/Merge.
    let (_, _, canonical_state) = derive_verb_for_storage(entry)?;
    let dedup_hash = storage_content_hash(
        entry,
        &hash_occur_str(entry),
        canonical_state.as_ref(),
        entry.data.as_ref(),
    );

    let existing: Option<(Slug, Hid, Option<String>)> = conn
        .query_row(
            "SELECT slug, hid, data FROM entries
             WHERE content_hash = ?1 AND deleted_at IS NULL AND superseded_by IS NULL
             LIMIT 1",
            params![dedup_hash],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    // da-2mn3k: Update / Merge also resolve a SLUG collision, applying the
    // strategy to the live row that holds the slug — the original datum's
    // addDoc did so on `_id`, which v3's slug is. That is how a keyed
    // record (`--id %client_id%`) is re-logged in place.
    let existing = match existing {
        None if matches!(strategy, ConflictStrategy::Update | ConflictStrategy::Merge) => conn
            .query_row(
                "SELECT slug, hid, data FROM entries
                 WHERE slug = ?1 AND deleted_at IS NULL AND superseded_by IS NULL",
                params![entry.slug],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?,
        found => found,
    };

    let written = match (existing, strategy) {
        (None, _) => {
            // No content_hash match — distinct content. da-95ez: if the slug
            // is held *only by a tombstone*, the deleted entry no longer owns
            // its identity, so revive that row in place rather than colliding
            // (delete-then-recreate at the same slug must work). One row per
            // slug is preserved (the full slug UNIQUE stays; a partial index
            // would let a ghost and a re-occur share a slug and break
            // tombstone propagation).
            // The pre-revive tombstone is snapshotted into entry_history so
            // `undo` can still walk back to the deletion.
            if let Some(ghost) = fetch_tombstone_by_slug(conn, &entry.slug)? {
                history::record(conn, &ghost, ChangeKind::UserEdit)?;
                // Clear the tombstone first, keyed on the retained hid, then
                // overwrite the now-live row's mutable columns with the new
                // content. The hid carries over so history/undo stay linked.
                conn.execute(
                    "UPDATE entries SET deleted_at = NULL WHERE hid = ?1",
                    params![ghost.hid],
                )?;
                raw_overwrite_mutables(conn, &ghost.hid, &entry.slug, entry, None)?;
                super::sync::emit(conn, Some(&ghost), &ghost.hid, ChangeKind::UserEdit)?;
                let revived = fetch_entry_any(conn, &ghost.hid.0)?;
                let outcome = InsertOutcome::Inserted {
                    slug: revived.slug.clone(),
                    hid: revived.hid.clone(),
                };
                (outcome, Some(revived))
            } else {
                // A *live* row at this slug means the user is asserting a
                // different thing at the same identity (same field+instant,
                // distinct content) — a real duplicate-identity conflict. We
                // let raw_insert's `Duplicate` propagate so the caller sees an
                // error rather than silently dropping the second write; this
                // applies even under `Ignore`, which skips true content
                // duplicates, not different-content same-slug writes. (The
                // bulk-import fast path takes the opposite stance — see
                // `insert_entry_bulk_in_tx` — because backups commonly carry
                // legitimate same-instant rows already accepted as historical.)
                //
                // da-gvcen is the one exception: a re-log of the row that
                // declared this field is not a different assertion, it is
                // the same event typed twice — see
                // `is_relog_of_a_declaring_row`.
                if is_relog_of_a_declaring_row(conn, entry) {
                    return Ok((
                        InsertOutcome::Ignored {
                            slug: entry.slug.clone(),
                        },
                        None,
                    ));
                }
                let (stored_slug, stored_hid) = raw_insert(conn, entry)?;
                let inserted = fetch_entry_any(conn, &stored_slug)?;
                history::record(conn, &inserted, ChangeKind::Create)?;
                super::sync::emit(conn, None, &inserted.hid, ChangeKind::Create)?;
                let outcome = InsertOutcome::Inserted {
                    slug: stored_slug,
                    hid: stored_hid,
                };
                (outcome, Some(inserted))
            }
        }
        (Some((slug, _, _)), ConflictStrategy::Ignore) => (InsertOutcome::Ignored { slug }, None),
        (Some((slug, hid, _)), ConflictStrategy::Update) => {
            let pre = fetch_entry_any(conn, &hid.0)?;
            history::record(conn, &pre, ChangeKind::UserEdit)?;
            raw_overwrite_mutables(conn, &hid, &slug, entry, None)?;
            super::sync::emit(conn, Some(&pre), &pre.hid, ChangeKind::UserEdit)?;
            (InsertOutcome::Updated { slug }, None)
        }
        (Some((slug, hid, existing_data_json)), ConflictStrategy::Merge) => {
            let pre = fetch_entry_any(conn, &hid.0)?;
            history::record(conn, &pre, ChangeKind::UserEdit)?;
            let existing_data: Option<JsonValue> = existing_data_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok());
            let merged = merge_extra_data(existing_data.as_ref(), entry.data.as_ref());
            raw_overwrite_mutables(conn, &hid, &slug, entry, Some(merged))?;
            super::sync::emit(conn, Some(&pre), &pre.hid, ChangeKind::UserEdit)?;
            (InsertOutcome::Merged { slug }, None)
        }
        (Some((old_slug, _, _)), ConflictStrategy::Rekey) => {
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
            super::sync::emit(conn, None, &inserted.hid, ChangeKind::Create)?;
            let outcome = InsertOutcome::Rekeyed {
                old_slug,
                new_slug,
                new_hid,
            };
            (outcome, Some(inserted))
        }
    };

    Ok(written)
}

/// Bulk-load fast path: dedup-by-content-hash + raw_insert only. Skips
/// `history::record` and `conflicts::detect_for_entry` — see
/// [`Db::insert_entries_bulk`] for the rationale.
///
/// Uses `prepare_cached` for the dedup SELECT so the bulk loop reuses one
/// prepared statement rather than re-parsing SQL per row. The INSERT side
/// (`try_insert`) is also cached.
fn insert_entry_bulk_in_tx(conn: &Connection, entry: &Entry) -> Result<InsertOutcome> {
    crate::slug::validate_field_name(&entry.field)?;
    crate::slug::validate_slug(&entry.slug.0)?;
    // da-r569: dedup against the canonical storage hash, matching the value
    // `try_insert` will persist (see `insert_entry_in_tx` for the rationale).
    let (_, _, canonical_state) = derive_verb_for_storage(entry)?;
    let dedup_hash = storage_content_hash(
        entry,
        &hash_occur_str(entry),
        canonical_state.as_ref(),
        entry.data.as_ref(),
    );
    let existing_slug: Option<Slug> = {
        let mut stmt = conn.prepare_cached(
            "SELECT slug FROM entries
             WHERE content_hash = ?1 AND deleted_at IS NULL AND superseded_by IS NULL
             LIMIT 1",
        )?;
        stmt.query_row(params![dedup_hash], |row| row.get::<_, Slug>(0))
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
/// Fetch the tombstoned row holding `slug`, if one exists. da-95ez: the slug
/// UNIQUE is total, so at most one row (live OR tombstoned) ever holds a slug;
/// this returns it only when that single row is a tombstone, signalling the
/// insert path that the slug is free to revive.
fn fetch_tombstone_by_slug(conn: &Connection, slug: &Slug) -> Result<Option<Entry>> {
    conn.query_row(
        "SELECT * FROM entries WHERE slug = ?1 AND deleted_at IS NOT NULL LIMIT 1",
        params![slug],
        row_to_entry,
    )
    .optional()
    .map_err(DatumError::Db)
}

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
                        // da-2fkr: for a stateless point the slug is now the
                        // ONLY thing standing between two events at one
                        // instant, so the message has to name the way out.
                        // The default template is `%occur_time%`, which two
                        // transactions dated to one day both resolve to; an
                        // id template is how the user says they differ, as
                        // it was in datum v2.
                        // da-2mn3k: an id template with no time in it made
                        // this collision on purpose — the way out is a
                        // conflict flag, not another template.
                        let tmpl = entry.id_structure.as_deref().unwrap_or("%occur_time%");
                        let keyed = !crate::field_spec::names_time(tmpl);
                        let hint = if keyed {
                            "; pass --update or --merge to change that entry"
                        } else if entry.asserts_state() {
                            ""
                        } else {
                            "; give the field an id template \
                             (--id / spec --id-template) if these are different events"
                        };
                        return Err(DatumError::Duplicate(format!(
                            "Entry already exists with slug {}{hint}",
                            entry.slug
                        )));
                    }
                    Some(CollisionKind::FieldOccurTime) => {
                        // da-ga8b: a different slug already occupies
                        // (field, occur_time) for a live row. Two
                        // state-changes at the same instant for the same
                        // field are semantically undefined, so surface a
                        // clear diagnostic naming the existing hid.
                        // da-2fkr narrowed the index to state-bearing rows,
                        // so reaching here means both sides assert a state.
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
    super::spec::check_meta_entry(slug, entry, entry.data.as_ref())?;
    // da-lxyw: a timeless entry (no `occur_time` — the defining trait of
    // `add` with no explicit `-t`) stores NULL, NOT `create_time`. The
    // column is nullable (schema v29); the partial UNIQUE on
    // `(field, occur_time)` excludes NULL rows, so multiple timeless adds
    // for one field coexist. The hash uses an empty occur string to match
    // `Entry::new` / the dedup pre-check (`hash_occur_str`).
    let occur_str: Option<String> = entry.occur_time.as_ref().map(|t| t.utc.to_rfc3339());
    let occur_offset = entry.occur_time.as_ref().map(|t| t.offset_minutes);
    let occur_tz = entry.occur_time.as_ref().and_then(|t| t.timezone.clone());
    let (verb_str, state_str, canonical_state) = derive_verb_for_storage(entry)?;
    let content_hash = storage_content_hash(
        entry,
        &hash_occur_str(entry),
        canonical_state.as_ref(),
        entry.data.as_ref(),
    );

    // `superseded_by` is named only when set — by a sync apply writing a row
    // that lost an identity (da-k329q) — so an insert during a migration walk
    // still runs against a table from before the column (v35).
    macro_rules! insert_sql {
        ($col:literal, $val:literal) => {
            concat!(
                "INSERT INTO entries (
                    hid, slug, content_hash, field,
                    verb, state, dur, following_state, data,
                    occur_time, occur_time_offset, occur_time_tz, occur_time_uncertainty,
                    dur_uncertainty,
                    lat, lon, location,
                    create_time, modify_time, client_id,
                    deleted_at, id_structure, field_structure,
                    raw_input, alias_chain, parser_version,
                    user_asserted_last_state, extra_metadata",
                $col,
                ") VALUES (
                    ?1, ?2, ?3, ?4,
                    ?5, ?6, ?7, ?8, ?9,
                    ?10, ?11, ?12, ?13,
                    ?14,
                    ?15, ?16, ?17,
                    ?18, ?19, ?20,
                    ?21, ?22, ?23,
                    ?24, ?25, ?26,
                    ?27, ?28",
                $val,
                ")"
            )
        };
    }
    let superseded = entry.superseded_by.is_some();
    let mut stmt = conn.prepare_cached(if superseded {
        insert_sql!(", superseded_by", ", ?29")
    } else {
        insert_sql!("", "")
    })?;
    let all = params![
        hid,
        slug,
        content_hash,
        entry.field,
        verb_str,
        state_str,
        entry.dur.as_ref().map(crate::dur::Dur::to_storage_string),
        entry.following_state.as_ref().map(|v| v.to_string()),
        entry.data.as_ref().map(|v| v.to_string()),
        occur_str,
        occur_offset,
        occur_tz,
        entry.occur_uncertainty(),
        entry.dur_uncertainty(),
        entry.lat,
        entry.lon,
        entry.location.as_deref(),
        entry.create_time.to_rfc3339(),
        entry.modify_time.to_rfc3339(),
        entry.client_id,
        entry.deleted_at.map(|t| t.to_rfc3339()),
        entry.id_structure.as_deref(),
        entry.field_structure.as_deref(),
        // §5 #11: nullable in the type, written as empty-string when None
        // so the bind round-trips against historical NOT NULL versions of
        // the column during a migration walk (m31→m32 relaxes the
        // constraint, but earlier migrations still see the old shape).
        // The row mapper restores None on read.
        entry.raw_input.as_deref().unwrap_or(""),
        entry.alias_chain.as_ref().map(|v| v.to_string()),
        entry.parser_version,
        entry
            .user_asserted_last_state
            .as_ref()
            .map(|v| v.to_string()),
        entry.extra_metadata.as_ref().map(|v| v.to_string()),
        entry.superseded_by,
    ];
    stmt.execute(&all[..if superseded { 29 } else { 28 }])?;
    Ok(())
}

/// Rewrite the live, unsuperseded row at `snapshot.slug` (addressed by its
/// hid) to match the supplied snapshot. Used by conflict resolution paths
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
            "SELECT * FROM entries WHERE slug = ?1 \
             AND deleted_at IS NULL AND superseded_by IS NULL",
            params![snapshot.slug],
            row_to_entry,
        )
        .optional()?;
    let Some(pre) = pre else {
        return Ok(());
    };
    history::record(conn, &pre, ChangeKind::UserEdit)?;
    raw_overwrite_mutables(conn, &pre.hid, &pre.slug, snapshot, None)?;
    super::sync::emit(conn, Some(&pre), &pre.hid, ChangeKind::UserEdit)
}

/// Overwrite the row `hid` (whose slug is `slug`) with the mutable fields
/// from `entry`. By hid, as a keep-both pair shares one slug. When
/// `override_data` is `Some`, it replaces the stored `data`; otherwise the
/// incoming `entry.data` wins. Refreshes `modify_time` to now. The slug is
/// immutable — this never renames the row.
fn raw_overwrite_mutables(
    conn: &Connection,
    hid: &Hid,
    slug: &str,
    entry: &Entry,
    override_data: Option<Option<JsonValue>>,
) -> Result<()> {
    let data_value: Option<&JsonValue> = match override_data.as_ref() {
        Some(o) => o.as_ref(),
        None => entry.data.as_ref(),
    };
    super::spec::check_meta_entry(slug, entry, data_value)?;
    let data_json = data_value.map(|v| v.to_string());
    let now = Utc::now().to_rfc3339();

    // da-lxyw: preserve a timeless entry's NULL `occur_time` across edits
    // rather than backfilling `create_time`. Hash with the empty occur
    // string (`hash_occur_str`) so the rewritten content_hash matches the
    // insert path.
    let occur_str: Option<String> = entry.occur_time.as_ref().map(|t| t.utc.to_rfc3339());
    let occur_offset = entry.occur_time.as_ref().map(|t| t.offset_minutes);
    let occur_tz = entry.occur_time.as_ref().and_then(|t| t.timezone.clone());
    let (verb_str, state_str, canonical_state) = derive_verb_for_storage(entry)?;
    let content_hash = storage_content_hash(
        entry,
        &hash_occur_str(entry),
        canonical_state.as_ref(),
        data_value,
    );

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
            occur_time_uncertainty = ?12,
            dur_uncertainty = ?13,
            lat = ?14,
            lon = ?15,
            location = ?16,
            modify_time = ?17,
            client_id = ?18,
            raw_input = ?19,
            alias_chain = ?20,
            parser_version = ?21,
            user_asserted_last_state = ?22,
            id_structure = ?23,
            field_structure = ?24
           WHERE hid = ?1"#,
        params![
            hid,
            content_hash,
            entry.field,
            verb_str,
            state_str,
            entry.dur.as_ref().map(crate::dur::Dur::to_storage_string),
            entry.following_state.as_ref().map(|v| v.to_string()),
            data_json,
            occur_str,
            occur_offset,
            occur_tz,
            entry.occur_uncertainty(),
            entry.dur_uncertainty(),
            entry.lat,
            entry.lon,
            entry.location.as_deref(),
            now,
            entry.client_id,
            // §5 #11: nullable in the type, written as empty-string when None
            // so the bind round-trips against historical NOT NULL versions of
            // the column during a migration walk (m31→m32 relaxes the
            // constraint, but earlier migrations still see the old shape).
            // The row mapper restores None on read.
            entry.raw_input.as_deref().unwrap_or(""),
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
    super::spec::check_meta_entry(&entry.slug, entry, entry.data.as_ref())?;
    let now = Utc::now().to_rfc3339();
    // da-plcx4: a timeless entry keeps its NULL `occur_time`, as on the
    // insert path (da-lxyw).
    let occur_str: Option<String> = entry.occur_time.as_ref().map(|t| t.utc.to_rfc3339());
    let occur_offset = entry.occur_time.as_ref().map(|t| t.offset_minutes);
    let occur_tz = entry.occur_time.as_ref().and_then(|t| t.timezone.clone());
    let (verb_str, state_str, canonical_state) = derive_verb_for_storage(entry)?;
    let content_hash = storage_content_hash(
        entry,
        &hash_occur_str(entry),
        canonical_state.as_ref(),
        entry.data.as_ref(),
    );

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
            occur_time_uncertainty = ?13,
            dur_uncertainty = ?14,
            lat = ?15,
            lon = ?16,
            location = ?17,
            modify_time = ?18,
            client_id = ?19,
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
        content_hash,
        entry.field,
        verb_str,
        state_str,
        entry.dur.as_ref().map(crate::dur::Dur::to_storage_string),
        entry.following_state.as_ref().map(|v| v.to_string()),
        entry.data.as_ref().map(|v| v.to_string()),
        occur_str,
        occur_offset,
        occur_tz,
        entry.occur_uncertainty(),
        entry.dur_uncertainty(),
        entry.lat,
        entry.lon,
        entry.location.as_deref(),
        now,
        entry.client_id,
        entry.id_structure.as_deref(),
        entry.field_structure.as_deref(),
        // §5 #11: nullable in the type, written as empty-string when None
        // so the bind round-trips against historical NOT NULL versions of
        // the column during a migration walk (m31→m32 relaxes the
        // constraint, but earlier migrations still see the old shape).
        // The row mapper restores None on read.
        entry.raw_input.as_deref().unwrap_or(""),
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
/// hid is the identifying target, every other column (including `slug`, so a
/// revert across an auto-reinterp rekey restores the old slug; da-mryo) takes
/// the snapshot's value. Used by `revert_entry_to` so a revert can flip the
/// `deleted_at` column back to NULL (or set it) in lockstep with the rest of
/// the row. `modify_time` is taken from the snapshot verbatim; the caller has
/// already persisted a history row capturing the pre-revert state. If another
/// live row has since claimed the snapshot's slug, the UNIQUE constraint
/// fails the revert loudly rather than silently keeping the wrong slug.
pub(super) fn apply_entry_state(conn: &Connection, entry: &Entry) -> Result<()> {
    super::spec::check_meta_entry(&entry.slug, entry, entry.data.as_ref())?;
    // da-plcx4: a timeless entry keeps its NULL `occur_time`, as on the
    // insert path (da-lxyw).
    let occur_str: Option<String> = entry.occur_time.as_ref().map(|t| t.utc.to_rfc3339());
    let occur_offset = entry.occur_time.as_ref().map(|t| t.offset_minutes);
    let occur_tz = entry.occur_time.as_ref().and_then(|t| t.timezone.clone());
    let (verb_str, state_str, canonical_state) = derive_verb_for_storage(entry)?;
    let content_hash = storage_content_hash(
        entry,
        &hash_occur_str(entry),
        canonical_state.as_ref(),
        entry.data.as_ref(),
    );

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
            occur_time_uncertainty = ?12,
            dur_uncertainty = ?13,
            lat = ?14,
            lon = ?15,
            location = ?16,
            modify_time = ?17,
            client_id = ?18,
            deleted_at = ?19,
            id_structure = ?20,
            field_structure = ?21,
            raw_input = ?22,
            alias_chain = ?23,
            parser_version = ?24,
            user_asserted_last_state = ?25,
            extra_metadata = ?26,
            slug = ?27
           WHERE hid = ?1"#,
        params![
            entry.hid,
            content_hash,
            entry.field,
            verb_str,
            state_str,
            entry.dur.as_ref().map(crate::dur::Dur::to_storage_string),
            entry.following_state.as_ref().map(|v| v.to_string()),
            entry.data.as_ref().map(|v| v.to_string()),
            occur_str,
            occur_offset,
            occur_tz,
            entry.occur_uncertainty(),
            entry.dur_uncertainty(),
            entry.lat,
            entry.lon,
            entry.location.as_deref(),
            entry.modify_time.to_rfc3339(),
            entry.client_id,
            entry.deleted_at.map(|t| t.to_rfc3339()),
            entry.id_structure.as_deref(),
            entry.field_structure.as_deref(),
            // §5 #11: nullable in the type, written as empty-string when None
            // so the bind round-trips against historical NOT NULL versions of
            // the column during a migration walk (m31→m32 relaxes the
            // constraint, but earlier migrations still see the old shape).
            // The row mapper restores None on read.
            entry.raw_input.as_deref().unwrap_or(""),
            entry.alias_chain.as_ref().map(|v| v.to_string()),
            entry.parser_version,
            entry
                .user_asserted_last_state
                .as_ref()
                .map(|v| v.to_string()),
            entry.extra_metadata.as_ref().map(|v| v.to_string()),
            entry.slug,
        ],
    )?;
    if affected == 0 {
        return Err(DatumError::NotFound(entry.hid.to_string()));
    }
    Ok(())
}

/// The live set's hids, and the set (hid, slug, content hash) hashed: the
/// fingerprint a replace's counts carry. With `if_unchanged` a different
/// one is [`DatumError::Changed`] (da-2zqad).
fn live_fingerprint(conn: &Connection, if_unchanged: Option<u64>) -> Result<(Vec<Hid>, u64)> {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    let live: Vec<Hid> = conn
        .prepare(&format!(
            "SELECT hid, slug, content_hash FROM entries WHERE {LIVE_SQL} ORDER BY hid"
        ))?
        .query_map([], |row| {
            for i in 0..3 {
                hash.update(row.get::<_, String>(i)?);
                hash.update([0]);
            }
            row.get::<_, Hid>(0)
        })?
        .collect::<rusqlite::Result<_>>()?;
    let fingerprint = u64::from_be_bytes(hash.finalize()[..8].try_into().unwrap());
    if if_unchanged.is_some_and(|f| f != fingerprint) {
        return Err(DatumError::Changed);
    }
    Ok((live, fingerprint))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dur::Dur;
    use crate::entry::Verb;
    use crate::test_util::{entry_builder, fresh_db};

    /// da-r569 regression: an insert that mutates the entry post-`Entry::new`
    /// (here: stamping `Dur::moment()` after construction, the pattern
    /// `log_entry` uses for `Action::Occur`) must store a content_hash that
    /// agrees with `Entry::expected_content_hash` applied to the row after
    /// load. Otherwise `datum check` flags every fresh occur entry.
    #[test]
    fn post_construction_dur_moment_does_not_drift_content_hash() {
        let db = fresh_db();
        let mut entry = entry_builder("testhash").verb(Verb::Point).build();
        // Replicate the `log_entry` post-construction mutation. The hash
        // computed by `Entry::new` was for `dur=None`; `Dur::moment()` here
        // would silently invalidate it pre-fix.
        entry.dur = Some(Dur::moment());
        db.insert_entry(&entry).unwrap();

        let stored = db.get_entry(&entry.slug).unwrap();
        assert_eq!(
            stored.content_hash,
            stored.expected_content_hash(),
            "stored content_hash must equal expected_content_hash on load"
        );
    }

    /// da-r569 regression: the dedup path inside `insert_entry_in_tx` must
    /// also use the canonical hash, otherwise a second identical-content
    /// write under `ConflictStrategy::Update` misses the dedup match and
    /// hits the `(field, occur_time)` UNIQUE collision instead of routing
    /// through Update.
    #[test]
    fn dedup_matches_after_post_construction_mutation() {
        let db = fresh_db();
        let mut first = entry_builder("testhash").verb(Verb::Point).build();
        first.dur = Some(Dur::moment());
        db.insert_entry(&first).unwrap();

        let mut second = first.clone();
        second.hid = crate::entry::new_hid();
        let outcome = db
            .insert_entry_with_strategy(&second, ConflictStrategy::Update)
            .unwrap();
        assert!(
            matches!(outcome, InsertOutcome::Updated { .. }),
            "identical content must route through Update, got {outcome:?}"
        );
    }

    /// da-lxyw (regression): an entry with no `occur_time` (a bare `add`)
    /// must round-trip through the insert path as a genuine NULL — the
    /// stored row reads back `occur_time == None`, NOT a `create_time`
    /// backfill. The content_hash must also agree on load (the hash uses
    /// the empty occur string, same as `Entry::new`).
    #[test]
    fn timeless_entry_stores_null_occur_time() {
        let db = fresh_db();
        let entry = entry_builder("thing").verb(Verb::Point).no_time().build();
        assert!(entry.occur_time.is_none(), "fixture is timeless");
        db.insert_entry(&entry).unwrap();

        let stored = db.get_entry(&entry.slug).unwrap();
        assert!(
            stored.occur_time.is_none(),
            "timeless entry must read back occur_time=None, got {:?}",
            stored.occur_time
        );
        assert_eq!(
            stored.content_hash,
            stored.expected_content_hash(),
            "content_hash must agree on load for a timeless entry"
        );
    }

    /// da-plcx4 (regression): an undo (`apply_entry_state`) of a timeless
    /// row keeps `occur_time` NULL rather than writing `create_time` in.
    #[test]
    fn undo_of_timeless_entry_keeps_null_occur_time() {
        let db = fresh_db();
        let entry = entry_builder("thing").verb(Verb::Point).no_time().build();
        db.insert_entry(&entry).unwrap();
        let mut edited = db.get_entry(&entry.slug).unwrap();
        edited.data = Some(serde_json::json!({"a": 2}));
        db.save_existing_entry(&edited).unwrap();

        let restored = db.revert_entry_to(entry.hid.as_ref(), 1).unwrap();
        assert!(
            restored.occur_time.is_none(),
            "got {:?}",
            restored.occur_time
        );
        assert_eq!(restored.content_hash, restored.expected_content_hash());
    }

    /// da-plcx4 (regression): a rekey (`raw_overwrite_full_by_hid`) of a
    /// timeless row keeps `occur_time` NULL.
    #[test]
    fn rekey_of_timeless_entry_keeps_null_occur_time() {
        let db = fresh_db();
        let entry = entry_builder("thing").verb(Verb::Point).no_time().build();
        db.insert_entry(&entry).unwrap();
        let mut rekeyed = db.get_entry(&entry.slug).unwrap();
        rekeyed.slug = Slug("thing:other".into());
        db.save_existing_entry_with_rekey(&entry.slug, &rekeyed)
            .unwrap();

        let stored = db.get_entry("thing:other").unwrap();
        assert!(stored.occur_time.is_none(), "got {:?}", stored.occur_time);
        assert_eq!(stored.content_hash, stored.expected_content_hash());
    }

    /// da-tlk2: storage boundary rejects field names with a `:`. The slug
    /// format is `<field>:<id>` and [`crate::doc_ref`] splits on the first
    /// colon — a colon in the field name leaves the boundary ambiguous.
    #[test]
    fn insert_rejects_field_name_with_colon() {
        let db = fresh_db();
        let mut entry = entry_builder("weird:field").verb(Verb::Point).build();
        // Builder built the slug already; force the field column to keep
        // the colon so we exercise the storage check rather than the
        // slug-derivation path.
        entry.field = "weird:field".into();
        let err = db.insert_entry(&entry).unwrap_err();
        assert!(
            matches!(err, DatumError::InvalidFieldSpec(_)),
            "expected InvalidFieldSpec, got {err:?}",
        );
        assert!(err.to_string().contains("':'"), "msg = {err}");
    }

    #[test]
    fn bulk_insert_rejects_field_name_with_colon() {
        let db = fresh_db();
        let mut entry = entry_builder("weird:field").verb(Verb::Point).build();
        entry.field = "weird:field".into();
        let err = db
            .insert_entries_bulk(&[entry], ConflictStrategy::Ignore, &mut |_| {})
            .unwrap_err();
        assert!(matches!(err, DatumError::InvalidFieldSpec(_)));
    }

    /// da-ij3x: the slug lookups the insert path runs per row
    /// (`fetch_tombstone_by_slug`, `fetch_entry_any`) must SEARCH an index.
    /// `entries_slug` is partial on `superseded_by IS NULL`, so before
    /// `entries_slug_lookup` both planned as `SCAN entries` — once per
    /// restored row, quadratic over a restore.
    #[test]
    fn insert_path_slug_lookups_search_an_index() {
        let db = fresh_db();
        for sql in [
            "SELECT * FROM entries WHERE slug = ?1 AND deleted_at IS NOT NULL LIMIT 1",
            "SELECT * FROM entries WHERE slug = ?1 OR hid = ?1 LIMIT 1",
        ] {
            let mut stmt = db
                .conn()
                .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
                .unwrap();
            let plan: Vec<String> = stmt
                .query_map(["x"], |r| r.get(3))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            assert!(
                !plan.iter().any(|step| step.starts_with("SCAN entries")),
                "{sql} scans: {plan:?}"
            );
        }
    }

    /// da-ij3x: `insert_entries_batch` detects conflicts once, after the
    /// whole batch is written, not row by row against the rows before it
    /// (which read the field's history per row). The proof: an open that
    /// arrives BEFORE the earlier open it repeats is still a DoubleOpen —
    /// row by row, the later instant was written first and saw no prior.
    #[test]
    fn batch_detects_conflicts_against_the_whole_batch() {
        use chrono::TimeZone;
        let db = fresh_db();
        let open_at = |hour| {
            entry_builder("posture")
                .verb(Verb::Switch)
                .state(serde_json::json!(true))
                .time_start_utc(Utc.with_ymd_and_hms(2026, 5, 1, hour, 0, 0).unwrap())
                .build()
        };
        let (later, earlier) = (open_at(10), open_at(9));
        db.insert_entries_batch(
            &[later.clone(), earlier],
            ConflictStrategy::Ignore,
            &mut |_| {},
        )
        .unwrap();

        let filed = crate::integrity::list(db.conn(), &Default::default()).unwrap();
        assert_eq!(filed.len(), 1, "{filed:?}");
        assert_eq!(filed[0].kind, crate::integrity::IntegrityKind::DoubleOpen);
        assert_eq!(filed[0].entry_slug, later.slug);
    }
}
