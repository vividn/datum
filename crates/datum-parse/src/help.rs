//! `help` — hand-authored `--help` / `--version` rendering.
//!
//! The text intentionally stays terse — long-form documentation is in
//! the README / man pages.
//!
//! Top-level structure mirrors `datum`'s `src/input/topLevelHelp.ts`
//! augmented with the commands daydumb added on top. Per-command help
//! is rendered by [`command_help`] — one small hand-written block per
//! subcommand, matching the fields defined in [`crate::cli`].

pub const DATUM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Render the top-level `datum --help` string.
pub fn top_level_help() -> String {
    let mut s = String::new();
    s.push_str("datum — personal data tracker\n");
    s.push_str("\nUSAGE:\n");
    s.push_str("  datum [--db NAME_OR_PATH] <command> [args]\n");
    s.push_str("\nLOGGING:\n");
    s.push_str("  add      Add a new entry (shape inferred from kv keys)\n");
    s.push_str("  occur    Record a point-in-time event\n");
    s.push_str("  start    Begin a duration-tracked activity\n");
    s.push_str("  stop     End a duration-tracked activity (alias: end)\n");
    s.push_str("  switch   Transition a field to a new state\n");
    s.push_str("\nREAD:\n");
    s.push_str("  tail     Show recent entries\n");
    s.push_str("  head     Show oldest entries\n");
    s.push_str("  now      Show current state of tracked fields\n");
    s.push_str("  get      Print a single entry by id or slug\n");
    s.push_str("  grep     Regex search over entry JSON content\n");
    s.push_str("  dayview  Render a dayview SVG (or dump layout JSON)\n");
    s.push_str("  history  Show the full modification trail for an entry\n");
    s.push_str("  why      Explain the intent behind an entry\n");
    s.push_str("\nMUTATE:\n");
    s.push_str("  edit     Open an entry in $EDITOR\n");
    s.push_str("  update   Replace keys on an existing entry\n");
    s.push_str("  merge    Deep-merge new values into an entry\n");
    s.push_str("  rekey    Copy an entry to a new id (old row stays)\n");
    s.push_str("  retime   Shift time_start/time_end on entries (alias: rt)\n");
    s.push_str("  delete   Delete an entry (alias: del)\n");
    s.push_str("  undo     Restore the most recently soft-deleted entry\n");
    s.push_str("\nADMIN:\n");
    s.push_str("  spec     Manage field specs (schemas)\n");
    s.push_str("  alias    Manage aliases\n");
    s.push_str("  track    Track a field in the now view\n");
    s.push_str("  ignore   Ignore a field in the now view\n");
    s.push_str("  migrate  Schema migrations + legacy datum import\n");
    s.push_str("  backup   Dump live entries to JSONL / tar.gz\n");
    s.push_str("  restore  Re-import entries from a backup\n");
    s.push_str("  check    Run database consistency checks\n");
    s.push_str("  setup    Initialize or migrate the DB schema\n");
    s.push_str("  info     Show health/metadata for DB(s)\n");
    s.push_str("  config   Show or manage user configuration\n");
    s.push_str("\nVIEWS / SYNC:\n");
    s.push_str("  count      Count entries (optionally filtered/grouped/since-bounded)\n");
    s.push_str("  ids        List entry ids (newest-first by default)\n");
    s.push_str("  sync       Push/pull changesets, manage auth & keys\n");
    s.push_str("  conflicts  Inspect and resolve sync conflicts\n");
    s.push_str("\nGLOBAL OPTIONS:\n");
    s.push_str("  --db <NAME_OR_PATH>   Override the active DB\n");
    s.push_str("  --show <MODE>         default|none|minimal|standard|format|all\n");
    s.push_str("  -A, --show-all        Shorthand for --show all\n");
    s.push_str("  -o, --format-string   %field%-templated output string\n");
    s.push_str("  -h, --help            Show this help\n");
    s.push_str("  -V, --version         Show version\n");
    s.push_str("\nRun `datum <command> --help` for per-command flag detail.\n");
    s
}

/// Render the per-command help. Unknown commands fall back to the top-level.
pub fn command_help(cmd: &str) -> String {
    match cmd {
        "occur" | "start" | "stop" | "end" | "switch" | "add" => log_help(cmd),
        "tail" => tail_help(),
        "head" => head_help(),
        "now" => now_help(),
        "get" => get_help(),
        "delete" | "del" => delete_help(),
        "undo" => undo_help(),
        "history" => history_help(),
        "why" => why_help(),
        "edit" => edit_help(),
        "update" | "merge" | "rekey" => update_help(cmd),
        "retime" | "rt" => retime_help(),
        "spec" => spec_help(),
        "alias" => alias_help(),
        "track" => track_help(),
        "ignore" => ignore_help(),
        "migrate" => migrate_help(),
        "backup" => backup_help(),
        "restore" => restore_help(),
        "grep" => grep_help(),
        "check" => check_help(),
        "count" => count_help(),
        "ids" => ids_help(),
        "dayview" => dayview_help(),
        "info" => info_help(),
        "setup" => setup_help(),
        "config" => config_help(),
        "sync" => sync_help(),
        "conflicts" => conflicts_help(),
        _ => top_level_help(),
    }
}

