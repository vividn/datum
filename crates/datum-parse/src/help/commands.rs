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
//! - **Multi-`USAGE` / subcommand trees** (`spec`, `db`, `conflicts`,
//!   `alias`): several calling shapes and prose sections.
//! - **`sync`**: Easy Sync's tree of groups (da-aam46).
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
    ArgSpec, Category, CommandSpec, EnvVarSpec, ExampleSpec, FlagSpec, FormGroup, FormSpec,
    SharedFlags, SubcommandSpec, TopicSpec,
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
const NO_SUBCOMMANDS: &[SubcommandSpec] = &[];

/// A subcommand whose flags are written out in its parent's body, as all of
/// today's are.
const fn sub(name: &'static str, usage: &'static [&'static str]) -> SubcommandSpec {
    SubcommandSpec {
        name,
        usage,
        flags: NO_FLAGS,
    }
}

// Each command's subcommands, in the order the parser's error names them.
// `help_json_subcommands_are_the_parsers` holds these to the parser.
const SPEC_SUBCOMMANDS: &[SubcommandSpec] = &[
    sub("show", &["datum spec show <FIELD> [--text]"]),
    sub(
        "add-state",
        &["datum spec add-state <FIELD> <NAME> [COLOR] [DESCRIPTION] [GLYPH] [--key K]... [--assign K=V]..."],
    ),
    sub(
        "set",
        &["datum spec set <FIELD> [--from-json JSON] [--verb V] [--state S]... [--tag T]... ..."],
    ),
    sub("list", &["datum spec list [--tag T]"]),
    sub("remove", &["datum spec remove <FIELD>"]),
];
const ALIAS_SUBCOMMANDS: &[SubcommandSpec] = &[
    sub("show", &["datum alias show <NAME>"]),
    sub("set", &["datum alias set <NAME> <EXPANSION>"]),
    sub("list", &["datum alias list"]),
    sub("remove", &["datum alias remove <NAME>"]),
];
const DB_SUBCOMMANDS: &[SubcommandSpec] = &[
    sub("setup", &["datum db setup"]),
    sub("purge", &["datum db purge [--yes | -y]"]),
    sub(
        "backup",
        &[
            "datum db backup <PATH | -> [--overwrite]",
            "datum db backup --to DIR --every DUR [--slots N]",
            "datum db backup --list DIR",
            "datum db backup --plan --every DUR [--slots N] --span DUR",
        ],
    ),
    sub(
        "restore",
        &["datum db restore <PATH | -> [--merge | --hard] [--force] [--dry-run] [--yes | -y] [--if-unchanged FP]"],
    ),
    sub("migrate", &["datum db migrate (run | status | edit)"]),
    sub(
        "v2import",
        &["datum db v2import [FILE | - | --from FILE] [--dry-run]"],
    ),
];
const CONFIG_SUBCOMMANDS: &[SubcommandSpec] = &[sub("show", &["datum config show"])];
const COMPLETIONS_SUBCOMMANDS: &[SubcommandSpec] = &[
    sub("zsh", &["datum completions <zsh|bash|fish>"]),
    sub("bash", &["datum completions <zsh|bash|fish>"]),
    sub("fish", &["datum completions <zsh|bash|fish>"]),
    sub(
        "install",
        &["datum completions install [--shell zsh|bash|fish]"],
    ),
    sub(
        "uninstall",
        &["datum completions uninstall [--shell zsh|bash|fish]"],
    ),
];
const COMPLETIONS_FLAGS: &[FlagSpec] = &[FlagSpec {
    short: None,
    long: "shell",
    takes_value: true,
    value_hint: Some("SHELL"),
    value_optional: false,
    summary: "zsh, bash or fish (install/uninstall; default: from $SHELL)",
    long_description: None,
    left_override: None,
}];
const CONFLICTS_SUBCOMMANDS: &[SubcommandSpec] = &[
    sub("list", &["datum conflicts [list] [--all]"]),
    sub(
        "resolve",
        &["datum conflicts resolve <HID> (--take-local | --take-remote | --keep-both)"],
    ),
];
const SYNC_SUBCOMMANDS: &[SubcommandSpec] = &[
    sub("status", &["datum sync status [--json]"]),
    sub("pause", &["datum sync pause"]),
    sub("resume", &["datum sync resume"]),
    sub(
        "chain",
        &[
            "datum sync chain invite [--reader] [--fields <F,…> | --kit <KIT>] [--qr] [--expires <DUR>] [--admin-key <FILE>]",
            "datum sync chain connect [<TICKET>]",
            "datum sync chain leave",
            "datum sync chain list [--json]",
            "datum sync chain rename [<WHO>] <LABEL> [--admin-key <FILE>]",
            "datum sync chain revoke <WHO> [--admin-key <FILE>]",
            "datum sync chain purge <WHO> [--yes] [--json] [--admin-key <FILE>]",
            "datum sync chain role <WHO> <member|reader> [--fields <F,…> | --kit <KIT>] [--admin-key <FILE>]",
            "datum sync chain ack <WHO|unlock>",
            "datum sync chain rekey",
        ],
    ),
    sub(
        "admin",
        &[
            "datum sync admin list [--json]",
            "datum sync admin add <KEYFILE|PASSKEY-JSON> [--label <L>] [--admin-key <FILE>]",
            "datum sync admin remove <KEY|LABEL> [--allow-unlock] [--admin-key <FILE>]",
        ],
    ),
    sub(
        "daemon",
        &[
            "datum sync daemon install",
            "datum sync daemon uninstall",
            "datum sync daemon status",
        ],
    ),
];
const META_SUBCOMMANDS: &[SubcommandSpec] = &[sub("show", &["datum meta show <FIELD>"])];
const KIT_SUBCOMMANDS: &[SubcommandSpec] = &[
    sub(
        "install",
        &["datum kit install <PATH|-> [--force] [--yes] [--dry-run]"],
    ),
    sub("export", &["datum kit export <NAME> [PATH]"]),
    sub("list", &["datum kit list"]),
    sub("uninstall", &["datum kit uninstall <NAME>"]),
    sub("show", &["datum kit show <NAME>"]),
];

// ---------------------------------------------------------------------------
// The time arguments (da-6yqu9): one FlagSpec per flag of
// `time_args::FLAG_SPECS`, which every table below and `datum help time`
// render from. A command that reads a value its own way (`tail`'s `-t` is
// an upper bound) keeps the flag and rewords only its summary.
// `time_page_flags_are_the_parsers` holds the set to the parser.
// ---------------------------------------------------------------------------

const YESTERDAY: FlagSpec = FlagSpec {
    short: Some('y'),
    long: "yesterday",
    takes_value: false,
    value_hint: None,
    value_optional: false,
    summary: "Go back 1 day (stackable: -yy=2 days, ...)",
    long_description: None,
    left_override: None,
};
const QUICK: FlagSpec = FlagSpec {
    short: Some('q'),
    long: "quick",
    takes_value: false,
    value_hint: None,
    value_optional: false,
    summary: "Go back 5 min (stackable: -qq=10 min, ...)",
    long_description: None,
    left_override: None,
};
const DATE: FlagSpec = FlagSpec {
    short: Some('d'),
    long: "date",
    takes_value: true,
    value_hint: Some("VAL"),
    value_optional: false,
    summary: "Date override (offset, YYYY-MM-DD, sunday, apr29)",
    long_description: None,
    left_override: None,
};
const TIME: FlagSpec = FlagSpec {
    short: Some('t'),
    long: "time",
    takes_value: true,
    value_hint: Some("VAL"),
    value_optional: false,
    summary: "Time override (offset, HH:MM, 9:15am, a date, or both)",
    long_description: None,
    left_override: None,
};
const TIMEZONE: FlagSpec = FlagSpec {
    short: Some('z'),
    long: "timezone",
    takes_value: true,
    value_hint: Some("TZ"),
    value_optional: false,
    summary: "Timezone (IANA or UTC offset)",
    long_description: None,
    left_override: None,
};
const FULL_DAY: FlagSpec = FlagSpec {
    short: Some('D'),
    long: "full-day",
    takes_value: false,
    value_hint: None,
    value_optional: false,
    summary: "Date-only entry (no clock time)",
    long_description: None,
    left_override: None,
};
const NO_TIMESTAMP: FlagSpec = FlagSpec {
    short: Some('T'),
    long: "no-timestamp",
    takes_value: false,
    value_hint: None,
    value_optional: false,
    summary: "Timeless note-data (create_time id, no occurrence)",
    long_description: None,
    left_override: None,
};

/// The whole time-argument group, as `datum help time` lists it.
const TIME_FLAGS: &[FlagSpec] = &[
    YESTERDAY,
    QUICK,
    DATE,
    TIME,
    TIMEZONE,
    FULL_DAY,
    NO_TIMESTAMP,
];

// ---------------------------------------------------------------------------
// Logging verbs — they share one flag table and one usage shape.
// ---------------------------------------------------------------------------

const LOG_FLAGS: &[FlagSpec] = &[
    YESTERDAY,
    QUICK,
    DATE,
    TIME,
    TIMEZONE,
    FULL_DAY,
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
        short: Some('F'),
        long: "field-key",
        takes_value: true,
        value_hint: Some("K"),
        value_optional: false,
        summary: "Field-contributing key: field__<value> (same K forms)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('c'),
        long: "comment",
        takes_value: true,
        value_hint: Some("T"),
        value_optional: false,
        summary: "Append a comment to data.comment (repeatable)",
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
        summary: "Remainder positionals → one appended comment",
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
        summary: "Zero created_at/modified_at, clear client_id",
        long_description: None,
        left_override: None,
    },
    NO_TIMESTAMP,
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
    IF_CHANGED,
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
        summary: "Centre the window on this instant",
        ..TIME
    },
    FlagSpec {
        summary: "Date of the instant",
        ..DATE
    },
    QUICK,
    YESTERDAY,
    FlagSpec {
        summary: "Zone -t and -d are read in",
        ..TIMEZONE
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
    DATE,
    FlagSpec {
        summary: "Upper-bound time",
        ..TIME
    },
    YESTERDAY,
    QUICK,
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
        summary:
            "Include flag/meta and spec/layout/alias/view/command/index entries (hidden by default)",
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
        summary: "Append a comment to data.comment (repeatable)",
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
        long: "all-or-nothing",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Change nothing if any batched id fails",
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
        summary: "state|verb|field|client",
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
        long: "group-by",
        takes_value: true,
        value_hint: Some("KEY"),
        value_optional: false,
        summary: "client: one row per client that wrote the field",
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
        summary: "A date names that whole day (unless --until)",
        ..DATE
    },
    FlagSpec {
        summary: "A time is a lower bound, like --since",
        ..TIME
    },
    FlagSpec {
        summary: "Zone the period boundaries are read in",
        ..TIMEZONE
    },
    FlagSpec {
        summary: "Go back 1 day (stackable); a whole day",
        ..YESTERDAY
    },
    QUICK,
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
const FIELDS_FLAGS: &[FlagSpec] = &[
    FlagSpec {
        short: Some('A'),
        long: "include-ignored",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "List ignored fields too (same as `--show all`)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "sort",
        takes_value: true,
        value_hint: Some("frecency"),
        value_optional: false,
        summary: "Most used first, recent use weighing more (halving per week)",
        long_description: None,
        left_override: None,
    },
];

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

