---
name: datum
description: Log and review personal life-data with the `datum` CLI in this repo. Use when recording events, states, or durations (the verbs occur / start / stop / switch / note / add) or reading them back (tail / now / get / grep / dayview). Covers time-flags for backdating, choosing a database, and the data model.
---

# Using datum

`datum` is a local-first life-tracker: every entry is one row in a local SQLite
database, and logging is near-instant. You record *moments and states*, then read
them back as lists, current state, or visual timelines. `datum --help` and
`datum <command> --help` document the full surface.

## Logging — pick the verb by what you're recording

```bash
datum occur coffee                  # a point-in-time event (no duration)
datum switch mood happy             # a field's new state (persists until changed)
datum switch weight 75.2kg          # state stored exactly as typed: "75.2kg"
datum start work project=writing    # begin a duration-tracked activity
datum stop work                     # end the open one (alias: end)
datum note idea spark=svg-timeline  # timeless jot, auto-id'd on create_time
datum add person -K name name=Ada   # timeless keyed record (needs an explicit id)
```

`track` / `ignore` control whether a field appears in `datum now`; `flag` marks a
field to come back to. Structured data is `key=value`, and values are stored as
typed: `75.2kg` stays the string `"75.2kg"`, with no unit parsing.

### Declared keys: positional values fill a field's slots

A field's spec can declare keys, and bare values after the field then fill them in
order, so a log line needs no `key=`:

```bash
datum spec set juice --verb occur --data-key cups --data-key temp=cold
datum juice 2              # cups=2, temp=cold (the default)
datum juice 3 temp=warm    # a key=value still overrides
datum juice                # Error: Missing required key: cups
```

`--data-key K` is required, `K=` optional, and `K=D` has a default. `--id-key K` puts
the value in the entry's id (like `-K`), and `--assign K=V` writes a fixed value on
every log. A new field needs `--verb` (occur | start | switch | add) so its bare
name knows what a press means. `datum spec show <field>` prints what is declared.

### note vs add — timeless data, split by identity (when vs what)

Both `note` and `add` record **timeless** data (no `occur_time`); they differ in
how the entry is identified:

- **`note` = "record this moment, won't look it up."** Identified by *when* —
  auto-ids on its create_time, so its slug is `field:c<create_time>` and many
  notes per field coexist. No id needed. Use it to update a field without
  claiming an occurrence.
- **`add` = "register this keyed thing, will look it up."** Identified by *what* —
  it **requires** an explicit id so you can reference it later. Give one with
  `-K <key>` (derive the slug from a data key), `--id <value>` (set it directly,
  templates like `--id %field%__%srcLang%` allowed), or the `id=<value>`
  positional. A bare `add field` with no id errors descriptively.

`-T` / `--no-timestamp` converts any occurred verb into **note-data**: the data
still lands, but no occurrence is logged and the entry gets a create_time id.
Canonical use — *chore-shirk*: advance a recurring chore's schedule without
claiming you did it. On a state verb it is a note *about* a state: the state is
kept, `dur=MOMENT`, and nothing starts or ends. `-T` with a duration errors.

```bash
datum occur chore -T next_date=+3   # bump the next-due date; logs no occurrence
datum switch environment outside -T # a note about "outside"; the field's state is unchanged
```

`-T` (drop the time entirely → timeless) is distinct from `-D` / `--full-day`
(keep a **date**, drop the clock time → a date-only, still time-anchored entry).

## Backdating / shifting time

An entry logs *now* unless you place it:

- `-y` = 1 day ago, `-yy` = 2 (stackable); `-d -1` = yesterday, `-d +1` = tomorrow.
- `-q` = 5 min ago (stackable); `-t -8` = 8 min ago; `-t 1513` = 15:13 today; `-t 9:15am`.
- `-t '2026-09-28 10:00'`, `-t 'yesterday 10am'`: a date, then a time. `datum help time` lists every form.
- A duration is a trailing positional (`datum end work 30m`) or `dur=30m` — also
  `1:30`, `2h`, ISO `PT30M`. There is no `--dur` flag. It says how long the thing
  LASTED, so the block ENDS at `-t` and the span precedes it: `datum end work 12m
  -t 14:00` is 13:48–14:00 (da-zwct).
