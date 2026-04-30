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

use std::collections::HashMap;

use anyhow::{Context, Result};
use datum_core::db::Db;
use datum_core::field_spec::{detect_anchor_cycles, format_cycle};
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

    // Cycle detection on the kit's own anchor graph. Cycles abort install —
    // there's no sensible interpretation of `A relative_to B relative_to A`.
    // Checked across the kit's specs alone (cross-kit anchors fall through to
    // the missing-anchor warning instead, which is non-fatal).
    let spec_map: HashMap<String, _> = specs.iter().map(|s| (s.field.clone(), s.clone())).collect();
    let cycles = detect_anchor_cycles(&spec_map);
    if !cycles.is_empty() {
        let rendered: Vec<String> = cycles.iter().map(|c| format_cycle(c)).collect();
        anyhow::bail!(
            "Cycle detected in field positioning: {}",
            rendered.join("; ")
        );
    }

    // Missing-anchor diagnostics: anchors that point at fields not in this
    // kit (and not already in the DB) get a warning. The dependent will
    // fall back to absolute positioning at render time.
    let mut missing: Vec<(String, String)> = Vec::new();
    for spec in &specs {
        if let Some(anchor) = spec.relative_to.as_deref() {
            if !spec_map.contains_key(anchor) && db.load_spec(anchor)?.is_none() {
                missing.push((spec.field.clone(), anchor.to_string()));
            }
        }
    }
    if !missing.is_empty() {
        for (field, anchor) in &missing {
            eprintln!(
                "warning: field '{}' references missing anchor '{}'; falling back to absolute positioning",
                field, anchor
            );
        }
    }

    let report = db.install_kit(&kit.kit.name, &specs, args.force)?;
    // Views are independent of fields; installed in the same kit-install
    // pass so a TOML containing both lands atomically (per slice 4 bead).
    let view_report = db.install_kit_views(&kit.kit.name, &kit.views, args.force)?;

    let installed: Vec<&str> = report.installed().collect();
    let skipped: Vec<&str> = report.skipped().collect();
    let views_installed: Vec<&str> = view_report.installed().collect();
    let views_skipped: Vec<&str> = view_report.skipped().collect();

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
    if !views_installed.is_empty() {
        println!("Installed {} view(s):", views_installed.len());
        for v in &views_installed {
            println!("  + {v}");
        }
    }
    if !views_skipped.is_empty() {
        println!(
            "Skipped {} view(s) (existing rows kept; use --force to overwrite):",
            views_skipped.len()
        );
        for v in &views_skipped {
            println!("  - {v}");
        }
    }
    Ok(())
}

fn export(args: KitExportArgs, db: &Db) -> Result<()> {
    let specs = db.load_specs_by_kit(&args.name)?;
    let views = db.load_views_by_kit(&args.name)?;
    if specs.is_empty() && views.is_empty() {
        anyhow::bail!("no installed kit named '{}'", args.name);
    }
    let view_count = views.len();
    let kit = KitToml::from_kit_rows(&args.name, None, &specs, views);
    let toml = kit.to_toml()?;

    match args.path.as_deref() {
        Some(p) => {
            fs::write(p, &toml).with_context(|| format!("write kit TOML to {p}"))?;
            println!(
                "Exported kit '{}' ({} field(s), {} view(s)) → {p}",
                args.name,
                specs.len(),
                view_count
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
    println!("{:<24} {:>6}  {:>5}", "kit", "fields", "views");
    for k in kits {
        println!("{:<24} {:>6}  {:>5}", k.kit_id, k.field_count, k.view_count);
    }
    Ok(())
}

fn uninstall(args: KitUninstallArgs, db: &Db) -> Result<()> {
    let removed = db.uninstall_kit(&args.name)?;
    let removed_views = db.uninstall_kit_views(&args.name)?;
    if removed.is_empty() && removed_views.is_empty() {
        println!("No fields or views tagged with kit '{}'.", args.name);
    } else {
        println!(
            "Uninstalled kit '{}': {} field(s), {} view(s)",
            args.name,
            removed.len(),
            removed_views.len()
        );
        for f in &removed {
            println!("  - field {f}");
        }
        for v in &removed_views {
            println!("  - view  {v}");
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
