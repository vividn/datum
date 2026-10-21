//! README generation — the single implementation of "what the README's
//! generated regions should say".
//!
//! It lives in a library rather than in `main.rs` so that the anti-drift
//! test (`crates/datum-cli/tests/readme_anti_drift.rs`) can call the very
//! function `cargo xtask readme` writes with. A test that reimplemented the
//! generator would only prove the two reimplementations agree (da-ce7n3).

use anyhow::{bail, Context, Result};

use datum_parse::help::{render_markdown, ENV_VARS};

/// The regions of the docs that are generated rather than written, each with
/// the file it lives in (relative to the workspace root). Kept in sync with
/// `crates/datum-cli/tests/readme_anti_drift.rs`, which fails CI when a file
/// and the generator disagree.
///
/// `COMMANDS` was the only one for a while, and everything outside it went
/// on drifting: the README's 80-line alias section and `datum help alias`
/// reached the same false claim from opposite directions, and its env-var
/// table never learned about `DATUM_ID_DELIMITER` (da-ce7n3). All three lived
/// in `README.md` until the README became a short front page (2026-10-20);
/// each now sits in the docs page about its subject.
pub const AUTOGEN_BLOCKS: &[(&str, &str)] = &[
    ("docs/commands.md", "COMMANDS"),
    ("docs/aliases.md", "ALIASES"),
    ("docs/configuration.md", "ENV"),
];

fn begin_marker(block: &str) -> String {
    format!("<!-- BEGIN AUTOGEN: {block} -->")
}

fn end_marker(block: &str) -> String {
    format!("<!-- END AUTOGEN: {block} -->")
}

/// The generated body for one README block — the single place that
/// answers "what should this region say?", for both `cargo xtask readme`
/// and the test that checks it.
pub fn autogen_section(block: &str) -> Result<String> {
    match block {
        "COMMANDS" => Ok(autogen_commands_section()),
        "ALIASES" => Ok(autogen_alias_section()),
        "ENV" => Ok(autogen_env_section()),
        other => bail!("no generator for README block `{other}`"),
    }
}

/// The command surface.
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

/// The alias reference, straight out of the table that answers `datum help
/// alias`. Headings are demoted two levels so they nest under the README's
/// own `## Aliases`.
pub fn autogen_alias_section() -> String {
    render_markdown(Some("alias"))
        .lines()
        .map(|line| {
            if line.starts_with('#') {
                format!("##{line}\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect()
}

/// The environment-variable table. Widths are computed so the raw
/// Markdown stays readable in a terminal.
pub fn autogen_env_section() -> String {
    // Column widths in CHARACTERS, not bytes: `{:width$}` pads by chars,
    // and a summary carrying a `…` would otherwise push its own row two
    // columns past the rule under it.
    let width = |s: &str| s.chars().count();
    let name_col = ENV_VARS
        .iter()
        .map(|v| width(v.name) + 2)
        .chain(std::iter::once(width("Variable")))
        .max()
        .unwrap_or(0);
    let summary_col = ENV_VARS
        .iter()
        .map(|v| width(v.summary))
        .chain(std::iter::once(width("Purpose")))
        .max()
        .unwrap_or(0);
    let mut s = format!(
        "| {:name_col$} | {:summary_col$} |\n|{}|{}|\n",
        "Variable",
        "Purpose",
        "-".repeat(name_col + 2),
        "-".repeat(summary_col + 2),
    );
    for v in ENV_VARS {
        s.push_str(&format!(
            "| {:name_col$} | {:summary_col$} |\n",
            format!("`{}`", v.name),
            v.summary,
        ));
    }
    s
}

/// Replace the body between block `block`'s AUTOGEN markers with `body`.
/// Errors if either marker is missing or out of order.
pub fn splice_autogen(readme: &str, block: &str, body: &str) -> Result<String> {
    let begin_marker = begin_marker(block);
    let end_marker = end_marker(block);
    let begin = readme
        .find(&begin_marker)
        .with_context(|| format!("README missing marker `{begin_marker}`"))?;
    let end = readme
        .find(&end_marker)
        .with_context(|| format!("README missing marker `{end_marker}`"))?;
    if end < begin {
        bail!("README `{block}` autogen markers are out of order");
    }
    let after_begin = begin + begin_marker.len();
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
