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
    /// `datum rename OLD NEW` — a field's entries, spec and derived aliases
    /// under another name (da-ftls7).
    Rename(RenameArgs),

    Spec(SpecCommand),
    Alias(AliasCommand),
    Kit(KitCommand),
    /// `datum conflicts` — the entries whose ops leave a key contested:
    /// list them, resolve one (da-05rkv, da-07594).
    Conflicts(ConflictsCommand),
    /// `datum debug <what>` — test situations made in the current database
    /// (da-4dvk7). Kept off the command list; `datum help debug` has it.
    Debug(DebugCommand),

    /// `track F [CHANGES]` and `ignore F [CHANGES]` (da-ijf3p): `setup F
    /// [CHANGES] --track` / `--ignore`, with `track` or `ignore` set.
    Track(Box<SpecSetArgs>),
    Ignore(Box<SpecSetArgs>),

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
    /// `datum states <field>` — the states a field declares and the states
    /// it has been logged with, each with its count and last use (da-7905t).
    States(StatesArgs),

    Dayview(DayviewArgs),
    Nowview(NowviewArgs),
    /// `datum view <name>` — execute a named SELECT installed via a kit
    /// (or declared by the user) and render the rows. Slice 4 (da-d4t.4).
    View(ViewArgs),

    Info(InfoArgs),
    /// `datum db <subcommand>` — DB-lifecycle group. Houses every database
    /// administration verb: `purge`, `backup`, `restore`,
    /// `migrate`, `v2import` (da-33g consolidated the standalone verbs).
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

    /// `datum install`, `uninstall` and `upgrade`: the binary sets itself
    /// up, takes itself down, and replaces itself with the newest release
    /// (da-uphgn).
    Install(InstallArgs),
    Uninstall,
    Upgrade {
        /// `--check`: only say whether a newer release exists.
        check: bool,
    },
}

#[derive(Debug, Default)]
pub struct InstallArgs {
    /// `--shell`: the shell to complete for; `None` reads `$SHELL`.
    pub shell: Option<Shell>,
    /// `--daemon`: install the sync daemon even with no database syncing.
    pub daemon: bool,
    pub no_completions: bool,
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

// ─── Info ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct InfoArgs {
    pub target: Option<String>,
    pub all: bool,
}

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
    /// `datum db purge` — drop every datum table and rebuild from scratch.
    Purge(DbPurgeArgs),
    /// `datum db backup` — dump live entries to JSONL or tar.gz.
    Backup(BackupArgs),
    /// `datum db backup --to DIR | --list DIR | --plan` — the Tower of
    /// Hanoi rotation (da-d5cmj).
    BackupRotation(BackupRotationArgs),
    /// `datum db restore` — replay entries from a backup file.
    Restore(RestoreArgs),
    /// `datum db migrate` — schema migration runner: bare or `run` applies
    /// what is pending; `status`, `edit`.
    Migrate(MigrateCommand),
    /// `datum db v2import` — load a legacy datum CouchDB backup JSON.
    V2import(DbImportArgs),
    /// `datum db config [KEY [true|false]]` — the database's own settings,
    /// entries that sync (da-w6pp2).
    Config(DbConfigArgs),
}

#[derive(Debug, Default)]
pub struct DbConfigArgs {
    /// The setting named; `None` lists them all.
    pub key: Option<String>,
    /// The value to set it to; `None` prints it.
    pub value: Option<bool>,
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
    /// The words after a bare `--`: positionals as typed (da-gqy3b).
    pub verbatim: Vec<String>,
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
    /// `-F` / `--field-key` (da-0csuh): a `-k` slot whose value is
    /// appended to the field, `__`-joined, and not stored (da-vk747) —
    /// `issue -F issue_id ex7` logs `issue__ex7`. Same K / K= / K=val forms.
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
    /// `-l`/`--last-state` user assertion (or `-L`'s, below).
    ///
    /// * `None` — flag not supplied.
    /// * `Some("<value>")` — `-l <value>`/`--last-state=<value>`; recorded
    ///   as an assertion on the entry's `user_asserted_last_state`, and on
    ///   Stop closes the most recent open block.
    /// * `Some("")` — never from the parser, which requires a value; set
    ///   internally by `end_time=` on a switch that names no state.
    pub last_state: Option<String>,
    /// `-L`/`--ensure-last-state` (da-ovnf9): `last_state` is a CLAIM — the
    /// write lands only if the field holds it, checked in its transaction.
    pub ensure_last_state: bool,
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
    /// `--setup` (da-j04cq): this write may create its field where a
    /// covering template's `auto_setup` is false.
    pub setup: bool,
    /// The types the field's spec declares for its keys (da-b2f8o) — not
    /// a flag; filled from the spec before the line is parsed.
    pub key_types: std::collections::BTreeMap<String, crate::input::KeyType>,
}

