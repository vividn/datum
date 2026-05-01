//! The `COMMANDS` table — one [`CommandSpec`] per subcommand of `datum`.
//!
//! Layout mirrors the section ordering of `datum --help`:
//! Logging → Mutate → Read → Render → Admin → ViewsSync.

use super::spec::{ArgSpec, Category, CommandSpec, ExampleSpec, FlagSpec};

const NO_ARGS: &[ArgSpec] = &[];
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
        summary: "Date override (offset or YYYY-MM-DD)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('t'),
        long: "time",
        takes_value: true,
        value_hint: Some("VAL"),
        value_optional: false,
        summary: "Time override (offset or HH:MM or 9:15am)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "dur",
        takes_value: true,
        value_hint: Some("VAL"),
        value_optional: false,
        summary: "Duration (30m, 1:30, 2h, PT30M)",
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
        short: Some('u'),
        long: "unit",
        takes_value: true,
        value_hint: Some("UNIT"),
        value_optional: false,
        summary: "Unit for the primary value",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('k'),
        long: "key",
        takes_value: true,
        value_hint: Some("KEY_SPEC"),
        value_optional: false,
        summary: "Data-only id-key slot (repeatable)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: Some('K'),
        long: "slug-key",
        takes_value: true,
        value_hint: Some("S"),
        value_optional: false,
        summary: "Slug-contributing id-key (repeatable)",
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
        long_description: None,
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
        summary: "Route through timeless `add`",
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
];

// Shared body for all log verbs — parameterized by the verb at lookup time.
// Stored as an associated function below rather than a const because of the verb substitution.

// ---------------------------------------------------------------------------
// Tail flag table (used both as structured render and also matches body).
// ---------------------------------------------------------------------------

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
        summary: "Stream new entries (default poll 1s)",
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
];

// ---------------------------------------------------------------------------
// Per-command body_override constants — verbatim copies of the legacy
// hand-authored strings. These guarantee byte-identical output for the
// transition; structured render coverage will expand in follow-up PRs.
// ---------------------------------------------------------------------------

const HEAD_BODY: &str = "USAGE:\n  datum head [FIELD]\n\nOPTIONS:\n  -n, --limit <N>   Number of entries (default 10)\n";

const NOW_BODY: &str = "USAGE:\n  datum now [-A] [--bare] [--no-color] [--cols N]\n\n  -A           Show ignored fields too (dim)\n      --bare   Suppress the terminal nowview band; only show the state list\n      --no-color   Disable ANSI color in the nowview band\n      --cols N Override terminal width in cells (auto-detected from $COLUMNS)\n";

const GET_BODY: &str = "USAGE:\n  datum get <ID_OR_SLUG> [--pretty]\n";

const DELETE_BODY: &str = "USAGE:\n  datum delete <ID_OR_SLUG> [--force] [--dry-run] [--strict]\n";

const UNDO_BODY: &str = "USAGE:\n  datum undo [ID_OR_SLUG] [--to-version N] [--force] [--window 15m]\n\nReverts an entry to a prior version via its entry_history trail. By default,\nrestores the immediately-previous version; --to-version N picks an explicit\nversion. Falls back to undeleting recently-tombstoned legacy entries (no\nhistory) within --window (default 15m).\n";

const HISTORY_BODY: &str =
    "USAGE:\n  datum history <ID_OR_SLUG> [--version N | --diff A B] [--json]\n";

const WHY_BODY: &str = "USAGE:\n  datum why <ID_OR_SLUG> [--full]\n";

const EDIT_BODY: &str = "USAGE:\n  datum edit <ID_OR_SLUG>\n";

const UPDATE_BODY: &str = "USAGE:\n  datum update <ID_OR_SLUG> [VALUE | KEY=VALUE]...\n\nOPTIONS:\n  --strategy <S>        update|merge|rekey\n  --id <TEMPLATE>       New id/slug template\n  --id-delimiter <S>    Delimiter between empty segments\n  -k, --key <KV>        Extra key=value (repeatable)\n  -c, --comment <T>     Comment\n  --dry-run             Show what would change\n  --strict              Abort on first batch error\n";

