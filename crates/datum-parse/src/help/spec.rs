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
    Mutate,
    Read,
    Render,
    Admin,
    ViewsSync,
}

impl Category {
    /// Header label as it appears in `datum --help`.
    pub fn label(self) -> &'static str {
        match self {
            Category::Logging => "LOGGING",
            Category::Mutate => "MUTATE",
            Category::Read => "READ",
            Category::Render => "RENDER",
            Category::Admin => "ADMIN",
            Category::ViewsSync => "VIEWS / SYNC",
        }
    }

    /// Display order in `datum --help`.
    pub fn order() -> &'static [Category] {
        &[
            Category::Logging,
            Category::Mutate,
            Category::Read,
            Category::Render,
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
    /// The one sentence that describes this command, in both seats: the
    /// `datum NAME — SUMMARY` page header verbatim, and the top-level
    /// listing line with its first letter capitalized. Conventionally
    /// lowercase, and SPECIFIC — the listing is where a reader chooses a
    /// command and the page is where they land, so the same words have to
    /// meet them (da-ce7n3). There is deliberately no second field: the
    /// listing line and the page title drifted apart the whole time there
    /// were two.
    pub summary: &'static str,
    /// The page's opening, between its header and its USAGE: what the
    /// command is for, an example or two, and the one thing people get
    /// wrong, for a newcomer (da-efruz). Rendered verbatim; supply the
    /// trailing newline. Empty for none — the header's summary is then
    /// all the page says before its usage.
    pub intro: &'static str,
    /// Optional long-form prose for Markdown / man output.
    pub long_description: Option<&'static str>,
    /// Usage lines, rendered under the `USAGE:` heading. Each entry becomes
    /// one line and is indented with two spaces by the renderer. Tab
    /// completion reads their placeholders (`datum __complete`): a word
    /// ending in `FILE` or `PATH` is a file, one ending in `DIR` a
    /// directory, `-` among its alternatives stdin, so spell a file
    /// argument that way (`<PATH | ->`, `--to DIR`) or it completes nothing.
    pub usage: &'static [&'static str],
    /// Positional arguments. Optional metadata for Markdown/man output;
    /// the bare `--help` text relies on `usage` strings.
    pub args: &'static [ArgSpec],
    /// Flag table. Always the command's WHOLE accepted set — the machine
    /// readers (`datum help --json`, the web's command bar) ask this, and
    /// [`Self::shared_flags`] only changes how much of it `--help` prints.
    pub flags: &'static [FlagSpec],
    /// When set, the plain-text `OPTIONS:` block prints only
    /// [`SharedFlags::own`] and then one line pointing at `datum help
    /// <TOPIC>`, where the wall the commands share is written out once
    /// (da-ce7n3). Markdown and roff still print the whole of
    /// [`Self::flags`]: those are standalone reference documents, and a
    /// man page that hid its options behind a topic no reader can `man`
    /// would be a worse trade than the one `--help` makes.
    pub shared_flags: Option<&'static SharedFlags>,
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
    /// The words that may follow the name to pick a subcommand (`db
    /// backup`), each with its own usage lines, as the parser dispatches
    /// them. Empty for a command that takes none. `datum help --json`
    /// carries it so a reader never parses one out of a usage line, where
    /// `add`'s two forms and `db`'s six subcommands look alike (da-803mk).
    pub subcommands: &'static [SubcommandSpec],
}

/// One subcommand of a [`CommandSpec`] (`backup` of `db`).
pub struct SubcommandSpec {
    pub name: &'static str,
    /// The parent's usage lines that are this subcommand's, verbatim.
    pub usage: &'static [&'static str],
    pub flags: &'static [FlagSpec],
}

/// One CLI flag.
#[derive(Clone, Copy)]
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

/// A command's share of an option wall documented once, on its own
/// `datum help <topic>` page.
///
/// `man git` does this with its shared option sets rather than reprinting
/// them on every page; the six logging verbs were 38-40 lines each, of
/// which about thirty were byte-identical, so the lines that actually
/// distinguished them were buried (da-ce7n3).
pub struct SharedFlags {
    /// The [`TopicSpec::name`] that documents the wall — `datum help logging`.
    pub topic: &'static str,
    /// The flags PARTICULAR to this command, printed on its own page
    /// above the pointer. Their wording may be narrower than the shared
    /// table's (`add`'s `-K` is required; nobody else's is).
    pub own: &'static [FlagSpec],
    /// The wall's flags that bind a value with spaces, named under the
    /// pointer so a reader knows they are behind it (da-wxan9); empty for
    /// none.
    pub binders: &'static str,
}

/// A `datum help <NAME>` page that is not a command: the one place an
/// option wall shared by several commands is written out.
pub struct TopicSpec {
    /// The word `datum help <NAME>` answers to.
    pub name: &'static str,
    /// Page header, rendered as `datum help NAME — SUMMARY`.
    pub summary: &'static str,
    /// Prose between the header and the option table. Rendered verbatim;
    /// supply the trailing newline.
    pub intro: &'static str,
    /// The shared wall itself.
    pub flags: &'static [FlagSpec],
    /// Column at which the flag summaries align, as [`CommandSpec::summary_col`].
    pub summary_col: usize,
    /// Examples of the values those flags take, in groups after the
    /// table. `datum help time` has them (da-6yqu9); empty for none.
    pub forms: &'static [FormGroup],
    /// Environment variables, as an ENVIRONMENT table after the options.
    /// `datum help env` lists [`ENV_VARS`](super::ENV_VARS); empty for none.
    pub env: &'static [EnvVarSpec],
    /// Prose after the option table, for the exceptions a shared set
    /// always has. Rendered verbatim; empty for none.
    pub outro: &'static str,
}

/// One environment variable `datum` reads.
///
/// The README used to keep this table by hand, which is how
/// `DATUM_ID_DELIMITER` went undocumented and `DATUM_EDITOR` was
/// described as `EDITOR` (da-ce7n3). The README's table is generated from
/// here now, and so is `datum help env`, which is how a binary without
/// the checkout says what it reads (da-bread);
/// `env_table_covers_what_the_config_layer_reads` in
/// `datum-cli` fails if the config layer grows a variable it does not name.
pub struct EnvVarSpec {
    pub name: &'static str,
    pub summary: &'static str,
}

/// A group of accepted forms on a [`TopicSpec`] page, each shown by example.
pub struct FormGroup {
    pub heading: &'static str,
    /// `true` when the examples are durations, which parse on their own;
    /// otherwise each is argv as typed after a logging verb's field.
    pub durations: bool,
    pub forms: &'static [FormSpec],
}

/// One accepted form: an example exactly as typed, and what it means.
/// A test runs every example through the real parser.
pub struct FormSpec {
    pub example: &'static str,
    pub meaning: &'static str,
}