const GET_BODY: &str = "USAGE:\n  datum get <ID_OR_SLUG[,ID_OR_SLUG]...> [--pretty]\n  datum get <ID_OR_SLUG>::<NAME>[,<ID_OR_SLUG>::<NAME>]...\n  datum get <ID_OR_SLUG>::<REF>[::<REF>]...::<NAME>\n\nA comma list prints each entry in the order given (`--pretty` and `--show json`\ntoo). An id that does not resolve is named on stderr after the documents that\ndid, and the command exits 1.\n\n`<id>::<name>` prints one value of the entry alone on stdout, for scripts:\n  datum get di91z::hid       the full 12-char hid\n  datum get di91z::state     the whole state\n  datum get di91z::.title    state.title (the `.key` shorthand of data entry)\n  datum get di91z::priority  a data key\nA name is resolved in this order: an entry column (hid, slug, field, verb,\nstate, dur, following_state, data, occur_time, create_time, modify_time, …)\nalways means the column; `.x.y` walks the state; anything else walks data.\nA string prints bare, an object or array as compact JSON. A missing or null\nvalue exits 1 with a message on stderr; an empty string prints an empty line\nand exits 0. A path splits on `.` as data entry does. `::name` alone reads\nthe last entry. A comma list prints one value per line and takes only\n`<id>::<name>` pieces; `--show` and `--pretty` do not apply.\n\n`::state::<name>` follows the state (or following_state) to the entry it\nrefers to and reads <name> there, in the same order: di91z::state::class\nreads class from issue_state:queued. A state holding `:` is that slug; a\nbare one refers through the spec's state lookup field. Hops chain\n(`::state::state::x`, at most 8). A data key hops too, when its value is a\nreference: `ref://<slug>`, or a bare slug on a key the spec types `ref` —\ndi91z::needs::title. A value that refers to nothing, an array, a missing\nentry or a loop exits 1 naming it.\n\nStructured states (objects with `id` and other fields) render as an `{id}`\nchip on the main info line, with sub-fields broken out on a state second\nline below. Parallel states wrap in `[ … ]` with one inner state per line.\nThe per-entry data block follows the same alignment convention — keys are\nright-aligned so their trailing `:` shares a column when the block goes\nmultiline.\n";

const DELETE_BODY: &str = "USAGE:\n  datum delete [ID_OR_SLUG[,ID_OR_SLUG]...] [-c WHY] [--force] [--dry-run] [--all-or-nothing]\n                [-A | -o FMT]\n\nARGS:\n  [ID_OR_SLUG]  QuickId(s) — hid prefix, slug, `_`/`_LAST`, `<field>:_`,\n                slug wildcard. Multiple targets may be comma-batched\n                (`<id>,<id>,...`); resolution is per-token.\n                Omitting the arg targets `_LAST`. Nothing refuses on\n                age: past ten minutes delete warns on stderr with\n                the pointer's age and proceeds (`datum undo` is the\n                way back).\n\nOPTIONS:\n  -c, --comment <WHY>\n                    Say why: appends `deleted: <WHY>` to each row's\n                    comments, in the write that deletes it.\n      --force       Accepted, but there is no longer a prompt to skip.\n      --dry-run     Print what would be deleted and exit; no DB writes.\n      --all-or-nothing\n                    With comma-batched tokens, delete nothing if ANY\n                    token failed to resolve (default: delete the\n                    resolved subset, skip-and-warn on the failures).\n  -A, --show-all    Show the full entry document for each deleted row.\n  -o, --format-string <FMT>\n                    Render each deleted row through datum's format-string DSL.\n\nEXAMPLES:\n  datum delete                    # delete _LAST (warns if > 10 min stale)\n  datum delete activity:_         # delete latest activity row\n  datum delete a1b2,c3d4          # batched\n  datum delete _ -c \"wrong time\"  # delete, saying why\n  datum delete a1b2,c3d4 --dry-run\n                                  # preview only — no DB writes\n";

const UNDO_BODY: &str = "USAGE:\n  datum undo [ID_OR_SLUG[,ID_OR_SLUG]...] [--to-version N]\n\nWith no ID_OR_SLUG, undo reverts the last command that changed anything,\nevery entry it wrote at once (db restore, db v2import, kit install, retime\n--field, a comma-list delete or update are each one command), and the next\nundo the command before that: undo walks back, redo walks forward, and a new\ncommand drops whatever was undone, as in an editor. Reads never move it, so\n`get foo; undo` reverts the last write, not foo; `_UNDO` names the entries a\nbare undo reverts next. An entry another copy has changed since (a sync) is\nnot reverted: the undo refuses, names it, and changes nothing. The stack is\nthis copy's own and starts empty on a database from before it.\n\nWith ID_OR_SLUG (`_LAST` among them) it steps that entry back one version of\nits entry_history trail instead; --to-version N picks an explicit version. A\ncomma list steps each entry back along its own trail; --to-version with a\nlist is refused. A `db restore --hard` drops the history undo reads and\ncannot be undone.\n\nUndo never refuses on age. Past ten minutes it names the age on stderr and\nproceeds anyway: `datum redo` is the way forward again.\n";

const REDO_BODY: &str = "USAGE:\n  datum redo [ID_OR_SLUG[,ID_OR_SLUG]...] [--dry-run]\n\nThe inverse of `datum undo`. With no ID_OR_SLUG it re-applies the oldest\ncommand a bare undo took back, every entry of it, and the next redo the one\nafter; a command run since an undo drops what was undone, leaving nothing to\nredo. Redo of an undone create re-creates the entry under the same hid; redo\nof an undone delete deletes it again. With ID_OR_SLUG it re-applies the\nversion that entry's last undo stepped back from; every row of a named\n`_LAST` steps together, as every entry of a comma list does.\n\nExits 1 with 'nothing to redo' when nothing lies ahead, so\n`undo; redo; redo` moves once, not twice. --dry-run moves nothing and\nprints nothing: it only exits as the redo would, 0 or 1.\n";

const HISTORY_BODY: &str = "USAGE:\n  datum history <ID_OR_SLUG[,ID_OR_SLUG]...> [--version N | --diff A B]\n\nA comma list prints each entry's trail in turn, each under its own `hid:` line\n(`--show json`: one array of `{hid, versions}`). `--version` and `--diff` number\nONE entry's versions, so with a list they are refused.\n";

const WHY_BODY: &str = "USAGE:\n  datum why <ID_OR_SLUG[,ID_OR_SLUG]...> [--full]\n\nA comma list answers for each entry in turn, each under its own `hid:` line.\n";

const EDIT_BODY: &str = "USAGE:\n  datum edit <ID_OR_SLUG>\n\nThe editor is $DATUM_EDITOR, else the `editor` key in datumrc.toml, else\n$VISUAL, else $EDITOR, else `vi` — what `datum config show` reports.\nIt is spawned directly, never through a shell, so a value carrying\nflags — `code --wait`, `emacsclient -nw` — still works.\n";

