//! `datum kit` — installable, exportable, shareable bundles of field specs
//! and (slice 5) custom indices.
//!
//! Subcommands:
//! - `install <path|-> [--force] [--yes] [--dry-run]` — parse a kit TOML
//!   (from stdin when the path is `-`) and upsert each `[[field]]` as a
//!   `spec` entry whose data carries `kit_id = <kit name>`. Existing specs
//!   for the same field win unless `--force` is passed. If the kit declares `[[index]]`
//!   blocks, the user is prompted for trust unless `--yes`. `--dry-run`
//!   prints what the kit brings and writes nothing. URLs are blocked until
//!   the signed-registry hook lands.
//! - `export <name> [<path>]` — round-trip the rows installed by this kit
//!   back to TOML. None path → stdout.
//! - `list` — installed kits with per-kit field/index counts.
//! - `show <name>` — one kit's installed contents, for reading (export is
//!   the TOML round-trip), with each index that fails to build here and why.
//!
//! A kit may also carry `[[layout]]` blocks (da-gg0v). Those do not go to
//! a table of their own: each becomes an ordinary entry of field `layout`
//! in THIS database, which is how a layout syncs with the data it belongs
//! to and how the web reads it beside the person's own — see
//! `datum_core::db::kit_layout`.
//! - `uninstall <name>` — drop every row tagged with this kit's id and
//!   `DROP INDEX` every kit-owned index.

use std::fs;
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::path::Path;

use std::collections::HashMap;

use anyhow::{Context, Result};
use datum_core::db::Db;
use datum_core::field_spec::{detect_anchor_cycles, format_cycle};
use datum_core::kit::{validate_layout_entry, FieldEntry, IndexEntry, KitToml};
use datum_core::kit_color::resolve_color;

use crate::args::{KitCommand, KitExportArgs, KitInstallArgs, KitUninstallArgs};
use crate::config::device_id;

pub fn kit(cmd: KitCommand, db: &Db) -> Result<()> {
    match cmd {
        KitCommand::Install(args) => install(args, db, &mut StdIoTrust),
        KitCommand::Export(args) => export(args, db),
        KitCommand::List => list(db),
        KitCommand::Uninstall(args) => uninstall(args, db),
        KitCommand::Show { name } => show(&name, db),
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
    /// `-`: the TOML on stdin, as `db restore -` reads its backup — what the
    /// browser, which has no filesystem, hands the file its dialog chose.
    Stdin,
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
    if s == "-" {
        return KitSource::Stdin;
    }
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
        KitSource::Stdin => {
            let mut src = String::new();
            io::stdin()
                .read_to_string(&mut src)
                .context("read kit TOML from stdin")?;
            return install_toml(&src, "stdin", args, db, trust);
        }
        KitSource::Local => {}
    }

    let path_str = args.path.strip_prefix("file://").unwrap_or(&args.path);
    let path = Path::new(path_str);
    let src =
        fs::read_to_string(path).with_context(|| format!("read kit TOML at {}", path.display()))?;
    install_toml(&src, &path.display().to_string(), args, db, trust)
}