- `start_time=` / `end_time=` place a block outright, with the same values `-t`
  takes: `start posture start_time=-20 end_time=-5` (a 15-minute block ending five
  minutes ago), `start posture start_time=4pm 15` (16:00–16:15, the row `-t 16:15
  15` writes). `end_time=` alone on `start` is `end -t E`; on `switch STATE` it is
  `end -t E -L=STATE`. The keys are consumed, never stored, and never combine
  with `-t`.

## Reading it back

```bash
datum tail                          # recent entries
datum tail mood                     # filtered to one field
datum now                           # current state of tracked fields + a terminal timeline band
datum get <id-or-slug>[,<id>...]    # entries by id or slug
datum grep <regex>                  # regex search over entry JSON
datum count --group-by field        # counts, optionally grouped / --since-bounded
datum dayview -o day.svg --days 3   # render days as a timeline (.svg / .png / .html / .json; SVG to stdout by default)
```

## For agents

- **Read machine-readably:** reads take `--json` (short for `--show json`), e.g.
  `datum tail mood -n 5 --json`, instead of parsing the table.
- **Preview a write:** `--dry-run` on a logging verb prints the entry it *would*
  write and writes nothing.
- **`_LAST` is what you last touched, and bare commands act on it.** Every write
  sets it, and so does `get`, by design: query an entry, then `update` or
  `delete` it with no id.
- **Bare `undo` is an editor's undo.** It reverts your last *command* that
  wrote anything — every entry it wrote (a restore, an import, a kit install, a
  comma-list delete is one command) — and the next `undo` the one before;
  `redo` walks forward again, and a new command drops what was undone. Reads
  never move it: after a `get`, a bare `undo` still reverts your last write;
  `datum undo _LAST` (or `<hid>`) steps what you fetched back one version
  instead. An entry another device changed since (a sync) is refused, not
  reverted.
- **Check after bulk work:** `datum check` reports consistency problems, such as a
  state repeated with no change between.
- **Time formats:** `datum help time` lists every form `-t`, `-d`, `-y`/`-q` and
  durations take, with examples.
- **Trying sync? Set `DATUM_SYNC_NO_DAEMON=1` first.** A copy's first
  `sync chain invite` or `connect` installs a background sync service for your
  user (a systemd user unit or launchd agent), even on a throwaway db. If one
  slipped in, `datum sync daemon status` shows it and `datum sync daemon uninstall`
  removes it.

## The data model (essentials)

An **entry** has a `field` (what is tracked), a `state` (JSON value), an optional
ISO-8601 `dur`, an `occur_time`, and a `slug` — the human-readable id, `field:<time>`
for a time-anchored entry or `field:c<create_time>` for a timeless `note`/`-T` one
(the short `hid` in `datum tail` resolves to the same entry, as does the `_n`
ordinal beside it — `_1` is the newest entry, `_2` the one before it). You choose the CLI verb;
datum *derives* the storage classifier (point / switch / block / block-switch /
point-switch) from the (state, dur) it produces.

## Choosing a database

Entries go to the default database unless overridden — `datum --db <name-or-path> …`
or the `DATUM_DB` environment variable. All reads (`tail`, `now`, …) respect the same
selection. Use a throwaway db while experimenting: `datum --db /tmp/scratch.db occur test`.

## Handy

- **Aliases** pre-bake commands: `datum alias set vbp "switch body vertical , occur caffeine"`,
  then `datum vbp` runs the body.
- A whitespace-bounded `,` token logs **multiple entries atomically** with one shared
  timestamp: `datum switch body vertical , occur caffeine`.
- `datum --man <command>` prints a man page to stdout; `datum spec set <field> …`
  shapes a field: its keys (above), allowed states, colours, glyphs and id template.
