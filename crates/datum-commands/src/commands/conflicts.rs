//! `datum conflicts` — D5 conflict queue management (list, resolve, auto-resolve).

use anyhow::Result;
use chrono::Utc;
use datum_core::db::Db;

use crate::args::{
    ConflictsAutoResolveArgs, ConflictsCommand, ConflictsListArgs, ConflictsResolveArgs,
    ConflictsShowArgs,
};

pub fn conflicts(sub: ConflictsCommand, db: &Db) -> Result<()> {
    match sub {
        ConflictsCommand::List(args) => list(args, db),
        ConflictsCommand::Show(args) => show(args, db),
        ConflictsCommand::Resolve(args) => resolve(args, db),
        ConflictsCommand::AutoResolve(args) => auto_resolve(args, db),
    }
}

fn list(args: ConflictsListArgs, db: &Db) -> Result<()> {
    use datum_core::conflicts::{ConflictKind, ListFilter};
    let kind = match args.kind.as_deref() {
        None => None,
        Some(s) => Some(
            s.parse::<ConflictKind>()
                .map_err(|e| anyhow::anyhow!(e))?,
        ),
    };
    let filter = ListFilter {
        kind,
        pending: if args.all { None } else { Some(true) },
    };
    let rows = db.list_conflicts(&filter)?;
    if rows.is_empty() {
        println!("No conflicts.");
        return Ok(());
    }
    for c in &rows {
        let status = match &c.resolution {
            None => "pending".to_string(),
            Some(r) => format!("resolved:{r}"),
        };
        println!(
            "{hid}  {kind:<12}  {slug}  [{status}]  detected={detected}",
            hid = c.hid,
            kind = c.conflict_kind.as_str(),
            slug = c.entry_slug,
            status = status,
            detected = c.detected_at.to_rfc3339(),
        );
    }
    Ok(())
}

fn show(args: ConflictsShowArgs, db: &Db) -> Result<()> {
    let conflict = db
        .get_conflict(&args.hid)?
        .ok_or_else(|| anyhow::anyhow!("conflict '{}' not found", args.hid))?;
    print!("{}", crate::render::render_conflict_show(&conflict));
    Ok(())
}

fn resolve(args: ConflictsResolveArgs, db: &Db) -> Result<()> {
    use datum_core::conflicts::Resolution;

    if args.skip {
        // `--skip` is an explicit no-op: leave the conflict pending.
        // We still acknowledge so scripts get a stable signal.
        let conflict = db
            .get_conflict(&args.hid)?
            .ok_or_else(|| anyhow::anyhow!("conflict '{}' not found", args.hid))?;
        println!(
            "↷ skipped {hid}  {kind}  (still pending)",
            hid = conflict.hid,
            kind = conflict.conflict_kind.as_str(),
        );
        return Ok(());
    }

    if args.edit {
        let resolved = run_conflict_edit(&args.hid, db)?;
        println!(
            "✓ resolved {hid}  {kind}  {resolution}",
            hid = resolved.hid,
            kind = resolved.conflict_kind.as_str(),
            resolution = resolved.resolution.as_deref().unwrap_or("?"),
        );
        return Ok(());
    }

    let resolution = match args.take.as_deref() {
        Some("local") => Resolution::TakeLocal,
        Some("remote") => Resolution::TakeRemote,
        Some(other) => anyhow::bail!(
            "--take only accepts 'local' or 'remote' (got '{other}')"
        ),
        None => {
            if args.close_older {
                Resolution::CloseOlder
            } else {
                // `--keep both` records the user's explicit keep-both decision;
                // any other string is a usage error (we only support "both" today).
                match args.keep.as_deref() {
                    Some("both") => Resolution::KeepBoth,
                    Some(other) => anyhow::bail!(
                        "--keep only supports 'both' (got '{other}'); use --close-older for auto-close"
                    ),
                    None => anyhow::bail!(
                        "specify a resolution: --keep both | --close-older | --take local|remote | --edit | --skip"
                    ),
                }
            }
        }
    };
    let resolved = db.resolve_conflict(&args.hid, resolution)?;
    println!(
        "✓ resolved {hid}  {kind}  {resolution}",
        hid = resolved.hid,
        kind = resolved.conflict_kind.as_str(),
        resolution = resolved.resolution.as_deref().unwrap_or("?"),
    );
    Ok(())
}

/// `--edit` flow: dump the local snapshot to a temp file as JSON, open
/// `$EDITOR` (or `$VISUAL`, falling back to `vi`), parse the edited
/// result back into an `Entry`, and apply it as the merged version.
///
/// The temp file is prefixed with the conflict hid so editors that show
/// the buffer name surface it. We only support JSON for now; the user
/// is expected to keep the structure intact. A blank save aborts the
/// resolution (the conflict stays pending).
fn run_conflict_edit(token: &str, db: &Db) -> Result<datum_core::conflicts::Conflict> {
    use std::io::Write;

    let conflict = db
        .get_conflict(token)?
        .ok_or_else(|| anyhow::anyhow!("conflict '{}' not found", token))?;
    let initial = serde_json::to_string_pretty(&conflict.local_entry)?;

    let mut tmp = std::env::temp_dir();
    tmp.push(format!("datum-conflict-{}.json", conflict.hid));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(initial.as_bytes())?;
    }

    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(&editor).arg(&tmp).status()?;
    if !status.success() {
        anyhow::bail!("editor '{editor}' exited with {status}; conflict left pending");
    }

    let edited_raw = std::fs::read_to_string(&tmp)?;
    let _ = std::fs::remove_file(&tmp);
    if edited_raw.trim().is_empty() {
        anyhow::bail!("editor saved an empty file; conflict left pending");
    }
    let edited: datum_core::entry::Entry = serde_json::from_str(&edited_raw)
        .map_err(|e| anyhow::anyhow!("could not parse edited entry as JSON Entry: {e}"))?;
    db.resolve_conflict_with_edited(&conflict.id, &edited)
        .map_err(Into::into)
}

fn auto_resolve(args: ConflictsAutoResolveArgs, db: &Db) -> Result<()> {
    let applied = db.auto_resolve_conflicts(Utc::now(), args.min_age)?;
    if applied.is_empty() {
        println!("No conflicts eligible for auto-resolution (min_age={}s).", args.min_age);
        return Ok(());
    }
    for c in &applied {
        println!(
            "✓ auto-resolved {hid}  {kind}  ({slug})",
            hid = c.hid,
            kind = c.conflict_kind.as_str(),
            slug = c.entry_slug,
        );
    }
    Ok(())
}