const MERGE_BODY: &str = "USAGE:\n  datum merge <ID_OR_SLUG> [VALUE | KEY=VALUE]...\n\nOPTIONS:\n  --strategy <S>        update|merge|rekey\n  --id <TEMPLATE>       New id/slug template\n  --id-delimiter <S>    Delimiter between empty segments\n  -k, --key <KV>        Extra key=value (repeatable)\n  -c, --comment <T>     Comment\n  --dry-run             Show what would change\n  --strict              Abort on first batch error\n";

const REKEY_BODY: &str = "USAGE:\n  datum rekey <ID_OR_SLUG> [VALUE | KEY=VALUE]...\n\nOPTIONS:\n  --strategy <S>        update|merge|rekey\n  --id <TEMPLATE>       New id/slug template\n  --id-delimiter <S>    Delimiter between empty segments\n  -k, --key <KV>        Extra key=value (repeatable)\n  -c, --comment <T>     Comment\n  --dry-run             Show what would change\n  --strict              Abort on first batch error\n";

const RETIME_BODY: &str = "USAGE:\n  datum retime [SLUG] (--offset DUR | --time NEW) [--field F] [--after BND] [--before BND]\n";

const SPEC_BODY: &str = "USAGE:\n  datum spec show <FIELD>\n  datum spec set <FIELD> [--json JSON] [--default-verb V] [--state S]... [--tag T]... ...\n  datum spec list [--tag T]\n  datum spec remove <FIELD>\n";

const ALIAS_BODY: &str = "USAGE:\n  datum alias show <NAME>\n  datum alias set <NAME> <EXPANSION>\n  datum alias list\n  datum alias remove <NAME>\n";

const TRACK_BODY: &str =
    "USAGE:\n  datum track <FIELD> [--color C] [--state S=C]... [--tag T]...\n";

const IGNORE_BODY: &str = "USAGE:\n  datum ignore <FIELD>\n";

const GREP_BODY: &str = "USAGE:\n  datum grep <PATTERN>... [-f FIELD]\n";

const CHECK_BODY: &str = "USAGE:\n  datum check [--json]\n";

const COUNT_BODY: &str = "USAGE:\n  datum count [FIELD] [--group-by KEY] [--since TIME] [--json]\n\nOPTIONS:\n  --group-by <KEY>   state|value|verb|field\n  --since <TIME>     RFC3339 / YYYY-MM-DD / duration offset (1d, 30m, 2h)\n  --json             Machine-readable output\n";

const IDS_BODY: &str = "USAGE:\n  datum ids [FIELD] [-n N] [--oldest-first] [--since TIME] [--slug] [--json]\n\nOPTIONS:\n  -n, --limit <N>    Cap the number of ids returned\n  --oldest-first     Return oldest-first instead of newest-first\n  --since <TIME>     Same grammar as `datum count --since`\n  --slug             Print slugs instead of short hids\n  --json             Machine-readable output\n";

const DAYVIEW_BODY: &str = "USAGE:\n  datum dayview [OPTIONS]\n\nOPTIONS:\n  --start-date <DATE>      Start date (ISO YYYY-MM-DD or relative offset like -3, 0)\n  --end-date <DATE>, -d    End date (default: today)\n  --n-days <N>, -n         Number of days (default: 1)\n  --width <N>              SVG width in pixels (default: 2000)\n  --height <N>             Total SVG height (auto from day-height + n-days when unset)\n  --day-height <N>         Per-day band height (default: 100)\n  --time-axis-height <N>   Bottom time-axis strip height (default: 15)\n  --margin <N>             Outer margin (default: 2)\n  -o, --output-file <PATH> .svg / .png / .html (default: SVG to stdout)\n  --json                   Emit layout JSON instead of SVG (debug)\n  -w, --watch              Redraw on a 5-minute poll loop\n";

const NOWVIEW_BODY: &str = "USAGE:\n  datum nowview [OPTIONS]\n\nOPTIONS:\n  --history <DUR>          History window before \"now\" (e.g. 15m, 1h). Omit for now-only view.\n  --width <N>              SVG width in pixels (auto-scales with --history when unset)\n  --height <N>             SVG height in pixels (default 300)\n  --now-width <DUR>        Width of the right \"now\" panel in time units (default 5m)\n  --margin <N>             Outer margin (default 2)\n  --time-axis-height <N>   Time-axis strip height (default 15)\n  -o, --output-file <PATH> .svg / .png / .html (default: SVG to stdout)\n  -w, --watch              Redraw every minute\n";

