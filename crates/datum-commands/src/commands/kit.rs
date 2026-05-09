//! `datum kit` — installable, exportable, shareable bundles of field specs
//! and (slice 5) custom indices.
//!
//! Subcommands:
//! - `install <path> [--force] [--yes]` — parse a kit TOML and upsert each
//!   `[[field]]` into `field_specs` with `kit_id = <kit name>`. Existing
//!   rows for the same field win unless `--force` is passed. If the kit
//!   declares `[[index]]` blocks, the user is prompted for trust unless
//!   `--yes`. URLs are blocked until the signed-registry hook lands.
//! - `export <name> [<path>]` — round-trip the rows installed by this kit
//!   back to TOML. None path → stdout.
//! - `list` — installed kits with per-kit field/index counts.
//! - `uninstall <name>` — drop every row tagged with this kit's id and
//!   `DROP INDEX` every kit-owned index.

use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::Path;

use std::collections::HashMap;

use anyhow::{Context, Result};
use datum_core::db::Db;
use datum_core::field_spec::{detect_anchor_cycles, format_cycle};
use datum_core::kit::{FieldEntry, IndexEntry, KitToml};
use datum_core::kit_color::resolve_color;

use crate::args::{KitCommand, KitExportArgs, KitInstallArgs, KitUninstallArgs};

pub fn kit(cmd: KitCommand, db: &Db) -> Result<()> {
    match cmd {
        KitCommand::Install(args) => install(args, db, &mut StdIoTrust),
        KitCommand::Export(args) => export(args, db),
        KitCommand::List => list(db),
        KitCommand::Uninstall(args) => uninstall(args, db),
    }
}

/// Source from which a kit is being installed. Drives the trust check:
/// local files prompt the user; URLs are gated on a (future) signed
/// registry. Detected by [`classify_source`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KitSource {
    /// Filesystem path. Trust is interactive (or `--yes`).
    Local,
    /// HTTP/HTTPS URL. Blocked until signed-registry support lands.
    Url,
}

/// Trust callback surface. Production wires `StdIoTrust` (reads stdin /
/// honors `--yes`); tests substitute a scripted impl. The single method
/// is asked once per install, after the kit is parsed but before any DB
/// writes — implementations can prompt, log, or short-circuit.
pub trait TrustGate {
    /// Decide whether to allow installing the kit's `[[index]]` blocks.
    /// Implementations receive the kit name + the proposed entries so
    /// they can render them in whatever format suits the surface.
    /// Returning `Ok(false)` aborts the install with a clean error.
    fn approve_indices(
        &mut self,
        kit_name: &str,
        indices: &[IndexEntry],
        is_reinstall: bool,
        assume_yes: bool,
    ) -> Result<bool>;
}

/// Production trust gate: prints the proposed DDL to stderr and reads
/// y/N from stdin. Honors `--yes` (skip the prompt and approve). Falls
/// back to deny if stdin is not a TTY and `--yes` was not passed — the
/// design says the default is no, and a non-interactive shell that
/// didn't pre-authorize indices should fail loudly rather than silently
/// approve.
pub struct StdIoTrust;

impl TrustGate for StdIoTrust {
    fn approve_indices(
        &mut self,
        kit_name: &str,
        indices: &[IndexEntry],
        is_reinstall: bool,
        assume_yes: bool,
    ) -> Result<bool> {
        if assume_yes {
            return Ok(true);
        }
        if !io::stdin().is_terminal() {
            anyhow::bail!(
                "kit '{kit_name}' wants to create {} index/indices but stdin is not a TTY; \
                 re-run with --yes to approve non-interactively",
                indices.len()
            );
        }
        let mut stderr = io::stderr().lock();
        let prefix = if is_reinstall { "Re-installing" } else { "Kit" };
        writeln!(
            stderr,
            "{prefix} '{kit_name}' wants to create {} index/indices:",
            indices.len()
        )?;
        for ix in indices {
            writeln!(stderr, "  - {}", summarize_index(ix))?;
        }
        write!(stderr, "Allow? (y/N) [N]: ")?;
        stderr.flush()?;
        drop(stderr);

        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        let answer = line.trim();
        Ok(matches!(answer, "y" | "Y" | "yes" | "YES" | "Yes"))
    }
}

fn summarize_index(ix: &IndexEntry) -> String {
    let unique = if ix.unique { "UNIQUE " } else { "" };
    match &ix.where_clause {
        Some(w) => format!(
            "{unique}{name} ON {on}({expr}) WHERE {w}",
            name = ix.name,
            on = ix.on,
            expr = ix.expression
        ),
        None => format!(
            "{unique}{name} ON {on}({expr})",
            name = ix.name,
            on = ix.on,
            expr = ix.expression
        ),
    }
}

