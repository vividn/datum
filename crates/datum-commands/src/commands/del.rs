//! `datum del <id_or_slug>` — mark an entry deleted (tombstone, sync-safe).

use anyhow::Result;
use datum_core::{db::Db, ids::Hid, time::DatumTime};

use crate::{
    args::{DeleteArgs, OutputFlagsExt},
    commands::{
        helpers::{load_registry_opt, resolve_batch, BatchMode},
        render_helpers::build_render_contexts,
    },
    render::{print_entry, show_delete},
};

pub fn del(args: DeleteArgs, db: &Db) -> Result<()> {
    let registry = load_registry_opt(db);
    let output = args.output()?;
    // da-uzxj: bare `datum delete` defaults to `_LAST`. The 15-minute
    // protection window is now a *prompt* (see
    // `confirm_default_last_if_stale`) rather than a hard refusal —
    // when the pointer is stale, we render the candidate and ask
    // [Y/n] before mutating. With an explicit id, or within the
    // window, or `--force`, no prompt fires at all.
    let pattern = args
        .id_or_slug
        .as_deref()
        .unwrap_or(datum_core::ids::resolve::LAST_TOKEN);
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

    // da-uzxj: prompt only when the user defaulted to `_LAST` AND the
    // pointer is older than the protection window. Explicit ids,
    // fresh `_LAST`, and `--force` skip the prompt entirely — the
    // user named what they wanted (or the pointer is fresh enough to
    // trust the default).
    if !crate::commands::helpers::confirm_default_last_if_stale(
        db,
        args.id_or_slug.as_deref(),
        args.force,
        "Delete",
        &entries,
    )? {
        println!("Aborted.");
        return Ok(());
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
