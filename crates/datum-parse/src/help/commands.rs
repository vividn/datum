//! The `COMMANDS` table — one [`CommandSpec`] per subcommand of `datum`.
//!
//! Layout mirrors the section ordering of `datum --help`:
//! Logging → Mutate → Read → Render → Admin → ViewsSync.
//!
//! # Generated OPTIONS (preferred path for new commands)
//!
//! A command's `OPTIONS:` section should be *generated* from its
//! [`CommandSpec::flags`] table, not hand-written, so that adding a flag
//! forces a `--help` change and the two can never silently drift. To opt
//! in, leave [`CommandSpec::body_override`] unset and populate `flags`:
//! the renderer then builds USAGE (from `usage`) + OPTIONS (via
//! [`crate::help::render::render_options_block`]) structurally. `tail`,
//! `head`, the log verbs, and `update` / `merge` / `rekey` / `count` /
//! `ids` use this path.
//!
//! `body_override` is the legacy escape hatch for commands whose body
//! doesn't fit the plain USAGE + OPTIONS shape. Migrate them as their
//! quirks gain structural support; the ones still on `body_override` are:
//!
//! - **Multi-`USAGE` / subcommand trees** (`spec`, `db`, `sync`,
//!   `conflicts`, `alias`): several calling shapes and prose sections.
//! - **Narrative bodies with no OPTIONS table** (`now`, `get`, `undo`,
//!   `delete`, `grep`, `why`, `edit`, `history`, `info`, `config`,
//!   `track`, `ignore`): nothing to generate.
//! - **Multi-line flag summaries** (`check`): its `--fix` summary wraps
//!   across continuation lines, which `render_flag_table` does not yet
//!   emit.
//! - **Irregular flag layout** (`dayview`, `nowview`): short-after-long
//!   forms like `--end-date <DATE>, -d` need per-flag `left_override`s;
//!   deferred until the table is worth porting wholesale.
//! - **One-off parser, no FlagSpec** (`retime`): its body is a `DEFAULTS`
//!   narrative, not an OPTIONS table.
//!
//! # One summary, both seats
//!
//! [`CommandSpec::summary`] is the ONLY sentence a command gets. The
//! top-level listing capitalizes it; the page is titled with it verbatim.
//! There used to be a second field for the listing line, and every one of
//! them drifted: four verbs were titled "log an entry" while the listing
//! gave each a distinguishing sentence, `rekey` described two different
//! operations in the two places, and `nowview`'s listing line was
//! backwards (da-ce7n3). Write the specific sentence once.
//!
//! # Shared option walls
//!
//! A set of flags several commands share is written out ONCE, on a
//! [`TopicSpec`] page that `datum help <topic>` answers to, and each
//! command points at it through [`CommandSpec::shared_flags`] while
//! keeping whatever is particular to it. See `LOG_SHARED_FLAGS` below.

use super::spec::{
    ArgSpec, Category, CommandSpec, EnvVarSpec, ExampleSpec, FlagSpec, SharedFlags, TopicSpec,
};

const NO_ARGS: &[ArgSpec] = &[];

const UPDATE_ARGS: &[ArgSpec] = &[ArgSpec {
    name: "<ID_OR_SLUG>",
    required: true,
    variadic: false,
    summary: "Several ids are joined with commas (`a1b2,c3d4`): a word after a space is a VALUE",
}];
const NO_EXAMPLES: &[ExampleSpec] = &[];
const NO_FLAGS: &[FlagSpec] = &[];

// ---------------------------------------------------------------------------
// Logging verbs — they share one flag table and one usage shape.
// ---------------------------------------------------------------------------

const LOG_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        short: Some('y'),
        long: "",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Go back 1 day (stackable: -yy=2 days, ...)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('q'),
        long: "",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Go back 5 min (stackable: -qq=10 min, ...)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('d'),
        long: "date",
        takes_value: true,
        value_hint: Some("VAL"),
        value_optional: false,
        summary: "Date override (offset, YYYY-MM-DD, sunday, apr29)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('t'),
        long: "time",
        takes_value: true,
        value_hint: Some("VAL"),
        value_optional: false,
        summary: "Time override (offset, HH:MM, 9:15am, or a date)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('z'),
        long: "timezone",
        takes_value: true,
        value_hint: Some("TZ"),
        value_optional: false,
        summary: "Timezone (IANA or UTC offset)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('D'),
        long: "full-day",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Date-only entry (no clock time)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('u'),
        long: "undo",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Undo: delete most recent matching entry",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('k'),
        long: "key",
        takes_value: true,
        value_hint: Some("KEY_SPEC"),
        value_optional: false,
        summary: "Data-only id-key (forms: K req, K= opt, K=val default)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('K'),
        long: "slug-key",
        takes_value: true,
        value_hint: Some("S"),
        value_optional: false,
        summary: "Slug-contributing id-key (same K / K= / K=val forms)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('c'),
        long: "comment",
        takes_value: true,
        value_hint: Some("T"),
        value_optional: false,
        summary: "Comment",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('R'),
        long: "remainder",
        takes_value: true,
        value_hint: Some("KEY"),
        value_optional: false,
        summary: "Route un-keyed positionals to data key <KEY>",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('S'),
        long: "string-remainder",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Join remainder positionals into one string (not array)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('C'),
        long: "comment-remainder",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Remainder positionals → extra_data.comment string",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('I'),
        long: "string-state",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Join remainder positionals into the state as one string",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "id",
        takes_value: true,
        value_hint: Some("TEMPLATE"),
        value_optional: false,
        summary: "Slug template (e.g. %field%:%uid%)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "id-delimiter",
        takes_value: true,
        value_hint: Some("S"),
        value_optional: false,
        summary: "Delimiter to collapse empty segments",
        long_description: Some(
            "Resolution: this flag > DATUM_ID_DELIMITER env > datumrc.toml id_delimiter > \":\".",
        ),
        left_override: None,
    },
    FlagSpec {
        short: Some('L'),
        long: "last-state",
        takes_value: true,
        value_hint: Some("V"),
        value_optional: true,
        summary: "Assert prior state (bare: close-most-recent on stop)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('m'),
        long: "moment",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Point-in-time state change",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "conflict",
        takes_value: true,
        value_hint: Some("S"),
        value_optional: false,
        summary: "update|merge|rekey|ignore",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "update",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Shorthand for --conflict update",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "merge",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Shorthand for --conflict merge",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('M'),
        long: "no-metadata",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Zero created_at/modified_at, clear device_id",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('T'),
        long: "no-timestamp",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Timeless note-data (create_time id, no occurrence)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "alias",
        takes_value: true,
        value_hint: Some("NAME"),
        value_optional: false,
        summary: "On success, save current invocation as alias <NAME>",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "alias-overwrite",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Allow --alias to replace an existing alias",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "dry-run",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Preview the would-be entry without writing",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('A'),
        long: "show-all",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Show full entry detail after writing",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('o'),
        long: "format-string",
        takes_value: true,
        value_hint: Some("FMT"),
        value_optional: false,
        summary: "Render written entry via format string",
        long_description: None,
        left_override: None,
    },
];

// Shared body for all log verbs — parameterized by the verb at lookup time.
// Stored as an associated function below rather than a const because of the verb substitution.

// ---------------------------------------------------------------------------
// Tail flag table (used both as structured render and also matches body).
// ---------------------------------------------------------------------------

/// nowview's time flags (da-5flt): the grammar `help --json` hands the web,
/// which offers nowview under a `-t` pill. The text is `NOWVIEW_BODY`'s.
const NOWVIEW_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        short: Some('t'),
        long: "time",
        takes_value: true,
        value_hint: Some("TIME"),
        value_optional: false,
        summary: "Centre the window on this instant",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('d'),
        long: "date",
        takes_value: true,
        value_hint: Some("DATE"),
        value_optional: false,
        summary: "Date of the instant",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('q'),
        long: "quick",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "5 min back (stackable)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('y'),
        long: "yesterday",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "A day back (stackable)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('z'),
        long: "timezone",
        takes_value: true,
        value_hint: Some("TZ"),
        value_optional: false,
        summary: "Zone -t and -d are read in",
        long_description: None,
        left_override: None,
    },
];

