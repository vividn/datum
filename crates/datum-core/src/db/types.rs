//! Public types and internal helpers shared across the db submodules.
//!
//! Type definitions for `TimeMetric`, `GroupColumn`, `ListParams`, etc. live
//! here so each submodule can pull them in from a single sibling. The internal
//! helpers (`time_expr`, `filtered_where`, `like_prefix_pattern`) are
//! `pub(super)` — visible to other db submodules but not the wider crate.

use chrono::{DateTime, Utc};

use crate::ids::{Hid, Slug};

/// Projection of a field's tracking state for the `now` view.
///
/// `ignored` is derived from the entry log (most recent state == "IGNORED")
/// and `color` is pulled from the field's [`crate::field_spec::FieldSpec`]
/// when one exists. Built by [`super::Db::get_tracked_fields`].
pub struct TrackedField {
    pub field: String,
    pub ignored: bool,
    pub color: Option<String>,
}

/// Sentinel state value representing an ignored field. Stored as the JSON
/// string `"IGNORED"` in `entries.value`. The renderer's
/// `render::characters::IGNORED_STATE` constant carries the same literal.
pub const IGNORED_STATE_VALUE: &str = "IGNORED";

/// Which timestamp to sort/filter entries by in [`super::Db::list_entries`].
///
/// `Hybrid` falls back to `modify_time` for entries without a `start_time`
/// (e.g. notes). `Occur` excludes rows lacking `start_time` entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeMetric {
    #[default]
    Hybrid,
    Occur,
    Create,
    Modify,
}

impl std::str::FromStr for TimeMetric {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hybrid" => Ok(TimeMetric::Hybrid),
            "occur" => Ok(TimeMetric::Occur),
            "create" => Ok(TimeMetric::Create),
            "modify" => Ok(TimeMetric::Modify),
            other => Err(format!("unknown time metric: {other}")),
        }
    }
}

/// Column to group on in [`super::Db::count_entries_grouped`]. Restricted to
/// columns that map cleanly to SQL identifiers so callers never pass raw
/// strings into the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupColumn {
    /// The `value` column (state for switch, quantity for occur, …).
    Value,
    /// The derived `verb` column.
    Verb,
    /// The `field` column.
    Field,
}

impl GroupColumn {
    pub fn as_sql(self) -> &'static str {
        match self {
            GroupColumn::Value => "state",
            GroupColumn::Verb => "verb",
            GroupColumn::Field => "field",
        }
    }
}

impl std::str::FromStr for GroupColumn {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "state" | "value" => Ok(GroupColumn::Value),
            "verb" => Ok(GroupColumn::Verb),
            "field" => Ok(GroupColumn::Field),
            other => Err(format!(
                "unsupported group-by key '{other}' (supported: state/value, verb, field)"
            )),
        }
    }
}

/// SQL expression for an entry's effective sort time under a given
/// [`TimeMetric`]. Centralizes the metric-aware branch used by `tail`/`head`.
pub(super) fn time_expr(metric: TimeMetric) -> &'static str {
    // da-5ns: occur_time is NOT NULL post-pivot, so the COALESCE
    // chains collapse to single-column references.
    match metric {
        TimeMetric::Hybrid | TimeMetric::Occur => "occur_time",
        TimeMetric::Create => "create_time",
        TimeMetric::Modify => "modify_time",
    }
}

/// SQL expression for an entry's "endbound" time — the canonical occurrence
/// sort key used when no explicit metric is selected. Post-pivot,
/// `occur_time` is NOT NULL so the v=19 COALESCE chain collapses.
pub(super) fn time_expr_endbound() -> &'static str {
    "occur_time"
}

/// Build the shared `WHERE` clause + parameter bindings used by
/// `count_entries_filtered`, `count_entries_grouped`, and `list_entry_ids`.
pub(super) fn filtered_where(
    field: Option<&str>,
    since: Option<DateTime<Utc>>,
) -> (String, Vec<String>) {
    let mut parts: Vec<String> = vec!["deleted_at IS NULL".to_string()];
    let mut bindings: Vec<String> = Vec::new();
    if let Some(f) = field {
        parts.push("field = ?".to_string());
        bindings.push(f.to_string());
    }
    if let Some(since_utc) = since {
        parts.push(format!("{} >= ?", time_expr_endbound()));
        bindings.push(since_utc.to_rfc3339());
    }
    (parts.join(" AND "), bindings)
}

