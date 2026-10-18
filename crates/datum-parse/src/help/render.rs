//! Output adapters for the help data model.
//!
//! - [`render_help_text`] produces the legacy `--help` text (replaces
//!   the previous hand-written `top_level_help` / `command_help`).
//! - [`render_markdown`] produces a Markdown rendering for downstream
//!   docs (README, cargo doc).
//! - [`render_roff`] produces roff for man-page generation; see the
//!   roff section of this module for the formatting conventions.

use super::commands::{
    find_command, find_topic, page_name, starts_with_words, COMMANDS, ENV_VARS, GLOBAL_FLAGS,
    GLOBAL_FLAGS_COL, TOP_LEVEL_EXAMPLES, TOP_LEVEL_FOOTER, TOP_LEVEL_USAGE,
};
use super::spec::{Category, CommandSpec, FlagSpec, TopicSpec};
use crate::input::output_args::{shows_for, Show};

/// The output family as `--help` lists it for one command: `(left, summary)`.
///
/// `--show` and its `--json` shorthand are read by the shared argument
/// layer for every command (da-qt6z), so they are documented from here
/// once rather than repeated in thirteen hand-written flag tables. The
/// values listed are the ones that command actually draws — the same
/// table the parser rejects an unsupported value against, so `--help` and
/// the error message cannot disagree.
fn output_rows(command: &str) -> Vec<(String, String)> {
    let shows = shows_for(command);
    let mut rows = vec![(
        "    --show <MODE>".to_string(),
        shows.iter().map(|s| s.name()).collect::<Vec<_>>().join("|"),
    )];
    if shows.contains(&Show::Json) {
        rows.push((
            "    --json".to_string(),
            "Shorthand for --show json".to_string(),
        ));
    }
    rows
}

/// Render the plain-text help string for `cmd`, or the top-level help
/// when `cmd` is `None`.
///
/// When the command name is unknown the function falls back to the
/// top-level help (matching the legacy behaviour of `command_help`).
pub fn render_help_text(cmd: Option<&str>) -> String {
    match cmd {
        None => render_top_level(),
        // Read again as the parser reads it, so a word naming no
        // subcommand (`db nope`) is the parent's page.
        Some(name) => match name.split(' ').collect::<Vec<_>>().split_first() {
            Some((cmd, words)) if !words.is_empty() => {
                let page = page_name(cmd, words);
                match find_command(&page) {
                    Some((spec, _)) if page.contains(' ') => render_subcommand(spec, &page),
                    _ => render_page(&page),
                }
            }
            _ => render_page(name),
        },
    }
}

fn render_page(name: &str) -> String {
    match find_command(name) {
        Some((spec, was_alias)) => render_command(spec, name, was_alias),
        // `datum help logging` is not a command but is a page; only
        // a word that is neither falls back to the top level, saying
        // so — the CLI answers an alias or a kit command before this
        // (da-3odno), so a miss must not read as the page it asked for.
        None => match find_topic(name) {
            Some(topic) => render_topic(topic),
            None => format!(
                "datum help: no command, topic or alias named '{name}'\n\n{}",
                render_top_level()
            ),
        },
    }
}

/// `datum db purge --help` (da-2bb7z): the subcommand's usage lines, then
/// what its parent's page says of it, found there by name rather than
/// written twice — a paragraph that opens with it (`Purge leaves…`), its row
/// in a table (`  purge       …`), or, for `sync chain`, its section.
fn render_subcommand(spec: &CommandSpec, page: &str) -> String {
    let words: Vec<&str> = page.split(' ').skip(1).collect();
    let full = format!("datum {} {}", spec.name, words.join(" "));
    let mut s = format!("{full}\n\nUSAGE:\n");
    for sub in spec.subcommands.iter().filter(|s| s.name == words[0]) {
        for u in sub.usage {
            if words.len() == 1 || starts_with_words(u, &full) {
                s.push_str(&format!("  {u}\n"));
            }
        }
    }
    for part in said_of(spec.body_override.unwrap_or(""), &words) {
        s.push('\n');
        s.push_str(&part);
        s.push('\n');
    }
    s.push_str(&format!(
        "\nAll of `datum {0}`: datum help {0}\n",
        spec.name
    ));
    s
}