const RETIME_BODY: &str = "USAGE:\n  datum retime [SLUG[,SLUG]...] [SHIFT]\n  datum retime [SLUG[,SLUG]...] [-r] [-t T] [-y]... [-q]... [-d D]\n  datum retime [SLUG[,SLUG]...] --offset DUR [--field F] [--after BND] [--before BND]\n  datum retime [SLUG[,SLUG]...] -T\n\nDEFAULTS:\n  SLUG omitted        → `_LAST` (matches update/delete/undo)\n  No time-args        → set to now\n  SHIFT (bare int)    → minutes-offset shorthand; `retime <id> -5` shifts -5 min\n                        Alone it takes a sign (`retime +30`): an unsigned\n                        number alone is a SLUG, since a short hid can be digits.\n  -r/--relative       → interpret time-args as a delta from now, applied to the\n                        entry's current occur_time (rather than absolute target).\n                        Same flag is `--reverse` on tail; namespaces don't collide.\n  --offset DUR        → explicit ISO duration shift (legacy form)\n  -T/--no-timestamp   → take the time away: the entry becomes a note. A state\n                        stays as a point; a duration or following state goes.\n\nBULK FILTERS (--field/--after/--before select rows instead of _LAST):\n  BND                 → a duration ago (1d, 30m, 1:30), a date (2026-06-01,\n                        apr29, today, yesterday), or any -t time. Both bounds\n                        are inclusive, and a bare date names that day's\n                        opening midnight — so --before 2026-06-01 excludes\n                        June 1.\n";

const SPEC_BODY: &str = "USAGE:\n  datum spec show <FIELD> [--text]\n  datum spec add-state <FIELD> <NAME> [COLOR] [DESCRIPTION] [GLYPH] [--key K]... [--assign K=V]...\n  datum spec set <FIELD> [--from-json JSON] [--verb V] [--state S]... [--tag T]... ...\n  datum spec list [--tag T]\n  datum spec remove <FIELD>\n\n`add-state` merges one state into a field spec without dropping the others\n(unlike `set --state`, which replaces the whole list). `-` in a slot leaves\nit alone, so `datum spec add-state mood meh - \"so-so\"` sets only a\ndescription and `datum spec add-state mood sad - - ▼` only a glyph.\n\nGLYPHS: an occur dot draws a circle unless its field names a glyph — one\ncharacter, drawn in the dot's colour in the dayview and in `now`'s band.\n  --glyph CHAR           the field's glyph (`--glyph ''` clears it)\n  --state-glyph S=CHAR   a state's own, over the field's (`S=` clears it)\n  --glyph-by KEY         pick the glyph by a data key's value...\n  --glyph-map V=CHAR     ...one value at a time (`V=` removes it); a\n                         value the map does not name draws `--glyph`\n\n  datum spec set coffee --glyph ● --glyph-by size \\\n    --glyph-map small=• --glyph-map large=☕\n\ndraws ☕ for `coffee size=large`, • for `size=small`, ● for anything else.\nThe band gives each slice of time one column, so a glyph two columns wide\nthere (most emoji, like ☕) draws as ● in the band and as itself in the\ndayview.\n\nICONS: `--icon FILE` gives the field's dots an .svg of their own in the\ndayview (`--icon ''` clears it). One <svg> element with a viewBox, at most\n4 kB, and nothing that runs, styles or links outside it; `fill=\"currentColor\"`\ntakes the dot's colour. The band is text and cannot draw it, so there the\ndot draws its glyph — ● unless `--glyph` names one.\n\nKEYS: a declared key is a slot of every log of the field, as if an alias\nhad written it after the line's own `-k`/`-K`. `--data-key` declares a\n`-k` one, `--id-key` a `-K` one (its value goes into the slug), in the\norder they bind:\n  K        required        -k K\n  K=       optional        -k K=\n  K=D      default D       -k K=D\nAn assigned key takes no positional: `--assign K=V` writes K=V onto every\nlog, `--id-assign K=V` into the slug too, and `--assign K=` removes one.\nPositionals bind in declared order after the field (after the state, for a\nswitch), the `-K` ones first as on any line:\n`spec set juice --verb occur --data-key cups --data-key temp=cold`\nmakes `datum juice 2` store cups 2, temp cold. A key the line or its\nalias names keeps the line's form. Only an active entry takes them — an occur,\na start, an add, a switch to a named state; never an end, false, null,\nIGNORED or TRACKED. An alias of several sides gives each side its field's\nkeys, and a key two sides declare is one slot.\n\nA state may declare keys of its own, in the same spellings — `--state-key\nS=K` on `set` (the flags naming S give its whole list, an `S=` clearing\nthose before it) or `--key K` on `add-state` — and assign its own with\n`--state-assign S=K=V` on `set` or `--assign K=V` on `add-state`.\nThey join the field's when an entry is in that state, overriding a field\nkey of the same name:\n`spec set mood --verb switch --state happy --state sad --state-key sad=reason`\nmakes `datum sad because` store reason because, and a bare `datum sad`\nrefuse the missing key; `datum happy` does not ask for it.\n\nSIZE: the dayview draws a dot at a size it picks from the day's width.\n  --glyph-scale N        N times that size (`1` clears it)\n  --glyph-aspect N       N times as wide as it is tall (`1` clears it)\nA dot's alone: a block's size is its lane, and the band draws every dot one\ncolumn wide.\n\nNEVER OFF: `--never-false` says a switch field is always in one of its\nstates, as an environment is: datum.day's ring then leaves a gap where its\n`end` would stand. `--allow-false` clears it. A field whose `--state` list\nnames `false` keeps its `end`. With `--only-listed-states` as well, an\n`end` or a `switch FIELD false` is refused.\n\nONLY WHAT IS LISTED: each is off until set, and refused at log time.\n  --only-listed-keys     a data key the spec does not declare is refused\n                         (`datum check` flags old ones); `--any-keys` clears it.\n                         The spec's `--remainder` key counts as declared\n  --only-listed-states   a state `--state` does not list is refused (off\n                         stays allowed unless `--never-false`);\n                         `--any-states` clears it\nRequired keys are required either way.\n\nLEFTOVER WORDS: words no slot takes are an error, unless the line says\nwhere they go (`-R KEY`) or the spec does:\n  --remainder KEY        send them to KEY, as `-R KEY` would — e.g.\n                         `--remainder extra_data` (`''` clears it)\n\nREFERENCES: a state holding `:` is a full slug `field:id` on any field.\n  --state-lookup-field F\n                         a bare state names the entry `F:<state>`, so\n                         `spec set issue --state-lookup-field issue_state`\n                         makes `switch issue queued` refer to\n                         `issue_state:queued` (`''` clears it). One field\n                         name, never a template.\nA data value `ref://<slug>` refers to that entry, on any key; a literal is\n`'\"ref://…\"'` or `str://ref://…`, stored `str://ref://…` and shown without\nthe `str://`. On a key whose subfield `value_type` is `ref` or `ref[]` (`--from-json`,\nor a kit's field spec) a bare slug refers too: `needs=issue:k7f2a`.\n`datum check refs` lists the references that name no live entry.\n\nTYPES: a subfield's `value_type` parses its value at log time and on `update` or `merge`:\n  number    a point for the decimal, `_` between digits (`1_000.5`); a\n            comma makes a list, so `3,5` is refused (that is `number[]`)\n  bool      `true` or `false`\n  date      any day `-d` takes, stored `2026-10-07`\n  datetime  any `-t` form, stored `2026-10-07T09:00:00`\n  string    kept as typed: `007` and `1e5` are not numbers\n  number[], string[], bool[], ref[]\n            a list, each element parsed as its type or the value\n            refused: a glued comma always splits (`8,8,6`), one value\n            is a one-item list (`8` → [8]), and `[8, 6]` or `[8,]` works\n            too; a bracket value that is not JSON (`[a,b]`) is refused\n  array     a list of anything\nA key named `…_time`, `…_date`, `…_dur`/`…_duration` or `…_num`/`…_number`\n(`dueDate` too, not `duedate`) is typed by its name. A value that does not\nparse is refused and nothing is written. One another device wrote is\nmerged as it is, and `datum check` lists it, as it lists an old value\nthat a type declared later doesn't fit (`reps: 8` under `number[]`).\n`datum update <hid> reps=8` writes that value again, typed.\n";

const ALIAS_LONG_DESCRIPTION: &str = "Aliases expand argv before the parser sees it, so an alias can stand in for any prefix of a `datum` invocation — verb, field, flags, or all three.\n\nA standalone, whitespace-bounded `,` token splits one invocation into N sides that write atomically with the same occur_time. The trigger works at the top-level CLI, inside alias bodies, and through chains of alias-of-alias (multi-of-multi flattens to N flat sides). The comma must be its own token: `vertical , sit` triggers a multi, while `vertical,sit`, `vertical ,sit`, and `vertical, sit` do not. Values with embedded commas (`key=a,b`) pass through unchanged.\n\nKey slot forms (`-k` and `-K` accept the same three shapes): `-k key` is required and errors at call time if unfilled, `-k key=` is optional with no default (silently skipped), `-k key=val` defaults to `val` when no value is supplied. A `-k` or `-K` name declared on multiple sides becomes a shared slot — one call-site value propagates to every declaring side. The `.` token skips a slot on its side; for a shared slot it skips on all sides.\n\n`--` semantics in multi-entry contexts: inside an alias body, flags before `--` scope to their side and flags after `--` apply to all sides. `--` is implicit at the end of every alias body, so call-site flags apply to all sides by default. A `--` inside an alias body does not propagate past the alias boundary.\n\nTime flags at the call site compose with the ones the alias bound: `-y`, `-q` and `-d` shift whatever time the alias bound, and a relative `-t` adds to it — on an alias bound to `-t +5`, `-t +10` writes at +15. In a multi-entry alias every side writes at ONE occur_time, taken from the first side, so a later side's `-t` is dropped.\n\nAfter a multi-entry write, `_LAST` refers to every entry written by the most recent invocation, so `rekey _LAST oldkey=newkey` affects every row of the atomic group. A subsequent single-entry write resets `_LAST` to that one row.";

