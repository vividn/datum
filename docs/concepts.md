# Concepts

One-line summaries of the model:

- **Entry** — one row. Has an id (ULID), a slug (`field:time`), a field,
  a verb (storage classifier), an `occur_time`, a `state` JSON value, an
  optional ISO-8601 `dur`, an optional `following_state`, and arbitrarily
  nested `extra_data`.
- **Field** — what is being tracked: `mood`, `coffee`, `weight`,
  `project`. Freeform by default; can be schema-constrained via
  `datum setup <field>`.
- **Action** — the user's command intent at the CLI: `add`, `occur`,
  `start`, `stop`, `switch`. Action is parsed from argv, canonicalizes
  the (state, dur, following_state) triple (`start` ⇒ state=true,
  `stop` ⇒ state=false), and is then discarded — only the triple
  reaches storage.
- **Verb** — the storage classifier derived from (state, dur,
  following_state) by `derive_verb` (datum-parse::verb): `point`,
  `switch`, `block`, `block-switch`, `point-switch`. The CLI verbs
  `add`/`occur`/`start`/`stop`/`switch` map onto these via the truth
  table in `crates/datum-parse/src/verb.rs`.
- **State** — derived from state-bearing entries (`Switch`,
  `BlockSwitch`, `PointSwitch`, `Block` with state=set, `Point` as a
  false-event marker): at any past instant we can reconstruct a
  field's value. `datum now` consults this.
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
  `IGNORED`, and the next `occur`/`start`/`switch`/`track` on it tracks
  it again; so tracked-ness is read off the entry log
  (`is_ignored_state` in `crates/datum-parse/src/shape.rs`,
  `Db::get_tracked_fields` in `crates/datum-core/src/db/state.rs`),
  which is what `datum fields` filters on, what `datum now` hides
  (`now -A` shows them again), and what `datum info` tallies as
  `tracked`/`ignored`. `TRACKED` is a different thing wearing the same
  word: the OFF state `datum track` writes on a field of any verb,
  "declared, nothing logged yet", which reads as `false` everywhere but
  the printed text, where `tail` draws it in grey. It is not what makes
  a field tracked, and no row carrying it is ever required.
- **Slug vs. hid** — `slug` is the human-readable id (`mood:2026-04-29T09:15Z`).
  `hid` is the short hash you see in `datum tail`; both resolve to the
  same entry. `datum tail` also prints an `_n` column: `_1` is the most
  recent entry, `_2` the one before it, and so on, so `datum del _3`
  names the row you just read. Ordinals count every live entry, so a
  field-filtered listing shows gaps — `_3` always means the same row it
  does everywhere else. Two ordinals joined by a dash name the whole run
  between them, so `datum del _2-_10` acts on nine rows in one go
  (`_10-_2` is the same run, `mood:_1-_3` counts inside one field, and a
  range running off the end acts on the rows that exist). The command
  prints one action record per entry it touched, so you see exactly what
  the range caught.
- **Intent** — the original argv is captured and replayed under a
  pinned `parser_version`, so a future parser fix can retroactively
  correct old entries without losing what the user actually typed.

The command `datum track` and the tracked-ness above are not the same
thing, and it is worth keeping them apart. The COMMAND declares a field:
it writes the spec that gives the field its verb, its color and its
states, and one row saying the declaration happened (`datum help
track`). The STATE is the default every logged field already has. A
field you never tracked is tracked; a field you tracked and then
ignored is not. Tracking is a thing you say about a field, ignoring is
a thing you say about a stretch of time.
