//! `datum get <id_or_slug>[,<id_or_slug>...]` — print entries by id or
//! slug (a list in the order given; an id that does not resolve is named
//! after the ones that did), or, in search mode (da-yhh1), every entry on a
//! field matching the supplied matchers.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::{GetArgs, OutputFlagsExt};
use crate::commands::helpers::{resolve_batch, BatchMode};
use crate::render::{print_entry_pretty, show_action_with_context, Outcome};

pub fn get(args: GetArgs, db: &Db) -> Result<()> {
    // da-yhh1: any matchers on the command line route to the search
    // path. The first positional is the field; no positional is a
    // user error (we can't search across every field on a whim).
    if !args.matchers.is_empty() {
        let field = args.id_or_slug.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "`datum get` with matchers requires a <FIELD> positional, e.g. `datum get sleep state=true`"
            )
        })?;
        return search(&field, args, db);
    }

    // Bare `datum get` (no positional) defaults to `_LAST` — resolve
    // against the last-accessed pointer with no time guard. The pointer
    // may name multiple hids when the previous command was a batch
    // insert/update; expand to all of them.
    let pattern = args
        .id_or_slug
        .as_deref()
        .unwrap_or(datum_core::ids::resolve::LAST_TOKEN);

    // da-u7mgc: `<id>::<name>` prints one value of the entry, for scripts.
    // The value prints alone whatever `--show`/`--pretty` say, but a bad
    // value for them is still refused.
    if pattern.contains("::") {
        args.output()?;
        return crate::commands::helpers::for_each_id(db, pattern, |piece| project(piece, db));
    }

    // `dm get id1,id2,id3` — comma splits the pattern into independent
    // quickIds via the canonical batch resolver (da-obes), the same path
    // del/update/retime use. Each piece runs the normal precedence
    // (underscore shorthands, exact match, wildcards, prefix); empty
    // tokens (trailing comma) drop, duplicates collapse by slug. Live
    // only — get never surfaces tombstones.
    // get renders every match, so an ambiguous prefix expands to all live
    // entries (`expand_ambiguous = true`) rather than erroring.
    let (entries, token_errors) = resolve_batch(db, pattern, BatchMode::Live, true);

    // A token that can't resolve fails the command, but only after the
    // ones that did are printed — `delete`'s precedent for a partly-bad
    // list (da-9p3o). Nothing resolved: its error, verbatim.
    if entries.is_empty() {
        return Err(token_errors.into_iter().next().map_or_else(
            || datum_core::error::DatumError::NotFound(pattern.to_string()).into(),
            |(_, err)| err,
        ));
    }

    let registry = crate::commands::helpers::registry_of(db, &entries);
    let output = args.output()?;
    // Stamp the pointer at the resolved hids BEFORE printing, so a reader
    // closing the pipe cannot cut the record short (da-ki5j3). A follow-up
    // bare command sees the same set; read-only browsing commands (tail,
    // head, grep) do NOT touch the pointer. A set with a missing token is
    // not stamped, so a partial set never sticks.
    if token_errors.is_empty() {
        let hids: Vec<datum_core::ids::Hid> = entries.iter().map(|e| e.hid.clone()).collect();
        let _ = datum_core::last_docs::set_last_docs(db, &hids);
    }

    let ctxs = crate::commands::render_helpers::build_render_contexts(&entries, db);
    for (i, entry) in entries.iter().enumerate() {
        if args.pretty {
            print_entry_pretty(entry)?;
        } else {
            show_action_with_context(
                entry,
                Outcome::Exists,
                registry.as_ref(),
                ctxs.get(i),
                &output,
            );
        }
    }

    // Named on stderr after the documents.
    if !token_errors.is_empty() {
        for (tok, err) in &token_errors {
            eprintln!("{tok}: {err}");
        }
        let toks: Vec<&str> = token_errors.iter().map(|(t, _)| t.as_str()).collect();
        anyhow::bail!("not found: {}", toks.join(","));
    }
    Ok(())
}

/// A chain of `::state` hops stops after this many (da-pimix).
const MAX_HOPS: usize = 8;

