//! `datum del <id_or_slug>` — mark an entry deleted (tombstone, sync-safe).

use anyhow::Result;
use datum_core::{db::Db, ids::Hid, time::DatumTime};

use crate::{
    args::{DeleteArgs, OutputFlagsExt},
    cmd::{build_render_contexts, load_registry_opt, resolve_batch, BatchMode},
    render::{print_entry, show_delete},
};

pub fn del(args: DeleteArgs, db: &Db) -> Result<()> {
    let registry = load_registry_opt(db);
    let output = args.output()?;
    // Bare `datum delete` defaults to `_LAST_WITH_PROTECTION` —
    // refuses if the last-accessed pointer is > 15 minutes stale.
    let pattern = args
        .id_or_slug
        .as_deref()
        .unwrap_or(datum_core::ids::resolve::LAST_WITH_PROTECTION_TOKEN);
    let (entries, token_errors) = resolve_batch(db, pattern, BatchMode::Live);

    // Nothing resolved — propagate the first token error verbatim so single-token
    // NotFound / AmbiguousPrefix surfaces unchanged in the common case.
    if entries.is_empty() {
        if let Some((_, err)) = token_errors.into_iter().next() {
            return Err(err);
        }
        anyhow::bail!("no entries specified");
    }

    // Strict: abort up-front on any resolution error so we don't half-apply
    // against a partial resolution.
    if args.strict {
        if let Some((_, err)) = token_errors.first() {
            return Err(anyhow::anyhow!("{err}"));
        }
    }

    if args.dry_run {
        let now = DatumTime::now();
        let ctxs = build_render_contexts(&entries, db);
        for (i, entry) in entries.iter().enumerate() {
            print_entry(entry, ctxs.get(i), Some(&now), registry.as_ref());
        }
        for (tok, err) in &token_errors {
            eprintln!("[dry-run] resolve '{tok}': {err}");
        }
        eprintln!(
            "[dry-run] would delete {} {}",
            entries.len(),
            if entries.len() == 1 {
                "entry"
            } else {
                "entries"
            },
        );
        return Ok(());
    }

    if !args.force {
        let now = DatumTime::now();
        let ctxs = build_render_contexts(&entries, db);
        for (i, entry) in entries.iter().enumerate() {
            print_entry(entry, ctxs.get(i), Some(&now), registry.as_ref());
        }
        let prompt = if entries.len() == 1 {
            "Delete this entry?".to_string()
        } else {
            format!("Delete these {} entries?", entries.len())
        };
        eprint!("{prompt} [y/N] ");
        use std::io::BufRead;
        let stdin = std::io::stdin();
        let line = stdin
            .lock()
            .lines()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No input provided"))??;
        if !matches!(line.to_lowercase().as_str(), "y" | "yes") {
            println!("Aborted.");
            return Ok(());
        }
    }

    let mut first_err: Option<anyhow::Error> = None;
    let mut deleted_hids: Vec<Hid> = Vec::new();
    for entry in &entries {
        match db.soft_delete_entry(&entry.slug) {
            Ok(()) => {
                show_delete(entry, registry.as_ref(), &output);
                deleted_hids.push(entry.hid.clone());
            }
            Err(e) => {
                let msg = format!("{}: {}", entry.slug, e);
                if args.strict {
                    return Err(anyhow::anyhow!(msg));
                }
                eprintln!("{msg}");
                first_err.get_or_insert_with(|| anyhow::anyhow!(msg));
            }
        }
    }

    for (tok, err) in token_errors {
        let msg = format!("{tok}: {err}");
        eprintln!("{msg}");
        first_err.get_or_insert_with(|| anyhow::anyhow!(msg));
    }

    // Pointer keeps the deleted hids so `undo` (with no arg) can
    // restore them. Skipped if nothing actually got deleted.
    if !deleted_hids.is_empty() {
        let _ = datum_core::last_docs::set_last_docs(db, &deleted_hids);
    }

    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
