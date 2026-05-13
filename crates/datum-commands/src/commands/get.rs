//! `datum get <id_or_slug>` — print a single entry.

use anyhow::Result;
use datum_core::{db::Db, ids::resolve_quickid};

use crate::args::{GetArgs, OutputFlagsExt};
use crate::render::{print_entry_pretty, show_action_with_context, Action};

pub fn get(args: GetArgs, db: &Db) -> Result<()> {
    // Bare `datum get` (no positional) defaults to `_LAST` — resolve
    // against the last-accessed pointer with no time guard. The pointer
    // may name multiple hids when the previous command was a batch
    // insert/update; expand to all of them.
    let pattern = args
        .id_or_slug
        .as_deref()
        .unwrap_or(datum_core::ids::resolve::LAST_TOKEN);

    // `dm get id1,id2,id3` — comma splits the pattern into independent
    // quickIds. Each piece is resolved through the normal precedence
    // (underscore shorthands, exact match, wildcards, prefix). Slugs and
    // hids never contain commas, so the split is unambiguous.
    let pieces: Vec<&str> = if pattern.contains(',') {
        pattern.split(',').filter(|s| !s.is_empty()).collect()
    } else {
        vec![pattern]
    };

    let mut entries = Vec::new();
    for piece in &pieces {
        let mut sub = resolve_quickid(db, piece).map_err(anyhow::Error::from)?;
        if sub.is_empty() {
            return Err(anyhow::Error::from(
                datum_core::error::DatumError::NotFound((*piece).to_string()),
            ));
        }
        entries.append(&mut sub);
    }
    if entries.is_empty() {
        return Err(anyhow::Error::from(
            datum_core::error::DatumError::NotFound(pattern.to_string()),
        ));
    }

    let registry = crate::cmd::load_registry_opt(db);
    let output = args.output();
    let ctxs = crate::cmd::build_render_contexts(&entries, db);
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
        get(GetArgs::fresh(pattern), &db).unwrap();

        // Pointer must reflect every requested hid, in order.
        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(pointer.hids, vec![a.hid, b.hid, c.hid]);
    }

    #[test]
    fn single_quickid_unchanged() {
        let db = Db::open_in_memory().unwrap();
        let a = entry_at("coffee", 100);
        db.insert_entry(&a).unwrap();

        get(GetArgs::fresh(a.hid.clone()), &db).unwrap();
        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(pointer.hids, vec![a.hid]);
    }

    #[test]
    fn comma_separated_errors_when_any_missing() {
        let db = Db::open_in_memory().unwrap();
        let a = entry_at("coffee", 100);
        db.insert_entry(&a).unwrap();

        let pattern = format!("{},zzzz", &a.hid[..4]);
        let err = get(GetArgs::fresh(pattern), &db).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("zzzz") || msg.to_lowercase().contains("not found"),
            "missing piece should surface in error: {msg}"
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
        get(GetArgs::fresh(pattern), &db).unwrap();
        let pointer = datum_core::last_docs::get_last_docs(&db).unwrap().unwrap();
        assert_eq!(pointer.hids, vec![a.hid, b.hid]);
    }
}
