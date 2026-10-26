# daydumb: a guide for contributors and coding agents

## What this is

**daydumb** is a personal life-data tracker. The CLI command is `datum`.
Inspired by https://github.com/vividn/datum (a 10-year personal project by the author).

Core purpose: let users manually log data about their own lives and review it as visualized timelines and statistics. The act of logging forces mindfulness. The visualization is "data as art."

## Repo layout

```
crates/
  datum-core/         # Rust library: data model, SQLite, field specs
  datum-parse/        # Pure: argv + time parsing, verb truth table, entry identity
  datum-cli/          # `datum` binary: clap CLI, commands, output formatting
  datum-commands/     # Command-layer crate: typed + string APIs, dispatched by datum-cli
  datum-layout/       # Pure: dayview/nowview geometry + SVG render
  datum-parity/       # Cross-runtime parity tests against the original datum (vividn/datum, TypeScript)
  datum-sync/         # Easy Sync natively: the wire crate over `Db`, `sync run`, the QR
  datum-sync-wire/    # Easy Sync's session protocol and iroh endpoint over a `Store` trait; also wasm32-unknown-unknown (datum.day's sync worker)
  datum-test-util/    # Shared harness for tests that mutate process-global state (env vars, TZ)
web/            # SvelteKit PWA: static, the real CLI as WASI on OPFS, offline-capable
                # its conventions — the seam, the beat, the panes, the tests: docs/web-conventions.md
```

(Flutter/Android/Wear OS clients are planned but the directories don't
exist yet; legacy datum migration logic lives in
`crates/datum-core/src/migrations/` and `commands/db/import.rs`, not
a top-level `migrations/` folder.)

## Core principles (never violate these)

1. **CLI startup must be near-instantaneous** — the original datum (Node.js) took ~1s which is unacceptable. Rust gives us <5ms. Never add synchronous overhead to the hot path (logging an entry).
2. **Local first** — all data stored on device. Network only for sync.
3. **End-to-end sync** — copies sync peer to peer (Easy Sync, over iroh), and a relay only forwards traffic it cannot read. Never put decryption logic on a relay.
4. **Flexible data model** — both freeform and schema-defined data must work without friction.
5. **American spelling** in everything that lands in the repo: user-facing text, docs, help, error messages, commit messages, new code and comments — color, center, gray, behavior, practice (verb and noun), license (noun), canceled, organize, recognize, favorite (vividn, 2026-10-22). Input stays tolerant: `--colour` is read as `--color`, and `grey` is a color name.

## Data model fundamentals

### Entry fields
- `id` (`hid`): internal, prefix-addressable, sortable. **Primary key and the durable identity, local and cross-device** — sync merges key on it and never rewrite it.
- `slug`: `field:2024-03-16T09:15Z` (human reference, datum-compatible). Derived from field + occur_time, so `retime` rewrites it — **mutable, not an identity**.
- `content_hash`: SHA-256 dedup key
- `field`: what is being tracked (e.g. "coffee", "mood", "project", "word")
- `verb`: the STORED classification, derived from state + dur — `point` | `switch` | `block` | `block-switch` | `point-switch` (`crates/datum-parse/src/verb.rs`). The words a person types — `occur`, `start`, `stop`/`end`, `switch`, `note`, `add` — are the COMMANDS; they set the state and the duration the verb is derived from.
- `state`: the primary JSON value (the state for a switch, the quantity for an occur). **There is no `value` field** — `state` is the canonical name, and a `value` appearing in new code is a bug, not a synonym.
- `following_state`: the trailing-edge state of a switch-style verb; meaningful only alongside a `dur`.
- `data`: arbitrarily nested JSON. (Not `extra_data`.) Nothing infers a unit: `weight=75.2kg` stores the string `"75.2kg"`.
  `update` replaces its top-level keys; `merge` is datum's: a key both hold collects both values
  into one list without duplicates (`item=apple`, `merge … item=pear` → `["apple","pear"]`).
  The deep merge where the new value wins is `update --strategy preferNew` (da-scdm3).
  datum has no units anywhere, in specs, subfields or entries.
  The only planned form is a typed value (`u://`/`unit://`), not built.
- `occur_time` + `dur`: the one instant and the span it covers — see the time model below. There are no `time_start` / `time_end` columns.
- the zone travels with the time (`occur_time_tz`): an IANA name, e.g. "America/Chicago"
- `create_time` / `modify_time` / `client_id`

### Time model
All four combinations are valid as INPUT:
- `time_start` only — open/ongoing event
- `time_start` + `time_end` — complete block, dur derived
- `time_start` + a duration — end derived
- `time_end` + a duration — start derived

What the row actually stores is `occur_time` + `dur`, and `occur_time` is
the block's END: the duration is the span that PRECEDES it (matching the original datum's `durationBlocks` view). So a `(start, end)`
pair lands as `occur_time = end`, and a bare `start` with no end — an
open block — is the one shape whose `occur_time` is its leading edge,
because it has no other instant to store.

### `stats`' per-day cache
`stats` reads a window's whole closed days from `stats_cache` (one row per
field, local day and state) and computes only its partial edges and today
live (`commands/stats/cache.rs`), so all time costs what a day does. A
field's periods line stays one read of the year it reaches back, which is
cheaper than its nine windows' cached days and edges; the cache serves a
field's window only when it starts before that year. It is
local and derived: never synced, wiped when its stamp (`_meta`
`stats_cache`: version, host zone, schema version) differs, refilled from
the first day a write reached. Triggers on `entries` record that day for
every write path, and a row that changes the database's zone forgets every
field from it, since it moves their midnights. The tables, triggers and the
index they read are `db/stats_cache.sql`, laid down by the v55→v56 migration
and after `schema.sql` at every open; a later migration that drops or
rebuilds an `entries` column they name must drop them first. A span's seconds are `floor(end) − floor(start)`, which adds up
exactly across midnights. `datum check stats` recomputes a sample of days and says how many.

### A command costs the same however many fields there are
A command reads the specs of the fields it shows and no others: a renderer
gets `registry_of` / `registry_for` its entries (`commands/helpers.rs`), and
`Db::load_registry`, which parses every spec, is for the commands that list
every field. The word a command starts with is looked up by name:
`spec_alias_names` (`db/spec_alias_names.sql`) holds each derived alias name
by the `spec` row carrying it, kept by triggers on `entries` like the stats
cache, local and derived; a migration that rebuilds `entries` refills it
(`fill_spec_alias_names`), and `datum check aliases` says so where one
did not, `--fix` refilling it (da-ppkv6). `state_read_scaling.rs` holds both to it in VM
steps (da-ulu8f).
A spec may be on a field TEMPLATE (`setup 'issue__%hid:8%' …`, a kit's
`[[field]]`, da-38obu): it covers every field whose name parses against it
(`datum_parse::field_template`; `%hid:8%` is eight characters of `[0-9a-z]`),
which then gets no spec row at its first log. A covered field's own spec
stores only the keys it sets differently, and they override the template's
key by key (`setup F` names the template and them). The longer literal text
wins between two templates, and `setup` refuses one tying another. The
templates are read by their own index, `entries_spec_templates`.
A template's `auto_setup = false` (`setup T --auto-setup false`, a kit's key)
refuses a log naming a field it covers that has no entries and no spec of its
own, unless the line says `--setup` (da-j04cq): a template covering the name
is not the field existing. Only a template takes the key.
The database's own `auto_setup` (`datum db config auto_setup false`, da-w6pp2)
does the same for every field, and a template's key, `true` or `false`, wins
over it for the fields it covers. It is an entry, `setting:auto_setup` of the
meta field `setting` (`db/definition.rs`), so every copy of the db is strict
(vividn, 2026-10-26: «yes it should sync»).

