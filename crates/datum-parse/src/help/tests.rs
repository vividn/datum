//! Snapshot tests asserting the structured renderer produces output
//! byte-identical to the previous hand-written help text.
//!
//! The expected strings here are lifted verbatim from the pre-refactor
//! `help.rs`. Any structural change to the renderer that touches a
//! per-command `body_override`, the top-level layout, or the `tail`
//! flag table will trip these.

use super::{command_help, top_level_help};

// ---------------------------------------------------------------------------
// Top-level — fully structurally rendered.
// ---------------------------------------------------------------------------

const EXPECTED_TOP_LEVEL: &str = "\
datum — personal data tracker

USAGE:
  datum [--db NAME_OR_PATH] <command> [args]

LOGGING:
  add      Add a new entry (shape inferred from kv keys)
  occur    Record a point-in-time event
  start    Begin a duration-tracked activity
  stop     End a duration-tracked activity (alias: end)
  switch   Transition a field to a new state

READ:
  tail     Show recent entries
  head     Show oldest entries
  now      Show current state of tracked fields
  get      Print a single entry by id or slug
  grep     Regex search over entry JSON content
  dayview  Render a dayview SVG (or dump layout JSON)
  nowview  Render a nowview — terminal block art by default, SVG with -o
  history  Show the full modification trail for an entry
  why      Explain the intent behind an entry

MUTATE:
  edit     Open an entry in $EDITOR
  update   Replace keys on an existing entry
  merge    Deep-merge new values into an entry
  rekey    Copy an entry to a new id (old row stays)
  retime   Shift time_start/time_end on entries (alias: rt)
  delete   Delete an entry (alias: del)
  undo     Restore the most recently soft-deleted entry

ADMIN:
  spec     Manage field specs (schemas)
  alias    Manage aliases
  track    Track a field in the now view
  ignore   Ignore a field in the now view
  migrate  Schema migrations + legacy datum import
  backup   Dump live entries to JSONL / tar.gz
  restore  Re-import entries from a backup
  check    Run database consistency checks
  setup    Initialize or migrate the DB schema
  info     Show health/metadata for DB(s)
  config   Show or manage user configuration

VIEWS / SYNC:
  count      Count entries (optionally filtered/grouped/since-bounded)
  ids        List entry ids (newest-first by default)
  sync       Push/pull changesets, manage auth & keys
  conflicts  Inspect and resolve sync conflicts

GLOBAL OPTIONS:
  --db <NAME_OR_PATH>   Override the active DB
  --show <MODE>         default|none|minimal|standard|format|all
  -A, --show-all        Shorthand for --show all
  -o, --format-string   %field%-templated output string
  -h, --help            Show this help
  -V, --version         Show version

Run `datum <command> --help` for per-command flag detail.
";

#[test]
fn snapshot_top_level_help() {
    let actual = top_level_help();
    assert_eq!(
        actual, EXPECTED_TOP_LEVEL,
        "top-level help drifted from snapshot"
    );
}

// ---------------------------------------------------------------------------
// `tail` — structurally rendered (no body_override). The bead names
// this as the canonical structural-render snapshot.
// ---------------------------------------------------------------------------

const EXPECTED_TAIL: &str = "\
datum tail — show recent entries

USAGE:
  datum tail [FIELD]

OPTIONS:
  -n <N>                Number of entries (default 10)
  -m, --metric <M>      hybrid|occur|create|modify
  -r, --reverse         Invert ordering
      --column <NAME>   Extra extra_data column (repeatable)
  -d, --date <VAL>      Date override
  -t, --time <VAL>      Upper-bound time
  -y / -q               Day / 5-min offsets (stackable)
  -w, --watch [INT]     Stream new entries (default poll 1s)
      --table           Legacy columnar view (default: info-line)
";

#[test]
fn snapshot_tail_help() {
    let actual = command_help("tail");
    assert_eq!(actual, EXPECTED_TAIL, "tail --help drifted from snapshot");
}

// ---------------------------------------------------------------------------
// Spot-checks for the bead's "look the same as before" list.
// ---------------------------------------------------------------------------

// Full snapshot of `datum add --help` — this is the structurally-rendered
// log verb, exercising LOG_FLAGS through the table renderer. The expected
// string is byte-identical to the legacy `log_help("add")` output.
const EXPECTED_ADD: &str = "\
datum add — log an entry

USAGE:
  datum add <FIELD> [VALUE | KEY=VALUE]...

OPTIONS:
  -y                    Go back 1 day (stackable: -yy=2 days, ...)
  -q                    Go back 5 min (stackable: -qq=10 min, ...)
  -d, --date <VAL>      Date override (offset or YYYY-MM-DD)
  -t, --time <VAL>      Time override (offset or HH:MM or 9:15am)
      --dur <VAL>       Duration (30m, 1:30, 2h, PT30M)
  -z, --timezone <TZ>   Timezone (IANA or UTC offset)
  -u, --unit <UNIT>     Unit for the primary value
  -k, --key <KEY_SPEC>  Data-only id-key slot (repeatable)
  -K, --slug-key <S>    Slug-contributing id-key (repeatable)
  -c, --comment <T>     Comment
      --id <TEMPLATE>   Slug template (e.g. %field%:%uid%)
      --id-delimiter <S>  Delimiter to collapse empty segments
  -L, --last-state[=<V>]  Assert prior state (bare: close-most-recent on stop)
  -m, --moment          Point-in-time state change
      --conflict <S>    update|merge|rekey|ignore
      --update          Shorthand for --conflict update
      --merge           Shorthand for --conflict merge
  -M, --no-metadata     Zero created_at/modified_at, clear device_id
  -T, --no-timestamp    Route through timeless `add`
      --alias <NAME>    On success, save current invocation as alias <NAME>
      --alias-overwrite Allow --alias to replace an existing alias