/// Install the kit TOML `src`, read from `from` (a path, or `stdin`).
fn install_toml(
    src: &str,
    from: &str,
    args: KitInstallArgs,
    db: &Db,
    trust: &mut impl TrustGate,
) -> Result<()> {
    let kit = KitToml::from_str(src)?;

    if kit.kit.name.trim().is_empty() {
        anyhow::bail!("kit TOML at {from} is missing [kit].name");
    }

    // Trust gate runs BEFORE any DB writes so a "no" answer leaves the
    // DB untouched. Index DDL is also validated up front so the user
    // sees a malformed kit before being asked to approve it — and a dry
    // run, which asks nothing, still fails on it.
    for ix in &kit.indices {
        // The string is recomputed inside install_kit.
        let _ = ix.build_ddl(&kit.kit.name)?;
    }
    // Layouts are screened here too, so a kit whose tree is not JSON fails
    // before the trust prompt and before a dry run reports anything.
    for layout in &kit.layouts {
        let _ = validate_layout_entry(layout)?;
    }
    if !kit.indices.is_empty() && !args.dry_run {
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
    // A state's colour resolves the same way, seeded by the state's name as
    // the renderer's fallback for it is (da-uokdb).
    let mut specs = Vec::with_capacity(kit.fields.len());
    for mut entry in kit.fields {
        let resolved = resolve_color(entry.color.as_deref(), &entry.name)
            .with_context(|| format!("kit field '{}'", entry.name))?;
        for (state, s) in &mut entry.states {
            s.color = resolve_color(s.color.as_deref(), state)
                .with_context(|| format!("kit field '{}' state '{state}'", entry.name))?;
        }
        let entry_with_resolved = FieldEntry {
            color: resolved.clone(),
            ..entry
        };
        let mut spec = entry_with_resolved.into_spec(resolved)?;
        super::spec::check_presentation(&mut spec)
            .with_context(|| format!("kit field '{}'", spec.field))?;
        specs.push(spec);
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
        if let Some(anchor) = spec.presentation.relative_to.as_deref() {
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

    if args.dry_run {
        println!(
            "Would install kit '{}': {} field(s), {} alias(es), {} view(s), {} command(s), {} layout(s), {} index/indices",
            kit.kit.name,
            specs.len(),
            kit.aliases.len(),
            kit.views.len(),
            kit.commands.len(),
            kit.layouts.len(),
            kit.indices.len()
        );
        return Ok(());
    }

    db.set_device_id(&device_id()?);
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
    // Layouts (da-gg0v) land as ENTRIES in this very database, not in a
    // table of their own — so they sync with the data, and the web reads
    // them beside the person's own layouts in `_internal`.
    let layout_report =
        db.install_kit_layouts(&kit.kit.name, &kit.layouts, &device_id()?, args.force)?;

    let installed: Vec<&str> = report.installed().collect();
    let skipped: Vec<&str> = report.skipped().collect();
    let views_installed: Vec<&str> = view_report.installed().collect();
    let views_skipped: Vec<&str> = view_report.skipped().collect();
    let aliases_installed: Vec<&str> = alias_report.installed().collect();
    let aliases_skipped: Vec<&str> = alias_report.skipped().collect();
    let cmds_installed: Vec<&str> = cmd_report.installed().collect();
    let cmds_skipped: Vec<&str> = cmd_report.skipped().collect();
    let layouts_installed: Vec<&str> = layout_report.installed().collect();
    let layouts_skipped: Vec<&str> = layout_report.skipped().collect();

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
        && layouts_installed.is_empty()
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
    if !layouts_installed.is_empty() {
        println!("Installed {} layout(s):", layouts_installed.len());
        for l in &layouts_installed {
            println!("  + {l}");
        }
    }
    if !layouts_skipped.is_empty() {
        println!(
            "Skipped {} layout(s) (existing entries kept; use --force to overwrite):",
            layouts_skipped.len()
        );
        for l in &layouts_skipped {
            println!("  - {l}");
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
    let layouts = db.load_layouts_by_kit(&args.name)?;
    if specs.is_empty()
        && views.is_empty()
        && aliases.is_empty()
        && installed_indices.is_empty()
        && commands.is_empty()
        && layouts.is_empty()
    {
        anyhow::bail!("no installed kit named '{}'", args.name);
    }
    let view_count = views.len();
    let alias_count = aliases.len();
    let cmd_count = commands.len();
    let layout_count = layouts.len();
    let index_count = installed_indices.len();
    let indices: Vec<IndexEntry> = installed_indices.into_iter().map(|row| row.entry).collect();
    let kit = KitToml::from_kit_rows_full(
        &args.name, None, &specs, views, &aliases, &indices, commands, layouts,
    );
    let toml = kit.to_toml()?;

    match args.path.as_deref() {
        Some(p) => {
            fs::write(p, &toml).with_context(|| format!("write kit TOML to {p}"))?;
            println!(
                "Exported kit '{}' ({} field(s), {} view(s), {} alias(es), {} index/indices, {} command(s), {} layout(s)) → {p}",
                args.name,
                specs.len(),
                view_count,
                alias_count,
                index_count,
                cmd_count,
                layout_count
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
        "{:<24} {:>6}  {:>5}  {:>7}  {:>7}  {:>5}  {:>7}",
        "kit", "fields", "views", "aliases", "indices", "cmds", "layouts"
    );
    for k in kits {
        println!(
            "{:<24} {:>6}  {:>5}  {:>7}  {:>7}  {:>5}  {:>7}",
            k.kit_id,
            k.field_count,
            k.view_count,
            k.alias_count,
            k.index_count,
            k.command_count,
            k.layout_count
        );
    }
    Ok(())
}

fn show(name: &str, db: &Db) -> Result<()> {
    let specs = db.load_specs_by_kit(name)?;
    let aliases = db.load_aliases_by_kit(name)?;
    let views = db.load_views_by_kit(name)?;
    let commands = db.load_commands_by_kit(name)?;
    let layouts = db.load_layouts_by_kit(name)?;
    let indices = db.kit_index_status(name)?;
    if specs.is_empty()
        && aliases.is_empty()
        && views.is_empty()
        && commands.is_empty()
        && layouts.is_empty()
        && indices.is_empty()
    {
        anyhow::bail!("no installed kit named '{name}'");
    }
    // The DDL of each index that validates, for the line that names it.
    let built = db.load_indices_by_kit(name)?;
    println!("kit '{name}'");
    section(
        "fields",
        specs.iter().map(|s| {
            let color = s.color.as_deref().unwrap_or("(auto)");
            format!("{:<16} {:<7} {color}", s.field, s.verb.to_string())
        }),
    );
    section(
        "aliases",
        aliases
            .iter()
            .map(|a| format!("{:<16} {}", a.name, a.expands_to)),
    );
    section("views", views.iter().map(|v| v.name.clone()));
    section(
        "commands",
        commands
            .iter()
            .map(|c| format!("{:<16} {}", c.name, c.help.as_deref().unwrap_or(""))),
    );
    section("layouts", layouts.iter().map(|l| l.name.clone()));
    section(
        "indices",
        indices.iter().map(|(ix, err)| {
            let ddl = built
                .iter()
                .find(|b| b.index_name == *ix)
                .map_or_else(|| ix.clone(), |b| summarize_index(&b.entry));
            match err {
                None => format!("✓ {ddl}"),
                Some(e) => format!("✗ {ddl}\n      not built on this device: {e}"),
            }
        }),
    );
    Ok(())
}

/// One `kit show` section: its title and count, then a line per item — or
/// nothing, for a kind the kit does not carry.
fn section(title: &str, lines: impl ExactSizeIterator<Item = String>) {
    if lines.len() == 0 {
        return;
    }
    println!("{title} ({}):", lines.len());
    for line in lines {
        println!("  {}", line.trim_end());
    }
}

fn uninstall(args: KitUninstallArgs, db: &Db) -> Result<()> {
    // Capture index ownership before the call so we can report it after
    // (uninstall_kit removes the definitions and drops the indices).
    let indices_before = db.load_indices_by_kit(&args.name)?;
    let removed = db.uninstall_kit(&args.name)?;
    // A logged field gets back the bare spec its first log wrote, which a
    // kit install may have replaced (da-1mkej).
    for field in &removed {
        if db.field_has_entries(field)? {
            super::helpers::save_track_spec(db, field, db.field_verb(field)?)?;
        }
    }
    let removed_views = db.uninstall_kit_views(&args.name)?;
    let removed_aliases = db.uninstall_kit_aliases(&args.name)?;
    let removed_cmds = db.uninstall_kit_commands(&args.name)?;
    // A layout is an entry, so its removal is a SOFT delete — the deletion
    // has to reach the other devices of this database (da-gg0v).
    let removed_layouts = db.uninstall_kit_layouts(&args.name)?;
    if removed.is_empty()
        && removed_views.is_empty()
        && removed_aliases.is_empty()
        && indices_before.is_empty()
        && removed_cmds.is_empty()
        && removed_layouts.is_empty()
    {
        println!(
            "No fields, views, aliases, indices, commands, or layouts tagged with kit '{}'.",
            args.name
        );
    } else {
        println!(
            "Uninstalled kit '{}': {} field(s), {} view(s), {} alias(es), {} index/indices, {} command(s), {} layout(s)",
            args.name,
            removed.len(),
            removed_views.len(),
            removed_aliases.len(),
            indices_before.len(),
            removed_cmds.len(),
            removed_layouts.len()
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
        for l in &removed_layouts {
            println!("  - layout {l}");
        }
    }
    Ok(())
}
