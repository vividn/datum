//! Snapshot tests asserting the structured renderer produces output
//! byte-identical to the previous hand-written help text.
//!
//! The expected strings here are lifted verbatim from the pre-refactor
//! `help.rs`. Any structural change to the renderer that touches a
//! per-command `body_override`, the top-level layout, or the `tail`
//! flag table will trip these.

use super::{command_help, render_json, top_level_help};

// ---------------------------------------------------------------------------
// Top-level — fully structurally rendered.
// ---------------------------------------------------------------------------

const EXPECTED_TOP_LEVEL: &str = "\
datum — personal data tracker

USAGE:
  datum [--db NAME_OR_PATH] <command> [args]

LOGGING:
  add      Register a keyed timeless record (needs an id)
  note     Jot a timeless entry (auto create_time id)
  occur    Record a point-in-time event
  start    Begin a duration-tracked activity
  stop     End a duration-tracked activity (alias: end)
  switch   Transition a field to a new state
  track    Track a field in the now view
  ignore   Ignore a field in the now view
  flag     Mark a field for review (lightweight 'come back to this')

MUTATE:
  edit     Open an entry in the configured editor
  update   Replace keys on an existing entry
  merge    Deep-merge new values into an entry
  rekey    Rename an entry's data keys, in place
  retime   Move an entry's occur_time (alias: rt)
  delete   Delete an entry (alias: del)
  undo     Undo the last command, or one entry's last change
  redo     Redo the last undone change (the inverse of undo)

READ:
  tail     Show recent entries
  head     Show oldest entries
  get      Print entries by id or slug
  grep     Regex search over entry JSON content
  history  Show the full modification trail for an entry
  why      Explain the intent behind an entry

RENDER:
  dayview  Render a multi-day timeline as SVG/PNG/HTML
  nowview  Paint each field's state now as a terminal band (-o writes a file)
  now      Show the current state of tracked fields + terminal nowview band

ADMIN:
  spec         Manage field specs (schemas)
  alias        Manage aliases
  check        Run database consistency checks
  db           DB-lifecycle (setup, purge, backup, restore, migrate, v2import)
  info         Show health/metadata for DB(s)
  config       Show or manage user configuration
  completions  Shell tab completion: print, install or remove the script
  meta         Attach or list protocol-change notes on a field
  kit          Install / export / list field-spec kits (TOML bundles)
  reparse      Replay an entry's stored raw_input through the current parser

VIEWS / SYNC:
  count      Count entries (optionally filtered/grouped/since-bounded)
  stats      Count, total duration, and per-state ratio over a time slice
  ids        List entry ids (newest-first by default)
  fields     List every field: its spec and the states it was logged with
  sync       Sync with the other copies in this database's sync chain
  conflicts  Triage sync conflicts (see `datum check` for integrity findings)
  view       Execute a named SELECT view and render the rows

GLOBAL OPTIONS:
  --db <NAME_OR_PATH>        Override the active DB
  --color <WHEN>             auto|always|never; see `datum help color`
  --show <MODE>              default|none|minimal|standard|format|all|json
  --json                     Shorthand for --show json
  -A, --show-all             Shorthand for --show all
  -o, --format-string <FMT>  %field%-templated output string
  -h, --help                 Show this help
  -V, --version              Show version

Run `datum <command> --help` for per-command flag detail.
With no subcommand, `datum` drops into an interactive REPL (inherits `--db`).
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
  -d, --date <VAL>      Date override (offset, YYYY-MM-DD, sunday, apr29)
  -t, --time <VAL>      Upper-bound time
  -y, --yesterday       Go back 1 day (stackable: -yy=2 days, ...)
  -q, --quick           Go back 5 min (stackable: -qq=10 min, ...)
  -w, --watch [INT]     Live scrollable view; stream entries (poll 1s)
      --table           Legacy columnar view (default: info-line)
      --show-meta       (alias: --show-all-metadata)  Include flag/meta and spec/layout/alias/view/command/index entries (hidden by default)
      --above <ID>      Only the entries listed above this one, timeless ones included

OUTPUT:
      --show <MODE>     default|none|format|json
      --json            Shorthand for --show json
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
datum add — register a keyed timeless record (needs an id)

