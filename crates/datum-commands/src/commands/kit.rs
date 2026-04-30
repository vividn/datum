//! `datum kit` — installable, exportable, shareable bundles of field specs
//! (slice 1: field-spec subset only).
//!
//! Subcommands:
//! - `install <path> [--force]` — parse a kit TOML and upsert each
//!   `[[field]]` into `field_specs` with `kit_id = <kit name>`. Existing
//!   rows for the same field win unless `--force` is passed.
//! - `export <name> [<path>]` — round-trip the rows installed by this kit
//!   back to TOML. None path → stdout.
//! - `list` — installed kits and per-kit field counts.
//! - `uninstall <name>` — drop every row tagged with this kit's id.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use datum_core::db::Db;
use datum_core::kit::{FieldEntry, KitToml};
use datum_core::kit_color::resolve_color;

use crate::args::{KitCommand, KitExportArgs, KitInstallArgs, KitUninstallArgs};

pub fn kit(cmd: KitCommand, db: &Db) -> Result<()> {
    match cmd {
        KitCommand::Install(args) => install(args, db),
        KitCommand::Export(args) => export(args, db),
        KitCommand::List => list(db),
        KitCommand::Uninstall(args) => uninstall(args, db),
    }
}

fn install(args: KitInstallArgs, db: &Db) -> Result<()> {
    let path = Path::new(&args.path);
    let src =
        fs::read_to_string(path).with_context(|| format!("read kit TOML at {}", path.display()))?;
    let kit = KitToml::from_str(&src)?;

    if kit.kit.name.trim().is_empty() {
        anyhow::bail!("kit TOML at {} is missing [kit].name", path.display());
    }

    // Convert each [[field]] entry into a stored FieldSpec, resolving
    // colors at install time so the saved row carries hex.
    let mut specs = Vec::with_capacity(kit.fields.len());
    for entry in kit.fields {
        let resolved = resolve_color(entry.color.as_deref(), &entry.name)?;
        let entry_with_resolved = FieldEntry {
            color: Some(resolved.clone()),
            ..entry
        };
        specs.push(entry_with_resolved.into_spec(Some(resolved))?);
    }

    let report = db.install_kit(&kit.kit.name, &specs, args.force)?;

    let installed: Vec<&str> = report.installed().collect();
    let skipped: Vec<&str> = report.skipped().collect();

    if !installed.is_empty() {
        println!(
            "Installed kit '{}': {} field(s)",
            kit.kit.name,
            installed.len()
        );
        for f in &installed {
            println!("  + {f}");
        }
    } else {
        println!("Installed kit '{}' (no new fields)", kit.kit.name);
    }
    if !skipped.is_empty() {
        println!(
            "Skipped {} field(s) (existing rows kept; use --force to overwrite):",
            skipped.len()
        );
        for f in &skipped {
            println!("  - {f}");
        }
    }
    Ok(())
}

fn export(args: KitExportArgs, db: &Db) -> Result<()> {
    let specs = db.load_specs_by_kit(&args.name)?;
    if specs.is_empty() {
        anyhow::bail!("no installed kit named '{}'", args.name);
    }
    let kit = KitToml::from_field_specs(&args.name, None, &specs);
    let toml = kit.to_toml()?;

    match args.path.as_deref() {
        Some(p) => {
            fs::write(p, &toml).with_context(|| format!("write kit TOML to {p}"))?;
            println!(
                "Exported kit '{}' ({} field(s)) → {p}",
                args.name,
                specs.len()
            );
        }
        None => print!("{toml}"),
    }
    Ok(())
}

fn list(db: &Db) -> Result<()> {
    let kits = db.list_kits()?;
    if kits.is_empty() {
        println!("No kits installed.");
        return Ok(());
    }
    println!("{:<24} fields", "kit");
    for k in kits {
        println!("{:<24} {}", k.kit_id, k.field_count);
    }
    Ok(())
}

fn uninstall(args: KitUninstallArgs, db: &Db) -> Result<()> {
    let removed = db.uninstall_kit(&args.name)?;
    if removed.is_empty() {
        println!("No fields tagged with kit '{}'.", args.name);
    } else {
        println!(
            "Uninstalled kit '{}': {} field(s)",
            args.name,
            removed.len()
        );
        for f in &removed {
            println!("  - {f}");
        }
    }
    Ok(())
}

pub fn kit_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("kit_from_argv: stub — R2 extracts bespoke parser")
}

pub fn kit_from_string(_input: &str, _db: &Db) -> Result<()> {
    anyhow::bail!("kit_from_string: stub — R2 extracts bespoke parser")
}