const ALIAS_BODY: &str = "USAGE:\n  datum alias show <NAME>\n  datum alias set <NAME> <EXPANSION>\n  datum alias list\n  datum alias remove <NAME>\n\nATOMIC MULTI-ENTRY ALIASES:\n  A standalone, whitespace-bounded `,` token splits one invocation\n  into N sides that all write atomically with the same occur_time.\n  Works at the top-level CLI, inside alias bodies, and through chains\n  of alias-of-alias (multi-of-multi flattens to N flat sides).\n\n  The comma must be its own token. `vertical , sit` triggers a multi;\n  `vertical,sit`, `vertical ,sit`, and `vertical, sit` do not. Values\n  with embedded commas (`key=a,b`) pass through unchanged.\n\n  Examples:\n    datum switch body vertical , switch body sit\n        Two same-field switches at one occur_time merge into one\n        body row with state=Parallel([vertical, sit]).\n    datum alias set oov \\\n        \"switch environment outside , switch body vertical\"\n        Define a multi-entry alias. `datum oov` writes both rows.\n    datum alias set caf5 \"occur caffeine -t +5\"\n        A one-sided alias binding a time.\n\nCALL-SITE TIME FLAGS:\n  Compose with the alias-bound ones: -y / -q / -d shift whatever\n  time the alias bound, and a relative -t adds to it. An absolute\n  -t replaces the bound clock and keeps the bound offset, either way\n  round.\n\n    datum caf5            caffeine at now + 5 min (the bound time)\n    datum caf5 -t +10     now + 15\n    datum caf5 -t 10:00   10:05\n    datum caf5 -q         now + 5 then 5 back, so now\n    datum caf5 -y         yesterday, + 5 min\n\n  Every side of a multi writes at ONE occur_time, taken from the\n  FIRST side, so a later side's -t is dropped.\n\nKEY SLOT FORMS (-k / -K):\n  -k key       required (errors at call site if unfilled)\n  -k key=      optional, no default (silently skipped if unfilled)\n  -k key=val   defaulted (auto-filled with `val` when unfilled)\n\n  A `-k` (or `-K`) name declared on multiple sides becomes a shared\n  slot — one call-site value propagates to every declaring side. `.`\n  skips a slot on its side; `.` for a shared slot skips on all sides.\n\nFIELD SLOT FORMS (-F):\n  Same three shapes as `-k`/`-K`: the value is kept as a data key and\n  also appended to the field with `__`, so `issue -F issue_id` logs\n  `issue__<value>`. `update <hid> issue_id=…` re-derives the field.\n    -F key         required (errors if unfilled)\n    -F key=        optional (the field stays `issue` when unfilled)\n    -F key=val     defaulted (`issue__val` when unfilled)\n\n  Several `-F` append in order: `issue -F a -F b x y` logs\n  `issue__x__y`. Shared across sides by name like `-k`: one\n  call-site value propagates to every declaring side.\n\n`--` SEMANTICS IN MULTI-ENTRY CONTEXTS:\n  Inside an alias body, flags before `--` scope to their side; flags\n  after `--` apply to all sides. `--` is implicit at the end of every\n  alias body, so call-site flags apply to all sides by default. A `--`\n  inside an alias body does not propagate past the alias boundary.\n\n`_LAST` AFTER A MULTI-WRITE:\n  `_LAST` refers to every entry written by the most recent invocation,\n  so `rekey _LAST oldkey=newkey` affects every row of an atomic group.\n  A subsequent single-entry write resets `_LAST` to that one row.\n";

const TRACK_BODY: &str = "USAGE:\n  datum track <FIELD> [--verb V] [--color C] [--state S|S=C]... [--tag T]... [--key K[=D]]... [--required-key K[=D]]... [-k K[=D]]... [-K K[=D]]... [K=V]...\n\nOPTIONS:\n  --verb <V>   What a bare press of the field means: occur|start|switch|add.\n               Required for a field with neither a spec nor a log; with a\n               log, the log answers once and the answer is written down.\n  --color <C>  The colour FIELD is drawn in — its chip in `now`, its band\n               in the nowview, its marks in the dayview. `#RRGGBB`, or a\n               legacy ANSI-256 number. Anything else is stored as typed\n               and falls back to the hash of the field's name.\n  --state <S>  A state the field may be switched to. `S=COLOR` paints one\n               instead; the `=` is what tells the two apart.\n  --tag <T>    Repeatable, and each flag ADDS its tag to the field's set.\n               `datum spec set --tag` is the one that REPLACES the set.\n  --key <K[=D]>\n               A data key the field's entries carry: `K` required, `K=`\n               optional, `K=D` defaulting to D (JSON when it parses,\n               text when not) — see `datum help spec`. Repeatable. A key the spec has already keeps its type and\n               its place in the slug.\n  --required-key <K[=D]>\n               The same, marked required in the spec.\n  -k <K[=D]> / -K <K[=D]>\n               As on an alias line: a data / id slot, in the order they\n               bind — `K` required, `K=` optional, `K=D` defaulting to D.\n  K=V          After the field, assigns V to K on every log, as a\n               `K=V` on an alias line does (`K=` removes one):\n                 datum track vacuum --verb occur -k next_date=+4 proj=household\n\nTracking is not what puts a field in `datum now` — every field that has\never been logged is already there (`datum now -A` shows the whole set).\nWhat `track` does is DECLARE one: it writes the field's SPEC — the verb,\nthe colour, the states, the tags, the keys — and one switch row\ninto the log saying the declaration happened.\n\nThe spec is what gives a field its own words. `track mood --verb switch\n--state good` makes `datum mood` mean `switch mood` and `datum good`\nmean `switch mood good`; `datum alias list` prints them. That spec is\nitself an entry, `spec:mood`, so it syncs and backs up with the rows: a\nsecond device that pulls the same history gets the words as well.\n\nThe row is written at now and carries an OFF state — `false` for a\nstart/switch field, the TRACKED sentinel for occur/add — and a field's\nstate is whatever its newest row says. So tracking a field that is\nRUNNING ends it, and `now` shows it off from that moment on: `track`\ndeclares a field, it does not inspect one. That row also un-ignores an\nignored field, as any later log does — see `datum help ignore`.\n";

const IGNORE_BODY: &str = "USAGE:\n  datum ignore <FIELD>\n\nIgnoring is the other half of `datum track`, and it is one row: a switch\nentry carrying the IGNORED sentinel, written at now. A field is ignored\nwhen its NEWEST state-bearing row is that sentinel — ignored-ness is\nread off the log, never off a flag — so the next `occur`/`start`/\n`switch` on the field un-ignores it, as `datum track` does.\n\nWhat that hides is the field's place in the views that ask what exists:\n`datum now` drops it from the list (`now -A` shows it again, dimmed,\nbelow a rule), `datum fields` stops listing it, and `datum info` counts\nit under `ignored` instead of `tracked`. Nothing else moves — the\nentries stay, `tail` / `get` / `grep` still find them, and the field's\nspec is left exactly as it was.\n\n`ignore` takes no time flags; the row lands at now. And it needs no\nfield to ignore: naming one that was never logged INVENTS it, with the\nsentinel as the only row it has.\n";

const GREP_BODY: &str = "USAGE:\n  datum grep <PATTERN>... [-f FIELD] [-A | -o FMT]\n\nARGS:\n  <PATTERN>...    One or more regex patterns; rows must match EVERY one\n                  (AND semantics). Each pattern is matched against the\n                  entry's serialized JSON representation, so slug, value,\n                  extra_data, and metadata are all in scope.\n\nOPTIONS:\n  -f, --field <F>      Restrict the search to entries on field F.\n  -A, --show-all       Show the full entry document (verbose form).\n  -o, --format-string <FMT>\n                       Render each hit through datum's format-string mini-DSL.\n\nEXAMPLES:\n  datum grep coffee                      # any entry mentioning 'coffee'\n  datum grep -f mood happy               # mood rows with state/value matching 'happy'\n  datum grep -A 'kg' caffeine            # caffeine rows whose JSON mentions 'kg'\n";

const CHECK_BODY: &str = "USAGE:\n  datum check [CHECKER]... [--fix]\n  datum check --list\n\nARGS:\n  [CHECKER]...   Run only the named checker(s). With no positionals, the\n                 full default battery runs. Known names:\n                   verbs      Drift between stored verb and derive_verb\n                   aliases    Spec/user alias divergence + state collisions\n                   slugs      Drift between stored slug and re-derived slug\n                   integrity  Pending warnings from the conflict_queue\n                   flags      Pending review markers from `datum flag`\n                   tz         Stored UTC offset vs timezone at that instant\n                   refs       State and data references naming no live entry\n                   stats      A sample of stats' cached days against the entries\n\nOPTIONS:\n  --fix          Rewrite drifted `verb` / `slug` columns to match what\n                 the live deriver would produce; annotate alias state\n                 collisions with the explicit first-saved-wins\n                 resolution; rewrite legacy `content_hash` columns\n                 that disagree with `expected_content_hash`; drop\n                 cached stats days that differ from the entries.\n  --list         Print the set of known checker names and exit.\n\nEXAMPLES:\n  datum check                  # full default battery\n  datum check verbs            # only verb-drift\n  datum check aliases --fix    # only alias divergence, annotate collisions\n  datum check integrity flags  # only the queue + review-marker views\n  datum check slugs --fix      # rewrite drifted slug columns in place\n";

