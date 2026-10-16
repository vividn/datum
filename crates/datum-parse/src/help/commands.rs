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
//! - **Multi-`USAGE` / subcommand trees** (`setup`, `db`, `conflicts`,
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
const REKEY_ARGS: &[ArgSpec] = &[ArgSpec {
    name: "<OLD=NEW>",
    required: true,
    variadic: true,
    summary: "Move OLD's value to the key NEW. The id template is read again, so the slug follows: a template key renamed away empties its part of the id",
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
const ALIAS_SUBCOMMANDS: &[SubcommandSpec] = &[
    sub("show", &["datum alias show <NAME>"]),
    sub("set", &["datum alias set <NAME> <EXPANSION>"]),
    sub("list", &["datum alias list"]),
    sub("remove", &["datum alias remove <NAME>"]),
];
const DB_SUBCOMMANDS: &[SubcommandSpec] = &[
    sub(
        "purge",
        &[
            "datum db purge [--leave [--force] | --rejoin] [--yes | -y]",
            "datum db purge --chain [--yes | -y] [--admin-key <FILE>]",
        ],
    ),
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
        &["datum db restore <PATH | -> [--merge | --hard [--leave]] [--force] [--dry-run] [--yes | -y] [--if-unchanged FP]"],
    ),
    sub("migrate", &["datum db migrate [run | status | edit]"]),
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
    sub("list", &["datum conflicts [list] [--json]"]),
    sub("resolve", &["datum conflicts resolve <HID> [--use-a | --use-b | --prefer-a | --prefer-b | --merge] [KEY=a|b|VALUE…]"]),
];
const SYNC_SUBCOMMANDS: &[SubcommandSpec] = &[
    sub("status", &["datum sync status [--show-revoked] [--json]"]),
    sub("pause", &["datum sync pause"]),
    sub("resume", &["datum sync resume"]),
    sub("diff", &["datum sync diff <WHO> [--json]"]),
    sub("refold", &["datum sync refold [--dry-run] [--json]"]),
    sub(
        "chain",
        &[
            "datum sync chain invite [--reader] [--fields <F,…> | --kit <KIT>] [--qr] [--expires <DUR>] [--label <L>] [--admin-key <FILE>]",
            "datum sync chain connect [<TICKET>] [--label <L>]",
            "datum sync chain leave [--force]",
            "datum sync chain list [--show-revoked] [--json]",
            "datum sync chain rename [<WHO>] (<LABEL> | --unset) [--admin-key <FILE>]",
            "datum sync chain revoke <WHO> [--admin-key <FILE>]",
            "datum sync chain revert <WHO> [--yes] [--json] [--admin-key <FILE>]",
            "datum sync chain purge (--data | --all) [--yes] [--admin-key <FILE>]",
            "datum sync chain role <WHO> <member|reader> [--fields <F,…> | --kit <KIT>] [--admin-key <FILE>]",
            "datum sync chain ack [<WHO|unlock> | --all]",
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
    LAST_STATE,
    ENSURE_LAST_STATE,
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
        summary: "Include spec/layout/alias/view/command/index entries (hidden by default)",
        long_description: None,
        left_override: Some("    --show-meta       (alias: --show-all-metadata)"),
    },
    FlagSpec {
        short: None,
        long: "above",
        takes_value: true,
        value_hint: Some("ID"),
        value_optional: false,
        summary: "Only the entries listed above this one",
        long_description: None,
        left_override: None,
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
        summary: "update|merge|rekey, or a combine: useOld|useNew|preferOld|preferNew|append",
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

const DELETE_BODY: &str = "USAGE:\n  datum delete [ID_OR_SLUG[,ID_OR_SLUG]...] [-c WHY] [--dry-run] [--all-or-nothing]\n                [-A | -o FMT]\n  datum delete --all [--keep-meta] [-c WHY] [--dry-run]\n\nARGS:\n  [ID_OR_SLUG]  QuickId(s) — hid prefix, slug, `_`/`_LAST`, `<field>:_`,\n                slug wildcard. Multiple targets may be comma-batched\n                (`<id>,<id>,...`); resolution is per-token.\n                Omitting the arg targets `_LAST`. Nothing refuses on\n                age: past ten minutes delete warns on stderr with\n                the pointer's age and proceeds (`datum undo` is the\n                way back).\n\nOPTIONS:\n  -c, --comment <WHY>\n                    Say why: appends `deleted: <WHY>` to each row's\n                    comments, in the write that deletes it.\n      --dry-run     Print what would be deleted and exit; no DB writes.\n      --all-or-nothing\n                    With comma-batched tokens, delete nothing if ANY\n                    token failed to resolve (default: delete the\n                    resolved subset, skip-and-warn on the failures).\n      --all         Delete every entry, specs, aliases, views, commands,\n                    layouts and flags included, as one command: one\n                    bare `datum undo` brings them all back.\n      --keep-meta   With --all, keep what `tail --show-meta` adds:\n                    specs, layouts, aliases, views, commands, indexes,\n                    kits, flags and `datum meta` notes.\n  -A, --show-all    Show the full entry document for each deleted row.\n  -o, --format-string <FMT>\n                    Render each deleted row through datum's format-string DSL.\n                    Neither with --all, which prints one count line.\n\nEXAMPLES:\n  datum delete                    # delete _LAST (warns if > 10 min stale)\n  datum delete activity:_         # delete latest activity row\n  datum delete a1b2,c3d4          # batched\n  datum delete _ -c \"wrong time\"  # delete, saying why\n  datum delete a1b2,c3d4 --dry-run\n                                  # preview only — no DB writes\n  datum delete --all --keep-meta   # every data entry; the setup stays\n";

const UNDO_BODY: &str = "USAGE:\n  datum undo [ID_OR_SLUG[,ID_OR_SLUG]...] [--to-version N]\n  datum undo --skip-changed\n\nWith no ID_OR_SLUG, undo reverts the last command that changed anything,\nevery entry it wrote at once (db restore, db v2import, kit install, retime\n--field, a comma-list delete or update are each one command), and the next\nundo the command before that: undo walks back, redo walks forward, and a new\ncommand drops whatever was undone, as in an editor. Reads never move it, so\n`get foo; undo` reverts the last write, not foo; `_UNDO` names the entries a\nbare undo reverts next. An entry another copy has changed since (a sync) is\nnot reverted: the undo refuses, names every such entry and what to do about\neach, and changes nothing. `--skip-changed` takes back the rest of the command\nand leaves those entries as the other copy left them, naming each; its redo\nleaves them too. The stack is this copy's own and starts empty on a database\nfrom before it.\n\nWith ID_OR_SLUG (`_LAST` among them) it steps that entry back one change this\ncopy made to it instead (its own undo journal); --to-version N goes back to\nversion N as `datum history` numbers them. A comma list steps each entry back\nalong its own trail; --to-version with a list is refused. A `db restore --hard` drops the history undo reads and\ncannot be undone.\n\nUndo never refuses on age. Past ten minutes it names the age on stderr and\nproceeds anyway: `datum redo` is the way forward again.\n";

const REDO_BODY: &str = "USAGE:\n  datum redo [ID_OR_SLUG[,ID_OR_SLUG]...] [--dry-run]\n\nThe inverse of `datum undo`. With no ID_OR_SLUG it re-applies the oldest\ncommand a bare undo took back, every entry of it the undo took back (not\nwhat an `undo --skip-changed` left as it was), and the next redo the one\nafter; a command run since an undo drops what was undone, leaving nothing to\nredo. Redo of an undone create re-creates the entry under the same hid; redo\nof an undone delete deletes it again. With ID_OR_SLUG it re-applies the\nversion that entry's last undo stepped back from; every row of a named\n`_LAST` steps together, as every entry of a comma list does.\n\nExits 1 with 'nothing to redo' when nothing lies ahead, so\n`undo; redo; redo` moves once, not twice. --dry-run moves nothing and\nprints nothing: it only exits as the redo would, 0 or 1.\n";

const HISTORY_BODY: &str = "USAGE:\n  datum history <ID_OR_SLUG[,ID_OR_SLUG]...> [--version N | --diff A B]\n\nA comma list prints each entry's trail in turn, each under its own `hid:` line\n(`--show json`: one array of `{hid, versions}`). `--version` and `--diff` number\nONE entry's versions, so with a list they are refused.\n\nThe trail is read from the entry's ops, a version per change, newest first:\nwhen, its kind, the line that wrote it, what it set and unset, and in a sync\nchain the device that made it. Every copy that holds the same ops prints the\nsame trail. Where two copies set a key without seeing each other, the later\nversion reads `fork KEY:` with each side (value, device, time, version), and\nthe edit that saw them all `join KEY:` with the value it took and whose side\nthat was. `--version N` prints the entry as version N left it, `--diff A B`\nwhat differs between two; `undo ID --to-version N` goes back to one. An\nentry no command has changed since before ops has no trail yet.\n";

const WHY_BODY: &str = "USAGE:\n  datum why <ID_OR_SLUG[,ID_OR_SLUG]...> [--full]\n\nA comma list answers for each entry in turn, each under its own `hid:` line.\n";

const EDIT_BODY: &str = "USAGE:\n  datum edit [ID_OR_SLUG]\n\nID_OR_SLUG omitted edits `_LAST`, asking first when it is stale, as `update`\ndoes.\n\nThe editor is $DATUM_EDITOR, else the `editor` key in datumrc.toml, else\n$VISUAL, else $EDITOR, else `vi` — what `datum config show` reports.\nIt is spawned directly, never through a shell, so a value carrying\nflags — `code --wait`, `emacsclient -nw` — still works.\n";

const RETIME_BODY: &str = "USAGE:\n  datum retime [SLUG[,SLUG]...] [T]\n  datum retime [SLUG[,SLUG]...] [-r] [-t T] [-y]... [-q]... [-d D]\n  datum retime [SLUG[,SLUG]...] --offset DUR [--field F] [--after BND] [--before BND]\n  datum retime [SLUG[,SLUG]...] -T\n\nDEFAULTS:\n  SLUG omitted        → `_LAST` (matches update/delete/undo)\n  No time-args        → set to now\n  T, -t T             → any time -t takes (`datum help time`), read from the\n                        entry's OWN time and zone rather than from now:\n                        `retime <id> 1040` is 10:40 on the entry's day, `+5`\n                        five minutes after it, `-1h` an hour before it,\n                        `yesterday` the day before it. An unsigned number is\n                        a clock time, not minutes (`+30` is the shift). In a\n                        comma list each entry is read from its own time.\n                        Alone T takes a sign (`retime +30`): an unsigned\n                        word alone is a SLUG, since a short hid can be digits.\n  -y, -q, -d D, -z    → from the entry likewise: `-y` is the day before it.\n  -r/--relative       → the time-args as a pure shift of the entry's time:\n                        `-r -y` moves it a day back and keeps its clock.\n                        Same flag is `--reverse` on tail; namespaces don't collide.\n  --offset DUR        → explicit ISO duration shift (legacy form)\n  -T/--no-timestamp   → take the time away: the entry becomes a note. A state\n                        stays as a point; a duration or following state goes.\n\nBULK FILTERS (--field/--after/--before select rows instead of _LAST):\n  BND                 → a duration ago (1d, 30m, 1:30), a date (2026-06-01,\n                        apr29, today, yesterday), or any -t time. Both bounds\n                        are inclusive, and a bare date names that day's\n                        opening midnight — so --before 2026-06-01 excludes\n                        June 1.\n";

const SPEC_USAGE: &[&str] = &[
    "datum setup [--tag T]",
    "datum setup <FIELD> [--json]",
    "datum setup <FIELD> [--from-json JSON] [--verb V] [--add-state S]... [--add-tag T]... ... [--track | --ignore]",
    "datum setup <FIELD> --delete",
];
const SPEC_BODY: &str = "USAGE:\n  datum setup [--tag T]\n  datum setup <FIELD> [--json]\n  datum setup <FIELD> [--from-json JSON] [--verb V] [--add-state S]... [--add-tag T]... ... [--track | --ignore]\n  datum setup <FIELD> --delete\n\n`datum setup` lists the fields that have a spec (`--tag T`, those tagged T).\n`datum setup mood` prints mood's spec, `--json` as JSON. `datum setup mood`\nwith flags changes it and prints the whole spec again, each line it changed\nmarked `+`, `-` or `~`; a change that changes nothing writes nothing and says\n`no change`. A field with no spec yet needs `--verb`, unless it has a log to\nread it off. `datum setup mood --delete` deletes the spec document.\n\nSTATES AND TAGS: each list takes three operations.\n  --add-state S          append S if it is not listed\n  --remove-state S       drop S, with its color, glyph, description, keys\n                         and assigns\n  --states a,b,c         exactly these, in this order: a state kept keeps\n                         its own, one dropped loses it (`''` clears)\n  --add-tag T, --remove-tag T, --tags a,b\n                         the same, for the tags\n  --state-description S=TEXT\n                         what a state means (`S=` clears it)\nRemoving what is not there writes nothing and says so. The names in\n`--states` and `--tags` are split on commas, so none can hold one:\n\n  datum setup mood --add-state meh --state-color meh=yellow\n\nCOLOR: a field draws in a color hashed from its name, a state in one\nhashed from its own, until the spec names one.\n  --color C              the field's color: six hex digits (`#` optional),\n                         an ANSI-256 number or a hue name (red, orange,\n                         yellow, lime, green, mint, cyan, azure, blue,\n                         violet, magenta, pink), as a kit's `color`; any\n                         other C is refused. `--color ''` clears it\n  --state-color S=C      a state's own (`S=` clears it)\n\n  datum setup mood --color '#b87333'\n\nGLYPHS: an occur dot draws a circle unless its field names a glyph — one\ncharacter, drawn in the dot's color in the dayview and in `now`'s band.\n  --glyph CHAR           the field's glyph (`--glyph ''` clears it)\n  --state-glyph S=CHAR   a state's own, over the field's (`S=` clears it)\n  --glyph-by KEY         pick the glyph by a data key's value...\n  --glyph-map V=CHAR     ...one value at a time (`V=` removes it); a\n                         value the map does not name draws `--glyph`\n\n  datum setup coffee --glyph ● --glyph-by size \\\n    --glyph-map small=• --glyph-map large=☕\n\ndraws ☕ for `coffee size=large`, • for `size=small`, ● for anything else.\nThe band gives each slice of time one column, so a glyph two columns wide\nthere (most emoji, like ☕) draws as ● in the band and as itself in the\ndayview.\n\nICONS: `--icon FILE` gives the field's dots an .svg of their own in the\ndayview (`--icon ''` clears it). One <svg> element with a viewBox, at most\n4 kB, and nothing that runs, styles or links outside it; `fill=\"currentColor\"`\ntakes the dot's color. The band is text and cannot draw it, so there the\ndot draws its glyph — ● unless `--glyph` names one.\n\nKEYS: a declared key is a slot of every log of the field, as if an alias\nhad written it after the line's own `-k`/`-K`. `-k`/`--data-key` adds a\n`-k` one, `-K`/`--id-key` a `-K` one (its value goes into the slug), in the\norder they bind; a key already declared takes its new spelling in its place,\nand `--remove-key K` takes one away (a state's own are `--state-key S=`'s):\n  K        required        -k K\n  K=       optional        -k K=\n  K=D      default D       -k K=D\nAn assigned key takes no positional: `--assign K=V` writes K=V onto every\nlog, `--id-assign K=V` into the slug too, and `--assign K=` removes one.\nPositionals bind in declared order after the field (after the state, for a\nswitch), the `-K` ones first as on any line:\n`setup juice --verb occur --data-key cups --data-key temp=cold`\nmakes `datum juice 2` store cups 2, temp cold. A key the line or its\nalias names keeps the line's form. Only an active entry takes them — an occur,\na start, an add, a switch to a named state; never an end, false, null,\nIGNORED or TRACKED. An alias of several sides gives each side its field's\nkeys, and a key two sides declare is one slot.\n\nA state may declare keys of its own, in the same spellings — `--state-key\nS=K` (the flags naming S give its whole list, an `S=` clearing those\nbefore it) — and assign its own with `--state-assign S=K=V`.\nThey join the field's when an entry is in that state, overriding a field\nkey of the same name:\n`setup mood --verb switch --states happy,sad --state-key sad=reason`\nmakes `datum sad because` store reason because, and a bare `datum sad`\nrefuse the missing key; `datum happy` does not ask for it.\n\nSIZE: a dot is as tall as its lane, in the dayview and the nowview alike.\n  --glyph-scale N        N times that tall (`1` clears it)\n  --glyph-aspect N       N times as wide as it is tall (`1` clears it)\nA dot's alone: a block's size is its lane, and the band draws every dot one\ncolumn wide.\n\nNEVER OFF: `--never-false` says a switch field is always in one of its\nstates, as an environment is: datum.day's ring then leaves a gap where its\n`end` would stand, and it gets no `FIELDe` alias for one. `--allow-false`\nclears it. A field whose states name `false` keeps its `end`.\nWith `--only-listed-states` as well, an `end` or a `switch FIELD false` is\nrefused.\n\nONLY WHAT IS LISTED: each is off until set, and refused at log time.\n  --only-listed-keys     a data key the spec does not declare is refused\n                         (`datum check` flags old ones); `--any-keys` clears it.\n                         The spec's `--remainder` key counts as declared\n  --only-listed-states   a state the spec does not list is refused (off\n                         stays allowed unless `--never-false`);\n                         `--any-states` clears it\nRequired keys are required either way.\n\nLEFTOVER WORDS: words no slot takes are an error, unless the line says\nwhere they go (`-R KEY`) or the spec does:\n  -R/--remainder KEY     send them to KEY, as `-R KEY` would — e.g.\n                         `--remainder extra_data` (`''` clears it)\n  -S                     with `-R KEY`, joined into one string\n  -C                     kept as one comment instead\nEither names the whole route, replacing the spec's.\n\nREFERENCES: a state holding `:` is a full slug `field:id` on any field.\n  --state-lookup-field F\n                         a bare state names the entry `F:<state>`, so\n                         `setup issue --state-lookup-field issue_state`\n                         makes `switch issue queued` refer to\n                         `issue_state:queued` (`''` clears it). One field\n                         name, never a template.\nA data value `ref://<slug>` refers to that entry, on any key; a literal is\n`'\"ref://…\"'` or `str://ref://…`, stored `str://ref://…` and shown without\nthe `str://`. On a key whose subfield `value_type` is `ref` or `ref[]` (`--from-json`,\nor a kit's field spec) a bare slug refers too: `needs=issue:k7f2a`.\n`datum check refs` lists the references that name no live entry.\n\nTRACKED OR IGNORED: `--track` writes the field's TRACK row with its spec,\nas `datum track` does for a field IGNORED or with no entries, and `--ignore`\nan IGNORED row for a field not ignored already. One command, so one `undo`\ntakes back the spec and the row together:\n  datum setup coffee --from-json '{\"verb\":\"occur\",\"glyph\":\"☕\"}' --track\n\nTYPES: a subfield's `value_type` parses its value at log time and on `update` or `merge`:\n  number    a point for the decimal, `_` between digits (`1_000.5`); a\n            comma makes a list, so `3,5` is refused (that is `number[]`)\n  bool      `true` or `false`\n  date      any day `-d` takes, stored `2026-10-07`\n  datetime  any `-t` form, stored `2026-10-07T09:00:00`\n  string    kept as typed: `007` and `1e5` are not numbers\n  number[], string[], bool[], ref[]\n            a list, each element parsed as its type or the value\n            refused: a glued comma always splits (`8,8,6`), one value\n            is a one-item list (`8` → [8]), and `[8, 6]` or `[8,]` works\n            too; a bracket value that is not JSON (`[a,b]`) is refused\n  array     a list of anything\nA key named `…_time`, `…_date`, `…_dur`/`…_duration` or `…_num`/`…_number`\n(`dueDate` too, not `duedate`) is typed by its name; a `…_time` value is an\ninstant, any `-t` form read from the entry's time, stored as v2 stored it:\n`{utc, o, tz}`. A value that does not parse is refused and nothing is written.\nOne another device wrote is merged as it is, and `datum check` lists it, as it\nlists an old value that a type declared later doesn't fit (`reps: 8` under\n`number[]`). `datum update <hid> reps=8` writes that value again, typed. A\n`…_time` value logged before it was an instant, a clock time without its day,\nis listed too: `datum reparse <hid> --fix` reads the day again from the\nentry's line.\n";

const ALIAS_LONG_DESCRIPTION: &str = "Aliases expand argv before the parser sees it, so an alias can stand in for any prefix of a `datum` invocation — verb, field, flags, or all three.\n\nA standalone, whitespace-bounded `,` token splits one invocation into N sides that write atomically with the same occur_time. The trigger works at the top-level CLI, inside alias bodies, and through chains of alias-of-alias (multi-of-multi flattens to N flat sides). The comma must be its own token: `vertical , sit` triggers a multi. Inside a word it joins a list instead — `vertical,sit` is one parallel state, `key=a,b` one list value — and a comma touching one side only (`vertical, sit`) is refused.\n\nKey slot forms (`-k` and `-K` accept the same three shapes): `-k key` is required and errors at call time if unfilled, `-k key=` is optional with no default (silently skipped), `-k key=val` defaults to `val` when no value is supplied. A `-k` or `-K` name declared on multiple sides becomes a shared slot — one call-site value propagates to every declaring side. The `.` token skips a slot on its side; for a shared slot it skips on all sides.\n\n`--` semantics in multi-entry contexts: inside an alias body, flags before `--` scope to their side and flags after `--` apply to all sides. `--` is implicit at the end of every alias body, so call-site flags apply to all sides by default. A `--` inside an alias body does not propagate past the alias boundary.\n\nTime flags at the call site compose with the ones the alias bound: `-q` and a relative `-t` add to whatever time the alias bound — on an alias bound to `-t +5`, `-t +10` writes at +15 — and `-y` and `-d` move the day of a clock it bound; beside a relative time they need one (`datum help time`). In a multi-entry alias every side writes at ONE occur_time, taken from the first side, so a later side's `-t` is dropped.\n\nAfter a multi-entry write, `_LAST` refers to every entry written by the most recent invocation, so `rekey _LAST oldkey=newkey` affects every row of the atomic group. A subsequent single-entry write resets `_LAST` to that one row.";

const ALIAS_BODY: &str = "USAGE:\n  datum alias show <NAME>\n  datum alias set <NAME> <EXPANSION>\n  datum alias list\n  datum alias remove <NAME>\n\nATOMIC MULTI-ENTRY ALIASES:\n  A standalone, whitespace-bounded `,` token splits one invocation\n  into N sides that all write atomically with the same occur_time.\n  Works at the top-level CLI, inside alias bodies, and through chains\n  of alias-of-alias (multi-of-multi flattens to N flat sides).\n\n  The comma must be its own token: `vertical , sit` triggers a multi.\n  Inside a word it joins a list instead — `vertical,sit` is one\n  parallel state, `key=a,b` one list value — and a comma touching one\n  side only (`vertical, sit`) is refused.\n\n  Examples:\n    datum switch body vertical , switch body sit\n        Two same-field switches at one occur_time merge into one\n        body row with state=Parallel([vertical, sit]).\n    datum alias set oov \\\n        \"switch environment outside , switch body vertical\"\n        Define a multi-entry alias. `datum oov` writes both rows.\n    datum alias set caf5 \"occur caffeine -t +5\"\n        A one-sided alias binding a time.\n\nCALL-SITE TIME FLAGS:\n  Compose with the alias-bound ones: -q and a relative -t add to\n  whatever time the alias bound, and -y / -d move the day of a clock\n  it bound; beside a relative time they need one. An absolute -t\n  replaces the bound clock and keeps the bound offset, either way\n  round.\n\n    datum caf5               caffeine at now + 5 min (the bound time)\n    datum caf5 -t +10        now + 15\n    datum caf5 -t 10:00      10:05\n    datum caf5 -q            now + 5 then 5 back, so now\n    datum caf5 -y -t 10:00   10:05 yesterday\n\n  Every side of a multi writes at ONE occur_time, taken from the\n  FIRST side, so a later side's -t is dropped.\n\nKEY SLOT FORMS (-k / -K):\n  -k key       required (errors at call site if unfilled)\n  -k key=      optional, no default (silently skipped if unfilled)\n  -k key=val   defaulted (auto-filled with `val` when unfilled)\n\n  A `-k` (or `-K`) name declared on multiple sides becomes a shared\n  slot — one call-site value propagates to every declaring side. `.`\n  skips a slot on its side; `.` for a shared slot skips on all sides.\n\nFIELD SLOT FORMS (-F):\n  Same three shapes as `-k`/`-K`: the value is kept as a data key and\n  also appended to the field with `__`, so `issue -F issue_id` logs\n  `issue__<value>`. `update <hid> issue_id=…` re-derives the field.\n    -F key         required (errors if unfilled)\n    -F key=        optional (the field stays `issue` when unfilled)\n    -F key=val     defaulted (`issue__val` when unfilled)\n\n  Several `-F` append in order: `issue -F a -F b x y` logs\n  `issue__x__y`. Shared across sides by name like `-k`: one\n  call-site value propagates to every declaring side.\n\n`--` SEMANTICS IN MULTI-ENTRY CONTEXTS:\n  Inside an alias body, flags before `--` scope to their side; flags\n  after `--` apply to all sides. `--` is implicit at the end of every\n  alias body, so call-site flags apply to all sides by default. A `--`\n  inside an alias body does not propagate past the alias boundary.\n\n`_LAST` AFTER A MULTI-WRITE:\n  `_LAST` refers to every entry written by the most recent invocation,\n  so `rekey _LAST oldkey=newkey` affects every row of an atomic group.\n  A subsequent single-entry write resets `_LAST` to that one row.\n";

const TRACK_BODY: &str = "USAGE:\n  datum track <FIELD> [SETUP CHANGES]\n\n`track F …` is `datum setup F … --track`: it takes every flag `setup`\ntakes (`datum help setup`), and none of its own. The spec's changes and\nthe row saying tracking began are one write, and one `undo`.\n\n  datum track vacuum --verb occur -k next_date=+4 --assign proj=household\n\nOn a field that has a spec, `track F` alone changes nothing of it: it\nwrites the row and nothing else. On a field with none, it declares one,\nas `setup F --verb V --track` does; `--verb` may be left out when the\nfield's log answers it, and a new field is asked what a press of it\nmeans.\n\nTracking is not what puts a field in `datum now` — every field that has\never been logged is already there (`datum now -A` shows the whole set).\nWhat the spec does is give a field its own words. `track mood --verb\nswitch --add-state good` makes `datum mood` mean `switch mood` and `datum\ngood` mean `switch mood good`; `datum alias list` prints them. That spec\nis itself an entry, `spec:mood`, so it syncs and backs up with the rows.\n\nThe row is written at now and carries the TRACKED state, whatever the\nfield's verb: tracked, and in no state yet. `tail` draws it in grey, as\nthe marker of when tracking began. It is what un-ignores an ignored\nfield, as any later log does — see `datum help ignore`. A field that\nalready has entries and is not ignored gets no row, so a RUNNING field\nkeeps running, and `track F` alone on it says «no change».\n";

const IGNORE_BODY: &str = "USAGE:\n  datum ignore <FIELD> [SETUP CHANGES]\n\n`ignore F …` is `datum setup F … --ignore`, as `track` is `setup … --track`\n(`datum help track`): `setup`'s flags, one write and one `undo`. `ignore F`\nalone changes no spec: it writes the row, and on a field with no spec it\ndeclares none.\n\nIgnoring is the other half of `datum track`, and it is one row: a switch\nentry carrying the IGNORED sentinel, written at now. A field is ignored\nwhen its NEWEST state-bearing row is that sentinel — ignored-ness is\nread off the log, never off a flag — so the next `occur`/`start`/\n`switch` on the field un-ignores it, as `datum track` does. A field\nignored already gets no second row: «no change».\n\nWhat that hides is the field's place in the views that ask what exists:\n`datum now` drops it from the list (`now -A` shows it again, dimmed,\nbelow a rule), `datum fields` stops listing it, and `datum info` counts\nit under `ignored` instead of `tracked`. Nothing else moves — the\nentries stay, and `tail` / `get` / `grep` still find them.\n\n`ignore` takes no time flags; the row lands at now. And it needs no\nfield to ignore: naming one that was never logged INVENTS it, with the\nsentinel as the only row it has.\n";

const GREP_BODY: &str = "USAGE:\n  datum grep <PATTERN>... [-f FIELD] [-A | -o FMT]\n\nARGS:\n  <PATTERN>...    One or more regex patterns; rows must match EVERY one\n                  (AND semantics). Each pattern is matched against the\n                  entry's serialized JSON representation, so slug, value,\n                  extra_data, and metadata are all in scope.\n\nOPTIONS:\n  -f, --field <F>      Restrict the search to entries on field F.\n  -A, --show-all       Show the full entry document (verbose form).\n  -o, --format-string <FMT>\n                       Render each hit through datum's format-string mini-DSL.\n\nEXAMPLES:\n  datum grep coffee                      # any entry mentioning 'coffee'\n  datum grep -f mood happy               # mood rows with state/value matching 'happy'\n  datum grep -A 'kg' caffeine            # caffeine rows whose JSON mentions 'kg'\n";

const CHECK_BODY: &str = "USAGE:\n  datum check [CHECKER]... [--fix]\n  datum check --list\n\nARGS:\n  [CHECKER]...   Run only the named checker(s). With no positionals, the\n                 full default battery runs. Known names:\n                   verbs      Drift between stored verb and derive_verb\n                   aliases    Spec/user alias divergence + state collisions\n                   slugs      Drift between stored slug and re-derived slug\n                   integrity  Pending warnings from the conflict_queue\n                   flags      Pending review markers from `datum flag`\n                   tz         Stored UTC offset vs timezone at that instant\n                   refs       State and data references naming no live entry\n                   stats      A sample of stats' cached days against the entries\n                   sync       Sync ops with no row, rows their ops don't fold to, and\n                              what waits on sync: notices, a newer schema, unsent\n                              changes\n\nOPTIONS:\n  --fix          Rewrite drifted `verb` / `slug` columns to match what\n                 the live deriver would produce; annotate alias state\n                 collisions with the explicit first-saved-wins\n                 resolution; rewrite legacy `content_hash` columns\n                 that disagree with `expected_content_hash`; drop\n                 cached stats days that differ from the entries.\n  --list         Print the set of known checker names and exit.\n\nEXAMPLES:\n  datum check                  # full default battery\n  datum check verbs            # only verb-drift\n  datum check aliases --fix    # only alias divergence, annotate collisions\n  datum check integrity flags  # only the queue + review-marker views\n  datum check slugs --fix      # rewrite drifted slug columns in place\n";

const DAYVIEW_BODY: &str = "USAGE:\n  datum dayview [OPTIONS]\n\nOPTIONS:\n  --start-date <DATE>      Start date (ISO YYYY-MM-DD or relative offset like -3, 0)\n  --end-date <DATE>, -d    End date (default: today)\n  --days <N>, -n           Number of days (default: 1)\n  --width <N>              SVG width in pixels (default: 2000)\n  --height <N>             Total SVG height (auto from day-height + days when unset)\n  --day-height <N>         Per-day band height (default: 100)\n  --time-axis-height <N>   Bottom time-axis strip height (default: 15)\n  --label-width <N>        Date-label column width (default: from the label font).\n                           0 draws no date labels — the bands start at the margin\n  --margin <N>             Outer margin (default: 2)\n  -o, --output-file <PATH> .svg / .png / .html (default: SVG to stdout)\n  -w, --watch              Redraw on a 5-minute poll loop\n  --orientation <O>        horizontal (default): time runs left to right and the\n                           days stack as rows. vertical: time runs top to bottom\n                           and the days stand side by side as columns, the way\n                           most calendars draw a week, and perhaps the better fit\n                           on a phone. Pivoted, the size flags pivot with it:\n                           --width spreads the days, --height is the day's\n                           length, --day-height a column's width, the time axis\n                           runs down the left and the date labels across the top\n  --horizontal, --vertical Shorthand for --orientation\n\nA day is drawn on the clock it was lived by: the zone its entries were\nlogged in, whatever zone this machine is in now. Where that clock jumped\nforward (summer time beginning, a flight east) the times it skipped are\nhatched grey. Where it jumped back, the times it repeated fork the day into\na lane for each time they happened, and an entry is drawn in the lane of\nthe offset it was logged at. Each end is labelled with its offset: UTC+2.\n";

const NOWVIEW_BODY: &str = "USAGE:\n  datum nowview [OPTIONS]\n\nOPTIONS:\n  --context <DUR>          Context before the instant (e.g. 15m, 1h). Default: 0 —\n                           the snapshot: each field's state, no time band\n  --history <DUR>          The same flag, under its older name\n  --labels                 Name each field's state, in a gutter right of the\n                           picture, or below it when vertical (SVG only; off by\n                           default)\n  --now-label              Write a small «now» under the now strip instead of the\n                           clock (SVG only)\n  --surround <DUR>         Also draw this much time either side of the window,\n                           outside the SVG's box, for a viewer to pan or zoom\n                           into (before it only when it ends at now; SVG only)\n  -t, --time <TIME>        Centre the window on this instant instead of ending it\n  -d, --date <DATE>        at now; the time flags as the verbs take them\n  -q, --quick              (-q = 5 min back, stackable)\n  -y, --yesterday          (a day back, stackable)\n  -z, --timezone <TZ>      The zone -t and -d are read in\n  --width <N>              SVG width in pixels (auto-scales with --context when unset;\n                           300 when vertical)\n  --height <N>             SVG height in pixels (default 300; auto-scales with\n                           --context when vertical)\n  --orientation <O>        horizontal (default), or vertical: time runs down the page\n                           as a calendar's does, and each field is a column. SVG\n                           only; --horizontal / --vertical for short\n  --margin <N>             Outer margin (default 2)\n  --time-axis-height <N>   Time-axis strip height (default 15)\n  -o, --output-file <PATH> .svg / .png / .html, or - for the SVG on stdout\n                           (default: the terminal band)\n  -w, --watch              Redraw on DB change (60s idle refresh)\n  --window <DUR>           The terminal band's older name for --context; unlike it,\n                           refuses a zero rather than reading it as the snapshot\n";

const INFO_BODY: &str = "USAGE:\n  datum info [TARGET] [--all]\n\nARGS:\n  [TARGET]   A registered DB name, or a path — anything containing `/`,\n             starting with `.`, or ending in `.db` is read as a path.\n             Omitted, the active DB is used (`--db`, else `$DATUM_DB`,\n             else the default).\n\nOPTIONS:\n  --all      Every database instead of one: the names in the registry\n             (`<config>/datum/dbs.toml`) plus every `*.db` in the data\n             directory (`$DATUM_HOME`, else `$XDG_DATA_HOME/datum`), one\n             block each, the registry's path winning a name collision.\n\nOne block per database: where the file is, its schema version, live and\ndeleted entry counts, the tracked/ignored tally that `datum fields`\nreports one field at a time, the last write, and the size on disk. The\nschema row reads the version the file holds: `v<N> ✓ (current)`, `⚠\n(older than current v<N>)`, or `✗ (newer than binary (v<N>))`. One\nthat will not open says `✗` and why, and one that is not there says `✗\n(missing)`. None of those fails the command.\n\nA TARGET, and every database `--all` lists, is only READ: `info` does\nnot migrate it, so an older one stays as it is until a command opens it\nwith `--db`. The active database is the exception — every command,\n`info` included, brings it to this binary's schema before it runs.\n`info` also creates a missing parent directory for a TARGET path.\nWhat it never does is create the database file itself, or add an\nunknown name to the registry.\n\n`last write` is the newest live entry's modify time in the host's zone,\nand `(none)` when there is no live entry. A row whose read fails prints\n`QUERY ERROR` (red when color is on) and says why on stderr, the rows\nthat did read still print, and the command exits non-zero.\n";

const DB_BODY: &str = "USAGE:\n  datum db purge [--leave [--force] | --rejoin] [--yes | -y]\n  datum db purge --chain [--yes | -y] [--admin-key <FILE>]\n  datum db backup <PATH | -> [--overwrite]\n  datum db backup --to DIR --every DUR [--slots N]\n  datum db backup --list DIR\n  datum db backup --plan --every DUR [--slots N] --span DUR\n  datum db restore <PATH | -> [--merge | --hard [--leave]] [--force] [--dry-run] [--yes | -y] [--if-unchanged FP]\n  datum db restore <PATH | -> --hard --chain [--dry-run] [--yes | -y] [--admin-key <FILE> | --approve-link <LINK> --approval-code <CODE>]\n  datum db migrate [run | status | edit]\n  datum db v2import [FILE | - | --from FILE] [--dry-run]\n\nPurge leaves nothing of this copy of the database, as deleting its file\nwould; other copies keep theirs. It drops every table, sync's included: its\nops, its chain and its endpoint key go, so the purged copy is a new one, in\nno chain. A copy in a chain says which it means, or is refused:\n--leave signs its departure and syncs it, as `sync chain leave` does,\nrefusing until a member holds it (--force tells no one); --rejoin keeps its\nendpoint key and where its members were last seen, so it stays a member, and\nrefetches everything from them, its own changes too, writing nothing until\nthey are back. --rejoin is refused while a change made here has reached no\nmember, which the wipe would lose. It rewrites the file, so none of the old\npages are left in it, and removes the pre-migration snapshots beside it,\nnaming each. A backup made with `db backup` is not touched. Without --yes,\nprompts for the database path as confirmation; aborts on mismatch. Non-TTY\nstdin without --yes is rejected.\n\nPurge --chain purges every copy in this copy's sync chain instead,\nand keeps the chain: it is `datum sync chain purge --data` by another name.\nThis copy purges now, after a backup of its entries beside the database,\nand each other member as it next syncs. Without --yes it says when each\nmember last synced, and changes nothing. `datum help sync chain purge`\nsays the rest.\n\nBackup dumps live entries to JSONL (default) or tar.gz when the path ends in\n.tar.gz / .tgz, gzipped JSONL when it ends in .gz, or to stdout as JSONL\nwhen the path is `-`.\n\nBackup --to keeps a Tower of Hanoi rotation in DIR: slot 0 is the first\nbackup taken there and is kept forever; each later one lands, by its\n--every period p since slot 0, in slot 1 + trailing_zeros(p), replacing\nwhat that slot held. --slots caps the rotating slots (default: they grow\nwith time). A run with a different --every counts its periods from slot 0\nat the new rhythm, so the slots already held are read at it. Each slot is a .jsonl.gz, and DIR/index.json lists them per\ndb. --list reads the index, newest first; --plan prints which slots a\nrotation holds after --span and how far back each reaches.\n\nRestore makes the active database's live entries exactly a backup file's\n(or JSONL on stdin, for `-`): entries the backup lacks are deleted as ordinary deletions, so they\nsync and `undo` can bring them back. On a TTY it asks before deleting, unless\n--yes. --merge only adds the backup's entries and deletes nothing; an entry\nchanged since the backup keeps its live version and counts as skipped, and\none whose slug or state's instant a different live entry holds is merged\ninto it as a sync merges two: the later-created wins a key both have (of\ntwo states at one instant the live one is kept), what they give differently\nis a conflict in `datum conflicts`, on every synced copy, and `undo`\nunmerges.\n--hard makes the database exactly the backup: every entry, deletion and\nhistory row is dropped, not tombstoned, and the backup's entries written\nas they are, so nothing before the restore can be undone. Its sync ops go\ntoo, so a copy in a sync chain says which it means. --leave leaves the\nchain as purge --leave does, telling it first (`sync chain leave --force`\nbefore it tells no one); a fresh invite rejoins it. --chain restores EVERY\ncopy in the chain to the backup: it orders a chain purge, as `sync chain\npurge --data` does (an admin act in a locked chain: --admin-key, or the\npasskey's approval of the order `--dry-run --json` names as `approve`,\nwhose link and code --approve-link and --approval-code take), then\nwrites the backup here as new changes, which each other copy takes as it\nnext syncs. An entry any copy made before the restore is dropped when it\nsyncs, an offline copy's too; one made after it is kept. --dry-run names\neach member and when it follows. On a TTY it asks for the database's name,\nas purge does; --yes skips that.\n--dry-run makes the writes and rolls them back, so it counts what would be\ninserted, updated, merged and deleted.\n--if-unchanged takes the fingerprint `--dry-run --json` printed and replaces\nonly if the live entries are still the ones it counted, else exits 12 with\nnothing changed.\n\nMigrate applies any pending schema migrations to the active database, bare\nor as `run`, advancing it to the binary's version; `status` lists applied\nand pending steps, and `edit` is reserved for per-entry data migrations (not\nyet implemented). `datum db purge` drops and recreates the schema instead.\n\nV2import ingests a legacy datum CouchDB backup JSON (`{ docs: [...] }`), or\nthat JSON on stdin for `-`, into the active database. Idempotent —\nre-importing the same backup is a no-op. It lists each doc it skips, with\nwhy and its `_id`, and ignores CouchDB design docs (`_design/…`), which\nare views.\n";

const CONFIG_BODY: &str = "USAGE:\n  datum config show\n";

const COMPLETIONS_BODY: &str = "USAGE:\n  datum completions <zsh|bash|fish>\n  datum completions install [--shell zsh|bash|fish]\n  datum completions uninstall [--shell zsh|bash|fish]\n\nTab completion for commands, subcommands, flags, fields, states, keys and\nrecent entries. `scripts/install.sh` installs it with datum.\n\n  install     Write the script where the shell loads it by itself, and say\n              where (default shell: $SHELL). No rc file is edited:\n                fish  ~/.config/fish/completions/datum.fish\n                bash  $XDG_DATA_HOME/bash-completion/completions/datum\n                      (loaded by the bash-completion package)\n                zsh   _datum in the first writable directory on $fpath,\n                      else ~/.local/share/zsh/site-functions, with the one\n                      line to add to ~/.zshrc before compinit\n              Running it again overwrites. When a new shell already\n              completes datum it says so in one line; else start a new\n              shell after.\n  uninstall   Remove that file.\n  <shell>     Print the script, to source (`source <(datum completions zsh)`)\n              or for a packager to place.\n\nThe script only asks datum for its answers, so it never goes stale:\nupgrading datum changes them without reinstalling. An installed script\nasks the datum that installed it, whatever `datum` names in the shell; a\nprinted one, or one whose datum is gone, asks the datum the shell would\nrun, an alias of that name included. Another name for datum, in zsh:\n`compdef _datum dm`.\n\nTAB still offers only files? Delete zsh's completion cache and start a new\nshell.\n";

const SYNC_BODY: &str = "USAGE:\n  datum sync\n  datum sync status [--show-revoked] [--json]\n  datum sync pause\n  datum sync resume\n  datum sync diff <WHO> [--json]\n  datum sync refold [--dry-run] [--json]\n  datum sync chain invite [--reader] [--fields <F,…> | --kit <KIT>] [--qr] [--expires <DUR>] [--label <L>] [--admin-key <FILE>]\n  datum sync chain connect [<TICKET>] [--label <L>]\n  datum sync chain leave [--force]\n  datum sync chain list [--show-revoked] [--json]\n  datum sync chain rename [<WHO>] (<LABEL> | --unset) [--admin-key <FILE>]\n  datum sync chain revoke <WHO> [--admin-key <FILE>]\n  datum sync chain revert <WHO> [--yes] [--json] [--admin-key <FILE>]\n  datum sync chain purge (--data | --all) [--yes] [--admin-key <FILE>]\n  datum sync chain role <WHO> <member|reader> [--fields <F,…> | --kit <KIT>] [--admin-key <FILE>]\n  datum sync chain ack [<WHO|unlock> | --all]\n  datum sync chain rekey\n  datum sync admin list [--json]\n  datum sync admin add <KEYFILE|PASSKEY-JSON> [--label <L>] [--admin-key <FILE>]\n  datum sync admin remove <KEY|LABEL> [--allow-unlock] [--admin-key <FILE>]\n  datum sync daemon install\n  datum sync daemon uninstall\n  datum sync daemon status\n\nCopies of a database sync peer to peer, as members of a sync chain: a\nsigned membership log every copy checks, so ops from anyone else are\nrefused. `datum sync chain invite` starts a chain and invites a device;\n`chain connect <TICKET>` joins it. A chain with no admin is open: any\nmember may add, revoke or rename devices. `admin add` locks it.\n\n  (bare)      Sync with every member once: one line each, sent / got /\n              conflicts / whether the two copies' verify hashes agree, or\n              that this copy has no address for it yet; then one line of\n              status. In no chain, how to join one.\n  status      Per member, how this copy stands with it: synced (the last\n              session verified and nothing is waiting), pending (ops to\n              send), error (the last session failed; why), never (no\n              session yet); what is waiting, and since when. A member\n              this copy has no address for yet says so: it is reached\n              once a member that knows it passes its address on. An open\n              chain says so, and an unlocked one until acknowledged; so\n              does a copy in a chain with no daemon installed. A revoked\n              or reverted member is hidden and counted: --show-revoked\n              shows it. One that left is not listed here.\n  pause       Stop syncing this copy: it neither sends nor receives,\n              and says so in `status`. Only this copy is paused.\n  resume      Sync again.\n  diff        What this copy and one member (a label or an id prefix) hold\n              differently: each entry, the columns that differ with both\n              values, and the ops each side holds for it. It asks the\n              member, which must be up, and changes nothing on either.\n  refold      Write again from its ops every row that is not what its ops\n              fold to, and say what changed; `datum check sync` lists them\n              first. No op is signed, and a correct row is left as it is.\n              A row a write left without its op holds what the refold\n              drops, and nothing offers it back: --dry-run names each row\n              it would write, every column old → new, and writes nothing.\n\nchain — its devices:\n  invite      A one-time invite (default --expires 10m): any member it\n              reaches admits the redeemer, so this copy need not be up.\n              Serves until redeemed, printing `datum sync chain connect\n              <LINK>`, a datum.day join link a browser opens to join too\n              (and a QR code of it with --qr). A copy in no chain\n              starts an open one first, itself its first member.\n              --reader admits a reader: it receives every op, and writes\n              nothing — its copy refuses a write, it invites no one, and\n              it relays nothing. --fields coffee,tea (or --kit <KIT>, that\n              kit's fields) admits a filtered reader: every member sends it\n              only those fields' entries, each with its spec, and the\n              chain's records. `status` and `list` say «filtered: n of m\n              fields», so an empty field there reads as not shared.\n              Not yet a privacy boundary for a line of several commands:\n              `occur tea , note diary …` keeps the whole line in each\n              entry it makes, so a tea reader sees the diary's words\n              (da-6956u). --label names this copy.\n  connect     Redeem an invite, its join link or the bare ticket (from stdin\n              if not given): this copy joins the chain, and a first sync\n              runs. It joins as --label, or unlabelled (see rename). A\n              revoked copy rejoins its chain by a fresh invite, as a new\n              device would: what each holds, both will. A reverted one's\n              revert goes too: what it made comes back. A copy in another\n              chain is refused, unless it holds only its own data: then it\n              leaves its chain to join.\n  leave       Leave this copy's chain, keeping every entry, its own and\n              those it received. It signs its departure, a reader too, and\n              syncs it: every member learns of it through those it reached,\n              and lists it as left. Reaching none, it refuses and stays,\n              writing nothing, until a sync delivers it; --force leaves\n              telling no one. Then it forgets the chain and its peers and\n              takes a new endpoint key: an invite from it starts a new\n              chain, and a connect joins another.\n  list        The chain's admins and endpoints: each endpoint's label,\n              role, and the client it runs in, cli or web (one per CLI\n              data directory or browser profile, so one machine may be\n              two). A former member — revoked, reverted, or one that left\n              — is hidden and counted: --show-revoked lists it. --json for\n              a program, with every one.\n  rename      Relabel this copy, or with <WHO> (a label or an id prefix)\n              another member. --unset takes the label away: an unlabelled\n              member is shown by the least that tells it apart, the same\n              on every copy: its host; <user>@ before it once the chain\n              holds two users; its database's name after it where another\n              member shares its host; a short id where still alike. A new\n              member can make a label grow.\n  revoke      Remove a member: its ops from then on are refused while it\n              is out. A fresh invite admits it again, and what it made\n              while out with it; `list` says when.\n  revert      Set aside every op a member signed, on every copy: the\n              entries it made go, its edits and deletes of others' are\n              undone. Says what it would take (created, edited, deleted,\n              and when) and changes nothing without --yes. It revokes too,\n              until a fresh invite brings it back, its ops with it.\n  purge       Every copy drops every entry, op and history: this one\n              now, after a backup of its entries beside the database (the\n              only one: a backup on every copy would keep what the purge\n              is for), each other member as it next syncs with a member\n              holding the order, offline for a year or not. --data keeps\n              the chain and its members, and what is written afterwards\n              syncs as ever; --all dissolves it too, so every copy is in\n              no chain. A revoked member is never told. Without --yes it\n              says when each member last synced with this copy, and\n              changes nothing. `datum db purge --chain` is --data by\n              another name; its --leave and --rejoin purge this copy\n              only.\n  role        Make a member a reader, or a reader a member, from now on.\n              A reader's --fields/--kit set its filter: widened, it is\n              sent the added fields' history; narrowed, its own software\n              deletes what is left out. Without them it is sent all.\n  ack         Acknowledge a new endpoint or admin, or (`unlock`) the\n              chain's unlocking: each is announced once on stderr, and\n              stays in `datum check` until acknowledged. Bare, it takes\n              the one notice pending, or lists them to name one; --all\n              acknowledges every notice pending, each on its own line.\n  rekey       A new endpoint key for this copy, when two copies of one file\n              are found sharing one. It must join the chain again.\n\nadmin — the keys that lock the chain:\n  list        The chain's admins.\n  add         Add an admin: a key file (one that does not exist is written\n              there) or a datum.day passkey. The first locks the chain.\n  remove      Remove an admin. The last one only with --allow-unlock,\n              which opens the chain again; every copy announces it.\n\ndaemon — syncing in the background:\n  install     A systemd user service (Linux) or launchd agent (macOS)\n              that keeps every chained database syncing, in one process,\n              and honours `pause`. The first `chain invite` or `chain\n              connect` of a copy in no chain installs it and says so;\n              DATUM_SYNC_NO_DAEMON=1 skips that. A connect installs it\n              once admitted: its first sync cut short, the daemon\n              finishes it. `scripts/install.sh` restarts it after an\n              upgrade.\n  uninstall   Stop it and remove it.\n  status      Whether it is installed and running, the chained databases\n              it syncs, and whether it stops when you log out (a user\n              service does, unless `loginctl enable-linger`).\n\nIn a locked chain, revoke, revert, purge, role, rename of another\nmember, invite and the admin acts take an admin: without --admin-key they\nprint a datum.day link to approve with the passkey, and read the approval\ncode it shows from stdin.\n\nPer key the op with the later hybrid logical clock wins. Two copies that\nset one key without seeing each other's op leave it contested, and so\ndoes a delete against an edit made without seeing it (the delete stands):\nevery copy holding both shows it in `datum conflicts` and `datum check`,\nuntil an edit that saw both sides settles it.\n";

const CONFLICTS_BODY: &str = "USAGE:\n  datum conflicts [list] [--json]\n  datum conflicts resolve <HID> [--use-a | --use-b | --prefer-a | --prefer-b | --merge] [KEY=a|b|VALUE…]\n\nA conflict is a value two copies of the database set without seeing each\nother's: one key of one entry given two values, or an entry deleted on one\ncopy and edited on another. The entry shows one of them (the later clock's;\na delete stands), and every copy that holds both lists the same sides,\nlettered in clock order: A the earlier, B the later, C a third. The letters\nare the same on every copy.\n\n  list        Every entry with conflicts, each contested key's sides: the\n              letter, the device that set it, when, and the value.\n              --json: each entry's conflicts as they are stored.\n  resolve     One edit that saw every side, so it settles them on every\n              copy it reaches. A pick names a key: KEY=a or KEY=b takes\n              that side's value, KEY=VALUE a new one, typed as any data\n              value (so the text «a» is KEY='\"a\"'); `deleted=a|b|true|false`\n              for an entry deleted on one side. The keys not picked keep\n              what the entry shows, so a bare resolve settles and changes\n              no data; or, when one is named, follow a strategy over A\n              and B, as `update --strategy` combines old and new:\n                --use-a      A's value; a key only B has is unset\n                --use-b      B's value; a key only A has is unset\n                --prefer-a   A's value; a key only one has is kept\n                --prefer-b   B's value; a key only one has is kept\n                --merge      both, as one array without duplicates\n              A strategy is over two sides: a key with more takes a pick.\n              `deleted` follows it as any key does: a delete that is A is\n              kept by --use-a or --prefer-a and undone by --use-b or\n              --prefer-b. --merge refuses it, a delete and an edit being\n              no array: pick `deleted=a` or `deleted=b`.\n\nAn ordinary edit of a contested key settles it too. Integrity findings —\ndouble opens, orphan ends, overlaps, repeated states — are reported by\n`datum check` and resolved by fixing the data.\n";

const FLAG_BODY: &str = "USAGE:\n  datum flag <FIELD> [COMMENT...] [-y / -q / -d / -t / -z]\n\nAttaches a 'come back to this' marker to <FIELD>. Stored as a Verb::Point\nentry with the comment as its `state` and `extra_metadata = { flag: true }`.\nThe bare form `datum flag <FIELD>` records a marker at now with no comment.\n\n`datum now` and the dayview leave flags out; `datum tail` lists them as it\nlists any row, and `datum check` surfaces them as pending review markers\nuntil one is marked done with `datum update <hid> done=true`, which keeps it\nin the record, or deleted with `datum del <hid>`; editing its comment does\nnot clear it.\n`datum meta` marks its notes the same way, with `meta: true`.\n";

const META_BODY: &str = "USAGE:\n  datum meta <FIELD> <TEXT>...\n  datum meta show <FIELD>\n\nAttach (or list) a protocol-change note on <FIELD>. Stored as a Verb::Point\nentry with the note as its `state` and `extra_metadata = { meta: true }`.\n`meta show` lists every note attached to <FIELD> in create_time order.\n\nLike `flag`, a meta note is left out of `now` and the dayview, and `tail`\nlists it as it lists any row.\n";

const KIT_BODY: &str = "USAGE:\n  datum kit install <PATH|-> [--force] [--yes] [--dry-run]\n  datum kit export <NAME> [PATH]\n  datum kit list\n  datum kit uninstall <NAME>\n  datum kit show <NAME>\n\nA kit is a TOML bundle of field specs, aliases, SQL views, indices, and\nlevel-1/2 commands. Install writes the kit's rows tagged with the kit id,\nkeeping a field's own spec unless --force (the bare one a field's first\nlog wrote is the kit's to replace); uninstall drops every row with that\nid and gives a logged field its bare spec back. Export round-trips the\ninstalled rows and the kit's [kit] table back to TOML (defaults to\nstdout when path is omitted).\nInstall reads the TOML from stdin when PATH is `-`; --dry-run prints\nwhat the kit brings and writes nothing; --yes approves its indices\nwithout asking.\n\nList shows every installed kit, its version and what it installed. Show\nprints a single kit's version, description and installed contents, and\nnames each index that would not build on this device.\n";

const VIEW_BODY: &str = "USAGE:\n  datum view <NAME> [--format FMT] [--watch]\n  datum view --list\n\nARGS:\n  <NAME>           Installed view name (see `datum view --list`).\n\nOPTIONS:\n  -f, --format <FMT>   pretty | tsv | csv | json | auto (default auto).\n  -w, --watch          Re-run the query every 2s; clear-screen between\n                       redraws.\n  --list               Print installed view names and exit.\n\nViews execute under PRAGMA query_only = 1, so a malicious or buggy\nview cannot mutate the DB.\n";

const REPARSE_BODY: &str = "USAGE:\n  datum reparse <HID[,HID]...>... [--fix]\n\nARGS:\n  <HID>...      One or more entries to replay. At least one is required;\n                reparse never walks the entry log implicitly.\n\nOPTIONS:\n  --fix         UPDATE rows where the parser produced a different shape\n                from the persisted columns; bumps `parser_version` and\n                recomputes `content_hash`. Default is dry-run (report only).\n\nReplay is hermetic to the alias state at write time — the entry's\nstored `alias_chain` is rehydrated as the resolver, never the live\naliases (da-bkak).\n";

// ---------------------------------------------------------------------------
// Global flags shown in `datum --help` under `GLOBAL OPTIONS:`.
// ---------------------------------------------------------------------------

/// `--color`, in GLOBAL OPTIONS and on `datum help color`.
const COLOR_FLAG: FlagSpec = FlagSpec {
    short: None,
    long: "color",
    takes_value: true,
    value_hint: Some("WHEN"),
    value_optional: false,
    summary: "auto|always|never; see `datum help color`",
    long_description: None,
    left_override: Some("--color <WHEN>"),
};

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
    COLOR_FLAG,
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
     Environment variables datum reads: `datum help env`.\n\
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

/// The binders the pointer names on every verb but `note`.
const BINDERS: &str = "-I/-R/-S/-C";

/// Nothing particular: the verb is told apart by its USAGE lines.
const LOG_SHARED_FLAGS: SharedFlags = SharedFlags {
    topic: "logging",
    own: &[],
    binders: BINDERS,
};

/// `note` names no binders: its USAGE already says where its words go.
const NOTE_SHARED_FLAGS: SharedFlags = SharedFlags {
    binders: "",
    ..LOG_SHARED_FLAGS
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

/// `-l` (da-n86g3, lowercase since da-ovnf9): the state being left, a
/// required value as v2's `-L` was.
const LAST_STATE: FlagSpec = FlagSpec {
    short: Some('l'),
    long: "last-state",
    takes_value: true,
    value_hint: Some("V"),
    value_optional: false,
    summary: "The state being left; `datum check` flags a mismatch",
    long_description: Some(
        "Stored as the entry's `user_asserted_last_state` and shown by \
         `tail -A`. On `end` it also closes the field's open block, and \
         fails when there is none.",
    ),
    left_override: None,
};

/// `-L` (da-ovnf9): `-l` as a claim, which refuses unless it holds.
const ENSURE_LAST_STATE: FlagSpec = FlagSpec {
    short: Some('L'),
    long: "ensure-last-state",
    takes_value: true,
    value_hint: Some("V"),
    value_optional: false,
    summary: "As -l, but write nothing unless the field is in V",
    long_description: Some(
        "Refuses, exit non-zero, naming the state found, unless the field \
         holds V just before the entry (at `-t`, or now). The check and the \
         write are one transaction, so of two writers claiming the same \
         state on one database only one lands; never queued while the \
         database is locked. Synced copies are not one database: two \
         claims made on two copies can both land and meet at the next sync. \
         v2's `-L` recorded; this one refuses — record with `-l`.",
    ),
    left_override: None,
};

/// `start`, `stop` and `switch` keep `--if-changed`, `-l` and `-L` on their own pages.
const SWITCH_SHARED_FLAGS: SharedFlags = SharedFlags {
    topic: "logging",
    own: &[IF_CHANGED, LAST_STATE, ENSURE_LAST_STATE],
    binders: BINDERS,
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
    binders: BINDERS,
};

/// `datum help logging`'s opening: the welcome's «Logging an entry» (da-efruz).
const LOGGING_INTRO: &str = "\
An entry is one line: a verb, a field, then the field's state or data.
The verb says what kind of thing happened:

  occur    a moment                  datum occur coffee
  start    something that lasts      datum start work
  stop     ... and its end           datum stop work
  switch   a change of state         datum switch mood happy
  note     a timeless jot            datum note coffee too strong
  add      a record with a key       datum add book -K title Dune

An entry is at now unless -t, -d, -q or -y say otherwise. Words after
the field need a key (`size=large`) unless the field's spec gives them
one; a note takes them as its text.

The options below are read by all six. Each verb's own page lists only
what is particular to it.
";

/// `datum help <NAME>` pages that are not commands.
pub const TOPICS: &[TopicSpec] = &[
    TopicSpec {
        name: "logging",
        summary: "the options every logging verb takes",
        intro: LOGGING_INTRO,
        flags: LOG_FLAGS,
        summary_col: 24,
        forms: &[],
        env: &[],
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
        env: &[],
        outro: "\nNot taken: slash dates (9/28 is month-first or day-first depending on\n\
                who types it), and words beyond the ones above (\"3 days ago at 10\").\n",
    },
    TopicSpec {
        name: "debug",
        summary: "test situations, made in the current database",
        intro: "`datum debug` is off the command list: it writes made-up data, for\n\
                trying out the CLI and datum.day, into a database of its own\n\
                (`datum --db <scratch> debug …`, ?db=<scratch> on datum.day). It\n\
                refuses one holding an entry it did not make — specs, layouts and\n\
                DEBUG's sample=true entries aside — or in a sync chain.\n\
                \n\
                datum debug conflicts [N]  N entries (3 by default) whose state this\n\
                \x20                          copy and a debug device each edited without\n\
                \x20                          seeing the other's edit, and one more deleted\n\
                \x20                          here while the device edited it: N + 1 sync\n\
                \x20                          conflicts, for `datum conflicts`. The device's\n\
                \x20                          ops go in as a sync's do; a copy holding them\n\
                \x20                          never syncs, nor starts or joins a chain.\n\
                datum debug ops <HID>      every op held of an entry: its HLC, endpoint\n\
                \x20                          and seq, kind, the ops it had seen (deps) and\n\
                \x20                          what it changed. Two ops conflict when neither\n\
                \x20                          saw the other.\n",
        flags: &[],
        summary_col: 24,
        forms: &[],
        env: &[],
        outro: "",
    },
    TopicSpec {
        name: "color",
        summary: "when the output is colored",
        intro: "The first of these that says anything decides. In front of the command:\n",
        flags: &[FlagSpec {
            summary: "always or never outranks the environment; auto leaves it to it",
            ..COLOR_FLAG
        }],
        summary_col: 24,
        forms: &[],
        env: &[],
        outro: "\nThen the environment, as other tools read it:\n\
                \n\
                \x20 NO_COLOR=1          any non-empty value: no color\n\
                \x20 CLICOLOR_FORCE=1    color even through a pipe (not 0 or empty)\n\
                \x20 FORCE_COLOR=1       the same (not 0, false or empty)\n\
                \n\
                Then whether stdout is a terminal: color on one, none in a pipe or file.\n",
    },
    TopicSpec {
        name: "env",
        summary: "the environment variables datum reads",
        intro: "What each variable does when it is set.\n",
        flags: &[],
        summary_col: 24,
        forms: &[],
        env: ENV_VARS,
        outro: "",
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
                example: "-t 2026-09-28T10:00Z",
                meaning: "ISO 8601 in UTC, with or without seconds",
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
            FormSpec {
                example: "-y -t 18:30 -t -6",
                meaning: "18:24 yesterday: beside a day, -q or a relative -t needs a clock",
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
        heading: "ZONE (what -t and -d are read in; with no -z, a past time takes the zone\n  the entries either side of it agree on, and says so)",
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
                meaning: "UTC+4: an offset, kept with no zone name",
            },
            FormSpec {
                example: "-z +5:30",
                meaning: "",
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
// README's table is generated from here by `cargo xtask readme`, `datum
// help env` prints it (da-bread), and
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
        name: "NO_COLOR",
        summary: "Any non-empty value turns color off (`--color always` wins).",
    },
    EnvVarSpec {
        name: "CLICOLOR_FORCE",
        summary: "Color even when stdout is not a terminal; `0` or empty does not (`NO_COLOR` wins).",
    },
    EnvVarSpec {
        name: "FORCE_COLOR",
        summary: "The same as `CLICOLOR_FORCE` (Node's name); `0` or `false` does not force.",
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
        description: "Call-site time flags compose with the alias-bound ones: `-q` and a relative `-t` add to whatever time the alias bound, and `-y` / `-d` move the day of a clock it bound.",
    },
];

// ----- Intros (da-efruz) -----
//
// What a newcomer reads first on a command's page, before its usage: what
// the command is for, an example or two, and the one thing people get
// wrong. `datum help time` is the model. Every example here is checked
// against the CLI it describes.

const OCCUR_INTRO: &str = "\
Occur marks a moment: something that happened at one instant and took
no time worth measuring, such as a coffee, a headache or a sneeze.

  datum occur coffee                  a coffee, now
  datum occur coffee size=large -q    five minutes ago, with data

Words after the field need a key: `datum occur coffee 2` is refused,
`cups=2` is not. Something that lasts is `start` and `stop`, or `switch`.
";

const START_INTRO: &str = "\
Start begins something that lasts, such as work, sleep or a run. It stays
open until a `stop` (or `end`) of the same field closes it.

  datum start work                    work, from now on
  datum start work 25m                work for the last 25 minutes

A duration reaches BACK, not forward: `datum start work 25m` is work
that began 25 minutes ago and ended now. With -t it ends at -t, so
`datum start work 25m -t 12:30` is work from 12:05 to 12:30.
";

const STOP_INTRO: &str = "\
Stop, or end, closes what a `start` of the same field opened.

  datum stop work                     work ends now
  datum end work -t 17:30             work ended at 17:30

A duration on stop is the BREAK, not the work: `datum end work 12m -t
14:12` is a break from 14:00 to 14:12.
";

const SWITCH_INTRO: &str = "\
Switch puts a field in a state, and it stays there until the next switch:
a mood, where you are, what you are reading. No stop is needed, because
each switch ends the state before it.

  datum switch mood happy

`datum end mood` turns the field off. With states in its spec (`datum
track mood --verb switch --add-state happy`), `datum happy` is enough.
";

const TAIL_INTRO: &str = "\
Tail shows the latest entries, newest at the bottom: the quickest way to
check what you just logged.

  datum tail                          the last ten entries
  datum tail coffee -n 30             the last thirty coffees
  datum tail -w                       keeps watching as entries arrive

The columns are for reading and change between versions. A script asks
for `--json`.
";

const UNDO_INTRO: &str = "\
Undo takes back the last command that changed anything, every entry it
wrote at once. `datum redo` puts it back. Undo again to go further back,
as in an editor.

  datum undo                          take back the last command
  datum undo k7f2a                    step that one entry back a change

Looking something up does not count: after `datum tail`, undo still
takes back the last write.
";

const TRACK_INTRO: &str = "\
Track declares a field: what a bare `datum FIELD` does, its color and
its states. It is `datum setup F … --track`, and how a field gets words of
its own.

  datum track coffee --verb occur     `datum coffee` logs a coffee
  datum track mood --verb switch --add-state good --add-state bad
                                      `datum good` switches mood to good

A field does not need it to be seen: every field you have logged is in
`datum now` already. Tracking gives a field its shape, not its place.
";

const SPEC_INTRO: &str = "\
Setup writes a field's shape down: its verb, its states and their
colors, glyphs, and the data keys every log of it carries. `datum
track F …` is `setup F … --track`.

  datum setup mood                     mood's spec, every part of it
  datum setup mood --add-tag mind      change a part; it prints what changed

`--add-state S` adds a state, `--remove-state S` takes one away, and
`--states a,b,c` sets the whole list.
";

const ALIAS_INTRO: &str = "\
An alias is a word of your own that expands into a command. It is kept
in the database, so it syncs to your other devices and is in your
backups.

  datum alias set cof5 \"occur coffee -t -5\"
  datum cof5 size=large               a large coffee five minutes ago

Quote the expansion: it is one argument. Whatever follows the alias
goes on after its expansion.
";

const SYNC_INTRO: &str = "\
Sync keeps the copies of a database on your devices alike: a laptop, a
phone, datum.day in a browser. It is peer to peer and end to end; a relay
only passes along what it cannot read.

  datum sync chain invite             on a device you have: an invite
  datum sync chain connect <TICKET>   on the new one: join with it
  datum sync                          sync with every member now

No server holds your data, so two copies sync while both are running.
A computer that has joined keeps syncing in the background on its own
(`datum sync daemon status`).
";

const DB_INTRO: &str = "\
db looks after the database itself: backups, restores, upgrades and
starting over. Your entries live in one file on this device.

  datum db backup ~/datum.jsonl.gz
  datum db restore ~/datum.jsonl.gz --merge

A bare `restore` makes the database EXACTLY the backup, deleting what
the backup lacks (undo brings it back); `--merge` only adds.
";

const STATS_INTRO: &str = "\
Stats says how much: how often a field was logged, how long it ran, and
what share of its time each state took, over a slice of time.

  datum stats                         every field, ranked by time
  datum stats work --week             work this week
  datum stats mood:happy --month      how much of the month was happy

-t is where the slice STARTS, as --since is; -d names one whole day.
";

pub const COMMANDS: &[CommandSpec] = &[
    // ----- LOGGING -----
    CommandSpec {
        name: "add",
        aliases: &[],
        category: Category::Logging,
        summary: "register a keyed timeless record (needs an id)",
        intro: "",
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
        intro: "",
        long_description: Some(
            "Timeless jot identified by *when* — auto-ids on its create_time (slug field:c<create_time>), so many notes per field coexist. \"Record this moment, won't look it up.\" Sibling of `add`, which instead requires an explicit id for a keyed record.\n\nBare words are the note: every un-keyed word after the field joins `extra_data.comment`, in order and single-spaced, so `datum note coffee too strong today` needs no quotes and no `-C` (`note` always behaves as if `-C` were given). `KEY=VALUE` anywhere in the line is still data, a leading `-` is still a flag, and an explicit `-R <bucket>` still names where the words go instead.",
        ),
        usage: &[
            "datum note <FIELD> [WORD | KEY=VALUE]...",
            "    the WORDs join into extra_data.comment — no quotes, no -C",
        ],
        args: NO_ARGS,
        flags: NOTE_FLAGS,
        shared_flags: Some(&NOTE_SHARED_FLAGS),
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
        intro: OCCUR_INTRO,
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
        intro: START_INTRO,
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
        intro: STOP_INTRO,
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
        intro: SWITCH_INTRO,
        long_description: Some(
            "`datum switch light off` (or `end`, `stop`) turns the field off, \
             as `datum end light` does; a state named off is `'\"off\"'` or \
             `str://off`.\n\n\
             A `:` inside a state is reserved: the state is then a full slug \
             `field:id` naming another entry, whatever field it is logged on \
             — `datum switch media book:great_gatsby` refers to the entry \
             `book:great_gatsby`. A state without one refers to a document \
             only when the field's spec names a `--state-lookup-field` (see \
             `datum help setup`). Nothing is refused at write, since the \
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
        intro: TAIL_INTRO,
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
        intro: "",
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
        intro: "",
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
        intro: "",
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
        intro: "",
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
        intro: "",
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
        intro: "",
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
        intro: "",
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
        intro: "",
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
        intro: "",
        long_description: None,
        usage: &["datum edit [ID_OR_SLUG]"],
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
        intro: "",
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
        summary: "merge values into an entry, collecting a key's into a list",
        intro: "",
        long_description: Some(
            "`merge <id> item=pear` merges into the entry's data as datum does: a key only one side holds is kept, an object in both is merged again, and a key both hold with different values collects both into one list, the entry's first, without duplicates. So `item=apple` becomes `[\"apple\",\"pear\"]`, and a second `item=pear` changes nothing. `update --strategy preferNew` merges with the new value winning instead.",
        ),
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
        summary: "rename an entry's data keys, in place",
        intro: "",
        long_description: Some(
            "`rekey <id> old=new` moves `old`'s value to the key `new` on the same row: nothing is copied and no row is added. The entry's id template is read again afterwards, so its slug follows: a key renamed to one the template names fills that part of the id, and a template's key renamed away empties it.",
        ),
        usage: &["datum rekey <ID_OR_SLUG[,ID_OR_SLUG]...> <OLD=NEW>..."],
        args: REKEY_ARGS,
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
        intro: "",
        long_description: None,
        usage: &[
            "datum retime [SLUG[,SLUG]...] [T]",
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
        intro: "",
        long_description: None,
        usage: &[
            "datum delete [ID_OR_SLUG[,ID_OR_SLUG]...] [-c WHY] [--dry-run] [--all-or-nothing]",
            "datum delete --all [--keep-meta] [-c WHY] [--dry-run]",
        ],
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
        intro: UNDO_INTRO,
        long_description: Some(
            "Bare, reverts the last command that changed anything, all of it, \
             and walks back one command per undo; reads never move it. Named, \
             steps an entry back one change this copy made to it, or to \
             --to-version N of `datum history`. A change synced in from another copy since is \
             refused, never reverted: the refusal names every such entry and what to do \
             about each, and --skip-changed takes back the rest of the command and \
             leaves those as they are. Undo never refuses on age: past ten \
             minutes it warns and proceeds, and `datum redo` is the way \
             forward again.",
        ),
        usage: &[
            "datum undo [ID_OR_SLUG[,ID_OR_SLUG]...] [--to-version N]",
            "datum undo --skip-changed",
        ],
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
        intro: "",
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
        name: crate::parse::SETUP,
        aliases: &["spec"],
        category: Category::Admin,
        summary: "set up a field: its verb, states and look (alias: spec)",
        intro: SPEC_INTRO,
        long_description: None,
        usage: SPEC_USAGE,
        args: NO_ARGS,
        flags: NO_FLAGS,
        shared_flags: None,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(SPEC_BODY),
        display_input_name: false,
        subcommands: NO_SUBCOMMANDS,
    },
    CommandSpec {
        name: "alias",
        aliases: &[],
        category: Category::Admin,
        summary: "manage aliases",
        intro: ALIAS_INTRO,
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
        intro: TRACK_INTRO,
        long_description: None,
        usage: &["datum track <FIELD> [SETUP CHANGES]"],
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
        intro: "",
        long_description: None,
        usage: &["datum ignore <FIELD> [SETUP CHANGES]"],
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
        intro: "",
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
        summary: "DB-lifecycle (purge, backup, restore, migrate, v2import)",
        intro: DB_INTRO,
        long_description: None,
        usage: &[
            "datum db purge [--leave [--force] | --rejoin] [--yes | -y]",
            "datum db purge --chain [--yes | -y] [--admin-key <FILE>]",
            "datum db backup <PATH | -> [--overwrite]",
            "datum db backup --to DIR --every DUR [--slots N]",
            "datum db backup --list DIR",
            "datum db backup --plan --every DUR [--slots N] --span DUR",
            "datum db restore <PATH | -> [--merge | --hard [--leave]] [--force] [--dry-run] [--yes | -y] [--if-unchanged FP]",
            "datum db migrate [run | status | edit]",
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
        intro: "",
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
        intro: "",
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
        intro: "",
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
        intro: "",
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
        intro: STATS_INTRO,
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
        intro: "",
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
        intro: "",
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
        name: "states",
        aliases: &[],
        category: Category::ViewsSync,
        summary: "a field's declared and logged states, with counts and last use",
        intro: "",
        long_description: None,
        usage: &["datum states <FIELD> [--json]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
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
        intro: SYNC_INTRO,
        long_description: None,
        usage: &[
            "datum sync",
            "datum sync status [--show-revoked] [--json]",
            "datum sync pause",
            "datum sync resume",
            "datum sync diff <WHO> [--json]",
            "datum sync refold [--dry-run] [--json]",
            "datum sync chain invite [--reader] [--fields <F,…> | --kit <KIT>] [--qr] [--expires <DUR>] [--label <L>] [--admin-key <FILE>]",
            "datum sync chain connect [<TICKET>] [--label <L>]",
            "datum sync chain leave [--force]",
            "datum sync chain list [--show-revoked] [--json]",
            "datum sync chain rename [<WHO>] (<LABEL> | --unset) [--admin-key <FILE>]",
            "datum sync chain revoke <WHO> [--admin-key <FILE>]",
            "datum sync chain revert <WHO> [--yes] [--json] [--admin-key <FILE>]",
            "datum sync chain purge (--data | --all) [--yes] [--admin-key <FILE>]",
            "datum sync chain role <WHO> <member|reader> [--fields <F,…> | --kit <KIT>] [--admin-key <FILE>]",
            "datum sync chain ack [<WHO|unlock> | --all]",
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
        intro: "",
        long_description: None,
        usage: &[
            "datum conflicts [list] [--json]",
            "datum conflicts resolve <HID> [--use-a | --use-b | --prefer-a | --prefer-b | --merge] [KEY=a|b|VALUE…]",
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
        intro: "",
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
        intro: "",
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
        intro: "",
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
        intro: "",
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
        intro: "",
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
/// A [`page_name`]'s first word is its command: `db purge` finds `db`.
pub fn find_command(query: &str) -> Option<(&'static CommandSpec, bool)> {
    let query = query.split(' ').next().unwrap_or(query);
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

/// The page `datum CMD WORDS… --help` asks for: CMD, then each of WORDS that
/// names a subcommand of what precedes it — `db purge`, `sync chain purge`
/// (da-2bb7z). The first word that names none ends it: `occur coffee` is
/// `occur`'s page.
pub fn page_name(cmd: &str, words: &[&str]) -> String {
    let mut page = cmd.to_string();
    let Some((spec, _)) = find_command(cmd) else {
        return page;
    };
    let mut path = format!("datum {}", spec.name);
    for (depth, w) in words.iter().enumerate() {
        let deeper = format!("{path} {w}");
        let names = |s: &SubcommandSpec| {
            depth == 0 && s.name == *w || s.usage.iter().any(|u| starts_with_words(u, &deeper))
        };
        if w.starts_with('-') || !spec.subcommands.iter().any(names) {
            break;
        }
        page = format!("{page} {w}");
        path = deeper;
    }
    page
}

/// Whether `line` begins with the words of `prefix`, whole.
pub(crate) fn starts_with_words(line: &str, prefix: &str) -> bool {
    line.strip_prefix(prefix)
        .is_some_and(|r| r.is_empty() || r.starts_with(' '))
}