USAGE:
  datum add <FIELD> -K <KEY> [KEY=VALUE]...
  datum add <FIELD> --id <VALUE>

OPTIONS:
  -K, --slug-key <S>    Required: derive the id from this key (K / K= / K=val)
      --id <TEMPLATE>   ...or set the id directly (e.g. %field%:%uid%)
  common logging options: `datum help logging`

OUTPUT:
      --show <MODE>     default|none|minimal|standard|format|all|json
      --json            Shorthand for --show json
";

#[test]
fn snapshot_add_help() {
    let actual = command_help("add");
    assert_eq!(
        actual, EXPECTED_ADD,
        "datum add --help drifted from snapshot"
    );
}

// da-r17h: `note` is the timeless sibling of `add` — same LOG_FLAGS table,
// distinct header/usage. The header reflects the auto-create_time-id contract.
#[test]
fn snapshot_note_help() {
    // da-r17h: `note` is the timeless sibling of `add` — same LOG_FLAGS
    // table, distinct header/usage. da-ce7n3 folded the shared wall onto
    // `datum help logging`, so what is left on the page IS the difference:
    // `add` keeps the two flags that give it an id, `note` keeps none and
    // says in its USAGE where the bare words go.
    let expected = "\
datum note — jot a timeless entry (auto create_time id)

USAGE:
  datum note <FIELD> [WORD | KEY=VALUE]...
      the WORDs join into extra_data.comment \u{2014} no quotes, no -C

OPTIONS:
  common logging options: `datum help logging`

OUTPUT:
      --show <MODE>     default|none|minimal|standard|format|all|json
      --json            Shorthand for --show json
";
    assert_eq!(
        command_help("note"),
        expected,
        "datum note --help drifted from snapshot"
    );
}

// ---------------------------------------------------------------------------
// Help-vs-parser drift guard (da-uk4t).
//
// The log verbs' `--help` flag table (`LOG_FLAGS`) is authored separately
// from the parser registry (`log_args_registry`). They drifted: working
// flags like `-D/--full-day`, `-R/--remainder`, and the output group never
// appeared in `occur --help`. This test binds the two so they cannot
// silently diverge again — every flag the parser accepts must be documented
// (with a matching value form), and the help table may not document a flag
// the parser rejects.
// ---------------------------------------------------------------------------

#[test]
fn log_help_matches_parser_registry() {
    use crate::help::commands::find_command;
    use crate::help::spec::FlagSpec;
    use crate::input::ValueMode;
    use crate::parse::log_args_registry;
    use std::collections::HashMap;

    // Flags the parser knows about but `--help` intentionally omits.
    // `--dryrun` is a pure spelling-alias of `--dry-run`. `--dur` is a
    // tombstone: da-azqq removed the flag, and the registry keeps a
    // rejecting spec only so the failure names the two surviving
    // spellings instead of a bare "unknown flag" (see
    // `input/duration_args.rs` and `datum-cli/tests/dur_flag_removed.rs`).
    // Documenting a flag that always errors would be worse than hiding it.
    const PARSER_ONLY: &[&str] = &["dryrun", "dur"];

    // Identity key: the short letter (`-y`) when present, else the long name
    // (`dur`). Short-first so a row carrying both keys the same way on
    // either side.
    fn help_key(f: &FlagSpec) -> String {
        match f.short {
            Some(c) => format!("-{c}"),
            None => f.long.to_string(),
        }
    }
    fn parser_key(short: Option<char>, long: Option<&str>) -> String {
        match short {
            Some(c) => format!("-{c}"),
            None => long
                .expect("parser flag has neither short nor long")
                .to_string(),
        }
    }

    let help_flags = find_command("occur").expect("occur command spec").0.flags;
    let help: HashMap<String, &FlagSpec> = help_flags.iter().map(|f| (help_key(f), f)).collect();

    let registry = log_args_registry();
    let mut parser_keys = std::collections::HashSet::new();
    for spec in &registry {
        let key = parser_key(spec.short, spec.long);
        if PARSER_ONLY.contains(&key.as_str()) {
            continue;
        }
        parser_keys.insert(key.clone());

        let f = help.get(&key).unwrap_or_else(|| {
            panic!(
                "parser accepts `{key}` on the log verbs but LOG_FLAGS \
                 (help/commands.rs) omits it — `datum occur --help` hides it"
            )
        });
        let takes_value = spec.value_mode != ValueMode::None;
        assert_eq!(
            f.takes_value, takes_value,
            "`{key}`: help takes_value={} but parser value_mode={:?}",
            f.takes_value, spec.value_mode
        );
        let optional = spec.value_mode == ValueMode::OptionalAttached;
        assert_eq!(
            f.value_optional, optional,
            "`{key}`: help value_optional={} but parser value_mode={:?}",
            f.value_optional, spec.value_mode
        );
    }

    for f in help_flags {
        let key = help_key(f);
        assert!(
            parser_keys.contains(&key),
            "LOG_FLAGS documents `{key}` but the parser registry has no such flag"
        );
    }
}

