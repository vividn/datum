//! QuickId resolution — generalized `<pattern> → Entry` lookup.
//!
//! The original datum CLI accepted a handful of short-forms wherever a
//! user needed to name an entry: a short hid prefix, a full slug, and the
//! underscore shorthands. This module brings that surface to the Rust
//! port, plus slug wildcards for batch commands.
//!
//! ## Supported patterns
//!
//! | Pattern             | Meaning                                     |
//! |---------------------|---------------------------------------------|
//! | `ab3d…` (any hid)   | Exact hid match (full or unique prefix).    |
//! | `mood:2024-…`       | Exact slug match (full or unique prefix).   |
//! | `_`                 | Most recently-touched live entry globally.  |
//! | `__` … `_____…`     | 2nd / 3rd / Nth most-recent (count of `_`). |
//! | `_4`, `_10`, …      | Nth most-recent (single `_` + decimal).     |
//! | `<field>:_`         | Most recent live entry for `<field>`.       |
//! | `<field>:__`, `_4`  | Field-scoped Nth most-recent.               |
//! | `book:gol*`         | Slug wildcard. `*` → any chars, `?` → one.  |
//!
//! Exact hid/slug matches always beat every other rule, so typing a full
//! slug never gets "stolen" by a wildcard or prefix that would otherwise
//! match it.
//!
//! ## Resolver variants
//!
//! - [`resolve_quickid`] returns every match (used by batch commands
//!   landing in S3).
//! - [`resolve_quickid_single`] enforces exactly-one-match, returning
//!   [`DatumError::AmbiguousPrefix`] with candidate slugs/hids when the
//!   pattern is ambiguous and [`DatumError::NotFound`] when nothing
//!   matches.
//!
//! Both variants only consider live (non-tombstoned) entries. Tombstone
//! fallback for commands like `undo` / `history` stays in the command
//! layer's wrapper.

use std::collections::HashMap;

use chrono::Utc;
use rusqlite::{params, OptionalExtension};

use crate::db::{row_to_entry_public, Db};
use crate::entry::Entry;
use crate::error::{DatumError, PrefixCandidate, Result};
use crate::ids::Hid;
use crate::last_docs::{get_last_docs, within_protection_window, PROTECTION_WINDOW_MINUTES};

/// `_LAST` — resolves to whatever hids the last-accessed pointer holds,
/// no time guard.
pub const LAST_TOKEN: &str = "_LAST";
/// `_LAST_WITH_PROTECTION` — same as `_LAST`, but refuses if the
/// pointer is older than [`PROTECTION_WINDOW_MINUTES`].
pub const LAST_WITH_PROTECTION_TOKEN: &str = "_LAST_WITH_PROTECTION";

/// Maximum number of candidates surfaced on an ambiguous resolution.
const AMBIGUOUS_CANDIDATE_CAP: usize = 16;

/// How deep [`quick_ordinals`] counts. Two digits is where the shorthand
/// stops being something anyone reaches for — past `_99` the hid sitting
/// in the next column is the handle you'd actually type — so the display
/// surfaces stop offering a short form there.
pub const QUICK_ORDINAL_CAP: usize = 99;

/// The order `_`, `__`, `_4`, … count in: most recent first. Shared by the
/// resolvers below and by [`quick_ordinals`], so what `tail` displays and
/// what `_N` resolves to can't drift apart.
const NTH_ORDER: &str = "ORDER BY occur_time DESC, slug DESC";

