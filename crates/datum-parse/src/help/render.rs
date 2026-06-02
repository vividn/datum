//! Output adapters for the help data model.
//!
//! - [`render_help_text`] produces the legacy `--help` text (replaces
//!   the previous hand-written `top_level_help` / `command_help`).
//! - [`render_markdown`] produces a Markdown rendering for downstream
//!   docs (README, cargo doc).
//! - [`render_roff`] produces roff for man-page generation; see the
//!   roff section of this module for the formatting conventions.

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
    let options = render_options_block(spec.flags, spec.summary_col);
    if !options.is_empty() {
        s.push('\n');
        s.push_str(&options);
    }
    s
}

// ---------------------------------------------------------------------------
// Shared flag-table renderer.
// ---------------------------------------------------------------------------

/// Render the `OPTIONS:` block for a flag table — the `OPTIONS:` heading
/// line followed by the aligned flag rows from [`render_flag_table`].
///
/// Returns the empty string when `flags` is empty, so callers can decide
/// whether to emit the leading blank line that separates it from `USAGE:`.
///
/// This is the single structural source of truth for a command's `OPTIONS`
/// section: a command opts into generated options by leaving `body_override`
/// unset (rendered fully structurally, like `tail`), so that adding a flag
/// to its `FlagSpec` table is the *only* thing needed to update its `--help`.
/// `col` is the command's [`CommandSpec::summary_col`].
pub fn render_options_block(flags: &[FlagSpec], col: usize) -> String {
    if flags.is_empty() {
        return String::new();
    }
    let mut s = String::from("OPTIONS:\n");
    s.push_str(&render_flag_table(flags, col));
    s
}

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
            s.push_str(&format!(
                "```\n{}\n```\n\n{}\n\n",
                ex.command, ex.description
            ));
        }
    }
    s
}

// ---------------------------------------------------------------------------
// Roff (man-page) rendering.
// ---------------------------------------------------------------------------
//
// Conventions follow `man-pages(7)`:
//   .TH  — title header (NAME SECTION DATE SOURCE MANUAL)
//   .SH  — section heading (uppercase)
//   .SS  — subsection
//   .TP  — tagged paragraph (next line is the tag, then body)
//   .B   — bold line
//   .I   — italic line
//   \-   — literal ASCII hyphen (preserves `-` in copy/paste)
//
// `cmd = None` produces the top-level `datum(1)` man page; `cmd =
// Some(name)` produces the per-subcommand `datum-NAME(1)` page. Unknown
// names fall back to the top-level page (matches the help-text and
// markdown renderers).

use super::DATUM_VERSION;

/// Render roff source for the `datum(1)` man page (`cmd = None`) or for
/// a specific subcommand (`cmd = Some("tail")` → `datum-tail(1)`).
pub fn render_roff(cmd: Option<&str>) -> String {
    match cmd {
        None => render_roff_top_level(),
        Some(name) => match find_command(name) {
            Some((spec, _)) => render_roff_command(spec),
            None => render_roff_top_level(),
        },
    }
}

fn render_roff_top_level() -> String {
    let mut s = String::new();
    s.push_str(&render_th("DATUM", 1));
    s.push_str(".SH NAME\ndatum \\- personal data tracker\n");
    // TOP_LEVEL_USAGE already starts with "datum"; emit the first word
    // bold and the remainder as plain text on the same logical line.
    s.push_str(".SH SYNOPSIS\n");
    let (first, rest) = TOP_LEVEL_USAGE
        .split_once(' ')
        .unwrap_or((TOP_LEVEL_USAGE, ""));
    s.push_str(&format!(".B {}\n", roff_escape(first)));
    if !rest.is_empty() {
        s.push_str(&format!("{}\n", roff_escape(rest)));
    }
    s.push_str(".SH DESCRIPTION\n");
    s.push_str(
        "datum is a local-first personal data tracker. \
         Entries are logged from the command line and stored in a local \
         SQLite database; an optional encrypted relay synchronises devices.\n",
    );

    s.push_str(".SH COMMANDS\n");
    for cat in Category::order() {
        let in_section: Vec<&CommandSpec> =
            COMMANDS.iter().filter(|c| c.category == *cat).collect();
        if in_section.is_empty() {
            continue;
        }
        s.push_str(&format!(".SS {}\n", roff_escape(cat.label())));
        for spec in &in_section {
            let summary = spec
                .top_summary
                .map(String::from)
                .unwrap_or_else(|| capitalize_first(spec.summary));
            s.push_str(".TP\n");
            s.push_str(&format!(".B {}\n", roff_escape(spec.name)));
            s.push_str(&format!("{}\n", roff_escape(&summary)));
        }
    }

    s.push_str(".SH GLOBAL OPTIONS\n");
    s.push_str(&render_roff_flag_table(GLOBAL_FLAGS));

    s.push_str(".SH SEE ALSO\n");
    let names: Vec<String> = COMMANDS
        .iter()
        .map(|c| format!(".BR datum\\-{} (1)", roff_escape(c.name)))
        .collect();
    s.push_str(&names.join(",\n"));
    s.push('\n');

    s
}

fn render_roff_command(spec: &CommandSpec) -> String {
    let mut s = String::new();
    let title = format!("DATUM-{}", spec.name.to_ascii_uppercase());
    s.push_str(&render_th(&title, 1));
    s.push_str(&format!(
        ".SH NAME\ndatum\\-{} \\- {}\n",
        roff_escape(spec.name),
        roff_escape(spec.summary),
    ));

    s.push_str(".SH SYNOPSIS\n");
    for line in spec.usage {
        s.push_str(&format!("{}\n.br\n", roff_escape(line)));
    }

    if let Some(desc) = spec.long_description {
        s.push_str(".SH DESCRIPTION\n");
        s.push_str(&roff_escape(desc));
        s.push('\n');
    }

    if !spec.flags.is_empty() {
        s.push_str(".SH OPTIONS\n");
        s.push_str(&render_roff_flag_table(spec.flags));
    }

    if !spec.examples.is_empty() {
        s.push_str(".SH EXAMPLES\n");
        for ex in spec.examples {
            s.push_str(".TP\n");
            s.push_str(&format!(".B {}\n", roff_escape(ex.command)));
            s.push_str(&format!("{}\n", roff_escape(ex.description)));
        }
    }

    s.push_str(".SH SEE ALSO\n.BR datum (1)\n");
    s
}

fn render_th(name: &str, section: u8) -> String {
    format!(
        ".TH \"{}\" \"{}\" \"\" \"datum {}\" \"datum manual\"\n",
        name, section, DATUM_VERSION
    )
}

fn render_roff_flag_table(flags: &[FlagSpec]) -> String {
    let mut s = String::new();
    for f in flags {
        s.push_str(".TP\n");
        s.push_str(&format!(".B {}\n", roff_escape(&render_flag_left(f))));
        s.push_str(&format!("{}\n", roff_escape(f.summary)));
    }
    s
}

/// Escape user-supplied text for safe inclusion as roff source.
///
/// - `\` is doubled so groff doesn't interpret it as an escape introducer.
/// - `-` becomes `\-` so copy/paste from a rendered man page yields the
///   ASCII hyphen rather than the Unicode hyphen-minus.
/// - A leading `.` or `'` would otherwise be interpreted as a request;
///   prefix with `\&` to neuter it.
fn roff_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    if matches!(chars.peek(), Some('.') | Some('\'')) {
        out.push_str("\\&");
    }
    for c in chars {
        match c {
            '\\' => out.push_str("\\\\"),
            '-' => out.push_str("\\-"),
            _ => out.push(c),
        }
    }
    out
}