// The global flags have no registry to compare against: `--db`, `--show`,
// `--json`, `-h` and `-V` are read by hand in `parse_tail` and the `--show`
// sweep, `-A` and `-o` by the output family. So the parser is asked: each
// spelling, bare, in front of a subcommand and at the end of a writer's
// line. Where it is recognised at all, a flag that takes a value fails
// naming the missing value. The alias walk reads `takes_value` from this
// table (da-tckse), so a wrong row there is a wrong walk (da-pkt4r).
#[test]
fn global_flags_take_a_value_as_the_parser_does() {
    use crate::help::GLOBAL_FLAGS;
    use crate::parse::parse_argv;

    for f in GLOBAL_FLAGS {
        let long = format!("--{}", f.long);
        let short = f.short.map(|c| format!("-{c}"));
        for flag in std::iter::once(&long).chain(short.as_ref()) {
            let outcomes: Vec<String> = [vec!["datum", flag], vec!["datum", "occur", "x", flag]]
                .iter()
                .map(|argv| {
                    let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
                    parse_argv(&argv).map_or_else(|e| e.to_string(), |_| "parsed".into())
                })
                .filter(|e| !e.contains("unexpected flag"))
                .collect();
            assert!(
                !outcomes.is_empty(),
                "GLOBAL_FLAGS documents `{flag}` but the parser takes it nowhere"
            );
            let takes_value = outcomes
                .iter()
                .any(|e| e.contains("requires a value") || e.contains("missing value"));
            assert_eq!(
                f.takes_value, takes_value,
                "`{flag}`: GLOBAL_FLAGS says takes_value={} but the parser says {outcomes:?}",
                f.takes_value
            );
        }
    }
}

#[test]
fn snapshot_end_alias_substitutes_verb() {
    // `datum end --help` (alias of `stop`) renders with `end` in both the
    // header and the USAGE line, matching the legacy `log_help("end")`.
    //
    // The USAGE carries the second line da-azqq added: with `--dur` gone,
    // the trailing duration is the only spelling on this verb and the help
    // has to say which side of `-t` it falls. da-zwct settled that side:
    // `-t` is the block's END and the duration is the span that preceded
    // it (da-vq0z fixed the words).
    let expected = "\
datum end — end a duration-tracked activity (alias: end)

USAGE:
  datum end <FIELD> [VALUE | KEY=VALUE]... [DUR]
    DUR (or dur=DUR) runs BACK from the entry's time:
    `datum end work 12m -t 14:12` is a break from 14:00 to 14:12

OPTIONS:
      --if-changed      switch/start/end: write nothing if already in this state
  -L, --last-state <V>  The state being left; `datum check` flags a mismatch
  common logging options: `datum help logging`

OUTPUT:
      --show <MODE>     default|none|minimal|standard|format|all|json
      --json            Shorthand for --show json
";
    assert_eq!(command_help("end"), expected);
}

#[test]
fn snapshot_sync_help_starts_with_header() {
    let s = command_help("sync");
    assert!(
        s.starts_with("datum sync — sync with the other copies in this database's sync chain\n")
    );
}