/// da-u7mgc: print `<name>` of the entry `<id>` names, alone on stdout —
/// a string bare, anything structured as compact JSON. A null or absent
/// value is an error, so a script can tell "missing" from an empty string.
/// An empty `<id>` (`::hid`) is `_LAST`. The pointer is not stamped.
///
/// da-pimix: the name splits on `::` into hops. Each hop but the last must
/// read a state or following state that refers to one entry
/// ([`state_refs`](datum_core::doc_ref::state_refs)), or a data value that
/// does ([`data_refs`](datum_core::doc_ref::data_refs), da-s5k2s), and the
/// next name is read from that entry — `x::state::class`,
/// `x::needs::title`. A value that refers to nothing, an array, a missing
/// entry, a revisit or a ninth hop is an error. A `str://ref://…` literal
/// prints without its `str://`.
fn project(piece: &str, db: &Db) -> Result<()> {
    let (id, names) = piece.split_once("::").ok_or_else(|| {
        anyhow::anyhow!("'{piece}' is not <id>::<name>, and a list of values takes no plain id")
    })?;
    let id = if id.is_empty() {
        datum_core::ids::resolve::LAST_TOKEN
    } else {
        id
    };
    let names: Vec<&str> = names.split("::").collect();
    let (last, hops) = names.split_last().expect("split yields one piece");
    let mut entry = datum_core::ids::resolve_quickid_single(db, id)?;
    let registry = (!hops.is_empty())
        .then(|| crate::commands::helpers::load_registry_opt(db))
        .flatten();
    let mut at = id.to_string();
    let mut chain = vec![entry.slug.to_string()];
    let mut seen = vec![entry.hid.clone()];
    for &name in hops {
        let value = read(&entry, name, &at)?;
        at = format!("{at}::{name}");
        if value.is_array() {
            anyhow::bail!(
                "{at} is an array, {value}; :: follows one reference, so get an element's own slug"
            );
        }
        let spec = registry.as_ref().and_then(|r| r.field_spec(&entry.field));
        let refs = match name {
            "state" | "following_state" => datum_core::doc_ref::state_refs(spec, &value),
            key => datum_core::doc_ref::data_refs(spec, key, &value),
        };
        let [slug] = refs.as_slice() else {
            anyhow::bail!("{at} is {value}, which refers to nothing");
        };
        if chain.len() > MAX_HOPS {
            anyhow::bail!(
                "{at} goes past {MAX_HOPS} hops: {} → {slug}",
                chain.join(" → ")
            );
        }
        entry = match db.get_entry(slug) {
            Err(datum_core::error::DatumError::NotFound(_)) => {
                anyhow::bail!("{at} refers to {slug}, which does not exist")
            }
            found => found?,
        };
        chain.push(entry.slug.to_string());
        if seen.contains(&entry.hid) {
            anyhow::bail!("{at} loops back to {slug}: {}", chain.join(" → "));
        }
        seen.push(entry.hid.clone());
    }
    match datum_core::doc_ref::shown(&read(&entry, last, &at)?).into_owned() {
        serde_json::Value::String(s) => println!("{s}"),
        other => println!("{other}"),
    }
    Ok(())
}

/// One value of `entry`: a column name (any key of the serialized
/// [`datum_core::entry::Entry`]) means the column, `.x.y` walks the state,
/// and anything else walks `data`. Null or absent is "`at` has no `name`".
fn read(entry: &datum_core::entry::Entry, name: &str, at: &str) -> Result<serde_json::Value> {
    let mut row = serde_json::to_value(entry)?;
    row["raw_input"] = serde_json::json!(entry.raw_input);
    if let Some(t) = &entry.occur_time {
        row["occur_time"] = serde_json::json!(t.utc);
    }
    let (root, path) = match name.strip_prefix('.') {
        Some(path) => (entry.state.as_ref(), Some(path)),
        None if row.get(name).is_some() => (row.get(name), None),
        None => (entry.data.as_ref(), Some(name)),
    };
    path.map_or(root, |p| {
        p.split('.')
            .try_fold(root?, |v, seg| v.as_object()?.get(seg))
    })
    .filter(|v| !v.is_null())
    .cloned()
    .ok_or_else(|| anyhow::anyhow!("{at} has no {name}"))
}

