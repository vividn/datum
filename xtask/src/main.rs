//! Workspace task runner — invoked via `cargo xtask <task>`.
//!
//! Tasks live here rather than in `build.rs` files so that they don't
//! run on every `cargo build` and so that they can depend on workspace
//! crates (a build script can't).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use datum_parse::help::{render_markdown, render_roff, COMMANDS};

/// Marker bracketing the auto-generated command list inside `README.md`.
/// Kept in sync with `crates/datum-cli/tests/readme_anti_drift.rs`.
const README_BEGIN: &str = "<!-- BEGIN AUTOGEN: COMMANDS -->";
const README_END: &str = "<!-- END AUTOGEN: COMMANDS -->";

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let task = args.next().unwrap_or_default();
    match task.as_str() {
        "manpages" => manpages(args.collect()),
        "readme" => readme(),
        "" => {
            print_usage();
            bail!("missing task name");
        }
        other => {
            print_usage();
            bail!("unknown task: {other}");
        }
    }
}

fn print_usage() {
    eprintln!("usage: cargo xtask <task>");
    eprintln!();
    eprintln!("tasks:");
    eprintln!("  manpages [DIR]   write `man/datum.1` and `man/datum-NAME.1` files");
    eprintln!("  readme           rewrite the autogen Commands section in README.md");
}

fn readme() -> Result<()> {
    let root = workspace_root()?;
    let readme_path = root.join("README.md");
    let original = fs::read_to_string(&readme_path)
        .with_context(|| format!("reading {}", readme_path.display()))?;
    let body = autogen_commands_section();
    let updated = splice_autogen(&original, &body)?;
    if updated == original {
        eprintln!("README.md already in sync");
        return Ok(());
    }
    fs::write(&readme_path, updated)
        .with_context(|| format!("writing {}", readme_path.display()))?;
    eprintln!("wrote {}", readme_path.display());
    Ok(())
}

/// The Markdown chunk that lives between the autogen markers in README.md.
///
/// `render_markdown(None)` emits a self-contained document beginning
/// `# datum\n\nPersonal data tracker.\n\n## Usage…` — for the README we
/// strip the title/tagline so the embedded section starts at `## Usage`.
pub fn autogen_commands_section() -> String {
    let full = render_markdown(None);
    let needle = "## Usage";
    match full.find(needle) {
        Some(idx) => full[idx..].to_string(),
        None => full,
    }
}

/// Replace the body between `README_BEGIN` and `README_END` with `body`.
/// Errors if either marker is missing or out of order.
pub fn splice_autogen(readme: &str, body: &str) -> Result<String> {
    let begin = readme
        .find(README_BEGIN)
        .with_context(|| format!("README missing marker `{README_BEGIN}`"))?;
    let end = readme
        .find(README_END)
        .with_context(|| format!("README missing marker `{README_END}`"))?;
    if end < begin {
        bail!("README autogen markers are out of order");
    }
    let after_begin = begin + README_BEGIN.len();
    let mut out = String::with_capacity(readme.len() + body.len());
    out.push_str(&readme[..after_begin]);
    out.push('\n');
    out.push_str(body);
    if !body.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&readme[end..]);
    Ok(out)
}

fn manpages(args: Vec<String>) -> Result<()> {
    let dir: PathBuf = match args.first() {
        Some(p) => PathBuf::from(p),
        None => workspace_root()?.join("man"),
    };
    fs::create_dir_all(&dir)
        .with_context(|| format!("creating output directory {}", dir.display()))?;

    write_page(&dir, "datum", &render_roff(None))?;
    for spec in COMMANDS {
        let stem = format!("datum-{}", spec.name);
        write_page(&dir, &stem, &render_roff(Some(spec.name)))?;
    }

    eprintln!(
        "wrote {} man pages to {}",
        1 + COMMANDS.len(),
        dir.display()
    );
    Ok(())
}

fn write_page(dir: &Path, stem: &str, contents: &str) -> Result<()> {
    let path = dir.join(format!("{stem}.1"));
    fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Locate the workspace root by walking up from `CARGO_MANIFEST_DIR`
/// until we find a `Cargo.toml` whose first non-comment line declares
/// `[workspace]`.
fn workspace_root() -> Result<PathBuf> {
    let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in start.ancestors() {
        let candidate = ancestor.join("Cargo.toml");
        if candidate.is_file() {
            let text = fs::read_to_string(&candidate)?;
            if text
                .lines()
                .any(|l| l.trim_start().starts_with("[workspace]"))
            {
                return Ok(ancestor.to_path_buf());
            }
        }
    }
    bail!("could not locate workspace root from {}", start.display())
}
