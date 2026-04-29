//! Structured data model for `datum`'s top-level CLI surface.
//!
//! Every subcommand listed in `datum --help` is described by exactly one
//! [`CommandSpec`]. Renderers in [`crate::help::render`] consume this table
//! to produce the various output formats (plain `--help` text, Markdown for
//! READMEs, roff for man pages).
//!
//! The data here is the single source of truth for command surface
//! documentation.

/// Functional grouping used to lay out the top-level `datum --help` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Logging,
    Read,
    Mutate,
    Admin,
    ViewsSync,
}

impl Category {
    /// Header label as it appears in `datum --help`.
    pub fn label(self) -> &'static str {
        match self {
            Category::Logging => "LOGGING",
            Category::Read => "READ",
            Category::Mutate => "MUTATE",
            Category::Admin => "ADMIN",
            Category::ViewsSync => "VIEWS / SYNC",
        }
    }

    /// Display order in `datum --help`.
    pub fn order() -> &'static [Category] {
        &[
            Category::Logging,
            Category::Read,
            Category::Mutate,
            Category::Admin,
            Category::ViewsSync,
        ]
    }
}

/// One subcommand of `datum`.
pub struct CommandSpec {
    /// Primary canonical name (e.g. `"tail"`).
    pub name: &'static str,
    /// Alternate names that resolve to this command (e.g. `["end"]` for `stop`).
    pub aliases: &'static [&'static str],
    /// Category used to group commands in top-level help.
    pub category: Category,
    /// Per-command tagline ("show recent entries"). Used in the
    /// `datum NAME — SUMMARY` header. Conventionally lowercase.
    pub summary: &'static str,
    /// Optional override for the top-level help line. When `None` the
    /// renderer falls back to capitalizing the first letter of [`Self::summary`].
    pub top_summary: Option<&'static str>,
    /// Optional long-form prose for Markdown / man output.
    pub long_description: Option<&'static str>,
    /// Usage lines, rendered under the `USAGE:` heading. Each entry becomes
    /// one line and is indented with two spaces by the renderer.
    pub usage: &'static [&'static str],
    /// Positional arguments. Optional metadata for Markdown/man output;
    /// the bare `--help` text relies on `usage` strings.
    pub args: &'static [ArgSpec],
    /// Flag table.
    pub flags: &'static [FlagSpec],
    /// Worked examples for Markdown/man output.
    pub examples: &'static [ExampleSpec],
    /// Column at which flag summaries should align in the `--help` table
    /// (1-indexed). Lines whose left-hand side exceeds this value still
    /// get at least 2 spaces before the summary.
    pub summary_col: usize,
    /// When set, the plain-text renderer emits this string verbatim for
    /// the *body* of the per-command help (everything after the
    /// `datum NAME — SUMMARY\n` header). The structured fields above are
    /// still consulted by [`crate::help::render::render_markdown`] /
    /// [`crate::help::render::render_roff`].
    ///
    /// This is a transitional escape hatch: as the structured renderer
    /// gains coverage of the various per-command quirks, individual
    /// commands can drop their override and migrate to structured output.
    pub body_override: Option<&'static str>,
    /// When `true`, the per-command header uses the *input* alias verbatim
    /// (e.g. `datum end — log an entry`). When `false`, the canonical
    /// `name` is always used. Mirrors the historical behaviour of
    /// `log_help(verb)` in the previous hand-written renderer.
    pub display_input_name: bool,
}

/// One CLI flag.
pub struct FlagSpec {
    pub short: Option<char>,
    /// Empty string means short-only.
    pub long: &'static str,
    pub takes_value: bool,
    /// Bare hint like `"FIELD"`. The renderer wraps it as `<FIELD>`.
    pub value_hint: Option<&'static str>,
    /// When `true` and `takes_value` is set, the value renders as
    /// `[=<HINT>]` (no leading space).
    pub value_optional: bool,
    pub summary: &'static str,
    pub long_description: Option<&'static str>,
    /// When set, replaces the entire left-hand column for irregular
    /// renderings (e.g. `--end-date <DATE>, -d` puts short *after* long).
    pub left_override: Option<&'static str>,
}

/// Positional argument metadata.
pub struct ArgSpec {
    pub name: &'static str,
    pub required: bool,
    pub variadic: bool,
    pub summary: &'static str,
}

/// Worked example.
pub struct ExampleSpec {
    pub command: &'static str,
    pub description: &'static str,
}