const DAYVIEW_BODY: &str = "USAGE:\n  datum dayview [OPTIONS]\n\nOPTIONS:\n  --start-date <DATE>      Start date (ISO YYYY-MM-DD or relative offset like -3, 0)\n  --end-date <DATE>, -d    End date (default: today)\n  --days <N>, -n           Number of days (default: 1)\n  --width <N>              SVG width in pixels (default: 2000)\n  --height <N>             Total SVG height (auto from day-height + days when unset)\n  --day-height <N>         Per-day band height (default: 100)\n  --time-axis-height <N>   Bottom time-axis strip height (default: 15)\n  --label-width <N>        Date-label column width (default: from the label font).\n                           0 draws no date labels — the bands start at the margin\n  --margin <N>             Outer margin (default: 2)\n  -o, --output-file <PATH> .svg / .png / .html (default: SVG to stdout)\n  -w, --watch              Redraw on a 5-minute poll loop\n  --orientation <O>        horizontal (default): time runs left to right and the\n                           days stack as rows. vertical: time runs top to bottom\n                           and the days stand side by side as columns, the way\n                           most calendars draw a week, and perhaps the better fit\n                           on a phone. Pivoted, the size flags pivot with it:\n                           --width spreads the days, --height is the day's\n                           length, --day-height a column's width, the time axis\n                           runs down the left and the date labels across the top\n  --horizontal, --vertical Shorthand for --orientation\n";

const NOWVIEW_BODY: &str = "USAGE:\n  datum nowview [OPTIONS]\n\nOPTIONS:\n  --context <DUR>          Context before the instant (e.g. 15m, 1h). Default: 0 —\n                           the snapshot: each field's state, no time band\n  --history <DUR>          The same flag, under its older name\n  --labels                 Name each field's state, in a gutter right of the\n                           picture, or below it when vertical (SVG only; off by\n                           default)\n  -t, --time <TIME>        Centre the window on this instant instead of ending it\n  -d, --date <DATE>        at now; the time flags as the verbs take them\n  -q, --quick              (-q = 5 min back, stackable)\n  -y, --yesterday          (a day back, stackable)\n  -z, --timezone <TZ>      The zone -t and -d are read in\n  --width <N>              SVG width in pixels (auto-scales with --context when unset;\n                           300 when vertical)\n  --height <N>             SVG height in pixels (default 300; auto-scales with\n                           --context when vertical)\n  --orientation <O>        horizontal (default), or vertical: time runs down the page\n                           as a calendar's does, and each field is a column. SVG\n                           only; --horizontal / --vertical for short\n  --margin <N>             Outer margin (default 2)\n  --time-axis-height <N>   Time-axis strip height (default 15)\n  -o, --output-file <PATH> .svg / .png / .html, or - for the SVG on stdout\n                           (default: the terminal band)\n  -w, --watch              Redraw on DB change (60s idle refresh)\n  --window <DUR>           The terminal band's older name for --context; unlike it,\n                           refuses a zero rather than reading it as the snapshot\n";

const INFO_BODY: &str = "USAGE:\n  datum info [TARGET] [--all]\n\nARGS:\n  [TARGET]   A registered DB name, or a path — anything containing `/`,\n             starting with `.`, or ending in `.db` is read as a path.\n             Omitted, the active DB is used (`--db`, else `$DATUM_DB`,\n             else the default).\n\nOPTIONS:\n  --all      Every database instead of one: the names in the registry\n             (`<config>/datum/dbs.toml`) plus every `*.db` in the data\n             directory (`$DATUM_HOME`, else `$XDG_DATA_HOME/datum`), one\n             block each, the registry's path winning a name collision.\n\nOne block per database: where the file is, its schema version, live and\ndeleted entry counts, the tracked/ignored tally that `datum fields`\nreports one field at a time, the last write, and the size on disk. The\nschema row reads the version the file holds: `v<N> ✓ (current)`, `⚠\n(older than current v<N>)`, or `✗ (newer than binary (v<N>))`. One\nthat will not open says `✗` and why, and one that is not there says `✗\n(missing)`. None of those fails the command.\n\nA TARGET, and every database `--all` lists, is only READ: `info` does\nnot migrate it, so an older one stays as it is until a command opens it\nwith `--db`. The active database is the exception — every command,\n`info` included, brings it to this binary's schema before it runs.\n`info` also creates a missing parent directory for a TARGET path.\nWhat it never does is create the database file itself, or add an\nunknown name to the registry.\n\n`last write` is the newest live entry's modify time in the host's zone,\nand `(none)` when there is no live entry. A row whose read fails prints\n`QUERY ERROR` (red when colour is on) and says why on stderr, the rows\nthat did read still print, and the command exits non-zero.\n";

const DB_BODY: &str = "USAGE:\n  datum db setup\n  datum db purge [--yes | -y]\n  datum db backup <PATH | -> [--overwrite]\n  datum db backup --to DIR --every DUR [--slots N]\n  datum db backup --list DIR\n  datum db backup --plan --every DUR [--slots N] --span DUR\n  datum db restore <PATH | -> [--merge | --hard] [--force] [--dry-run] [--yes | -y] [--if-unchanged FP]\n  datum db migrate (run | status | edit)\n  datum db v2import [FILE | - | --from FILE] [--dry-run]\n\nSetup applies any pending schema migrations to the active database. Use\n`datum db purge` to drop and recreate the schema from scratch.\n\nPurge drops every table in the file and rebuilds the schema from scratch,\nsync's included: its ops, its chain and its endpoint key go, so the purged\ncopy is a new one, in no chain. Without\n--yes, prompts for the database path as confirmation; aborts on mismatch.\nNon-TTY stdin without --yes is rejected.\n\nBackup dumps live entries to JSONL (default) or tar.gz when the path ends in\n.tar.gz / .tgz, gzipped JSONL when it ends in .gz, or to stdout as JSONL\nwhen the path is `-`.\n\nBackup --to keeps a Tower of Hanoi rotation in DIR: slot 0 is the first\nbackup taken there and is kept forever; each later one lands, by its\n--every period p since slot 0, in slot 1 + trailing_zeros(p), replacing\nwhat that slot held. --slots caps the rotating slots (default: they grow\nwith time). A run with a different --every counts its periods from slot 0\nat the new rhythm, so the slots already held are read at it. Each slot is a .jsonl.gz, and DIR/index.json lists them per\ndb. --list reads the index, newest first; --plan prints which slots a\nrotation holds after --span and how far back each reaches.\n\nRestore makes the active database's live entries exactly a backup file's\n(or JSONL on stdin, for `-`): entries the backup lacks are deleted as ordinary deletions, so they\nsync and `undo` can bring them back. On a TTY it asks before deleting, unless\n--yes. --merge only adds the backup's entries and deletes nothing; an entry\nchanged since the backup keeps its live version and counts as skipped, and\none whose slug or state's instant a different live entry holds is merged\ninto it as a sync merges two: the later-created wins a key both have, the\nclash is in `datum conflicts`, and `undo` unmerges.\n--hard makes the database exactly the backup: every entry, deletion and\nhistory row is dropped, not tombstoned, and the backup's entries written\nas they are, so nothing before the restore can be undone. On a TTY it asks\nfor the database's name, as purge does; --yes skips that.\n--dry-run makes the writes and rolls them back, so it counts what would be\ninserted, updated, merged and deleted.\n--if-unchanged takes the fingerprint `--dry-run --json` printed and replaces\nonly if the live entries are still the ones it counted, else exits 12 with\nnothing changed.\n\nMigrate is the schema-migration runner: `run` advances the DB to the binary's\ntarget version, `status` lists applied/pending steps, `edit` is reserved for\nper-entry data migrations (not yet implemented).\n\nV2import ingests a legacy datum CouchDB backup JSON (`{ docs: [...] }`), or\nthat JSON on stdin for `-`, into the active database. Idempotent —\nre-importing the same backup is a no-op.\n";

const CONFIG_BODY: &str = "USAGE:\n  datum config show\n";

const COMPLETIONS_BODY: &str = "USAGE:\n  datum completions <zsh|bash|fish>\n  datum completions install [--shell zsh|bash|fish]\n  datum completions uninstall [--shell zsh|bash|fish]\n\nTab completion for commands, subcommands, flags, fields, states, keys and\nrecent entries. `scripts/install.sh` installs it with datum.\n\n  install     Write the script where the shell loads it by itself, and say\n              where (default shell: $SHELL). No rc file is edited:\n                fish  ~/.config/fish/completions/datum.fish\n                bash  $XDG_DATA_HOME/bash-completion/completions/datum\n                      (loaded by the bash-completion package)\n                zsh   _datum in the first writable directory on $fpath,\n                      else ~/.local/share/zsh/site-functions, with the one\n                      line to add to ~/.zshrc before compinit\n              Running it again overwrites, and it warns when no datum is\n              on PATH or the one there is not this one. When a new shell\n              already completes datum it says so in one line; else start\n              a new shell after.\n  uninstall   Remove that file.\n  <shell>     Print the script, to source (`source <(datum completions zsh)`)\n              or for a packager to place.\n\nThe script only asks datum for its answers, so it never goes stale:\nupgrading datum changes them without reinstalling. It asks the datum the\nshell would run, an alias of that name included.\n\nTAB still offers only files? Delete zsh's completion cache and start a new\nshell.\n";