/// Classify a kit-source string as a local path or a remote URL.
///
/// We treat `http://`, `https://`, and any `<scheme>://` prefix that
/// isn't `file://` as remote. `file://` is collapsed back to local —
/// today it's mostly used in tests but it semantically refers to the
/// local filesystem. This function is `pub(crate)` so the trust-gate
/// tests can drive it directly.
pub(crate) fn classify_source(s: &str) -> KitSource {
    if let Some(rest) = s.strip_prefix("file://") {
        let _ = rest;
        return KitSource::Local;
    }
    if s.contains("://") {
        return KitSource::Url;
    }
    KitSource::Local
}

/// Public entry-point with a pluggable trust gate, used by integration
/// tests that need to script the y/N answer. Production paths go
/// through [`kit`].
pub fn install(args: KitInstallArgs, db: &Db, trust: &mut impl TrustGate) -> Result<()> {
    match classify_source(&args.path) {
        KitSource::Url => {
            // The signed-registry hook is intentionally a stub for
            // slice 5 (see the bead's "future" callout). Until a
            // signed manifest format ships, URL installs are blocked
            // unconditionally — there's no safe code path that would
            // accept arbitrary remote DDL.
            anyhow::bail!(
                "URL kit installs are not yet supported. \
                 Download the kit to a local file and install from there. \
                 (signed-registry trust hook is a planned future feature.)"
            );
        }
        KitSource::Local => {}
    }

    let path_str = args.path.strip_prefix("file://").unwrap_or(&args.path);
    let path = Path::new(path_str);
    let src =
        fs::read_to_string(path).with_context(|| format!("read kit TOML at {}", path.display()))?;
    let kit = KitToml::from_str(&src)?;

    if kit.kit.name.trim().is_empty() {
        anyhow::bail!("kit TOML at {} is missing [kit].name", path.display());
    }

    // Trust gate runs BEFORE any DB writes so a "no" answer leaves the
    // DB untouched. Index DDL is also validated up front so the user
    // sees a malformed kit before being asked to approve it.
    if !kit.indices.is_empty() {
        for ix in &kit.indices {
            // Build the DDL early — surfaces validation errors before
            // we prompt. The string is recomputed inside install_kit.
            let _ = ix.build_ddl(&kit.kit.name)?;
        }
        let is_reinstall = !db.load_indices_by_kit(&kit.kit.name)?.is_empty();
        let approved =
            trust.approve_indices(&kit.kit.name, &kit.indices, is_reinstall, args.assume_yes)?;
        if !approved {
            anyhow::bail!(
                "kit '{}' install declined: index DDL not approved",
                kit.kit.name
            );
        }
    }

    // Convert each [[field]] entry into a stored FieldSpec, resolving any
    // user-supplied color at install time so the saved row carries hex. A
    // missing color stays `None` — the renderer derives one at paint time
    // via the datum-compatible `md5_rgb` (first three bytes of md5(name)).
    let mut specs = Vec::with_capacity(kit.fields.len());
    for entry in kit.fields {
        let resolved = resolve_color(entry.color.as_deref(), &entry.name)?;
        let entry_with_resolved = FieldEntry {
            color: resolved.clone(),
            ..entry
        };
        specs.push(entry_with_resolved.into_spec(resolved)?);
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

    let report = db.install_kit(&kit.kit.name, &specs, &kit.indices, args.force)?;
    // Views are independent of fields; installed in the same kit-install
    // pass so a TOML containing both lands atomically (per slice 4 bead).
    let view_report = db.install_kit_views(&kit.kit.name, &kit.views, args.force)?;
    // Aliases (slice 3): same pattern. Tagged with the kit's id so
    // uninstall removes only what this kit owns.
    let alias_report = db.install_kit_aliases(&kit.kit.name, &kit.aliases, args.force)?;
    // Slice 6: kit-defined commands. Validation runs first so a bad rule
    // surfaces before any partial-install of fields/views.
    let cmd_report = db.install_kit_commands(&kit.kit.name, &kit.commands, args.force)?;

    let installed: Vec<&str> = report.installed().collect();
    let skipped: Vec<&str> = report.skipped().collect();
    let views_installed: Vec<&str> = view_report.installed().collect();
    let views_skipped: Vec<&str> = view_report.skipped().collect();
    let aliases_installed: Vec<&str> = alias_report.installed().collect();
    let aliases_skipped: Vec<&str> = alias_report.skipped().collect();
    let cmds_installed: Vec<&str> = cmd_report.installed().collect();
    let cmds_skipped: Vec<&str> = cmd_report.skipped().collect();

    if !installed.is_empty() {
        println!(
            "Installed kit '{}': {} field(s)",
            kit.kit.name,
            installed.len()
        );
        for f in &installed {
            println!("  + {f}");
        }
    } else if kit.indices.is_empty()
        && views_installed.is_empty()
        && aliases_installed.is_empty()
        && cmds_installed.is_empty()
    {
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
    if !aliases_installed.is_empty() {
        println!("Installed {} alias(es):", aliases_installed.len());
        for a in &aliases_installed {
            println!("  + {a}");
        }
    }
    if !aliases_skipped.is_empty() {
        println!(
            "Skipped {} alias(es) (existing rows kept; use --force to overwrite):",
            aliases_skipped.len()
        );
        for a in &aliases_skipped {
            println!("  - {a}");
        }
    }
    if !report.indices_dropped.is_empty() {
        println!(
            "Replaced {} previously-installed index/indices.",
            report.indices_dropped.len()
        );
    }
    if !report.indices_installed.is_empty() {
        println!(
            "Installed {} index/indices for kit '{}':",
            report.indices_installed.len(),
            kit.kit.name
        );
        for ix in &report.indices_installed {
            println!("  + {ix}");
        }
    }
    if !cmds_installed.is_empty() {
        println!("Installed {} command(s):", cmds_installed.len());
        for c in &cmds_installed {
            println!("  + {c}");
        }
    }
    if !cmds_skipped.is_empty() {
        println!(
            "Skipped {} command(s) (existing rows kept; use --force to overwrite):",
            cmds_skipped.len()
        );
        for c in &cmds_skipped {
            println!("  - {c}");
        }
    }
    Ok(())
}

fn export(args: KitExportArgs, db: &Db) -> Result<()> {
    let specs = db.load_specs_by_kit(&args.name)?;
    let views = db.load_views_by_kit(&args.name)?;
    let aliases = db.load_aliases_by_kit(&args.name)?;
    let installed_indices = db.load_indices_by_kit(&args.name)?;
    let commands = db.load_commands_by_kit(&args.name)?;
    if specs.is_empty()
        && views.is_empty()
        && aliases.is_empty()
        && installed_indices.is_empty()
        && commands.is_empty()
    {
        anyhow::bail!("no installed kit named '{}'", args.name);
    }
    let view_count = views.len();
    let alias_count = aliases.len();
    let cmd_count = commands.len();
    let index_count = installed_indices.len();
    // Reconstruct the TOML-side `IndexEntry` shape from the persisted
    // SQL text. The on-disk DDL is what matters for re-install, and the
    // `sql_text` is preserved verbatim so users can audit it. Slice 5
    // serializes a synthetic shape rather than hand-parsing the SQL
    // back out; future slices can add a proper round-trip when the
    // TOML layer becomes the source of truth.
    let synthetic_indices: Vec<IndexEntry> = installed_indices
        .iter()
        .map(|row| IndexEntry {
            name: row.index_name.clone(),
            on: "entries".into(),
            expression: format!("/* persisted DDL: {} */", row.sql_text),
            where_clause: None,
            unique: false,
        })
        .collect();
    let kit = KitToml::from_kit_rows_full(
        &args.name,
        None,
        &specs,
        views,
        &aliases,
        &synthetic_indices,
        commands,
    );
    let toml = kit.to_toml()?;

    match args.path.as_deref() {
        Some(p) => {
            fs::write(p, &toml).with_context(|| format!("write kit TOML to {p}"))?;
            println!(
                "Exported kit '{}' ({} field(s), {} view(s), {} alias(es), {} index/indices, {} command(s)) → {p}",
                args.name,
                specs.len(),
                view_count,
                alias_count,
                index_count,
                cmd_count
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
    println!(
        "{:<24} {:>6}  {:>5}  {:>7}  {:>7}  {:>5}",
        "kit", "fields", "views", "aliases", "indices", "cmds"
    );
    for k in kits {
        println!(
            "{:<24} {:>6}  {:>5}  {:>7}  {:>7}  {:>5}",
            k.kit_id, k.field_count, k.view_count, k.alias_count, k.index_count, k.command_count
        );
    }
    Ok(())
}

fn uninstall(args: KitUninstallArgs, db: &Db) -> Result<()> {
    // Capture index ownership before the call so we can report it after
    // (uninstall_kit drops them inside its transaction).
    let indices_before = db.load_indices_by_kit(&args.name)?;
    let removed = db.uninstall_kit(&args.name)?;
    let removed_views = db.uninstall_kit_views(&args.name)?;
    let removed_aliases = db.uninstall_kit_aliases(&args.name)?;
    let removed_cmds = db.uninstall_kit_commands(&args.name)?;
    if removed.is_empty()
        && removed_views.is_empty()
        && removed_aliases.is_empty()
        && indices_before.is_empty()
        && removed_cmds.is_empty()
    {
        println!(
            "No fields, views, aliases, indices, or commands tagged with kit '{}'.",
            args.name
        );
    } else {
        println!(
            "Uninstalled kit '{}': {} field(s), {} view(s), {} alias(es), {} index/indices, {} command(s)",
            args.name,
            removed.len(),
            removed_views.len(),
            removed_aliases.len(),
            indices_before.len(),
            removed_cmds.len()
        );
        for f in &removed {
            println!("  - field {f}");
        }
        for v in &removed_views {
            println!("  - view  {v}");
        }
        for a in &removed_aliases {
            println!("  - alias {a}");
        }
        for ix in &indices_before {
            println!("  - index {}", ix.index_name);
        }
        for c in &removed_cmds {
            println!("  - cmd   {c}");
        }
    }
    Ok(())
}
