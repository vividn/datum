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
//! | `<field>:_`         | Most recent live entry for `<field>`.       |
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

use chrono::Utc;
use rusqlite::{params, OptionalExtension};

use crate::db::{row_to_entry_public, Db};
use crate::entry::Entry;
use crate::error::{DatumError, PrefixCandidate, Result};
use crate::last_docs::{
    get_last_docs, within_protection_window, PROTECTION_WINDOW_MINUTES,
};

/// `_LAST` — resolves to whatever hids the last-accessed pointer holds,
/// no time guard.
pub const LAST_TOKEN: &str = "_LAST";
/// `_LAST_WITH_PROTECTION` — same as `_LAST`, but refuses if the
/// pointer is older than [`PROTECTION_WINDOW_MINUTES`].
pub const LAST_WITH_PROTECTION_TOKEN: &str = "_LAST_WITH_PROTECTION";

/// Maximum number of candidates surfaced on an ambiguous resolution.
const AMBIGUOUS_CANDIDATE_CAP: usize = 16;

/// Resolve a quickId pattern to zero or more live entries.
///
/// Precedence:
/// 1. Underscore shorthands (`_` and `<field>:_`).
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

    // Bare `_` — the most recent live entry across every field.
    if pattern == "_" {
        return Ok(last_live_entry(db)?.into_iter().collect());
    }

    // `<field>:_` — the most recent live entry within a single field.
    if let Some(field) = pattern.strip_suffix(":_") {
        if !field.is_empty() {
            return Ok(last_live_entry_for_field(db, field)?.into_iter().collect());
        }
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

fn last_live_entry(db: &Db) -> Result<Option<Entry>> {
    db.conn()
        .query_row(
            "SELECT * FROM entries
             WHERE deleted_at IS NULL
             ORDER BY COALESCE(start_time, end_time, create_time) DESC,
                      slug DESC
             LIMIT 1",
            [],
            row_to_entry_public,
        )
        .optional()
        .map_err(DatumError::Db)
}

fn last_live_entry_for_field(db: &Db, field: &str) -> Result<Option<Entry>> {
    db.conn()
        .query_row(
            "SELECT * FROM entries
             WHERE field = ?1 AND deleted_at IS NULL
             ORDER BY COALESCE(start_time, end_time, create_time) DESC,
                      slug DESC
             LIMIT 1",
            params![field],
            row_to_entry_public,
        )
        .optional()
        .map_err(DatumError::Db)
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
         ORDER BY COALESCE(start_time, end_time, create_time) ASC,
                  slug ASC",
    )?;
    let rows = stmt.query_map(params![like], row_to_entry_public)?;
    rows.map(|r| r.map_err(DatumError::Db)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::{EntryShape, Verb, HID_LEN};
    use crate::time::DatumTime;
    use chrono::{Duration, Utc};

    fn fresh_db() -> Db {
        Db::open_in_memory().unwrap()
    }

    fn fixture(
        db: &Db,
        slug: &str,
        hid: &str,
        field: &str,
        start: Option<DatumTime>,
    ) -> Entry {
        assert_eq!(hid.len(), HID_LEN, "hid fixtures must be {HID_LEN} chars");
        let now = Utc::now();
        let e = Entry {
            hid: hid.to_string(),
            slug: slug.to_string(),
            content_hash: format!("hash-{slug}"),
            field: field.to_string(),
            entry_shape: EntryShape::Point,
            intent: None,
            verb: Verb::Occur,
            value: None,
            data: None,
            unit: None,
            start_time: start,
            end_time: None,
            start_time_precision: 0,
            start_time_precision_unit: Default::default(),
            end_time_precision: 0,
            end_time_precision_unit: Default::default(),
            lat: None,
            lon: None,
            location: None,
            create_time: now,
            modify_time: now,
            device_id: "test".to_string(),
            id_structure: None,
            field_structure: None,
            deleted_at: None,
            raw_input: String::new(),
            alias_chain: None,
            parser_version: 1,
            user_asserted_last_state: None,
        };
        db.insert_entry(&e).unwrap();
        e
    }

    fn at_offset(seconds: i64) -> DatumTime {
        DatumTime::from_utc_and_tz(Utc::now() + Duration::seconds(seconds), "UTC").unwrap()
    }

    // ── Exact id/slug (datum: "returns the string directly if the exact id exists") ──

    #[test]
    fn exact_hid_resolves_to_its_entry() {
        let db = fresh_db();
        let e = fixture(&db, "coffee:0001", "abcdefghijkl", "coffee", None);
        fixture(&db, "tea:xxxx", "mnopqrstuvwx", "tea", None);

        let got = resolve_quickid_single(&db, &e.hid).unwrap();
        assert_eq!(got.slug, e.slug);
    }

    #[test]
    fn exact_slug_resolves_to_its_entry() {
        let db = fresh_db();
        let e = fixture(&db, "coffee:2024-06-01", "abcdefghijkl", "coffee", None);
        fixture(&db, "tea:xxxx", "mnopqrstuvwx", "tea", None);

        let got = resolve_quickid_single(&db, &e.slug).unwrap();
        assert_eq!(got.hid, e.hid);
    }

    // ── Prefix matching (datum: "matches beginning of one humanId/id") ─────

    #[test]
    fn unique_hid_prefix_resolves() {
        let db = fresh_db();
        let e = fixture(&db, "mood:1", "ab3d0000zzzz", "mood", None);
        fixture(&db, "mood:2", "ffffffffffff", "mood", None);

        let got = resolve_quickid_single(&db, "ab3d").unwrap();
        assert_eq!(got.slug, e.slug);
    }

    #[test]
    fn ambiguous_prefix_errors_with_candidates() {
        let db = fresh_db();
        let e1 = fixture(&db, "mood:1", "abcdaaaaaaaa", "mood", None);
        let e2 = fixture(&db, "mood:2", "abceaaaaaaaa", "mood", None);

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
        fixture(&db, "coffee:1", "abcdefghijkl", "coffee", None);

        let err = resolve_quickid_single(&db, "does-not-exist").unwrap_err();
        assert!(matches!(err, DatumError::NotFound(ref s) if s == "does-not-exist"));
    }

    // ── Exact-match precedence (datum: "matches id exactly and a humanId exactly, prefer id") ──

    #[test]
    fn exact_match_wins_over_longer_prefixes() {
        // A slug that is itself a prefix of another slug — plain resolve_prefix
        // would flag this as ambiguous, but an exact match has to win.
        let db = fresh_db();
        let e = fixture(&db, "coffee:ab", "aaaabbbbcccc", "coffee", None);
        fixture(&db, "coffee:abc", "ddddeeeeffff", "coffee", None);
        fixture(&db, "coffee:abcd", "gggghhhhiiii", "coffee", None);

        let got = resolve_quickid_single(&db, "coffee:ab").unwrap();
        assert_eq!(got.slug, e.slug);
    }

    #[test]
    fn exact_hid_beats_slug_prefix() {
        // A bare hid that also happens to prefix another slug: the exact hid
        // match takes precedence.
        let db = fresh_db();
        let target = fixture(&db, "mood:alpha", "abcdefghijkl", "mood", None);
        fixture(&db, "abcdefghijklbeta", "mnopqrstuvwx", "mood", None);

        let got = resolve_quickid_single(&db, "abcdefghijkl").unwrap();
        assert_eq!(got.slug, target.slug);
    }

    // ── Underscore: global last ────────────────────────────────────────────

    #[test]
    fn underscore_resolves_to_most_recent_entry() {
        let db = fresh_db();
        fixture(&db, "mood:a", "aaaaaaaaaaaa", "mood", Some(at_offset(-3000)));
        fixture(&db, "mood:b", "bbbbbbbbbbbb", "mood", Some(at_offset(-2000)));
        let newest = fixture(&db, "coffee:c", "cccccccccccc", "coffee", Some(at_offset(-10)));

        let got = resolve_quickid_single(&db, "_").unwrap();
        assert_eq!(got.slug, newest.slug);
    }

    #[test]
    fn underscore_respects_tombstones() {
        let db = fresh_db();
        let old = fixture(&db, "mood:a", "aaaaaaaaaaaa", "mood", Some(at_offset(-3000)));
        let ghost = fixture(&db, "coffee:c", "cccccccccccc", "coffee", Some(at_offset(-10)));
        db.soft_delete_entry(&ghost.slug).unwrap();

        let got = resolve_quickid_single(&db, "_").unwrap();
        assert_eq!(got.slug, old.slug);
    }

    // ── Underscore: per-field last ────────────────────────────────────────

    #[test]
    fn field_colon_underscore_resolves_field_tail() {
        let db = fresh_db();
        let mood_old = fixture(
            &db,
            "mood:old",
            "aaaaaaaaaaaa",
            "mood",
            Some(at_offset(-3000)),
        );
        let mood_new = fixture(
            &db,
            "mood:new",
            "bbbbbbbbbbbb",
            "mood",
            Some(at_offset(-1000)),
        );
        // A newer entry in a different field — proves we scope to `mood`.
        fixture(
            &db,
            "coffee:newest",
            "cccccccccccc",
            "coffee",
            Some(at_offset(-10)),
        );

        let got = resolve_quickid_single(&db, "mood:_").unwrap();
        assert_eq!(got.slug, mood_new.slug);
        assert_ne!(got.slug, mood_old.slug);
    }

    #[test]
    fn field_colon_underscore_on_empty_field_errors() {
        let db = fresh_db();
        fixture(&db, "mood:a", "aaaaaaaaaaaa", "mood", Some(at_offset(-100)));

        let err = resolve_quickid_single(&db, "unknown:_").unwrap_err();
        assert!(matches!(err, DatumError::NotFound(_)));
    }

    // ── Wildcards (datum: "substring/wildcard — S3 batch vs single-target") ──

    #[test]
    fn slug_wildcard_expands_for_batch_callers() {
        let db = fresh_db();
        fixture(&db, "book:golden", "aaaaaaaaaaaa", "book", Some(at_offset(-30)));
        fixture(&db, "book:goldfish", "bbbbbbbbbbbb", "book", Some(at_offset(-20)));
        fixture(&db, "book:silver", "cccccccccccc", "book", Some(at_offset(-10)));

        let matches = resolve_quickid(&db, "book:gol*").unwrap();
        let slugs: std::collections::HashSet<_> =
            matches.iter().map(|e| e.slug.clone()).collect();
        assert_eq!(
            slugs,
            std::collections::HashSet::from([
                "book:golden".to_string(),
                "book:goldfish".to_string(),
            ]),
        );
    }

    #[test]
    fn slug_wildcard_on_single_target_errors_when_ambiguous() {
        let db = fresh_db();
        fixture(&db, "book:golden", "aaaaaaaaaaaa", "book", None);
        fixture(&db, "book:goldfish", "bbbbbbbbbbbb", "book", None);

        let err = resolve_quickid_single(&db, "book:gol*").unwrap_err();
        assert!(matches!(err, DatumError::AmbiguousPrefix { .. }));
    }

    #[test]
    fn single_char_wildcard_question_mark() {
        let db = fresh_db();
        let e = fixture(&db, "mood:a1", "aaaaaaaaaaaa", "mood", None);
        fixture(&db, "mood:ab", "bbbbbbbbbbbb", "mood", None);

        // `?` matches exactly one char, so `mood:a?` matches only the 2-char tail.
        // Both `a1` and `ab` are two chars, so multiple matches — that's fine,
        // just assert expansion happened across both.
        let matches = resolve_quickid(&db, "mood:a?").unwrap();
        let slugs: std::collections::HashSet<_> =
            matches.iter().map(|e| e.slug.clone()).collect();
        assert_eq!(
            slugs,
            std::collections::HashSet::from([
                e.slug.clone(),
                "mood:ab".to_string(),
            ]),
        );
    }

    #[test]
    fn wildcard_does_not_match_tombstones() {
        let db = fresh_db();
        fixture(&db, "book:golden", "aaaaaaaaaaaa", "book", None);
        let ghost = fixture(&db, "book:goldfish", "bbbbbbbbbbbb", "book", None);
        db.soft_delete_entry(&ghost.slug).unwrap();

        let matches = resolve_quickid(&db, "book:gol*").unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].slug, "book:golden");
    }

    #[test]
    fn exact_slug_wins_over_wildcard() {
        // A user types a literal slug that happens to contain no wildcard
        // chars — nothing to prove here beyond "exact works". When the
        // literal matches but a wildcard would also have hit siblings,
        // we still return just the one.
        let db = fresh_db();
        let e = fixture(&db, "book:goldfish", "aaaaaaaaaaaa", "book", None);
        fixture(&db, "book:goldfinger", "bbbbbbbbbbbb", "book", None);

        let got = resolve_quickid_single(&db, "book:goldfish").unwrap();
        assert_eq!(got.slug, e.slug);
    }
}