const SYNC_BODY: &str = "USAGE:\n  datum sync\n  datum sync status [--json]\n  datum sync pause\n  datum sync resume\n  datum sync chain invite [--reader] [--fields <F,…> | --kit <KIT>] [--qr] [--expires <DUR>] [--admin-key <FILE>]\n  datum sync chain connect [<TICKET>]\n  datum sync chain leave\n  datum sync chain list [--json]\n  datum sync chain rename [<WHO>] <LABEL> [--admin-key <FILE>]\n  datum sync chain revoke <WHO> [--admin-key <FILE>]\n  datum sync chain purge <WHO> [--yes] [--json] [--admin-key <FILE>]\n  datum sync chain role <WHO> <member|reader> [--fields <F,…> | --kit <KIT>] [--admin-key <FILE>]\n  datum sync chain ack <WHO|unlock>\n  datum sync chain rekey\n  datum sync admin list [--json]\n  datum sync admin add <KEYFILE|PASSKEY-JSON> [--label <L>] [--admin-key <FILE>]\n  datum sync admin remove <KEY|LABEL> [--allow-unlock] [--admin-key <FILE>]\n  datum sync daemon install\n  datum sync daemon uninstall\n  datum sync daemon status\n\nCopies of a database sync peer to peer, as members of a sync chain: a\nsigned membership log every copy checks, so ops from anyone else are\nrefused. `datum sync chain invite` starts a chain and invites a device;\n`chain connect <TICKET>` joins it. A chain with no admin is open: any\nmember may add, revoke or rename devices. `admin add` locks it.\n\n  (bare)      Sync with every member once: one line each, sent / got /\n              conflicts / whether the two copies' verify hashes agree;\n              then one line of status. In no chain, how to join one.\n  status      Per member, how this copy stands with it: synced (the last\n              session verified and nothing is waiting), pending (ops to\n              send), error (the last session failed; why), never (no\n              session yet); what is waiting, and since when. An open\n              chain says so, and an unlocked one until acknowledged.\n  pause       Stop syncing this copy: it neither sends nor receives,\n              and says so in `status`. Only this copy is paused.\n  resume      Sync again.\n\nchain — its devices:\n  invite      A one-time invite (default --expires 10m): any member it\n              reaches admits the redeemer, so this copy need not be up.\n              Serves until redeemed, printing `datum sync chain connect\n              <LINK>`, a datum.day join link a browser opens to join too\n              (and a QR code of it with --qr). A copy in no chain\n              starts an open one first, itself its first member.\n              --reader admits a reader: it receives every op, and writes\n              nothing — its copy refuses a write, it invites no one, and\n              it relays nothing. --fields coffee,tea (or --kit <KIT>, that\n              kit's fields) admits a filtered reader: every member sends it\n              only those fields' entries, each with its spec, and the\n              chain's records. `status` and `list` say «filtered: n of m\n              fields», so an empty field there reads as not shared.\n              Not yet a privacy boundary for a line of several commands:\n              `occur tea , note diary …` keeps the whole line in each\n              entry it makes, so a tea reader sees the diary's words\n              (da-6956u).\n  connect     Redeem an invite, its join link or the bare ticket (from\n              stdin if not given): this copy joins the chain, and a first\n              sync runs. A revoked copy rejoins its chain by a fresh\n              invite, as a new device would: what each holds, both will.\n              A purged one's purge goes too: what it made comes back.\n              A copy in another chain is refused, unless it holds only\n              its own data: then it leaves its chain to join.\n  leave       Leave this copy's chain, keeping every entry, its own and\n              those it received. It forgets the chain and its peers and\n              takes a new endpoint key: an invite from it starts a new\n              chain, and a connect joins another.\n  list        The chain's admins and endpoints: each endpoint's label,\n              role, and the client it runs in, cli or web (one per CLI\n              data directory or browser profile, so one machine may be\n              two). --json for a program.\n  rename      Relabel this copy, or with <WHO> (a label or an id prefix)\n              another member.\n  revoke      Remove a member: its ops from then on are refused while it\n              is out. A fresh invite admits it again, and what it made\n              while out with it; `list` says when.\n  purge       Set aside every op a member signed, on every copy: the\n              entries it made go, its edits and deletes of others' are\n              undone. Says what it would take (created, edited, deleted,\n              and when) and changes nothing without --yes. It revokes too,\n              until a fresh invite brings it back, its ops with it.\n  role        Make a member a reader, or a reader a member, from now on.\n              A reader's --fields/--kit set its filter: widened, it is\n              sent the added fields' history; narrowed, its own software\n              deletes what is left out. Without them it is sent all.\n  ack         Acknowledge a new endpoint or admin, or (`unlock`) the\n              chain's unlocking: each is announced once on stderr, and\n              stays in `datum check` until acknowledged.\n  rekey       A new endpoint key for this copy, when two copies of one file\n              are found sharing one. It must join the chain again.\n\nadmin — the keys that lock the chain:\n  list        The chain's admins.\n  add         Add an admin: a key file (one that does not exist is written\n              there) or a datum.day passkey. The first locks the chain.\n  remove      Remove an admin. The last one only with --allow-unlock,\n              which opens the chain again; every copy announces it.\n\ndaemon — syncing in the background:\n  install     A systemd user service (Linux) or launchd agent (macOS)\n              that keeps every chained database syncing, in one process,\n              and honours `pause`. The first `chain invite` or `chain\n              connect` of a copy in no chain installs it and says so;\n              DATUM_SYNC_NO_DAEMON=1 skips that. `scripts/install.sh`\n              restarts it after an upgrade.\n  uninstall   Stop it and remove it.\n  status      Whether it is installed and running, the chained databases\n              it syncs, and whether it stops when you log out (a user\n              service does, unless `loginctl enable-linger`).\n\nIn a locked chain, revoke, purge, role, rename of another\nmember, invite and the admin acts take an admin: without --admin-key they\nprint a datum.day link to approve with the passkey, and read the approval\ncode it shows from stdin.\n\nPer key the op with the later hybrid logical clock wins. Two copies that\nset one key without seeing each other's op file a sync_conflict (see\n`datum check` and `datum conflicts`); so does a delete against a\nconcurrent edit, and the delete stands.\n";

const CONFLICTS_BODY: &str = "USAGE:\n  datum conflicts [list] [--all]\n  datum conflicts resolve <HID> (--take-local | --take-remote | --keep-both)\n\nOPTIONS:\n  -a, --all        Include already-resolved conflicts (default: pending only).\n  --take-local     Restore this device's version of the entry.\n  --take-remote    Accept the other device's version.\n  --keep-both      Leave both rows as they are; record the decision.\n\nSync conflicts are the two things a pull can file when two devices claim\none identity: `lww_overwrite` (a newer remote edit replaced a local one)\nand `identity_collision` (two distinct entries claimed one slug, or one\n(field, occur_time)). The losing version survives only in the conflict\nrow, so someone has to say which side wins — `list` prints both.\n\nEverything else the integrity queue holds — double opens, orphan ends,\noverlaps, repeated states — is reported by `datum check` and resolved by\nfixing the data with `datum edit` / `datum delete`, not from here.\n";

const FLAG_BODY: &str = "USAGE:\n  datum flag <FIELD> [COMMENT...] [-y / -q / -d / -t / -z]\n\nAttaches a 'come back to this' marker to <FIELD>. Stored as a Verb::Point\nentry with `extra_metadata = { flag: true, comment: \"...\" }`. The bare\nform `datum flag <FIELD>` records a marker at now with no comment.\n\nDefault rendering hides flags from `datum tail` and `datum now`; use\n`tail --show-meta` to include them, or `datum check` to surface them as\npending review markers. The shape mirrors `datum meta` (which sets\n`meta: true` instead of `flag: true`).\n";

const META_BODY: &str = "USAGE:\n  datum meta <FIELD> <TEXT>...\n  datum meta show <FIELD>\n\nAttach (or list) a protocol-change note on <FIELD>. Stored as a\nVerb::Point entry with `extra_metadata = { meta: true, comment: \"...\" }`.\n`meta show` lists every note attached to <FIELD> in create_time order.\n\nLike `flag`, meta rows are filtered from default tail / now views\n(use `tail --show-meta` to include them).\n";

const KIT_BODY: &str = "USAGE:\n  datum kit install <PATH|-> [--force] [--yes] [--dry-run]\n  datum kit export <NAME> [PATH]\n  datum kit list\n  datum kit uninstall <NAME>\n  datum kit show <NAME>\n\nA kit is a TOML bundle of field specs, aliases, SQL views, indices, and\nlevel-1/2 commands. Install writes the kit's rows tagged with the kit id,\nkeeping a field's own spec unless --force (the bare one a field's first\nlog wrote is the kit's to replace); uninstall drops every row with that\nid and gives a logged field its bare spec back. Export round-trips the\ninstalled rows and the kit's [kit] table back to TOML (defaults to\nstdout when path is omitted).\nInstall reads the TOML from stdin when PATH is `-`; --dry-run prints\nwhat the kit brings and writes nothing; --yes approves its indices\nwithout asking.\n\nList shows every installed kit, its version and what it installed. Show\nprints a single kit's version, description and installed contents, and\nnames each index that would not build on this device.\n";