const INFO_BODY: &str = "USAGE:\n  datum info [TARGET] [--all]\n";

const DB_BODY: &str = "USAGE:\n  datum db setup\n  datum db purge [--yes | -y]\n  datum db backup <PATH> [--overwrite]\n  datum db restore <PATH> [--force] [--dry-run] [--json]\n  datum db migrate (run | status | edit)\n  datum db import [FILE | --from FILE] [--dry-run]\n\nSetup applies any pending schema migrations to the active database. Use\n`datum db purge` to drop and recreate the schema from scratch.\n\nPurge drops every datum table and rebuilds the schema from scratch. Without\n--yes, prompts for the database path as confirmation; aborts on mismatch.\nNon-TTY stdin without --yes is rejected.\n\nBackup dumps live entries to JSONL (default) or tar.gz when the path ends in\n.tar.gz / .tgz. Restore replays a backup file into the active database.\n\nMigrate is the schema-migration runner: `run` advances the DB to the binary's\ntarget version, `status` lists applied/pending steps, `edit` is reserved for\nper-entry data migrations (not yet implemented).\n\nImport ingests a legacy datum CouchDB backup JSON (`{ docs: [...] }`) into\nthe active database. Idempotent — re-importing the same backup is a no-op.\n";

const CONFIG_BODY: &str = "USAGE:\n  datum config show [--json]\n";

const SYNC_BODY: &str = "USAGE:\n  datum sync [--push | --pull] [--dry-run] [--server URL]\n  datum sync login [--server URL]\n  datum sync config set-server <URL> [--global]\n  datum sync config disable\n  datum sync config show\n  datum sync key (init | unlock | rotate-passphrase | show-fingerprint) [--server URL] [--force]\n  datum sync status [--server URL]\n  datum sync install-daemon [--show | --uninstall]\n";

const CONFLICTS_BODY: &str = "USAGE:\n  datum conflicts list [--kind K] [--all]\n  datum conflicts resolve <HID> (--keep V | --close-older)\n  datum conflicts auto-resolve [--min-age SECS]\n";

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
        summary: "default|none|minimal|standard|format|all",
        long_description: None,
        left_override: Some("--show <MODE>"),
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
pub const TOP_LEVEL_FOOTER: &str = "Run `datum <command> --help` for per-command flag detail.";

// ---------------------------------------------------------------------------
// COMMANDS table — listed in display order for each category.
// ---------------------------------------------------------------------------