// ---------------------------------------------------------------------------
// Alias dispatch — the legacy command_help routed `end` → log_help("end")
// which echoed the verb back into the header. Preserve that.
// ---------------------------------------------------------------------------

#[test]
fn alias_end_renders_with_input_name() {
    let s = command_help("end");
    assert!(
        s.starts_with("datum end — end a duration-tracked activity (alias: end)\n"),
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
    assert!(s.starts_with("datum retime — move an entry's occur_time (alias: rt)\n"));
}

// ---------------------------------------------------------------------------
// Unknown commands fall back to top-level, saying they are unknown
// (da-3odno): a miss is told apart from `datum help` itself.
// ---------------------------------------------------------------------------

#[test]
fn unknown_command_falls_back_to_top_level() {
    let s = command_help("not-a-real-command");
    assert_eq!(
        s,
        format!(
            "datum help: no command, topic or alias named 'not-a-real-command'\n\n{EXPECTED_TOP_LEVEL}"
        )
    );
}

// ---------------------------------------------------------------------------
// Markdown render — sanity check, not byte-identity.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Multi-entry alias documentation — surfaces in --help, markdown, and roff.
// ---------------------------------------------------------------------------

#[test]
fn alias_help_documents_comma_trigger() {
    let s = command_help("alias");
    assert!(
        s.contains("ATOMIC MULTI-ENTRY ALIASES"),
        "alias --help should document multi-entry aliases"
    );
    assert!(
        s.contains("whitespace-bounded `,`"),
        "alias --help should explain the whitespace-bounded comma trigger"
    );
}

#[test]
fn alias_help_documents_k_trichotomy() {
    let s = command_help("alias");
    assert!(s.contains("-k key       required"));
    assert!(s.contains("-k key=      optional"));
    assert!(s.contains("-k key=val   defaulted"));
}

#[test]
fn alias_help_documents_f_trichotomy() {
    let s = command_help("alias");
    assert!(s.contains("FIELD SLOT FORMS (-F)"));
    assert!(s.contains("-F key         required"));
    assert!(s.contains("-F key=        optional"));
    assert!(s.contains("-F key=val     defaulted"));
    assert!(s.contains("appended to the field with `__`"));
}

#[test]
fn alias_help_documents_double_dash_semantics() {
    let s = command_help("alias");
    assert!(
        s.contains("`--` SEMANTICS"),
        "alias --help should have a `--` semantics section"
    );
    assert!(s.contains("implicit at the end of every"));
    assert!(s.contains("does not propagate past the alias boundary"));
}

#[test]
fn alias_help_documents_last_after_multi() {
    let s = command_help("alias");
    assert!(s.contains("`_LAST`"));
    assert!(s.contains("every row of an atomic group"));
}

#[test]
fn log_flag_summaries_document_k_trichotomy() {
    // The three -k / -K forms are on `datum help logging` now, which is
    // where every logging verb's page sends the reader (da-ce7n3).
    let s = command_help("logging");
    assert!(
        s.contains("K req, K= opt, K=val default"),
        "the logging topic should expose the -k trichotomy in the flag summary"
    );
    assert!(
        s.contains("Slug-contributing id-key (same K / K= / K=val forms)"),
        "the logging topic should expose the -K trichotomy in the flag summary"
    );
}

// ---------------------------------------------------------------------------
// The shared logging wall, written out once (da-ce7n3).
//
// The six logging verbs were 38-40 lines each, of which about thirty were
// byte-identical — so the lines that told them apart were buried in the
// middle of the wall. `man git` points at its shared option sets instead
// of reprinting them; these tests hold the six to that shape.
// ---------------------------------------------------------------------------

/// Every logging verb's page is short, carries the pointer, and does NOT
/// reprint the wall. `-R/--remainder` stands in for the wall here: it is a
/// shared flag no verb makes particular, so a page that shows it has
/// un-folded.
#[test]
fn logging_pages_point_at_the_topic_instead_of_reprinting_the_wall() {
    for verb in ["add", "note", "occur", "start", "stop", "switch"] {
        let page = command_help(verb);
        assert!(
            page.contains("  common logging options: `datum help logging`\n"),
            "`datum help {verb}` should send the reader to the topic page:\n{page}"
        );
        assert!(
            !page.contains("--remainder <KEY>"),
            "`datum help {verb}` still reprints the shared wall:\n{page}"
        );
        let lines = page.lines().count();
        assert!(
            lines <= 16,
            "`datum help {verb}` is {lines} lines; the wall is meant to be gone"
        );
    }
}

/// The wall itself is on the topic page, whole — every flag of LOG_FLAGS,
/// and the one exception a shared set always has.
#[test]
fn the_logging_topic_carries_the_whole_wall() {
    let page = command_help("logging");
    assert!(page.starts_with("datum help logging — "));
    let occur = super::commands::find_command("occur").unwrap().0;
    for flag in occur.flags {
        assert!(
            page.contains(flag.summary),
            "`datum help logging` is missing `{}`:\n{page}",
            flag.summary
        );
    }
    assert!(
        page.contains("`note` is the exception"),
        "the topic has to name the one verb the shared set is not true for:\n{page}"
    );
}

/// The fold is a RENDERING choice: the machine readers still get the whole
/// flag set, so the web's command bar does not lose `-t` on `occur`.
#[test]
fn folding_the_wall_does_not_shrink_the_grammar() {
    let table: serde_json::Value = serde_json::from_str(&render_json()).unwrap();
    let occur = table
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "occur")
        .unwrap();
    let longs: Vec<&str> = occur["flags"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["long"].as_str())
        .collect();
    for expected in ["remainder", "comment-remainder", "time", "conflict"] {
        assert!(
            longs.contains(&expected),
            "`datum help --json` lost `--{expected}` from occur when the page folded"
        );
    }
}

