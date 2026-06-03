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
        summary: "state|value|verb|field",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "since",
        takes_value: true,
        value_hint: Some("TIME"),
        value_optional: false,
        summary: "RFC3339 / YYYY-MM-DD / duration offset (1d, 30m, 2h)",
        long_description: None,
        left_override: None,
    },
    FlagSpec {
        short: None,
        long: "json",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Machine-readable output",
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
    FlagSpec {
        short: None,
        long: "json",
        takes_value: false,
        value_hint: None,
        value_optional: false,
        summary: "Machine-readable output",
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

const GET_BODY: &str = "USAGE:\n  datum get <ID_OR_SLUG> [--pretty]\n\nStructured states (objects with `id` and other fields) render as an `{id}`\nchip on the main info line, with sub-fields broken out on a state second\nline below. Parallel states wrap in `[ … ]` with one inner state per line.\nThe per-entry data block follows the same alignment convention — keys are\nright-aligned so their trailing `:` shares a column when the block goes\nmultiline.\n";

const DELETE_BODY: &str = "USAGE:\n  datum delete [ID_OR_SLUG[,ID_OR_SLUG]...] [--force] [--dry-run] [--strict]\n                [--show V | -A | -o FMT]\n\nARGS:\n  [ID_OR_SLUG]  QuickId(s) — hid prefix, slug, `_`/`_LAST`, `<field>:_`,\n                slug wildcard. Multiple targets may be comma-batched\n                (`<id>,<id>,...`); resolution is per-token.\n                Omitting the arg defaults to `_LAST_WITH_PROTECTION`,\n                which refuses if the last-accessed pointer is > 15 min\n                stale (prevents accidental delete of an old `_LAST`).\n\nOPTIONS:\n      --force       Skip the y/N confirmation prompt.\n      --dry-run     Print what would be deleted and exit; no DB writes.\n      --strict      With comma-batched tokens, abort up-front if ANY\n                    token failed to resolve (default: delete the\n                    resolved subset, skip-and-warn on the failures).\n  -A, --show-all    Show the full entry document for each deleted row.\n      --show <V>    Output verbosity: default|none|minimal|standard|format|all.\n  -o, --format-string <FMT>\n                    Render each deleted row through datum's format-string DSL.\n\nEXAMPLES:\n  datum delete                    # delete _LAST (refuses if > 15 min stale)\n  datum delete activity:_         # delete latest activity row\n  datum delete a1b2,c3d4 --force  # batched, no prompt\n  datum delete a1b2,c3d4 --dry-run\n                                  # preview only — no DB writes\n";

const UNDO_BODY: &str = "USAGE:\n  datum undo [ID_OR_SLUG] [--to-version N] [--force] [--window 15m]\n\nReverts an entry to a prior version via its entry_history trail. By default,\nrestores the immediately-previous version; --to-version N picks an explicit\nversion. Falls back to undeleting recently-tombstoned legacy entries (no\nhistory) within --window (default 15m).\n";

const HISTORY_BODY: &str =
    "USAGE:\n  datum history <ID_OR_SLUG> [--version N | --diff A B] [--json]\n";

const WHY_BODY: &str = "USAGE:\n  datum why <ID_OR_SLUG> [--full]\n";

const EDIT_BODY: &str = "USAGE:\n  datum edit <ID_OR_SLUG>\n";

const RETIME_BODY: &str = "USAGE:\n  datum retime [SLUG] [SHIFT]\n  datum retime [SLUG] [-r] [-t T] [-y]... [-q]... [-d D]\n  datum retime [SLUG] --offset DUR [--field F] [--after BND] [--before BND]\n\nDEFAULTS:\n  SLUG omitted        → `_LAST` (matches update/delete/undo)\n  No time-args        → set to now\n  SHIFT (bare int)    → minutes-offset shorthand; `retime <id> -5` shifts -5 min\n  -r/--relative       → interpret time-args as a delta from now, applied to the\n                        entry's current occur_time (rather than absolute target).\n                        Same flag is `--reverse` on tail; namespaces don't collide.\n  --offset DUR        → explicit ISO duration shift (legacy form)\n";

const SPEC_BODY: &str = "USAGE:\n  datum spec show <FIELD>\n  datum spec set <FIELD> [--json JSON] [--default-verb V] [--state S]... [--tag T]... ...\n  datum spec list [--tag T]\n  datum spec remove <FIELD>\n";

const ALIAS_LONG_DESCRIPTION: &str = "Aliases expand argv before the parser sees it, so an alias can stand in for any prefix of a `datum` invocation — verb, field, flags, or all three.\n\nA standalone, whitespace-bounded `,` token splits one invocation into N sides that write atomically with the same occur_time. The trigger works at the top-level CLI, inside alias bodies, and through chains of alias-of-alias (multi-of-multi flattens to N flat sides). The comma must be its own token: `vertical , sit` triggers a multi, while `vertical,sit`, `vertical ,sit`, and `vertical, sit` do not. Values with embedded commas (`key=a,b`) pass through unchanged.\n\nKey slot forms (`-k` and `-K` accept the same three shapes): `-k key` is required and errors at call time if unfilled, `-k key=` is optional with no default (silently skipped), `-k key=val` defaults to `val` when no value is supplied. A `-k` or `-K` name declared on multiple sides becomes a shared slot — one call-site value propagates to every declaring side. The `.` token skips a slot on its side; for a shared slot it skips on all sides.\n\n`--` semantics in multi-entry contexts: inside an alias body, flags before `--` scope to their side and flags after `--` apply to all sides. `--` is implicit at the end of every alias body, so call-site flags apply to all sides by default. A `--` inside an alias body does not propagate past the alias boundary. Call-site time flags compose with alias-bound times rather than overriding them.\n\nAfter a multi-entry write, `_LAST` refers to every entry written by the most recent invocation, so `rekey _LAST oldkey=newkey` affects every row of the atomic group. A subsequent single-entry write resets `_LAST` to that one row.";

const ALIAS_BODY: &str = "USAGE:\n  datum alias show <NAME>\n  datum alias set <NAME> <EXPANSION>\n  datum alias list\n  datum alias remove <NAME>\n\nATOMIC MULTI-ENTRY ALIASES:\n  A standalone, whitespace-bounded `,` token splits one invocation\n  into N sides that all write atomically with the same occur_time.\n  Works at the top-level CLI, inside alias bodies, and through chains\n  of alias-of-alias (multi-of-multi flattens to N flat sides).\n\n  The comma must be its own token. `vertical , sit` triggers a multi;\n  `vertical,sit`, `vertical ,sit`, and `vertical, sit` do not. Values\n  with embedded commas (`key=a,b`) pass through unchanged.\n\n  Examples:\n    datum switch body vertical , switch body sit\n        Two same-field switches at one occur_time merge into one\n        body row with state=Parallel([vertical, sit]).\n    datum alias set oov \\\n        \"switch environment outside , switch body vertical\"\n        Define a multi-entry alias. `datum oov` writes both rows.\n    datum alias set vvcaf \\\n        \"switch body vertical , occur caffeine -t +5\"\n    datum vvcaf -t +10\n        Call-site time flags compose with alias-bound times (not\n        override): vertical at +10min, caffeine at +15min.\n\nKEY SLOT FORMS (-k / -K):\n  -k key       required (errors at call site if unfilled)\n  -k key=      optional, no default (silently skipped if unfilled)\n  -k key=val   defaulted (auto-filled with `val` when unfilled)\n\n  A `-k` (or `-K`) name declared on multiple sides becomes a shared\n  slot — one call-site value propagates to every declaring side. `.`\n  skips a slot on its side; `.` for a shared slot skips on all sides.\n\nFIELD SLOT FORMS (-F):\n  Same three shapes as `-k`/`-K`, but the resolved value becomes the\n  entry's field rather than a data key.\n    -F field       required (errors if unfilled)\n    -F field=      optional, no default (no field bound when unfilled)\n    -F field=val   defaulted (entry field is `val` when unfilled)\n\n  Shared across sides by name like `-k`: one call-site value\n  propagates to every declaring side.\n\n`--` SEMANTICS IN MULTI-ENTRY CONTEXTS:\n  Inside an alias body, flags before `--` scope to their side; flags\n  after `--` apply to all sides. `--` is implicit at the end of every\n  alias body, so call-site flags apply to all sides by default. A `--`\n  inside an alias body does not propagate past the alias boundary.\n\n`_LAST` AFTER A MULTI-WRITE:\n  `_LAST` refers to every entry written by the most recent invocation,\n  so `rekey _LAST oldkey=newkey` affects every row of an atomic group.\n  A subsequent single-entry write resets `_LAST` to that one row.\n";

const TRACK_BODY: &str =
    "USAGE:\n  datum track <FIELD> [--color C] [--state S=C]... [--tag T]...\n";

const IGNORE_BODY: &str = "USAGE:\n  datum ignore <FIELD>\n";

const GREP_BODY: &str = "USAGE:\n  datum grep <PATTERN>... [-f FIELD] [--show V | -A | -o FMT]\n\nARGS:\n  <PATTERN>...    One or more regex patterns; rows must match EVERY one\n                  (AND semantics). Each pattern is matched against the\n                  entry's serialized JSON representation, so slug, value,\n                  extra_data, and metadata are all in scope.\n\nOPTIONS:\n  -f, --field <F>      Restrict the search to entries on field F.\n  -A, --show-all       Show the full entry document (verbose form).\n      --show <V>       Output verbosity: default|none|minimal|standard|format|all.\n  -o, --format-string <FMT>\n                       Render each hit through datum's format-string mini-DSL.\n\nEXAMPLES:\n  datum grep coffee                      # any entry mentioning 'coffee'\n  datum grep -f mood happy               # mood rows with state/value matching 'happy'\n  datum grep -A 'kg' caffeine            # caffeine rows with units/value matching 'kg'\n";

const CHECK_BODY: &str = "USAGE:\n  datum check [CHECKER]... [--fix] [--json]\n  datum check --list [--json]\n\nARGS:\n  [CHECKER]...   Run only the named checker(s). With no positionals, the\n                 full default battery runs. Known names:\n                   verbs      Drift between stored verb and derive_verb\n                   aliases    Spec/user alias divergence + state collisions\n                   slugs      Drift between stored slug and re-derived slug\n                   integrity  Pending warnings from the conflict_queue\n                   flags      Pending review markers from `datum flag`\n\nOPTIONS:\n  --fix          Rewrite drifted `verb` / `slug` columns to match what\n                 the live deriver would produce; annotate alias state\n                 collisions with the explicit first-saved-wins\n                 resolution; rewrite legacy `content_hash` columns\n                 that disagree with `expected_content_hash`.\n  --list         Print the set of known checker names and exit.\n  --json         Machine-readable output.\n\nEXAMPLES:\n  datum check                  # full default battery\n  datum check verbs            # only verb-drift\n  datum check aliases --fix    # only alias divergence, annotate collisions\n  datum check integrity flags  # only the queue + review-marker views\n  datum check slugs --fix      # rewrite drifted slug columns in place\n";

const DAYVIEW_BODY: &str = "USAGE:\n  datum dayview [OPTIONS]\n\nOPTIONS:\n  --start-date <DATE>      Start date (ISO YYYY-MM-DD or relative offset like -3, 0)\n  --end-date <DATE>, -d    End date (default: today)\n  --n-days <N>, -n         Number of days (default: 1)\n  --width <N>              SVG width in pixels (default: 2000)\n  --height <N>             Total SVG height (auto from day-height + n-days when unset)\n  --day-height <N>         Per-day band height (default: 100)\n  --time-axis-height <N>   Bottom time-axis strip height (default: 15)\n  --margin <N>             Outer margin (default: 2)\n  -o, --output-file <PATH> .svg / .png / .html (default: SVG to stdout)\n  --json                   Emit layout JSON instead of SVG (debug)\n  -w, --watch              Redraw on a 5-minute poll loop\n";

const NOWVIEW_BODY: &str = "USAGE:\n  datum nowview [OPTIONS]\n\nOPTIONS:\n  --history <DUR>          History window before \"now\" (e.g. 15m, 1h). Default: 30m.\n  --width <N>              SVG width in pixels (auto-scales with --history when unset)\n  --height <N>             SVG height in pixels (default 300)\n  --margin <N>             Outer margin (default 2)\n  --time-axis-height <N>   Time-axis strip height (default 15)\n  -o, --output-file <PATH> .svg / .png / .html (default: SVG to stdout)\n  -w, --watch              Redraw on DB change (60s idle refresh)\n";

const INFO_BODY: &str = "USAGE:\n  datum info [TARGET] [--all]\n";

const DB_BODY: &str = "USAGE:\n  datum db setup\n  datum db purge [--yes | -y]\n  datum db backup <PATH> [--overwrite]\n  datum db restore <PATH> [--force] [--dry-run] [--json]\n  datum db migrate (run | status | edit)\n  datum db import [FILE | --from FILE] [--dry-run]\n\nSetup applies any pending schema migrations to the active database. Use\n`datum db purge` to drop and recreate the schema from scratch.\n\nPurge drops every datum table and rebuilds the schema from scratch. Without\n--yes, prompts for the database path as confirmation; aborts on mismatch.\nNon-TTY stdin without --yes is rejected.\n\nBackup dumps live entries to JSONL (default) or tar.gz when the path ends in\n.tar.gz / .tgz. Restore replays a backup file into the active database.\n\nMigrate is the schema-migration runner: `run` advances the DB to the binary's\ntarget version, `status` lists applied/pending steps, `edit` is reserved for\nper-entry data migrations (not yet implemented).\n\nImport ingests a legacy datum CouchDB backup JSON (`{ docs: [...] }`) into\nthe active database. Idempotent — re-importing the same backup is a no-op.\n";

const CONFIG_BODY: &str = "USAGE:\n  datum config show [--json]\n";

const SYNC_BODY: &str = "USAGE:\n  datum sync [--push | --pull] [--dry-run] [--server URL]\n  datum sync login [--server URL] [--cluster NAME]\n  datum sync config set-server <URL> [--global]\n  datum sync config disable\n  datum sync config show\n  datum sync key (init | unlock | rotate-passphrase | show-fingerprint) [--server URL] [--force]\n  datum sync status [--server URL]\n  datum sync install-daemon [--show | --uninstall]\n";

const CONFLICTS_BODY: &str = "USAGE:\n  datum conflicts list [--kind K] [--all]\n  datum conflicts resolve <HID> (--keep V | --close-older)\n  datum conflicts auto-resolve [--min-age SECS]\n";

const FLAG_BODY: &str = "USAGE:\n  datum flag <FIELD> [COMMENT...] [-y / -q / -d / -t / -z]\n\nAttaches a 'come back to this' marker to <FIELD>. Stored as a Verb::Point\nentry with `extra_metadata = { flag: true, comment: \"...\" }`. The bare\nform `datum flag <FIELD>` records a marker at now with no comment.\n\nDefault rendering hides flags from `datum tail` and `datum now`; use\n`tail --show-meta` to include them, or `datum check` to surface them as\npending review markers. The shape mirrors `datum meta` (which sets\n`meta: true` instead of `flag: true`).\n";

const META_BODY: &str = "USAGE:\n  datum meta <FIELD> <TEXT>...\n  datum meta show <FIELD>\n\nAttach (or list) a protocol-change note on <FIELD>. Stored as a\nVerb::Point entry with `extra_metadata = { meta: true, comment: \"...\" }`.\n`meta show` lists every note attached to <FIELD> in create_time order.\n\nLike `flag`, meta rows are filtered from default tail / now views\n(use `tail --show-meta` to include them).\n";

const KIT_BODY: &str = "USAGE:\n  datum kit install <PATH> [--force]\n  datum kit export <NAME> [PATH]\n  datum kit list\n  datum kit uninstall <NAME>\n  datum kit show <NAME>\n\nA kit is a TOML bundle of field specs, aliases, SQL views, indices, and\nlevel-1/2 commands. Install upserts the kit's rows tagged with the kit id;\nuninstall drops every row with that id. Export round-trips the installed\nrows back to TOML (defaults to stdout when path is omitted).\n\nList shows every installed kit and its field count. Show prints a single\nkit's installed contents.\n";

const VIEW_BODY: &str = "USAGE:\n  datum view <NAME> [--format FMT] [--watch]\n  datum view --list\n\nARGS:\n  <NAME>           Installed view name (see `datum view --list`).\n\nOPTIONS:\n  -f, --format <FMT>   pretty | tsv | csv | json | auto (default auto).\n  -w, --watch          Re-run the query every 2s; clear-screen between\n                       redraws.\n  --list               Print installed view names and exit.\n\nViews execute under PRAGMA query_only = 1, so a malicious or buggy\nview cannot mutate the DB.\n";

const REPARSE_BODY: &str = "USAGE:\n  datum reparse <HID>... [--fix]\n\nARGS:\n  <HID>...      One or more entries to replay. At least one is required;\n                reparse never walks the entry log implicitly.\n\nOPTIONS:\n  --fix         UPDATE rows where the parser produced a different shape\n                from the persisted columns; bumps `parser_version` and\n                recomputes `content_hash`. Default is dry-run (report only).\n\nReplay is hermetic to the alias state at write time — the entry's\nstored `alias_chain` is rehydrated as the resolver, never the live\n`aliases` table (da-bkak).\n";

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
pub const TOP_LEVEL_FOOTER: &str = "Run `datum <command> --help` for per-command flag detail.\n\
     With no subcommand, `datum` drops into an interactive REPL \
     (inherits `--db`).";

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
        long_description: Some(
            "Structured-state entries show their id only on the chip line \
             (`{id}`); use `datum get` for the full structure.",
        ),
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
        flags: HEAD_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 20,
        body_override: None,
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
        flags: UPDATE_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 26,
        body_override: None,
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
        flags: UPDATE_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 26,
        body_override: None,
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
        flags: UPDATE_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 26,
        body_override: None,
        display_input_name: false,
    },
    CommandSpec {
        name: "retime",
        aliases: &["rt"],
        category: Category::Mutate,
        summary: "shift time_start/time_end on entries (alias: rt)",
        top_summary: None,
        long_description: None,
        usage: &[
            "datum retime [SLUG] [SHIFT]",
            "datum retime [SLUG] [-r] [-t T] [-y]... [-q]... [-d D]",
            "datum retime [SLUG] --offset DUR [--field F] [--after BND] [--before BND]",
        ],
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
        long_description: Some(ALIAS_LONG_DESCRIPTION),
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
        usage: &["datum check [CHECKER]... [--fix] [--json]"],
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
        flags: COUNT_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 24,
        body_override: None,
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
        flags: IDS_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 22,
        body_override: None,
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
            "datum sync login [--server URL] [--cluster NAME]",
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
    CommandSpec {
        name: "flag",
        aliases: &[],
        category: Category::Logging,
        summary: "mark a field for later review",
        top_summary: Some("Mark a field for review (lightweight 'come back to this')"),
        long_description: None,
        usage: &["datum flag <FIELD> [COMMENT...]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(FLAG_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "meta",
        aliases: &[],
        category: Category::Admin,
        summary: "manage protocol-change notes on a field",
        top_summary: Some("Attach or list protocol-change notes on a field"),
        long_description: None,
        usage: &[
            "datum meta <FIELD> <TEXT>...",
            "datum meta show <FIELD>",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(META_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "kit",
        aliases: &[],
        category: Category::Admin,
        summary: "install / export / list field-spec kits",
        top_summary: Some("Install / export / list field-spec kits (TOML bundles)"),
        long_description: None,
        usage: &[
            "datum kit install <PATH> [--force]",
            "datum kit export <NAME> [PATH]",
            "datum kit list",
            "datum kit uninstall <NAME>",
            "datum kit show <NAME>",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
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
        summary: "execute a named SELECT installed via a kit (or via spec)",
        top_summary: Some("Execute a named SELECT view and render the rows"),
        long_description: None,
        usage: &[
            "datum view <NAME> [--format pretty|tsv|csv|json] [--watch]",
            "datum view --list",
        ],
        args: NO_ARGS,
        flags: NO_FLAGS,
        examples: NO_EXAMPLES,
        summary_col: 0,
        body_override: Some(VIEW_BODY),
        display_input_name: false,
    },
    CommandSpec {
        name: "reparse",
        aliases: &[],
        category: Category::Admin,
        summary: "replay stored raw_input against the current parser",
        top_summary: Some("Replay an entry's stored raw_input through the current parser; --fix rewrites drifted columns"),
        long_description: None,
        usage: &["datum reparse <HID>... [--fix]"],
        args: NO_ARGS,
        flags: NO_FLAGS,
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
