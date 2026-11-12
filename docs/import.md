# Importing entries: JSONL rows

`datum import` takes entries kept somewhere else, one JSON object a line (JSONL), and `datum export --jsonl` prints a
database's entries the same way. An export imports back to the same entries, so its rows are examples of what an
import takes: when in doubt, log one entry by hand and export it.

```bash
datum import rows.jsonl            # or: datum import - < rows.jsonl
datum import rows.jsonl --dry-run  # what it would import, writing nothing
datum export --jsonl > all.jsonl   # every entry; or name fields: export --jsonl coffee mood
datum import --schema              # the rows' JSON Schema
```

## A row

```json
{"field": "coffee", "at": "2026-11-10T08:30:00+01:00", "dur": "MOMENT", "data": {"cups": 2}}
{"field": "mood", "at": "2026-11-10 09:00", "state": "happy"}
{"field": "sleep", "at": "2026-11-10 07:10", "state": true, "dur": "PT8H", "following_state": false}
{"field": "work", "at": "2026-11-10 09:30", "state": true}
{"field": "work", "at": "2026-11-10 17:00", "state": false}
{"field": "book", "data": {"title": "Dune"}, "create_time": "2026-11-10T20:00:00Z"}
```

What each kind of entry looks like:

| kind | the row |
|---|---|
| a moment | `"at"`, `"dur": "MOMENT"`; a quantity is its `state`, other values `data` |
| on/off | `"at"`, `"state": true` to turn it on, `false` to turn it off |
| a period | `"at"` its END, `"dur"` how long it lasted, `"state": true`, `"following_state": false` |
| a change | `"at"`, `"state"` the new state (`"happy"`) |
| a record or remark | no `at`: `data` alone, and a `create_time` so a second run finds it |

### The keys

- **`field`** (required): what is tracked. A new field gets a spec as a first log would: a record where none of its
  rows has a time, else the kind its states say.
- **`at`**: when. Required for a timed entry, one with a `state`, a `dur` or a `following_state`, or of a field whose
  kind is timed: a row without it is refused by its line, never put at now. Either a time as `-t` reads it
  (`2026-11-10T08:30:00+01:00`, `2026-11-10 08:30`; `datum help time` lists the forms), or the object export prints,
  `{"utc": "2026-11-10T07:30:00Z", "offset_minutes": 60, "timezone": "Europe/Berlin"}`. A time that names no zone (no
  offset, no `timezone`) is in `--tz ZONE`, else this machine's, and the import says which.
- **`state`**: the state an on/off or change field takes at `at`, or a moment's quantity. Any JSON value.
- **`dur`**: how long it lasted, ENDING at `at`: a string as `dur=` reads it (`PT30M`, `30m`, `1:30`, and `30` is
  minutes), or a JSON number of seconds. `MOMENT` for a moment.
- **`following_state`**: the state after a period's trailing edge. It needs a `dur`.
- **`data`**: every other key of the entry, an object. No key may be named `at`, the entry's time.
- **`verb`**: the stored shape (`point`, `switch`, `block`, `block-switch`, `point-switch`). It is derived from
  `state`, `dur` and `following_state`; given, it must be what they make.
- **`create_time`**: when it was logged. Given, it is the entry's own; omitted, the import's time. An untimed row's
  identity is its field and create time, so give it one if the file may be imported twice.
- **`lat`**, **`lon`** (numbers), **`location`** (a string); **`raw_input`**, the command line that logged it;
  **`extra_metadata`**, **`user_asserted_last_state`** and **`field_structure`**, as export prints them.
- Derived by the import, and **ignored** if given: `hid`, `slug`, `id_structure`, `content_hash`, `client_id`,
  `parser_version`, `modify_time`, `alias_chain`.
- Refused: any other key (a misspelled `time` or `value` would otherwise be lost), a `deleted_at`, and a field of
  datum's own (`spec`, `alias`, …), which `setup`, `alias` and `kit` write. A `timezone` entry's state is an IANA zone
  or `IGNORED`.

## What an import does

- **Every row, or none.** Each row is read first; one refused is named by its line (`error: line 7: …`), nothing is
  written, and datum exits 1. `--allow-partial` writes the rows that read, names each refused, and still exits 1.
- **A second run writes nothing new.** A row already there, the same field at the same time with the same content, is
  skipped. One whose field and time hold other content is kept as it is and named.
- **One `undo`** takes the whole import back, the specs it made with it.
- **A summary per field**: how many imported, how many already there, and which fields are new.
- In a database whose `auto_setup` is off, a row of a new field is refused: `datum setup <field>` first.

## Exporting

`datum export --jsonl [FIELD…]` prints each live entry, or each of the FIELDs', oldest first, leaving out the keys the
import derives and every key with no value. The entries of datum's own fields (specs, aliases, layouts) are not
exported: a field's setup goes with `datum kit export`.