/// A word that is neither a command nor a topic still falls back to the
/// top-level page — the topic table is a new arm, not a new failure mode.
#[test]
fn a_topic_name_is_not_a_command_name() {
    assert!(super::commands::find_command("logging").is_none());
    assert!(super::commands::find_topic("logging").is_some());
    assert!(super::commands::find_topic("occur").is_none());
}

#[test]
fn markdown_alias_includes_multi_entry_description() {
    let md = super::render::render_markdown(Some("alias"));
    assert!(md.contains("multi-of-multi flattens"));
    assert!(md.contains("`-k key`"));
    assert!(md.contains("`_LAST`"));
}

#[test]
fn roff_alias_includes_multi_entry_description() {
    let r = super::render::render_roff(Some("alias"));
    assert!(r.contains(".SH DESCRIPTION\n"));
    assert!(r.contains("multi\\-of\\-multi flattens"));
    // Hyphens in `-k`/`-K` must render as escaped roff hyphens.
    assert!(r.contains("\\-k key"));
}

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

// ---------------------------------------------------------------------------
// Commands migrated off `body_override` to generated OPTIONS (da-btkq).
// These snapshots are the regression guard the migration buys: a flag
// added to one of these FlagSpec tables now forces a diff here.
// ---------------------------------------------------------------------------

const EXPECTED_HEAD: &str = "\
datum head — show oldest entries

USAGE:
  datum head [FIELD]

OPTIONS:
  -n, --limit <N>   Number of entries (default 10)

OUTPUT:
      --show <MODE>  default|none|format|json
      --json         Shorthand for --show json
";

#[test]
fn snapshot_head_help() {
    assert_eq!(command_help("head"), EXPECTED_HEAD);
}

const EXPECTED_UPDATE: &str = "\
datum update — replace keys on an existing entry

USAGE:
  datum update <ID_OR_SLUG[,ID_OR_SLUG]...> [VALUE | KEY=VALUE]...

ARGS:
  <ID_OR_SLUG>            Several ids are joined with commas (`a1b2,c3d4`): a word after a space is a VALUE

OPTIONS:
      --strategy <S>      update|merge|rekey, or a combine: useOld|useNew|preferOld|preferNew|intersection|removeConflicting|xor|append|prepend|mergeSort|appendSort
      --id <TEMPLATE>     New id/slug template
      --id-delimiter <S>  Delimiter between empty segments
  -k, --key <KV>          Extra key=value (repeatable)
  -c, --comment <T>       Append a comment to data.comment (repeatable)
      --dry-run           Show what would change
      --all-or-nothing    Change nothing if any batched id fails