/// Resolve a quickId pattern to zero or more live entries.
///
/// Precedence:
/// 1. Underscore shorthands (`_`, `__`, `_4`, and the `<field>:` variants).
/// 2. Exact slug or hid match.
/// 3. Slug wildcard expansion (when the pattern contains `*` or `?`).
/// 4. Hid/slug prefix match (the pre-existing C3 behaviour).
///
/// An empty vector is returned when the pattern is syntactically valid
/// but has no matches. Callers that need a scalar result should use
/// [`resolve_quickid_single`] instead — it converts the 0/>1 cases into
/// the appropriate [`DatumError`] variants.
pub fn resolve_quickid(db: &Db, pattern: &str) -> Result<Vec<Entry>> {
    // `_LAST` / `_LAST_WITH_PROTECTION` — expand to the last-accessed
    // pointer's hids. Tombstoned hids are silently dropped so the
    // resolver stays live-only; a follow-up `undo` can target them by
    // explicit hid/slug if needed.
    if pattern == LAST_TOKEN || pattern == LAST_WITH_PROTECTION_TOKEN {
        let Some(reference) = get_last_docs(db)? else {
            return Err(DatumError::NoLastDocsRef);
        };
        if pattern == LAST_WITH_PROTECTION_TOKEN
            && !within_protection_window(reference.time, Utc::now())
        {
            return Err(DatumError::LastDocsTooOld {
                minutes: PROTECTION_WINDOW_MINUTES,
            });
        }
        let mut out = Vec::with_capacity(reference.hids.len());
        for hid in &reference.hids {
            if let Some(entry) = exact_live_match(db, hid)? {
                out.push(entry);
            }
        }
        return Ok(out);
    }

    // `<field>:_+` / `<field>:_<digits>` — Nth most recent entry within
    // a single field. Checked before the bare-underscore path so a
    // pattern with a colon never falls into the global-Nth branch.
    if let Some(idx) = pattern.find(':') {
        let field = &pattern[..idx];
        let suffix = &pattern[idx + 1..];
        if !field.is_empty() {
            if let Some(ordinal) = parse_underscore_ordinal(suffix) {
                return Ok(nth_live_entry_for_field(db, field, ordinal)?
                    .into_iter()
                    .collect());
            }
        }
    }

    // Bare `_+` / `_<digits>` — Nth most recent entry across every field.
    // `_LAST` already handled above so it never reaches here.
    if let Some(ordinal) = parse_underscore_ordinal(pattern) {
        return Ok(nth_live_entry(db, ordinal)?.into_iter().collect());
    }

    // Exact match always wins. Using the live-only `get_entry` here keeps
    // tombstoned rows out of the resolution surface by design.
    if let Some(entry) = exact_live_match(db, pattern)? {
        return Ok(vec![entry]);
    }

    // Slug wildcard expansion.
    if contains_wildcard(pattern) {
        return wildcard_match(db, pattern);
    }

    // Fallback: the existing C3 prefix matcher. `resolve_prefix` returns
    // `NotFound` on zero matches and `AmbiguousPrefix` on many; we keep
    // its behaviour on the many-case but convert `NotFound` to an empty
    // vector so callers can layer their own fallbacks.
    match db.resolve_prefix(pattern) {
        Ok(entry) => Ok(vec![entry]),
        Err(DatumError::NotFound(_)) => Ok(vec![]),
        Err(other) => Err(other),
    }
}

/// Resolve a quickId pattern to every live match, expanding an ambiguous
/// prefix instead of erroring.
///
/// Behaves exactly like [`resolve_quickid`] except for the prefix-fallback
/// case: where [`resolve_quickid`] surfaces [`DatumError::AmbiguousPrefix`]
/// when a bare prefix matches many entries, this returns all of them. Used
/// by the read-only `get` command, which renders every match rather than
/// asking the user to type a discriminating prefix. Mutating commands keep
/// [`resolve_quickid`]'s fail-on-ambiguous behaviour as their
/// disambiguation UX.
pub fn resolve_quickid_all(db: &Db, pattern: &str) -> Result<Vec<Entry>> {
    match resolve_quickid(db, pattern) {
        // AmbiguousPrefix can only originate from the prefix fallback, whose
        // `prefix` field equals `pattern`; re-running the all-matches query
        // reproduces exactly that match set.
        Err(DatumError::AmbiguousPrefix { prefix, .. }) => db.resolve_prefix_all(&prefix),
        other => other,
    }
}

/// Resolve a quickId pattern to exactly one live entry.
///
/// Errors:
/// - [`DatumError::NotFound`] when no match is found.
/// - [`DatumError::AmbiguousPrefix`] when the pattern matches multiple
///   entries. The candidates list is capped at
///   [`AMBIGUOUS_CANDIDATE_CAP`].
pub fn resolve_quickid_single(db: &Db, pattern: &str) -> Result<Entry> {
    let mut matches = resolve_quickid(db, pattern)?;
    match matches.len() {
        0 => Err(DatumError::NotFound(pattern.to_string())),
        1 => Ok(matches.pop().unwrap()),
        _ => {
            let candidates = matches
                .iter()
                .take(AMBIGUOUS_CANDIDATE_CAP)
                .map(|e| PrefixCandidate {
                    slug: e.slug.clone(),
                    hid: e.hid.clone(),
                })
                .collect();
            Err(DatumError::AmbiguousPrefix {
                prefix: pattern.to_string(),
                candidates,
            })
        }
    }
}

