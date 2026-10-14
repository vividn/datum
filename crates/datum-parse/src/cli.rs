//! CLI intent — the parsed shape of a `datum` invocation.
//!
//! [`Cli`] is the top-level `CommandIntent`: global flags plus a
//! [`Command`] variant carrying the subcommand's typed args. The bespoke
//! parser in [`crate::parse`] produces one of these; `datum-commands`
//! consumes it via `dispatch`.
//!
//! All types here are pure data — no derives, no I/O. Convenience
//! methods that reference `datum-commands` render types (e.g.
//! `OutputArgs`, `ConflictStrategy`) live in `datum-commands::args`
//! alongside the re-exports.

use crate::input::{DurationArgs, Matcher, OutputArgs, Show, TimeArgs};

#[derive(Debug)]
pub struct Cli {
    pub db: Option<String>,
    /// `--color always|never` (`Some(true)` / `Some(false)`); `auto`, or no
    /// flag, leaves it to `NO_COLOR`, `CLICOLOR_FORCE` / `FORCE_COLOR` and
    /// the tty.
    pub color: Option<bool>,
    pub command: Command,
}

#[derive(Debug)]
pub enum Command {
    /// No subcommand given — drop into the interactive REPL. Carries no
    /// args because every global flag (`--db`, etc.) already lives on
    /// the outer [`Cli`]. Reached when argv has only global flags (or is
    /// empty after `argv[0]`).
    Repl,
    Occur(LogArgs),
    Start(LogArgs),
    Stop(LogArgs),
    Switch(LogArgs),
    Add(LogArgs),
    /// `datum note <field>` — timeless, auto create_time id (da-r17h).
    Note(LogArgs),

    Tail(TailArgs),
    /// Show oldest entries — powered by Tail with `oldest_first = true`.
    Head(TailArgs),
    Now(NowArgs),

    Get(GetArgs),
    Delete(DeleteArgs),
    Undo(UndoArgs),
    Redo(RedoArgs),

    History(HistoryArgs),
    Why(WhyArgs),

    Edit(EditArgs),

    Update(UpdateArgs),
    Merge(UpdateArgs),
    Rekey(UpdateArgs),

    Retime(RetimeArgs),

    Spec(SpecCommand),
    Alias(AliasCommand),
    Kit(KitCommand),
    /// `datum conflicts` — the entries whose ops leave a key contested:
    /// list them, resolve one (da-05rkv, da-07594).
    Conflicts(ConflictsCommand),
    /// `datum debug <what>` — test situations made in the current database
    /// (da-4dvk7). Kept off the command list; `datum help debug` has it.
    Debug(DebugCommand),

    Track(TrackArgs),
    Ignore(IgnoreArgs),

    /// `datum meta <field> <text>` — record an out-of-band protocol
    /// note about how a field is being classified or interpreted.
    /// `datum meta show <field>` lists every note for a field in
    /// `create_time` order. Notes are stored as timeless entries on
    /// the field with `extra_metadata.meta = true`.
    Meta(MetaCommand),

    /// `datum flag <field> [comment] [-t/-d/-y/-q]` — drop a manual
    /// review-later marker on a field at a time point. Stored as a
    /// regular entry carrying `extra_metadata.flag = true` so it
    /// surfaces in `datum check`'s pending-review sweep while
    /// staying out of tail / dayview / now (per da-dnnn Option A).
    Flag(FlagArgs),

    Grep(GrepArgs),
    Check(CheckArgs),

    Count(CountArgs),
    Stats(StatsArgs),
    Ids(IdsArgs),
    /// `datum fields` — every field the database knows, with its spec and
    /// the states it has been logged with.
    Fields(FieldsArgs),

    Dayview(DayviewArgs),
    Nowview(NowviewArgs),
    /// `datum view <name>` — execute a named SELECT installed via a kit
    /// (or declared by the user) and render the rows. Slice 4 (da-d4t.4).
    View(ViewArgs),

    Info(InfoArgs),
    /// `datum db <subcommand>` — DB-lifecycle group. Houses every database
    /// administration verb: `setup`, `purge`, `backup`, `restore`,
    /// `migrate`, `import` (da-33g consolidated the standalone verbs).
    Db(DbCommand),

    Config(ConfigCommand),

    /// `datum sync <…>` — Easy Sync: pairing and syncing over iroh
    /// (da-rugg8), and the core's pipe (da-k329q).
    Sync(SyncCommand),

    /// `dm reparse` — replay each entry's stored `raw_input` through the
    /// current parser, reporting (or fixing with `--fix`) drift between
    /// what the parser produces today vs. the persisted columns.
    Reparse(ReparseArgs),

    /// `datum completions <shell | install | uninstall>` — shell tab
    /// completion (da-bu09u).
    Completions(CompletionsCommand),
}

// ─── Completions ──────────────────────────────────────────────────────────

/// A shell `datum completions` has a script for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
}

impl Shell {
    pub const ALL: [Shell; 3] = [Shell::Zsh, Shell::Bash, Shell::Fish];

    pub fn name(self) -> &'static str {
        match self {
            Shell::Zsh => "zsh",
            Shell::Bash => "bash",
            Shell::Fish => "fish",
        }
    }

    pub fn from_name(name: &str) -> Option<Shell> {
        Shell::ALL.into_iter().find(|s| s.name() == name)
    }
}

#[derive(Debug)]
pub enum CompletionsCommand {
    /// Print the script for a shell.
    Script(Shell),
    /// Write it where the shell autoloads it; `None` detects the shell.
    Install(Option<Shell>),
    Uninstall(Option<Shell>),
}

// ─── Config ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ConfigCommand {
    Show(ConfigShowArgs),
}

#[derive(Debug, Default)]
pub struct ConfigShowArgs {
    /// The output level the shared layer swept out of argv (da-qt6z).
    /// `--json` is the value `json`; `Show::None` prints nothing.
    pub show: Show,
}

// ─── Info / Setup ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct InfoArgs {
    pub target: Option<String>,
    pub all: bool,
}

#[derive(Debug, Default)]
pub struct SetupArgs {}

/// `datum reparse <hid>...` — replay one or more named entries' stored
/// `raw_input` through the current parser, diffing the result against
/// the stored columns.
///
/// Closes the persisted-intent loop (da-g5uk): the insert path stamps
/// `raw_input` + `alias_chain` + `parser_version` on every row; this
/// command is the consumer that lets a parser-bug fix retroactively
/// correct already-stored rows. Replay is hermetic to the alias state
/// at write time — `alias_chain` is rehydrated as the resolver, never
/// the live aliases.
///
/// Per da-bkak: reparse operates ONLY on entries the user explicitly
/// names by hid. Walking all entries is out of scope — long-standing
/// data validated by the user is sacrosanct, never bulk-rewritten.
#[derive(Debug, Default)]
pub struct ReparseArgs {
    /// Hids (or slugs / quickIds) of the entries to re-parse. At least
    /// one is required; reparse never walks the entry log implicitly.
    pub hids: Vec<String>,
    /// UPDATE rows where the parser produced a different shape; bumps
    /// `parser_version` and recomputes `content_hash`. Default is dry-run.
    pub fix: bool,
}

// ─── Db (subcommand group) ────────────────────────────────────────────────

#[derive(Debug)]
pub enum DbCommand {
    /// `datum db setup` — initialize or migrate the active DB schema.
    Setup(SetupArgs),
    /// `datum db purge` — drop every datum table and rebuild from scratch.
    Purge(DbPurgeArgs),
    /// `datum db backup` — dump live entries to JSONL or tar.gz.
    Backup(BackupArgs),
    /// `datum db backup --to DIR | --list DIR | --plan` — the Tower of
    /// Hanoi rotation (da-d5cmj).
    BackupRotation(BackupRotationArgs),
    /// `datum db restore` — replay entries from a backup file.
    Restore(RestoreArgs),
    /// `datum db migrate` — schema migration runner (run/status/edit).
    Migrate(MigrateCommand),
    /// `datum db v2import` — load a legacy datum CouchDB backup JSON.
    V2import(DbImportArgs),
}