const TAIL_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        short: Some('n'),
        long: "",
        takes_value: true,
        value_hint: Some("N"),
        value_optional: false,
        summary: "Number of entries (default 10)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('m'),
        long: "metric",
        takes_value: true,
        value_hint: Some("M"),
        value_optional: false,
        summary: "hybrid|occur|create|modify",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('r'),
        long: "reverse",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Invert ordering",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "column",
        takes_value: true,
        value_hint: Some("NAME"),
        value_optional: false,
        summary: "Extra extra_data column (repeatable)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('d'),
        long: "date",
        takes_value: true,
        value_hint: Some("VAL"),
        value_optional: false,
        summary: "Date override",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('t'),
        long: "time",
        takes_value: true,
        value_hint: Some("VAL"),
        value_optional: false,
        summary: "Upper-bound time",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Day / 5-min offsets (stackable)",
        long_description: None,
        left_override: Some("-y / -q"),
    },
    FlagSpec {
        short: Some('w'),
        long: "watch",
        takes_value: true,
        value_hint: Some("INT"),
        value_optional: true,
        summary: "Live scrollable view; stream entries (poll 1s)",
        long_description: None,
        left_override: Some("-w, --watch [INT]"),
    },
    FlagSpec {
        short: None,
        long: "table",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Legacy columnar view (default: info-line)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "show-meta",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Include flag/meta entries (hidden by default)",
        long_description: None,
        left_override: Some("    --show-meta       (alias: --show-all-metadata)"),
    },
];

// ---------------------------------------------------------------------------
// Structurally-rendered flag tables. Commands wired to these leave
// `body_override` unset, so their OPTIONS section is generated from the
// table by `render::render_options_block` — adding a flag here is the
// only edit needed to update the command's `--help`. See the module-level
// "Generated OPTIONS" note below.
// ---------------------------------------------------------------------------

const HEAD_FLAGS: &[FlagSpec] = &[FlagSpec {
    short: Some('n'),
    long: "limit",
    takes_value: true,
    value_hint: Some("N"),
    value_optional: false,
    summary: "Number of entries (default 10)",
    long_description: None,
    left_override: None,
}];

// Shared by update / merge / rekey — they take the same flag set.
const UPDATE_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        short: None,
        long: "strategy",
        takes_value: true,
        value_hint: Some("S"),
        value_optional: false,
        summary: "update|merge|rekey",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "id",
        takes_value: true,
        value_hint: Some("TEMPLATE"),
        value_optional: false,
        summary: "New id/slug template",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "id-delimiter",
        takes_value: true,
        value_hint: Some("S"),
        value_optional: false,
        summary: "Delimiter between empty segments",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('k'),
        long: "key",
        takes_value: true,
        value_hint: Some("KV"),
        value_optional: false,
        summary: "Extra key=value (repeatable)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('c'),
        long: "comment",
        takes_value: true,
        value_hint: Some("T"),
        value_optional: false,
        summary: "Comment",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "dry-run",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Show what would change",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "strict",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Abort on first batch error",
        long_description: None,
        left_override: None,
    },
];

const COUNT_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        short: None,
        long: "group-by",
        takes_value: true,
        value_hint: Some("KEY"),
        value_optional: false,
        summary: "state|verb|field",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "since",
        takes_value: true,
        value_hint: Some("TIME"),
        value_optional: false,
        summary: "Duration ago (1d, 30m, 1:30), YYYY-MM-DD, or any -t time",
        long_description: None,
        left_override: None,
    },
];

const STATS_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        short: None,
        long: "since",
        takes_value: true,
        value_hint: Some("TIME"),
        value_optional: false,
        summary: "Inclusive lower bound (duration ago / date / any -t time)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "until",
        takes_value: true,
        value_hint: Some("TIME"),
        value_optional: false,
        summary: "Upper bound; a bare YYYY-MM-DD covers the whole named day",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "today",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "This day so far (midnight to now)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "week",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Since the start of this week (see week_start config)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "last-week",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "The whole of the previous week",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "month",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Since the first of this month",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "last-month",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "The whole of the previous calendar month",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('d'),
        long: "date",
        takes_value: true,
        value_hint: Some("VAL"),
        value_optional: false,
        summary: "A date names that whole day (unless --until)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('t'),
        long: "time",
        takes_value: true,
        value_hint: Some("VAL"),
        value_optional: false,
        summary: "A time is a lower bound, like --since",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('z'),
        long: "timezone",
        takes_value: true,
        value_hint: Some("TZ"),
        value_optional: false,
        summary: "Zone the period boundaries are read in",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Day / 5-min offsets (stackable); -y is a whole day",
        long_description: None,
        left_override: Some("-y / -q"),
    },
    FlagSpec {
        short: None,
        long: "include-false",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Count the closed (false) state in the ratio denominator",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "include-untracked",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Count untracked (null / IGNORED) states in the ratio denominator",
        long_description: None,
        left_override: None,
    },
];

// `--json` is documented once, in the shared OUTPUT block (da-qt6z).
const FIELDS_FLAGS: &[FlagSpec] = &[FlagSpec {
    short: Some('A'),
    long: "include-ignored",
    takes_value: false,
    value_hint: None,
    value_optional: false,
    summary: "List ignored fields too (same as `--show all`)",
    long_description: None,
    left_override: None,
}];

const IDS_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        short: Some('n'),
        long: "limit",
        takes_value: true,
        value_hint: Some("N"),
        value_optional: false,
        summary: "Cap the number of ids returned",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "oldest-first",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Return oldest-first instead of newest-first",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "since",
        takes_value: true,
        value_hint: Some("TIME"),
        value_optional: false,
        summary: "Same grammar as `datum count --since`",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "slug",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Print slugs instead of short hids",
        long_description: None,
        left_override: None,
    },
];

// ---------------------------------------------------------------------------
// Per-command body_override constants — verbatim copies of the legacy
// hand-authored strings. These guarantee byte-identical output for the
// transition; structured render coverage will expand in follow-up PRs.
// ---------------------------------------------------------------------------

const NOW_BODY: &str = "USAGE:\n  datum now [-A] [--bare] [--no-color] [--cols N]\n\n  -A           Show ignored fields too (dim)\n      --bare   Suppress the terminal nowview band; only show the state list\n      --no-color   Disable ANSI color in the nowview band\n      --cols N Override terminal width in cells (auto-detected from $COLUMNS)\n";

const GET_BODY: &str = "USAGE:\n  datum get <ID_OR_SLUG[,ID_OR_SLUG]...> [--pretty]\n\nA comma list prints each entry in the order given (`--pretty` and `--show json`\ntoo). An id that does not resolve is named on stderr after the documents that\ndid, and the command exits 1.\n\nStructured states (objects with `id` and other fields) render as an `{id}`\nchip on the main info line, with sub-fields broken out on a state second\nline below. Parallel states wrap in `[ … ]` with one inner state per line.\nThe per-entry data block follows the same alignment convention — keys are\nright-aligned so their trailing `:` shares a column when the block goes\nmultiline.\n";

const DELETE_BODY: &str = "USAGE:\n  datum delete [ID_OR_SLUG[,ID_OR_SLUG]...] [--force] [--dry-run] [--strict]\n                [-A | -o FMT]\n\nARGS:\n  [ID_OR_SLUG]  QuickId(s) — hid prefix, slug, `_`/`_LAST`, `<field>:_`,\n                slug wildcard. Multiple targets may be comma-batched\n                (`<id>,<id>,...`); resolution is per-token.\n                Omitting the arg targets `_LAST`. Nothing refuses on\n                age: past ten minutes delete warns on stderr with\n                the pointer's age and proceeds (`datum undo` is the\n                way back).\n\nOPTIONS:\n      --force       Accepted, but there is no longer a prompt to skip.\n      --dry-run     Print what would be deleted and exit; no DB writes.\n      --strict      With comma-batched tokens, abort up-front if ANY\n                    token failed to resolve (default: delete the\n                    resolved subset, skip-and-warn on the failures).\n  -A, --show-all    Show the full entry document for each deleted row.\n  -o, --format-string <FMT>\n                    Render each deleted row through datum's format-string DSL.\n\nEXAMPLES:\n  datum delete                    # delete _LAST (warns if > 10 min stale)\n  datum delete activity:_         # delete latest activity row\n  datum delete a1b2,c3d4          # batched\n  datum delete a1b2,c3d4 --dry-run\n                                  # preview only — no DB writes\n";

const UNDO_BODY: &str = "USAGE:\n  datum undo [ID_OR_SLUG[,ID_OR_SLUG]...] [--to-version N]\n\nReverts an entry to a prior version via its entry_history trail. By default,\nrestores the immediately-previous version; --to-version N picks an explicit\nversion. With no ID_OR_SLUG it acts on `_LAST`, falling back to the most\nrecent tombstone for legacy entries with no trail. A comma list steps each\nentry back along its own trail; --to-version with a list is refused.\n\nUndo never refuses on age. Past ten minutes it names the age on stderr and\nproceeds anyway: `datum redo` is the way forward again.\n";

const REDO_BODY: &str = "USAGE:\n  datum redo [ID_OR_SLUG[,ID_OR_SLUG]...]\n\nThe inverse of `datum undo`: re-applies the version the last undo stepped\nback from. Redo of an undone create re-creates the entry under the same hid;\nredo of an undone delete deletes it again. With no ID_OR_SLUG it acts on\n`_LAST`, and every row of a multi-entry `_LAST` steps together, as every entry\nof a comma list does.\n\nExits 1 with 'nothing to redo' when the entry's newest step is not an undo,\nso `undo; redo; redo` moves once, not twice.\n";