fn exact_live_match(db: &Db, pattern: &str) -> Result<Option<Entry>> {
    db.conn()
        .query_row(
            "SELECT * FROM entries
             WHERE (slug = ?1 OR hid = ?1) AND deleted_at IS NULL
             LIMIT 1",
            params![pattern],
            row_to_entry_public,
        )
        .optional()
        .map_err(DatumError::Db)
}

/// Parse an underscore-shorthand ordinal (1-indexed).
///
/// Accepted forms:
/// - All underscores (`_`, `__`, `___`, …): ordinal = number of `_`.
/// - Single leading underscore + decimal digits (`_4`, `_10`, …):
///   ordinal = parsed integer (must be ≥ 1).
///
/// Anything else (including `_LAST`, `__5`, the empty string, or
/// patterns missing a leading underscore) returns `None`.
///
/// `pub` because the switch-state back-resolver (`_` / `__` / `_n` as a
/// previous-state selector, da-state-back) reuses the same ordinal
/// grammar — one parse, two id-spaces (entries and states).
pub fn parse_underscore_ordinal(s: &str) -> Option<usize> {
    if !s.starts_with('_') {
        return None;
    }
    if s.bytes().all(|b| b == b'_') {
        return Some(s.len());
    }
    let rest = &s[1..];
    if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
        let n: usize = rest.parse().ok()?;
        if n >= 1 {
            return Some(n);
        }
    }
    None
}

fn nth_live_entry(db: &Db, ordinal: usize) -> Result<Option<Entry>> {
    let offset = ordinal.saturating_sub(1) as i64;
    db.conn()
        .query_row(
            &format!(
                "SELECT * FROM entries WHERE deleted_at IS NULL {NTH_ORDER} LIMIT 1 OFFSET ?1"
            ),
            params![offset],
            row_to_entry_public,
        )
        .optional()
        .map_err(DatumError::Db)
}

fn nth_live_entry_for_field(db: &Db, field: &str, ordinal: usize) -> Result<Option<Entry>> {
    let offset = ordinal.saturating_sub(1) as i64;
    db.conn()
        .query_row(
            &format!(
                "SELECT * FROM entries WHERE field = ?1 AND deleted_at IS NULL \
                 {NTH_ORDER} LIMIT 1 OFFSET ?2"
            ),
            params![field, offset],
            row_to_entry_public,
        )
        .optional()
        .map_err(DatumError::Db)
}

/// Hid → `_N` ordinal for the [`QUICK_ORDINAL_CAP`] most recent live
/// entries.
///
/// The `_N` shorthands resolve but were invisible: nothing told the user
/// that the row in front of them is the one `_3` names. The listing
/// surfaces (`tail`, `head`, `grep`) call this once per batch and label
/// the rows they recognise; a row outside the map has no short form worth
/// showing and is referenced by its hid.
///
/// One query, same `ORDER BY` as [`nth_live_entry`], so a printed `_3` is
/// the row `_3` resolves to — ordinals are global and count every live
/// entry, including the ones the caller's own filters hide, so a filtered
/// listing shows gaps rather than a renumbered 1..n that wouldn't resolve.
///
/// The `entries_occur_time` partial index covers this exact ordering, so
/// the scan stops after [`QUICK_ORDINAL_CAP`] rows.
pub fn quick_ordinals(db: &Db) -> Result<HashMap<Hid, usize>> {
    let mut stmt = db.conn().prepare(&format!(
        "SELECT hid FROM entries WHERE deleted_at IS NULL {NTH_ORDER} LIMIT {QUICK_ORDINAL_CAP}"
    ))?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut out = HashMap::new();
    for (i, hid) in rows.enumerate() {
        out.insert(Hid(hid.map_err(DatumError::Db)?), i + 1);
    }
    Ok(out)
}

/// Does the pattern contain a wildcard (`*` or `?`)?
pub fn contains_wildcard(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?')
}

/// Translate a user-supplied glob into a SQL `LIKE` pattern.
///
/// - `*` → `%` (zero or more characters)
/// - `?` → `_` (exactly one character)
/// - Literal `%`, `_`, and `\` in the input are escaped with `\`.
///
/// Pair the returned pattern with `ESCAPE '\\'` in the query.
fn glob_to_like(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len() + 1);
    for ch in pattern.chars() {
        match ch {
            '*' => out.push('%'),
            '?' => out.push('_'),
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(ch);
            }
            other => out.push(other),
        }
    }
    out
}

