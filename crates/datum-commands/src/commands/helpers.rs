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

/// An op's instant as the trail prints one: an RFC 3339 UTC time through
/// the render layer's local formatter (`HH:MM:SS+TZ`, relative-day prefix
/// when the day differs), so the trail's times agree with what tail/now
/// show — and with seconds, so same-minute edits stay distinguishable in
/// the very listing meant to sequence them (da-r8xj).
fn version_time(rfc3339: &str, now: &datum_core::time::DatumTime) -> String {
    use crate::render::action::{format_timestamp, synthesize_local_datumtime};
    match rfc3339.parse() {
        Ok(utc) => format_timestamp(&synthesize_local_datumtime(utc), now, None),
        Err(_) => rfc3339.to_string(),
    }
}

/// The lines `datum history` and `datum why --full` print for one version
/// of an entry (da-s4erq): the op — when, its kind, the device when
/// `device` says the trail names them, and `raw`, the caller's
/// already-decorated cell for the line that wrote it (the two commands
/// quote/placeholder it differently) — then what an edit set and unset,
/// each key it left contested as a `fork` with every side, and each it
/// settled as a `join` with the value it took and whose that was.
pub(crate) fn version_lines(
    v: &datum_core::db::Version,
    now: &datum_core::time::DatumTime,
    raw: &str,
    device: bool,
) -> Vec<String> {
    let who = match device {
        true => format!("{}  ", v.device),
        false => String::new(),
    };
    let head = format!(
        "  v{}  {}  {:<9} {who}{raw}",
        v.version,
        version_time(&v.time, now),
        v.kind
    );
    let mut lines = vec![head.trim_end().to_string()];
    let (set, unset): (Vec<_>, Vec<_>) = v.changes().into_iter().partition(|(_, v)| v.is_some());
    let mut changed = Vec::new();
    if !set.is_empty() {
        let pairs = set
            .iter()
            .map(|(k, v)| format!("{k}={}", v.as_ref().unwrap_or(&JsonValue::Null)));
        changed.push(format!("set {}", pairs.collect::<Vec<_>>().join(" ")));
    }
    if !unset.is_empty() {
        let keys: Vec<&str> = unset.iter().map(|(k, _)| k.as_str()).collect();
        changed.push(format!("unset {}", keys.join(" ")));
    }
    if !changed.is_empty() {
        lines.push(format!("        {}", changed.join(", ")));
    }
    for (key, sides) in &v.fork {
        let sides = sides.as_array().into_iter().flatten().map(|s| {
            format!(
                "{} ({}, {}, v{})",
                s["value"],
                s["device"].as_str().unwrap_or("?"),
                version_time(
                    datum_core::db::chain::hlc_time(s["hlc"].as_str().unwrap_or("")),
                    now
                ),
                s["version"],
            )
        });
        lines.push(format!(
            "        fork {key}: {}",
            sides.collect::<Vec<_>>().join(" | ")
        ));
    }
    for (key, join) in &v.join {
        let took: Vec<String> = join["took"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| Some(format!("{}'s", d.as_str()?)))
            .collect();
        let whose = match took.is_empty() {
            true => "a value of its own".to_string(),
            false => took.join(" and "),
        };
        lines.push(format!("        join {key}: {} ({whose})", join["value"]));
    }
    lines
}

/// Does a trail name its devices: in a sync chain, or with ops of more than
/// one. A database that has never synced reads as it did.
pub(crate) fn names_devices(db: &Db, versions: &[datum_core::db::Version]) -> Result<bool> {
    Ok(versions.iter().any(|v| v.device != versions[0].device) || db.sync_chain()?.exists())
}

/// Convenience wrapper that swallows registry-load errors (the renderer
/// treats `None` as "no registry available"). Exposed for sibling modules
/// in `commands/` that don't want to import the private `load_registry`.
pub(crate) fn load_registry_opt(db: &Db) -> Option<datum_core::field_spec::SpecRegistry> {
    load_registry(db).ok()
}

/// [`load_registry_opt`] for a renderer that shows only entries of
/// `fields`: it reads each entry's own field's spec and nothing else, so a
/// log or a `tail` costs the same whatever else the database tracks
/// (da-ulu8f).
pub(crate) fn registry_for(db: &Db, fields: &[&str]) -> Option<SpecRegistry> {
    db.load_registry_for(fields).ok()
}