const HISTORY_BODY: &str = "USAGE:\n  datum history <ID_OR_SLUG[,ID_OR_SLUG]...> [--version N | --diff A B]\n\nA comma list prints each entry's trail in turn, each under its own `hid:` line\n(`--show json`: one array of `{hid, versions}`). `--version` and `--diff` number\nONE entry's versions, so with a list they are refused.\n";

const WHY_BODY: &str = "USAGE:\n  datum why <ID_OR_SLUG[,ID_OR_SLUG]...> [--full]\n\nA comma list answers for each entry in turn, each under its own `hid:` line.\n";

const EDIT_BODY: &str = "USAGE:\n  datum edit <ID_OR_SLUG>\n\nThe editor is $DATUM_EDITOR, else the `editor` key in datumrc.toml, else\n$VISUAL, else $EDITOR, else `vi` — what `datum config show` reports.\nIt is spawned directly, never through a shell, so a value carrying\nflags — `code --wait`, `emacsclient -nw` — still works.\n";

const RETIME_BODY: &str = "USAGE:\n  datum retime [SLUG[,SLUG]...] [SHIFT]\n  datum retime [SLUG[,SLUG]...] [-r] [-t T] [-y]... [-q]... [-d D]\n  datum retime [SLUG[,SLUG]...] --offset DUR [--field F] [--after BND] [--before BND]\n\nDEFAULTS:\n  SLUG omitted        → `_LAST` (matches update/delete/undo)\n  No time-args        → set to now\n  SHIFT (bare int)    → minutes-offset shorthand; `retime <id> -5` shifts -5 min\n  -r/--relative       → interpret time-args as a delta from now, applied to the\n                        entry's current occur_time (rather than absolute target).\n                        Same flag is `--reverse` on tail; namespaces don't collide.\n  --offset DUR        → explicit ISO duration shift (legacy form)\n\nBULK FILTERS (--field/--after/--before select rows instead of _LAST):\n  BND                 → a duration ago (1d, 30m, 1:30), a date (2026-06-01,\n                        apr29, today, yesterday), or any -t time. Both bounds\n                        are inclusive, and a bare date names that day's\n                        opening midnight — so --before 2026-06-01 excludes\n                        June 1.\n";

const SPEC_BODY: &str = "USAGE:\n  datum spec show <FIELD> [--text]\n  datum spec add-state <FIELD> <NAME> [COLOR] [DESCRIPTION]\n  datum spec set <FIELD> [--from-json JSON] [--verb V] [--state S]... [--tag T]... ...\n  datum spec list [--tag T]\n  datum spec remove <FIELD>\n\n`add-state` merges one state into a field spec without dropping the others\n(unlike `set --state`, which replaces the whole list). Use `-` in the COLOR\nslot to set only a description: `datum spec add-state mood meh - \"so-so\"`.\n";

const ALIAS_LONG_DESCRIPTION: &str = "Aliases expand argv before the parser sees it, so an alias can stand in for any prefix of a `datum` invocation — verb, field, flags, or all three.\n\nA standalone, whitespace-bounded `,` token splits one invocation into N sides that write atomically with the same occur_time. The trigger works at the top-level CLI, inside alias bodies, and through chains of alias-of-alias (multi-of-multi flattens to N flat sides). The comma must be its own token: `vertical , sit` triggers a multi, while `vertical,sit`, `vertical ,sit`, and `vertical, sit` do not. Values with embedded commas (`key=a,b`) pass through unchanged.\n\nKey slot forms (`-k` and `-K` accept the same three shapes): `-k key` is required and errors at call time if unfilled, `-k key=` is optional with no default (silently skipped), `-k key=val` defaults to `val` when no value is supplied. A `-k` or `-K` name declared on multiple sides becomes a shared slot — one call-site value propagates to every declaring side. The `.` token skips a slot on its side; for a shared slot it skips on all sides.\n\n`--` semantics in multi-entry contexts: inside an alias body, flags before `--` scope to their side and flags after `--` apply to all sides. `--` is implicit at the end of every alias body, so call-site flags apply to all sides by default. A `--` inside an alias body does not propagate past the alias boundary.\n\nTime flags at the call site compose with the ones the alias bound: `-y`, `-q` and `-d` shift whatever time the alias bound, and a relative `-t` adds to it — on an alias bound to `-t +5`, `-t +10` writes at +15. In a multi-entry alias every side writes at ONE occur_time, taken from the first side, so a later side's `-t` is dropped.\n\nAfter a multi-entry write, `_LAST` refers to every entry written by the most recent invocation, so `rekey _LAST oldkey=newkey` affects every row of the atomic group. A subsequent single-entry write resets `_LAST` to that one row.";

const ALIAS_BODY: &str = "USAGE:\n  datum alias show <NAME>\n  datum alias set <NAME> <EXPANSION>\n  datum alias list\n  datum alias remove <NAME>\n\nATOMIC MULTI-ENTRY ALIASES:\n  A standalone, whitespace-bounded `,` token splits one invocation\n  into N sides that all write atomically with the same occur_time.\n  Works at the top-level CLI, inside alias bodies, and through chains\n  of alias-of-alias (multi-of-multi flattens to N flat sides).\n\n  The comma must be its own token. `vertical , sit` triggers a multi;\n  `vertical,sit`, `vertical ,sit`, and `vertical, sit` do not. Values\n  with embedded commas (`key=a,b`) pass through unchanged.\n\n  Examples:\n    datum switch body vertical , switch body sit\n        Two same-field switches at one occur_time merge into one\n        body row with state=Parallel([vertical, sit]).\n    datum alias set oov \\\n        \"switch environment outside , switch body vertical\"\n        Define a multi-entry alias. `datum oov` writes both rows.\n    datum alias set caf5 \"occur caffeine -t +5\"\n        A one-sided alias binding a time.\n\nCALL-SITE TIME FLAGS:\n  Compose with the alias-bound ones: -y / -q / -d shift whatever\n  time the alias bound, and a relative -t adds to it. An absolute\n  -t replaces the bound clock and keeps the bound offset, either way\n  round.\n\n    datum caf5            caffeine at now + 5 min (the bound time)\n    datum caf5 -t +10     now + 15\n    datum caf5 -t 10:00   10:05\n    datum caf5 -q         now + 5 then 5 back, so now\n    datum caf5 -y         yesterday, + 5 min\n\n  Every side of a multi writes at ONE occur_time, taken from the\n  FIRST side, so a later side's -t is dropped.\n\nKEY SLOT FORMS (-k / -K):\n  -k key       required (errors at call site if unfilled)\n  -k key=      optional, no default (silently skipped if unfilled)\n  -k key=val   defaulted (auto-filled with `val` when unfilled)\n\n  A `-k` (or `-K`) name declared on multiple sides becomes a shared\n  slot — one call-site value propagates to every declaring side. `.`\n  skips a slot on its side; `.` for a shared slot skips on all sides.\n\nFIELD SLOT FORMS (-F):\n  Same three shapes as `-k`/`-K`, but the resolved value becomes the\n  entry's field rather than a data key.\n    -F field       required (errors if unfilled)\n    -F field=      optional, no default (no field bound when unfilled)\n    -F field=val   defaulted (entry field is `val` when unfilled)\n\n  Shared across sides by name like `-k`: one call-site value\n  propagates to every declaring side.\n\n`--` SEMANTICS IN MULTI-ENTRY CONTEXTS:\n  Inside an alias body, flags before `--` scope to their side; flags\n  after `--` apply to all sides. `--` is implicit at the end of every\n  alias body, so call-site flags apply to all sides by default. A `--`\n  inside an alias body does not propagate past the alias boundary.\n\n`_LAST` AFTER A MULTI-WRITE:\n  `_LAST` refers to every entry written by the most recent invocation,\n  so `rekey _LAST oldkey=newkey` affects every row of an atomic group.\n  A subsequent single-entry write resets `_LAST` to that one row.\n";