";

#[test]
fn snapshot_add_help() {
    let actual = command_help("add");
    assert_eq!(
        actual, EXPECTED_ADD,
        "datum add --help drifted from snapshot"
    );
}

#[test]
fn snapshot_end_alias_substitutes_verb() {
    // `datum end --help` (alias of `stop`) should render with `end` in
    // both the header and the USAGE line, matching the legacy behaviour
    // of `log_help("end")`.
    let s = command_help("end");
    let expected = EXPECTED_ADD
        .replace("datum add — log an entry", "datum end — log an entry")
        .replace(
            "datum add <FIELD> [VALUE | KEY=VALUE]...",
            "datum end <FIELD> [VALUE | KEY=VALUE]...",
        );
    assert_eq!(s, expected);
}

#[test]
fn snapshot_sync_help_starts_with_header() {
    let s = command_help("sync");
    assert!(s.starts_with("datum sync — push/pull changesets, manage auth & keys\n"));
    assert!(s.contains("datum sync login [--server URL]\n"));
}

// ---------------------------------------------------------------------------
// Alias dispatch — the legacy command_help routed `end` → log_help("end")
// which echoed the verb back into the header. Preserve that.
// ---------------------------------------------------------------------------

#[test]
fn alias_end_renders_with_input_name() {
    let s = command_help("end");
    assert!(
        s.starts_with("datum end — log an entry\n"),
        "expected `datum end — …` header, got: {}",
        &s[..s.find('\n').unwrap_or(s.len())]
    );
}

#[test]
fn alias_del_renders_with_canonical_name() {
    // `delete_help` historically ignored the alias and always emitted
    // `datum delete — …`. Preserve that behaviour.
    let s = command_help("del");
    assert!(
        s.starts_with("datum delete — delete an entry (alias: del)\n"),
        "expected canonical `datum delete — …` header, got: {}",
        &s[..s.find('\n').unwrap_or(s.len())]
    );
}

#[test]
fn alias_rt_renders_with_canonical_name() {
    let s = command_help("rt");
    assert!(s.starts_with("datum retime — shift time_start/time_end on entries (alias: rt)\n"));
}

// ---------------------------------------------------------------------------
// Unknown commands fall back to top-level (legacy behaviour).
// ---------------------------------------------------------------------------

#[test]
fn unknown_command_falls_back_to_top_level() {
    let s = command_help("not-a-real-command");
    assert_eq!(s, EXPECTED_TOP_LEVEL);
}

// ---------------------------------------------------------------------------
// Markdown render — sanity check, not byte-identity.
// ---------------------------------------------------------------------------

#[test]
fn markdown_top_level_has_headings() {
    let md = super::render::render_markdown(None);
    assert!(md.contains("# datum"));
    assert!(md.contains("## LOGGING"));
    assert!(md.contains("## VIEWS / SYNC"));
    assert!(md.contains("## Global Options"));
}

#[test]
fn markdown_command_has_usage() {
    let md = super::render::render_markdown(Some("tail"));
    assert!(md.contains("# `datum tail`"));
    assert!(md.contains("## Usage"));
    assert!(md.contains("datum tail [FIELD]"));
}

// ---------------------------------------------------------------------------
// Roff (man-page) renderer.
// ---------------------------------------------------------------------------

#[test]
fn roff_top_level_has_expected_sections() {
    let r = super::render::render_roff(None);
    assert!(r.starts_with(".TH \"DATUM\" \"1\""));
    assert!(r.contains(".SH NAME\ndatum \\- personal data tracker\n"));
    assert!(r.contains(".SH SYNOPSIS\n"));
    assert!(r.contains(".SH DESCRIPTION\n"));
    assert!(r.contains(".SH COMMANDS\n"));
    assert!(r.contains(".SS LOGGING\n"));
    assert!(r.contains(".SS VIEWS / SYNC\n"));
    assert!(r.contains(".SH GLOBAL OPTIONS\n"));
    assert!(r.contains(".SH SEE ALSO\n"));
    // Cross-reference uses the per-command form `datum-tail(1)`.
    assert!(r.contains(".BR datum\\-tail (1)"));
}

#[test]
fn roff_per_command_has_expected_sections() {
    let r = super::render::render_roff(Some("tail"));
    assert!(r.starts_with(".TH \"DATUM-TAIL\" \"1\""));
    assert!(r.contains(".SH NAME\ndatum\\-tail \\- show recent entries\n"));
    assert!(r.contains(".SH SYNOPSIS\n"));
    assert!(r.contains("datum tail [FIELD]"));
    assert!(r.contains(".SH OPTIONS\n"));
    assert!(r.contains(".SH SEE ALSO\n.BR datum (1)\n"));
    // Flag entries render as .TP / .B lines.
    assert!(r.contains(".TP\n.B \\-n <N>\n"));
    assert!(r.contains("Number of entries (default 10)"));
}

#[test]
fn roff_unknown_command_falls_back_to_top_level() {
    let r = super::render::render_roff(Some("not-a-real-command"));
    assert!(r.starts_with(".TH \"DATUM\" \"1\""));
}

#[test]
fn roff_escapes_hyphens_and_backslashes() {
    // The synopsis line for the top-level page contains `--db`, which
    // must render as `\-\-db` so that `man -l` produces a copy-pasteable
    // ASCII hyphen.
    let r = super::render::render_roff(None);
    assert!(r.contains("\\-\\-db"));
    // No raw double-hyphen sequences leaked through.
    assert!(!r.contains("--db"));
}