const VIEW_BODY: &str = "USAGE:\n  datum view <NAME> [--format FMT] [--watch]\n  datum view --list\n\nARGS:\n  <NAME>           Installed view name (see `datum view --list`).\n\nOPTIONS:\n  -f, --format <FMT>   pretty | tsv | csv | json | auto (default auto).\n  -w, --watch          Re-run the query every 2s; clear-screen between\n                       redraws.\n  --list               Print installed view names and exit.\n\nViews execute under PRAGMA query_only = 1, so a malicious or buggy\nview cannot mutate the DB.\n";

const REPARSE_BODY: &str = "USAGE:\n  datum reparse <HID[,HID]...>... [--fix]\n\nARGS:\n  <HID>...      One or more entries to replay. At least one is required;\n                reparse never walks the entry log implicitly.\n\nOPTIONS:\n  --fix         UPDATE rows where the parser produced a different shape\n                from the persisted columns; bumps `parser_version` and\n                recomputes `content_hash`. Default is dry-run (report only).\n\nReplay is hermetic to the alias state at write time — the entry's\nstored `alias_chain` is rehydrated as the resolver, never the live\naliases (da-bkak).\n";

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
        takes_value: true,
        value_hint: Some("FMT"),
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
pub const GLOBAL_FLAGS_COL: usize = 29;

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

/// `--if-changed` (da-hxmtg): the switch family's one flag of its own.
const IF_CHANGED: FlagSpec = FlagSpec {
    short: None,
    long: "if-changed",
    takes_value: false,
    value_hint: None,
    value_optional: false,
    summary: "switch/start/end: write nothing if already in this state",
    long_description: Some(
        "Compare with the state the field holds at the entry's time (`-t`, or \
         now). If it is the same JSON value, nothing is written: the entry \
         that set it prints as ISLAST and the exit is 0.",
    ),
    left_override: None,
};

/// `start`, `stop` and `switch` keep `--if-changed` on their own pages.
const SWITCH_SHARED_FLAGS: SharedFlags = SharedFlags {
    topic: "logging",
    own: &[IF_CHANGED],
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
pub const TOPICS: &[TopicSpec] = &[
    TopicSpec {
        name: "logging",
        summary: "the options every logging verb takes",
        intro: "These are read by `add`, `note`, `occur`, `start`, `stop` and `switch`.\n\
                Each verb's own page lists only what is particular to it.\n",
        flags: LOG_FLAGS,
        summary_col: 24,
        forms: &[],
        outro: "\n`note` is the exception: it has no -C/--comment-remainder, because it\n\
                always behaves as if -C had been given — every un-keyed word after\n\
                the field joins extra_data.comment.\n\
                \n\
                Every form -t, -d and a duration take: `datum help time`.\n",
    },
    TopicSpec {
        name: "time",
        summary: "every form a time, a date or a duration takes",
        intro: "These say when an entry happened. Every command that takes a time reads\n\
                them the same way, and its own page lists which of them it takes.\n",
        flags: TIME_FLAGS,
        summary_col: 24,
        forms: TIME_FORMS,
        outro: "\nNot taken: slash dates (9/28 is month-first or day-first depending on\n\
                who types it), and words beyond the ones above (\"3 days ago at 10\").\n",
    },
];

/// The examples on `datum help time` (da-6yqu9).
/// `every_time_page_example_parses` runs each through the real parser.
const TIME_FORMS: &[FormGroup] = &[
    FormGroup {
        heading: "TIME OF DAY (today)",
        durations: false,
        forms: &[
            FormSpec {
                example: "-t 15:13",
                meaning: "15:13",
            },
            FormSpec {
                example: "-t 1513",
                meaning: "15:13: three or four digits are H:MM",
            },
            FormSpec {
                example: "-t 9",
                meaning: "09:00: one or two digits are an hour",
            },
            FormSpec {
                example: "-t 15:13:45",
                meaning: "with seconds",
            },
            FormSpec {
                example: "-t 153045",
                meaning: "15:30:45: five or six digits add seconds",
            },
            FormSpec {
                example: "-t 9:15am",
                meaning: "09:15",
            },
            FormSpec {
                example: "-t 9:15a",
                meaning: "09:15",
            },
            FormSpec {
                example: "-t '9:15 am'",
                meaning: "09:15",
            },
            FormSpec {
                example: "-t 5pm",
                meaning: "17:00",
            },
            FormSpec {
                example: "-t now",
                meaning: "this moment",
            },
        ],
    },
    FormGroup {
        heading: "RELATIVE (from now: minutes, unless a unit is given)",
        durations: false,
        forms: &[
            FormSpec {
                example: "-t -8",
                meaning: "8 minutes ago",
            },
            FormSpec {
                example: "-t +5",
                meaning: "5 minutes ahead",
            },
            FormSpec {
                example: "-t -1.5h",
                meaning: "an hour and a half ago",
            },
            FormSpec {
                example: "-t -90s",
                meaning: "units: s, m, h, d",
            },
            FormSpec {
                example: "-t 10:00 -t +5",
                meaning: "10:05: a relative -t stacks, as -q does",
            },
            FormSpec {
                example: "-qqq",
                meaning: "15 minutes ago: each q is 5",
            },
        ],
    },
    FormGroup {
        heading: "DATES (at the current time of day; -t takes them too)",
        durations: false,
        forms: &[
            FormSpec {
                example: "-d -1",
                meaning: "yesterday: -d counts days, -t minutes",
            },
            FormSpec {
                example: "-d +1",
                meaning: "tomorrow",
            },
            FormSpec {
                example: "-d yesterday",
                meaning: "also today, tomorrow",
            },
            FormSpec {
                example: "-yy",
                meaning: "two days ago: each y is a day",
            },
            FormSpec {
                example: "-d sunday",
                meaning: "the most recent Sunday, today included",
            },
            FormSpec {
                example: "-d sun",
                meaning: "the same",
            },
            FormSpec {
                example: "-d 2026-09-28",
                meaning: "",
            },
            FormSpec {
                example: "-d sep28",
                meaning: "the most recent September 28",
            },
            FormSpec {
                example: "-d sep28-2025",
                meaning: "September 28, 2025",
            },
            FormSpec {
                example: "-d sep28@2025",
                meaning: "the same",
            },
            FormSpec {
                example: "-d sep28,2025",
                meaning: "the same",
            },
            FormSpec {
                example: "-t sunday",
                meaning: "any of these but a number, which -t reads as a time",
            },
        ],
    },
    FormGroup {
        heading: "A DATE AND A TIME (-t)",
        durations: false,
        forms: &[
            FormSpec {
                example: "-t '2026-09-28 10:00'",
                meaning: "any date -t takes, a space, any time of day",
            },
            FormSpec {
                example: "-t 'yesterday 10am'",
                meaning: "",
            },
            FormSpec {
                example: "-t 'sep28 22:30'",
                meaning: "",
            },
            FormSpec {
                example: "-t 2026-09-28T10",
                meaning: "a T joins them as well",
            },
            FormSpec {
                example: "-t 2026-09-28T10:00:00Z",
                meaning: "ISO 8601 in UTC",
            },
            FormSpec {
                example: "-t 2026-09-28T10:00+02:00",
                meaning: "ISO 8601 with an offset",
            },
            FormSpec {
                example: "-t 2026-09-28T10:00+0200",
                meaning: "the same",
            },
            FormSpec {
                example: "-d yesterday -t 10:00",
                meaning: "10:00 yesterday: -d and -y move -t's day",
            },
            FormSpec {
                example: "-y -t 9",
                meaning: "09:00 yesterday",
            },
        ],
    },
    FormGroup {
        heading: "MARKS (a suffix on -t or -d)",
        durations: false,
        forms: &[
            FormSpec {
                example: "-t 10:00+-5m",
                meaning: "give or take 5 minutes",
            },
            FormSpec {
                example: "-t 10:00±5m",
                meaning: "the same",
            },
            FormSpec {
                example: "-t 3+",
                meaning: "the next 03:00, not today's if it has passed",
            },
            FormSpec {
                example: "-t 18-",
                meaning: "the last 18:00",
            },
            FormSpec {
                example: "-d sunday+",
                meaning: "the coming Sunday",
            },
        ],
    },
    FormGroup {
        heading: "ZONE (what -t and -d are read in)",
        durations: false,
        forms: &[
            FormSpec {
                example: "-z America/Chicago -t 9",
                meaning: "09:00 in Chicago",
            },
            FormSpec {
                example: "-z UTC",
                meaning: "",
            },
            FormSpec {
                example: "-z +4",
                meaning: "UTC+4, in hours",
            },
            FormSpec {
                example: "-z UTC-7",
                meaning: "",
            },
        ],
    },
    FormGroup {
        heading: "DURATIONS (a trailing argument, or dur=: how long it LASTED, ending at -t)",
        durations: true,
        forms: &[
            FormSpec {
                example: "30",
                meaning: "30 minutes: a bare number is minutes",
            },
            FormSpec {
                example: "30m",
                meaning: "also min, minute(s)",
            },
            FormSpec {
                example: "90s",
                meaning: "also sec, second(s)",
            },
            FormSpec {
                example: "2h",
                meaning: "also hr, hour(s)",
            },
            FormSpec {
                example: "1d",
                meaning: "also day(s)",
            },
            FormSpec {
                example: "1.5h",
                meaning: "an hour and a half",
            },
            FormSpec {
                example: "1h30m",
                meaning: "the same",
            },
            FormSpec {
                example: "1:30",
                meaning: "the same: H:MM",
            },
            FormSpec {
                example: "1:15:30",
                meaning: "H:MM:SS",
            },
            FormSpec {
                example: "PT30M",
                meaning: "ISO 8601",
            },
        ],
    },
];

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
        name: "DATUM_CLIENT_ID",
        summary: "Override the per-installation client id stamped on entries.",
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
    EnvVarSpec {
        name: "DATUM_SHOW",
        summary: "`--show` for a command that has the rendering, kept out of `raw_input` (a typed `--show` wins).",
    },
    EnvVarSpec {
        name: "DATUM_NO_TIPS",
        summary: "Set to `1` to never print the one-time tab-completion tip.",
    },
    EnvVarSpec {
        name: "DATUM_SYNC_NO_DAEMON",
        summary:
            "Set to `1` so a first `sync chain invite`/`connect` does not install the sync daemon.",
    },
];

