//! Shared id-resolution + registry-loading helpers used across commands.
//!
//! Extracted from `cmd.rs` so individual command modules can share the
//! tombstone-aware quickId resolvers and the registry loader without
//! pulling in the dispatch shell.

use anyhow::Result;
use datum_core::{
    db::Db,
    entry::Entry,
    field_spec::SpecRegistry,
    ids::{contains_wildcard, resolve_quickid, resolve_quickid_single},
};
use serde_json::Value as JsonValue;

fn load_registry(db: &Db) -> Result<datum_core::field_spec::SpecRegistry> {
    Ok(db.load_registry()?)
}

/// Convenience wrapper that swallows registry-load errors (the renderer
/// treats `None` as "no registry available"). Exposed for sibling modules
/// in `commands/` that don't want to import the private `load_registry`.
pub(crate) fn load_registry_opt(db: &Db) -> Option<datum_core::field_spec::SpecRegistry> {
    load_registry(db).ok()
}

/// Insert-time enforcement of `spec.strict` for `entry.data` (da-rcoi).
///
/// When the entry's field carries a FieldSpec with `strict = true`, every
/// top-level key in `entry.data` must be declared in `spec.subfields`. Any
/// undeclared key fails the write up-front with a message naming the field,
/// the offending key(s), and the remediation. Non-strict specs and rows
/// with no spec / no extra_data are no-ops, preserving the existing
/// permissive default.
///
/// Loads the registry on demand (`None` = no specs configured → no-op) so
/// callers can use the same line at any insert site.
pub(crate) fn enforce_strict_extra_data(entry: &Entry, db: &Db) -> Result<()> {
    let Some(registry) = load_registry_opt(db) else {
        return Ok(());
    };
    enforce_strict_extra_data_against(entry, &registry)
}

