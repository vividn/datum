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
    ids::{contains_wildcard, resolve_quickid, resolve_quickid_all, resolve_quickid_single},
};
use serde_json::Value as JsonValue;

fn load_registry(db: &Db) -> Result<datum_core::field_spec::SpecRegistry> {
    Ok(db.load_registry()?)
}

/// One shared formatter for the version-listing row printed by
/// `datum history` and `datum why --full` (da-r8xj). `modify_time` is a
/// host-clock UTC instant; render it through the render layer's local
/// formatter (`HH:MM:SS+TZ`, relative-day prefix when the day differs)
/// so the trail's times agree with what tail/now show — and keep seconds
/// so same-minute edits stay distinguishable in the very listing meant
/// to sequence them. `raw` is the caller's already-decorated raw_input
/// cell (the two commands quote/placeholder it differently).
pub(crate) fn format_version_row(
    v: &datum_core::history::HistoryVersion,
    now: &datum_core::time::DatumTime,
    raw: &str,
) -> String {
    format!(
        "  v{}  {}  {:<22} {}",
        v.version,
        crate::render::action::format_timestamp(
            &crate::render::action::synthesize_local_datumtime(v.modify_time),
            now,
            None,
        ),
        v.change_kind.as_str(),
        raw
    )
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
///
/// When `expand_ambiguous` is set, a token whose bare prefix matches many
/// live entries yields all of them (the read-only `get` path); otherwise an
/// ambiguous prefix surfaces as a per-token `AmbiguousPrefix` error, which
/// is the disambiguation UX for the mutating commands.
pub(crate) fn resolve_batch(
    db: &Db,
    input: &str,
    mode: BatchMode,
    expand_ambiguous: bool,
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
        let resolved = if expand_ambiguous {
            resolve_quickid_all(db, token)
        } else {
            resolve_quickid(db, token)
        };
        match resolved {
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

/// da-uzxj: stale-default protection for mutating commands.
///
/// When the user runs a mutating command (`delete`, `update`, `merge`,
/// `rekey`, `undo`) with no explicit `<id_or_slug>`, the default
/// resolves to `_LAST`. The 15-minute "protection window" already used
/// by `_LAST_WITH_PROTECTION` is mirrored here as a *confirmation
/// prompt* rather than a hard refusal — the user is more likely to
/// have moved on after a quarter hour, so we surface the candidate and
/// ask before mutating.
///
/// Returns `Ok(true)` when the caller should proceed:
/// - the user gave an explicit selector (`explicit_id = Some(_)`), or
/// - `force` is set, or
/// - the last-docs pointer is within the protection window, or
/// - the user confirmed the prompt.
///
/// Returns `Ok(false)` only when the user declined the prompt — the
/// caller turns that into a clean "Aborted." exit.
///
/// Prints the candidate rows to stderr before prompting so the user
/// can verify what they're about to mutate.
pub(crate) fn confirm_default_last_if_stale(
    db: &Db,
    explicit_id: Option<&str>,
    force: bool,
    action_verb: &str,
    entries: &[Entry],
) -> Result<bool> {
    let stdin = std::io::stdin();
    let mut locked = stdin.lock();
    confirm_default_last_if_stale_with_io(
        db,
        explicit_id,
        force,
        action_verb,
        entries,
        &mut locked,
        /*render_candidates=*/ true,
    )
}

/// Test seam for [`confirm_default_last_if_stale`]: takes the stdin
/// reader as an argument so tests can drive the prompt with a scripted
/// answer instead of blocking on the process's real stdin.
/// `render_candidates = false` suppresses the candidate-row print
/// (which uses stdout; tests don't try to capture it).
pub(crate) fn confirm_default_last_if_stale_with_io<R: std::io::BufRead>(
    db: &Db,
    explicit_id: Option<&str>,
    force: bool,
    action_verb: &str,
    entries: &[Entry],
    stdin: &mut R,
    render_candidates: bool,
) -> Result<bool> {
    use datum_core::last_docs::{get_last_docs, within_protection_window};
    use std::io::Write;

    if explicit_id.is_some() || force {
        return Ok(true);
    }
    let Some(reference) = get_last_docs(db).ok().flatten() else {
        // No pointer — let the downstream resolver surface the
        // NoLastDocsRef error; no prompt to make.
        return Ok(true);
    };
    if within_protection_window(reference.time, chrono::Utc::now()) {
        return Ok(true);
    }

    // Stale: render candidates (production only — tests skip) and
    // prompt with yes-default ([Y/n]).
    if render_candidates {
        let registry = load_registry_opt(db);
        let now = datum_core::time::DatumTime::now();
        let ctxs = crate::commands::render_helpers::build_render_contexts(entries, db);
        {
            let mut stderr = std::io::stderr().lock();
            let _ = writeln!(
                stderr,
                "⚠ default `_LAST` is older than {} minutes — showing target:",
                datum_core::last_docs::PROTECTION_WINDOW_MINUTES,
            );
        }
        for (i, entry) in entries.iter().enumerate() {
            crate::render::print_entry(entry, ctxs.get(i), Some(&now), registry.as_ref());
        }
    }
    let prompt = if entries.len() == 1 {
        format!("{action_verb} this entry? [Y/n] ")
    } else {
        format!("{action_verb} these {} entries? [Y/n] ", entries.len())
    };
    {
        let mut stderr = std::io::stderr().lock();
        let _ = write!(stderr, "{prompt}");
        let _ = stderr.flush();
    }
    let mut line = String::new();
    let _ = stdin.read_line(&mut line);
    // Yes-default: empty input OR y/yes proceeds. Anything else aborts.
    let answer = line.trim().to_lowercase();
    Ok(answer.is_empty() || matches!(answer.as_str(), "y" | "yes"))
}

// ─── Stored alias-chain replay (shared by `reparse` + `why`) ─────────────

/// Owned alias resolver built from a stored `alias_chain` JSON column.
/// Replay must be hermetic to the alias state at write time, so we
/// reconstruct the resolver from the row, never read the live aliases
/// table.
pub(crate) struct ChainResolver {
    map: std::collections::HashMap<String, datum_parse::alias::ResolvedAlias>,
}

impl datum_parse::alias::Resolver for ChainResolver {
    fn lookup(&self, name: &str) -> Option<datum_parse::alias::ResolvedAlias> {
        self.map.get(name).cloned()
    }
}

pub(crate) fn build_resolver_from_chain(chain: Option<&JsonValue>) -> ChainResolver {
    use datum_parse::alias::shell_split;

    let mut map: std::collections::HashMap<String, datum_parse::alias::ResolvedAlias> =
        std::collections::HashMap::new();
    let Some(JsonValue::Array(steps)) = chain else {
        return ChainResolver { map };
    };
    for step in steps {
        let Some(name) = step.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(expansion) = step.get("expansion").and_then(|v| v.as_str()) else {
            continue;
        };
        // First-write-wins: a chain may record the same name twice if
        // the user re-ran a command, but the shape is identical (same
        // expansion text), so collisions are benign.
        map.entry(name.to_string())
            .or_insert_with(|| datum_parse::alias::ResolvedAlias {
                expands_to: expansion.to_string(),
                tokens: shell_split(expansion),
            });
    }
    ChainResolver { map }
}

/// Reconstruct the spec v2.1 *assembled wrapped expansion* — the
/// self-describing token stream where every alias resolve reads as
/// `( body ,, own-trailing )` — from an entry's stored provenance
/// (da-g5su). Replays `raw_input` through
/// [`datum_parse::alias::expand_wrapped`] with the stored `alias_chain`
/// rehydrated as the resolver (hermetic to the alias state at write
/// time, like `reparse`). Returns `None` when no chain is stored, when
/// replay fails, or when no alias actually fires on replay (the stream
/// would just echo `raw_input`).
///
/// Structural tokens (`(`, `)`, `,,`, `,`) render bare, per the spec's
/// own examples — they ARE the stream's structure; quoting them (as
/// `shell_join` would, for shell round-trip safety) buries exactly the
/// scoping the display exists to show. Plain tokens still shell-quote
/// as needed.
pub(crate) fn assembled_expansion(raw_input: &str, chain: Option<&JsonValue>) -> Option<String> {
    use datum_parse::alias::regions::{BOUNDARY, GROUP_CLOSE, GROUP_OPEN};
    use datum_parse::alias::{expand_wrapped, shell_join, shell_split, MULTI_COMMA};

    chain?;
    let mut argv = vec!["datum".to_string()];
    argv.extend(shell_split(raw_input));
    // Mirror `parse_invocation`: a global `--db <path>` rides at the
    // head of raw_input and would shadow the alias name at argv[1] —
    // strip it before replay, exactly as the original pre-pass did.
    let (argv, _db_flag) = crate::config::split_off_global_db_flag(argv);
    let resolver = build_resolver_from_chain(chain);
    let expanded = expand_wrapped(argv, &resolver).ok()?;
    if expanded.chain.is_empty() {
        return None;
    }
    let mut tokens: Vec<String> = Vec::new();
    for (i, side) in expanded.sides.into_iter().enumerate() {
        if i > 0 {
            tokens.push(MULTI_COMMA.to_string());
        }
        tokens.extend(side);
    }
    let rendered: Vec<String> = tokens
        .iter()
        .map(|t| match t.as_str() {
            GROUP_OPEN | GROUP_CLOSE | BOUNDARY | MULTI_COMMA => t.clone(),
            _ => shell_join(std::slice::from_ref(t)),
        })
        .collect();
    Some(rendered.join(" "))
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

    /// da-r8xj: version rows render modify_time through the local
    /// formatter WITH seconds, so two edits in the same minute stay
    /// distinguishable in the listing meant to sequence them.
    #[test]
    fn version_rows_distinguish_same_minute_edits() {
        use chrono::TimeZone;
        let _lock = datum_test_util::env_lock();
        let _nc = datum_test_util::ScopedEnv::set("NO_COLOR", "1");

        let snapshot = datum_core::test_util::entry_builder("mood").build();
        let now = DatumTime {
            utc: chrono::Utc.with_ymd_and_hms(2026, 6, 4, 12, 0, 0).unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
        };
        let mk = |version: u32, sec: u32| datum_core::history::HistoryVersion {
            entry_hid: "h1".to_string(),
            version,
            snapshot: snapshot.clone(),
            modify_time: chrono::Utc
                .with_ymd_and_hms(2026, 6, 4, 9, 15, sec)
                .unwrap(),
            raw_input: None,
            alias_chain: None,
            device_id: "dev".to_string(),
            change_kind: datum_core::history::ChangeKind::UserEdit,
        };
        // Same minute, different seconds. Seconds survive any whole-minute
        // host-TZ offset, so assert on them directly.
        let row_a = format_version_row(&mk(1, 7), &now, "");
        let row_b = format_version_row(&mk(2, 33), &now, "");
        assert!(row_a.contains(":07"), "seconds rendered: {row_a:?}");
        assert!(row_b.contains(":33"), "seconds rendered: {row_b:?}");
    }

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

    // ─── da-uzxj: 15-min protection prompt on default _LAST ─────────

    use datum_core::ids::Hid;
    use datum_core::last_docs::{set_last_docs, PROTECTION_WINDOW_MINUTES};

    fn seed_entry_with_hid(db: &Db, field: &str) -> Hid {
        let e = Entry::new_legacy(
            field,
            Verb::Switch,
            Some(json!("v")),
            None,
            Some(DatumTime::now()),
            None,
            "d1",
        );
        db.insert_entry(&e).unwrap();
        e.hid
    }

    /// Explicit id supplied → no prompt fires, regardless of pointer age.
    /// Stdin is empty (would panic on read_line if reached).
    #[test]
    fn confirm_with_explicit_id_skips_prompt() {
        let db = Db::open_in_memory().unwrap();
        let mut stdin = std::io::Cursor::new(Vec::<u8>::new());
        let proceed = confirm_default_last_if_stale_with_io(
            &db,
            Some("explicit-id"),
            false,
            "Delete",
            &[],
            &mut stdin,
            false,
        )
        .unwrap();
        assert!(proceed, "explicit id must skip the stale-default prompt");
    }

    /// `--force` short-circuits the prompt even without an explicit id.
    #[test]
    fn confirm_with_force_skips_prompt() {
        let db = Db::open_in_memory().unwrap();
        let mut stdin = std::io::Cursor::new(Vec::<u8>::new());
        let proceed = confirm_default_last_if_stale_with_io(
            &db,
            None,
            true,
            "Delete",
            &[],
            &mut stdin,
            false,
        )
        .unwrap();
        assert!(proceed, "force=true must skip the stale-default prompt");
    }

    /// No last-docs pointer at all → no prompt; downstream resolver
    /// surfaces the `NoLastDocsRef` error on its own. Empty stdin.
    #[test]
    fn confirm_without_last_docs_skips_prompt() {
        let db = Db::open_in_memory().unwrap();
        let mut stdin = std::io::Cursor::new(Vec::<u8>::new());
        let proceed = confirm_default_last_if_stale_with_io(
            &db,
            None,
            false,
            "Delete",
            &[],
            &mut stdin,
            false,
        )
        .unwrap();
        assert!(
            proceed,
            "missing last-docs ref must not prompt — let resolver surface NoLastDocsRef"
        );
    }

    /// Within the 15-minute window → no prompt fires. Empty stdin
    /// proves the read_line is never reached.
    #[test]
    fn confirm_within_protection_window_skips_prompt() {
        let db = Db::open_in_memory().unwrap();
        let hid = seed_entry_with_hid(&db, "mood");
        // set_last_docs stamps the pointer with Utc::now(), well within
        // the protection window.
        set_last_docs(&db, &[hid]).unwrap();
        let mut stdin = std::io::Cursor::new(Vec::<u8>::new());
        let proceed = confirm_default_last_if_stale_with_io(
            &db,
            None,
            false,
            "Delete",
            &[],
            &mut stdin,
            false,
        )
        .unwrap();
        assert!(proceed, "fresh pointer must skip the stale-default prompt");
    }

    fn force_last_docs_stale(db: &Db, hid: &Hid) {
        // Push the last-docs pointer back past the protection window.
        // `last_docs` stores `time` as an RFC-3339 string; we rewrite
        // it directly so the test doesn't have to wait 15 minutes.
        set_last_docs(db, std::slice::from_ref(hid)).unwrap();
        let stale = chrono::Utc::now() - chrono::Duration::minutes(PROTECTION_WINDOW_MINUTES + 5);
        db.conn()
            .execute(
                "UPDATE last_docs SET time = ?1",
                rusqlite::params![stale.to_rfc3339()],
            )
            .unwrap();
    }

    /// Stale pointer + user types `y` → proceeds.
    #[test]
    fn confirm_stale_pointer_yes_input_proceeds() {
        let db = Db::open_in_memory().unwrap();
        let hid = seed_entry_with_hid(&db, "mood");
        force_last_docs_stale(&db, &hid);
        let mut stdin = std::io::Cursor::new(b"y\n".to_vec());
        let proceed = confirm_default_last_if_stale_with_io(
            &db,
            None,
            false,
            "Delete",
            &[],
            &mut stdin,
            false,
        )
        .unwrap();
        assert!(proceed);
    }

    /// Stale pointer + bare Enter → defaults yes (the key UX change
    /// vs. the legacy `[y/N]` default-no prompt).
    #[test]
    fn confirm_stale_pointer_empty_input_defaults_yes() {
        let db = Db::open_in_memory().unwrap();
        let hid = seed_entry_with_hid(&db, "mood");
        force_last_docs_stale(&db, &hid);
        let mut stdin = std::io::Cursor::new(b"\n".to_vec());
        let proceed = confirm_default_last_if_stale_with_io(
            &db,
            None,
            false,
            "Delete",
            &[],
            &mut stdin,
            false,
        )
        .unwrap();
        assert!(proceed, "empty input must proceed (yes-default)");
    }

    /// Stale pointer + explicit `n` → aborts.
    #[test]
    fn confirm_stale_pointer_no_input_aborts() {
        let db = Db::open_in_memory().unwrap();
        let hid = seed_entry_with_hid(&db, "mood");
        force_last_docs_stale(&db, &hid);
        let mut stdin = std::io::Cursor::new(b"n\n".to_vec());
        let proceed = confirm_default_last_if_stale_with_io(
            &db,
            None,
            false,
            "Delete",
            &[],
            &mut stdin,
            false,
        )
        .unwrap();
        assert!(!proceed, "explicit 'n' must abort");
    }

    /// da-g5su: `datum why` surfaces the assembled v2.1 wrapped
    /// expansion — `( body ,, own-trailing )` — reconstructed from the
    /// stored raw_input + alias_chain.
    #[test]
    fn assembled_expansion_reconstructs_wrapped_stream() {
        let chain = json!([{
            "name": "vbp",
            "expansion": "add word -k word= -k translation= srcLang=pt pos=verb",
        }]);
        let s = assembled_expansion("vbp correr ardor", Some(&chain))
            .expect("alias fires on replay → wrapped stream");
        assert_eq!(
            s,
            "( add word -k word= -k translation= srcLang=pt pos=verb ,, correr ardor )",
        );
    }

    /// No stored chain, or a chain whose alias no longer matches the
    /// raw head, yields no `expanded:` line — the stream would just
    /// echo raw_input.
    #[test]
    fn assembled_expansion_absent_when_nothing_fires() {
        assert!(assembled_expansion("occur coffee", None).is_none());
        let chain = json!([{"name": "vbp", "expansion": "add word"}]);
        assert!(assembled_expansion("occur coffee", Some(&chain)).is_none());
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
