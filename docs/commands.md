# Commands

`datum help` is the reference: it lists every command, `datum help <command>` (or `datum <command> --help`)
documents one, and `datum help time`, `datum help color` and the other topics cover what spans commands. The
punctuation of the grammar (`,` `,,` `(` `)` `--` `±` `%…%` and the rest) is [grammar-tokens.md](grammar-tokens.md).

The list below is generated from `datum_parse::help::render_markdown(None)`, the table `datum help` answers from.
**Do not edit it by hand**: `cargo xtask readme` regenerates every `<!-- BEGIN AUTOGEN: … -->` region in the docs,
and the `readme_autogen_sections_in_sync` test in `datum-cli` fails CI on drift. Each command's sentence here is
the same one its own `--help` page is titled with; there is one field behind both.

<!-- BEGIN AUTOGEN: COMMANDS -->
## Usage

```
datum [--db NAME_OR_PATH] <command> [args]
```

## Examples

```
  datum moment coffee                 a coffee, now
  datum change mood happy             mood is happy from now on
  datum tail                          the last ten entries
  datum now                           each field's state now
```

## LOGGING

- `moment` — A point in time (alias: occur)
- `allday` — A value for a whole day
- `on` — Turn a field on, until an off (alias: start)
- `off` — Turn a field off (alias: end, stop)
- `change` — Put a field in a new state (alias: switch)
- `record` — An untimed entry, by a key (alias: add, card)
- `remark` — Untimed words on a field, by its create time (alias: note)
- `track` — Track a field in the now view
- `ignore` — Ignore a field in the now view
- `finish` — Finish a field for good: no entry after it
- `flag` — Mark a field for review (lightweight 'come back to this')

## MUTATE

- `edit` — Open an entry in the configured editor
- `update` — Replace keys on an existing entry
- `comment` — Add a comment to an existing entry
- `merge` — Merge values into an entry, collecting a key's into a list
- `rekey` — Rename an entry's data keys, in place
- `retime` — Move an entry's time, its `at` (alias: rt)
- `rename` — Give a field another name, its entries and spec with it
- `script` — Run a script of command lines, stopping at the first that fails
- `delete` — Delete an entry (alias: del)
- `undo` — Undo the last command, all of it
- `redo` — Redo the last undone command (the inverse of undo)
- `revert` — Step entries back, or take back a field's last command

## READ

- `tail` — Show recent entries
- `head` — Show oldest entries
- `get` — Print entries by id or slug
- `grep` — Regex search over entry JSON content
- `history` — Show the full modification trail for an entry
- `why` — Explain the intent behind an entry

## RENDER

- `dayview` — Render a multi-day timeline as SVG/PNG/HTML
- `nowview` — Draw each field's state now, and the time before it, as an SVG
- `now` — Show the current state of tracked fields + terminal nowview band

## ADMIN

- `setup` — Set up a field: its kind, states and look (alias: spec)
- `alias` — Manage aliases
- `check` — Run database consistency checks
- `backup` — Back up a database: one file, or a rotation on a schedule
- `import` — Import entries from JSONL rows, every row or none
- `export` — Print entries as the JSONL rows `import` reads
- `db` — DB-lifecycle (purge, backup, restore, migrate, v2import, config, rename, ops)
- `info` — Show health/metadata for DB(s)
- `config` — Show or manage user configuration
- `completions` — Shell tab completion: print, install or remove the script
- `install` — Set datum up: tab completion, the sync daemon, backups and upgrades
- `uninstall` — Undo `datum install`, remove the binary, keep your record
- `upgrade` — Replace this datum with the newest release
- `meta` — Attach or list protocol-change notes on a field
- `kit` — Install / export / list field-spec kits (TOML bundles)
- `reparse` — Replay an entry's stored raw_input through the current parser

## VIEWS / SYNC

- `count` — Count entries (optionally filtered/grouped/since-bounded)
- `stats` — Count, total duration, and per-state ratio over a time slice
- `ids` — List entry ids (newest-first by default)
- `fields` — List every field: its spec and the states it was logged with
- `states` — A field's declared and logged states, with counts and last use
- `sync` — Sync with the other copies in this database's sync chain
- `conflicts` — Triage sync conflicts (see `datum check` for integrity findings)
- `view` — Execute a named SELECT view and render the rows

## Global Options

- `--db <NAME_OR_PATH>` — Override the active DB
- `--color <WHEN>` — auto|always|never; see `datum help color`
- `--show <MODE>` — default|none|minimal|standard|format|all|json
- `--json` — Shorthand for --show json
- `-A, --show-all` — Shorthand for --show all
- `-o, --format-string <FMT>` — %field%-templated output string
- `-h, --help` — Show this help
- `-V, --version` — Show version
<!-- END AUTOGEN: COMMANDS -->

## PNG output

`-o x.png` on `dayview` and `nowview` needs `rsvg-convert` on PATH (the librsvg package). `-o x.svg` and `-o x.html` need
nothing.

## Man pages

`datum --man` prints the top-level man page to stdout in roff format;
`datum --man <subcommand>` does the same for one subcommand. Useful for
ad-hoc viewing without installing the pages:

```bash,no_run
man -l <(datum --man tail)
```

To generate the pages on disk for packaging, see [CONTRIBUTING.md](../CONTRIBUTING.md#man-pages).