/// Variant of [`enforce_strict_extra_data`] that takes an already-loaded
/// registry. Lets callers reuse a registry they loaded for rendering.
pub(crate) fn enforce_strict_extra_data_against(
    entry: &Entry,
    registry: &SpecRegistry,
) -> Result<()> {
    let Some(spec) = registry.field_spec(&entry.field) else {
        return Ok(());
    };
    if !spec.strict {
        return Ok(());
    }
    let Some(JsonValue::Object(map)) = entry.data.as_ref() else {
        return Ok(());
    };
    let mut undeclared: Vec<&str> = map
        .keys()
        .filter(|k| !spec.subfields.contains_key(k.as_str()))
        .map(String::as_str)
        .collect();
    if undeclared.is_empty() {
        return Ok(());
    }
    undeclared.sort();
    let rendered = undeclared
        .iter()
        .map(|k| format!("'{k}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let plural = if undeclared.len() == 1 { "" } else { "s" };
    anyhow::bail!(
        "field '{field}' is strict: undeclared extra_data key{plural}: {rendered} \
         (declare {them} in the field spec via `datum spec set {field} key:<type>`, \
          or remove `--strict` from the spec)",
        field = entry.field,
        them = if undeclared.len() == 1 { "it" } else { "them" },
    );
}

/// Resolve `input` to an entry via quickId matching, preserving the
/// "Tombstoned" signal so update/edit can produce their tailored error.
///
/// The live-entry path goes through [`resolve_quickid_single`], which adds
/// `_` / `<field>:_` shorthands and slug wildcards on top of the existing
/// C3 prefix behaviour. It excludes soft-deleted rows, so a prefix of a
/// tombstone comes back as `NotFound`. When that happens we retry with an
/// exact id/slug lookup that includes tombstones, then one more attempt
/// that scans tombstones by hid/slug prefix — so the undo / history / why
/// commands still work on a soft-deleted row's short hid (e.g. `5arfa`).
pub(crate) fn resolve_prefix_or_tombstone(db: &Db, input: &str) -> Result<Entry> {
    match resolve_quickid_single(db, input) {
        Ok(e) => Ok(e),
        Err(datum_core::error::DatumError::NotFound(_)) => {
            // Exact match first, then fall through to a tombstone-prefix scan.
            if let Ok(exact) = db.get_entry_including_deleted(input) {
                return Ok(exact);
            }
            let conn = db.conn();
            let pattern = {
                let mut s = String::with_capacity(input.len() + 1);
                for ch in input.chars() {
                    if ch == '\\' || ch == '%' || ch == '_' {
                        s.push('\\');
                    }
                    s.push(ch);
                }
                s.push('%');
                s
            };
            let mut stmt = conn.prepare(
                "SELECT slug FROM entries
                 WHERE (slug LIKE ?1 ESCAPE '\\' OR hid LIKE ?1 ESCAPE '\\')
                   AND deleted_at IS NOT NULL
                 ORDER BY slug",
            )?;
            let matches: Vec<String> = stmt
                .query_map([pattern.as_str()], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<_, _>>()?;
            match matches.len() {
                0 => Err(datum_core::error::DatumError::NotFound(input.to_string()).into()),
                1 => Ok(db.get_entry_including_deleted(&matches[0])?),
                _ => Err(anyhow::anyhow!(
                    "ambiguous tombstone prefix '{input}' matches {} entries",
                    matches.len()
                )),
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// Batch-resolution surface for S3: how "only a tombstone matches" should
/// be reported.
#[derive(Clone, Copy)]
pub(crate) enum BatchMode {
    /// Live only. A literal token hitting a soft-deleted row surfaces as
    /// `NotFound` — delete won't re-delete a tombstone.
    Live,
    /// Live preferred; a literal token matching only a tombstone surfaces
    /// the tombstoned entry so update/merge/rekey can emit their tailored
    /// "Cannot X tombstoned entry" error. Wildcards still only match live.
    LiveOrTombstone,
}

/// Split a comma-separated id argument, resolve each token, and return a
/// deduplicated entry list plus the tokens that failed to resolve. Wildcards
/// (`*` / `?`) expand to every live match; literals go through the quickId
/// resolver (underscore shorthands, exact slug/hid, hid prefix). Empty
/// tokens (`","` / trailing comma) are silently dropped so datum-style
/// `"abc,def,ghi,"` works without special casing. Duplicates (two tokens
/// reaching the same slug) collapse in first-seen order.
pub(crate) fn resolve_batch(
    db: &Db,
    input: &str,
    mode: BatchMode,
) -> (Vec<Entry>, Vec<(String, anyhow::Error)>) {
    let mut entries: Vec<Entry> = Vec::new();
    let mut seen: std::collections::HashSet<datum_core::ids::Slug> =
        std::collections::HashSet::new();
    let mut errors: Vec<(String, anyhow::Error)> = Vec::new();
    for raw in input.split(',') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        match resolve_quickid(db, token) {
            Ok(matches) if matches.is_empty() => {
                // Literal (non-wildcard) tokens may refer to a tombstone —
                // surface it in LiveOrTombstone mode so update/rekey can
                // emit their tailored error. Wildcards stay strictly live.
                if matches!(mode, BatchMode::LiveOrTombstone) && !contains_wildcard(token) {
                    if let Ok(tomb) = db.get_entry_including_deleted(token) {
                        if seen.insert(tomb.slug.clone()) {
                            entries.push(tomb);
                        }
                        continue;
                    }
                }
                errors.push((
                    token.to_string(),
                    anyhow::Error::from(datum_core::error::DatumError::NotFound(token.to_string())),
                ));
            }
            Ok(matches) => {
                for entry in matches {
                    if seen.insert(entry.slug.clone()) {
                        entries.push(entry);
                    }
                }
            }
            Err(e) => errors.push((token.to_string(), anyhow::Error::from(e))),
        }
    }
    (entries, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::{
        action::Action,
        entry::Verb,
        field_spec::{FieldSpec, SubfieldSpec, SubfieldType},
        time::DatumTime,
    };
    use serde_json::json;
    use std::collections::HashMap;

    fn spec_with_intensity(strict: bool) -> FieldSpec {
        let mut subfields = HashMap::new();
        subfields.insert(
            "intensity".to_string(),
            SubfieldSpec {
                required: false,
                value_type: SubfieldType::Number,
                in_slug: false,
                default: None,
                unit: None,
            },
        );
        let mut spec = FieldSpec::new("mood", Action::Switch);
        spec.subfields = subfields;
        spec.strict = strict;
        spec
    }

    fn mood_entry(data: serde_json::Value) -> Entry {
        Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(json!("happy")),
            Some(data),
            Some(DatumTime::now()),
            None,
            "d1",
        )
    }

    fn registry_with(spec: FieldSpec) -> SpecRegistry {
        let db = Db::open_in_memory().unwrap();
        db.save_field_spec(&spec).unwrap();
        db.load_registry().unwrap()
    }

    #[test]
    fn strict_spec_rejects_undeclared_key() {
        let registry = registry_with(spec_with_intensity(true));
        let entry = mood_entry(json!({"weather": "sunny"}));
        let err = enforce_strict_extra_data_against(&entry, &registry)
            .expect_err("undeclared key must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("strict"), "error mentions strict: {msg}");
        assert!(msg.contains("'weather'"), "error names the bad key: {msg}");
        assert!(msg.contains("mood"), "error names the field: {msg}");
    }

    #[test]
    fn strict_spec_accepts_declared_key() {
        let registry = registry_with(spec_with_intensity(true));
        let entry = mood_entry(json!({"intensity": 7}));
        enforce_strict_extra_data_against(&entry, &registry).expect("declared key must pass");
    }

    #[test]
    fn nonstrict_spec_accepts_undeclared_key() {
        let registry = registry_with(spec_with_intensity(false));
        let entry = mood_entry(json!({"weather": "sunny"}));
        enforce_strict_extra_data_against(&entry, &registry)
            .expect("non-strict spec must silently accept undeclared keys");
    }

    #[test]
    fn missing_spec_is_a_noop() {
        // No spec saved → registry returns None → enforcement skips.
        let registry = registry_with(spec_with_intensity(true));
        let entry = Entry::new_legacy(
            "other",
            Verb::Switch,
            Some(json!("v")),
            Some(json!({"any_key": 1})),
            Some(DatumTime::now()),
            None,
            "d1",
        );
        enforce_strict_extra_data_against(&entry, &registry)
            .expect("fields without a spec are unaffected");
    }

    #[test]
    fn strict_spec_lists_all_undeclared_keys_sorted() {
        let registry = registry_with(spec_with_intensity(true));
        let entry = mood_entry(json!({"weather": "sunny", "altitude": 100}));
        let err = enforce_strict_extra_data_against(&entry, &registry).unwrap_err();
        let msg = err.to_string();
        let alt_pos = msg.find("'altitude'").expect("lists altitude");
        let weather_pos = msg.find("'weather'").expect("lists weather");
        assert!(alt_pos < weather_pos, "keys reported sorted: {msg}");
        assert!(msg.contains("key"), "plural marker present: {msg}");
    }

    #[test]
    fn strict_spec_with_no_extra_data_is_a_noop() {
        let registry = registry_with(spec_with_intensity(true));
        let entry = Entry::new_legacy(
            "mood",
            Verb::Switch,
            Some(json!("happy")),
            None,
            Some(DatumTime::now()),
            None,
            "d1",
        );
        enforce_strict_extra_data_against(&entry, &registry)
            .expect("strict spec accepts rows with no extra_data");
    }
}