const TRACK_BODY: &str = "USAGE:\n  datum track <FIELD> [--verb V] [--color C] [--state S|S=C]... [--tag T]...\n\nOPTIONS:\n  --verb <V>   What a bare press of the field means: occur|start|switch|add.\n               Required for a field with neither a spec nor a log; with a\n               log, the log answers once and the answer is written down.\n  --color <C>  The colour FIELD is drawn in — its chip in `now`, its band\n               in the nowview, its marks in the dayview. `#RRGGBB`, or a\n               legacy ANSI-256 number. Anything else is stored as typed\n               and falls back to the hash of the field's name.\n  --state <S>  A state the field may be switched to. `S=COLOR` paints one\n               instead; the `=` is what tells the two apart.\n  --tag <T>    Repeatable, and each flag ADDS its tag to the field's set.\n               `datum spec set --tag` is the one that REPLACES the set.\n\nTracking is not what puts a field in `datum now` — every field that has\never been logged is already there (`datum now -A` shows the whole set).\nWhat `track` does is DECLARE one: it writes the field's SPEC — the verb,\nthe colour, the states, the tags — and one switch row into the log\nsaying the declaration happened.\n\nThe spec is what gives a field its own words. `track mood --verb switch\n--state good` makes `datum mood` mean `switch mood` and `datum good`\nmean `switch mood good`; `datum alias list` prints them. That spec is\nTHIS DEVICE'S: sync carries entries, not specs, so a second device that\npulls the same history gets the rows without the words.\n\nThe row is written at now and carries an OFF state — `false` for a\nstart/switch field, the TRACKED sentinel for occur/add — and a field's\nstate is whatever its newest row says. So tracking a field that is\nRUNNING ends it, and `now` shows it off from that moment on: `track`\ndeclares a field, it does not inspect one. That row also un-ignores an\nignored field, as any later log does — see `datum help ignore`.\n";

const IGNORE_BODY: &str = "USAGE:\n  datum ignore <FIELD>\n\nIgnoring is the other half of `datum track`, and it is one row: a switch\nentry carrying the IGNORED sentinel, written at now. A field is ignored\nwhen its NEWEST state-bearing row is that sentinel — ignored-ness is\nread off the log, never off a flag — so the next `occur`/`start`/\n`switch` on the field un-ignores it, as `datum track` does.\n\nWhat that hides is the field's place in the views that ask what exists:\n`datum now` drops it from the list (`now -A` shows it again, dimmed,\nbelow a rule), `datum fields` stops listing it, and `datum info` counts\nit under `ignored` instead of `tracked`. Nothing else moves — the\nentries stay, `tail` / `get` / `grep` still find them, and the field's\nspec is left exactly as it was.\n\n`ignore` takes no time flags; the row lands at now. And it needs no\nfield to ignore: naming one that was never logged INVENTS it, with the\nsentinel as the only row it has.\n";

const GREP_BODY: &str = "USAGE:\n  datum grep <PATTERN>... [-f FIELD] [-A | -o FMT]\n\nARGS:\n  <PATTERN>...    One or more regex patterns; rows must match EVERY one\n                  (AND semantics). Each pattern is matched against the\n                  entry's serialized JSON representation, so slug, value,\n                  extra_data, and metadata are all in scope.\n\nOPTIONS:\n  -f, --field <F>      Restrict the search to entries on field F.\n  -A, --show-all       Show the full entry document (verbose form).\n  -o, --format-string <FMT>\n                       Render each hit through datum's format-string mini-DSL.\n\nEXAMPLES:\n  datum grep coffee                      # any entry mentioning 'coffee'\n  datum grep -f mood happy               # mood rows with state/value matching 'happy'\n  datum grep -A 'kg' caffeine            # caffeine rows with units/value matching 'kg'\n";

const CHECK_BODY: &str = "USAGE:\n  datum check [CHECKER]... [--fix]\n  datum check --list\n\nARGS:\n  [CHECKER]...   Run only the named checker(s). With no positionals, the\n                 full default battery runs. Known names:\n                   verbs      Drift between stored verb and derive_verb\n                   aliases    Spec/user alias divergence + state collisions\n                   slugs      Drift between stored slug and re-derived slug\n                   integrity  Pending warnings from the conflict_queue\n                   flags      Pending review markers from `datum flag`\n                   tz         Stored UTC offset vs timezone at that instant\n\nOPTIONS:\n  --fix          Rewrite drifted `verb` / `slug` columns to match what\n                 the live deriver would produce; annotate alias state\n                 collisions with the explicit first-saved-wins\n                 resolution; rewrite legacy `content_hash` columns\n                 that disagree with `expected_content_hash`.\n  --list         Print the set of known checker names and exit.\n\nEXAMPLES:\n  datum check                  # full default battery\n  datum check verbs            # only verb-drift\n  datum check aliases --fix    # only alias divergence, annotate collisions\n  datum check integrity flags  # only the queue + review-marker views\n  datum check slugs --fix      # rewrite drifted slug columns in place\n";

const DAYVIEW_BODY: &str = "USAGE:\n  datum dayview [OPTIONS]\n\nOPTIONS:\n  --start-date <DATE>      Start date (ISO YYYY-MM-DD or relative offset like -3, 0)\n  --end-date <DATE>, -d    End date (default: today)\n  --n-days <N>, -n         Number of days (default: 1)\n  --width <N>              SVG width in pixels (default: 2000)\n  --height <N>             Total SVG height (auto from day-height + n-days when unset)\n  --day-height <N>         Per-day band height (default: 100)\n  --time-axis-height <N>   Bottom time-axis strip height (default: 15)\n  --label-width <N>        Date-label column width (default: from the label font).\n                           0 draws no date labels — the bands start at the margin\n  --margin <N>             Outer margin (default: 2)\n  -o, --output-file <PATH> .svg / .png / .html (default: SVG to stdout)\n  -w, --watch              Redraw on a 5-minute poll loop\n  --orientation <O>        horizontal (default): time runs left to right and the\n                           days stack as rows. vertical: time runs top to bottom\n                           and the days stand side by side as columns, the way\n                           most calendars draw a week, and perhaps the better fit\n                           on a phone. Pivoted, the size flags pivot with it:\n                           --width spreads the days, --height is the day's\n                           length, --day-height a column's width, the time axis\n                           runs down the left and the date labels across the top\n  --horizontal, --vertical Shorthand for --orientation\n";

const NOWVIEW_BODY: &str = "USAGE:\n  datum nowview [OPTIONS]\n\nOPTIONS:\n  --context <DUR>          Context before the instant (e.g. 15m, 1h). Default: 0 —\n                           the snapshot: each field's state, no time band\n  --history <DUR>          The same flag, under its older name\n  --labels                 Name each field's state, in a gutter right of the\n                           picture, or below it when vertical (SVG only; off by\n                           default)\n  -t, --time <TIME>        Centre the window on this instant instead of ending it\n  -d, --date <DATE>        at now; the time flags as the verbs take them\n  -q, --quick              (-q = 5 min back, stackable)\n  -y, --yesterday          (a day back, stackable)\n  -z, --timezone <TZ>      The zone -t and -d are read in\n  --width <N>              SVG width in pixels (auto-scales with --context when unset;\n                           300 when vertical)\n  --height <N>             SVG height in pixels (default 300; auto-scales with\n                           --context when vertical)\n  --orientation <O>        horizontal (default), or vertical: time runs down the page\n                           as a calendar's does, and each field is a column. SVG\n                           only; --horizontal / --vertical for short\n  --margin <N>             Outer margin (default 2)\n  --time-axis-height <N>   Time-axis strip height (default 15)\n  -o, --output-file <PATH> .svg / .png / .html, or - for the SVG on stdout\n                           (default: the terminal band)\n  -w, --watch              Redraw on DB change (60s idle refresh)\n  --window <DUR>           The terminal band's older name for --context; unlike it,\n                           refuses a zero rather than reading it as the snapshot\n";

const INFO_BODY: &str = "USAGE:\n  datum info [TARGET] [--all]\n\nARGS:\n  [TARGET]   A registered DB name, or a path — anything containing `/`,\n             starting with `.`, or ending in `.db` is read as a path.\n             Omitted, the active DB is used (`--db`, else `$DATUM_DB`,\n             else the default).\n\nOPTIONS:\n  --all      Every database instead of one: the names in the registry\n             (`<config>/datum/dbs.toml`) plus every `*.db` in the data\n             directory (`$DATUM_HOME`, else `$XDG_DATA_HOME/datum`), one\n             block each, the registry's path winning a name collision.\n\nOne block per database: where the file is, its schema version, live and\ndeleted entry counts, the tracked/ignored tally that `datum fields`\nreports one field at a time, the last write, and the size on disk. The\nschema row reads the version the file holds: `v<N> ✓ (current)`, `⚠\n(older than current v<N>)`, or `✗ (newer than binary (v<N>))`. One\nthat will not open says `✗` and why, and one that is not there says `✗\n(missing)`. None of those fails the command.\n\nA TARGET, and every database `--all` lists, is only READ: `info` does\nnot migrate it, so an older one stays as it is until a command opens it\nwith `--db`. The active database is the exception — every command,\n`info` included, brings it to this binary's schema before it runs.\n`info` also creates a missing parent directory for a TARGET path.\nWhat it never does is create the database file itself, or add an\nunknown name to the registry.\n\nTwo rows are placeholders, wired to nothing: `sync:` always prints `not\nconfigured` (`datum sync status` is the real answer), and `last write`\nalways prints `(none)`.\n";