/// What a command's page says of one of its subcommands: `words` is its
/// path below the command, `chain purge` for `sync chain purge`.
fn said_of(body: &str, words: &[&str]) -> Vec<String> {
    let paras: Vec<&str> = body.split("\n\n").map(|p| p.trim_end()).collect();
    // A section is a paragraph headed `chain — its devices:`.
    let section_of = |p: &str, w: &str| p.starts_with(&format!("{w} — "));
    let w = words[words.len() - 1];
    if words.len() == 1 {
        if let Some(p) = paras.iter().find(|p| section_of(p, w)) {
            return vec![p.to_string()];
        }
    }
    let mut out = Vec::new();
    for p in &paras {
        // A one-word page reads outside the sections (`daemon — …:`), where
        // a word of the same name is another subcommand's; a deeper one
        // reads inside its parent's section alone.
        let in_scope = match words {
            [_] => !p
                .lines()
                .next()
                .is_some_and(|l| l.contains(" — ") && l.ends_with(':')),
            [parent, ..] => section_of(p, parent),
            [] => false,
        };
        if !in_scope {
            continue;
        }
        let first = p.split_whitespace().next().unwrap_or("").trim_matches('`');
        if words.len() == 1 && first.eq_ignore_ascii_case(w) {
            out.push(p.to_string());
            continue;
        }
        // A table row, `  purge       …`, and the lines indented under it.
        let row = format!("  {w}  ");
        let mut lines = p.lines().skip_while(|l| !l.starts_with(&row));
        if let Some(head) = lines.next() {
            let mut r = head.to_string();
            for l in lines.take_while(|l| l.starts_with("   ")) {
                r.push('\n');
                r.push_str(l);
            }
            out.push(r);
        }
    }
    out
}

/// Render a `datum help <TOPIC>` page — the one place an option wall
/// shared by several commands is written out (da-ce7n3).
fn render_topic(topic: &TopicSpec) -> String {
    let mut s = format!("datum help {} — {}\n\n", topic.name, topic.summary);
    s.push_str(topic.intro);
    s.push('\n');
    s.push_str(&render_options_block(topic.flags, topic.summary_col));
    for group in topic.forms {
        s.push_str(&format!("\n{}:\n", group.heading));
        for f in group.forms {
            let row = format!("  {:<26}  {}", f.example, f.meaning);
            s.push_str(row.trim_end());
            s.push('\n');
        }
    }
    if !topic.env.is_empty() {
        s.push_str("ENVIRONMENT:\n");
        for v in topic.env {
            let pad = topic.summary_col.saturating_sub(2 + v.name.len()).max(2);
            s.push_str(&format!("  {}{:pad$}{}\n", v.name, "", v.summary));
        }
    }
    s.push_str(topic.outro);
    s
}

/// The command table as JSON — `datum help --json` (da-r13j).
///
/// Each command's name, aliases, usage lines, flags (each with its
/// one-line help, whether it takes a value, da-st2xl, and whether that
/// value is optional, da-e9e5g) and subcommands,
/// straight out of [`COMMANDS`], for a caller that asks the grammar rather
/// than keeping a copy of it: the web's commands widget reads which
/// commands take an id or `-t` from here, and which drill into
/// subcommands. `bare` is whether the command runs with nothing after its
/// name (`sync` does, `db` does not), asked of the parser itself.
pub fn render_json() -> String {
    let flags = |flags: &[FlagSpec]| {
        flags
            .iter()
            .map(|f| {
                serde_json::json!({
                    "short": f.short.map(String::from),
                    "long": f.long,
                    "takes_value": f.takes_value,
                    "value_optional": f.value_optional,
                    "help": f.summary,
                })
            })
            .collect::<Vec<_>>()
    };
    let commands: Vec<_> = COMMANDS
        .iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "aliases": c.aliases,
                "usage": c.usage,
                "flags": flags(c.flags),
                "subcommands": c.subcommands.iter().map(|s| serde_json::json!({
                    "name": s.name,
                    "usage": s.usage,
                    "flags": flags(s.flags),
                })).collect::<Vec<_>>(),
                "bare": crate::parse_argv(&["datum".into(), c.name.into()]).is_ok(),
            })
        })
        .collect();
    format!("{}\n", serde_json::Value::from(commands))
}

