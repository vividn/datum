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

    let entries = resolve_quickid(db, pattern).map_err(anyhow::Error::from)?;
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