OUTPUT:
      --show <MODE>       default|none|minimal|standard|format|all|json
      --json              Shorthand for --show json
";

#[test]
fn snapshot_update_help() {
    assert_eq!(command_help("update"), EXPECTED_UPDATE);
}

#[test]
fn merge_and_rekey_share_update_options() {
    // All three share UPDATE_FLAGS; only the header + USAGE differ.
    let options = "OPTIONS:\n      --strategy <S>      update|merge|rekey, or a combine: useOld|useNew|preferOld|preferNew|intersection|removeConflicting|xor|append|prepend|mergeSort|appendSort\n";
    assert!(
        command_help("merge").contains(options),
        "merge should render generated OPTIONS"
    );
    assert!(
        command_help("rekey").contains(options),
        "rekey should render generated OPTIONS"
    );
}

#[test]
fn rekey_help_says_it_renames_data_keys_in_place() {
    let help = command_help("rekey");
    assert!(
        help.starts_with(
            "datum rekey — rename an entry's data keys, in place\n\nUSAGE:\n  datum rekey <ID_OR_SLUG[,ID_OR_SLUG]...> <OLD=NEW>...\n\nARGS:\n  <OLD=NEW>               Move OLD's value to the key NEW. The id template is read again, so the slug follows: a template key renamed away empties its part of the id\n\nOPTIONS:\n"
        ),
        "{help}"
    );
}

const EXPECTED_COUNT: &str = "\
datum count — count entries (optionally filtered/grouped/since-bounded)

USAGE:
  datum count [FIELD] [--group-by KEY] [--since TIME]

OPTIONS:
      --group-by <KEY>  state|verb|field|client
      --since <TIME>    Duration ago (1d, 30m, 1:30), YYYY-MM-DD, or any -t time

OUTPUT:
      --show <MODE>     default|none|json
      --json            Shorthand for --show json
";

#[test]
fn snapshot_count_help() {
    assert_eq!(command_help("count"), EXPECTED_COUNT);
}

const EXPECTED_IDS: &str = "\
datum ids — list entry ids (newest-first by default)

USAGE:
  datum ids [FIELD] [-n N] [--oldest-first] [--since TIME] [--slug]

OPTIONS:
  -n, --limit <N>     Cap the number of ids returned
      --oldest-first  Return oldest-first instead of newest-first
      --since <TIME>  Same grammar as `datum count --since`
      --slug          Print slugs instead of short hids

OUTPUT:
      --show <MODE>   default|none|json
      --json          Shorthand for --show json
";

#[test]
fn snapshot_ids_help() {
    assert_eq!(command_help("ids"), EXPECTED_IDS);
}

/// da-5flt: the grammar the web reads says nowview takes `-t` and the other
/// time flags, so its commands bar offers nowview under a time pill.
#[test]
fn json_nowview_takes_the_time_flags() {
    let table: serde_json::Value = serde_json::from_str(&render_json()).unwrap();
    let nowview = table
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "nowview")
        .unwrap();
    let shorts: Vec<_> = nowview["flags"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["short"].as_str())
        .collect();
    assert_eq!(shorts, ["t", "d", "q", "y", "z"]);
}

/// da-78evn: nowview's pivot is documented where its flags are.
#[test]
fn nowview_help_documents_the_orientation() {
    let help = command_help("nowview");
    assert!(help.contains("--orientation <O>"), "{help}");
    assert!(help.contains("--vertical"), "{help}");
}

/// da-803mk: `help --json` names `db`'s subcommands outright, so the web's
/// commands widget drills into them instead of parsing usage lines — and
/// says `db` alone does not run while `sync` alone does.
#[test]
fn help_json_lists_db_subcommands() {
    let table: serde_json::Value = serde_json::from_str(&render_json()).unwrap();
    let entry = |name: &str| {
        table
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap()
            .clone()
    };
    let db = entry("db");
    let names: Vec<_> = db["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["setup", "purge", "backup", "restore", "migrate", "v2import"]
    );
    assert_eq!(db["subcommands"][2]["usage"].as_array().unwrap().len(), 4);
    assert_eq!(db["bare"], false);
    assert_eq!(entry("sync")["bare"], true);
    assert_eq!(entry("conflicts")["bare"], true);
    assert_eq!(entry("occur")["subcommands"], serde_json::json!([]));
}

