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
         ORDER BY occur_time ASC,
                  slug ASC",
    )?;
    let rows = stmt.query_map(params![like], row_to_entry_public)?;
    rows.map(|r| r.map_err(DatumError::Db)).collect()
}