/// The headings whose sections are what the curious and the keyboarders
/// look up — the usage forms and the flag tables — and not what a page is
/// read for: datum.day folds them away (da-efruz).
const REFERENCE: &[&str] = &["USAGE", "ARGS", "OPTIONS", "OUTPUT", "GLOBAL OPTIONS"];

/// A section heading of a text page: `OPTIONS:`, `TIME OF DAY (today):` —
/// a line at the margin ending in `:` whose words before any `(` are capitals.
pub(super) fn heading(line: &str) -> Option<&str> {
    let h = line.strip_suffix(':').filter(|_| !line.starts_with(' '))?;
    let caps = h.split('(').next().unwrap_or(h);
    let upper = caps.chars().filter(char::is_ascii_uppercase).count();
    (upper >= 2 && !caps.chars().any(char::is_lowercase)).then_some(h)
}

/// One help page as JSON — `datum help <NAME> --json` (da-efruz): the
/// page's own first line (`title`), the prose before its first heading
/// (`intro`), then its `sections` in order, each a `heading` and its
/// `text`. Prose at the margin after a heading's indented lines is a
/// section of its own with no heading. `reference` marks the usage forms
/// and flag tables. Read off the text page, so every page has one: a
/// topic, a subcommand, an alias or a kit command's too.
pub fn render_page_json(text: &str) -> String {
    let (title, body) = text.split_once('\n').unwrap_or((text, ""));
    let (intro, sections) = sections_of(body);
    let join = |lines: &[&str]| lines.join("\n").trim_matches('\n').to_string();
    serde_json::json!({
        "title": title,
        "intro": join(&intro),
        "sections": sections.iter().map(|(h, lines)| serde_json::json!({
            "heading": h,
            "text": join(lines),
            "reference": REFERENCE.contains(h),
        })).collect::<Vec<_>>(),
    })
    .to_string()
        + "\n"
}

/// A text page's body split as [`render_page_json`] reads it: the lines
/// before its first heading, then each section's heading and lines.
type Section<'a> = (&'a str, Vec<&'a str>);
fn sections_of(body: &str) -> (Vec<&str>, Vec<Section<'_>>) {
    let mut intro: Vec<&str> = Vec::new();
    let mut sections: Vec<Section> = Vec::new();
    for line in body.lines() {
        match (heading(line), sections.last_mut()) {
            (Some(h), _) => sections.push((h, Vec::new())),
            (None, None) => intro.push(line),
            (None, Some((h, lines))) => {
                let prose = !h.is_empty() && !line.is_empty() && !line.starts_with(' ');
                if prose && lines.iter().any(|l| !l.is_empty()) {
                    sections.push(("", vec![line]));
                } else {
                    lines.push(line);
                }
            }
        }
    }
    (intro, sections)
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
    s.push_str("\nEXAMPLES:\n");
    s.push_str(TOP_LEVEL_EXAMPLES);

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
            let summary = capitalize_first(spec.summary);
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

    // Under its alias the summary's «(alias: end)» would name the page
    // itself, so it names the canonical command instead.
    let summary = if displayed_name == spec.name {
        spec.summary.to_string()
    } else {
        spec.summary.replace(
            &format!("(alias: {displayed_name})"),
            &format!("(also: {})", spec.name),
        )
    };
    let mut s = format!("datum {} — {}\n", displayed_name, summary);

    s.push('\n');
    s.push_str(&page_text(spec, displayed_name));
    let rows = output_rows(spec.name);
    // The block keeps the command's own summary column where it fits, and
    // widens for a command whose OPTIONS table is narrower than `--show
    // <MODE>` — the two rows always line up with each other.
    let col = rows
        .iter()
        .map(|(left, _)| 2 + left.len() + 2)
        .max()
        .unwrap_or(0)
        .max(spec.summary_col);
    s.push_str("\nOUTPUT:\n");
    for (left, summary) in rows {
        s.push_str("  ");
        s.push_str(&left);
        for _ in 0..(col - 2 - left.len()) {
            s.push(' ');
        }
        s.push_str(&summary);
        s.push('\n');
    }
    s
}