#[derive(Debug, Default)]
pub struct DbPurgeArgs {
    /// `--yes` / `-y` — skip the type-the-database-path confirmation prompt
    /// and purge non-interactively. Required when stdin is not a TTY.
    pub yes: bool,
    /// `--leave` — a chained copy tells its chain it leaves, then is wiped
    /// (da-qjxfx); `--force` with it tells no one.
    pub leave: bool,
    pub force: bool,
    /// `--rejoin` — a chained copy is wiped and stays a member, with its
    /// endpoint key, refetching everything from the chain (da-qjxfx).
    pub rejoin: bool,
}

#[derive(Debug, Default)]
pub struct DbImportArgs {
    pub backup_file: Option<String>,
    pub backup_file_positional: Option<String>,
    pub dry_run: bool,
    /// `--skip-log <path>` — write the full list of skipped docs (with
    /// reason and original id) to this file for triage. The summary always
    /// reports a reason-grouped count; this flag persists the detail.
    pub skip_log: Option<String>,
}

// ─── Logging commands (occur/start/stop/switch/add) ──────────────────────

/// The shared arg bag for all logging verbs. Composed shape (da-e2e
/// Phase A): timing/duration/output flags live in their concern bags so
/// callers can call `args.time.build()`, `args.duration.build()`,
/// `args.output.resolve()` directly. Log-specific extras (keys,
/// slug_keys, conflict, …) remain as direct fields since no shared
/// concern module owns them.
#[derive(Debug, Default)]
pub struct LogArgs {
    pub field_or_verb: String,
    pub rest: Vec<String>,
    /// `-y/-q/-d/-t/-z/-T/-D` — composed timing concern.
    pub time: TimeArgs,
    /// `-m/--moment` + the resolved duration — composed duration concern.
    pub duration: DurationArgs,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
    /// `-u/--undo` — delete the most recent live entry that matches this
    /// command (same field + state). Re-creates original datum's undo
    /// semantics so the user's `uu` muscle-memory alias (re-run last
    /// command with `-u` appended) reverses the prior insert.
    pub undo: bool,
    pub keys: Vec<String>,
    pub slug_keys: Vec<String>,
    /// `-F` / `--field-key` (da-0csuh): a `-k` slot whose value is also
    /// appended to the field, `__`-joined — `issue -F issue_id` builds the
    /// field template `issue__%issue_id%`. Same K / K= / K=val forms.
    pub field_keys: Vec<String>,
    /// `-c` / `--comment`. Repeatable; each flag APPENDS one string to the
    /// `data.comment` array, which is an array however many there are
    /// (da-ult68). Joined into the entry by
    /// [`crate::input::data_args::handle_data_args`].
    pub comment: Vec<String>,
    /// `-R` / `--remainder`: route un-keyed positionals to `<field>` in
    /// `extra_data` instead of the primary `value` column. Mirrors datum's
    /// remainder-bucket override.
    pub remainder: Option<String>,
    /// `-S` / `--string-remainder`: join the remainder positionals into
    /// one space-separated string instead of an array. When supplied
    /// without `-R`/`-C`, "the rest" lands in the `extra_data` bucket
    /// rather than erroring.
    pub string_remainder: bool,
    /// `-C` / `--comment-remainder`: shorthand for `-SR comment` —
    /// remainder positionals join into a single string, appended to the
    /// `data.comment` array.
    pub comment_remainder: bool,
    /// `-I` / `--string-state`: join the remainder positionals into the
    /// entry's state (canonical slot `state.id`) as one space-separated
    /// string — the state-targeting analogue of `-S`. (da-vo0y Feature 1.)
    pub string_state: bool,
    pub id: Option<String>,
    /// `--id-delimiter` raw CLI value. `None` means "fall through to env /
    /// `datumrc.toml` / built-in default" at the slug-interpolation site;
    /// `Some(_)` is the user's per-invocation override.
    pub id_delimiter: Option<String>,
    /// `-L`/`--last-state` user assertion.
    ///
    /// * `None` — flag not supplied.
    /// * `Some("<value>")` — `-L <value>`/`--last-state=<value>`; recorded
    ///   as an assertion on the entry's `user_asserted_last_state`, and on
    ///   Stop closes the most recent open block.
    /// * `Some("")` — never from the parser, which requires a value; set
    ///   internally by `end_time=` on a switch that names no state.
    pub last_state: Option<String>,
    pub conflict: Option<ConflictStrategyArg>,
    pub update: bool,
    pub merge: bool,
    pub no_metadata: bool,
    /// `--alias <NAME>` — when set, after the command succeeds, persist
    /// `<NAME> -> <canonical-form>` to the alias table. The canonical
    /// form is the original argv with the alias-naming flag (and any
    /// `--alias-overwrite`) stripped.
    pub alias_name: Option<String>,
    /// `--alias-overwrite` — companion to `--alias`. Allows replacing an
    /// existing alias of the same name; without it, a name collision is
    /// an error.
    pub alias_overwrite: bool,
    /// `--dry-run` / `--dryrun` — compute the full effect of the write
    /// (build the would-be Entry, including slug derivation, field
    /// interpolation, conflict detection) and print a preview without
    /// touching the database.
    pub dry_run: bool,
    /// `--if-changed` (switch/start/end only, da-hxmtg) — write nothing
    /// when the field already holds this state at the entry's time.
    pub if_changed: bool,
    /// The types the field's spec declares for its keys (da-b2f8o) — not
    /// a flag; filled from the spec before the line is parsed.
    pub key_types: std::collections::BTreeMap<String, crate::input::KeyType>,
}

impl LogArgs {
    /// Start from `Default` with `field_or_verb` pre-set — the one field
    /// every logging caller supplies up front.
    pub fn with_field(field_or_verb: impl Into<String>) -> Self {
        Self {
            field_or_verb: field_or_verb.into(),
            ..Self::default()
        }
    }
}

/// Mirrors `datum_core::db::ConflictStrategy`; lives here so `datum-parse`
/// stays `datum-core`-free. The `From<ConflictStrategyArg> for
/// ConflictStrategy` conversion lives in `datum-commands::args`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictStrategyArg {
    Update,
    Merge,
    Rekey,
    Ignore,
}

/// `update --strategy`: daydumb's update | merge | rekey, or a row of the
/// original datum's combineData table by its name there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateStrategyArg {
    Update,
    Merge,
    Rekey,
    Combine(crate::combine::Combine),
}

// ─── Read / list commands ─────────────────────────────────────────────────

#[derive(Debug)]
pub struct TailArgs {
    /// Zero, one, or many field names. Comma-separated on the CLI:
    /// `datum tail mood,project` parses to `["mood", "project"]`. Empty = no
    /// filter. Multi-field rows interleave by occur_time in render.
    pub fields: Vec<String>,
    pub count: Option<usize>,
    pub metric: String,
    pub reverse: bool,
    pub columns: Vec<String>,
    /// `-y/-q/-d/-t` — composed timing concern. Tail does not expose
    /// `-z/-T/-D`; the parser strips those specs when building the
    /// registry.
    pub time: TimeArgs,
    /// Take the first N (oldest) instead of the last N (newest). Powers
    /// `datum head`, which routes through tail with this flag set.
    pub oldest_first: bool,
    pub watch: Option<f64>,
    /// `--table`: force the legacy columnar view (time | field | state | dur |
    /// hid). Default is the info-line renderer matching `show_create` /
    /// `show_update`. Auto-enabled when `--column` or a non-hybrid `--metric`
    /// is set — those are tabular concerns.
    pub table: bool,
    /// `field:state` form (da-plbb): when set, `tail` renders state-blocks
    /// (start/end/dur) for periods where `fields[0]` was in the matching
    /// state. Parsed from the positional by splitting on the first `:`
    /// and then on `,` to form the parallel-state array. Mutually
    /// exclusive with the comma-separated multi-field form — when the
    /// positional contains `:`, the whole token is treated as a
    /// state-filter, not as a field list. Stored as raw strings;
    /// normalization happens in the command layer at compare time.
    pub state_filter: Option<Vec<String>>,
    /// `--show-meta`: include `extra_metadata.flag = true` (da-dnnn) and
    /// `extra_metadata.meta = true` (da-xn8j) rows. Default is to hide
    /// them — they're review markers / out-of-band notes, not tracked
    /// data. See da-ghlx. It also brings back the meta fields' entries —
    /// specs, layouts, aliases, views, kit commands and kit index definitions
    /// (da-lc26c, da-b3752, da-fwwtn) — which an unnamed tail leaves out.
    pub show_meta: bool,
    /// `--above <ID>`: only the rows listed above that entry, whatever the
    /// page that listed it (da-mhcla). Unlike `-t` it reaches the timeless
    /// rows, which sort above every timed one.
    pub above: Option<String>,
    /// `-A/--show-all` / `-o/--format-string`, plus the swept `--show`.
    ///
    /// `--show json` (da-c7uw's `--json`) emits one named object per row
    /// instead of the human table. The table's column set is a display
    /// decision and has changed under scripts that position-parsed it;
    /// this is the shape consumers are meant to read. With `--watch` the
    /// rows stream as JSON Lines, one document per changed frame.
    pub output: OutputArgs,
}