### The spool: a write is never lost to the lock
A command that changes entries (every one but sync's, `db`'s, `config`'s and
`kit`'s, whose indices are DDL no op carries)
and can't take the write lock in 750 ms — a migration, a long sync apply,
another command holds it — runs unchanged on a SNAPSHOT of the database in
memory (`db/spool.rs`, da-9q3fr), with the changes already queued landed in
it first. It prints what it would write with QUEUED as the action word, and
the ops it signed, its resolved effect (the entry a log makes, which entry an
undo takes back, a retime's new time), go as one fsynced JSON line to
`<db>.spool` before it exits 0. A command that may ask its person (`edit`,
an `update` or `retime` of `_LAST` by default) runs on the snapshot from the
start, whatever the lock, and its line lands at once when it can: what the
person answered is about the entry they saw, and nothing is run twice. The
next command that opens the database,
and a sync daemon's tick, land the lines in order, one transaction each:
every op signed again as this copy's next with the `deps` it was made with,
so a change that landed meanwhile meets it under the fold's conflict rules
as a late op from another copy would, and an undo or redo moves the undo
stack as it did on the snapshot. `_meta` `spool_drained` names the last
line committed, so a drain run twice writes nothing more. A REPL line
spools as a command does. A line that fails to land for any reason but the
lock holds every later one behind it: `datum check spool` (in the default
battery, and `sync status`) names it an error, with when it was queued and
why, and `--fix` sets it aside to `<db>.spool.rejected` (da-3j6g7). A line
that does not read never blocks one after it (da-29om3): a torn last line
(a crash mid-append) is set aside to `<db>.spool.torn`, any other to
`<db>.spool.rejected`, with a warning, and an append starts on a fresh line
(so a torn line an append has followed is no longer last: `.rejected`).
Native only: the browser's database is one process's at a time.

## CLI conventions

### Time flags (datum-compatible)
- `-y` / `-yy` / `-yyy` = 1/2/3 days ago (stackable)
- `-q` / `-qq` / `-qqqq` = 5/10/20 min ago (stackable, 5 min each)
- `-d -1` = yesterday, `-d +1` = tomorrow (day offset)
- `-t -8` = 8 min ago, `-t +5` = 5 min ahead (minute offset)
- `-t 1513` = 15:13 today, `-t 9:15am` = natural time
- `-d yesterday` / `today` / `tomorrow`; `-d sunday` = the most recent
  Sunday, `-d sunday+` = the coming one (trailing `+`/`-` forces the
  direction, and applies to `apr29`-style dates too)
- `-t` accepts anything `-d` does — day words, weekdays, `2024-03-16` —
  so a caller holding "either a time or a date" needs only one flag
- `-t` also takes any date it accepts, then any time of day it accepts,
  joined by a space or `T` (`'2026-09-28 10:00'`, `'yesterday 10am'`,
  `2026-09-28T10`), and ISO with an offset (`…T10:00+02:00`).
  `datum help time` lists every form, and a test parses each example on it
- the flags compose in one order however they are typed, v2's: `-t` (a
  clock, then its `±N`), `-q`, `-d`, `-y`. So `-t 10:15 -q` is 10:10,
  `-d 2010-09-01 -y` is August 31, and `-d` names the day whatever a shift
  crossed. One composer for every command, `TimeBuilder`, which
  `TimeArgs::builder` reads the flags into; a clock a DST night skipped comes
  out at the change, one it repeated at its first reading
  (`crates/datum-parse/tests/time_flag_grid.rs`)
- a relative time (`-q`, `-t ±N`) beside a named day (`-d` of any form, `-y`,
  a `-t` that names one) needs a clock, and without one is refused wherever
  the anchor is now (`TimeBuilder::shift_needs_clock`): shift then day and
  day then shift part at midnight and at DST. `-y -t 18:30 -t -6` is 18:24
  yesterday. `retime` anchors on the entry's own time, a clock, and takes them.
  A relative `occur_time=` on a log is one more `-t ±N` to the rule
  (`-y occur_time=-6` is refused), and `-D` wins over the key as over `-t`
  (`-D occur_time=18:30` is the whole day); on `update` its anchor is the
  entry's time
- with no `-z`, a log at a past instant is placed as if `-z` named the zone
  the entries around it agree on (`agreed_zone`: most of the three nearest on
  each side, the same zone on both, so a move names nothing) where that is not
  the host's; the log says so in one line and the entry carries
  `extra_metadata.tz_from = "neighbours"` (da-h8dvi). An offset on `-t` is
  believed. `datum check tz` warns of the converse, `tz_off_neighbours`: a run
  of one or two entries whose neighbours agree on another zone — a spike,
  where a move is a step
- a log whose flags name a wall time a fall-back night repeats (`-d
  2026-10-25 -t 02:30` in Europe/Berlin) asks which (da-6i7l1): a terminal
  lists both full spellings, `-t 2026-10-25T02:30:00+02:00` and `…+01:00`,
  and stores the one picked; datum.day gets them as `DATUM_CHOOSE=` with
  exit 12 and asks with a button each (`asking` in `web/src/lib/dbio.ts`);
  anywhere else the line is refused naming both, nothing written. The
  answer is `TimeArgs::later`, which every build of the line reads. A
  `retime` is asked the same of the time it builds from the first entry it
  makes one of, before it writes, and the answer is every entry's (da-z3fzv)
- a duration is a trailing positional (`datum end work 30m`) or `dur=30m`
  (`1:30`, `2h`, ISO `PT30M` all work). It says how long the thing LASTED,
  so the block ENDS at the entry's time and the span precedes it —
  `datum start posture 4` means posture was happening for the past four
  minutes, and `-t 1400` with `30m` is 13:30–14:00 (the original
  datum's `durationBlocks` view). There is no `--dur` flag. It was
  removed because the three spellings disagreed about which side of
  `-t` the block fell on; `crates/datum-cli/tests/dur_flag_removed.rs`
  keeps it gone, and the two survivors now agree with the reference.
  A full-day time (`-y`, `-d`, `-D`, a `-t` naming a day) takes a duration
  in whole days alone (`outside -D 3d`); `start walk -y 13:00`, `24h` or
  `1d2h` is refused saying so (da-s6ech)
- `-l V` / `--last-state V` records the state being left
  (`user_asserted_last_state`, `tail -A`; `datum check` flags a mismatch).
  `-L V` / `--ensure-last-state V` is the claim: nothing is written, exit
  non-zero naming the state found, unless the field holds V just before the
  entry and nothing has changed it since — read in the insert's own
  transaction, so of racing claims on one database one lands, and never
  spooled. Two synced copies can each grant one (da-ovnf9). v2's `-L`
  recorded: record with `-l`
- `start_time=` / `end_time=` on `add`/`note`/`start`/`switch`/`occur` are
  lifted onto `occur_time`/`dur`, parsed exactly as `-t` is: the
  pair is the block between them, `start_time=S` + a duration ends at
  `S + dur`, `end_time=E` alone on `start` becomes `end -t E` (on `switch
  STATE`, `end -t E -l=STATE`). Never combined with `-t`; nothing of the
  keys stays in `data`

### The command comes first
- A line is `datum <command> <field> …`: `datum switch mood happy`. There is
  no other order and no swap; a word that is not a command is an alias or
  an error (`datum zzz tail` is «unknown subcommand: 'zzz'»).
- A field's own word is its ALIAS (`mood` → `switch mood`, made by its
  spec), so `datum mood happy` logs and `datum mood tail` is that alias's
  line given the word `tail`, never `datum tail mood`.
- Trailing verbs, v2's `changeDatumCommand` (`dataArgs.ts`): on a log line,
  `occur` / `start` / `end` (`stop`) / `switch` in the `dur=` slot, or
  leading the words no slot or remainder takes, change the command and the
  words after it are read again as the new command's. So with `bed` →
  `start sleep`, `datum bed end` ends sleep; `datum occur coffee start`
  starts coffee; `datum mood happy end` ends mood with `-l happy`. A
  remainder keeps the word as text (`datum note jot the end of it`).
  `handle_log_data_args` in `datum-parse`'s `input/data_args`, the alias
  walk in `alias/regions.rs`, and `change_command` in `log_core.rs` (da-z5vek)
- Trailing commands: after an alias, any other command in the same place
  runs on the alias's fields, its state the filter where the command takes
  one, the rest of its data ignored (`command_side` in `alias/regions.rs`,
  da-90pv8). Slots fill first, so a state wins: with `envi` → `switch
  environment` and `outside` → `envi outside`, `datum envi tail` switches to
  «tail», `datum outside tail` is `tail environment:outside`, `datum envi .
  tail` is `tail environment`. `.` takes a slot's default (a switch's state
  true, a start's dur none), and `...` is a `.` for every slot left
  (`datum practice ... tail`). `datum help alias` says it.

### A field name may be the start of one
A name that is no field but starts exactly one names that one, after alias
expansion (da-tldkb, `datum help names`): `datum tail envi` reads
`environment`; one starting several is refused, naming them. Reads resolve
always (`resolve_read_fields` in `cmd.rs`); a write, once `-F` has built
the name, only where it could not create it anyway, which `creation_closed_by`
in `log_core.rs` answers (`Db::creation_closer`: a template with
`auto_setup = false` covering the name, or its literal head `issue__` starting
it, or the database's `auto_setup = false`; no `--setup`), so `occur envi` in an open db still makes `envi`. In an entry's place (`get x`, `get x::state`) a field
wins over a hid, and no field is named as a hid, nor a hid drawn as a field.

### Alias system (built-in, not shell aliases)
Stored in the DB. Resolved in the CLI pre-pass before clap sees argv.
- `datum alias set vbp "note word srcLang=pt pos=verb"`
- Then `datum vbp correr "to run"` expands to `datum note word srcLang=pt pos=verb correr "to run"`

A command's own short names are not these: they sit in its `aliases` in the
help table (`crates/datum-parse/src/help/commands.rs`) — `rt` for `retime`,
`del` for `delete`, `end` for `stop`, and `spec` for `setup`, which kept the
old name as its alias when it was renamed (da-a4ptr, then da-3ie71: «leave
"spec" as a verb which is aliased to setup»). Do not remove it.

## Sync architecture

The relay sync is deleted; p2p «Easy Sync» replaced it. Its core is in
`crates/datum-core/src/db/sync.rs`: every entry change emits a signed op beside
its `history::record` (endpoint key in `sync_local`, HLC, the change's own `raw`
line, its key-level `effect`, its `origin`: user, host, installation, program,
version and the `DATUM_SCRIPT` name, each where known, which `datum history`
shows, da-zjc9w), and an entry's row is the FOLD of its ops in HLC order, so the
same ops in any order give the same row.

### Conflicts and restored rows

Its conflicts are part of the fold (da-05rkv): the `conflicts` column names each
key two ops set without seeing each other's (and a delete against an edit made
without seeing it), derived from the ops alone, so every copy holding them
agrees; an edit that saw every side — `datum conflicts resolve` — settles it on
each copy it reaches. A key's sides are lettered A, B, … in the column's order,
the fold's, so the letters are every copy's (da-07594): `resolve` takes
`KEY=a|b|<value>` picks, and for the keys not picked one of `--use-a | --use-b |
--prefer-a | --prefer-b | --merge` (`datum_parse::combine` over A and B,
`deleted` a key like any other); with no strategy named they keep what the entry
shows, so a bare `resolve` settles and changes no data. Two copies' creates of
one field at one instant are ONE entry in every arrival order (da-vkeg0): a
create's `deps` name all its copy held, as a slug's move's do, so it joins an
entry that had its slug for a stretch whose end it never saw, begun by an op
that never saw it (da-8mh8i), and a retime of either row moves the one entry —
a conflict only when both did.

`db restore --merge` is a sync from a copy frozen at the backup (da-4ldvb): a
backup's entry whose slug or state's instant a live one holds gets a row with a
hid of its own and a `restore` op, the one kind that saw only what its `deps`
name and not its endpoint's earlier ops, so the fold merges the two and derives
their conflict, its side's device `backup`. Two rules are a restored row's
alone: it takes the identity from no row that was not restored, so the live
entry keeps its hid, and as a tombstone it is no part of the entry it went under
— which is `undo` of the restore, exactly, on every copy.

### History and the clock

An entry's history is read from the same ops (`db/sync/versions.rs`, da-s4erq):
a version per op in fold order, a `fork` where a key became contested and a
`join` where an edit settled it, so `datum history`, `why --full` and `undo
--to-version` number the same versions on every copy; `entry_history` is this
copy's undo journal alone. A copy's HLC passes only the ops it has applied, and
a received op more than five minutes ahead of its clock (`MAX_CLOCK_AHEAD`) is
held until the clock catches up, its device named by `sync status`.

### The transport

The hidden `datum sync pipe vector / ops / apply / verify` move ops as JSON
lines over a pipe. The transport is iroh (`crates/datum-sync`, behind
datum-commands' `net` feature, so the CLI's wasm build has no iroh): the iroh
endpoint key IS the db's endpoint key, copies join a sync chain with one-time
invites (`sync chain invite` / `connect`; the invite is a datum.day join link,
`/sync/join#<ticket>`, which `connect`, a browser and the page's prompt bar all
redeem, so a browser can join a chain as well as start one), only members are
served (`sync_peers` is just where they were last seen), and bare `sync` / the
hidden `sync run` swap vectors and ops until neither moves, with every member
this copy has an address for.

`DATUM_SYNC_BIND=<ip:port>` binds one address with no relay or discovery (the
tests), `DATUM_SYNC_RELAY=<url>` one relay and nothing else.

### A session: handshake, rounds, verify

Each copy says what it runs in the handshake — `datum` (the build) and `schema`
on `Hello` and `Welcome`, defaulted so a copy one version back still reads them
— and the other records it (`sync_peers.datum` / `schema`, in `sync chain
list`). A copy behind a member's schema syncs on and warns to upgrade, once per
new version on stderr and for as long as it lasts in `sync status` and `datum
check` (da-3zqyv).

A round carries at most 8,192 ops, the lowest by HLC so each applies whole, and
applies before the next is sent; rounds go on while full, so a big catch-up
finishes in one session and one cut short loses a round (da-ymgiv).
Two members in a session with one copy at once read its vector before either
applies, and send it the same round: a copy that applied a full round it mostly
held already sends `end` for its next vector, and that session ends there, the
other carrying the catch-up (da-3l8ay, a handshake's `ends` says a copy reads it).

A session ends by comparing state hashes: `verify ok`, `verify MISMATCH` when
they differ over the same ops, and with one copy ahead of the other — a reader
holding what a member lacks, which it never relays — «not verified: R is N
changes ahead of this copy», `pending` in `sync status` and no error (da-t3ey2);
a copy whose vector moved between the last round and its hash (a write, another
session) sends `moved <hash>`, and both say «not verified: X changed as it
verified», never a MISMATCH (da-o2arv). The hash is the sum, mod 2^256, of
each row's SHA-256 (da-9ln3s), kept as rows change: each row's part is kept
(`sync_row_hashes`, per build), triggers on `entries` name every row a write
touches (`sync_rows_changed`), and a verify reads those rows again and no
others, so a session that brought 20 ops hashes 20 rows whatever the history.
Two copies verify by it when both handshakes say `sums`; with a copy before
it, both send the ordered hash it knows (`sync pipe verify --ordered`), kept
until an entry changes (`sync_state_hashes`, da-4be5x). A round reads each
endpoint's ops from its `since` on, by the primary key, and no other op.

### Who dials whom

Bare `sync` on a copy a `sync run` serves dials nothing itself: it connects to
the run's socket beside the database, `<db>.sync`, and the run dials every
member at once and answers each session's line, so the endpoint key is bound
once and the traffic is not doubled; with no run listening it dials as before
(da-tltjj).

`sync run` tries a member it failed to reach again after a minute, the gap
doubling to half an hour, dials every member when it starts or its address
changes, dials no member it is already in a session with unless `datum sync`
asks, and a failure the peer's row already says writes nothing (da-u1ul1).

Two copies that dial each other at once keep one session: the higher endpoint
id, welcomed by a peer whose own dial it is serving, closes its dial as busy,
which neither side records, and the lower id's carries both ways (da-vj3ly).
A dial past its handshake carries either way, and the peer's dial reaching it
only then is refused as busy (da-mghh3).
A dial a `datum sync` asked for, closed so, waits the served session out and
dials again, so what it prints is its own verify (da-221fe).

Each side says in its handshake where it last saw the chain's members (`known`
on `Hello` and `Welcome`), and the other keeps what it has not itself seen once
the session has synced, so two copies one inviter admitted dial each other
(da-fly3f); a member with no address yet is named as one by `sync` and `sync
status`, with nothing «to send».

### One at a time: the apply gate, BUSY, the write lock

One session applies into a database at a time, in any process (`<db>.applying`,
da-h31wt): another waits its turn sending BUSYs, a copy applying refuses a new
session as busy and pushes to every member once done, and neither side counts
busy, or giving way to a write, as a failure. datum.day runs one session at a
time, applying or not (da-dwm2y): its CLI runs one process at a time, so a
second session's handshake waited behind the first's verify with nothing sent.
Its dials wait their turn, and a session served while another is open is
refused as busy, never the dial holding the turn; a join refused as busy (two
pages each holding a turn refuse each other) gives up its turn and tries
again, jittered, for 30 s (da-tqg19).

A round's apply goes on at the chunk size the session's last round ended at, and
says how far the session is in all, of what the peer's vector holds past this
copy's (da-dwm2y): each round began at 64 ops again, and counted only itself.

The first serve after a bulk import signs its baselines in chunks sized by the
clock, a BUSY after each, so the peer hears from it within its idle timeout
(da-f7mmn).

A transaction datum opens writes who holds the lock — pid, command, since — to
`<db>.writing` until it lets go, so a write that waits out the lock and does not
spool (above) names its holder, and `sync run` logs its own holds past a second
(`DATUM_SYNC_LONG_HOLD_MS`) with the rows each wrote per table (`db/holder.rs`,
da-hyem1).

### Offline, paused, unsent

A session that failed before it reached the member (the transport's
`Failure::Unreachable`) is recorded as such (`sync_peers.last_error_offline`),
and `sync status` calls the member `offline`, with when it was last seen and
none of the words, which `--json`'s `error` still carries; one that reached it
and then failed is `error`, with them (da-ueb4p).

`sync pause` is this copy's alone (`sync_local.paused_at`): its handshake and
pipe refuse, `sync run` and the page's leader push nothing, and datum.day's root
bar says «sync paused» in every tab until a tap resumes it.

A write warns once a day on stderr (`sync_local.warned_at`) when changes made on
this copy have reached no other live member for over a day, by each one's last
known vector (da-u10rl): another device being off is no reason. Only a member
holding everything counts, so a copy whose only others are filtered readers
never warns, as a lone member never does.

### Keys, the chain and its notices

A copy of the file at another path mints its own key (`sync_local.path`, v44);
two copies found signing with one key are refused by name, and `sync chain
rekey` gives one a new key.

Membership is the signed chain (`db/sync/chain.rs`), and there is one mechanism:
a chain with no admin is OPEN — any member invites, revokes or renames, and
`invite` on a copy in no chain starts one — and the first `admin add` LOCKS it,
after which those acts take an admin key (passkey or key file). A copy starting
a chain while it holds another endpoint's ops — piped in, or its own old key's —
starts afresh as `chain leave` does, and signs every entry again as its own
(da-1flih), so no joiner applies or refuses them by where its apply was cut.

Removing the last admin takes `--allow-unlock` and reopens it; every copy keeps
a notice of the unlock until `sync chain ack unlock`. A notice — a new device, a
new admin, an unlock — not yet acknowledged is a ⚠ in datum.day's root bar, in
every tab, for each database a tab is on: its sync leader reads them with its
setup and carries them in its state (da-8fucg).

A warning is stored once, shown wherever its question is asked, and cleared in
one place (da-nq07y): the notices, a member's newer schema and changes no other
device has for a day (`Db::sync_unsent`) are each read by both `sync status` and
`datum check`, whose findings carry their `domain` (`entries`, `aliases`,
`stats`, `sync`) and are grouped by it; one `ack` clears a notice from both.

An op the chain refuses is a notice too, never an integrity warning (da-0c2ne):
one `refused:<endpoint>` per device however many ops, gone once the chain admits
it. `integrity_warnings` holds timeline problems alone, and `check` derives each
finding's domain and category from its source (`Source` in `check.rs`), so a
sync finding cannot print as `integrity_*`.

### Filtered readers

A reader may be FILTERED: its invite's `caps` or a `role` record carries
`fields` (`--fields a,b`, or `--kit K` resolved to that kit's fields) and `of`.
Every member serving it sends only the entries that belong to those fields — an
op's `field`, a spec's being the field it specifies — one line per entry, an
entry that moved in whole, `{"remove": hid}` for one that moved out, and a
`caught_up` mark that answers the `since` it was given; the reader applies with
no gap wait, drops what falls outside, and its vector is its marks
(`sync_caught_up`, v50). `verify` hashes the filtered set on both sides.

### Revert and purge

`sync chain revert <WHO>` (da-ry11c; `chain purge <WHO>` until da-c9fiy, now
refused with the new name) previews, and with `--yes` writes a `purge` record:
every copy refuses every op that endpoint signed and folds its entries again
without them — what it made goes, its edits and deletes are undone — and revokes
it. There is no unpurge: a fresh invite brings it back (below), though an old
chain's `unpurge` record still folds as it did.

`sync chain purge --data | --all` (da-qzrj9) is the chain-wide hard purge (`db
purge --chain` is `--data`, parsed to the same command, da-2bb7z), an admin act
any member orders in an open chain: a `wipe` record each copy carries out as it
comes to hold it, offline a year or not — every entry op before it scrubbed to a
stub (`applied` = 3: its endpoint, seq and HLC, so no vector waits on a gap and
no seq is signed twice), every entry, history, undo step, warning and stats
cache row gone — a change spooled before it too, which its drain drops and says
so (da-59hwx), or drained before the order came signs again with the HLC it was
queued at (`queued`), which the wipe compares (da-m6c4j); a build before that
reads `hlc` alone and keeps such an op, so its rows differ from a newer copy's
(`verify MISMATCH`, `check sync` clean on both) until the next wipe, updating or
not (da-371m8) — and a sender's `{"purged": …}` line after the order filling the
gaps of a copy behind. `--all` dissolves the chain: the fold takes no record
after it, a copy holding it serves only records and stubs and says «in no sync
chain», and keeps its key and records to pass the order on until an invite or
connect starts it afresh.

The ordering copy writes a backup, `<db>.purge-<time>.jsonl.gz`, before it
purges, and every other copy it reaches one of its own, as a restore's does
(below; da-acwey, vividn: «every member keeps one»). A passkey's order is
written by `sync approve` (datum.day's), which writes the backup then, and
datum.day keeps it in OPFS (da-459uh). The order is the approving copy's: an
approve link opened on another device (`/sync/approve`) backs up what that
device holds, and the copy that asked keeps its own as it receives the order.
The CLI's order, approved elsewhere and finished with Enter, keeps the backup
it wrote before asking (da-xc321).

`db restore <B> --hard --chain` (da-ncmmj) is that `--data` order and then the
backup written on the ordering copy as new ops, signed after it, so every copy
holds exactly the backup once it syncs; plain `--hard` on a chained copy is
refused, naming `--chain` and `--leave` (this copy alone). In a locked chain its
order is an admin's to sign, and its stdin can be the backup: so `--dry-run
--json` names the order as `approve`, an approve link, and the run takes it back
signed (`--approve-link`, `--approval-code`), refusing any but this chain's
restore order — datum.day's passkey (da-sd7xt).

Its order says `restore`. Every other copy a restore's or a purge's order
reaches writes what it held first, `<db>.wiped-<the wipe's HLC>.jsonl` beside it
(`keep_what_was_here`, da-uk23v, da-acwey): an entry made offline before the
wipe is dropped as it syncs, and not lost. A `wipe` notice names the file and
`db restore <it> --merge`, which brings it back, until `sync chain ack`, saying
«purged» or «restored to a backup»; datum.day keeps the file in OPFS. A later
wipe leaves an earlier one's file.

### Leaving, joining and rejoining

A copy belongs to one chain — its apply refuses another chain's records
(da-d8ort). `chain leave` leaves it from any chain: its ops, peers and notices
go, its entries stay — signed again as baselines on the next sync — and it takes
a new endpoint key, being a new device now. `db purge` on a chained copy takes
`--leave`, that departure and then the wipe, `--chain` (above), or `--rejoin`
(da-qjxfx), and with none is refused naming the three, as `restore --hard` is
(da-n0zx1). `--rejoin`: the wipe keeps `sync_local`, `sync_peers` and
`sync_notices`, the relaying members become the copies it is joining through,
and `_meta` `sync_rejoin` holds the highest seq its key signed, which `own_head`
counts as signed until its ops are back — so it takes them back and serves a
member's vector without reading itself as a copy sharing its key, and signs
nothing meanwhile. Refused while a change made here has reached no member.

Redeeming another chain's invite leaves on its own only a copy holding nothing
but its own data (no other live member, no other endpoint's op), keeping its
key.

A revoked device rejoins by a fresh invite to its chain: the `admit` of a listed
endpoint RE-ADMITS it, clearing the revoke, so what it made while out folds in
(da-j8c4l) — and a revert too, so a reverted device rejoins as at a first
connection, every op it made folding back in (da-q4rau).

`chain list` leaves a revoked, reverted or departed member out and counts it,
and `status` a revoked or reverted one, a departed one having no row there at
all (`--show-revoked` shows them, `--json` always carries them), as bare `sync`
and datum.day's sync widget do: the chain's record of it stays (da-73gyz).

A first sync carries the inviter's chain records after its history, so a join
cut short (a reload, a lost connection) leaves the joiner in no chain holding
part of it, or none (`sync_peers.redeemed_at`, da-38eye): `sync chain list` /
`status` then name the copy it is JOINING through, which counts as its peer, and
bare `sync` or datum.day's leader resumes there (da-idw54).

### Admission by the dialer's records

A member admitted since the serving copy last synced is no member of the chain
as that copy holds it (da-2u95o). Asked (`Reply::Prove`, to a dialer whose
`Hello` says it `proves`), the dialer sends its chain's records, and is admitted
when they make it a member: folded with the server's own (`Chain::load_with`),
each signed by its endpoint, and two kinds not read: a genesis, and a record of
an endpoint the server holds as revoked, which only that endpoint's clock dates
before its revoke. A member's are not kept, since the session brings every op
the server lacks.

A reader brings none but its own, so its records are kept (da-91wab): they wait
on whatever entry op of the inviter's the server lacks, and the server serves by
the chain they make meanwhile (`Chain::served`: the handshake, a reader's filter
in what it is sent and in the verify, `sync status`), while which ops apply
stays with the records applied. The waiting records it reads are all of them,
one held for its clock (`MAX_CLOCK_AHEAD`) as well as one held for a missing op,
so a member's admission dated far ahead opens a session before it applies and a
held revoke stops one (da-zo8ak): no hole, since a member is trusted to admit.

A dialer still refused is told so by name, with what fixes it: sync the refusing
copy with the copy that invited the other. datum.day's store does the same
through the hidden `sync pipe records` and `sync handshake --records`.

### The command tree

The tree: `sync`, `status`, `pause | resume`, `diff <WHO>`, `refold`, `chain
invite | connect | leave | list | rename | revoke | revert | ack | rekey`,
`admin list | add | remove`, `daemon install | uninstall | status` (a systemd
user unit / launchd agent running the hidden `sync daemon run`, which is `sync
run` for every chained db `known_dbs` lists; a copy's first `chain invite` /
`connect` keeps a db opened by path in the registry under its absolute path, so
the daemon finds it too (da-zrl7n), and installs it unless
`DATUM_SYNC_NO_DAEMON=1`, and `datum install` and `datum upgrade` restart it onto a new binary; a `connect`
installs it once admitted and holds `<db>.joining` locked for its life, so the
daemon leaves that database alone until the `connect` ends or is killed, and
then finishes a first sync cut short; an `invite` holds the same lock, so a
joiner is served by the `invite` that shows it and not by the daemon — whose
thread already serving the database steps aside on its next tick, da-8sah2, and a foreground `sync run` unbinds
until it ends, da-x5ynh; the
`invite` serves every member meanwhile, as the daemon would, da-dwm2y — and
a second `connect` or `invite` on the database is refused while it is held,
da-apd52; a chained copy with none says so in `sync status`, da-qr7gz).

### Diagnosing a mismatch

A mismatch is diagnosed natively (da-n0ruf, `db/sync/diagnose.rs`). `datum check
sync`, in the default battery too, reads every row against its ops: an entry
whose applied ops fold to a row this copy lacks (`sync_orphan_ops`, an error:
what `db restore --hard` left before it dropped the ops too, da-95y4o), a row
that is not what its ops fold to (`sync_stale_row`, an error: `derived` as
storage writes it, which derives `verb` and `content_hash` itself, and the
`conflicts` column), each with its columns and both values, an entry whose ops
fold to no row storage takes (`sync_unfoldable`, an error naming it and why,
which the check reads on past), and in a chain a row no op covers (`sync_no_op`,
a warning: the next sync signs it). `datum sync refold` writes each stale row
again from its ops and signs none, so a correct copy is unchanged; orphans it
leaves as found, and `datum check --fix` deletes each (da-53whj): its row
written again from its ops and deleted as `datum delete` does, so the signed
`delete` syncs and every copy, one holding the row included, folds the entry
to the same tombstone. One whose identity a live entry holds (a slug logged
again, as a re-import does) it writes again from its ops and signs nothing
(da-t5kxx): merged with that entry, as every copy holding both holds them, since
a delete would take the live entry too. A refold
stays manual, since a row a write left without its op holds
the newer data and loses it: `--dry-run` names each row it would write, every
column old → new, and writes nothing (da-g8wj9).

A copy does the same as it opens when its rows were folded under an older fold
rule (da-sqpjt): `FOLD_RULE` in `db/sync.rs`, stamped in `_meta` `fold_rule` and
bumped by any change to what the same ops fold to, schema change or not —
`the_fold_rule_is_fingerprinted` fails on a change to that code until it is
bumped or recorded as none. A copy on the current rule reads one row. A build
older than the stamp does not fold back as it opens, but each row it writes
lowers the stamp to its rule (a trigger of its connection's, so a daemon or REPL
opened before the newer build stamped too), and the newer build's next open
refolds (da-w13ei).

`datum sync diff <WHO>` asks one member over a second ALPN, `datum/sync-diff/1`,
which `serve` answers after the same handshake and a copy before it refuses:
state hashes, then a hash per row by hid, then for each hid that differs the row
and the ops each side holds (the hidden `sync pipe digest` / `held`, which are
datum.day's store too). It applies nothing on either side and records no
session. The sync tests' `converged` asserts `check sync` clean, so every
scenario that converges is its control.

### The wire crate and datum.day

The session itself is `crates/datum-sync-wire`, generic over a `Store`, and the
handshake's storage side — members, invites, admission — is datum-core's, so
datum.day syncs too: `web/src/lib/sync.worker.ts` runs the wire crate built for
wasm32-unknown-unknown (relay-only iroh) with the CLI as its store (hidden `sync
handshake` / `new-invite` / `endpoint-key` / `record-session`), and
`$lib/sync.ts` elects ONE tab per database to run the endpoint by a Web Lock
(`datum-sync:<db>`), which pushes on every tab's writes (`onAnyChange`).

A session is its copy's only while the file's endpoint key is the one it began
with (da-otz0d): a `db purge --leave`, `chain leave` or `chain rekey` under an
open session makes the file another copy, in no chain, which took the peer's
whole chain at the next round as a copy joining does. So core's `endpoint()`,
where what a session writes learns who this copy is, inside the transaction it
writes in, fails and mints nothing once the key is not the one the session bound
(`Db::sync_session_as`): datum-sync's `bind` binds a native session, and
datum.day's store runs each command as the hidden `sync --as <ENDPOINT> …`.
There too a session's file is at the schema its build writes (da-1tnup): once a
newer datum has migrated it, the session and `sync run` stop naming both
versions, and `sync daemon run` exits, for its unit to start the installed
binary.

`web/tests/sync*.test.mjs` run on a pinned local `iroh-relay`.

## Web app (PWA)

- SvelteKit static adapter — no server-side rendering
- The real `datum` CLI is the only runner: `datum-cli` built for
  wasm32-wasip1, one WASI process per command in `web/src/lib/db.worker.ts`
  (`cli.ts`, `wasiFs.ts`), its SQLite file in OPFS. There is no SQL, parser
  or renderer in TypeScript — every pane, the widget bar (`fields --json`),
  the tail table (`tail --json`'s `columns` and, for its state cell alone,
  that row's three state values), /now (`now --bare`) and the
  gear's «Backups» (`db backup -`, and plans' `db backup --to` into a
  process's `/data/stage`, whose files a `run` hands in and gets back)
  are CLI commands through `run(argv)`. What TypeScript does
  hold is the transport: carrying those files to an S3-compatible bucket
  (`$lib/s3.ts`, SigV4 by aws4fetch, the browser straight to the bucket
  under its CORS), each device under a folder of its own,
  `client-<client id>/`, with its own index, so no two devices write one;
  restore lists every device's folder and the older backups at the top
  (da-help3). A device's tabs take
  backups one run at a time, under the `datum-backups` Web Lock (da-u050c).
  A command a reader runs also carries `--show json` (`echoArgv`
  in `web/src/lib/echo.ts`), so the bottom bar and the REPL PLACE the
  CLI's own action echo — action, hid, id, info, data — instead of
  parsing its printed lines; a command with no echo falls back to them.
- The bottom bar is the page's command line AND its status line
  and the SEAM between the dashboard and the history: an INPUT
  section over an OUTPUT section, `--input-height` (2.9rem) and
  `--output-height` (3.6rem) on the shell. The input is one row — the mark,
  the clock with seconds at every width, `❯`, the box with its time-cursor
  pills, which stand to the RIGHT of the box and never over the `❯`, going
  off the screen's right edge and coming back on the box's own sideways
  scroll (text and pills are ONE strip in one scrollport, `.field` and
  `.run` in Prompt.svelte); the output is the last command's result as one
  section per document (`+N` opens a modal when they do not fit). The row's
  items are centered in the whole input section, and folded a command
  answers with its ACTION words along the bottom of it STARTING AT THE `❯`,
  in the color the CLI paints the tag (`Outcome::hex`, carried as
  `action_color` on `--show json`), fading, or a red `ERROR` that opens the
  output on a tap. UNDER the bar, below the fold, is the newest-first REPL
  (`$lib/BarLog.svelte`).
- THE SCROLLBACK STANDS IN COLUMNS (`$lib/scrollback.ts`) — the aim:
  "the output of a single entry should be narrow enough to comfortably fit
  2 on mobile side by side". As many entries abreast as fit at the width an
  entry's lines WANT (270px, measured), and never fewer than two wherever
  two clear what an entry NEEDS (160px, measured against the width it
  starts being cut at): a 390px phone gets two, a 1280px screen four, a box
  narrower than two columns the single column it always had. The REPL cuts
  nothing to pay for it — it has the vertical room the bar's fixed band has
  not, so a line too long for its column wraps and the brace drops under it
  (`.doc.all` in Doc.svelte); a command whose answer is the CLI's own
  printed text — a table, a drawn day, an error — takes a whole row at the
  width it was printed for. The hairline between commands is the top edge
  of every cell on a row past the first, so two entries side by side share
  one rule and never stand either side of it. `tests/scrollback.test.mjs`
  takes both measurements again against the built app.
- THE BAR MOVES BY GESTURE ALONE — a drag anywhere on it, or a wheel over
  it. The `^` on its top rule is a HINT and not a control (`pointer-events:
  none`, `aria-hidden`): as the author put it, "The ^ button obscures the text… just put a
  little carrat above the input bar to suggest that it can be pulled up."
  HOW FAR OPEN it is, is ONE number of pixels (`reveal` in `$lib/log.ts`)
  that three boxes are sized from — the page itself never scrolls. The
  output opens over the first `--output-height` of it, which is the NOTCH;
  past it the log grows to whatever height the finger leaves it at, up to
  the whole page. A release CARRIES ITS SPEED, at any speed (`rest` in
  `$lib/detent.ts`; there is no fling threshold): it glides on by `speed *
  TAU` (320ms) and may rest anywhere but for three DETENTS — shut, the
  notch, and the ceiling, the bar at the top of the screen. The notch and
  the ceiling each take a glide ending within `DETENT` (24px) of them, and
  a coast never crosses either from above — the SNAG — so a throw down from
  a tall log stops at the notch, the output still open, and a second one
  shuts it; a finger drags through both. BELOW THE NOTCH there is nowhere to
  rest, and there the gesture's direction decides, not its speed: a glide
  ending short of the notch's catch goes to the notch from a gesture
  heading up, and shuts from one heading down. Every settle is one
  ease-out cubic whose duration is set to leave at the finger's own speed.
  A phone starts folded, anything wider open, remembered per device.
  `return to top` sits pinned at the bottom of the screen and Escape does
  the same. NB `touch-action: pan-x` is on `.field` as well as the bar: the
  value resolves only up to the nearest scroll container, so without it the
  browser claims every drag that starts on the box.
- `?db=<name>` names the DATABASE the page runs against (`web/src/lib/database.ts`): the name becomes the CLI's `DATUM_DB` at the
  one seam (`run` in `run.ts`), outside argv so `raw_input` is what was typed
  (da-7xne1), so no caller carries it, and a caller that
  names its own database keeps it — which is what leaves the layout store's
  `--db _internal` alone, layouts being the device's and not the database's.
  Nothing is set at the default, the query is the whole of the state
  (it survives a reload, links and `goto` carry it, nothing is stored), the
  tab is named after it, and `_internal` or a path is refused with a notice.
  The worker persists whatever database a command NAMES — it reads the name
  out of argv and env as the CLI does and opens a handle for `<name>.db` — and the
  cross-tab refresh is keyed by it, on the names `announceChange` already
  carries.
- A burst takes the database files for itself (sync access handles) under
  the `datum-cli-files` Web Lock, so tabs take turns fairly and none is the
  owner. Back-to-back commands share one acquisition: the handles
  and the lock are kept while jobs wait in the worker's queue, and given
  back when it drains or once a burst has held them 250 ms, so a tab beside
  it is never made to wait for the whole burst. What a process reads of a
  `.db` is kept for the next (`cached` in `wasiFs.ts`), across a release
  too while the file's size and change counter are unchanged, and the CLI
  holds SQLite's lock for a connection's life on wasm — a private window
  pays ~1 ms an OPFS call, and a command made hundreds (da-o3sns). The SQLite header's change
  counter (offset 24) decides whether a process wrote, which is what
  `web/src/lib/changes.ts` tells the other tabs. A browser that refuses the
  handles gets a database in memory for the tab, which keeps going back for
  the file and moves its rows across with `db backup -` / `db restore -
  --merge` (a bare restore REPLACES, tombstoning what the backup lacks).
  SQLite runs in rollback-journal mode under the shim (WASI has no WAL); a
  journal left by a killed process is rolled back by the next one, which
  takes SQLite's `unix-none` VFS: its WASI default, `unix-dotfile`, read a
  connection's own lock as another's and so never saw a journal as hot, and
  a write cut short read as «malformed» for good (da-dfmg8).
- Service Worker caches app shell for offline use — the shell, everything
  `$service-worker` lists, and Vite's separate worker bundle (the database
  worker and datum-cli.wasm), which that list does not know about and which
  the `datum-worker-precache` plugin in `web/vite.config.ts` enumerates
  instead. `web/tests/offline.test.mjs` reloads the app with the network
  down and logs an entry, so the precache cannot silently lose a file
  again.
- The page never waits on the CLI to draw (da-bhudz): the first command
  after a deploy can be a schema upgrade, a minute on a big history. The
  shell draws the tree it last drew on this database (`loadShell`,
  `draft.ts`) before asking anything, each widget's last output is laid over
  it, dimmed, until its answer lands — the widget mounts once the bar can
  take a command, so the bar's reads queue first (`layout/Stale.svelte`, in a closed
  shadow root so no locator finds it; `ready()` in the tests waits the
  copies out), and `Waiting.svelte` says «Opening your data…» or
  «Upgrading your data…» once the first answer is a second late — the
  upgrade heard by the worker as the pre-migration backup is created,
  which the worker keeps in OPFS beside the database (da-hse93).
  It stays until the bar is ready, and at any time says «Another
  datum.day tab or window is using your data» while another tab has held
  the files' lock past a second (da-89kom): the one wait with no deadline,
  which a tab the phone froze mid-burst could make forever (suspected,
  not reproduced).
- Background Sync API queues sync when offline
- The page is a LAYOUT TREE, not a grid (`web/src/lib/layout/`):
  panes hold widgets or splits of panes, each pane has tabs and draggable
  borders, and its bar reads maximize · title-or-tabs · `+` · `×`. Two
  MODES (da-vvcxc, `mode.ts`). In normal mode nothing moves the tree: no
  border is a handle, no `+` or `×` is drawn, and the NAME opens what the
  pane IS and what it is saved as (Reset widget, the widget's OWN OPTIONS
  one item each, the layout's acts). LAYOUT MODE is entered by a hold let
  go where it was, or a right click, the menu key or Shift+F10, on the
  OPEN root tab (a hold that moves still reorders, `reorder.ts`), and is
  drawn loud: the page hatched, every outline the accent, a big handle in
  every gap. There a pane's name opens what is DONE to it — Change Widget,
  Rename, Split, Break out, `Widget options ▸` (the normal-mode list, one
  step in), Close — the `+` and `×` are back, the maximize is the bar's
  HANDLE (dragged to an edge and a third of it; the arrows move it), each
  root tab wears a `×`, and the open tab's name opens Change Widget,
  Split, Close tab and the three ways out: Accept (the draft this device
  keeps, which a reload keeps too), Save layout (the database, so every
  synced device), Discard changes. Escape asks which — Esc Esc discards,
  Esc Enter accepts — and so does moving to another root tab with a
  change in hand. There is no `▾` and no lock. One builder, `menu.ts`,
  one order, two ways to draw it — `named` and `arranged`. A HOLD on a
  pane's title in layout mode — the app's one hold, `web/src/lib/hold.ts`
  — is a second door on Change Widget; the `contextmenu` is eaten on the
  title alone, so a reader still gets the browser's menu over a pane's
  content, where text is selected and copied. The root's `+` is drawn in
  either mode.
  A WIDGET OPTION IS SESSION STATE (`session.ts` + `options.ts`):
  the kind declares it in the registry, the menu draws it inline — a
  switch is a `menuitemcheckbox` that turns over without closing the menu,
  a choice a submenu of words — and setting it changes what THIS TAB shows
  and never the layout. So does a widget moving a setting of its own
  (nowview's − / +, the strip's handle): `Layout.svelte`'s `on:change`
  goes to the session too. The one writer to the tree from the widget's
  side is «Save options with layout», which only appears when something
  is held and which the reader has to choose — so «a widget's own
  controls never modify the layout» is true by construction rather than
  by a guard. The
  overlay is the DIFFERENCE from the tree, keyed by layout + tab + widget,
  pruned when a tab closes or its widget is changed, and kept across a
  reload on this device's `localStorage`, never synced (da-ray91);
  `Reset widget` drops it along with writing the kind's defaults.
  The contract is `types.ts`; the pure operations and the canon (no
  single-child split, no empty pane, ratios sum to 1) are `tree.ts` (`tests/tree.test.mjs`, node); the
  widget kinds and their components,
  floors and settings are `registry.ts`; today's page as data is
  `defaults.ts` (a desktop and a phone tree), which a first visit opens as
  the third of three root tabs (da-u7vz8, `fresh` in `shell.ts`):
  `welcome`, a widget saying what datum is and how tabs and widgets are
  handled, `setup`, the setup widget (da-rgyq8; `track` and `field` open it), and `collect`, that page; the one live store is
  `current.ts` (`layout` + `apply`), which the header's save/restore
  controls fill from the draft or the saved layout. `engine.ts`
  owns the epoch and routes a widget-bar press through whichever REPL is
  mounted. `Pane.svelte` inside a frame renders body-only and hands its
  title slot and alarm up to `PaneFrame.svelte`. Tests find a pane by its
  `data-title`; the old grid's area names live on as classes of the cells.
  A saved layout lives in one of TWO stores (`store.ts`): the
  person's own in `_internal`, and whatever a `[[layout]]` block in a kit
  installed into the DATA db — an ordinary `layout` entry carrying its
  `kit_id`, so it syncs to every device of that database. A listing says
  which (`builtin` | `user` | `kit:<name>`); a save shadows a kit's or a
  built-in's name, `Reset layout` goes back to the kit's tree, and
  `designedFor` decides which of two same-named layouts wins on THIS
  device and orders the chooser — it never hides one. Which device this
  is, `mobile` or `desktop`, is decided ONCE, by the screen it first opens
  on, and kept in `localStorage` (`device` in `shell.ts`, da-lsjs9): a
  phone turned on its side keeps its trees, and «Reset default layouts»
  decides again.
  A KIT SHIPS WIDGETS TOO (da-gh37k): a `[[widget]]` is a `command` that
  only reads (`WIDGET_READS`, or a kit command that runs a view) and a
  `row_action` that only logs (`WIDGET_ACTIONS`), refused at install
  otherwise, and no code; kept in the kit's own `kit:<name>` entry, so
  only the database it is installed in offers it (Change Widget, the `kit`
  kind, `KitWidget.svelte`). It draws the command's `--json` rows from
  their `row_cells` in their `row_color`, and a tap runs the row action,
  each `{column}` that row's value.
  A NEW DEFAULT IS OFFERED, ONCE (da-cr0w2): a change to what a first
  visit opens is a sentence appended to `NEW_DEFAULTS` (`defaults.ts`), and
  a device that started before it is asked at boot (`offerDefaults`,
  `shell.ts`) — the sentence, then «Use the new layout», which is «Reset
  default layouts», or «Keep mine». Either is remembered per device; a
  first visit starts on the newest and is never asked.
- The tail pane is `tail -w` (`web/src/lib/TailTable.svelte`):
  its box decides the row count (`tail -n <n>`, n from a hidden probe
  row's height, never from the drawn table, and never under `tail`'s own
  10: a shorter pane scrolls, pinned at the newest, and agrees
  with the REPL's `tail`; nothing is asked until the box is taller than
  the header), scrolling to the top pages
  the rows above in (`tail -n <page> --above <oldest row's hid>`, the
  CLI's bound in the listing's own order, `occur_time ?? create_time`,
  so a timeless row pages in at its create time, da-mhcla, da-2vgj1;
  capped at 300 rows) with the
  reader's row anchored, and a write while scrolled up keeps the place
  and shows `↓ new`. `web/tests/tailwatch.test.mjs` holds it to that.
  It follows the page's selection (da-lqz3j): an entry or an instant not
  held is listed around (`tail --above` / `-t`, then `head --below`) and
  centered, an instant with a white dotted line over the first row after
  it, and the rows under a listing short of the live end page in as the
  reader scrolls down; a range narrows it, `--since` and `-t`, as the field
  pill does; cleared, it goes back to the live end
  (`web/tests/tailfollow.test.mjs`).
  Its STATE cell is the one cell not taken whole from `columns` (`web/src/lib/StateCell.svelte`), and the reason is NOT that the page
  renders it differently — it renders it the SAME. A state transition is
  THREE values in the row — `previous_state`, `state`, `following_state` —
  which on a terminal are the `[LEAD][body][TRAIL]` trio, and the row now
  carries all three PAINTED: `previous_state_cell`, `state_body`,
  `following_state_cell`, which concatenate to the `columns` cell byte for
  byte. The page draws those three in order and adds only what a terminal
  cannot — the state's name on HOVER, and a box it can select. The author's rule:
  "The previous and following states should just have the colors, not the
  full text — just like the CLI. Really mirror the CLI here. The
  rerendering in the web is just for some niceities and being able to
  select." An earlier take drew the edges as WORDS and cost the column its
  alignment (a transitioning row's chip sat 70px right of one that did
  not); a color block is ONE CHARACTER WIDE whatever it stands for, so the
  raggedness has nowhere to come from. ⛔ `white-space: pre` on the edge is
  load-bearing: the CLI's slot is a painted SPACE, and a lone space in a
  `<td>` collapses to zero width with its background set and nothing drawn.
  `web/tests/statecell.test.mjs`.
- The chart widget (`layout/ChartWidget.svelte`, `$lib/chart.ts`, da-xv7ti)
  draws one key of the fields chosen over time, from `tail F --json` a
  field: a number key as a series a field, points and a line of their mean
  per day, week or month; any other key its values counted per bucket,
  stacked, a bar a field. Its fields and keys are `offered` options
  (`layout/types.ts`): choices read as the menu is built from what the
  widget has read (`offers`, `layout/options.ts`), a field set's words
  switches that keep the submenu open. Time is the lived wall clock, as
  below.
- A day is drawn in LOCAL TIME AS IT WAS LIVED (da-1v9gp), the author's
  rule: "if you see 10am somewhere it actually meant it was 10am when it
  was experienced regardless if your timezone, location, or dst has changed
  meanwhile." `dayview` cuts a day on the wall clock of `db.zone_timeline`
  (the zone its entries were logged in, the host's only with none), into
  one `DayviewSpan` per UTC offset (`day_spans`, `commands/dayview.rs`), on
  an axis that is always 00:00 to 24:00. A forward jump leaves clock times
  no span covers, a hatched `.dead-zone`; a backward one lays two spans
  over the same clock times, and the layout gives each a lane
  (`RenderedSpan::lane`), which blocks narrow into as `<path>`s. Each end
  inside the day is labelled `UTC+n`. The SVG is one `g.day-data` per span,
  each a straight axis, so `timeAt` (`$lib/hover.ts`) reads a press against
  the span it is over (`stretchAt`), and each says its clock, `data-offset`
  in seconds east of UTC, which the hover tip tells a mark's time by and
  not the browser's (da-3ctgr). `web/tests/dayjump.test.mjs`.
- The dayview strip is a stack of days, not one band: today is at the
  bottom — the last band of the scroller, or pinned under it by the
  widget's «today frozen at the bottom» option, which is off by default
  and has no button in the pane (da-2hd21) — and a quarter of yesterday
  shows over it, past a 3px seam, with the `+` on today's top. The days
  before it are stacked above, as many
  as the strip's box SHOWS (the height comes from the container,
  the day height is a `dayHeight` prop with a `change` event, and the
  strip keeps nothing in `localStorage`) — but the stack HOLDS a fortnight
  at least, however short the box, which is the past the reader scrolls
  back through (the two numbers were one, so the whole scroll
  range was the single partly-revealed day and the strip moved 38px and
  stopped), one `dayview` render per band through the seam (`bandArgv` in
  `web/src/lib/panes.ts`). Every band asks for the same `--width` and
  `--day-height`, which is what pins `label_width` and so makes the hour
  gridlines of one band land on those of the next; all but today's
  suppress the axis strip with `--time-axis-height 0`. When the band above
  today is not the day before it, a jagged `<button>` between them scrolls
  yesterday back down. A change re-draws today alone: each earlier day on
  screen is checked by one `dayview --digests` run, the SHA-256 of each
  day's picture, and asked for again only when its held picture differs
  (da-efxk5).

## Migration from original datum

The original datum uses CouchDB JSON documents. Backup format:
```json
{ "backupTime": "...", "docs": [...CouchDB docs...] }
```
Each doc has `data.field`, `data.occurTime`, `data.state`, `data.dur`, `meta.createTime`.
Migration: `datum db v2import --from backup.json`

## Running tests

```bash
cargo test              # all crates
cargo test -p datum-core
cargo test -p datum-cli
```

Add tests for new behavior.

## Integration branches

Work too big for one PR, many beads under one epic that should land on staging together, is built on a branch of
its own, `integration/<name>`, so it can move fast, and lands on staging in one PR at the end. The first was the
tutorial (epic da-uzyih, `integration/tutorial`); the rules below are for any such branch. vividn, 2026-10-22:
«shorten the turn around time for integration branch work … we'll do one big PR at the end to merge the integration
branch into staging later, that will go through a deeper check», and «I want this integration procedure to be
generally applicable in the future, not just for this tutorial development».

- **The permit.** A branch is opened by one small PR into staging adding its entry to
  `.forgejo/integration-branches.toml`: the branch, its epic, the web tests its work reaches, and its default review
  level. The file is under `.forgejo/`, so the PR is vividn's to approve, and that approval is the permit:
  `.forgejo/CODEOWNERS` requests his review on any PR changing `.forgejo/` or `.github/`, and staging's rule blocks
  the merge while that request is open, however many other approvals the PR has
  (vividn, 2026-10-22: «like an integration permit or something … it requires approval from me to establish a new
  integration section»). The crane or the mayor then cuts the branch from staging; `integration/*` takes no direct
  pushes from anyone else. When the branch has landed, a second PR removes its entry.
- **CI, in four layers**, all from staging's ci.yml, which every branch inherits when it is cut:
  1. A PR into a listed branch runs the shared light job: the CLI's build, the wasm module, fmt, clippy,
     svelte-check, the Rust tests its diff reaches (the test map), and the web tests its entry lists. Minutes, not
     the web suite's forty.
  2. A push to the branch (the crane's staging sync, a merge) runs the same job on the merged tree.
  3. Each night `integration-nightly.yml` (05:20 UTC) dispatches ci.yml's full job at every listed branch's tip,
     holding nothing; vremya reads the runs and files a red as a bead under the epic.
  4. The final PR into staging runs staging's CI as any PR does.
- Each bead under the epic carries a review level, a label set when it is filed (vividn, 2026-10-22: «for certain
  issues/beads you could flag if they needed a certain amount of reviews»). The default follows what it touches,
  and whoever files it, or vividn, may raise it. A PR whose bead carries no label is treated as `review/canary`:
  - `review/light`, the default for the branch's own files: the PR merges on the branch's CI. `integration/*` needs
    no approval. Its workflow files are NOT protected there (a protected file refuses every push that changes it,
    the crane's syncs of staging included), so a bead that changes `.forgejo/` or `.github/` on the branch is
    `review/hero` and goes to vividn by convention: the `integration/*` rule asks for no approval, so nothing on
    the forge holds such a PR there.
  - `review/canary`, for a change that reaches shared code (app code outside the branch's own files, a crate): the
    PR waits for the canaries' round, and only a fault they find holds it.
  - `review/hero`, for data, sync, storage, security, or whatever vividn marks: vremya approves before it merges.
- The canaries review every PR into the branch, `review/light` too, without holding the merge: their verdict may
  arrive after it («canaries should run anyway, even if the branch gets merged so that future nits can be beaded»).
  vremya also reads what lands, in batches. A fault found after the merge becomes a bead that must close before
  the final PR. A nit becomes a bead under the epic naming its files, and is done by the next bead that touches
  them («scooped into other issues to avoid slowing things down», vividn). A fault is a finding that, left in,
  makes the branch wrong to use (wrong behaviour, lost or corrupted data, a broken build or test, a security
  hole); style, naming, wording and «a better way to do it» are nits.
- Two beads that change the same files get a dependency between them, so their PRs don't collide.
- The crane merges green staging into the branch, a merge commit pushed with no PR, so the final merge has little
  to reconcile. The branch's CI runs on that push; a red is fixed forward, and a conflict goes to a can as a bead.
  Until the crane's sync is confirmed live, a sync is by hand: a PR from `sync/staging-into-<branch>-<sha>`,
  carrying a bead under the epic, of green staging only (the last commit whose push CI passed), merged as a merge
  commit, never a squash.
- The final PR, from the branch into staging, gets the deep check: staging's CI as any PR there (the Rust tests
  its change can reach, by the test map) and the full suites when it lands, the canaries, vremya's full gate over
  the whole diff, and a read of the entire branch.

## Pre-submit

Before opening a PR, run the gate:

```bash
bash scripts/mq-check.sh
```

It runs only what the diff can break: for Rust,
`scripts/check-changed.sh` — `cargo fmt --check`, then clippy, build, tests
and doc tests scoped to the crates you touched plus their workspace
dependents, falling back to the full `scripts/check.sh` on a workspace-root
config change or when the closure is most of the workspace; for `web/`, the
web suite; for docs alone, nothing. Do not run `cargo test --workspace` or
`scripts/check.sh` for a localized change. CI on a PR runs the Rust tests
its change can reach by per-test coverage (`cargo xtask testmap select`
against the `testmap` branch's map of a staging commit, da-fnt3v) — for
`README.md` or a `docs/*.md`, the tests that read the docs (`DOC_READERS`,
da-1nqv4), which docs alone run nowhere else — and
everything when the change is a manifest, a build script, CI, a non-Rust
file under `crates/` or the map is missing or stale; the job log names each
test or the reason. Staging runs the full workspace, and that is the safety
net for what a scoped run cannot reach.

A PR's CI runs no web suite for a change under `crates/` (da-q587o): one that
changes only `crates/`, docs and markdown runs none and only checks that the
CLI still compiles for datum.day (`node web/scripts/build-cli-wasm.mjs
--check`), and one that also changes `web/` runs the web tests its `web/`
change selects. Staging's full run is what tests a crate change against
datum.day. So a change under `crates/` that alters what a command prints or
accepts — its `--json` shape, its text, a flag, an error — looks for that
command's callers in `web/src` (`git grep -n '<command>' web/src`), runs the
web test files that reach them (`DATUM_TEST_ONLY=<a file listing
tests/<file>.test.mjs, one a line> pnpm test`), and changes the web too where
it reads the old shape.

## Building

```bash
cargo build --release -p datum-cli   # produces target/release/datum
```

### Signed releases and rotating the key

Each push to main publishes the prebuilt binaries with a SHA256SUMS signed
by minisign (`.github/workflows/release.yml`, the secret key in the GitHub
`release` environment). `release/minisign.pub` is the public half, built
into every datum by `include_str!`, and `datum upgrade` refuses a release
that key did not sign, trusting no line of SHA256SUMS before (da-0b55k).
A binary trusts only the key it was built with, so a new key ships in a
release still signed by the old one: change `release/minisign.pub` (and
its copies in `web/static/install.sh`, `README.md` and
`web/static/agents.md`, which a test holds to it) in one commit while the
old secret still signs, and switch the secret, made with `minisign -G -W`
(no password: the job cannot type one), only once that release is out. A binary that skips it refuses every later
release, and is reinstalled with install.sh. A test build may trust a
throwaway key (`DATUM_TEST_RELEASE_KEY`, read only under datum-commands'
`test-util` feature); a release build has no such door.

## Web app dev

```bash
cd web && pnpm install && pnpm run dev   # localhost:5173
pnpm test                              # the seam suite (builds first)
pnpm run test:fixtures                 # recapture the CLI-parity fixtures
```

The browser runs `datum-cli` itself, compiled to wasm32-wasip1 by
`pnpm run build:cli-wasm` (pinned wasi-sdk and binaryen, fetched into
`target/toolchains/`) and loaded through `@bjorn3/browser_wasi_shim`. What
the page supplies is what a shell would: argv, stdin, `TZ`, a client id,
`FORCE_COLOR=1` (there is no tty), and `HOME`/`DATUM_DB` inside the
preopened `/data`. The module is NOT committed: a binary cannot
merge, and every CLI merge conflicted every other open CLI PR on it. Each
push to main builds one onto the `wasm` branch (`.forgejo/workflows/wasm.yml`)
— `datum-cli.wasm` plus `SOURCE`, the main sha it is for, one commit per
main commit — and `web/scripts/ensure-wasm.mjs`, the `predev`,
`prebuild`, `precheck` and `precheck:watch` of `web/`, puts one in `web/src/lib/wasm/` like a build product
in `target/`: it keeps the one there if its crates are this tree's, fetches
the `wasm` branch's if this tree's crates are the merge-base's with
`origin/main`, and builds otherwise, printing which and why. Deploy runs on
the push to main and does the same, so a web-only merge fetches and
installs no Rust, and nothing waits for a Rust build. It lives under `src/`
rather than `static/` so the app reaches it as a `?url` import and Vite
emits it content-hashed under `/_app/immutable/`: a runtime artifact must
not have a stable name, because the service worker serves precached names
cache-first and `deploy.sh` marks hashed assets `immutable` for a year, and
together those handed a returning browser the previous deploy's module to
run the current deploy's JavaScript against. `deploy.sh
--check-dist <dir>` is that rule on its own.

Reusing a module built elsewhere is honest because the build is
byte-reproducible: it remaps the absolute paths rustc and clang bake into
the bytes. The fixtures are still committed, and `build-and-test` re-runs
`pnpm run test:fixtures` and fails on any byte of difference.

A branch cut before the module stopped being committed still carries `datum-cli.wasm`, and rebasing it
conflicts once (modify/delete). The fix is `git rm
web/src/lib/wasm/datum-cli.wasm` — no rebuild; nothing commits the module
any more.

`web/tests` drives the built app in a real browser and asserts through
`run(argv)` — the one seam every region of the app goes through.
`tests/parity.test.mjs` compares what a given argv stores in the browser
against what the native `datum` binary stored for the same argv, and
`tests/stdout.test.mjs` what a script of commands PRINTS, byte for byte
after masking hids and write times. Both are hard assertions: the browser
runs the CLI, so there is nothing for an exemption to excuse. Needs a headless browser once: `npx
playwright install --only-shell chromium`.