pub const COMMANDS: &[CommandSpec] = &[
    // ----- LOGGING -----
    CommandSpec {
        name: "add",
        aliases: &[],
        category: Category::Logging,
        summary: "log an entry (timeless data)",
        top_summary: Some("Add a new entry (timeless data)"),
        long_description: None,
        usage: &["datum add <FIELD> [VALUE | KEY=VALUE]..."],
        args: NO_ARGS,
        flags: LOG_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "occur",
        aliases: &[],
        category: Category::Logging,
        summary: "log an entry",
        top_summary: Some("Record a point-in-time event"),
        long_description: None,
        usage: &["datum occur <FIELD> [VALUE | KEY=VALUE]..."],
        args: NO_ARGS,
        flags: LOG_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "start",
        aliases: &[],
        category: Category::Logging,
        summary: "log an entry",
        top_summary: Some("Begin a duration-tracked activity"),
        long_description: None,
        usage: &["datum start <FIELD> [VALUE | KEY=VALUE]..."],
        args: NO_ARGS,
        flags: LOG_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "stop",
        aliases: &["end"],
        category: Category::Logging,
        summary: "log an entry",
        top_summary: Some("End a duration-tracked activity (alias: end)"),
        long_description: None,
        usage: &["datum stop <FIELD> [VALUE | KEY=VALUE]..."],
        args: NO_ARGS,
        flags: LOG_FLAGS,
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
        summary: "log an entry",
        top_summary: Some("Transition a field to a new state"),
        long_description: None,
        usage: &["datum switch <FIELD> [VALUE | KEY=VALUE]..."],
        args: NO_ARGS,
        flags: LOG_FLAGS,
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
        top_summary: None,
        long_description: None,
        usage: &["datum tail [FIELD]"],
        args: NO_ARGS,
        flags: TAIL_FLAGS,
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
        top_summary: None,
        long_description: None,
        usage: &["datum head [FIELD]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 20,
        body_override: Some(HEAD_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "get",
        aliases: &[],
        category: Category::Read,
        summary: "print a single entry",
        top_summary: Some("Print a single entry by id or slug"),
        long_description: None,
        usage: &["datum get <ID_OR_SLUG> [--pretty]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
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
        top_summary: None,
        long_description: None,
        usage: &["datum grep <PATTERN>... [-f FIELD]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(GREP_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "dayview",
        aliases: &[],
        category: Category::Render,
        summary: "render a multi-day timeline SVG/PNG/HTML",
        top_summary: Some("Render a dayview SVG (or dump layout JSON)"),
        long_description: None,
        usage: &["datum dayview [OPTIONS]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 27,
        body_override: Some(DAYVIEW_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "nowview",
        aliases: &[],
        category: Category::Render,
        summary: "render an SVG showing each field's current state plus history",
        top_summary: Some("Render a nowview SVG (drop -o to write to stdout)"),
        long_description: None,
        usage: &["datum nowview [OPTIONS]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 27,
        body_override: Some(NOWVIEW_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "now",
        aliases: &[],
        category: Category::Render,
        summary: "current state of tracked fields + terminal nowview band",
        top_summary: Some("Show current state of tracked fields + terminal nowview band"),
        long_description: None,
        usage: &["datum now [-A] [--bare]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(NOW_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "history",
        aliases: &[],
        category: Category::Read,
        summary: "modification trail for an entry",
        top_summary: Some("Show the full modification trail for an entry"),
        long_description: None,
        usage: &["datum history <ID_OR_SLUG> [--version N | --diff A B] [--json]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
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
        top_summary: None,
        long_description: None,
        usage: &["datum why <ID_OR_SLUG> [--full]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
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
        summary: "open an entry in $EDITOR",
        top_summary: None,
        long_description: None,
        usage: &["datum edit <ID_OR_SLUG>"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(EDIT_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "update",
        aliases: &[],
        category: Category::Mutate,
        summary: "mutate an existing entry",
        top_summary: Some("Replace keys on an existing entry"),
        long_description: None,
        usage: &["datum update <ID_OR_SLUG> [VALUE | KEY=VALUE]..."],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: Some(UPDATE_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "merge",
        aliases: &[],
        category: Category::Mutate,
        summary: "mutate an existing entry",
        top_summary: Some("Deep-merge new values into an entry"),
        long_description: None,
        usage: &["datum merge <ID_OR_SLUG> [VALUE | KEY=VALUE]..."],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: Some(MERGE_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "rekey",
        aliases: &[],
        category: Category::Mutate,
        summary: "mutate an existing entry",
        top_summary: Some("Copy an entry to a new id (old row stays)"),
        long_description: None,
        usage: &["datum rekey <ID_OR_SLUG> [VALUE | KEY=VALUE]..."],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: Some(REKEY_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "retime",
        aliases: &["rt"],
        category: Category::Mutate,
        summary: "shift time_start/time_end on entries (alias: rt)",
        top_summary: None,
        long_description: None,
        usage: &["datum retime [SLUG] (--offset DUR | --time NEW) [--field F] [--after BND] [--before BND]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
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
        top_summary: None,
        long_description: None,
        usage: &["datum delete <ID_OR_SLUG> [--force] [--dry-run] [--strict]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
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
        top_summary: Some("Undo the last change to an entry (revert via history)"),
        long_description: Some(
            "Reverts an entry to a prior version using its entry_history trail. \
             By default, undo restores the immediately-previous version; pass \
             --to-version N to restore an explicit version number. \
             For legacy entries without a history trail, undo falls back to \
             undeleting a recently tombstoned entry within --window (default 15m).",
        ),
        usage: &["datum undo [ID_OR_SLUG] [--to-version N] [--force] [--window 15m]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(UNDO_BODY),
        display_input_name: false,
    },
    // ----- ADMIN -----
    CommandSpec {
        name: "spec",
        aliases: &[],
        category: Category::Admin,
        summary: "manage field specs",
        top_summary: Some("Manage field specs (schemas)"),
        long_description: None,
        usage: &[
            "datum spec show <FIELD>",
            "datum spec set <FIELD> [--json JSON] [--default-verb V] [--state S]... [--tag T]... ...",
            "datum spec list [--tag T]",
            "datum spec remove <FIELD>",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
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
        top_summary: None,
        long_description: None,
        usage: &[
            "datum alias show <NAME>",
            "datum alias set <NAME> <EXPANSION>",
            "datum alias list",
            "datum alias remove <NAME>",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(ALIAS_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "track",
        aliases: &[],
        category: Category::Logging,
        summary: "track a field in the now view",
        top_summary: None,
        long_description: None,
        usage: &["datum track <FIELD> [--color C] [--state S=C]... [--tag T]..."],
        args: NO_ARGS,
        flags: NO_FLAGS,
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
        top_summary: None,
        long_description: None,
        usage: &["datum ignore <FIELD>"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(IGNORE_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "check",
        aliases: &[],
        category: Category::Admin,
        summary: "run DB consistency checks",
        top_summary: Some("Run database consistency checks"),
        long_description: None,
        usage: &["datum check [--json]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(CHECK_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "db",
        aliases: &[],
        category: Category::Admin,
        summary: "DB-lifecycle subcommands",
        top_summary: Some("DB-lifecycle (setup, purge, backup, restore, migrate, import)"),
        long_description: None,
        usage: &[
            "datum db setup",
            "datum db purge [--yes | -y]",
            "datum db backup <PATH> [--overwrite]",
            "datum db restore <PATH> [--force] [--dry-run] [--json]",
            "datum db migrate (run | status | edit)",
            "datum db import [FILE | --from FILE] [--dry-run]",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(DB_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "info",
        aliases: &[],
        category: Category::Admin,
        summary: "DB health/metadata",
        top_summary: Some("Show health/metadata for DB(s)"),
        long_description: None,
        usage: &["datum info [TARGET] [--all]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
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
        top_summary: None,
        long_description: None,
        usage: &["datum config show [--json]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
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
        summary: "count entries (filtered/grouped/since-bounded)",
        top_summary: Some("Count entries (optionally filtered/grouped/since-bounded)"),
        long_description: None,
        usage: &["datum count [FIELD] [--group-by KEY] [--since TIME] [--json]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 21,
        body_override: Some(COUNT_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "ids",
        aliases: &[],
        category: Category::ViewsSync,
        summary: "list entry ids (newest-first)",
        top_summary: Some("List entry ids (newest-first by default)"),
        long_description: None,
        usage: &["datum ids [FIELD] [-n N] [--oldest-first] [--since TIME] [--slug] [--json]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 21,
        body_override: Some(IDS_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "sync",
        aliases: &[],
        category: Category::ViewsSync,
        summary: "push/pull changesets, manage auth & keys",
        top_summary: None,
        long_description: None,
        usage: &[
            "datum sync [--push | --pull] [--dry-run] [--server URL]",
            "datum sync login [--server URL]",
            "datum sync config set-server <URL> [--global]",
            "datum sync config disable",
            "datum sync config show",
            "datum sync key (init | unlock | rotate-passphrase | show-fingerprint) [--server URL] [--force]",
            "datum sync status [--server URL]",
            "datum sync install-daemon [--show | --uninstall]",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(SYNC_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "conflicts",
        aliases: &[],
        category: Category::ViewsSync,
        summary: "inspect and resolve sync conflicts",
        top_summary: None,
        long_description: None,
        usage: &[
            "datum conflicts list [--kind K] [--all]",
            "datum conflicts resolve <HID> (--keep V | --close-older)",
            "datum conflicts auto-resolve [--min-age SECS]",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(CONFLICTS_BODY),
        display_input_name: false,
    },
];

/// Look up a `CommandSpec` by primary name or alias. Returns `(spec, was_alias)`.
pub fn find_command(query: &str) -> Option<(&'static CommandSpec, bool)> {
    for cmd in COMMANDS {
        if cmd.name == query {
            return Some((cmd, false));
        }
        if cmd.aliases.iter().any(|a| *a == query) {
            return Some((cmd, true));
        }
    }
    None
}
