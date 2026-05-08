# datum-commands

The command layer between argv parsing ([`datum-parse`](../datum-parse/))
and the data model ([`datum-core`](../datum-core/)). Every `datum`
subcommand has a module here; the binary lives in
[`datum-cli`](../datum-cli/) and is mostly just a `main()` plus the
dispatcher.

Splitting commands out of `datum-cli` keeps the binary crate small and
gives integration tests a typed API surface they can call directly,
instead of round-tripping through argv.

## Shape

| Module           | What it owns                                                  |
|------------------|---------------------------------------------------------------|
| [`args`]         | Typed argument structs, one per command. Doubles as the typed entry point until R2 lands a bespoke parser per command. |
| [`commands`]     | One module per subcommand (`add`, `switch`, `tail`, `dayview`, …). Each exports a `run`/`exec` function the dispatcher calls. |
| [`commands::sync`] | Sub-tree for `datum sync` (push/pull/status/setup). Wraps [`datum-sync-client`](../datum-sync-client/). |
| [`cmd`]          | Dispatcher: takes a [`datum_parse::Cli`] and maps it onto the right `commands::*::run`. |
| [`config`]       | XDG path resolution, db registry, `datumrc.toml` load/save. |
| [`render`]       | Entry/table output formatting. The single source of field-cell rendering shared between the info-line and tail table. |

## How dispatch works

1. The CLI binary calls `datum_parse::parse_argv` and gets a
   [`datum_parse::Cli`].
2. [`cmd::dispatch`] matches on [`datum_parse::Command`] and calls into
   the right module under [`commands`].
3. Each command module:
   - takes a typed args struct from [`args`],
   - opens or accepts a [`datum_core::db::Db`],
   - performs the operation (read or write),
   - hands the result to [`render`] for terminal output.

Commands never re-parse argv and never speak HTTP directly — sync
commands delegate to [`datum-sync-client`](../datum-sync-client/) for
that.

## Adding a new command

1. Add the parser side in [`datum-parse`](../datum-parse/) — a
   `CommandSpec` plus a [`datum_parse::Command`] variant.
2. Add an args struct to [`args`].
3. Add `commands/<name>.rs` exporting `pub fn run(args, db) -> Result<…>`.
4. Wire the variant into [`cmd::dispatch`].
5. Add a render path if the output is non-trivial.
6. Test the typed API directly; don't shell out for unit tests.

## Test helpers

The `test-util` feature re-exports the same fixtures as
[`datum-core`](../datum-core/) plus a `TestPrompter` for commands that
read user input. Consumed by `datum-cli/tests/*` to spin up an
in-process CLI without a subprocess.

## Where to look next

- Top-level [README](../../README.md) for the user-facing command list.
- `docs/parity-audit/commands.md` for the datum-Node compatibility map.
