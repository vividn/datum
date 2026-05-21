//! `datum integrity` — local data-integrity warning queue (list, show,
//! resolve, auto-resolve). These rows describe single-device
//! inconsistencies (double-open, orphan-end, overlap, same-state-repeat,
//! last-state-mismatch); they are NOT cross-device sync conflicts.

use anyhow::Result;
use chrono::Utc;
use datum_core::db::Db;

use crate::args::{
    IntegrityAutoResolveArgs, IntegrityCommand, IntegrityListArgs, IntegrityResolveArgs,
    IntegrityShowArgs,
};

pub fn integrity(sub: IntegrityCommand, db: &Db) -> Result<()> {
    match sub {
        IntegrityCommand::List(args) => list(args, db),
        IntegrityCommand::Show(args) => show(args, db),
        IntegrityCommand::Resolve(args) => resolve(args, db),
        IntegrityCommand::AutoResolve(args) => auto_resolve(args, db),
    }
}

fn list(args: IntegrityListArgs, db: &Db) -> Result<()> {
    use datum_core::integrity::{IntegrityKind, ListFilter};
    let kind = match args.kind.as_deref() {
        None => None,
        Some(s) => Some(s.parse::<IntegrityKind>().map_err(|e| anyhow::anyhow!(e))?),
    };
    let filter = ListFilter {
        kind,
        pending: if args.all { None } else { Some(true) },
    };
    let rows = db.list_integrity_warnings(&filter)?;
    if rows.is_empty() {
        println!("No integrity warnings.");
        return Ok(());
    }
    for w in &rows {
        let status = match &w.resolution {
            None => "pending".to_string(),
            Some(r) => format!("resolved:{r}"),
        };
        println!(
            "{hid}  {kind:<12}  {slug}  [{status}]  detected={detected}",
            hid = w.hid,
            kind = w.kind.as_str(),
            slug = w.entry_slug,
            status = status,
            detected = w.detected_at.to_rfc3339(),
        );
    }
    Ok(())
}

fn show(args: IntegrityShowArgs, db: &Db) -> Result<()> {
    let warning = db
        .get_integrity_warning(&args.hid)?
        .ok_or_else(|| anyhow::anyhow!("integrity warning '{}' not found", args.hid))?;
    print!("{}", crate::render::render_integrity_warning_show(&warning));
    Ok(())
}

fn resolve(args: IntegrityResolveArgs, db: &Db) -> Result<()> {
    use datum_core::integrity::Resolution;

    if args.skip {
        // `--skip` is an explicit no-op: leave the warning pending.
        // We still acknowledge so scripts get a stable signal.
        let warning = db
            .get_integrity_warning(&args.hid)?
            .ok_or_else(|| anyhow::anyhow!("integrity warning '{}' not found", args.hid))?;
        println!(
            "↷ skipped {hid}  {kind}  (still pending)",
            hid = warning.hid,
            kind = warning.kind.as_str(),
        );
        return Ok(());
    }

    if args.edit {
        let resolved = run_warning_edit(&args.hid, db)?;
        println!(
            "✓ resolved {hid}  {kind}  {resolution}",
            hid = resolved.hid,
            kind = resolved.kind.as_str(),
            resolution = resolved.resolution.as_deref().unwrap_or("?"),
        );
        return Ok(());
    }

    let resolution = match args.take.as_deref() {
        Some("local") => Resolution::TakeLocal,
        Some("remote") => Resolution::TakeRemote,
        Some(other) => anyhow::bail!("--take only accepts 'local' or 'remote' (got '{other}')"),
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
    let resolved = db.resolve_integrity_warning(&args.hid, resolution)?;
    println!(
        "✓ resolved {hid}  {kind}  {resolution}",
        hid = resolved.hid,
        kind = resolved.kind.as_str(),
        resolution = resolved.resolution.as_deref().unwrap_or("?"),
    );
    Ok(())
}

/// `--edit` flow: dump the local snapshot to a temp file as JSON, open
/// `$EDITOR` (or `$VISUAL`, falling back to `vi`), parse the edited
/// result back into an `Entry`, and apply it as the merged version.
///
/// The temp file is prefixed with the warning hid so editors that show
/// the buffer name surface it. We only support JSON for now; the user
/// is expected to keep the structure intact. A blank save aborts the
/// resolution (the warning stays pending).
fn run_warning_edit(token: &str, db: &Db) -> Result<datum_core::integrity::IntegrityWarning> {
    use std::io::Write;

    let warning = db
        .get_integrity_warning(token)?
        .ok_or_else(|| anyhow::anyhow!("integrity warning '{}' not found", token))?;
    let initial = serde_json::to_string_pretty(&warning.local_entry)?;

    let mut tmp = std::env::temp_dir();
    tmp.push(format!("datum-integrity-{}.json", warning.hid));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(initial.as_bytes())?;
    }

    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new(&editor).arg(&tmp).status()?;
    if !status.success() {
        anyhow::bail!("editor '{editor}' exited with {status}; warning left pending");
    }

    let edited_raw = std::fs::read_to_string(&tmp)?;
    let _ = std::fs::remove_file(&tmp);
    if edited_raw.trim().is_empty() {
        anyhow::bail!("editor saved an empty file; warning left pending");
    }
    let edited: datum_core::entry::Entry = serde_json::from_str(&edited_raw)
        .map_err(|e| anyhow::anyhow!("could not parse edited entry as JSON Entry: {e}"))?;
    db.resolve_integrity_warning_with_edited(&warning.id, &edited)
        .map_err(Into::into)
}

fn auto_resolve(args: IntegrityAutoResolveArgs, db: &Db) -> Result<()> {
    let applied = db.auto_resolve_integrity(Utc::now(), args.min_age)?;
    if applied.is_empty() {
        println!(
            "No integrity warnings eligible for auto-resolution (min_age={}s).",
            args.min_age
        );
        return Ok(());
    }
    for w in &applied {
        println!(
            "✓ auto-resolved {hid}  {kind}  ({slug})",
            hid = w.hid,
            kind = w.kind.as_str(),
            slug = w.entry_slug,
        );
    }
    Ok(())
}