impl Default for TailArgs {
    fn default() -> Self {
        Self {
            fields: Vec::new(),
            state_filter: None,
            count: None,
            metric: "hybrid".to_string(),
            reverse: false,
            columns: Vec::new(),
            time: TimeArgs::default(),
            oldest_first: false,
            watch: None,
            table: false,
            show_meta: false,
            above: None,
            output: OutputArgs::default(),
        }
    }
}

#[derive(Debug, Default)]
pub struct GetArgs {
    /// Optional. When omitted, callers default to the `_LAST` quickId
    /// token (resolves against the last-accessed pointer). Repurposed
    /// as the `<field>` positional in search mode (`matchers` non-empty).
    pub id_or_slug: Option<String>,
    pub pretty: bool,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
    /// da-yhh1: matcher tokens (`k=v`, `k>v`, `k=gte=v`, …) supplied
    /// after the positional. When non-empty the dispatcher routes to
    /// the search path; when empty the existing quickId resolve runs.
    pub matchers: Vec<Matcher>,
}

impl GetArgs {
    /// Start from `Default` with the positional id/slug pre-set. The bare
    /// `datum get` form uses `GetArgs::default()` directly.
    pub fn with_id(id_or_slug: impl Into<String>) -> Self {
        Self {
            id_or_slug: Some(id_or_slug.into()),
            ..Self::default()
        }
    }
}

// ─── Mutation (update / merge / rekey) ────────────────────────────────────