const DB_BODY: &str = "USAGE:\n  datum db setup\n  datum db purge [--yes | -y]\n  datum db backup <PATH | -> [--overwrite]\n  datum db restore <PATH | -> [--force] [--dry-run]\n  datum db migrate (run | status | edit)\n  datum db v2import [FILE | - | --from FILE] [--dry-run]\n\nSetup applies any pending schema migrations to the active database. Use\n`datum db purge` to drop and recreate the schema from scratch.\n\nPurge drops every datum table and rebuilds the schema from scratch. Without\n--yes, prompts for the database path as confirmation; aborts on mismatch.\nNon-TTY stdin without --yes is rejected.\n\nBackup dumps live entries to JSONL (default) or tar.gz when the path ends in\n.tar.gz / .tgz, or to stdout as JSONL when the path is `-`. Restore replays\na backup file (or JSONL on stdin, for `-`) into the active database.\n\nMigrate is the schema-migration runner: `run` advances the DB to the binary's\ntarget version, `status` lists applied/pending steps, `edit` is reserved for\nper-entry data migrations (not yet implemented).\n\nV2import ingests a legacy datum CouchDB backup JSON (`{ docs: [...] }`), or\nthat JSON on stdin for `-`, into the active database. Idempotent —\nre-importing the same backup is a no-op.\n";

const CONFIG_BODY: &str = "USAGE:\n  datum config show\n";

const SYNC_BODY: &str = "USAGE:\n  datum sync [--push | --pull] [--dry-run] [--server URL]\n  datum sync login [--server URL] [--cluster NAME]\n  datum sync config set-server <URL> [--global]\n  datum sync config disable\n  datum sync config show\n  datum sync key (init | unlock | rotate-passphrase | show-fingerprint) [--server URL] [--force]\n  datum sync status [--server URL]\n  datum sync install-daemon [--show | --uninstall]\n  datum sync agent [--pid-file PATH] [-v] [--one-shot]\n\nA sync is a pull and then a push of ENTRIES, encrypted on this device\nbefore they leave it. The relay holds opaque blobs keyed by device and\nsequence number and can never read one — which is why the key is yours\nto carry between devices, and why entries are all that travel. Field\nspecs stay on the device that declared them, and so do the words `datum\ntrack` makes out of them.\n\nThree things must be in place before a sync runs, and whichever is\nmissing is what the error names: a SERVER, a CREDENTIAL for it, and an\nunlocked MASTER KEY. Both credential and key are per relay HOST, so a\ndevice that talks to two relays keeps two of each.\n\n  sync                  Pull, then push. `--push` / `--pull` does one\n                        half; `--dry-run` reports the counts and writes\n                        nothing, here or on the relay.\n  config set-server     Pin the relay for the active `--db`, or for every\n                        DB with `--global`. Written to `datumrc.toml` —\n                        which is also the file that enlists the DB with\n                        the daemon, auto-sync being on wherever a server\n                        resolves.\n  config disable        Set it to `none`: sync off for the active DB, or\n                        globally when no DB is named. `config show`\n                        prints global, per-DB and effective.\n  login                 The passkey / device-code flow, live once the\n                        relay enables it. Writes a bearer under the\n                        config directory as `auth/<host>.json` (mode\n                        0600) and binds this DB to a CLUSTER — named\n                        after the DB's file unless `--cluster` says\n                        otherwise, so two devices on a same-named DB\n                        share one with no configuration. Until a relay\n                        offers login, the supported credential is a token\n                        minted on the relay host (`datum-sync-server\n                        mint-token <user>`) and exported to the client as\n                        `$DATUM_SYNC_TOKEN`.\n  key init              Make a 32-byte master key, wrap it with a\n                        passphrase typed twice, upload the WRAPPED copy\n                        to the relay, and write the plain one under the\n                        data directory as `keys/<host>.key` (mode 0600).\n                        `--force` overwrites a local key already there.\n  key unlock            The second device's half: fetch the wrapped copy,\n                        ask for the passphrase, write the same key here.\n  key rotate-passphrase Re-wrap the local key under a new passphrase and\n                        re-upload it. The key itself does not change, so\n                        nothing already stored needs re-encrypting and\n                        the fingerprint stays as it was.\n  key show-fingerprint  Print the local key's SHA-256, to check two\n                        devices against each other by eye.\n  status                Server, user, daemon, and the active DB's last\n                        push and pull with its pending integrity count.\n  install-daemon        Write and enable the user service that runs\n                        `datum sync agent` at login — a systemd user unit\n                        under `$XDG_CONFIG_HOME/systemd/user/`, or a\n                        launchd plist under `~/Library/LaunchAgents`.\n                        `--uninstall` disables and removes it. `--show`,\n                        which would print that unit instead of installing\n                        it, cannot be reached: the global `--show <MODE>`\n                        claims the word before this command is parsed.\n  agent                 The daemon in the foreground: one worker per DB\n                        that has a server and has not set `auto_sync =\n                        false`, pushing shortly after a write and pulling\n                        on a timer. `install-daemon` is how it is meant\n                        to be started.\n\nThe `<host>` in those two paths is the relay's host, plus `_<port>` when\nthe URL names one.\n\nA pull applies what it can and FILES what it cannot: `lww_overwrite` and\n`identity_collision` go to `datum conflicts`, every other integrity\nfinding to `datum check`. A relay that has been purged or restored\nanswers an old-generation push with `cluster forked`, and says how to\nre-init this device.\n";

const CONFLICTS_BODY: &str = "USAGE:\n  datum conflicts [list] [--all]\n  datum conflicts resolve <HID> (--take-local | --take-remote | --keep-both)\n\nOPTIONS:\n  -a, --all        Include already-resolved conflicts (default: pending only).\n  --take-local     Restore this device's version of the entry.\n  --take-remote    Accept the other device's version.\n  --keep-both      Leave both rows as they are; record the decision.\n\nSync conflicts are the two things a pull can file when two devices claim\none identity: `lww_overwrite` (a newer remote edit replaced a local one)\nand `identity_collision` (two distinct entries claimed one slug, or one\n(field, occur_time)). The losing version survives only in the conflict\nrow, so someone has to say which side wins — `list` prints both.\n\nEverything else the integrity queue holds — double opens, orphan ends,\noverlaps, repeated states — is reported by `datum check` and resolved by\nfixing the data with `datum edit` / `datum delete`, not from here.\n";

const FLAG_BODY: &str = "USAGE:\n  datum flag <FIELD> [COMMENT...] [-y / -q / -d / -t / -z]\n\nAttaches a 'come back to this' marker to <FIELD>. Stored as a Verb::Point\nentry with `extra_metadata = { flag: true, comment: \"...\" }`. The bare\nform `datum flag <FIELD>` records a marker at now with no comment.\n\nDefault rendering hides flags from `datum tail` and `datum now`; use\n`tail --show-meta` to include them, or `datum check` to surface them as\npending review markers. The shape mirrors `datum meta` (which sets\n`meta: true` instead of `flag: true`).\n";

const META_BODY: &str = "USAGE:\n  datum meta <FIELD> <TEXT>...\n  datum meta show <FIELD>\n\nAttach (or list) a protocol-change note on <FIELD>. Stored as a\nVerb::Point entry with `extra_metadata = { meta: true, comment: \"...\" }`.\n`meta show` lists every note attached to <FIELD> in create_time order.\n\nLike `flag`, meta rows are filtered from default tail / now views\n(use `tail --show-meta` to include them).\n";

const KIT_BODY: &str = "USAGE:\n  datum kit install <PATH|-> [--force] [--yes] [--dry-run]\n  datum kit export <NAME> [PATH]\n  datum kit list\n  datum kit uninstall <NAME>\n  datum kit show <NAME>\n\nA kit is a TOML bundle of field specs, aliases, SQL views, indices, and\nlevel-1/2 commands. Install upserts the kit's rows tagged with the kit id;\nuninstall drops every row with that id. Export round-trips the installed\nrows back to TOML (defaults to stdout when path is omitted). Install reads\nthe TOML from stdin when PATH is `-`; --dry-run prints what the kit brings\nand writes nothing; --yes approves its indices without asking.\n\nList shows every installed kit and its field count. Show prints a single\nkit's installed contents.\n";

const VIEW_BODY: &str = "USAGE:\n  datum view <NAME> [--format FMT] [--watch]\n  datum view --list\n\nARGS:\n  <NAME>           Installed view name (see `datum view --list`).\n\nOPTIONS:\n  -f, --format <FMT>   pretty | tsv | csv | json | auto (default auto).\n  -w, --watch          Re-run the query every 2s; clear-screen between\n                       redraws.\n  --list               Print installed view names and exit.\n\nViews execute under PRAGMA query_only = 1, so a malicious or buggy\nview cannot mutate the DB.\n";