impl LogArgs {
    /// da-znksm: whether an `add` line's first bare word is its id: where
    /// today the line is refused for having none. So no `-K` or `--id`, no
    /// time (`-T`, a time flag, or an `occur_time=` / `start_time=` /
    /// `end_time=` key, whose instant is the id), and no `-R`/`-S`/`-C`
    /// to take its bare words. A spec's `id_template` is the caller's to
    /// rule out.
    pub fn takes_positional_id(&self) -> bool {
        let timed = |t: &String| {
            matches!(
                t.split_once('='),
                Some(("occur_time" | "start_time" | "end_time", _))
            )
        };
        self.slug_keys.is_empty()
            && self.id.is_none()
            && !self.time.omit_timestamp
            && !self.time.places()
            && !self.rest.iter().any(timed)
            && self.remainder.is_none()
            && !self.string_remainder
            && !self.comment_remainder
    }

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
    /// `--show-meta`: bring back the meta fields' entries — specs, layouts,
    /// aliases, views, kit commands and kit index definitions (da-lc26c,
    /// da-b3752, da-fwwtn) — which an unnamed tail leaves out, as `_N`
    /// does. A flag marker or meta note is listed either way (da-9e02u).
    pub show_meta: bool,
    /// `--above <ID>`: only the rows listed above that entry, whatever the
    /// page that listed it (da-mhcla). Unlike `-t` it tells two rows at one
    /// instant apart.
    pub above: Option<String>,
    /// `--below <ID>`: only the rows listed below that entry (da-lqz3j),
    /// `--above`'s other side; with `head`, the nearest of them.
    pub below: Option<String>,
    /// `--since <TIME>`: only the rows at or after it, `count`'s grammar
    /// (da-lqz3j). With `-t`, the rows between the two.
    pub since: Option<String>,
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
            below: None,
            since: None,
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
    /// da-xxqdb: `--backlinks`, the records referring to the field's.
    pub backlinks: bool,
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
    /// The words after a bare `--`: positionals as typed (da-gqy3b).
    pub verbatim: Vec<String>,
    pub strategy: Option<UpdateStrategyArg>,
    pub id: Option<String>,
    /// `--id-delimiter` raw CLI value. `None` falls through to env /
    /// `datumrc.toml` / built-in default at the slug-interpolation site.
    pub id_delimiter: Option<String>,
    pub keys: Vec<String>,
    /// `-c` / `--comment`. Repeatable: a single flag yields a string, two
    /// or more yield an array — see [`LogArgs::comment`].
    pub comment: Vec<String>,
    /// `-C` / `--comment-remainder`: the words after the id are one
    /// appended comment.
    pub comment_remainder: bool,
    pub dry_run: bool,
    /// `--all-or-nothing`: with comma-batched ids, change nothing if any
    /// one would fail (da-eqb30; was `--strict`).
    pub all_or_nothing: bool,
    /// `-A/--show-all` / `--show` / `-o/--format-string`.
    pub output: OutputArgs,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RenameArgs {
    pub old: String,
    pub new: String,
    /// `--dry-run`: say what would move, and move nothing.
    pub dry_run: bool,
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
    /// Optional. When omitted, the command edits `_LAST`, asking first when
    /// the pointer is stale (da-llcb7).
    pub id_or_slug: Option<String>,
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

/// `datum setup [FIELD] [CHANGES]` (da-ez40f): no subcommands — the shape
/// of the line says which of these it is.
#[derive(Debug)]
pub enum SpecCommand {
    /// `datum setup [--tag T]` — the fields that have a spec.
    List { tag: Option<String> },
    /// `datum setup <FIELD>` — its spec as a block; `--json` as JSON.
    Show { field: String, show: Show },
    /// `datum setup <FIELD> <CHANGES>` — apply them.
    Set(Box<SpecSetArgs>),
    /// `datum setup <FIELD> --delete` — delete the spec document.
    Delete { field: String, show: Show },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpecSetArgs {
    pub field: String,
    /// The echo's level: `--show json` echoes the spec entry (da-ez40f).
    pub show: Show,
    /// `--from-json <JSON>` — the whole spec body, replacing the existing
    /// one. Spelled `--json` until da-qt6z made that the global
    /// `--show json`; vividn picked the new name.
    pub from_json: Option<String>,
    /// `--verb <occur|start|switch|add>` — what a bare press of the
    /// field means. Required when the spec is being created (da-c8mh).
    pub verb: Option<String>,
    /// `--states a,b,c` — exactly these, in this order (da-f9bky); `''`
    /// clears. A state kept keeps its own, one dropped loses it.
    pub states: Option<Vec<String>>,
    /// `--add-state S` — appended if absent; `--remove-state S` — dropped
    /// with its colour, glyph, description, keys and assigns.
    pub add_states: Vec<String>,
    pub remove_states: Vec<String>,
    /// `--archive-state S` — dropped from the list, keeping what it carries
    /// for its past rows (da-tdpjf).
    pub archive_states: Vec<String>,
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
    /// `-K`/`--id-key` and `-k`/`--data-key`: a slot added to its list, or
    /// given its new spelling in its place, as `track`'s (da-f9bky).
    pub id_keys: Vec<String>,
    pub data_keys: Vec<String>,
    /// `--remove-key K` — K gone from wherever the field declares it.
    pub remove_keys: Vec<String>,
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
    /// `--tags a,b`, `--add-tag T`, `--remove-tag T`: the states' three
    /// operations, on the tags (da-f9bky).
    pub tags: Option<Vec<String>>,
    pub add_tags: Vec<String>,
    pub remove_tags: Vec<String>,
    /// `setup --tag T`, the listing's filter; refused beside a field.
    pub list_tags: Vec<String>,
    /// `--only-listed-keys` / `--any-keys` (da-eqb30): the spec's
    /// `only_listed_keys`; `None` leaves the existing value.
    pub only_listed_keys: Option<bool>,
    /// `--only-listed-states` / `--any-states` (da-eqb30): the spec's
    /// `only_listed_states`; `None` leaves the existing value.
    pub only_listed_states: Option<bool>,
    /// `--remainder <KEY>` (da-eqb30): where the field's leftover words
    /// go, as `-R <KEY>` would send them. An empty value clears it.
    pub remainder: Option<String>,
    /// `-S` / `-C`, as `track`'s (da-zqnt3): with `-R`, either names the
    /// whole route of the leftover words.
    pub string_remainder: bool,
    pub comment_remainder: bool,
    /// `--never-false` / `--allow-false` (da-cwieh): the spec's
    /// `never_false`; `None` leaves the existing value.
    pub never_false: Option<bool>,
    /// `--auto-setup true|false|''` (da-j04cq): a template's
    /// `auto_setup`, `''` clearing it; `None` leaves the existing value.
    pub auto_setup: Option<Option<bool>>,
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
    /// `--glyph-scale <N>` — an occur dot's height, a multiple of the
    /// standard dot's, whatever its lane (da-c05v1, da-ijjjn). Positive;
    /// 1 clears it.
    pub glyph_scale: Option<f64>,
    /// `--glyph-aspect <N>` — the dot's width over its height. Positive;
    /// 1 clears it.
    pub glyph_aspect: Option<f64>,
}

/// `List { names }` prints each alias name a shell alias can take, one a
/// line; `Install` prints the rc-file loop that reads them (a shell alias
/// for every name no command already has); `Uninstall` removes exactly
/// that loop from the rc file.
#[derive(Debug)]
pub enum AliasCommand {
    Show { name: String },
    Set { name: String, expansion: String },
    List { show: Show, names: bool },
    Remove { name: String },
    Install { shell: String },
    Uninstall { shell: String },
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
    /// `--set NAME=VALUE`: the kit's parameters (da-netk8).
    pub set: std::collections::BTreeMap<String, String>,
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
    /// `datum sync [-v | --verbose]` — sync with every member once, then one
    /// line of status; `verbose` adds each session's route and time, and an
    /// offline member's error (da-a4fxr).
    Now {
        verbose: bool,
    },
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
    /// `--label` or with none, its label assembled by the chain (da-xijlw).
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
    /// `datum sync chain rename [<WHO>] (<LABEL> | --unset) [--admin-key
    /// <FILE>]` — relabel this copy (no admin needed), or another member
    /// (an admin act in a locked chain); `--unset` (no LABEL) goes back to
    /// the label assembled from what it runs on (da-xijlw). datum.day's,
    /// unlisted: `--check` only says whether LABEL is one a member may
    /// carry, before a join (da-uyyyg).
    Rename {
        who: Option<String>,
        label: Option<String>,
        admin_key: Option<String>,
        check: bool,
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
    /// `datum sync chain expel <WHO>`, `datum sync chain unjoin <WHO>
    /// [--yes]`, `datum sync chain wipe <WHO> [--yes]`, `datum sync chain role <WHO> member|reader [--fields <F,…> | --kit <KIT>]`, `datum sync admin add <KEY>
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
    /// filtered reader, only its fields (da-u81u1). `--limit <N>`: a
    /// session's round, the N lowest by HLC (da-ymgiv).
    Ops {
        since: Option<String>,
        overlap: bool,
        peer: Option<String>,
        limit: Option<usize>,
    },
    /// Hidden: `datum sync pipe apply [--json]` — apply op lines from stdin.
    Apply {
        show: Show,
    },
    /// Hidden: `datum sync pipe records` — the chain's records as this copy
    /// holds them, one op line each (da-2u95o).
    Records,
    /// Hidden: `datum sync pipe verify [--ordered] [--verbose] [--peer
    /// <ENDPOINT>]` — the state hash, summed, or ordered as a copy before
    /// da-9ln3s sends it, and per column of the ordered one; over what this
    /// copy shares with a peer, when either is a filtered reader (da-u81u1).
    Verify {
        ordered: bool,
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
    /// `datum sync refold [--dry-run] [--json]` (da-n0ruf): write every row
    /// that is not what its ops fold to again from its ops. Signs no op.
    /// `--dry-run` (da-g8wj9) says what each would lose and writes nothing.
    Refold {
        dry_run: bool,
        show: Show,
    },
    /// `datum sync grid [--json]` (da-n0uj1.3): devices × the chains this
    /// install holds, each cell the device's standing in that chain.
    Grid {
        show: Show,
    },
    /// `datum sync grant <DEVICE> <member|reader> [--as <NAME>] [--expires
    /// <DUR>] [--admin-key <FILE>]` (da-n0uj1.4): an invite to this chain,
    /// its ticket sealed to the device in a chain the two share already.
    /// `--as` is the database's name on that device, if not this one's.
    Grant {
        device: String,
        reader: bool,
        name: Option<String>,
        expires_secs: u64,
        admin_key: Option<String>,
        /// Hidden (da-n0uj1.7): datum.day's, whose CLI makes no ticket — the
        /// join link its sync worker made for invite `.1`, which `sync
        /// new-invite` wrote here, so no invite is made.
        ticket: Option<(String, String)>,
    },
    /// `datum sync grant --like <DB> [--devices <D>,…] [--expires <DUR>]
    /// [--admin-key <FILE>]` (da-n0uj1.5, da-pvax0): this database takes
    /// `--like`'s devices, a grant each with the role it has there; in no
    /// chain, it starts one whose admins are `--like`'s.
    GrantLike {
        like: String,
        devices: Option<Vec<String>>,
        expires_secs: u64,
        admin_key: Option<String>,
        /// Hidden (da-n0uj1.7): make no grant, and print the grants left as
        /// JSON, for datum.day to make each with a ticket of its own.
        defer: bool,
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
    /// [--verify match|mismatch] [--error <TEXT> [--offline]] [--met <JSON>]`
    /// — how a session the browser's transport ran ended, `--offline` when
    /// it failed before reaching the peer (da-ueb4p), and the members'
    /// addresses the peer passed on (da-fly3f).
    RecordSession {
        endpoint: String,
        vector: Option<String>,
        verify: Option<bool>,
        error: Option<String>,
        offline: bool,
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
    /// secret, expiry and id (its redeemer's `via`), for datum.day's
    /// transport to serve.
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
    /// Writes the chain's `revoke` record: the stored type keeps its name.
    Expel,
    /// `unjoin <WHO>` previews (`--json` for a program); `--yes` does it,
    /// writing a `purge` record.
    Unjoin {
        yes: bool,
        show: Show,
    },
    /// `wipe <WHO>` says what it does and its limit; `--yes` does it,
    /// writing an `erase` record (da-n0uj1.6).
    Wipe {
        yes: bool,
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
    /// The fields named: only theirs are listed, one the database knows
    /// nothing of too, where the pictures will draw it (da-gq232).
    pub fields: Vec<String>,
}

#[derive(Debug, Default)]
pub struct StatesArgs {
    pub field: String,
    /// The output level the shared layer swept out of argv (da-qt6z).
    pub show: Show,
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

/// A flag that names an end of `stats`' window, in [`StatsArgs::window`]:
/// the later flag wins, end by end (da-4va65). The values are the args'
/// own fields, and this is only which of them came last, but for
/// `--all-time`, which has no field: its entry here is its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsWindowFlag {
    /// A period flag: both ends.
    Period,
    /// `-d/-y/-t/-q`, composed as one: a date both ends, a moment the start.
    Time,
    /// `--since`: the start.
    Since,
    /// `--until`: the end.
    Until,
    /// `--all-time`: both, the first entry to now.
    AllTime,
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
    /// The window flags in the order typed, each naming the ends it sets;
    /// a later one overwrites an earlier one's. Empty (args built in code):
    /// the order is period, timing flags, `--since`, `--until`.
    pub window: Vec<StatsWindowFlag>,
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
    /// entries written verbatim. A chained copy refuses it without `--leave`
    /// (da-95y4o) or `--chain` (da-ncmmj).
    pub hard: bool,
    /// `--leave`, with `--hard` — leave this copy's sync chain, telling it
    /// first, as `db purge --leave` does: the ops, chain and endpoint key go
    /// with the rows, and a fresh invite rejoins it.
    pub leave: bool,
    /// `--chain`, with `--hard` (da-ncmmj) — restore EVERY copy in the
    /// chain to the backup: a chain wipe, then the backup's entries signed
    /// after it, as each copy next syncs.
    pub chain: bool,
    /// `--admin-key <FILE>`, with `--chain` — the wipe is an admin act in a
    /// locked chain.
    pub admin_key: Option<String>,
    /// `--approve-link <LINK> --approval-code <CODE>`, with `--chain`
    /// (da-sd7xt) — the wipe order a passkey approved: the link its
    /// `--dry-run --json` named as `approve`, and the code the passkey
    /// gave. Its stdin can be the backup, where the CLI would ask for one.
    pub approval: Option<(String, String)>,
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
    "spool",
    "install",
    "keys",
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
#[derive(Debug, Clone)]
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
    /// `--sample <SPEC-JSON>`, unlisted: datum.day's setup form (da-srrqd).
    /// The spec's field is drawn with that spec in place of its stored one,
    /// and on a day with nothing of it to draw, sample entries in its shape
    /// that no database holds.
    pub sample: Option<String>,
    /// `--digests 1,2,5`, unlisted: datum.day's day strip (da-efxk5). With
    /// `--show json`, the SHA-256 of the picture each day that many back
    /// from the end date would get as `--days 1` with the same flags, so
    /// the strip keeps a band whose picture is unchanged and asks for no
    /// other run.
    pub digests: Vec<u32>,
    /// `--pictures 0,3,4`, unlisted, beside `--digests` (da-p5lyt): with
    /// `--show json`, the SVG each day that many back would get as
    /// `--days 1` with the same flags, so the strip asks the bands it needs
    /// in one run rather than one process a day.
    pub pictures: Vec<u32>,
    /// `--context none|now|<DUR>`, unlisted, beside `--sample` (da-jc8ic):
    /// what is drawn around the sample. `none` the sample's field alone,
    /// sampled; `now` every field's state now across the band, the sample
    /// over it; a duration the band as that long before now, no integrity
    /// marks, the field's own entries in it drawn in place of a sample.
    pub context: Option<String>,
    /// `--data-layer`, unlisted: datum.day's day strip and nowview
    /// (da-0e06p, da-m6i2a). The bands' data alone, for a viewer that draws
    /// its own axis, gridlines and now over a crop of it: no gridlines and
    /// no now-line, a state still running drawn on to its day's end, for
    /// the viewer to cut at its now, and each day one stretch by instant at
    /// a second a unit, whatever `--width`, so any view scales it.
    pub data_layer: bool,
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
            sample: None,
            digests: Vec::new(),
            pictures: Vec::new(),
            context: None,
            data_layer: false,
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
    /// to the right of the picture (da-7q8r), in its color, with a key of
    /// the occur dots in the window (da-8oksz). The gutter comes out of a
    /// `--width` (`--height` when vertical) given, so the SVG is that size
    /// (da-us41f). Off by default, and the SVG path's alone: the terminal
    /// band never carries labels.
    pub labels: bool,
    /// `--now-label` — a small yellow «now» under the now strip in place
    /// of the `HH:MM` clock (da-9g3bf). The SVG path's alone.
    pub now_label: bool,
    /// `--overlay`, unlisted: datum.day's nowview (da-0e06p). The picture
    /// with no data and no database opened — its axis, ticks, gridlines,
    /// now-line and the labels under its now strip, and the strip's place
    /// as an empty `.now-strip` — for the page to lay a crop of the
    /// dayview's data layer under. The SVG path's alone.
    pub overlay: bool,
    /// `--surround <DUR>` — draw this much more time either side of the
    /// window (before it only, when it ends at now), past the SVG's own
    /// box, for a viewer that pans or zooms without asking again
    /// (da-61nm8). The SVG path's alone.
    pub surround: Option<String>,
    /// `--ahead <DUR>` — draw this much time on past now as well, behind
    /// the now line, for a viewer that slides the picture along with the
    /// clock instead of asking again (da-hklaz): what is open stays open to
    /// there, and what is logged for later is drawn. A window ending at
    /// now only; the SVG path's alone.
    pub ahead: Option<String>,
    /// `-t/-d/-q/-y`, the shared time flags, name the instant the now
    /// section is about instead of now (da-zccjs), and centre the window on
    /// it instead of ending it at now; `-z` only reframes them (da-5flt).
    /// Resolved at each render, so `--watch` follows the clock.
    pub time: TimeArgs,
    /// `--center <TIME>` — centre the window here instead, any `-t` value,
    /// the instant's section standing where the instant is, or pinned to
    /// the edge it lies beyond (da-zccjs). The SVG path's alone.
    pub center: Option<String>,
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
            now_label: false,
            overlay: false,
            surround: None,
            ahead: None,
            time: TimeArgs::default(),
            center: None,
            width: None,
            height: None,
            margin: 2.0,
            orientation: DayviewOrientation::default(),
            // As tall as its digits need, at dayview's size (da-a2cxp).
            time_axis_height: 15.0,
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