/// Parameters for [`super::Db::list_entries`]: the unified entry query used by
/// `tail` / `head`. Results are always returned oldest-first (unless
/// `reverse` is set), so callers can treat the output as a time-ordered slice.
#[derive(Debug, Clone, Default)]
pub struct ListParams {
    /// Restrict to entries whose `field` is in this list. Empty = no filter.
    /// One name → equality; multiple → SQL `IN (?, ?, …)` with one index lookup
    /// per name. Multi-field tail interleaves rows ordered by the chosen
    /// metric.
    pub fields: Vec<String>,
    /// Which timestamp governs ordering and the bound checks.
    pub metric: TimeMetric,
    /// Inclusive lower bound on the metric timestamp.
    pub start_utc: Option<DateTime<Utc>>,
    /// Inclusive upper bound on the metric timestamp.
    pub end_utc: Option<DateTime<Utc>>,
    /// `None` = unlimited. For tail/head, bounds the returned count.
    pub limit: Option<usize>,
    /// If true, take the *first* `limit` entries in the window (head semantics).
    /// If false (default), take the *last* `limit` entries (tail semantics).
    pub head: bool,
    /// Invert the final output ordering (newest-first).
    pub reverse: bool,
    /// da-ghlx: when false (default), rows whose `extra_metadata.flag` or
    /// `extra_metadata.meta` is JSON `true` are excluded — they are
    /// review markers / out-of-band notes (da-dnnn, da-xn8j) and should
    /// not pollute the live tail / dayview / nowview surface. Set true
    /// when the caller wants the full row set (e.g. via `datum tail
    /// --show-meta`, or `datum check flags`).
    pub include_meta: bool,
}

/// Selection filter for [`super::Db::update_times`]. Every non-None field narrows
/// the match. `slug_or_hid` matches either column. `after_utc` / `before_utc`
/// bound `start_time` (inclusive); rows with null `start_time` are
/// excluded whenever a time bound is set.
#[derive(Debug, Clone, Default)]
pub struct RetimeFilter<'a> {
    pub slug_or_hid: Option<&'a str>,
    pub field: Option<&'a str>,
    pub after_utc: Option<DateTime<Utc>>,
    pub before_utc: Option<DateTime<Utc>>,
}

/// Row reported by [`super::Db::find_time_end_before_start`].
#[derive(Debug, Clone)]
pub struct TimeOrderIssue {
    pub slug: Slug,
    pub field: String,
    pub start_time: String,
    pub end_time: String,
}

/// Strategy for handling content_hash collisions on insert.
///
/// Rows are mutable: `Update` / `Merge` / `Rekey` all bump `modify_time`.
/// CRDT/sync reconciliation is deferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictStrategy {
    /// Silently skip the new row and return [`InsertOutcome::Ignored`] with
    /// the existing id. Current default.
    #[default]
    Ignore,
    /// Overwrite the existing row's mutable fields with the incoming entry's
    /// values (bumps `modify_time`).
    Update,
    /// Deep-merge incoming `data` into the existing row's `data`
    /// and otherwise behave like [`ConflictStrategy::Update`].
    Merge,
    /// Insert the incoming row anyway with its new ULID, leaving the existing
    /// row untouched. Both rows end up with the same `content_hash`.
    Rekey,
}

impl From<datum_parse::cli::ConflictStrategyArg> for ConflictStrategy {
    fn from(v: datum_parse::cli::ConflictStrategyArg) -> Self {
        match v {
            datum_parse::cli::ConflictStrategyArg::Update => ConflictStrategy::Update,
            datum_parse::cli::ConflictStrategyArg::Merge => ConflictStrategy::Merge,
            datum_parse::cli::ConflictStrategyArg::Rekey => ConflictStrategy::Rekey,
            datum_parse::cli::ConflictStrategyArg::Ignore => ConflictStrategy::Ignore,
        }
    }
}