const REPARSE_BODY: &str = "USAGE:\n  datum reparse <HID[,HID]...>... [--fix]\n\nARGS:\n  <HID>...      One or more entries to replay. At least one is required;\n                reparse never walks the entry log implicitly.\n\nOPTIONS:\n  --fix         UPDATE rows where the parser produced a different shape\n                from the persisted columns; bumps `parser_version` and\n                recomputes `content_hash`. Default is dry-run (report only).\n\nReplay is hermetic to the alias state at write time — the entry's\nstored `alias_chain` is rehydrated as the resolver, never the live\n`aliases` table (da-bkak).\n";

// ---------------------------------------------------------------------------
// Global flags shown in `datum --help` under `GLOBAL OPTIONS:`.
// ---------------------------------------------------------------------------

pub const GLOBAL_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        short: None,
        long: "db",
        takes_value: true,
        value_hint: Some("NAME_OR_PATH"),
        value_optional: false,
        summary: "Override the active DB",
        long_description: None,
        left_override: Some("--db <NAME_OR_PATH>"),
    },
    FlagSpec {
        short: None,
        long: "show",
        takes_value: true,
        value_hint: Some("MODE"),
        value_optional: false,
        summary: "default|none|minimal|standard|format|all|json",
        long_description: None,
        left_override: Some("--show <MODE>"),
    },
    FlagSpec {
        short: None,
        long: "json",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Shorthand for --show json",
        long_description: None,
        left_override: Some("--json"),
    },
    FlagSpec {
        short: Some('A'),
        long: "show-all",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Shorthand for --show all",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('o'),
        long: "format-string",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "%field%-templated output string",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('h'),
        long: "help",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Show this help",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('V'),
        long: "version",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Show version",
        long_description: None,
        left_override: None,
    },
];

/// Column at which global-flag summaries align in `datum --help`.
pub const GLOBAL_FLAGS_COL: usize = 24;

/// Top-level USAGE line (single line under `USAGE:`).
pub const TOP_LEVEL_USAGE: &str = "datum [--db NAME_OR_PATH] <command> [args]";

/// Footer printed after `GLOBAL OPTIONS:`.
pub const TOP_LEVEL_FOOTER: &str = "Run `datum <command> --help` for per-command flag detail.\n\
     With no subcommand, `datum` drops into an interactive REPL \
     (inherits `--db`).";

// ---------------------------------------------------------------------------
// COMMANDS table — listed in display order for each category.
// ---------------------------------------------------------------------------

/// `note`'s flag table: [`LOG_FLAGS`] without `-C`.
///
/// da-py5w made `-C` implicit on `note` — every bare word after the field
/// joins the comment already — so listing the flag would offer a reader a
/// switch that turns nothing on.
const NOTE_FLAGS: &[FlagSpec] = &drop_short(LOG_FLAGS, 'C');

/// Copy `src` minus the flag whose short form is `short`.
const fn drop_short(src: &[FlagSpec], short: char) -> [FlagSpec; LOG_FLAGS.len() - 1] {
    let mut out = [src[0]; LOG_FLAGS.len() - 1];
    let (mut i, mut j) = (0, 0);
    while i < src.len() {
        match src[i].short {
            Some(c) if c == short => {}
            _ => {
                out[j] = src[i];
                j += 1;
            }
        }
        i += 1;
    }
    out
}

// ---------------------------------------------------------------------------
// The shared logging wall, and what each verb keeps on its own page (da-ce7n3).
//
// `add`, `note`, `occur`, `start`, `stop` and `switch` all take LOG_FLAGS,
// so their `--help` pages were 38-40 lines of which about thirty were
// byte-identical — and the lines that told them apart were buried in the
// middle of that. The wall is written out once, on `datum help logging`;
// each verb's page keeps what is PARTICULAR to it and points there.
// ---------------------------------------------------------------------------

/// Nothing particular: the verb is told apart by its USAGE lines.
const LOG_SHARED_FLAGS: SharedFlags = SharedFlags {
    topic: "logging",
    own: &[],
};

/// `add` is the one verb that REQUIRES an id, so the two flags that give it
/// one stay on its page, worded for `add` rather than for the shared table.
const ADD_SHARED_FLAGS: SharedFlags = SharedFlags {
    topic: "logging",
    own: &[
        FlagSpec {
            short: Some('K'),
            long: "slug-key",
            takes_value: true,
            value_hint: Some("S"),
            value_optional: false,
            summary: "Required: derive the id from this key (K / K= / K=val)",
            long_description: None,
            left_override: None,
        },
        FlagSpec {
            short: None,
            long: "id",
            takes_value: true,
            value_hint: Some("TEMPLATE"),
            value_optional: false,
            summary: "...or set the id directly (e.g. %field%:%uid%)",
            long_description: None,
            left_override: None,
        },
    ],
};

/// `datum help <NAME>` pages that are not commands.
pub const TOPICS: &[TopicSpec] = &[TopicSpec {
    name: "logging",
    summary: "the options every logging verb takes",
    intro: "These are read by `add`, `note`, `occur`, `start`, `stop` and `switch`.\n\
            Each verb's own page lists only what is particular to it.\n",
    flags: LOG_FLAGS,
    summary_col: 24,
    outro: "\n`note` is the exception: it has no -C/--comment-remainder, because it\n\
            always behaves as if -C had been given — every un-keyed word after\n\
            the field joins extra_data.comment.\n",
}];

/// Look up a [`TopicSpec`] by name.
pub fn find_topic(query: &str) -> Option<&'static TopicSpec> {
    TOPICS.iter().find(|t| t.name == query)
}

// ---------------------------------------------------------------------------
// Environment variables (da-ce7n3).
//
// The README kept this table by hand, which is how `DATUM_ID_DELIMITER`
// went undocumented and `DATUM_EDITOR` was described as `EDITOR`. The
// README's table is generated from here by `cargo xtask readme`, and
// `env_table_covers_what_the_config_layer_reads` (datum-cli) fails when
// the config layer reads a variable this table does not name.
// ---------------------------------------------------------------------------

pub const ENV_VARS: &[EnvVarSpec] = &[
    EnvVarSpec {
        name: "DATUM_DB",
        summary: "Path or registry name of the active database (`--db` wins).",
    },
    EnvVarSpec {
        name: "DATUM_HOME",
        summary: "Override the data directory (default: the OS data dir).",
    },
    EnvVarSpec {
        name: "DATUM_CONFIG",
        summary: "Path to a specific `datumrc.toml`.",
    },
    EnvVarSpec {
        name: "DATUM_CONFIG_HOME",
        summary: "Override the directory `datumrc.toml` lives in.",
    },
    EnvVarSpec {
        name: "DATUM_DEFAULT_FIELD",
        summary: "Field used when one is omitted from the argv.",
    },
    EnvVarSpec {
        name: "DATUM_DEVICE_ID",
        summary: "Override the per-device id stamped on entries.",
    },
    EnvVarSpec {
        name: "DATUM_EDITOR",
        summary: "Editor for `datum edit`. Read FIRST, before toml `editor`, `$VISUAL`, `$EDITOR`.",
    },
    EnvVarSpec {
        name: "DATUM_ID_DELIMITER",
        summary:
            "Delimiter that collapses empty `%key%` segments in a slug template (default `:`).",
    },
    EnvVarSpec {
        name: "DATUM_WEEK_START",
        summary: "Weekday a week opens on (`monday`…`sunday`, or `1`-`7`; default Monday).",
    },
    EnvVarSpec {
        name: "DATUM_COLOR",
        summary: "`0` disables colour; otherwise auto-detect from the TTY.",
    },
];

/// Worked alias examples, each one run against the binary before it was
/// written down (da-ce7n3). They render into the man page AND into the
/// README's generated alias reference, so there is no hand copy to drift.
const ALIAS_EXAMPLES: &[ExampleSpec] = &[
    ExampleSpec {
        command: "datum alias set vbp \"note word srcLang=pt pos=verb\"\ndatum vbp correr \"to run\"",
        description: "An alias stands in for any PREFIX of an invocation — verb, field, flags, data keys. The call above writes a `word` note with `srcLang=pt`, `pos=verb` and `comment=\"correr to run\"`.",
    },
    ExampleSpec {
        command: "datum switch body vertical , switch body sit",
        description: "A whitespace-bounded `,` splits one invocation into two sides that write atomically at one `occur_time`. Same field, same instant, so the two merge into ONE `body` row with `state = [vertical, sit]` rather than overwriting each other.",
    },
    ExampleSpec {
        command: "datum alias set oov \"switch environment outside , switch body vertical\"\ndatum oov",
        description: "The same trigger inside an alias body: `datum oov` writes both rows atomically.",
    },
    ExampleSpec {
        command: "datum alias set caf5 \"occur caffeine -t +5\"\ndatum caf5        # now + 5\ndatum caf5 -t +10 # now + 15\ndatum caf5 -q     # now + 5, then 5 back: now",
        description: "Call-site time flags compose with the alias-bound ones: `-y` / `-q` / `-d` shift whatever time the alias bound, and a relative `-t` adds to it.",
    },
];

