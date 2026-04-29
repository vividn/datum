//! Output adapters for the help data model.
//!
//! - [`render_help_text`] produces the legacy `--help` text (replaces
//!   the previous hand-written `top_level_help` / `command_help`).
//! - [`render_markdown`] produces a Markdown rendering for downstream
//!   docs (README, cargo doc).
//! - [`render_roff`] produces roff for man-page generation. The current
//!   implementation is a stub; the full implementation lands in da-docs.2.

use super::commands::{
    find_command, COMMANDS, GLOBAL_FLAGS, GLOBAL_FLAGS_COL, TOP_LEVEL_FOOTER, TOP_LEVEL_USAGE,
};
use super::spec::{Category, CommandSpec, FlagSpec};

/// Render the plain-text help string for `cmd`, or the top-level help
/// when `cmd` is `None`.
///
/// When the command name is unknown the function falls back to the
/// top-level help (matching the legacy behaviour of `command_help`).
pub fn render_help_text(cmd: Option<&str>) -> String {
    match cmd {
        None => render_top_level(),
        Some(name) => match find_command(name) {
            Some((spec, was_alias)) => render_command(spec, name, was_alias),
            None => render_top_level(),
        },
    }
}

// ---------------------------------------------------------------------------
// Top-level rendering — purely structural, driven by COMMANDS + GLOBAL_FLAGS.
// ---------------------------------------------------------------------------

fn render_top_level() -> String {
    let mut s = String::new();
    s.push_str("datum — personal data tracker\n");
    s.push_str("\nUSAGE:\n");
    s.push_str("  ");
    s.push_str(TOP_LEVEL_USAGE);
    s.push('\n');

    for cat in Category::order() {
        let in_section: Vec<&CommandSpec> =
            COMMANDS.iter().filter(|c| c.category == *cat).collect();
        if in_section.is_empty() {
            continue;
        }
        s.push('\n');
        s.push_str(cat.label());
        s.push_str(":\n");

        // Section name field: at least 9 chars wide, stretched if any
        // command name is long enough that it would otherwise leave less
        // than 2 spaces of padding (e.g. `conflicts` in VIEWS / SYNC).
        let max_name = in_section.iter().map(|c| c.name.len()).max().unwrap_or(0);
        let name_width = max_name + 2;
        let name_width = name_width.max(9);

        for spec in &in_section {
            let summary = spec
                .top_summary
                .map(String::from)
                .unwrap_or_else(|| capitalize_first(spec.summary));
            s.push_str("  ");
            s.push_str(spec.name);
            for _ in 0..(name_width - spec.name.len()) {
                s.push(' ');
            }
            s.push_str(&summary);
            s.push('\n');
        }
    }

    s.push_str("\nGLOBAL OPTIONS:\n");
    s.push_str(&render_flag_table(GLOBAL_FLAGS, GLOBAL_FLAGS_COL));
    s.push('\n');
    s.push_str(TOP_LEVEL_FOOTER);
    s.push('\n');
    s
}

// ---------------------------------------------------------------------------
// Per-command rendering — uses body_override when set, otherwise
// structurally renders from CommandSpec fields.
// ---------------------------------------------------------------------------

fn render_command(spec: &CommandSpec, input_name: &str, was_alias: bool) -> String {
    let displayed_name = if spec.display_input_name && was_alias {
        input_name
    } else {
        spec.name
    };

    let mut s = format!("datum {} — {}\n", displayed_name, spec.summary);

    s.push('\n');
    if let Some(body) = spec.body_override {
        s.push_str(body);
    } else {
        s.push_str(&render_command_body_structured(spec, displayed_name));
    }
    s
}

fn render_command_body_structured(spec: &CommandSpec, displayed_name: &str) -> String {
    let mut s = String::new();
    s.push_str("USAGE:\n");
    let canonical_prefix = format!("datum {}", spec.name);
    let displayed_prefix = format!("datum {}", displayed_name);
    for line in spec.usage {
        s.push_str("  ");
        if displayed_name == spec.name {
            s.push_str(line);
        } else {
            s.push_str(&line.replace(&canonical_prefix, &displayed_prefix));
        }
        s.push('\n');
    }
    if !spec.flags.is_empty() {
        s.push('\n');
        s.push_str("OPTIONS:\n");
        s.push_str(&render_flag_table(spec.flags, spec.summary_col));
    }
    s
}

// ---------------------------------------------------------------------------
// Shared flag-table renderer.
// ---------------------------------------------------------------------------

