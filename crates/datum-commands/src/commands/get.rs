//! `datum get <id_or_slug>` — print a single entry, or, in search
//! mode (da-yhh1), every entry on a field matching the supplied
//! matchers.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::{GetArgs, OutputFlagsExt};
use crate::commands::helpers::{resolve_batch, BatchMode};
use crate::render::{print_entry_pretty, show_action_with_context, Action};

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

    // `dm get id1,id2,id3` — comma splits the pattern into independent
    // quickIds via the canonical batch resolver (da-obes), the same path
    // del/update/retime use. Each piece runs the normal precedence
    // (underscore shorthands, exact match, wildcards, prefix); empty
    // tokens (trailing comma) drop, duplicates collapse by slug. Live
    // only — get never surfaces tombstones.
    // get renders every match, so an ambiguous prefix expands to all live
    // entries (`expand_ambiguous = true`) rather than erroring.
    let (entries, token_errors) = resolve_batch(db, pattern, BatchMode::Live, true);

    // get fails the whole command if any token can't resolve, so a
    // bad piece surfaces as an error rather than being silently dropped.
    // Check before stamping the pointer so a partial set never sticks.
    if let Some((_, err)) = token_errors.into_iter().next() {
        return Err(err);
    }
    if entries.is_empty() {
        return Err(anyhow::Error::from(
            datum_core::error::DatumError::NotFound(pattern.to_string()),
        ));
    }

    let registry = crate::commands::helpers::load_registry_opt(db);
    let output = args.output()?;
    let ctxs = crate::commands::render_helpers::build_render_contexts(&entries, db);
    for (i, entry) in entries.iter().enumerate() {
        if args.pretty {
            print_entry_pretty(entry)?;
        } else {
            show_action_with_context(
                entry,
                Action::Exists,
                registry.as_ref(),
                ctxs.get(i),
                &output,
            );
        }
    }

    // Stamp the pointer at the resolved hids so a follow-up bare command
    // sees the same set. Read-only browsing commands (tail, head, grep)
    // do NOT touch the pointer.
    let hids: Vec<datum_core::ids::Hid> = entries.iter().map(|e| e.hid.clone()).collect();
    let _ = datum_core::last_docs::set_last_docs(db, &hids);
    Ok(())
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

    let registry = crate::commands::helpers::load_registry_opt(db);
    let output = args.output()?;
    let ctxs = crate::commands::render_helpers::build_render_contexts(&entries, db);
    for (i, entry) in entries.iter().enumerate() {
        if args.pretty {
            print_entry_pretty(entry)?;
        } else {
            show_action_with_context(
                entry,
                Action::Exists,
                registry.as_ref(),
                ctxs.get(i),
                &output,
            );
        }
    }

    let hids: Vec<datum_core::ids::Hid> = entries.iter().map(|e| e.hid.clone()).collect();
    let _ = datum_core::last_docs::set_last_docs(db, &hids);
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
            timezone: "UTC".to_string(),
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