/// [`registry_for`] the fields of `entries`, and `spec`'s for the spec rows
/// a delete takes along with them.
pub(crate) fn registry_of<'a>(
    db: &Db,
    entries: impl IntoIterator<Item = &'a Entry>,
) -> Option<SpecRegistry> {
    let fields: std::collections::BTreeSet<&str> = entries
        .into_iter()
        .map(|e| e.field.as_str())
        .chain([datum_core::db::SPEC_FIELD])
        .collect();
    registry_for(db, &fields.into_iter().collect::<Vec<_>>())
}

/// An edit of a spec entry (`update spec:F`, `edit`) holds the colours it
/// changes to `setup --color`'s rule (da-eciuy). Here and not in
/// storage, which takes a synced spec as its copy wrote it.
pub(crate) fn resolve_spec_colors(updated: &mut Entry, existing: &Entry) -> Result<()> {
    match updated.data.as_mut() {
        Some(data) if updated.field == datum_core::db::SPEC_FIELD => Ok(
            datum_core::kit_color::resolve_changed(data, existing.data.as_ref())?,
        ),
        _ => Ok(()),
    }
}

/// Insert-time enforcement of a spec's lists (da-rcoi, da-eqb30): under
/// `only_listed_keys` every key in `entry.data` must be one the spec lists,
/// and under `only_listed_states` the entry's state and following state
/// must be listed states ([`FieldSpec::refuses_state`]). A write breaking
/// either fails up-front with a message naming the field and the setting.
/// No spec, or neither setting, is a no-op.
///
/// Loads the registry on demand (`None` = no specs configured → no-op) so
/// callers can use the same line at any insert site.
pub(crate) fn enforce_spec_lists(entry: &Entry, db: &Db) -> Result<()> {
    let Some(registry) = registry_for(db, &[&entry.field]) else {
        return Ok(());
    };
    enforce_spec_lists_against(entry, &registry)
}