/// Version banner (`datum --version`).
pub fn version_banner() -> String {
    format!("datum {DATUM_VERSION}")
}

fn log_help(verb: &str) -> String {
    let mut s = format!("datum {verb} — log an entry\n\nUSAGE:\n  datum {verb} <FIELD> [VALUE | KEY=VALUE]...\n\n");
    s.push_str("OPTIONS:\n");
    s.push_str("  -y                    Go back 1 day (stackable: -yy=2 days, ...)\n");
    s.push_str("  -q                    Go back 5 min (stackable: -qq=10 min, ...)\n");
    s.push_str("  -d, --date <VAL>      Date override (offset or YYYY-MM-DD)\n");
    s.push_str("  -t, --time <VAL>      Time override (offset or HH:MM or 9:15am)\n");
    s.push_str("      --dur <VAL>       Duration (30m, 1:30, 2h, PT30M)\n");
    s.push_str("  -z, --timezone <TZ>   Timezone (IANA or UTC offset)\n");
    s.push_str("  -u, --unit <UNIT>     Unit for the primary value\n");
    s.push_str("  -k, --key <KEY_SPEC>  Data-only id-key slot (repeatable)\n");
    s.push_str("  -K, --slug-key <S>    Slug-contributing id-key (repeatable)\n");
    s.push_str("  -c, --comment <T>     Comment\n");
    s.push_str("      --id <TEMPLATE>   Slug template (e.g. %field%:%uid%)\n");
    s.push_str("      --id-delimiter <S>  Delimiter to collapse empty segments\n");
    s.push_str("  -L, --last-state[=<V>]  Assert prior state (bare: close-most-recent on stop)\n");
    s.push_str("  -m, --moment          Point-in-time state change\n");
    s.push_str("      --conflict <S>    update|merge|rekey|ignore\n");
    s.push_str("      --update          Shorthand for --conflict update\n");
    s.push_str("      --merge           Shorthand for --conflict merge\n");
    s.push_str("  -M, --no-metadata     Zero created_at/modified_at, clear device_id\n");
    s.push_str("  -T, --no-timestamp    Route through timeless `add`\n");
    s
}

fn tail_help() -> String {
    let mut s = String::from("datum tail — show recent entries\n\nUSAGE:\n  datum tail [FIELD]\n\n");
    s.push_str("OPTIONS:\n");
    s.push_str("  -n <N>                Number of entries (default 10)\n");
    s.push_str("  -m, --metric <M>      hybrid|occur|create|modify\n");
    s.push_str("  -r, --reverse         Invert ordering\n");
    s.push_str("      --column <NAME>   Extra extra_data column (repeatable)\n");
    s.push_str("  -d, --date <VAL>      Date override\n");
    s.push_str("  -t, --time <VAL>      Upper-bound time\n");
    s.push_str("  -y / -q               Day / 5-min offsets (stackable)\n");
    s.push_str("  -w, --watch [INT]     Stream new entries (default poll 1s)\n");
    s
}

fn head_help() -> String {
    String::from(
        "datum head — show oldest entries\n\nUSAGE:\n  datum head [FIELD]\n\nOPTIONS:\n  -n, --limit <N>   Number of entries (default 10)\n",
    )
}

fn now_help() -> String {
    String::from(
        "datum now — current state of tracked fields\n\nUSAGE:\n  datum now [-A]\n\n  -A  Show ignored fields too (dim)\n",
    )
}

fn get_help() -> String {
    String::from(
        "datum get — print a single entry\n\nUSAGE:\n  datum get <ID_OR_SLUG> [--pretty]\n",
    )
}

fn delete_help() -> String {
    String::from(
        "datum delete — delete an entry (alias: del)\n\nUSAGE:\n  datum delete <ID_OR_SLUG> [--force] [--dry-run] [--strict]\n",
    )
}

fn undo_help() -> String {
    String::from(
        "datum undo — restore a soft-deleted or reverted entry\n\nUSAGE:\n  datum undo [ID_OR_SLUG] [--to-version N] [--force] [--window 15m]\n",
    )
}

fn history_help() -> String {
    String::from(
        "datum history — modification trail for an entry\n\nUSAGE:\n  datum history <ID_OR_SLUG> [--version N | --diff A B] [--json]\n",
    )
}

fn why_help() -> String {
    String::from(
        "datum why — explain the intent behind an entry\n\nUSAGE:\n  datum why <ID_OR_SLUG> [--full]\n",
    )
}

fn edit_help() -> String {
    String::from(
        "datum edit — open an entry in $EDITOR\n\nUSAGE:\n  datum edit <ID_OR_SLUG>\n",
    )
}

fn update_help(verb: &str) -> String {
    format!(
        "datum {verb} — mutate an existing entry\n\nUSAGE:\n  datum {verb} <ID_OR_SLUG> [VALUE | KEY=VALUE]...\n\nOPTIONS:\n  --strategy <S>        update|merge|rekey\n  --id <TEMPLATE>       New id/slug template\n  --id-delimiter <S>    Delimiter between empty segments\n  -k, --key <KV>        Extra key=value (repeatable)\n  -c, --comment <T>     Comment\n  --dry-run             Show what would change\n  --strict              Abort on first batch error\n"
    )
}