/// Worked alias examples, each one run against the binary before it was
/// written down (da-ce7n3). They render into the man page AND into the
/// README's generated alias reference, so there is no hand copy to drift.
const ALIAS_EXAMPLES: &[ExampleSpec] = &[
    ExampleSpec {
        command: "datum alias set vbp \"note word srcLang=pt pos=verb\"\ndatum vbp correr \"to run\"",
        description: "An alias stands in for any PREFIX of an invocation — verb, field, flags, data keys. The call above writes a `word` note with `srcLang=pt`, `pos=verb` and `comment=[\"correr to run\"]`.",
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        shared_flags: Some(&SWITCH_SHARED_FLAGS),
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        display_input_name: false,
        subcommands: NO_SUBCOMMANDS,
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
        shared_flags: Some(&SWITCH_SHARED_FLAGS),
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        // The legacy log_help echoed back the verb the user typed, so
        // `datum end --help` showed `datum end — log an entry`. Preserve
        // that by switching to alias-name rendering when looked up via
        // the alias table.
        display_input_name: true,
        subcommands: NO_SUBCOMMANDS,
    },
    CommandSpec {
        name: "switch",
        aliases: &[],
        category: Category::Logging,
        summary: "transition a field to a new state",
        long_description: Some(
            "A `:` inside a state is reserved: the state is then a full slug \
             `field:id` naming another entry, whatever field it is logged on \
             — `datum switch media book:great_gatsby` refers to the entry \
             `book:great_gatsby`. A state without one refers to a document \
             only when the field's spec names a `--state-lookup-field` (see \
             `datum help spec`). Nothing is refused at write, since the \
             document may arrive later by sync; `datum check refs` lists the \
             references that name no live entry.",
        ),
        usage: &["datum switch <FIELD> [VALUE | KEY=VALUE]..."],
        args: NO_ARGS,
        flags: LOG_FLAGS,
        shared_flags: Some(&SWITCH_SHARED_FLAGS),
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        display_input_name: false,
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
    },
    CommandSpec {
        name: "delete",
        aliases: &["del"],
        category: Category::Mutate,
        summary: "delete an entry (alias: del)",
        long_description: None,
        usage: &["datum delete [ID_OR_SLUG[,ID_OR_SLUG]...] [-c WHY] [--force] [--dry-run] [--all-or-nothing]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(DELETE_BODY),
        display_input_name: false,
        subcommands: NO_SUBCOMMANDS,
    },
    CommandSpec {
        name: "undo",
        aliases: &[],
        category: Category::Mutate,
        summary: "undo the last command, or one entry's last change",
        long_description: Some(
            "Bare, reverts the last command that changed anything, all of it, \
             and walks back one command per undo; reads never move it. Named, \
             steps an entry back along its entry_history trail, or to \
             --to-version N. A change synced in from another copy since is \
             refused, never reverted. Undo never refuses on age: past ten \
             minutes it warns and proceeds, and `datum redo` is the way \
             forward again.",
        ),
        usage: &["datum undo [ID_OR_SLUG[,ID_OR_SLUG]...] [--to-version N]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(UNDO_BODY),
        display_input_name: false,
        subcommands: NO_SUBCOMMANDS,
    },
    CommandSpec {
        name: "redo",
        aliases: &[],
        category: Category::Mutate,
        summary: "redo the last undone change (the inverse of undo)",
        long_description: Some(
            "Bare, re-applies the oldest command a bare undo took back, and \
             walks forward one per redo; a command run since drops them. Named, \
             re-applies the version that the last undo on that entry stepped \
             back from. Redo of an undone create re-creates it under the same \
             hid; redo of an undone delete deletes it again. Exits 1 when there \
             is nothing ahead; \
             --dry-run only exits as the redo would, moving nothing.",
        ),
        usage: &["datum redo [ID_OR_SLUG[,ID_OR_SLUG]...] [--dry-run]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(REDO_BODY),
        display_input_name: false,
        subcommands: NO_SUBCOMMANDS,
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
            "datum spec add-state <FIELD> <NAME> [COLOR] [DESCRIPTION] [GLYPH] [--key K]... [--assign K=V]...",
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
        subcommands: SPEC_SUBCOMMANDS,
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
        subcommands: ALIAS_SUBCOMMANDS,
    },
    CommandSpec {
        name: "track",
        aliases: &[],
        category: Category::Logging,
        summary: "track a field in the now view",
        long_description: None,
        usage: &["datum track <FIELD> [--verb V] [--color C] [--state S|S=C]... [--tag T]... [--key K[=D]]... [--required-key K[=D]]... [-k K[=D]]... [-K K[=D]]... [K=V]..."],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(TRACK_BODY),
        display_input_name: false,
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
            "datum db backup --to DIR --every DUR [--slots N]",
            "datum db backup --list DIR",
            "datum db backup --plan --every DUR [--slots N] --span DUR",
            "datum db restore <PATH | -> [--merge | --hard] [--force] [--dry-run] [--yes | -y] [--if-unchanged FP]",
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
        subcommands: DB_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: CONFIG_SUBCOMMANDS,
    },
    CommandSpec {
        name: "completions",
        aliases: &[],
        category: Category::Admin,
        summary: "shell tab completion: print, install or remove the script",
        long_description: None,
        usage: &[
            "datum completions <zsh|bash|fish>",
            "datum completions install [--shell zsh|bash|fish]",
            "datum completions uninstall [--shell zsh|bash|fish]",
        ],
        args: NO_ARGS,
        flags: COMPLETIONS_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(COMPLETIONS_BODY),
        display_input_name: false,
        subcommands: COMPLETIONS_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
            "datum stats <FIELD> --group-by client",
            "datum stats [FIELD] --show all  (the table, then its --json line)",
        ],
        args: NO_ARGS,
        flags: STATS_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        display_input_name: false,
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
    },
    CommandSpec {
        name: "fields",
        aliases: &[],
        category: Category::ViewsSync,
        summary: "list every field: its spec and the states it was logged with",
        long_description: None,
        usage: &["datum fields [--include-ignored | -A] [--sort frecency]"],
        args: NO_ARGS,
        flags: FIELDS_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 25,
        body_override: None,
        display_input_name: false,
        subcommands: NO_SUBCOMMANDS,
    },
    CommandSpec {
        name: "sync",
        aliases: &[],
        category: Category::ViewsSync,
        summary: "sync with the other copies in this database's sync chain",
        long_description: None,
        usage: &[
            "datum sync",
            "datum sync status [--json]",
            "datum sync pause",
            "datum sync resume",
            "datum sync chain invite [--reader] [--fields <F,…> | --kit <KIT>] [--qr] [--expires <DUR>] [--admin-key <FILE>]",
            "datum sync chain connect [<TICKET>]",
            "datum sync chain leave",
            "datum sync chain list [--json]",
            "datum sync chain rename [<WHO>] <LABEL> [--admin-key <FILE>]",
            "datum sync chain revoke <WHO> [--admin-key <FILE>]",
            "datum sync chain purge <WHO> [--yes] [--json] [--admin-key <FILE>]",
            "datum sync chain role <WHO> <member|reader> [--fields <F,…> | --kit <KIT>] [--admin-key <FILE>]",
            "datum sync chain ack <WHO|unlock>",
            "datum sync chain rekey",
            "datum sync admin list [--json]",
            "datum sync admin add <KEYFILE|PASSKEY-JSON> [--label <L>] [--admin-key <FILE>]",
            "datum sync admin remove <KEY|LABEL> [--allow-unlock] [--admin-key <FILE>]",
            "datum sync daemon install",
            "datum sync daemon uninstall",
            "datum sync daemon status",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(SYNC_BODY),
        display_input_name: false,
        subcommands: SYNC_SUBCOMMANDS,
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
        subcommands: CONFLICTS_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: META_SUBCOMMANDS,
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
        subcommands: KIT_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
        subcommands: NO_SUBCOMMANDS,
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
