# Concepts

## Words

datum's data words, defined here and in `AGENTS.md` (one table, kept the same in both). Code
and help that have not moved to a word yet say the old one until they do.

| concept | the word | command | echo | old words (kept as aliases where typed) |
|---|---|---|---|---|
| what a field's entries are | **field kind** | `setup <field> --kind <k>`, `--as <k>` | | shape, verb, `--verb` |
| a point in time | **moment** | `moment` | MOMENT | occur, occurrence, point |
| a state turned on or off | **on/off** | `on`, `off` | ON/OFF | start, end, stop, state |
| a stretch with a length | **period** | none: `on <field> <length>` makes one | PERIOD | block, span |
| one state among several | **change** | `change` | CHANGE | switch, states |
| an untimed, keyed entry | **record** | `record` | RECORD | add, card |
| a value for a whole day | **all day** | `allday` | ALLDAY | |
| untimed data on a timed field, not a field kind | **remark** | `remark` | REMARK | note |
| what an entry holds: a field's state, a moment's quantity | **state** | | | value |
| one written row of data | **entry** | | | record (noun), row, doc |
| writing one | **log** | | | record (verb), write, enter |

The command you type is the word the echo prints; a period alone has no command of its own.
A full-day entry (`-D`, a date and no clock) echoes ALLDAY whatever its kind, and a run of whole
days (`-D 3d`) PERIOD.
The old words still run, as aliases (`occur`, `start`, `stop`, `end`, `switch`, `add`, `card`, `note`,
`--verb`), and a spec still stores `occur`, `start`, `switch` or `add`, shown as its kind, until the
data moves (da-gpz6m).
An update's before-and-after is its **diff**, in its echo and in `--show json`, since `change` is
a command. A command waiting for its field, in datum.day's bar, is a **pending command**.

A remark is no field kind: it adds untimed data to a field that is otherwise timed.

## The model

One-line summaries of the model:

- **Entry** — one written row of data. Has an id (ULID), a slug
  (`field:time`), a field, a stored classifier, an `at`, a `state` JSON
  value, an optional ISO-8601 `dur`, an optional `following_state`, and
  arbitrarily nested `data`.
- **Field** — what is being tracked: `mood`, `coffee`, `weight`,
  `project`. Freeform by default; can be schema-constrained via
  `datum setup <field>`.
- **Command** — what is typed to log an entry: `moment`, `on`, `off`,
  `change`, `record`, `allday` and `remark`. It is parsed from argv,
  canonicalizes the (state, dur, following_state) triple (`on` ⇒
  state=true, `off` ⇒ state=false), and is then discarded — only the triple
  reaches storage.
- **Stored classifier** — derived from (state, dur, following_state) by
  `derive_verb` (datum-parse::verb, still the `verb` column): `point`,
  `switch`, `block`, `block-switch`, `point-switch`. The commands map
  onto these via the truth table in `crates/datum-parse/src/verb.rs`.
- **State** — derived from state-bearing entries (`Switch`,
  `BlockSwitch`, `PointSwitch`, `Block` with state=set, `Point` as a
  false-event marker): at any past instant we can reconstruct a
  field's state. `datum now` consults this.
- **Tracked** — a field's standing claim about its own GAPS. A tracked
  field is one you are actively recording, so a stretch of it with no
  entries is not missing data, it is evidence: it says the events did
  not happen. `datum ignore` withdraws that claim from there on — over
  an ignored stretch a gap says only that you stopped writing things
  down, and says nothing about whether events occurred. That is the
  whole reason `ignore` exists: without it a reader cannot tell "I did
  not do this" from "I was not logging this", and those are different
  facts about a life. Tracked is the default — every field that has
  been logged or declared is tracked until an ignore says otherwise.
- **TRACKED and IGNORED** — the two special `state` values. They are
  states a FIELD is in, carried by ordinary rows in the same `state`
  column every other value lands in — not flags on a row, not columns
  of their own. A field is ignored when its newest state-bearing row is
  `IGNORED`, and the next `moment`/`on`/`change`/`track` on it tracks
  it again; so tracked-ness is read off the entry log
  (`is_ignored_state` in `crates/datum-parse/src/shape.rs`,
  `Db::get_tracked_fields` in `crates/datum-core/src/db/state.rs`),
  which is what `datum fields` filters on, what `datum now` hides
  (`now -A` shows them again), and what `datum info` tallies as
  `tracked`/`ignored`. `TRACKED` is a different thing wearing the same
  word: the OFF state `datum track` writes on a field of any field kind,
  "declared, nothing logged yet", which reads as `false` everywhere but
  the printed text, where `tail` draws it in gray. It is not what makes
  a field tracked, and no row carrying it is ever required.
- **Slug vs. hid** — `slug` is the human-readable id (`mood:2026-04-29T09:15Z`).
  `hid` is the short hash; both resolve to the same entry. `datum tail`
  leads each row with its `ref`: its ordinal, `_1` for the most recent
  entry at or before now, `_2` the one before it, and so on, and `_+1` for
  the nearest entry in the future, `_+2` the one after it, so `datum del _3`
  names the row you just read, or its hid where it has none. Ordinals count every live entry, so a
  field-filtered listing shows gaps — `_3` always means the same row it
  does everywhere else. Two ordinals joined by a dash name the whole run
  between them, so `datum del _2-_10` acts on nine rows in one go
  (`_10-_2` is the same run, `mood:_1-_3` counts inside one field, and a
  range running off the end acts on the rows that exist). The command
  prints one action record per entry it touched, so you see exactly what
  the range caught.
- **Collision** — a log landing on an entry its field holds (the same
  slug, or the same instant) changes that entry by the field's conflict
  strategy, `datum setup <field> --conflict S` (vividn, 2026-11-05):
  `update`, the default, keeps both entries' keys and takes the new value
  where they differ; `replace` keeps the new entry's keys alone; `merge`
  makes a key both hold differently, the state too, a list of both; and
  `refuse` writes nothing and exits 11. These are v2's `preferNew`,
  `useNew`, `merge` and `fail`. A data key may take its own, over the
  field's for that key alone (`setup <field> --key-conflict K=S`). One that
  changes nothing echoes NODIFF and exits 0, and a line's own `--conflict`,
  `--update` or `--merge` wins over the field's and the keys'.
- **Intent** — the original argv is captured and replayed under a
  pinned `parser_version`, so a future parser fix can retroactively
  correct old entries without losing what the user actually typed.

The command `datum track` and the tracked-ness above are not the same
thing, and it is worth keeping them apart. The COMMAND declares a field:
it writes the spec that gives the field its field kind, its color and its
states, and one row saying the declaration happened (`datum help
track`). The STATE is the default every logged field already has. A
field you never tracked is tracked; a field you tracked and then
ignored is not. Tracking is a thing you say about a field, ignoring is
a thing you say about a stretch of time.