fn wildcard_match(db: &Db, pattern: &str) -> Result<Vec<Entry>> {
    let like = glob_to_like(pattern);
    let mut stmt = db.conn().prepare(
        "SELECT * FROM entries
         WHERE (slug LIKE ?1 ESCAPE '\\' OR hid LIKE ?1 ESCAPE '\\')
           AND deleted_at IS NULL
         ORDER BY occur_time ASC,
                  slug ASC",
    )?;
    let rows = stmt.query_map(params![like], row_to_entry_public)?;
    rows.map(|r| r.map_err(DatumError::Db)).collect()
}

#[cfg(test)]
mod tests {
    //! End-to-end tests for the underscore-shorthand quickId family.
    //!
    //! Every resolution test below runs the real SQL query against an
    //! in-memory v=CURRENT SQLite store. That's deliberate — the
    //! original bug (da-luc8) was a column-rename that mocks would have
    //! masked. Hitting the migrated schema is the only way the
    //! `start_time → occur_time` class of breakage surfaces.
    use super::*;
    use crate::test_util::{entry_builder, fresh_db};
    use chrono::{DateTime, TimeZone, Utc};

    fn at(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, min, 0)
            .unwrap()
    }

    /// Seed `n` entries on `field` at strictly-increasing `occur_time`s
    /// (one per minute starting 12:00). Returns them oldest-first, so
    /// `out.last()` is the newest.
    fn seed(db: &Db, field: &str, n: u32) -> Vec<Entry> {
        (0..n)
            .map(|i| {
                entry_builder(field)
                    .time_start_utc(at(2026, 5, 1, 12, i))
                    .insert(db)
            })
            .collect()
    }

    // ─── parse_underscore_ordinal: pure unit coverage ────────────────────

    #[test]
    fn ordinal_pure_underscores() {
        assert_eq!(parse_underscore_ordinal("_"), Some(1));
        assert_eq!(parse_underscore_ordinal("__"), Some(2));
        assert_eq!(parse_underscore_ordinal("___"), Some(3));
        assert_eq!(parse_underscore_ordinal("____"), Some(4));
        assert_eq!(parse_underscore_ordinal("__________"), Some(10));
    }

    #[test]
    fn ordinal_numeric_form() {
        assert_eq!(parse_underscore_ordinal("_1"), Some(1));
        assert_eq!(parse_underscore_ordinal("_2"), Some(2));
        assert_eq!(parse_underscore_ordinal("_4"), Some(4));
        assert_eq!(parse_underscore_ordinal("_10"), Some(10));
        assert_eq!(parse_underscore_ordinal("_25"), Some(25));
    }

    #[test]
    fn ordinal_rejects_non_underscore_patterns() {
        assert_eq!(parse_underscore_ordinal(""), None);
        assert_eq!(parse_underscore_ordinal("foo"), None);
        assert_eq!(parse_underscore_ordinal("4"), None);
        assert_eq!(parse_underscore_ordinal("a_"), None);
    }

    #[test]
    fn ordinal_rejects_last_tokens() {
        // _LAST and friends are reserved — they must NOT match the
        // generic shorthand parser, otherwise the resolver's earlier
        // _LAST branch would never see them.
        assert_eq!(parse_underscore_ordinal("_LAST"), None);
        assert_eq!(parse_underscore_ordinal("_LAST_WITH_PROTECTION"), None);
    }

    #[test]
    fn ordinal_rejects_zero_and_mixed_forms() {
        // Ordinals are 1-indexed; "0th most-recent" is meaningless.
        assert_eq!(parse_underscore_ordinal("_0"), None);
        // The numeric form is single-underscore + digits. Multi-underscore
        // mixed with digits is undefined and falls through to the prefix
        // matcher rather than guessing an ordinal.
        assert_eq!(parse_underscore_ordinal("__1"), None);
        assert_eq!(parse_underscore_ordinal("___2"), None);
        // Trailing non-digits aren't accepted either.
        assert_eq!(parse_underscore_ordinal("_1a"), None);
        assert_eq!(parse_underscore_ordinal("_4 "), None);
    }

    // ─── Bare `_` and `_LAST` regression: the column-rename bug ──────────

    #[test]
    fn bare_underscore_returns_most_recent_against_v3_schema() {
        // The original da-luc8 reproducer: `_` SELECT'd
        // `COALESCE(start_time, end_time, create_time)`, which blew up
        // with "no such column: start_time" once the v3 pivot landed.
        // This test must exercise the real SQL — that's the only way
        // a future column rename surfaces here instead of in production.
        let db = fresh_db();
        let entries = seed(&db, "mood", 5);
        let got = resolve_quickid_single(&db, "_").expect("_ must resolve");
        assert_eq!(got.slug, entries[4].slug);
    }

    #[test]
    fn field_scoped_underscore_returns_most_recent_in_field() {
        // The mirror reproducer for `<field>:_` — same column-rename bug
        // lived in `last_live_entry_for_field`.
        let db = fresh_db();
        let _other = seed(&db, "sleep", 3);
        let mood = seed(&db, "mood", 4);
        let got = resolve_quickid_single(&db, "mood:_").expect("mood:_ must resolve");
        assert_eq!(got.slug, mood[3].slug);
        assert_eq!(got.field, "mood");
    }

    #[test]
    fn bare_underscore_empty_store_is_not_found() {
        let db = fresh_db();
        let err = resolve_quickid_single(&db, "_").unwrap_err();
        assert!(matches!(err, DatumError::NotFound(_)));
    }

    #[test]
    fn field_underscore_empty_store_is_not_found() {
        let db = fresh_db();
        let err = resolve_quickid_single(&db, "mood:_").unwrap_err();
        assert!(matches!(err, DatumError::NotFound(_)));
    }

    // ─── `_N` family: bare ───────────────────────────────────────────────

    #[test]
    fn bare_double_underscore_resolves_second_most_recent() {
        let db = fresh_db();
        let entries = seed(&db, "mood", 5);
        let got = resolve_quickid_single(&db, "__").unwrap();
        assert_eq!(got.slug, entries[3].slug);
    }

    #[test]
    fn bare_triple_underscore_resolves_third_most_recent() {
        let db = fresh_db();
        let entries = seed(&db, "mood", 5);
        let got = resolve_quickid_single(&db, "___").unwrap();
        assert_eq!(got.slug, entries[2].slug);
    }

    #[test]
    fn bare_underscore_chain_overflows_to_not_found() {
        let db = fresh_db();
        let _ = seed(&db, "mood", 3);
        // Four underscores, three entries — out of range.
        let err = resolve_quickid_single(&db, "____").unwrap_err();
        assert!(matches!(err, DatumError::NotFound(_)));
    }

    #[test]
    fn bare_numeric_form_resolves_nth() {
        let db = fresh_db();
        let entries = seed(&db, "mood", 5);
        let got4 = resolve_quickid_single(&db, "_4").unwrap();
        assert_eq!(got4.slug, entries[1].slug);
        let got5 = resolve_quickid_single(&db, "_5").unwrap();
        assert_eq!(got5.slug, entries[0].slug);
    }

    #[test]
    fn bare_numeric_form_overflows_to_not_found() {
        let db = fresh_db();
        let _ = seed(&db, "mood", 5);
        let err = resolve_quickid_single(&db, "_6").unwrap_err();
        assert!(matches!(err, DatumError::NotFound(_)));
        // Far-out-of-range still NotFound, not an unrelated SQL error.
        let err2 = resolve_quickid_single(&db, "_9999").unwrap_err();
        assert!(matches!(err2, DatumError::NotFound(_)));
    }

    #[test]
    fn numeric_and_count_forms_are_equivalent() {
        // Boundary case from the spec table: `_1` ≡ `_`, `_2` ≡ `__`.
        let db = fresh_db();
        let entries = seed(&db, "mood", 5);
        let bare = resolve_quickid_single(&db, "_").unwrap();
        let n1 = resolve_quickid_single(&db, "_1").unwrap();
        assert_eq!(bare.slug, n1.slug);

        let dbl = resolve_quickid_single(&db, "__").unwrap();
        let n2 = resolve_quickid_single(&db, "_2").unwrap();
        assert_eq!(dbl.slug, n2.slug);
        assert_eq!(n2.slug, entries[3].slug);
    }

    // ─── `_N` family: field-scoped ───────────────────────────────────────

    #[test]
    fn field_scoped_double_underscore_ignores_other_fields() {
        let db = fresh_db();
        let f_entries = seed(&db, "f", 3);
        let _g_entries = seed(&db, "g", 4);
        let got = resolve_quickid_single(&db, "f:__").unwrap();
        assert_eq!(got.field, "f");
        assert_eq!(got.slug, f_entries[1].slug);
    }

    #[test]
    fn field_scoped_top_resolves_only_that_field() {
        // Even though `g` has more (and newer) entries, `f:_` must
        // return the newest in `f`.
        let db = fresh_db();
        let f_entries = seed(&db, "f", 3);
        let _g_entries = seed(&db, "g", 4);
        let got = resolve_quickid_single(&db, "f:_").unwrap();
        assert_eq!(got.field, "f");
        assert_eq!(got.slug, f_entries[2].slug);
    }

    #[test]
    fn field_scoped_numeric_overflow_is_not_found() {
        let db = fresh_db();
        let _ = seed(&db, "f", 3);
        let err = resolve_quickid_single(&db, "f:_4").unwrap_err();
        assert!(matches!(err, DatumError::NotFound(_)));
    }

    #[test]
    fn field_scoped_with_no_entries_is_not_found() {
        // `f` has no entries; `g` has plenty. `f:___` must NOT spill
        // into `g`'s entries.
        let db = fresh_db();
        let _ = seed(&db, "g", 5);
        let err = resolve_quickid_single(&db, "f:___").unwrap_err();
        assert!(matches!(err, DatumError::NotFound(_)));
    }

    // ─── Tombstone fallthrough ───────────────────────────────────────────

    #[test]
    fn tombstoned_newest_is_skipped_for_bare_underscore() {
        // After the newest is tombstoned, `_` must return the next
        // live one — the resolver is live-only by contract.
        let db = fresh_db();
        let entries = seed(&db, "mood", 3);
        db.soft_delete_entry(&entries[2].slug)
            .expect("soft_delete newest");
        let got = resolve_quickid_single(&db, "_").unwrap();
        assert_eq!(got.slug, entries[1].slug);
    }

    #[test]
    fn tombstoned_newest_is_skipped_for_field_scoped() {
        let db = fresh_db();
        let f_entries = seed(&db, "f", 3);
        db.soft_delete_entry(&f_entries[2].slug)
            .expect("soft_delete newest in f");
        let got = resolve_quickid_single(&db, "f:_").unwrap();
        assert_eq!(got.slug, f_entries[1].slug);
    }

    // ─── _LAST / _LAST_WITH_PROTECTION still work ────────────────────────

    #[test]
    fn last_token_is_unchanged_by_n_family() {
        // Regression guard: introducing the `_+` parser must not
        // shadow `_LAST` (it has no last-docs ref so we expect that
        // specific error, not an underscore-shorthand miss).
        let db = fresh_db();
        let _ = seed(&db, "mood", 2);
        let err = resolve_quickid(&db, "_LAST").unwrap_err();
        assert!(
            matches!(err, DatumError::NoLastDocsRef),
            "_LAST should hit the no-last-docs-ref path, got: {err:?}"
        );
    }

    // ─── Cold ports from the pre-v3 #[cfg(any())] module (da-gkmc) ──────
    //
    // These tests exercise exact / prefix / wildcard precedence against a
    // real v3-schema SQLite store. They were gated off when the `entries`
    // table pivoted to (occur_time, state, dur, following_state) and never
    // re-enabled — exactly the gap that let the da-luc8 column-rename bug
    // ship. Underscore-family ports (`_`, `<field>:_`, etc.) live above;
    // those four were dropped here because da-luc8 already covers them.
    //
    // Strategy: use entry_builder for time stamping + content_hash
    // distinctness, then mutate slug/hid to the controlled values the
    // assertions reference.

    /// Insert an entry with caller-controlled `slug` and `hid`. The
    /// builder picks unique default times so back-to-back fixtures land
    /// on distinct content_hashes and survive the live insert path.
    fn controlled(db: &Db, slug: &str, hid: &str, field: &str) -> Entry {
        use crate::entry::HID_LEN;
        use crate::ids::{Hid, Slug};
        assert_eq!(hid.len(), HID_LEN, "test hid must be {HID_LEN} chars");
        let mut e = entry_builder(field).build();
        e.slug = Slug::from(slug);
        e.hid = Hid::from(hid);
        db.insert_entry(&e)
            .expect("controlled fixture insert failed");
        e
    }

    // ── Exact id/slug ────────────────────────────────────────────────────

    #[test]
    fn exact_hid_resolves_to_its_entry() {
        let db = fresh_db();
        let e = controlled(&db, "coffee:0001", "abcdefghijkl", "coffee");
        controlled(&db, "tea:xxxx", "mnopqrstuvwx", "tea");

        let got = resolve_quickid_single(&db, e.hid.as_str()).unwrap();
        assert_eq!(got.slug, e.slug);
    }

    #[test]
    fn exact_slug_resolves_to_its_entry() {
        let db = fresh_db();
        let e = controlled(&db, "coffee:2024-06-01", "abcdefghijkl", "coffee");
        controlled(&db, "tea:xxxx", "mnopqrstuvwx", "tea");

        let got = resolve_quickid_single(&db, e.slug.as_str()).unwrap();
        assert_eq!(got.hid, e.hid);
    }

    // ── Prefix matching ─────────────────────────────────────────────────

    #[test]
    fn unique_hid_prefix_resolves() {
        let db = fresh_db();
        let e = controlled(&db, "mood:1", "ab3d0000zzzz", "mood");
        controlled(&db, "mood:2", "ffffffffffff", "mood");

        let got = resolve_quickid_single(&db, "ab3d").unwrap();
        assert_eq!(got.slug, e.slug);
    }

    #[test]
    fn ambiguous_prefix_errors_with_candidates() {
        let db = fresh_db();
        let e1 = controlled(&db, "mood:1", "abcdaaaaaaaa", "mood");
        let e2 = controlled(&db, "mood:2", "abceaaaaaaaa", "mood");

        let err = resolve_quickid_single(&db, "abc").unwrap_err();
        match err {
            DatumError::AmbiguousPrefix { prefix, candidates } => {
                assert_eq!(prefix, "abc");
                let hids: std::collections::HashSet<_> =
                    candidates.iter().map(|c| c.hid.clone()).collect();
                assert!(hids.contains(&e1.hid));
                assert!(hids.contains(&e2.hid));
            }
            other => panic!("expected AmbiguousPrefix, got {other:?}"),
        }
    }

    #[test]
    fn no_match_reports_not_found() {
        let db = fresh_db();
        controlled(&db, "coffee:1", "abcdefghijkl", "coffee");

        let err = resolve_quickid_single(&db, "does-not-exist").unwrap_err();
        assert!(matches!(err, DatumError::NotFound(ref s) if s == "does-not-exist"));
    }

    // ── Exact-match precedence ───────────────────────────────────────────

    #[test]
    fn exact_match_wins_over_longer_prefixes() {
        // A slug that is itself a prefix of another slug — plain
        // resolve_prefix would flag this as ambiguous, but exact has to
        // win.
        let db = fresh_db();
        let e = controlled(&db, "coffee:ab", "aaaabbbbcccc", "coffee");
        controlled(&db, "coffee:abc", "ddddeeeeffff", "coffee");
        controlled(&db, "coffee:abcd", "gggghhhhiiii", "coffee");

        let got = resolve_quickid_single(&db, "coffee:ab").unwrap();
        assert_eq!(got.slug, e.slug);
    }

    #[test]
    fn exact_hid_beats_slug_prefix() {
        // A bare hid that also happens to prefix another row's slug:
        // exact-hid match must win over the slug prefix candidate.
        let db = fresh_db();
        let target = controlled(&db, "mood:alpha", "abcdefghijkl", "mood");
        controlled(&db, "abcdefghijklbeta", "mnopqrstuvwx", "mood");

        let got = resolve_quickid_single(&db, "abcdefghijkl").unwrap();
        assert_eq!(got.slug, target.slug);
    }

    // ── Wildcards ────────────────────────────────────────────────────────

    #[test]
    fn slug_wildcard_expands_for_batch_callers() {
        let db = fresh_db();
        controlled(&db, "book:golden", "aaaaaaaaaaaa", "book");
        controlled(&db, "book:goldfish", "bbbbbbbbbbbb", "book");
        controlled(&db, "book:silver", "cccccccccccc", "book");

        let matches = resolve_quickid(&db, "book:gol*").unwrap();
        let slugs: std::collections::HashSet<_> = matches.iter().map(|e| e.slug.clone()).collect();
        assert_eq!(
            slugs,
            std::collections::HashSet::from([
                crate::ids::Slug::from("book:golden"),
                crate::ids::Slug::from("book:goldfish"),
            ]),
        );
    }

    #[test]
    fn slug_wildcard_on_single_target_errors_when_ambiguous() {
        let db = fresh_db();
        controlled(&db, "book:golden", "aaaaaaaaaaaa", "book");
        controlled(&db, "book:goldfish", "bbbbbbbbbbbb", "book");

        let err = resolve_quickid_single(&db, "book:gol*").unwrap_err();
        assert!(matches!(err, DatumError::AmbiguousPrefix { .. }));
    }

    #[test]
    fn single_char_wildcard_question_mark() {
        let db = fresh_db();
        let e = controlled(&db, "mood:a1", "aaaaaaaaaaaa", "mood");
        controlled(&db, "mood:ab", "bbbbbbbbbbbb", "mood");

        // `?` matches exactly one char, so `mood:a?` matches both
        // 7-char tails. Just assert expansion happened across both.
        let matches = resolve_quickid(&db, "mood:a?").unwrap();
        let slugs: std::collections::HashSet<_> = matches.iter().map(|e| e.slug.clone()).collect();
        assert_eq!(
            slugs,
            std::collections::HashSet::from([e.slug.clone(), crate::ids::Slug::from("mood:ab"),]),
        );
    }

    #[test]
    fn wildcard_does_not_match_tombstones() {
        let db = fresh_db();
        controlled(&db, "book:golden", "aaaaaaaaaaaa", "book");
        let ghost = controlled(&db, "book:goldfish", "bbbbbbbbbbbb", "book");
        db.soft_delete_entry(ghost.slug.as_str())
            .expect("soft_delete_entry");

        let matches = resolve_quickid(&db, "book:gol*").unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].slug, "book:golden");
    }

    // ─── quick_ordinals: what the listing prints must be what `_N`
    //     resolves to (da-8sx8) ─────────────────────────────────────────

    #[test]
    fn quick_ordinals_agree_with_the_resolver() {
        // The whole point of the map: a row labelled `_3` in `tail` must
        // be the row `_3` resolves to. Walk every ordinal and check both
        // directions against the live resolver.
        let db = fresh_db();
        seed(&db, "mood", 5);
        seed(&db, "coffee", 4);

        let map = quick_ordinals(&db).unwrap();
        assert_eq!(map.len(), 9);
        for (hid, ordinal) in &map {
            let resolved = resolve_quickid_single(&db, &format!("_{ordinal}")).unwrap();
            assert_eq!(&resolved.hid, hid, "_{ordinal} should name {hid:?}");
        }
    }

    #[test]
    fn quick_ordinals_skip_tombstones_and_renumber() {
        // A deleted row leaves the ordering entirely — the rows below it
        // shift up, exactly as the resolver's `LIMIT 1 OFFSET` does.
        let db = fresh_db();
        let entries = seed(&db, "mood", 3);
        let newest = entries.last().unwrap();
        db.soft_delete_entry(newest.slug.as_str())
            .expect("soft_delete_entry");

        let map = quick_ordinals(&db).unwrap();
        assert!(!map.contains_key(&newest.hid));
        assert_eq!(map.get(&entries[1].hid), Some(&1));
        assert_eq!(map.get(&entries[0].hid), Some(&2));
    }

    #[test]
    fn quick_ordinals_stop_at_the_cap() {
        // Rows past the cap get no short form; the display falls back to
        // the hid for them.
        let db = fresh_db();
        // `seed`'s minute-per-row clock can't reach past 59; step by a
        // `Duration` instead so the run stays strictly increasing.
        let base = at(2026, 5, 1, 12, 0);
        let entries: Vec<Entry> = (0..QUICK_ORDINAL_CAP as i64 + 2)
            .map(|i| {
                entry_builder("tick")
                    .time_start_utc(base + chrono::Duration::minutes(i))
                    .insert(&db)
            })
            .collect();

        let map = quick_ordinals(&db).unwrap();
        assert_eq!(map.len(), QUICK_ORDINAL_CAP);
        assert_eq!(map.get(&entries.last().unwrap().hid), Some(&1));
        // Oldest two rows fall off the end.
        assert_eq!(map.get(&entries[0].hid), None);
        assert_eq!(map.get(&entries[1].hid), None);
        assert_eq!(map.get(&entries[2].hid), Some(&QUICK_ORDINAL_CAP));
    }

    #[test]
    fn exact_slug_wins_over_wildcard() {
        // A literal slug with no wildcard chars must short-circuit
        // straight to exact_live_match — even when sibling rows would
        // have been wildcard-eligible. Smoke test for the precedence
        // rule in resolve_quickid.
        let db = fresh_db();
        let e = controlled(&db, "book:goldfish", "aaaaaaaaaaaa", "book");
        controlled(&db, "book:goldfinger", "bbbbbbbbbbbb", "book");

        let got = resolve_quickid_single(&db, "book:goldfish").unwrap();
        assert_eq!(got.slug, e.slug);
    }
}
