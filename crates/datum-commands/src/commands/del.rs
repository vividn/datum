//! `datum del <id_or_slug>` — mark an entry deleted (tombstone, sync-safe).

use anyhow::Result;
use datum_core::{db::Db, entry::Entry, ids::Hid, time::DatumTime};

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
    // da-uzxj: bare `datum delete` defaults to `_LAST`. Nothing guards that
    // default any more (da-s6no) — see the warning below.
    let pattern = args
        .id_or_slug
        .as_deref()
        .unwrap_or(datum_core::ids::resolve::LAST_TOKEN);
    let (entries, token_errors) = resolve_batch(db, pattern, BatchMode::Live, false);

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

    // da-s6no: the fifteen-minute protection on the bare `_LAST` default is
    // gone. vividn, 2026-09-15 20:19: «you can remove the delete protection,
    // since undo is so easy, but do give the warning.» A delete is a
    // tombstone and `datum undo` brings it straight back, so the pointer's
    // age is worth SAYING — it is not worth a prompt the user answers blind
    // on a tty, or a refusal everywhere else (a script, the web app, which
    // have no stdin to answer with). Past ten minutes the warning names the
    // age; nothing stops. An explicit id names its own target and needs no
    // warning about the pointer.
    if args.id_or_slug.is_none() {
        crate::commands::helpers::warn_if_stale_default_last("deleting an entry", db);
    }

    // Every write, `_LAST` included, lands before anything prints: a reader
    // closing the pipe ends the process at the print (da-ki5j3).
    let why = args.comment.as_ref().map(|c| format!("deleted: {c}"));
    let mut first_err: Option<anyhow::Error> = None;
    let mut deleted: Vec<Entry> = Vec::new();
    for entry in &entries {
        match db.soft_delete_entry_with_comment(&entry.slug, why.as_deref()) {
            Ok(dead) => deleted.push(dead),
            Err(e) => {
                let msg = format!("{}: {}", entry.slug, e);
                // Strict stops here, but what it already deleted is still
                // stamped into `_LAST` below and on the undo stack, so a bare `undo` reverts it
                // (da-5leng).
                if args.strict {
                    first_err = Some(anyhow::anyhow!(msg));
                    break;
                }
                eprintln!("{msg}");
                first_err.get_or_insert_with(|| anyhow::anyhow!(msg));
            }
        }
    }

    // A field left with no live entry takes its bare spec with it, into
    // the same `_LAST` (da-elv6d). The pointer keeps the deleted hids so
    // `undo` (with no arg) can restore them. Skipped if nothing actually
    // got deleted.
    let (specs, dropped) = crate::commands::helpers::drop_orphaned_bare_specs(
        db,
        &deleted,
        datum_core::history::ChangeKind::UserEdit,
    );
    deleted.extend(specs);
    let deleted_hids: Vec<Hid> = deleted.iter().map(|e| e.hid.clone()).collect();
    if !deleted_hids.is_empty() {
        let _ = datum_core::last_docs::set_last_docs(db, &deleted_hids);
    }
    for dead in &deleted {
        show_delete(dead, registry.as_ref(), &output);
    }

    for (tok, err) in token_errors {
        let msg = format!("{tok}: {err}");
        eprintln!("{msg}");
        first_err.get_or_insert_with(|| anyhow::anyhow!(msg));
    }
    dropped?;

    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