/// da-yhh1: load every live entry on `field`, filter by `args.matchers`
/// (AND semantics), and render the survivors in the same most-recent-
/// first order [`Db::entries_for_field`] already produces. The
/// `_LAST` pointer is updated to the resulting set so a follow-up
/// bare command sees the matched cohort.
fn search(field: &str, args: GetArgs, db: &Db) -> Result<()> {
    // Reject malformed matchers (bad regex) up front — a user error must
    // not masquerade as "no entries matching" (da-65fd).
    super::get_search::validate(&args.matchers)?;
    let candidates = db
        .entries_for_field(field, None)
        .map_err(anyhow::Error::from)?;
    let entries = super::get_search::filter(candidates, &args.matchers);

    if entries.is_empty() {
        return Err(anyhow::Error::from(
            datum_core::error::DatumError::NotFound(format!(
                "no entries on field '{field}' matching the supplied matchers"
            )),
        ));
    }

    let registry = crate::commands::helpers::registry_of(db, &entries);
    let output = args.output()?;
    // Stamped before printing (da-ki5j3).
    let hids: Vec<datum_core::ids::Hid> = entries.iter().map(|e| e.hid.clone()).collect();
    let _ = datum_core::last_docs::set_last_docs(db, &hids);

    let ctxs = crate::commands::render_helpers::build_render_contexts(&entries, db);
    for (i, entry) in entries.iter().enumerate() {
        if args.pretty {
            print_entry_pretty(entry)?;
        } else {
            show_action_with_context(
                entry,
                Outcome::Exists,
                registry.as_ref(),
                ctxs.get(i),
                &output,
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::{entry::Entry, entry::Verb, time::DatumTime};

    fn entry_at(field: &str, secs: i64) -> Entry {
        let base: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let utc = base + chrono::Duration::seconds(secs);
        let t = DatumTime {
            utc,
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        Entry::new_legacy(
            field,
            Verb::Point,
            Some(serde_json::json!(secs)),
            None,
            Some(t),
            None,
            "d1",
        )
    }

    #[test]
    fn comma_separated_quickids_fetch_all() {
        let db = Db::open_in_memory().unwrap();
        let a = entry_at("coffee", 100);
        let b = entry_at("mood", 200);
        let c = entry_at("sleep", 300);
        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();
        db.insert_entry(&c).unwrap();

        let pattern = format!("{},{},{}", &a.hid[..4], &b.hid[..4], &c.hid[..4],);
        get(GetArgs::with_id(pattern), &db).unwrap();

        // Pointer must reflect every requested hid, in order.
        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(pointer.hids, vec![a.hid, b.hid, c.hid]);
    }

    #[test]
    fn single_quickid_unchanged() {
        let db = Db::open_in_memory().unwrap();
        let a = entry_at("coffee", 100);
        db.insert_entry(&a).unwrap();

        get(GetArgs::with_id(a.hid.clone()), &db).unwrap();
        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(pointer.hids, vec![a.hid]);
    }

    #[test]
    fn comma_separated_errors_when_any_missing() {
        let db = Db::open_in_memory().unwrap();
        let a = entry_at("coffee", 100);
        db.insert_entry(&a).unwrap();

        let pattern = format!("{},zzzz", &a.hid[..4]);
        let err = get(GetArgs::with_id(pattern), &db).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("zzzz") || msg.to_lowercase().contains("not found"),
            "missing piece should surface in error: {msg}"
        );
    }

    #[test]
    fn ambiguous_prefix_returns_all_matches() {
        // Several entries share a slug prefix (`<field>:<year>…`). get
        // renders every match instead of erroring with AmbiguousPrefix —
        // the pointer reflects all of them.
        let db = Db::open_in_memory().unwrap();
        let a = entry_at("environment", 100);
        let b = entry_at("environment", 200);
        let c = entry_at("environment", 300);
        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();
        db.insert_entry(&c).unwrap();

        // All three slugs start with "environment:" — a bare prefix that
        // matches many. get must not fail.
        get(GetArgs::with_id("environment:".to_string()), &db).unwrap();

        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        let mut got: Vec<String> = pointer.hids.iter().map(|h| h.to_string()).collect();
        got.sort();
        let mut want: Vec<String> = [&a.hid, &b.hid, &c.hid]
            .iter()
            .map(|h| h.to_string())
            .collect();
        want.sort();
        assert_eq!(got, want);
    }

    /// da-65fd: a bad regex matcher errors as a user error naming the
    /// pattern — never the misleading "no entries matching".
    #[test]
    fn search_bad_regex_surfaces_user_error() {
        let db = Db::open_in_memory().unwrap();
        let a = entry_at("mood", 100);
        db.insert_entry(&a).unwrap();

        let mut args = GetArgs::with_id("mood");
        args.matchers = vec![datum_parse::input::Matcher {
            key: "state".to_string(),
            op: datum_parse::input::MatcherOp::Regex,
            value: "x[".to_string(),
        }];
        let err = get(args, &db).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("invalid regex 'x['"),
            "expected user error naming the pattern, got: {msg}"
        );
        assert!(
            !msg.contains("no entries"),
            "must not masquerade as an empty result: {msg}"
        );
    }

    #[test]
    fn comma_pattern_tolerates_trailing_comma() {
        let db = Db::open_in_memory().unwrap();
        let a = entry_at("coffee", 100);
        let b = entry_at("mood", 200);
        db.insert_entry(&a).unwrap();
        db.insert_entry(&b).unwrap();

        let pattern = format!("{},{},", &a.hid[..4], &b.hid[..4]);
        get(GetArgs::with_id(pattern), &db).unwrap();
        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(pointer.hids, vec![a.hid, b.hid]);
    }
}