fn retime_help() -> String {
    String::from(
        "datum retime — shift time_start/time_end on entries (alias: rt)\n\nUSAGE:\n  datum retime [SLUG] (--offset DUR | --time NEW) [--field F] [--after BND] [--before BND]\n",
    )
}

fn spec_help() -> String {
    String::from(
        "datum spec — manage field specs\n\nUSAGE:\n  datum spec show <FIELD>\n  datum spec set <FIELD> [--json JSON] [--default-verb V] [--state S]... ...\n  datum spec list\n  datum spec remove <FIELD>\n",
    )
}

fn alias_help() -> String {
    String::from(
        "datum alias — manage aliases\n\nUSAGE:\n  datum alias show <NAME>\n  datum alias set <NAME> <EXPANSION>\n  datum alias list\n  datum alias remove <NAME>\n",
    )
}

fn track_help() -> String {
    String::from(
        "datum track — track a field in the now view\n\nUSAGE:\n  datum track <FIELD> [--color C] [--state S=C]...\n",
    )
}

fn ignore_help() -> String {
    String::from(
        "datum ignore — ignore a field in the now view\n\nUSAGE:\n  datum ignore <FIELD>\n",
    )
}

fn migrate_help() -> String {
    String::from(
        "datum migrate — schema migrations + legacy datum import\n\nUSAGE:\n  datum migrate run\n  datum migrate status\n  datum migrate edit\n  datum migrate import [FILE | --from FILE] [--dry-run]\n",
    )
}

fn backup_help() -> String {
    String::from(
        "datum backup — dump live entries\n\nUSAGE:\n  datum backup <PATH> [--overwrite]\n\nPath ending in .tar.gz / .tgz → tar.gz; otherwise JSONL.\n",
    )
}

fn restore_help() -> String {
    String::from(
        "datum restore — re-import entries from a backup\n\nUSAGE:\n  datum restore <PATH> [--force] [--dry-run] [--json]\n",
    )
}

fn grep_help() -> String {
    String::from(
        "datum grep — regex search over entry JSON content\n\nUSAGE:\n  datum grep <PATTERN>... [-f FIELD]\n",
    )
}

fn check_help() -> String {
    String::from(
        "datum check — run DB consistency checks\n\nUSAGE:\n  datum check [--json]\n",
    )
}

fn count_help() -> String {
    String::from(
        "datum count — count entries (filtered/grouped/since-bounded)\n\nUSAGE:\n  datum count [FIELD] [--group-by KEY] [--since TIME] [--json]\n\nOPTIONS:\n  --group-by <KEY>   state|value|verb|field\n  --since <TIME>     RFC3339 / YYYY-MM-DD / duration offset (1d, 30m, 2h)\n  --json             Machine-readable output\n",
    )
}

fn ids_help() -> String {
    String::from(
        "datum ids — list entry ids (newest-first)\n\nUSAGE:\n  datum ids [FIELD] [-n N] [--oldest-first] [--since TIME] [--slug] [--json]\n\nOPTIONS:\n  -n, --limit <N>    Cap the number of ids returned\n  --oldest-first     Return oldest-first instead of newest-first\n  --since <TIME>     Same grammar as `datum count --since`\n  --slug             Print slugs instead of short hids\n  --json             Machine-readable output\n",
    )
}

fn dayview_help() -> String {
    String::from(
        "datum dayview — render a dayview SVG (or layout JSON)\n\nUSAGE:\n  datum dayview [--date YYYY-MM-DD] [--output PATH] [--json] [--width F] [--height F]\n",
    )
}

fn info_help() -> String {
    String::from(
        "datum info — DB health/metadata\n\nUSAGE:\n  datum info [TARGET] [--all]\n",
    )
}

fn setup_help() -> String {
    String::from(
        "datum setup — initialize or migrate the DB schema\n\nUSAGE:\n  datum setup [--force-recreate]\n",
    )
}

fn config_help() -> String {
    String::from(
        "datum config — show or manage user configuration\n\nUSAGE:\n  datum config show [--json]\n",
    )
}

fn sync_help() -> String {
    String::from(
        "datum sync — push/pull changesets, manage auth & keys\n\nUSAGE:\n  datum sync [--push | --pull] [--dry-run] [--server URL]\n  datum sync login [--server URL]\n  datum sync config set-server <URL> [--global]\n  datum sync config disable\n  datum sync config show\n  datum sync key (init | unlock | rotate-passphrase | show-fingerprint) [--server URL] [--force]\n  datum sync status [--server URL]\n  datum sync install-daemon [--show | --uninstall]\n",
    )
}

fn conflicts_help() -> String {
    String::from(
        "datum conflicts — inspect and resolve sync conflicts\n\nUSAGE:\n  datum conflicts list [--kind K] [--all]\n  datum conflicts resolve <HID> (--keep V | --close-older)\n  datum conflicts auto-resolve [--min-age SECS]\n",
    )
}