#[derive(Debug, Default)]
pub struct UpdateArgs {
    /// Optional. When omitted, callers default to the
    /// `_LAST_WITH_PROTECTION` quickId token (resolves against the
    /// last-accessed pointer, refusing if it's > 15 min stale).
    pub id_or_slug: Option<String>,
    pub rest: Vec<String>,
    pub strategy: Option<UpdateStrategyArg>,
    pub id: Option<String>,
    /// `--id-delimiter` raw CLI value. `None` falls through to env /
    /// `datumrc.toml` / built-in default at the slug-interpolation site.
    pub id_delimiter: Option<String>,
    pub keys: Vec<String>,
    /// `-c` / `--comment`. Repeatable: a single flag yields a string, two
    /// or more yield an array — see [`LogArgs::comment`].
    pub comment: Vec<String>,
    pub dry_run: bool,
    /// `--all-or-nothing`: with comma-batched ids, change nothing if any
    /// one would fail (da-eqb30; was `--strict`).
    pub all_or_nothing: bool,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

#[derive(Debug, Default)]
pub struct RetimeArgs {
    /// Optional. When omitted, the command resolves against `_LAST`
    /// (matching the update/merge/delete/undo default).
    pub slug: Option<String>,
    /// ISO-8601 duration shift relative to the current occur_time.
    /// Mutually exclusive with `time` / `relative` (the parser enforces).
    pub offset: Option<String>,
    /// Time-arg group (`-t`/`-y`/`-q`/`-d`/`-z`/`-D`/`-T`), with the
    /// positional time in the `-t` slot (da-jix52). Each entry's own
    /// occur_time is what these resolve against; when no flag in the group
    /// is set, the command treats the request as "set to now".
    pub time: TimeArgs,
    pub field: Option<String>,
    pub after: Option<String>,
    pub before: Option<String>,
    /// `-r`/`--relative`: read `time` as a pure shift of the entry's
    /// occur_time — `-y` moves it a day and keeps its clock.
    pub relative: bool,
}

#[derive(Debug)]
pub struct EditArgs {
    pub id_or_slug: String,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

#[derive(Debug, Default)]
pub struct DeleteArgs {
    /// Optional. When omitted, `delete` targets `_LAST` — plainly, with no
    /// protection window (da-s6no). Past ten minutes it warns on stderr with
    /// the pointer's age and proceeds; `datum undo` is the way back.
    pub id_or_slug: Option<String>,
    pub dry_run: bool,
    /// `--all-or-nothing`: with comma-batched tokens, delete nothing if
    /// any one fails to resolve (da-eqb30; was `--strict`).
    pub all_or_nothing: bool,
    /// `--all`: every live entry, as one command — one step of the undo
    /// stack, so a bare `undo` brings them all back (da-97gth).
    pub all: bool,
    /// `--keep-meta`, with `--all`: spare what `tail --show-meta` adds —
    /// specs, layouts, aliases, views, commands, indexes, kits, flags and
    /// `datum meta` notes.
    pub keep_meta: bool,
    /// `-c` / `--comment`: why — appended to each row's `data.comment` as
    /// `deleted: <why>` in the transaction that tombstones it (da-ult68).
    pub comment: Option<String>,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

/// `datum undo [ID_OR_SLUG[,ID_OR_SLUG]...] [--to-version N]` — one step back along the
/// entry's history trail.
///
/// There is no staleness flag here any more (da-s6no): `undo` used to refuse
/// a `_LAST` pointer older than fifteen minutes unless `--force` was given,
/// and to hunt tombstones only inside `--window`. Both were gates on a
/// reversible operation, and `redo` made them cost more than they saved —
/// `undo` now always proceeds and warns instead. `--force` is still accepted
/// and parsed into nothing, so scripts carrying it keep working (`delete --force`
/// is gone: da-286x); `--window` took a value and is refused with a message that says
/// what replaced it.
#[derive(Debug, Default)]
pub struct UndoArgs {
    pub id_or_slug: Option<String>,
    pub to_version: Option<u32>,
    /// `--skip-changed`: a bare undo leaves the entries another copy changed
    /// since as they are and takes back the rest (da-e3be3).
    pub skip_changed: bool,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

/// `datum redo [ID_OR_SLUG[,ID_OR_SLUG]...]` — one step FORWARD along the trail, undoing the
/// last `undo`. No `--to-version`: an explicit version number is what `undo`
/// is for, and `redo` is the word for the step a hand already took back.
#[derive(Debug, Default)]
pub struct RedoArgs {
    pub id_or_slug: Option<String>,
    /// `--dry-run`: step nothing, print nothing, and exit as the redo would —
    /// 0 when something lies ahead, 1 when there is nothing to redo. What the
    /// web's redo button asks before it lights (da-zq91a).
    pub dry_run: bool,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

#[derive(Debug)]
pub struct HistoryArgs {
    pub id_or_slug: String,
    pub version: Option<u32>,
    pub diff: Option<Vec<u32>>,
    /// The output level the shared layer swept out of argv (da-qt6z).
    /// `--json` is the value `json`; `Show::None` prints nothing.
    pub show: Show,
}

#[derive(Debug)]
pub struct WhyArgs {
    pub id_or_slug: String,
    pub full: bool,
}

// ─── Spec / alias management ──────────────────────────────────────────────

#[derive(Debug)]
pub enum SpecCommand {
    /// `datum spec show <FIELD> [--text]` — JSON by default; `--text`
    /// switches to a human-readable summary including per-state colors
    /// and descriptions (da-nhzo). `--show json` (and its `--json`
    /// shorthand) is the explicit form of the default, read by the shared
    /// output layer like every other command's (da-qt6z).
    Show {
        field: String,
        text: bool,
    },
    /// `datum spec add-state <FIELD> <NAME> [COLOR] [DESCRIPTION] [GLYPH]` —
    /// merge a single state into a field spec without disturbing the others
    /// (da-t9ps). The safe, incremental counterpart to `spec set --state`,
    /// which replaces the whole list. `-` in a slot leaves it alone.
    AddState {
        field: String,
        name: String,
        color: Option<String>,
        description: Option<String>,
        /// The state's occur-dot glyph (da-gzowt).
        glyph: Option<String>,
        /// `--key KEY`, repeated: the state's whole key list (da-cej7q).
        /// An empty `--key=` clears the ones before it.
        keys: Vec<String>,
        /// `--assign KEY=VALUE`, repeated: the state's assigned keys
        /// (da-ulc0v); an empty VALUE removes one.
        assign: Vec<String>,
    },
    Set(Box<SpecSetArgs>),
    List {
        tag: Option<String>,
    },
    Remove {
        field: String,
    },
}

#[derive(Debug, Default)]
pub struct SpecSetArgs {
    pub field: String,
    /// `--from-json <JSON>` — the whole spec body, replacing the existing
    /// one. Spelled `--json` until da-qt6z made that the global
    /// `--show json`; vividn picked the new name.
    pub from_json: Option<String>,
    /// `--verb <occur|start|switch|add>` — what a bare press of the
    /// field means. Required when the spec is being created (da-c8mh).
    pub verb: Option<String>,
    pub states: Vec<String>,
    pub color: Option<String>,
    /// Per-state colors in `STATE=COLOR` form. An empty COLOR clears the
    /// entry — the clear convention shared by all `STATE=VALUE` flags here.
    pub state_colors: Vec<String>,
    /// Per-state descriptions in `STATE=TEXT` form (da-zwqk). Mirrors
    /// `state_colors`, including empty-TEXT-clears. Empty by default.
    pub state_descriptions: Vec<String>,
    /// Per-state keys in `STATE=KEY` form (da-cej7q): the flags naming a
    /// state give its whole key list, as `--data-key` gives the field's;
    /// `STATE=` clears the ones before it.
    pub state_keys: Vec<String>,
    /// Per-state assigned keys in `STATE=KEY=VALUE` form (da-ulc0v); an
    /// empty VALUE removes the key.
    pub state_assign: Vec<String>,
    /// `--state-lookup-field <F>` — the field a bare state names a
    /// document of (da-f9jyv). An empty value clears it.
    pub state_lookup_field: Option<String>,
    pub id_template: Option<String>,
    pub description: Option<String>,
    pub track: bool,
    pub ignore: bool,
    pub id_keys: Vec<String>,
    pub data_keys: Vec<String>,
    /// `--assign KEY=VALUE` / `--id-assign KEY=VALUE` (da-ulc0v): an
    /// assigned key, merged into the spec's map; an empty VALUE removes it.
    pub assign: Vec<String>,
    pub id_assign: Vec<String>,
    pub no_autoalias: bool,
    /// Vertical position of the field on the dayview/nowview, fraction
    /// `[0.0, 1.0]`. `None` here means "leave as-is in the existing spec";
    /// to clear an explicit value, the user must currently re-set the spec
    /// from JSON. Default fallback is the `md5(field)` hash.
    pub y: Option<f64>,
    /// Vertical row height as a fraction of the dayview's data area.
    /// `None` here means "leave as-is"; default fallback is `0.12`.
    pub height: Option<f64>,
    /// Stacking order. Higher values render on top. `None` here means
    /// "leave as-is"; default fallback is the resolved `y`.
    pub z_index: Option<f64>,
    /// Replace the spec's `tags` list with this. Empty (`Vec::is_empty`) is
    /// treated as "leave as-is" — to clear tags, pass
    /// `--from-json '...tags=[]'`.
    pub tags: Vec<String>,
    /// `--only-listed-keys` / `--any-keys` (da-eqb30): the spec's
    /// `only_listed_keys`; `None` leaves the existing value.
    pub only_listed_keys: Option<bool>,
    /// `--only-listed-states` / `--any-states` (da-eqb30): the spec's
    /// `only_listed_states`; `None` leaves the existing value.
    pub only_listed_states: Option<bool>,
    /// `--remainder <KEY>` (da-eqb30): where the field's leftover words
    /// go, as `-R <KEY>` would send them. An empty value clears it.
    pub remainder: Option<String>,
    /// `--never-false` / `--allow-false` (da-cwieh): the spec's
    /// `never_false`; `None` leaves the existing value.
    pub never_false: Option<bool>,
    /// `--glyph <CHAR>` — what the field's occur dots draw instead of a
    /// circle (da-gzowt). An empty value clears it.
    pub glyph: Option<String>,
    /// `--state-glyph STATE=CHAR`, the `state_colors` shape and clear
    /// convention.
    pub state_glyphs: Vec<String>,
    /// `--glyph-by <KEY>` — the data key whose value picks the glyph. An
    /// empty value clears it, map and all.
    pub glyph_by: Option<String>,
    /// `--glyph-map VALUE=CHAR` — one entry of the `--glyph-by` map; an
    /// empty CHAR removes the entry.
    pub glyph_map: Vec<String>,
    /// `--icon <FILE>` — an `.svg` the dayview draws for the field's dots
    /// (da-45wgz). An empty value clears it.
    pub icon: Option<String>,
    /// `--glyph-scale <N>` — an occur dot's size, a multiple of the one
    /// the dayview picks (da-c05v1). Positive; 1 clears it.
    pub glyph_scale: Option<f64>,
    /// `--glyph-aspect <N>` — the dot's width over its height. Positive;
    /// 1 clears it.
    pub glyph_aspect: Option<f64>,
}

#[derive(Debug)]
pub enum AliasCommand {
    Show { name: String },
    Set { name: String, expansion: String },
    List { show: Show },
    Remove { name: String },
}

// ─── Kit (da-d4t.1) ────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum KitCommand {
    /// `datum kit install <path>` — parse a kit TOML and upsert its field
    /// specs. Existing rows for the same field win unless `--force`.
    Install(KitInstallArgs),
    /// `datum kit export <name> [path]` — serialize the kit's installed
    /// rows back to TOML. None path → stdout.
    Export(KitExportArgs),
    /// `datum kit list` — installed kits + per-kit field counts.
    List,
    /// `datum kit uninstall <name>` — remove every row tagged with this
    /// kit's id; user-declared rows untouched.
    Uninstall(KitUninstallArgs),
    /// `datum kit show <name>` — the kit's installed contents, for reading,
    /// with each index definition that failed to build on this device.
    Show { name: String },
}

#[derive(Debug)]
pub struct KitInstallArgs {
    pub path: String,
    /// When true, conflicting field rows are overwritten and re-tagged
    /// with this kit's id.
    pub force: bool,
    /// When true, auto-approve the index trust prompt (used for
    /// non-interactive shells / CI). Without `--yes`, the dispatcher
    /// prints the proposed indices and reads y/N from stdin.
    pub assume_yes: bool,
    /// Print the kit's name and what it brings, and write nothing — what a
    /// caller asks before it answers the trust prompt itself (the browser).
    pub dry_run: bool,
}

#[derive(Debug)]
pub struct KitExportArgs {
    pub name: String,
    /// Output file. None → write to stdout.
    pub path: Option<String>,
}

#[derive(Debug)]
pub struct KitUninstallArgs {
    pub name: String,
}

// ─── Sync (da-k329q, regrouped da-aam46) ───────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
pub enum SyncCommand {
    /// `datum sync` — sync with every member once, then one line of status.
    Now,
    /// `datum sync status [--show-revoked] [--json]` — per member, how this copy
    /// stands with it (da-rcb5m). A revoked member only with `--show-revoked` or
    /// `--json` (da-73gyz).
    Status {
        show: Show,
        show_revoked: bool,
    },
    /// `datum sync pause | resume` (da-wlbyx): this copy's, never synced.
    Pause,
    Resume,
    /// `datum sync chain invite [--reader] [--fields <F,…> | --kit <KIT>]
    /// [--qr] [--expires <DUR>] [--label <L>] [--admin-key <FILE>]` — an
    /// invite any member can admit, served until redeemed; on a copy in no
    /// chain, it starts an open one (da-6177s). `--reader` admits a reader
    /// (da-9yvb6), and a filter a reader sent only those fields (da-u81u1).
    /// `--label` names this copy (da-ey3hz).
    Invite {
        reader: bool,
        filter: Option<SyncFilter>,
        qr: bool,
        expires_secs: u64,
        label: Option<String>,
        admin_key: Option<String>,
    },
    /// `datum sync chain connect [<TICKET>] [--label <L>]` — redeem an
    /// invite (the ticket from stdin when not given) and sync, joining as
    /// `--label` or a default no other member carries (da-ey3hz).
    Connect {
        ticket: Option<String>,
        label: Option<String>,
    },
    /// `datum sync chain leave [--force] [--dry-run]` — leave this copy's
    /// chain, keeping every entry, under a new endpoint key (da-j8c4l), once
    /// a member holds its departure; `--force` leaves telling no one
    /// (da-dsob1). Hidden
    /// `--alone`: the leave `connect` makes to join another chain, only of a
    /// copy holding nothing but its own data, keeping its key (da-h7pnc).
    /// `--dry-run` only says whether it may.
    Leave {
        alone: bool,
        dry_run: bool,
        force: bool,
    },
    /// `datum sync chain purge (--data | --all) [--yes] [--admin-key <FILE>]`
    /// (da-qzrj9): every copy drops every entry, op and history; `--all`
    /// dissolves the chain too. Without `--yes` it says who is told when.
    Purge {
        all: bool,
        yes: bool,
        admin_key: Option<String>,
    },
    /// `datum sync chain list [--show-revoked] [--json]` — the chain's admins and
    /// endpoints, a revoked one only with `--show-revoked` or `--json` (da-73gyz).
    Members {
        show: Show,
        show_revoked: bool,
    },
    /// `datum sync chain rename [<WHO>] <LABEL> [--admin-key <FILE>]` —
    /// relabel this copy (no admin needed), or another member (an admin
    /// act in a locked chain).
    Rename {
        who: Option<String>,
        label: String,
        admin_key: Option<String>,
    },
    /// `datum sync chain ack [<WHO> | --all]` — acknowledge a notice: a new
    /// endpoint or admin, or (`unlock`) the chain's unlocking. Bare, the one
    /// pending; `--all`, every one pending, each named.
    Ack {
        who: Option<String>,
        all: bool,
    },
    /// `datum sync chain rekey` — a new endpoint key for this copy (da-p79vq).
    Rekey,
    /// `datum sync chain revoke <WHO>`, `datum sync chain revert <WHO>
    /// [--yes]`, `datum sync chain role <WHO> member|reader [--fields <F,…> | --kit <KIT>]`, `datum sync admin add <KEY>
    /// [--label <L>]`, `datum sync admin remove <WHO> [--allow-unlock]`,
    /// each `[--admin-key <FILE>]` — a signed act on the chain.
    Admin {
        act: SyncAdminAct,
        who: String,
        label: Option<String>,
        admin_key: Option<String>,
        /// `admin remove` of the last admin opens the chain again.
        allow_unlock: bool,
    },
    /// `datum sync admin list [--json]` — the chain's admins.
    AdminList {
        show: Show,
    },
    /// Hidden: `datum sync --as <ENDPOINT> <…>` — a `sync` command run as the
    /// copy whose endpoint id that is, the one datum.day's session began as:
    /// on a file whose key is another by now (purged, rekeyed, left its
    /// chain) it fails and writes nothing (da-otz0d).
    As(String, Box<SyncCommand>),
    /// Hidden: `datum sync pipe vector` — this copy's version vector as JSON.
    Vector,
    /// Hidden: `datum sync pipe ops [--since <VECTOR>]` — every op the
    /// vector lacks (all of them without one), one JSON line each.
    /// `--no-overlap` leaves out the one op a pipe adds to catch a shared
    /// key: what a session sends, for datum.day's transport (da-6kpj5).
    /// `--peer <ENDPOINT>`: what a session with that member is sent — a
    /// filtered reader, only its fields (da-u81u1).
    Ops {
        since: Option<String>,
        overlap: bool,
        peer: Option<String>,
    },
    /// Hidden: `datum sync pipe apply [--json]` — apply op lines from stdin.
    Apply {
        show: Show,
    },
    /// Hidden: `datum sync pipe records` — the chain's records as this copy
    /// holds them, one op line each (da-2u95o).
    Records,
    /// Hidden: `datum sync pipe verify [--verbose] [--peer <ENDPOINT>]` —
    /// the state hash, per column too; over what this copy shares with a
    /// peer, when either is a filtered reader (da-u81u1).
    Verify {
        verbose: bool,
        peer: Option<String>,
    },
    /// Hidden: `datum sync pipe digest [--peer <ENDPOINT>]` — the state
    /// hash and, by hid, each row's own, as JSON: what a `sync diff`
    /// compares (da-n0ruf).
    Digest {
        peer: Option<String>,
    },
    /// Hidden: `datum sync pipe held [--peer <ENDPOINT>]` — for the hids
    /// on stdin (a JSON array), the row and the ops this copy holds of each.
    Held {
        peer: Option<String>,
    },
    /// `datum sync diff <WHO> [--json]` (da-n0ruf): the entries this copy
    /// and one member hold differently, each with the columns that differ,
    /// both values, and the ops each side holds for it. Changes nothing.
    Diff {
        who: String,
        show: Show,
    },
    /// `datum sync refold [--json]` (da-n0ruf): write every row that is not
    /// what its ops fold to again from its ops. Signs no op.
    Refold {
        show: Show,
    },
    /// Hidden: `datum sync run` — stay up: serve members and push new ops.
    Run,
    /// `datum sync daemon install | uninstall | status` (da-vd2sn): the
    /// background service that runs `sync daemon run` (hidden) — `sync run`
    /// for every chained database in the data directory.
    Daemon(SyncDaemonAct),
    /// Hidden: `datum sync endpoint-key` — this copy's endpoint secret,
    /// for datum.day's network worker.
    EndpointKey,
    /// Hidden: `datum sync holds` — how many entries this copy holds, for
    /// datum.day's store: `count`'s, and the flag markers and meta notes it
    /// leaves out (da-lkh1j).
    Holds,
    /// Hidden: `datum sync record-session <ENDPOINT> [--vector <JSON>]
    /// [--verify match|mismatch] [--error <TEXT>] [--met <JSON>]` — how a
    /// session the browser's transport ran ended, and the members' addresses
    /// the peer passed on (da-fly3f).
    RecordSession {
        endpoint: String,
        vector: Option<String>,
        verify: Option<bool>,
        error: Option<String>,
        met: Option<String>,
    },
    /// Hidden: `datum sync handshake <ENDPOINT> --label <L> --addr <JSON>
    /// [--invite <SECRET> --chain <ID> --holds <N>] [--records] [--dialed]` — a
    /// session's handshake as it bears on this copy, for datum.day's
    /// transport (da-6kpj5): `added`, `known`, or the refusal as the error.
    /// `--chain` is the redeemer's own chain, empty for none (da-mepav);
    /// `--holds` its entries, as `datum sync holds` counts them (da-tti3w).
    /// `--dialed` with `--invite`: this copy redeemed it there (da-38eye).
    /// `--datum <BUILD> --schema <N>`: what the peer runs (da-3zqyv).
    /// `--records`: the peer's chain records are on stdin, to admit it by
    /// when this copy has not heard it joined (da-2u95o).
    Handshake {
        endpoint: String,
        label: String,
        addr: String,
        invite: Option<String>,
        chain: Option<String>,
        holds: u64,
        version: Option<(String, u32)>,
        records: bool,
        dialed: bool,
    },
    /// Hidden: `datum sync new-invite [--reader] [--expires <DUR>]
    /// [--admin-key <FILE>]` — write `invite`'s record and print its
    /// secret, for datum.day's transport to serve.
    NewInvite {
        reader: bool,
        filter: Option<SyncFilter>,
        expires_secs: u64,
        admin_key: Option<String>,
    },
    /// Hidden: `datum sync approve <LINK>` — write the chain record an
    /// approve link carries, signed with the approval code on stdin: how
    /// datum.day signs an act with a passkey (da-bjxts).
    Approve {
        link: String,
    },
    /// Hidden: `datum sync qr <TEXT>` — `TEXT` as a QR code, SVG: the
    /// ticket datum.day shows to add a device (da-bjxts).
    Qr {
        text: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDaemonAct {
    Install,
    Uninstall,
    Status,
    Run,
}

/// A filtered reader's fields (da-u81u1): named, or a kit's, resolved
/// against the database when the record is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncFilter {
    Fields(Vec<String>),
    Kit(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncAdminAct {
    Revoke,
    /// `revert <WHO>` previews (`--json` for a program); `--yes` does it.
    Revert {
        yes: bool,
        show: Show,
    },
    Role {
        reader: bool,
        filter: Option<SyncFilter>,
    },
    AddAdmin,
    RemoveAdmin,
}

// ─── Conflicts (da-ttsm, da-05rkv, da-07594) ──────────────────────────────

#[derive(Debug)]
pub enum ConflictsCommand {
    /// `datum conflicts [list] [--json]` — every entry with conflicts, each
    /// contested key's sides lettered A, B, … in clock order.
    List(ConflictsListArgs),
    /// `datum conflicts resolve <HID> [STRATEGY] [KEY=a|b|VALUE…]` — one
    /// edit that saw every side.
    Resolve(ConflictsResolveArgs),
}

#[derive(Debug, Default)]
pub struct ConflictsListArgs {
    /// `--json` is the value `json`: each entry's `conflicts` as stored.
    pub show: Show,
}

#[derive(Debug)]
pub struct ConflictsResolveArgs {
    /// The entry: a hid prefix or a slug.
    pub hid: String,
    /// `--use-a | --use-b | --prefer-a | --prefer-b | --merge`: what the
    /// keys no pick names become, A the earlier side and B the later.
    /// `None`: they keep what the entry shows.
    pub strategy: Option<crate::combine::Combine>,
    /// `KEY=LETTER` (a side) or `KEY=VALUE` (typed as a data value) picks.
    pub picks: Vec<(String, String)>,
}

// ─── Debug (da-4dvk7) ─────────────────────────────────────────────────────

#[derive(Debug)]
pub enum DebugCommand {
    /// `datum debug conflicts [N]` — N entries (3 by default) each edited
    /// here and by a debug device unseen, and one deleted here as it edited
    /// it: N + 1 sync conflicts.
    Conflicts(usize),
    /// `datum debug ops <HID>` — every op held of an entry.
    Ops(String),
}

// ─── Now / Track / Ignore ─────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct NowArgs {
    /// `-A` / `--all` — include ignored fields. Layer 3 in the
    /// disclosure ladder (da-84cg): everything that exists. Implies
    /// `--tracked` (the ladder is concentric).
    pub show_all: bool,
    /// `-a` / `--tracked` — also surface tracked fields whose current
    /// state is falsy. Layer 2 in the disclosure ladder; default is
    /// layer 1 (active-only). Rebound from `-t` to `-a` in da-tx8j so
    /// the standard `-t TIME` time_args flag can fly here.
    pub show_tracked: bool,
    /// The swept `--show` (da-qt6z). `--show json` — the old `--json` —
    /// emits the `datum_core::db::NowSnapshot` wire schema
    /// instead of the terminal view. Other surfaces (web, wear, voice)
    /// consume this; the schema is versioned via
    /// `datum_core::db::NOW_SCHEMA_ID`.
    pub show: Show,
    /// `--bare` — suppress the trailing terminal nowview band; only show
    /// the active-state list. Useful for piping or scripting.
    pub bare: bool,
    /// Force-disable color in the state list and the nowview band.
    /// Mirrors `NO_COLOR` env var behavior; useful for redirected output.
    pub no_color: bool,
    /// Width in cells for the whole view, the state list and the band
    /// (da-coi0m). None → `$COLUMNS`, else the terminal, else the list
    /// prints whole and the band at 80.
    pub cols: Option<u16>,
    /// `-t/-d/-y/-q/-z` — composed timing concern (da-tx8j). When any
    /// of these are set, `datum now` shows the state-at-that-point
    /// instead of the live wall-clock state, and centres the terminal
    /// nowview band on that point (da-coi0m).
    pub time: TimeArgs,
}

#[derive(Debug)]
pub struct TrackArgs {
    pub field: String,
    /// `--verb <occur|start|switch|add>`. Required for a field that has
    /// neither a spec nor a log to read the answer off (da-c8mh).
    pub verb: Option<String>,
    pub color: Option<String>,
    /// Repeatable `--state`. A bare `NAME` is one of the states the field
    /// may be switched to, as `spec set --state` means it; `NAME=COLOR`
    /// paints that state instead. The two forms are told apart by the `=`,
    /// so one flag declares a switch field whole (da-94q1).
    pub states: Vec<String>,
    /// Each `--tag <name>` flag adds the tag to the field's spec
    /// idempotently. Repeatable; order preserved on first-add.
    pub tags: Vec<String>,
    /// Repeatable `--key NAME[=DEFAULT]`: a data key the field's entries
    /// may carry, declared in the same breath as the field (da-h119q).
    pub keys: Vec<String>,
    /// Repeatable `--required-key NAME[=DEFAULT]`: the same, marked required.
    pub required_keys: Vec<String>,
    /// The alias-style tail (da-ulc0v): `-k` / `-K` declare the field's
    /// data / id slots in order, the kind by spelling (`K`, `K=`, `K=D`).
    pub data_keys: Vec<String>,
    pub id_keys: Vec<String>,
    /// ...and a bare `KEY=VALUE` declares an assigned key.
    pub assign: Vec<String>,
    /// `-R KEY` / `--remainder KEY`: where the words no slot takes go, as
    /// `spec set --remainder` says it (da-fu29p); `''` clears it.
    pub remainder: Option<String>,
}

#[derive(Debug)]
pub struct IgnoreArgs {
    pub field: String,
}

// ─── Meta (da-xn8j) ───────────────────────────────────────────────────────

/// `datum meta` subcommand surface — either add a new note or list
/// the notes already attached to a field.
#[derive(Debug)]
pub enum MetaCommand {
    /// `datum meta <field> <text...>` — append a protocol-change note
    /// to `<field>`. Positional tail joins on spaces.
    Add { field: String, text: String },
    /// `datum meta show <field>` — list every note attached to
    /// `<field>` in `create_time` order.
    Show { field: String },
}

// ─── Flag (da-dnnn) ───────────────────────────────────────────────────────

/// `datum flag <field> [comment]` — drop a review-later marker. The
/// standard time-flag group (`-t`/`-d`/`-y`/`-q`/`-z`) places the
/// flag at a specific moment, including back-dated. The optional
/// comment becomes the flag's payload.
#[derive(Debug, Default)]
pub struct FlagArgs {
    pub field: String,
    /// Optional payload — what the user wants to review. Empty
    /// string when the bare `datum flag <field>` form is used (the
    /// "just look here later" marker).
    pub comment: String,
    /// `-y/-q/-d/-t/-z` — the timing concern. `-T` and `-D` are
    /// intentionally not exposed (a flag without a moment loses its
    /// point).
    pub time: TimeArgs,
}

// ─── Grep / Count / Ids / Dayview / Check ────────────────────────────────

#[derive(Debug, Default)]
pub struct FieldsArgs {
    /// The output level the shared layer swept out of argv (da-qt6z).
    /// `--json` is the value `json`; `Show::None` prints nothing.
    pub show: Show,
    /// Ignored fields join the plain listing: `--include-ignored`, or the
    /// engineering spelling `-A` / `--show all` (da-1lnf). `--show json` is
    /// unaffected — it has always carried every field with its `ignored`
    /// flag, and the widget bar reads that.
    pub include_ignored: bool,
    /// `--sort frecency`: most used first, recent use weighing more, and
    /// each row carries its `frecency` (da-xtkcp). Otherwise alphabetical.
    pub by_frecency: bool,
}

#[derive(Debug)]
pub struct GrepArgs {
    pub patterns: Vec<String>,
    pub field: Option<String>,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

#[derive(Debug, Default)]
pub struct CountArgs {
    pub field: Option<String>,
    pub group_by: Option<String>,
    pub since: Option<String>,
    /// The output level the shared layer swept out of argv (da-qt6z).
    /// `--json` is the value `json`; `Show::None` prints nothing.
    pub show: Show,
}

/// Which whole calendar period a `--today` / `--week` / `--month` style
/// flag names. The in-progress periods (`Today`, `Week`, `Month`) run
/// from their start to *now*; the closed ones (`LastWeek`, `LastMonth`)
/// are bounded at both ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsPeriod {
    /// `--today` — this local day so far.
    Today,
    /// `--week` — since the start of this week (see the `week_start` config key).
    Week,
    /// `--last-week` — the whole of the previous week.
    LastWeek,
    /// `--month` — since the first of this month.
    Month,
    /// `--last-month` — the whole of the previous calendar month.
    LastMonth,
}

impl StatsPeriod {
    /// The flag that spells this period, for error messages.
    pub fn flag(self) -> &'static str {
        match self {
            StatsPeriod::Today => "--today",
            StatsPeriod::Week => "--week",
            StatsPeriod::LastWeek => "--last-week",
            StatsPeriod::Month => "--month",
            StatsPeriod::LastMonth => "--last-month",
        }
    }
}

/// `datum stats <field[:state]> [SLICE] [PERIOD] [-t/-d/-q/-y] [--since T]
/// [--until T] [--json]` — count / total-duration / per-state-ratio
/// rollup over a time slice.
///
/// The selector is a whole field (`activity` → every state) or a single
/// `field:state` (`activity:coding`); left out, the rollup is of every
/// field, ranked by the time it was tracked (da-ptuwr). SLICE is a shorthand whole-period
/// positional — a year (`2026`), a month (`2026-06`), or a day
/// (`2026-06-15`); it is exactly equivalent to the matching
/// `--since`/`--until` pair. Block durations are reconstructed from
/// consecutive `occur_time`s per field (switch-block semantics); a
/// stored `dur` on a block entry is used verbatim instead of the
/// gap-to-next.
#[derive(Debug, Default)]
pub struct StatsArgs {
    /// `field` or `field:state`. None is the whole database: every field,
    /// ranked by tracked time (da-ptuwr).
    pub selector: Option<String>,
    /// Shorthand whole-period slice: `YYYY`, `YYYY-MM`, or `YYYY-MM-DD`.
    pub slice: Option<String>,
    /// Inclusive lower bound: an unsigned duration read as "that long
    /// ago" (`1d`, `30m`, `1:30`), a calendar date, or anything `-t`
    /// takes (`now`, `9:15am`, RFC3339, `-30`).
    pub since: Option<String>,
    /// Upper bound, same grammar. A bare `YYYY-MM-DD` covers the whole
    /// named day. Refines the end of whichever start the other flags
    /// name, so `-y --until now` opens up a bounded day.
    pub until: Option<String>,
    /// `--today` / `--week` / `--last-week` / `--month` / `--last-month`.
    pub period: Option<StatsPeriod>,
    /// `-y/-q/-d/-t/-z` — the shared timing flags. A date-only reading
    /// (`-d`, `-y`) names that whole day; anything carrying a clock time
    /// (`-t`, `-q`) is a lower bound, exactly like `--since`.
    pub time: TimeArgs,
    /// `--include-false` — keep the `false` (closed / stopped) state in the
    /// ratio denominator. Off by default: on a real db the closed state is
    /// every gap between blocks and swamps the ratio column (da-icba).
    pub include_false: bool,
    /// `--include-untracked` — keep the untracked states (`null`, the
    /// `IGNORED` sentinel) in the ratio denominator. Off by default, same
    /// reason as [`StatsArgs::include_false`].
    pub include_untracked: bool,
    /// `--group-by client`: the field's rollup split by the client that
    /// wrote it, the key `count --group-by` takes (da-z99wr).
    pub group_by: Option<String>,
    /// Compute every day from the entries, past the per-day cache
    /// (da-n7tp2). No flag sets it: `datum check stats` and the tests do,
    /// to hold the cache to the record.
    pub no_cache: bool,
    /// The output level the shared layer swept out of argv (da-qt6z).
    /// `--json` is the value `json`; `Show::None` prints nothing.
    pub show: Show,
}

#[derive(Debug, Default)]
pub struct IdsArgs {
    pub field: Option<String>,
    pub limit: Option<usize>,
    pub oldest_first: bool,
    pub since: Option<String>,
    pub slug: bool,
    /// The output level the shared layer swept out of argv (da-qt6z).
    /// `--json` is the value `json`; `Show::None` prints nothing.
    pub show: Show,
}

#[derive(Debug)]
pub enum MigrateCommand {
    Run,
    Status,
    Edit,
}

#[derive(Debug)]
pub struct BackupArgs {
    pub path: String,
    pub overwrite: bool,
}

/// `datum db backup --to DIR --every D [--slots N]`, `--list DIR`, and
/// `--plan --every D [--slots N] --span S` (da-d5cmj).
#[derive(Debug)]
pub struct BackupRotationArgs {
    pub action: RotationAction,
    /// `--every`, in seconds: the rotation's period.
    pub every: i64,
    /// `--slots N`: the cap on the rotating slots, slot 0 not counted.
    /// `None` lets the slots grow as time passes.
    pub slots: Option<u32>,
    /// The output level the shared layer swept out of argv (da-qt6z).
    pub show: Show,
}

#[derive(Debug, PartialEq)]
pub enum RotationAction {
    /// Take one backup into the rotation kept in this directory.
    To(String),
    /// Read the directory's index: its backups, newest first.
    List(String),
    /// The slots a rotation holds after `span` seconds, and how far back
    /// each reaches — the arithmetic, with no database or directory.
    Plan { span: i64 },
}

#[derive(Debug)]
pub struct RestoreArgs {
    pub path: String,
    pub force: bool,
    pub dry_run: bool,
    /// `--merge` — add the backup's entries to what is there and delete
    /// nothing. Without it a restore REPLACES (da-5fooh): the live entries
    /// become exactly the backup's, and the rest are tombstoned.
    pub merge: bool,
    /// `--hard` (da-rle8g) — the database becomes exactly the backup:
    /// every entry, tombstone and history row is dropped and the backup's
    /// entries written verbatim.
    pub hard: bool,
    /// `--yes` / `-y` — replace without asking, on a TTY, even when the
    /// restore deletes entries.
    pub yes: bool,
    /// `--if-unchanged FP` (da-2zqad) — replace only if the live entries
    /// are still the ones a `--dry-run --json` fingerprinted as `FP`, so a
    /// yes to its counts deletes what they counted and no more — a
    /// `--hard`'s drops included (da-h3yb3).
    pub if_unchanged: Option<u64>,
    /// The output level the shared layer swept out of argv (da-qt6z).
    /// `--json` is the value `json`; `Show::None` prints nothing.
    pub show: Show,
}

#[derive(Debug, Default)]
pub struct CheckArgs {
    /// The output level the shared layer swept out of argv (da-qt6z).
    /// `--json` is the value `json`; `Show::None` prints nothing.
    pub show: Show,