pub const COMMANDS: &[CommandSpec] = &[
    // ----- LOGGING -----
    CommandSpec {
        name: "add",
        aliases: &[],
        category: Category::Logging,
        summary: "register a keyed timeless record (needs an id)",
        long_description: Some(
            "Timeless record identified by *what* it is. `add` requires an explicit id: derive one from a key (-K name) or set it directly (--id <value>); see --id %templates% and id= below. Sibling of `note`, which auto-ids on its create_time instead.",
        ),
        usage: &["datum add <FIELD> -K <KEY> [KEY=VALUE]...", "datum add <FIELD> --id <VALUE>"],
        args: NO_ARGS,
        flags: LOG_FLAGS,
        shared_flags: Some(&ADD_SHARED_FLAGS),
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "note",
        aliases: &[],
        category: Category::Logging,
        summary: "jot a timeless entry (auto create_time id)",
        long_description: Some(
            "Timeless jot identified by *when* — auto-ids on its create_time (slug field:c<create_time>), so many notes per field coexist. \"Record this moment, won't look it up.\" Sibling of `add`, which instead requires an explicit id for a keyed record.\n\nBare words are the note: every un-keyed word after the field joins `extra_data.comment`, in order and single-spaced, so `datum note coffee too strong today` needs no quotes and no `-C` (`note` always behaves as if `-C` were given). `KEY=VALUE` anywhere in the line is still data, a leading `-` is still a flag, and an explicit `-R <bucket>` still names where the words go instead.",
        ),
        usage: &[
            "datum note <FIELD> [WORD | KEY=VALUE]...",
            "    the WORDs join into extra_data.comment — no quotes, no -C",
        ],
        args: NO_ARGS,
        flags: NOTE_FLAGS,
        shared_flags: Some(&LOG_SHARED_FLAGS),
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "occur",
        aliases: &[],
        category: Category::Logging,
        summary: "record a point-in-time event",
        long_description: None,
        usage: &["datum occur <FIELD> [VALUE | KEY=VALUE]..."],
        args: NO_ARGS,
        flags: LOG_FLAGS,
        shared_flags: Some(&LOG_SHARED_FLAGS),
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "start",
        aliases: &[],
        category: Category::Logging,
        summary: "begin a duration-tracked activity",
        long_description: Some(
            "A trailing duration (`datum start work 25m`, or `dur=25m`) closes the \
             block: it ENDS at the entry's time — now, or wherever `-t`/`-d` put \
             it — and the span runs BACK from there. `datum start work 25m -t \
             12:30` is work from 12:05 to 12:30.",
        ),
        usage: &[
            "datum start <FIELD> [VALUE | KEY=VALUE]... [DUR]",
            "  DUR (or dur=DUR) runs BACK from the entry's time",
        ],
        args: NO_ARGS,
        flags: LOG_FLAGS,
        shared_flags: Some(&LOG_SHARED_FLAGS),
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "stop",
        aliases: &["end"],
        category: Category::Logging,
        summary: "end a duration-tracked activity (alias: end)",
        long_description: Some(
            "A trailing duration (`datum end work 12m`, or `dur=12m`) is how long the \
             break ran: `-t` is when it ENDED, and the span runs BACK from \
             there. `end work 12m -t 14:12` is a break from 14:00 to 14:12.",
        ),
        usage: &[
            "datum stop <FIELD> [VALUE | KEY=VALUE]... [DUR]",
            "  DUR (or dur=DUR) runs BACK from the entry's time:",
            "  `datum end work 12m -t 14:12` is a break from 14:00 to 14:12",
        ],
        args: NO_ARGS,
        flags: LOG_FLAGS,
        shared_flags: Some(&LOG_SHARED_FLAGS),
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        // The legacy log_help echoed back the verb the user typed, so
        // `datum end --help` showed `datum end — log an entry`. Preserve
        // that by switching to alias-name rendering when looked up via
        // the alias table.
        display_input_name: true,
    },
    CommandSpec {
        name: "switch",
        aliases: &[],
        category: Category::Logging,
        summary: "transition a field to a new state",
        long_description: None,
        usage: &["datum switch <FIELD> [VALUE | KEY=VALUE]..."],
        args: NO_ARGS,
        flags: LOG_FLAGS,
        shared_flags: Some(&LOG_SHARED_FLAGS),
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        display_input_name: false,
    },
    // ----- READ -----
    CommandSpec {
        name: "tail",
        aliases: &[],
        category: Category::Read,
        summary: "show recent entries",
        long_description: Some(
            "Structured-state entries show their id only on the chip line \
             (`{id}`); use `datum get` for the full structure.\n\n\
             The table's columns are a display choice and do change — do not \
             position-parse them from a script. Pass --json for named fields.",
        ),
        usage: &["datum tail [FIELD]"],
        args: NO_ARGS,
        flags: TAIL_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 24,
        // Tail is structurally rendered (no body_override); see
        // render::render_command_structured.
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "head",
        aliases: &[],
        category: Category::Read,
        summary: "show oldest entries",
        long_description: None,
        usage: &["datum head [FIELD]"],
        args: NO_ARGS,
        flags: HEAD_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 20,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "get",
        aliases: &[],
        category: Category::Read,
        summary: "print entries by id or slug",
        long_description: None,
        usage: &["datum get <ID_OR_SLUG[,ID_OR_SLUG]...> [--pretty]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(GET_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "grep",
        aliases: &[],
        category: Category::Read,
        summary: "regex search over entry JSON content",
        long_description: None,
        usage: &["datum grep <PATTERN>... [-f FIELD]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(GREP_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "dayview",
        aliases: &[],
        category: Category::Render,
        summary: "render a multi-day timeline as SVG/PNG/HTML",
        long_description: None,
        usage: &["datum dayview [OPTIONS]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 27,
        body_override: Some(DAYVIEW_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "nowview",
        aliases: &[],
        category: Category::Render,
        summary: "paint each field's state now as a terminal band (-o writes a file)",
        long_description: None,
        usage: &["datum nowview [OPTIONS]"],
        args: NO_ARGS,
        flags: NOWVIEW_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 27,
        body_override: Some(NOWVIEW_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "now",
        aliases: &[],
        category: Category::Render,
        summary: "show the current state of tracked fields + terminal nowview band",
        long_description: None,
        usage: &["datum now [-A] [--bare]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(NOW_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "history",
        aliases: &[],
        category: Category::Read,
        summary: "show the full modification trail for an entry",
        long_description: None,
        usage: &["datum history <ID_OR_SLUG[,ID_OR_SLUG]...> [--version N | --diff A B]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(HISTORY_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "why",
        aliases: &[],
        category: Category::Read,
        summary: "explain the intent behind an entry",
        long_description: None,
        usage: &["datum why <ID_OR_SLUG[,ID_OR_SLUG]...> [--full]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(WHY_BODY),
        display_input_name: false,
    },
    // ----- MUTATE -----
    CommandSpec {
        name: "edit",
        aliases: &[],
        category: Category::Mutate,
        summary: "open an entry in the configured editor",
        long_description: None,
        usage: &["datum edit <ID_OR_SLUG>"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(EDIT_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "update",
        aliases: &[],
        category: Category::Mutate,
        summary: "replace keys on an existing entry",
        long_description: None,
        usage: &["datum update <ID_OR_SLUG[,ID_OR_SLUG]...> [VALUE | KEY=VALUE]..."],
        args: UPDATE_ARGS,
        flags: UPDATE_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 26,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "merge",
        aliases: &[],
        category: Category::Mutate,
        summary: "deep-merge new values into an entry",
        long_description: None,
        usage: &["datum merge <ID_OR_SLUG[,ID_OR_SLUG]...> [VALUE | KEY=VALUE]..."],
        args: NO_ARGS,
        flags: UPDATE_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 26,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "rekey",
        aliases: &[],
        category: Category::Mutate,
        summary: "copy an entry to a new id (old row stays)",
        long_description: None,
        usage: &["datum rekey <ID_OR_SLUG[,ID_OR_SLUG]...> [VALUE | KEY=VALUE]..."],
        args: NO_ARGS,
        flags: UPDATE_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 26,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "retime",
        aliases: &["rt"],
        category: Category::Mutate,
        summary: "move an entry's occur_time (alias: rt)",
        long_description: None,
        usage: &[
            "datum retime [SLUG[,SLUG]...] [SHIFT]",
            "datum retime [SLUG[,SLUG]...] [-r] [-t T] [-y]... [-q]... [-d D]",
            "datum retime [SLUG[,SLUG]...] --offset DUR [--field F] [--after BND] [--before BND]",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(RETIME_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "delete",
        aliases: &["del"],
        category: Category::Mutate,
        summary: "delete an entry (alias: del)",
        long_description: None,
        usage: &["datum delete [ID_OR_SLUG[,ID_OR_SLUG]...] [--force] [--dry-run] [--strict]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(DELETE_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "undo",
        aliases: &[],
        category: Category::Mutate,
        summary: "undo the last change to an entry (revert via history)",
        long_description: Some(
            "Reverts an entry to a prior version using its entry_history trail. \
             By default, undo restores the immediately-previous version; pass \
             --to-version N to restore an explicit version number. \
             For legacy entries without a history trail, undo falls back to the \
             most recent tombstone. Undo never refuses on age: past ten minutes \
             it warns and proceeds, and `datum redo` is the way forward again.",
        ),
        usage: &["datum undo [ID_OR_SLUG[,ID_OR_SLUG]...] [--to-version N]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(UNDO_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "redo",
        aliases: &[],
        category: Category::Mutate,
        summary: "redo the last undone change (the inverse of undo)",
        long_description: Some(
            "Re-applies the version that the last undo on that entry stepped \
             back from. Redo of an undone create re-creates it under the same \
             hid; redo of an undone delete deletes it again. With no ID_OR_SLUG \
             it acts on `_LAST`. Exits 1 when there is nothing ahead.",
        ),
        usage: &["datum redo [ID_OR_SLUG[,ID_OR_SLUG]...]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(REDO_BODY),
        display_input_name: false,
    },
    // ----- ADMIN -----
    CommandSpec {
        name: "spec",
        aliases: &[],
        category: Category::Admin,
        summary: "manage field specs (schemas)",
        long_description: None,
        usage: &[
            "datum spec show <FIELD> [--text]",
            "datum spec add-state <FIELD> <NAME> [COLOR] [DESCRIPTION]",
            "datum spec set <FIELD> [--from-json JSON] [--verb V] [--state S]... [--tag T]... ...",
            "datum spec list [--tag T]",
            "datum spec remove <FIELD>",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(SPEC_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "alias",
        aliases: &[],
        category: Category::Admin,
        summary: "manage aliases",
        long_description: Some(ALIAS_LONG_DESCRIPTION),
        usage: &[
            "datum alias show <NAME>",
            "datum alias set <NAME> <EXPANSION>",
            "datum alias list",
            "datum alias remove <NAME>",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: ALIAS_EXAMPLES,
        summary_col: 0,
        body_override: Some(ALIAS_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "track",
        aliases: &[],
        category: Category::Logging,
        summary: "track a field in the now view",
        long_description: None,
        usage: &["datum track <FIELD> [--verb V] [--color C] [--state S|S=C]... [--tag T]..."],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(TRACK_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "ignore",
        aliases: &[],
        category: Category::Logging,
        summary: "ignore a field in the now view",
        long_description: None,
        usage: &["datum ignore <FIELD>"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(IGNORE_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "check",
        aliases: &[],
        category: Category::Admin,
        summary: "run database consistency checks",
        long_description: None,
        usage: &["datum check [CHECKER]... [--fix]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(CHECK_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "db",
        aliases: &[],
        category: Category::Admin,
        summary: "DB-lifecycle (setup, purge, backup, restore, migrate, v2import)",
        long_description: None,
        usage: &[
            "datum db setup",
            "datum db purge [--yes | -y]",
            "datum db backup <PATH | -> [--overwrite]",
            "datum db restore <PATH | -> [--force] [--dry-run]",
            "datum db migrate (run | status | edit)",
            "datum db v2import [FILE | - | --from FILE] [--dry-run]",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(DB_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "info",
        aliases: &[],
        category: Category::Admin,
        summary: "show health/metadata for DB(s)",
        long_description: None,
        usage: &["datum info [TARGET] [--all]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(INFO_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "config",
        aliases: &[],
        category: Category::Admin,
        summary: "show or manage user configuration",
        long_description: None,
        usage: &["datum config show"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(CONFIG_BODY),
        display_input_name: false,
    },
    // ----- VIEWS / SYNC -----
    CommandSpec {
        name: "count",
        aliases: &[],
        category: Category::ViewsSync,
        summary: "count entries (optionally filtered/grouped/since-bounded)",
        long_description: None,
        usage: &["datum count [FIELD] [--group-by KEY] [--since TIME]"],
        args: NO_ARGS,
        flags: COUNT_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "stats",
        aliases: &[],
        category: Category::ViewsSync,
        summary: "count, total duration, and per-state ratio over a time slice",
        long_description: None,
        usage: &[
            "datum stats  (no FIELD: every field, ranked by tracked time)",
            "datum stats <FIELD[:STATE]> [SLICE] [--since TIME] [--until TIME]",
            "datum stats <FIELD[:STATE]> (--today | --week | --last-week | --month | --last-month)",
            "datum stats <FIELD[:STATE]> [-d DATE | -y* | -t TIME | -q*] [--until TIME]",
            "datum stats <FIELD[:STATE]> [--include-false] [--include-untracked]",
        ],
        args: NO_ARGS,
        flags: STATS_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "ids",
        aliases: &[],
        category: Category::ViewsSync,
        summary: "list entry ids (newest-first by default)",
        long_description: None,
        usage: &["datum ids [FIELD] [-n N] [--oldest-first] [--since TIME] [--slug]"],
        args: NO_ARGS,
        flags: IDS_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 22,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "fields",
        aliases: &[],
        category: Category::ViewsSync,
        summary: "list every field: its spec and the states it was logged with",
        long_description: None,
        usage: &["datum fields [--include-ignored | -A]"],
        args: NO_ARGS,
        flags: FIELDS_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 25,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "sync",
        aliases: &[],
        category: Category::ViewsSync,
        summary: "push/pull changesets, manage auth & keys",
        long_description: None,
        usage: &[
            "datum sync [--push | --pull] [--dry-run] [--server URL]",
            "datum sync login [--server URL] [--cluster NAME]",
            "datum sync config set-server <URL> [--global]",
            "datum sync config disable",
            "datum sync config show",
            "datum sync key (init | unlock | rotate-passphrase | show-fingerprint) [--server URL] [--force]",
            "datum sync status [--server URL]",
            "datum sync install-daemon [--show | --uninstall]",
            "datum sync agent [--pid-file PATH] [-v] [--one-shot]",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(SYNC_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "conflicts",
        aliases: &[],
        category: Category::ViewsSync,
        summary: "triage sync conflicts (see `datum check` for integrity findings)",
        long_description: None,
        usage: &[
            "datum conflicts [list] [--all]",
            "datum conflicts resolve <HID> (--take-local | --take-remote | --keep-both)",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(CONFLICTS_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "flag",
        aliases: &[],
        category: Category::Logging,
        summary: "mark a field for review (lightweight 'come back to this')",
        long_description: None,
        usage: &["datum flag <FIELD> [COMMENT...]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(FLAG_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "meta",
        aliases: &[],
        category: Category::Admin,
        summary: "attach or list protocol-change notes on a field",
        long_description: None,
        usage: &[
            "datum meta <FIELD> <TEXT>...",
            "datum meta show <FIELD>",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(META_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "kit",
        aliases: &[],
        category: Category::Admin,
        summary: "install / export / list field-spec kits (TOML bundles)",
        long_description: None,
        usage: &[
            "datum kit install <PATH|-> [--force] [--yes] [--dry-run]",
            "datum kit export <NAME> [PATH]",
            "datum kit list",
            "datum kit uninstall <NAME>",
            "datum kit show <NAME>",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(KIT_BODY),
        display_input_name: false,
    },
    // View / reparse — both were dispatched by the parser and answered
    // `--help` via the parser's HelpRequested error before having a
    // CommandSpec row; that meant they were invisible to the top-level
    // `datum --help` category listing (the loop walks COMMANDS, not the
    // parser's subcommand match). Adding their rows here surfaces them
    // in the table without changing any behavior.
    CommandSpec {
        name: "view",
        aliases: &[],
        category: Category::ViewsSync,
        summary: "execute a named SELECT view and render the rows",
        long_description: None,
        usage: &[
            "datum view <NAME> [--format pretty|tsv|csv|json] [--watch]",
            "datum view --list",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(VIEW_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "reparse",
        aliases: &[],
        category: Category::Admin,
        summary: "replay an entry's stored raw_input through the current parser",
        long_description: None,
        usage: &["datum reparse <HID[,HID]...>... [--fix]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(REPARSE_BODY),
        display_input_name: false,
    },
];

/// Look up a `CommandSpec` by primary name or alias. Returns `(spec, was_alias)`.
pub fn find_command(query: &str) -> Option<(&'static CommandSpec, bool)> {
    for cmd in COMMANDS {
        if cmd.name == query {
            return Some((cmd, false));
        }
        if cmd.aliases.contains(&query) {
            return Some((cmd, true));
        }
    }
    None
}