/// Variant of [`enforce_spec_lists`] that takes an already-loaded
/// registry. Lets callers reuse a registry they loaded for rendering.
pub(crate) fn enforce_spec_lists_against(entry: &Entry, registry: &SpecRegistry) -> Result<()> {
    let Some(spec) = registry.field_spec(&entry.field) else {
        return Ok(());
    };
    for state in [&entry.state, &entry.following_state].into_iter().flatten() {
        if let Some(why) = spec.refuses_state(state) {
            anyhow::bail!(why);
        }
    }
    if !spec.only_listed_keys {
        return Ok(());
    }
    let Some(JsonValue::Object(map)) = entry.data.as_ref() else {
        return Ok(());
    };
    let undeclared = spec.unlisted_keys(map, entry.state.as_ref().and_then(JsonValue::as_str));
    if undeclared.is_empty() {
        return Ok(());
    }
    let rendered = undeclared
        .iter()
        .map(|k| format!("'{k}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let plural = if undeclared.len() == 1 { "" } else { "s" };
    anyhow::bail!(
        "field '{field}' takes only its listed keys (only_listed_keys): undeclared key{plural}: \
         {rendered} (declare {them} in the spec, e.g. `datum setup {field} --data-key <key>`, \
          or `datum setup {field} --any-keys`)",
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

/// The ids of a comma list. A slug holding a comma — a `%comment%` id reads
/// `tx:[electricity,paid]` — names ONE entry: the whole argument is tried as
/// an existing slug first, and split on every comma only when it is not
/// (da-7nv8r).
pub(crate) fn id_list<'a>(db: &Db, list: &'a str) -> Vec<&'a str> {
    let whole = list.trim();
    if whole.contains(',') && db.get_entry_including_deleted(whole).is_ok() {
        return vec![whole];
    }
    list.split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect()
}

/// Run `each` on every id of a comma list (da-9p3o) — the verbs that act on
/// one entry at a time (`history`, `why`, `undo`, `redo`). A single id is
/// passed through untouched, so its error reads as it always has. In a list
/// a failing id does not stop the rest: it is named on stderr and the
/// command exits non-zero, `delete`'s precedent for a partly-bad list. The
/// closing error names each failed id WITH its reason (da-b45wn), since a
/// reader that keeps only the last line — datum.day's bar — sees nothing else.
/// The list is read by [`id_list`], like `delete`/`update`.
pub(crate) fn for_each_id(
    db: &Db,
    list: &str,
    mut each: impl FnMut(&str) -> Result<()>,
) -> Result<()> {
    let ids = id_list(db, list);
    if ids.is_empty() {
        anyhow::bail!("no id given in '{list}'");
    }
    if ids.len() < 2 {
        return each(ids[0]);
    }
    let mut failed = Vec::new();
    for id in ids {
        if let Err(e) = each(id) {
            eprintln!("{id}: {e}");
            let e = e.to_string();
            let why = e.strip_suffix(&format!(": {id}")).unwrap_or(&e);
            failed.push(format!("{id} ({why})"));
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("failed for {}", failed.join(", "))
    }
}

/// Is `list` more than one id? The flags that name one entry's versions
/// (`history --version`, `undo --to-version`) refuse a list.
pub(crate) fn is_id_list(db: &Db, list: &str) -> bool {
    id_list(db, list).len() > 1
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

/// Split a comma-separated id argument ([`id_list`]), resolve each token, and return a
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
    for token in id_list(db, input) {
        // da-tldkb: `get`'s plain name is a field before it is a hid, while
        // the field has a live entry to show.
        if expand_ambiguous {
            let field = match id_field_named(db, token) {
                Ok(field) => field,
                Err(e) => {
                    errors.push((token.to_string(), e));
                    continue;
                }
            };
            if let Some(field) = field {
                match db.entries_for_field_asc(&field, None) {
                    Ok(found) if found.is_empty() => {}
                    Ok(found) => {
                        entries.extend(found.into_iter().filter(|e| seen.insert(e.slug.clone())));
                        continue;
                    }
                    Err(e) => {
                        errors.push((token.to_string(), e.into()));
                        continue;
                    }
                }
            }
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

/// The stale-`_LAST` WARNING that `undo`, `redo` and `delete` print in place
/// of a gate (da-s6no).
///
/// vividn, 2026-09-15: «let's remove the 15 minute gate on undo since redo is
/// possible; still put a warning though after 10 minutes», and at 20:19 «you
/// can remove the delete protection, since undo is so easy, but do give the
/// warning». Nothing refuses: past [`STALE_WARNING_MINUTES`] the command says
/// how old the change it is touching was, on stderr, and carries on.
///
/// `phrase` is the clause before the age — "undoing a change from" — so the
/// whole line reads as a sentence, and says WHAT is that old: for `undo` and
/// `redo` the change, for a bare `delete` the `_LAST` pointer and not the
/// entry it names (da-nduwi).
///
/// [`STALE_WARNING_MINUTES`]: datum_core::last_docs::STALE_WARNING_MINUTES
pub(crate) fn warn_if_stale(phrase: &str, reference_time: chrono::DateTime<chrono::Utc>) {
    if let Some(age) = datum_core::last_docs::stale_warning_age(reference_time, chrono::Utc::now())
    {
        eprintln!("warning: {phrase} {}", spell_age(age));
    }
}

/// [`warn_if_stale`] against the `_LAST` pointer's own age — for a command
/// that defaulted to it rather than naming a target. A database with no
/// pointer has no age to report and says nothing.
pub(crate) fn warn_if_stale_default_last(phrase: &str, db: &Db) {
    if let Ok(Some(reference)) = datum_core::last_docs::get_last_docs(db) {
        warn_if_stale(phrase, reference.time);
    }
}

/// "23 minutes ago", "1 hour 5 minutes ago" — the age spelled out, because
/// the warning is a sentence and `2h5m` is not.
fn spell_age(age: chrono::Duration) -> String {
    fn unit(n: i64, name: &str) -> String {
        if n == 1 {
            format!("1 {name}")
        } else {
            format!("{n} {name}s")
        }
    }
    let minutes = age.num_minutes();
    let spelled = if minutes < 60 {
        unit(minutes, "minute")
    } else if minutes < 60 * 24 {
        match (minutes / 60, minutes % 60) {
            (h, 0) => unit(h, "hour"),
            (h, m) => format!("{} {}", unit(h, "hour"), unit(m, "minute")),
        }
    } else {
        match (minutes / (60 * 24), (minutes / 60) % 24) {
            (d, 0) => unit(d, "day"),
            (d, h) => format!("{} {}", unit(d, "day"), unit(h, "hour")),
        }
    };
    format!("{spelled} ago")
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
        std::env::var("DATUM_LAST_SHOWN").ok().as_deref(),
        /*render_candidates=*/ true,
    )
}

/// Test seam for [`confirm_default_last_if_stale`]: takes the stdin
/// reader as an argument so tests can drive the prompt with a scripted
/// answer instead of blocking on the process's real stdin, and `shown`
/// the `DATUM_LAST_SHOWN` an answer's run carries (da-84h6a).
/// `render_candidates = false` suppresses the candidate-row print
/// (which uses stdout; tests don't try to capture it).
#[allow(clippy::too_many_arguments)]
pub(crate) fn confirm_default_last_if_stale_with_io<R: std::io::BufRead>(
    db: &Db,
    explicit_id: Option<&str>,
    force: bool,
    action_verb: &str,
    entries: &[Entry],
    stdin: &mut R,
    shown: Option<&str>,
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
    // An answer to an earlier asking names what it was shown (da-84h6a):
    // the pin the unanswered run printed. It is read only while `_LAST` is
    // still those entries; moved, the question is asked again, unanswered.
    let hids = reference
        .hids
        .iter()
        .map(datum_core::ids::Hid::as_str)
        .collect::<Vec<_>>()
        .join(",");
    let moved = shown.is_some_and(|s| s != hids);
    if shown.is_none() && within_protection_window(reference.time, chrono::Utc::now()) {
        return Ok(true);
    }
    // Answered, it was asked already: nothing is said again (da-e7739) —
    // least of all the age, which another tab's write may have made untrue.
    let answered = shown == Some(hids.as_str());

    // Stale: render candidates (production only — tests skip) and
    // prompt with yes-default ([Y/n]).
    if render_candidates && !answered {
        let registry = registry_of(db, entries);
        let now = datum_core::time::DatumTime::now();
        let ctxs = crate::commands::render_helpers::build_render_contexts(entries, db);
        {
            let mut stderr = std::io::stderr().lock();
            let _ = if moved {
                writeln!(
                    stderr,
                    "⚠ default `_LAST` has moved since — showing target:"
                )
            } else {
                writeln!(
                    stderr,
                    "⚠ default `_LAST` is older than {} minutes — showing target:",
                    datum_core::last_docs::PROTECTION_WINDOW_MINUTES,
                )
            };
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
    if !answered {
        let mut stderr = std::io::stderr().lock();
        let _ = write!(stderr, "{prompt}");
        let _ = stderr.flush();
    }
    let mut line = String::new();
    // A stdin that ends before a line is no answer where the host says so
    // (da-f5pg2): datum.day cannot answer on stdin, so it asks the person
    // in a dialog and runs the line again with their answer.
    // The pin goes after the question, for the answer to hand back.
    if moved
        || stdin.read_line(&mut line).unwrap_or(0) == 0
            && std::env::var("DATUM_NO_DEFAULT_ANSWER").is_ok_and(|v| v == "1")
    {
        eprint!("\nDATUM_LAST_SHOWN={hids}");
        return Err(Unanswered.into());
    }
    // Yes-default: empty input OR y/yes proceeds. Anything else aborts.
    let answer = line.trim().to_lowercase();
    Ok(answer.is_empty() || matches!(answer.as_str(), "y" | "yes"))
}

/// A question went unanswered: stdin ended under `DATUM_NO_DEFAULT_ANSWER=1`
/// (da-f5pg2), or answered for entries `_LAST` no longer is (da-84h6a). The
/// question is already on stderr, then the `DATUM_LAST_SHOWN=` pin for the
/// answer's run; `datum` exits with its own code for this and says nothing more.
#[derive(Debug)]
pub struct Unanswered;

impl std::fmt::Display for Unanswered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no answer on stdin")
    }
}

impl std::error::Error for Unanswered {}

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

/// Write the spec the TRACK document declares, unless the field already
/// has one (da-c8mh). `action` is the command word that brought the field
/// into being; [`verb_for_action`](datum_core::track::verb_for_action)
/// maps it onto one of the four canonical verbs.
pub fn save_track_spec(
    db: &Db,
    field: &str,
    action: datum_core::action::Action,
) -> anyhow::Result<()> {
    if let Some(spec) = track_spec(db, field, action)? {
        db.set_client_id(&crate::config::client_id()?);
        db.save_field_spec(&spec)?;
    }
    Ok(())
}

/// Delete the bare spec of every field `deleted` left with no live entry
/// ([`Db::delete_spec_if_orphaned_bare`], da-elv6d), recorded as `kind`.
/// Returns the specs, for the caller's `_LAST` beside the entries' — so one
/// `undo` brings back both — and to show as deletes once it is stamped
/// (da-ki5j3); apart, the first error, so a caller still points `_LAST` at
/// what it did delete.
pub fn drop_orphaned_bare_specs(
    db: &Db,
    deleted: &[datum_core::entry::Entry],
    kind: datum_core::history::ChangeKind,
) -> (Vec<datum_core::entry::Entry>, anyhow::Result<()>) {
    let mut fields: Vec<&str> = deleted.iter().map(|e| e.field.as_str()).collect();
    fields.sort_unstable();
    fields.dedup();
    let mut specs = Vec::new();
    for field in fields {
        match db.delete_spec_if_orphaned_bare(field, kind) {
            Ok(Some(spec)) => specs.push(spec),
            Ok(None) => {}
            Err(e) => return (specs, Err(e.into())),
        }
    }
    (specs, Ok(()))
}

/// The spec [`save_track_spec`] would write, or `None` when the field has
/// one — or is a meta field (a `layout`, say), which describes the
/// database rather than a life field (da-lc26c).
pub fn track_spec(
    db: &Db,
    field: &str,
    action: datum_core::action::Action,
) -> anyhow::Result<Option<datum_core::field_spec::FieldSpec>> {
    if datum_core::db::is_meta_field(field) || db.load_spec(field)?.is_some() {
        return Ok(None);
    }
    let mut spec =
        datum_core::field_spec::FieldSpec::new(field, datum_core::track::verb_for_action(action));
    spec.regenerate_derived_aliases();
    Ok(Some(spec))
}

/// How many fields a name starting several lists (da-tldkb).
const FIELD_CANDIDATES: usize = 16;

/// da-tldkb: the field `name` names — itself when it is a field, else the
/// one field whose name starts with it, as datum v1 read `tail envi`; `None`
/// when it starts none. One starting several is refused, naming them.
pub(crate) fn field_named(db: &Db, name: &str) -> Result<Option<String>> {
    if name.is_empty() || datum_parse::field_template::is_template(name) {
        return Ok(None);
    }
    if db.field_exists(name)? {
        return Ok(Some(name.to_string()));
    }
    match fields_starting(db, name)?.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(one.clone())),
        many => Err(fields_ambiguous(name, many)),
    }
}

/// The fields whose names start with `name`: as many as a refusal lists,
/// and one more to say there are more.
pub(crate) fn fields_starting(db: &Db, name: &str) -> Result<Vec<String>> {
    Ok(db.fields_starting(name, FIELD_CANDIDATES + 1)?)
}

/// da-tldkb: the field a name in an entry's place names (`get x`,
/// `get x::state`), which wins over a hid: [`field_named`], and never for a
/// slug, a wildcard or an `_` shorthand.
pub(crate) fn id_field_named(db: &Db, token: &str) -> Result<Option<String>> {
    if token.starts_with('_') || token.contains(':') || contains_wildcard(token) {
        return Ok(None);
    }
    field_named(db, token)
}

/// The refusal of a name starting several fields, naming them.
pub(crate) fn fields_ambiguous(name: &str, fields: &[String]) -> anyhow::Error {
    let shown = &fields[..fields.len().min(FIELD_CANDIDATES)];
    anyhow::anyhow!(
        "no field '{name}', and it starts {}: {}",
        if shown.len() < fields.len() {
            "these and more"
        } else {
            "these"
        },
        shown.join(", ")
    )
}

/// da-tldkb: no field is named as an entry's hid, so `get x` names one
/// thing: a name a write would make a new field of, that some entry's hid
/// is, is refused.
pub(crate) fn refuse_hid_named_field(db: &Db, field: &str) -> Result<()> {
    if !db.field_exists(field)? && db.hid_exists(field)? {
        anyhow::bail!("'{field}' is an entry's hid, and no field may be named as one");
    }
    Ok(())
}

/// da-xxqdb: a record reference written to data (`ref://P::`, or `P::` on a
/// key the spec types `ref`) is stored naming the field `P` names
/// ([`field_named`]), so it never depends on which prefixes are unique
/// later. One starting several fields is refused, naming them, and one
/// naming none where nothing may create it
/// ([`Db::creation_closer`](datum_core::db::Db::creation_closer)): in an open
/// database it is kept, to dangle until the field is logged. A reference
/// `before` held is kept as it was stored.
pub(crate) fn resolve_record_refs(
    db: &Db,
    entry: &mut Entry,
    before: Option<&Entry>,
) -> Result<()> {
    let Entry { field, data, .. } = entry;
    let Some(JsonValue::Object(data)) = data.as_mut() else {
        return Ok(());
    };
    let held = before.and_then(|e| e.data.as_ref()?.as_object());
    let values = data.iter_mut().map(|(k, v)| (k.as_str(), v));
    resolve_record_refs_in(db, field, values, held)
}

/// [`resolve_record_refs`] over `values`, each the value of its key on an
/// entry of `field`: a `key+=` / `key-=` operand as well as data. A
/// reference `held` holds is kept as stored.
pub(crate) fn resolve_record_refs_in<'a>(
    db: &Db,
    field: &str,
    values: impl IntoIterator<Item = (&'a str, &'a mut JsonValue)>,
    held: Option<&serde_json::Map<String, JsonValue>>,
) -> Result<()> {
    use datum_core::doc_ref::{data_refs, record_ref, rewrite_data_refs};
    // Only a value spelling `::` can hold one: no spec is read for the rest.
    let values: Vec<_> = values
        .into_iter()
        .filter(|(_, v)| v.to_string().contains("::"))
        .collect();
    if values.is_empty() {
        return Ok(());
    }
    let spec = db.load_spec(field)?;
    let held: Vec<String> = held
        .into_iter()
        .flatten()
        .flat_map(|(k, v)| data_refs(spec.as_ref(), k, v))
        .collect();
    for (key, value) in values {
        rewrite_data_refs(spec.as_ref(), key, value, &mut |r| {
            let Some(name) = record_ref(r).filter(|_| !held.iter().any(|h| h == r)) else {
                return Ok(None);
            };
            match field_named(db, name)? {
                Some(field) => Ok((field != name).then(|| format!("{field}::"))),
                None if db.creation_closer(name)?.is_some() => {
                    anyhow::bail!("{key} refers to {r}, and no field is or starts '{name}'")
                }
                None => Ok(None),
            }
        })?;
    }
    Ok(())
}

/// A read's field (da-tldkb): [`field_named`], or `name` as typed when it
/// starts no field.
pub(crate) fn read_field(db: &Db, name: &mut String) -> Result<()> {
    if let Some(field) = field_named(db, name)? {
        *name = field;
    }
    Ok(())
}

/// da-t3quu, da-mekab: a name datum has never seen — no live entry, no
/// spec — is most often a typo, and `ignore`, `end`, `flag` and `meta`
/// cannot sensibly begin a field. Each has a use ahead of a field's first
/// entry (a kit's field, an end of something begun before datum), so the
/// row is written and stderr says `doing` it anyway — under `--show none`
/// too: it is a warning about the input, not the command's output.
pub(crate) fn note_unseen_field(db: &Db, field: &str, doing: &str) -> Result<()> {
    if db.entries_for_field(field, Some(1))?.is_empty() && db.load_spec(field)?.is_none() {
        eprintln!("note: no field '{field}' yet (no entries, no spec); {doing} it anyway");
    }
    Ok(())
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
    use std::collections::BTreeMap;

    /// da-r8xj: version rows render modify_time through the local
    /// formatter WITH seconds, so two edits in the same minute stay
    /// distinguishable in the listing meant to sequence them.
    #[test]
    fn version_rows_distinguish_same_minute_edits() {
        use chrono::TimeZone;
        let _lock = datum_test_util::env_lock();
        let _nc = datum_test_util::ScopedEnv::set("NO_COLOR", "1");

        let now = DatumTime {
            utc: chrono::Utc.with_ymd_and_hms(2026, 6, 4, 12, 0, 0).unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        // Same minute, different seconds. Seconds survive any whole-minute
        // host-TZ offset, so assert on them directly.
        let row_a = version_time("2026-06-04T09:15:07.000Z", &now);
        let row_b = version_time("2026-06-04T09:15:33.000Z", &now);
        assert!(row_a.contains(":07"), "seconds rendered: {row_a:?}");
        assert!(row_b.contains(":33"), "seconds rendered: {row_b:?}");
    }

    /// A join that took a value two devices had both given names both.
    #[test]
    fn a_join_names_every_device_whose_side_it_took() {
        let _lock = datum_test_util::env_lock();
        let _nc = datum_test_util::ScopedEnv::set("NO_COLOR", "1");
        let join = |took: serde_json::Value| datum_core::db::Version {
            version: 4,
            hid: "h1".into(),
            op: "e/4".into(),
            device: "a".into(),
            hlc: "2026-06-04T09:15:07.000Z/0000".into(),
            time: "2026-06-04T09:15:07.000Z".into(),
            kind: "update".into(),
            raw: None,
            alias_chain: None,
            origin: None,
            effect: json!({ "set": { "data.cups": 5 } }),
            fork: Default::default(),
            join: json!({ "data.cups": { "value": 5, "took": took } })
                .as_object()
                .cloned()
                .unwrap(),
        };
        let last = |took| {
            let lines = version_lines(&join(took), &DatumTime::now(), "", false);
            lines.last().cloned().unwrap()
        };
        assert_eq!(
            last(json!(["b", "c"])),
            "        join data.cups: 5 (b's and c's)"
        );
        assert_eq!(last(json!(["b"])), "        join data.cups: 5 (b's)");
        assert_eq!(
            last(json!([])),
            "        join data.cups: 5 (a value of its own)"
        );
    }

    fn spec_with_intensity(only_listed_keys: bool) -> FieldSpec {
        let mut subfields = BTreeMap::new();
        subfields.insert(
            "intensity".to_string(),
            SubfieldSpec {
                required: false,
                value_type: SubfieldType::Number,
                in_slug: false,
                default: None,
            },
        );
        let mut spec = FieldSpec::new("mood", Action::Switch);
        spec.subfields = subfields;
        spec.only_listed_keys = only_listed_keys;
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
    fn only_listed_keys_rejects_undeclared_key() {
        let registry = registry_with(spec_with_intensity(true));
        let entry = mood_entry(json!({"weather": "sunny"}));
        let err = enforce_spec_lists_against(&entry, &registry)
            .expect_err("undeclared key must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("only_listed_keys"),
            "error names the setting: {msg}"
        );
        assert!(msg.contains("'weather'"), "error names the bad key: {msg}");
        assert!(msg.contains("mood"), "error names the field: {msg}");
    }

    #[test]
    fn only_listed_keys_accepts_declared_key() {
        let registry = registry_with(spec_with_intensity(true));
        let entry = mood_entry(json!({"intensity": 7}));
        enforce_spec_lists_against(&entry, &registry).expect("declared key must pass");
    }

    #[test]
    fn any_keys_spec_accepts_undeclared_key() {
        let registry = registry_with(spec_with_intensity(false));
        let entry = mood_entry(json!({"weather": "sunny"}));
        enforce_spec_lists_against(&entry, &registry)
            .expect("a spec without only_listed_keys must silently accept undeclared keys");
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
        enforce_spec_lists_against(&entry, &registry)
            .expect("fields without a spec are unaffected");
    }

    #[test]
    fn only_listed_keys_lists_all_undeclared_keys_sorted() {
        let registry = registry_with(spec_with_intensity(true));
        let entry = mood_entry(json!({"weather": "sunny", "altitude": 100}));
        let err = enforce_spec_lists_against(&entry, &registry).unwrap_err();
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
            false,
        )
        .unwrap();
        assert!(!proceed, "explicit 'n' must abort");
    }

    /// da-84h6a: an answer pinned to what `_LAST` still is gets read — even
    /// with the pointer fresh again — and one pinned to an entry `_LAST` has
    /// moved off is never read: the question goes back unanswered.
    #[test]
    fn confirm_reads_an_answer_only_for_the_entries_it_was_shown() {
        let db = Db::open_in_memory().unwrap();
        let shown = seed_entry_with_hid(&db, "coffee");
        set_last_docs(&db, std::slice::from_ref(&shown)).unwrap();
        let ask = |stdin: &[u8]| {
            confirm_default_last_if_stale_with_io(
                &db,
                None,
                false,
                "Retime",
                &[],
                &mut std::io::Cursor::new(stdin.to_vec()),
                Some(shown.as_str()),
                false,
            )
        };
        assert!(!ask(b"n\n").unwrap(), "the shown entry's 'n' is read");
        set_last_docs(&db, &[seed_entry_with_hid(&db, "tea")]).unwrap();
        let moved = ask(b"y\n").unwrap_err();
        assert!(
            moved.is::<Unanswered>(),
            "a moved _LAST takes no answer: {moved}"
        );
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
    fn only_listed_keys_with_no_extra_data_is_a_noop() {
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
        enforce_spec_lists_against(&entry, &registry)
            .expect("only_listed_keys accepts rows with no extra_data");
    }
}