/// Outcome reported by [`super::Db::insert_entry_with_strategy`].
///
/// All variants carry the stored slug — the human-facing key. The
/// [`InsertOutcome::Inserted`] variant also carries the actual stored `hid`,
/// which can differ from `entry.hid` if a (vanishingly rare) hid collision
/// forced a single retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    /// New row inserted (no collision).
    Inserted { slug: Slug, hid: Hid },
    /// Collision resolved by mutating the existing row in place.
    Updated { slug: Slug },
    /// Collision resolved by merging `extra_data` into the existing row.
    Merged { slug: Slug },
    /// Collision resolved by inserting a new row under a new slug. The
    /// `new_hid` reflects the actual stored hid for the freshly-inserted row.
    Rekeyed {
        old_slug: Slug,
        new_slug: Slug,
        new_hid: Hid,
    },
    /// Collision left the existing row untouched.
    Ignored { slug: Slug },
}

/// Report returned from [`super::Db::save_field_spec_with_report`] describing
/// which derived aliases from the spec were surfaced vs shadowed at the
/// time of the save. The check is best-effort: derived aliases are now
/// resolved through the in-memory [`SpecAliasIndex`] rather than written
/// to the `aliases` table, so the "skipped" set is computed by inspecting
/// the index after the spec is stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveSpecReport {
    /// Alias names from this spec that the index currently points at this
    /// field (i.e. no shadowing user/kit alias and no earlier-field
    /// collision).
    pub saved: Vec<String>,
    /// Aliases shadowed by an existing user-defined / kit alias row, or
    /// claimed by another field's spec (cross-spec state collision).
    pub skipped: Vec<AliasConflict>,
}

/// A single alias-name collision surfaced when saving a field spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasConflict {
    /// The alias name that could not be surfaced.
    pub name: String,
    /// What row / spec already owns that name.
    pub reason: AliasConflictReason,
}

/// Why a derived alias was skipped during a spec save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AliasConflictReason {
    /// A user-defined or kit-installed alias row already exists under
    /// this name (`aliases` table hit).
    UserAlias,
    /// Another field's spec already derives an alias under this name
    /// (cross-spec state collision in the [`SpecAliasIndex`]).
    OwnedByField(String),
}

/// Lazy index of derived aliases pulled out of every stored
/// [`crate::field_spec::FieldSpec`]. Populated on first access by
/// [`super::Db::spec_alias_index`] and invalidated whenever the
/// `field_specs` table is mutated. Replaces the materialized derived
/// rows that lived in the `aliases` table pre-v21 (da-8y2q.5).
#[derive(Debug, Clone, Default)]
pub struct SpecAliasIndex {
    /// Map from derived-alias name → resolved [`crate::field_spec::Alias`].
    /// Cross-spec collisions are resolved deterministically: the spec
    /// whose `field` name sorts first wins; later collisions are recorded
    /// in [`SpecAliasIndex::collisions`].
    pub entries: std::collections::HashMap<String, crate::field_spec::Alias>,
    /// `field` name → derived-alias names this spec contributed (for
    /// reverse lookup, e.g. so a spec save's report can ask "which of my
    /// aliases survived?").
    pub by_field: std::collections::HashMap<String, Vec<String>>,
    /// Cross-spec collisions: a spec wanted to register a derived alias
    /// under a name another spec already owns. The first registrant wins.
    pub collisions: Vec<SpecAliasCollision>,
}

/// One cross-spec collision recorded by [`SpecAliasIndex`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecAliasCollision {
    /// Alias name that two specs both wanted.
    pub name: String,
    /// Field whose derived alias is currently surfaced for `name`.
    pub winner: String,
    /// Field whose derived alias was shadowed.
    pub loser: String,
}

/// Max number of candidates returned in [`crate::error::DatumError::AmbiguousPrefix`].
/// A wide-enough error list to disambiguate in practice, but bounded so a
/// pathological prefix (e.g. empty) can't dump the whole table.
pub(super) const AMBIGUOUS_PREFIX_CANDIDATE_CAP: usize = 16;

/// Build a SQL LIKE pattern that matches anything starting with `prefix`
/// literally — `%`, `_`, and `\` in the user-supplied prefix are escaped so
/// they are treated as plain characters. Pair with `ESCAPE '\\'` in the query.
pub(super) fn like_prefix_pattern(prefix: &str) -> String {
    let mut out = String::with_capacity(prefix.len() + 1);
    for ch in prefix.chars() {
        if ch == '\\' || ch == '%' || ch == '_' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('%');
    out
}