fn render_flag_table(flags: &[FlagSpec], col: usize) -> String {
    let mut s = String::new();
    for f in flags {
        let left = render_flag_left(f);
        let prefix_len = 2 + left.len();
        let pad = if prefix_len < col {
            col - prefix_len
        } else {
            2
        };
        s.push_str("  ");
        s.push_str(&left);
        for _ in 0..pad {
            s.push(' ');
        }
        s.push_str(f.summary);
        s.push('\n');
    }
    s
}

fn render_flag_left(f: &FlagSpec) -> String {
    if let Some(o) = f.left_override {
        return o.to_string();
    }
    let value_part = if f.takes_value {
        let hint = f.value_hint.unwrap_or("VAL");
        if f.value_optional {
            format!("[=<{hint}>]")
        } else {
            format!(" <{hint}>")
        }
    } else {
        String::new()
    };
    match (f.short, f.long.is_empty()) {
        (Some(s), true) => format!("-{s}{value_part}"),
        (Some(s), false) => format!("-{s}, --{}{value_part}", f.long),
        (None, false) => format!("    --{}{value_part}", f.long),
        (None, true) => panic!("FlagSpec must have at least short or long, or a left_override"),
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------
// Markdown rendering — structural, ignores body_override entirely.
// ---------------------------------------------------------------------------

/// Render Markdown for the top-level command surface (`cmd = None`) or
/// for one specific command.
pub fn render_markdown(cmd: Option<&str>) -> String {
    match cmd {
        None => render_markdown_top_level(),
        Some(name) => match find_command(name) {
            Some((spec, _)) => render_markdown_command(spec),
            None => render_markdown_top_level(),
        },
    }
}

fn render_markdown_top_level() -> String {
    let mut s = String::new();
    s.push_str("# datum\n\n");
    s.push_str("Personal data tracker.\n\n");
    s.push_str("## Usage\n\n```\n");
    s.push_str(TOP_LEVEL_USAGE);
    s.push_str("\n```\n");
    for cat in Category::order() {
        let in_section: Vec<&CommandSpec> =
            COMMANDS.iter().filter(|c| c.category == *cat).collect();
        if in_section.is_empty() {
            continue;
        }
        s.push_str(&format!("\n## {}\n\n", cat.label()));
        for spec in &in_section {
            let summary = spec
                .top_summary
                .map(String::from)
                .unwrap_or_else(|| capitalize_first(spec.summary));
            s.push_str(&format!("- `{}` — {}\n", spec.name, summary));
        }
    }
    s.push_str("\n## Global Options\n\n");
    for f in GLOBAL_FLAGS {
        s.push_str(&format!("- `{}` — {}\n", render_flag_left(f), f.summary));
    }
    s
}

fn render_markdown_command(spec: &CommandSpec) -> String {
    let mut s = String::new();
    s.push_str(&format!("# `datum {}`\n\n", spec.name));
    s.push_str(&format!("{}\n\n", capitalize_first(spec.summary)));
    if let Some(desc) = spec.long_description {
        s.push_str(desc);
        s.push_str("\n\n");
    }
    s.push_str("## Usage\n\n```\n");
    for line in spec.usage {
        s.push_str(line);
        s.push('\n');
    }
    s.push_str("```\n");
    if !spec.flags.is_empty() {
        s.push_str("\n## Options\n\n");
        for f in spec.flags {
            s.push_str(&format!("- `{}` — {}\n", render_flag_left(f), f.summary));
        }
    }
    if !spec.examples.is_empty() {
        s.push_str("\n## Examples\n\n");
        for ex in spec.examples {
            s.push_str(&format!("```\n{}\n```\n\n{}\n\n", ex.command, ex.description));
        }
    }
    s
}

// ---------------------------------------------------------------------------
// Roff (man-page) rendering — stub. Full implementation in da-docs.2.
// ---------------------------------------------------------------------------

/// Render roff source for a man page. This is currently a stub
/// returning a minimal `.TH` header so callers can integrate; the full
/// implementation lands in `da-docs.2`.
pub fn render_roff(cmd: Option<&str>) -> String {
    let name = cmd.unwrap_or("datum");
    format!(
        ".TH \"{}\" \"1\" \"\" \"\" \"datum manual\"\n.SH NAME\n{} \\- (man-page rendering not yet implemented; see da-docs.2)\n",
        name.to_uppercase(),
        name
    )
}