/// A command's `--help` between its title and OUTPUT, which a
/// body_override command's man page carries too (da-nvpnh).
fn page_text(spec: &CommandSpec, displayed_name: &str) -> String {
    let mut s = String::new();
    if !spec.intro.is_empty() {
        s.push_str(spec.intro);
        s.push('\n');
    }
    if let Some(body) = spec.body_override {
        s.push_str(body);
    } else {
        s.push_str(&render_command_body_structured(spec, displayed_name));
    }
    if spec.name == "config" {
        s.push_str(&config_keys());
    }
    s
}

/// `datum config --help`'s KEYS: each `datumrc.toml` key, the variable
/// that overrides it and what it does, from [`ENV_VARS`] (da-6szdc).
fn config_keys() -> String {
    let keys: Vec<_> = ENV_VARS.iter().filter(|v| !v.key.is_empty()).collect();
    let kw = keys.iter().map(|v| v.key.len()).max().unwrap_or(0);
    let nw = keys.iter().map(|v| v.name.len()).max().unwrap_or(0);
    let mut s = String::from("KEYS:\n");
    for v in keys {
        s.push_str(&format!("  {:kw$}  {:nw$}  {}\n", v.key, v.name, v.summary));
    }
    s.push_str("\nEvery variable datum reads: datum help env\n");
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
    if !spec.args.is_empty() {
        s.push_str("\nARGS:\n");
        for a in spec.args {
            let left = format!("  {}", a.name);
            let pad = spec.summary_col.saturating_sub(left.len()).max(2);
            s.push_str(&format!("{left}{}{}\n", " ".repeat(pad), a.summary));
        }
    }
    let options = match spec.shared_flags {
        // The wall lives on `datum help <topic>`; print only what is
        // particular to this command, then the pointer.
        Some(shared) => {
            let mut block = render_options_block(shared.own, spec.summary_col);
            if block.is_empty() {
                block.push_str("OPTIONS:\n");
            }
            block.push_str(&format!(
                "  common {} options: `datum help {}`\n",
                shared.topic, shared.topic
            ));
            if !shared.binders.is_empty() {
                block.push_str(&format!(
                    "    including {} for values with spaces\n",
                    shared.binders
                ));
            }
            block
        }
        None => render_options_block(spec.flags, spec.summary_col),
    };
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
    s.push_str("\n```\n\n## Examples\n\n```\n");
    s.push_str(TOP_LEVEL_EXAMPLES);
    s.push_str("```\n");
    for cat in Category::order() {
        let in_section: Vec<&CommandSpec> =
            COMMANDS.iter().filter(|c| c.category == *cat).collect();
        if in_section.is_empty() {
            continue;
        }
        s.push_str(&format!("\n## {}\n\n", cat.label()));
        for spec in &in_section {
            let summary = capitalize_first(spec.summary);
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
    s.push_str("\n## Output\n\n");
    for (left, summary) in output_rows(spec.name) {
        s.push_str(&format!("- `{}` — {}\n", left.trim(), summary));
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
         SQLite database.\n",
    );
    s.push_str(".SH EXAMPLES\n");
    s.push_str(&roff_block(&TOP_LEVEL_EXAMPLES.lines().collect::<Vec<_>>()));

    s.push_str(".SH COMMANDS\n");
    for cat in Category::order() {
        let in_section: Vec<&CommandSpec> =
            COMMANDS.iter().filter(|c| c.category == *cat).collect();
        if in_section.is_empty() {
            continue;
        }
        s.push_str(&format!(".SS {}\n", roff_escape(cat.label())));
        for spec in &in_section {
            let summary = capitalize_first(spec.summary);
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

    // A body_override is what `--help` prints, so the page carries all of
    // it (da-nvpnh): its USAGE is the synopsis, its other headings are
    // sections, and the prose outside them is the description.
    let text = match spec.body_override {
        Some(_) => page_text(spec, spec.name),
        None => String::new(),
    };
    let (intro, sections) = sections_of(&text);
    s.push_str(".SH SYNOPSIS\n");
    match sections.iter().find(|(h, _)| *h == "USAGE") {
        Some((_, lines)) => s.push_str(&roff_block(lines)),
        None => {
            for line in spec.usage {
                s.push_str(&format!("{}\n.br\n", roff_escape(line)));
            }
        }
    }

    let mut described = false;
    if let Some(desc) = spec.long_description {
        s.push_str(".SH DESCRIPTION\n");
        s.push_str(&roff_escape(desc));
        s.push('\n');
        described = true;
    }
    let mut prose = vec![("", intro)];
    prose.extend(sections.into_iter().filter(|(h, _)| *h != "USAGE"));
    for (h, lines) in &prose {
        if !h.is_empty() {
            s.push_str(&format!(".SH {}\n", roff_escape(h)));
            described = true;
        } else if lines.iter().any(|l| !l.is_empty()) {
            s.push_str(if described {
                ".PP\n"
            } else {
                ".SH DESCRIPTION\n"
            });
            described = true;
        }
        s.push_str(&roff_block(lines));
    }

    if spec.body_override.is_none() && !spec.flags.is_empty() {
        s.push_str(".SH OPTIONS\n");
        s.push_str(&render_roff_flag_table(spec.flags));
    }

    s.push_str(".SH OUTPUT\n");
    for (left, summary) in output_rows(spec.name) {
        s.push_str(".TP\n");
        s.push_str(&format!(".B {}\n", roff_escape(left.trim())));
        s.push_str(&format!("{}\n", roff_escape(&summary)));
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

/// A text page's lines as roff: prose at the margin is filled, a blank line
/// starts a paragraph, and an indented run (a table, an example) is set
/// verbatim, less its common indent, so its columns stay aligned.
fn roff_block(lines: &[&str]) -> String {
    let lines: Vec<&str> = {
        let start = lines
            .iter()
            .position(|l| !l.is_empty())
            .unwrap_or(lines.len());
        let end = lines
            .iter()
            .rposition(|l| !l.is_empty())
            .map_or(start, |i| i + 1);
        lines[start..end].to_vec()
    };
    let mut s = String::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.is_empty() {
            s.push_str(".PP\n");
            i += 1;
        } else if line.starts_with(' ') {
            let run = lines[i..]
                .iter()
                .take_while(|l| l.starts_with(' ') || l.is_empty())
                .count();
            let run = lines[i..i + run]
                .iter()
                .rposition(|l| !l.is_empty())
                .map_or(0, |n| n + 1);
            let indent = lines[i..i + run]
                .iter()
                .filter(|l| !l.is_empty())
                .map(|l| l.len() - l.trim_start().len())
                .min()
                .unwrap_or(0);
            s.push_str(".nf\n");
            for l in &lines[i..i + run] {
                s.push_str(&roff_escape(l.get(indent..).unwrap_or("")));
                s.push('\n');
            }
            s.push_str(".fi\n");
            i += run;
        } else {
            s.push_str(&roff_escape(line));
            s.push('\n');
            i += 1;
        }
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
pub(super) fn roff_escape(s: &str) -> String {
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