    /// Positional list of checker names to run. When empty, the default
    /// battery runs (every known checker). When non-empty, only the named
    /// checkers run. Known names: see [`KNOWN_CHECKERS`].
    pub checkers: Vec<String>,
    /// `--fix` — rewrite drifted `verb` columns to match `derive_verb`.
    /// For the `aliases` checker, annotates state-name collisions with the
    /// explicit first-saved-wins resolution (no DB mutation — the
    /// precedence is already deterministic in `SpecAliasIndex`).
    pub fix: bool,
    /// `--list` — print the set of known checker names and exit without
    /// running any checks.
    pub list: bool,
}

/// Checker names accepted as `dm check <NAME>...` positionals. Kept in the
/// parse crate so both the parser (typo validation) and the command
/// dispatcher (which-categories-to-run) read from the same source.
pub const KNOWN_CHECKERS: &[&str] = &[
    "verbs",
    "aliases",
    "slugs",
    "integrity",
    "flags",
    "tz",
    "refs",
    "stats",
    "sync",
];

/// Render orientation for `datum dayview` and `datum nowview`, named by
/// time-axis direction.
/// Mirrors `datum_layout::dayview::Orientation` so callers don't need a
/// layout dep just to construct args.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DayviewOrientation {
    /// Time runs vertically (top→bottom); days stack horizontally — each
    /// day is a column.
    Vertical,
    /// Time runs horizontally (left→right); days stack vertically. Datum's
    /// original layout and the default per bead da-tbe.
    #[default]
    Horizontal,
}

/// `datum dayview` arguments — port of datum's `dayviewCmdArgs` (TS) plus
/// the orientation knob from bead da-a7f.
///
/// Defaults match the original. `width`, `height`, and `day_height` are
/// `Option` so the command can apply datum's auto-sizing rules at dispatch
/// time (see `dayview.ts` lines 38–78).
#[derive(Debug)]
pub struct DayviewArgs {
    /// `--start-date` — inclusive start (ISO `YYYY-MM-DD` or relative
    /// offset like `-3` or `0`). Resolved via `parse_date`.
    pub start_date: Option<String>,
    /// `--end-date`/`-d` — inclusive end. Default: today.
    pub end_date: Option<String>,
    /// `--days`/`-n` — number of days. Default: 1 (when no other dates
    /// are given).
    pub n_days: Option<u32>,
    /// `--width` — total SVG width in pixels. Default: 2000 (horizontal) or
    /// derived from day-stack (vertical).
    pub width: Option<f64>,
    /// `--height` — total SVG height in pixels. None → auto from
    /// `day_height + n_days` (horizontal) or default time extent (vertical).
    pub height: Option<f64>,
    /// `--day-height` — per-day cross-axis extent in pixels. Default: 100.
    pub day_height: Option<f64>,
    /// `--time-axis-height` — time-axis strip thickness. Default: 15.
    pub time_axis_height: f64,
    /// `--label-width` — the date-label column's width in pixels; `0`
    /// draws no date labels and no column, so the bands start at the
    /// margin (da-55ru). None → derived from the label font size.
    pub label_width: Option<f64>,
    /// `--margin` — outer margin. Default: 2.
    pub margin: f64,
    /// `-o`/`--output-file` — output path; extension picks format
    /// (.svg / .png / .html). None → SVG to stdout.
    pub output_file: Option<String>,
    /// The swept `--show` (da-qt6z). `--show json` — the old `--json` —
    /// emits the layout JSON instead of SVG (test-only / debug hook; not
    /// in datum-TS).
    pub show: Show,
    /// `--watch`/`-w` — redraw on a 5-minute interval (poll loop;
    /// follow-up will swap to DB-change events).
    pub watch: bool,
    /// `--orientation horizontal|vertical` (or shorthand `--horizontal` /
    /// `--vertical`), named by time-axis direction. Defaults to horizontal
    /// per bead da-tbe.
    pub orientation: DayviewOrientation,
}

impl Default for DayviewArgs {
    fn default() -> Self {
        Self {
            start_date: None,
            end_date: None,
            n_days: None,
            width: None,
            height: None,
            day_height: None,
            time_axis_height: 15.0,
            label_width: None,
            margin: 2.0,
            output_file: None,
            show: Show::Default,
            watch: false,
            orientation: DayviewOrientation::default(),
        }
    }
}

/// `datum nowview` arguments. Per bead da-6jy nowview shares dayview's
/// render path scoped to the last `context` minutes; there is no separate
/// "now panel" anymore so `--now-width` was dropped.
#[derive(Debug)]
pub struct NowviewArgs {
    /// Context duration string ("15m", "1h", "2h") — `--context`, and
    /// `--history` for the same thing. None is the snapshot (da-7q8r):
    /// each field's state, no time band, as `0` spells explicitly.
    pub history: Option<String>,
    /// `--labels` — name each field's state beside its lane, in a gutter
    /// to the right of the picture (da-7q8r). Off by default, and the SVG
    /// path's alone: the terminal band never carries labels.
    pub labels: bool,
    /// `-t/-d/-q/-y`, the shared time flags, centre the window on the instant
    /// they name instead of ending it at now; `-z` only reframes them
    /// (da-5flt). Resolved at each render, so `--watch` follows the clock.
    pub time: TimeArgs,
    /// SVG width in pixels. None → scale from `history` (datum behavior).
    pub width: Option<f64>,
    /// SVG height in pixels. None → 300 horizontal, and the history-scaled
    /// default `width` takes vertical, where time runs down the page.
    pub height: Option<f64>,
    /// Margin around the plot. Default 2.
    pub margin: f64,
    /// Pixel height of the time-axis strip. Default 15.
    pub time_axis_height: f64,
    /// `--orientation horizontal|vertical` (or `--horizontal` /
    /// `--vertical`), as dayview's (da-78evn). The SVG path's alone.
    pub orientation: DayviewOrientation,
    /// Output file. Extension picks format: .svg / .png / .html. None →
    /// terminal output (per da-57dj), routed through the
    /// `nowview_terminal` visual band-stack renderer.
    pub output_file: Option<String>,
    /// The swept `--show` (da-qt6z). `--show json` emits the
    /// current-state panel as JSON instead of rendering; consumed by
    /// external clients (e.g. the Pebble `nowview` watchface) that want
    /// the resolved per-field state + colors without parsing SVG. It
    /// short-circuits before terminal/SVG dispatch.
    pub show: Show,
    /// Watch mode: redraw on DB change (≈500ms) plus a 60s periodic
    /// refresh so the clock keeps advancing on an idle DB.
    pub watch: bool,
    // Terminal-mode controls (only consulted when `output_file` is None).
    // Per da-57dj `dm nowview` defaults to rendering a pure visual band
    // stack in the terminal — no field/state labels, only colored bands
    // plus an `hh:mm` footer.
    /// Time window for terminal Flow mode. None → vertical Snapshot mode
    /// (current state only). `Some("30m")` → horizontal flow across the
    /// last 30 minutes with a dashed yellow now-marker at the right edge.
    pub window: Option<String>,
    /// Override terminal width in cells. None → auto-detect via `$COLUMNS`.
    pub cols: Option<u16>,
    /// Override terminal height in rows. None → auto-detect via `$LINES`.
    pub rows: Option<u16>,
    /// Force-disable ANSI color in the terminal render. Mirrors `NO_COLOR`.
    pub no_color: bool,
}

impl Default for NowviewArgs {
    fn default() -> Self {
        Self {
            history: None,
            labels: false,
            time: TimeArgs::default(),
            width: None,
            height: None,
            margin: 2.0,
            orientation: DayviewOrientation::default(),
            // da-34w: bumped from 15 → 22 so the time-axis label font
            // (`time_axis_height - 6`, capped) lands at ~16pt instead of
            // ~9pt. The previous default rendered too small to read.
            time_axis_height: 22.0,
            output_file: None,
            show: Show::Default,
            watch: false,
            window: None,
            cols: None,
            rows: None,
            no_color: false,
        }
    }
}

/// Output format for `datum view <name>`. `Auto` resolves at dispatch
/// time: pretty when stdout is a TTY, tsv when piped (matching the
/// slice 4 bead's "pretty when tty, tsv when piped" rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewFormat {
    #[default]
    Auto,
    Pretty,
    Tsv,
    Csv,
    Json,
}

/// `datum view <name>` arguments.
#[derive(Debug, Default)]
pub struct ViewArgs {
    /// View name, looked up as the entry `view:<name>` (da-fwwtn).
    pub name: String,
    /// `--format pretty|tsv|csv|json`. Defaults to [`ViewFormat::Auto`].
    pub format: ViewFormat,
    /// `--watch` — re-render on every external DB write (SQLite
    /// `data_version` poll, ~500ms latency) until interrupted.
    pub watch: bool,
    /// `--list` — print the set of installed view names and exit. Mirrors
    /// `datum check --list` (da-79vv: discoverability gap — view names
    /// otherwise live only inside kit TOML).
    pub list: bool,
}