/// A flag in `help --json` says what it does and whether a value follows
/// it, so the web's flag list explains `-t` and opens a keyboard for it
/// without a copy of either (da-st2xl). `value_optional` says a bare flag
/// takes no word after it, which the web's field slot must know to place a
/// field past one (da-e9e5g); `-L` takes its word, as v2's did (da-n86g3).
#[test]
fn help_json_flags_carry_help_and_takes_value() {
    let table: serde_json::Value = serde_json::from_str(&render_json()).unwrap();
    let occur = table
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "occur")
        .unwrap();
    let flag = |short: &str| {
        occur["flags"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["short"] == short)
            .unwrap()
            .clone()
    };
    assert_eq!(flag("t")["takes_value"], true);
    assert_eq!(
        flag("t")["help"],
        "Time override (offset, HH:MM, 9:15am, a date, or both)"
    );
    assert_eq!(flag("q")["takes_value"], false);
    assert_eq!(flag("t")["value_optional"], false);
    assert_eq!(flag("L")["takes_value"], true);
    assert_eq!(flag("L")["value_optional"], false);
    assert_eq!(
        flag("q")["help"],
        "Go back 5 min (stackable: -qq=10 min, ...)"
    );
}

/// The listing is the parser's: every subcommand it names is one the parser
/// dispatches, its usage lines are its parent's, and a command whose parser
/// names its subcommands when none is given names exactly these.
#[test]
fn help_json_subcommands_are_the_parsers() {
    let parse = |argv: &[&str]| {
        let argv: Vec<String> = ["datum"]
            .iter()
            .chain(argv)
            .map(|s| s.to_string())
            .collect();
        crate::parse_argv(&argv).err().map(|e| e.to_string())
    };
    let mut named = 0;
    for c in super::commands::COMMANDS {
        let names: Vec<_> = c.subcommands.iter().map(|s| s.name).collect();
        for s in c.subcommands {
            for line in s.usage {
                assert!(
                    c.usage.contains(line),
                    "`{line}` is not a usage line of {}",
                    c.name
                );
            }
            let err = parse(&[c.name, s.name]).unwrap_or_default();
            assert!(
                !err.contains("unknown") && !err.contains("unexpected positional"),
                "`datum {} {}` is not a subcommand the parser knows: {err}",
                c.name,
                s.name
            );
        }
        let err = parse(&[c.name]).unwrap_or_default();
        if let Some((_, set)) = err.split_once(&format!("missing {} subcommand (", c.name)) {
            let set: Vec<_> = set.trim_end_matches(')').split('|').collect();
            assert_eq!(names, set, "{}'s subcommands", c.name);
            named += 1;
        }
    }
    assert_eq!(
        named, 6,
        "db, spec, alias, kit, config and completions name their set"
    );
}

// ---------------------------------------------------------------------------
// `datum help time`, held to the parser (da-6yqu9).
// ---------------------------------------------------------------------------

/// The page's flag table is the time parser's flag set, in both directions:
/// a flag the parser gains without a line here fails, and so does a line
/// for a flag it no longer has.
#[test]
fn time_page_flags_are_the_parsers() {
    let topic = super::commands::find_topic("time").expect("`datum help time`");
    let mut page: Vec<_> = topic.flags.iter().map(|f| (f.short, f.long)).collect();
    let mut parser: Vec<_> = crate::input::time_args::FLAG_SPECS
        .iter()
        .map(|f| (f.short, f.long.unwrap_or("")))
        .collect();
    page.sort();
    parser.sort();
    assert_eq!(page, parser);
}

/// The time args of `datum occur ping <argv>`, built against a fixed now.
fn build_time_args(argv: &str) -> crate::Result<crate::input::time_args::TimeFromArgs> {
    let crate::Cli {
        command: crate::Command::Occur(args),
        ..
    } = crate::parse_argv_string(&format!("occur ping {argv}"))?
    else {
        panic!("`occur ping {argv}` is not an occur");
    };
    // A word the time flags left over (an unquoted `-t 9:15 am`) would
    // parse as the entry's state, not as time.
    assert!(args.rest.is_empty(), "`{argv}` leaves {:?} over", args.rest);
    let now = crate::DatumTime::from_utc_and_tz(
        "2020-05-10T15:25:30Z".parse().unwrap(),
        "America/Chicago",
    )?;
    args.time.build(&now)
}

/// Every example on the page goes through the real parser, so the page
/// cannot promise a form the parser does not take. A few non-forms are
/// held to failing, so the check is not vacuous.
#[test]
fn every_time_page_example_parses() {
    let topic = super::commands::find_topic("time").expect("`datum help time`");
    let mut n = 0;
    for group in topic.forms {
        for f in group.forms {
            let parsed = if group.durations {
                crate::time::parse_duration(f.example).map(drop)
            } else {
                build_time_args(f.example).map(drop)
            };
            assert!(
                parsed.is_ok(),
                "`{}` on `datum help time`: {parsed:?}",
                f.example
            );
            n += 1;
        }
    }
    assert!(n > 40, "only {n} examples on the page");
    for bad in [
        "-t 9/28",
        "-d 9/28",
        "-t '3 days ago at 10'",
        "-t 25:00",
        "-y -t -6",
        "-d sunday -q",
    ] {
        assert!(build_time_args(bad).is_err(), "`{bad}` parsed");
    }
    assert!(crate::time::parse_duration("an hour").is_err());
}

/// da-2bb7z: a subcommand's `--help` is its own page — its usage lines and
/// what the parent's page says of it — where it was the parent's whole page,
/// byte for byte; and each purge names the other.
#[test]
fn subcommand_help_is_its_own_page() {
    let parse = |line: &str| {
        let argv: Vec<String> = std::iter::once("datum")
            .chain(line.split(' '))
            .map(String::from)
            .collect();
        let e = crate::parse_argv(&argv).unwrap_err();
        super::special_output(&e, "").unwrap()
    };
    let purge = parse("db purge --leave --help");
    assert_ne!(purge, command_help("db"));
    assert_eq!(purge, parse("help db purge"));
    assert!(purge.starts_with("datum db purge\n\nUSAGE:\n"), "{purge}");
    assert!(
        purge.contains("Purge leaves nothing of this copy"),
        "{purge}"
    );
    assert!(purge.contains("`datum sync chain purge --data`"), "{purge}");
    assert!(!purge.contains("Backup dumps"), "{purge}");

    let chain = parse("sync chain purge --help");
    assert_eq!(chain, parse("help sync chain purge"));
    assert!(chain.contains("  purge       Every copy drops"), "{chain}");
    assert!(
        chain.contains("`datum db purge --keep-sync-chain`"),
        "{chain}"
    );
    assert!(!chain.contains("  revert"), "{chain}");

    // `status` is the top-level row; the daemon's is under `daemon`.
    assert!(parse("sync status -h").contains("Per member"));
    assert!(parse("sync daemon status -h").contains("Whether it is installed"));
    // A word that names no subcommand stops the path.
    assert_eq!(parse("occur coffee --help"), command_help("occur"));
    assert_eq!(parse("db nope --help"), command_help("db"));
    // The renderer reads a page name as the parser does.
    assert_eq!(command_help("db nope"), command_help("db"));
    assert_eq!(command_help("db purge --leave"), purge);
}

/// Every subcommand's page carries its usage lines and something its
/// parent says of it, or at least the way to the parent's page.
#[test]
fn every_subcommand_page_has_its_usage() {
    for cmd in super::COMMANDS {
        for sub in cmd.subcommands {
            let page = command_help(&super::page_name(cmd.name, &[sub.name]));
            assert!(
                page.starts_with(&format!("datum {} {}\n", cmd.name, sub.name)),
                "{page}"
            );
            for u in sub.usage {
                assert!(page.contains(u), "{} {}: {u}\n{page}", cmd.name, sub.name);
            }
            assert!(
                page.ends_with(&format!("datum help {}\n", cmd.name)),
                "{page}"
            );
        }
    }
}
