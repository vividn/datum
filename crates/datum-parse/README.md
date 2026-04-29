# datum-parse

Pure-Rust parsing primitives for the `datum` CLI. The crate is split out
from `datum-cli` and `datum-core` so the parser, the intent IR, and the
time primitives can be reused by the web app without dragging in
SQLite, reqwest, or other native-only dependencies.

## Shape

| Module      | What it owns                                                       |
|-------------|--------------------------------------------------------------------|
| [`time`]    | Date/time/duration parsing, [`DatumTime`](time::DatumTime), [`PrecisionUnit`](time::PrecisionUnit), uncertainty. |
| [`input`]   | Argv primitives — `flexible_positional`, `consolidate_keys`, `data_args`. Direct port of datum's `src/input/*.ts`. |
| [`parse`]   | The bespoke argv parser. Owns the `datum` binary's CLI surface; emits an [`intent`] tree. |
| [`cli`]     | The intent IR ([`Cli`](cli::Cli), [`Command`](cli::Command)) consumed by `datum-commands`. |
| [`alias`]   | DB-backed and spec-generated alias expansion. Pre-pass that runs before [`parse_argv`](parse::parse_argv); pluggable via [`alias::Resolver`]. |
| [`help`]    | Hand-authored help rendering — both the global `--help` and per-command pages. |
| [`error`]   | The crate-wide [`ParseError`](error::ParseError) and [`Result`](error::Result) alias. |
| [`intent`]  | Sub-trees that combine into [`cli::Cli`] (verbs, fields, time flags, data args). |

## CommandSpec model

The parser is data-driven: each `datum` subcommand is a `CommandSpec`
declaring its positionals, flags, and data-arg shape. Adding a new
subcommand is a matter of:

1. Add a `CommandSpec` constant to the relevant module under [`parse`].
2. Extend the [`cli::Command`] enum and the dispatcher in
   `datum-commands` to handle it.
3. Add help copy in [`help`](help).
4. Add a parse test in `tests/parse_<command>.rs`.

There is no clap derive layer to fight; flag-vs-positional, field-first
vs verb-first, and `-y`/`-q`/`-d`/`-t` time stacking are all handled
in [`input`](input) and [`parse`](parse) directly.

## Adding a new flag

1. Update the relevant `CommandSpec` to declare the flag (long name,
   short, value shape).
2. Add a field to the corresponding intent struct in [`intent`](intent).
3. Wire it into [`cli::Cli`] / [`cli::Command`].
4. Cover the parse and a happy-path dispatch test.

The downstream `datum-commands` crate sees only the intent tree, so
the parser change and command change can be reviewed independently.

## WASM-safety

The crate builds for `wasm32-unknown-unknown` when default features are
disabled:

```bash
cargo build --target wasm32-unknown-unknown -p datum-parse --no-default-features
```

The `native` feature (default-on) enables [`DatumTime::now()`](time::DatumTime::now)
via `iana-time-zone`. WASM callers must supply a timezone explicitly.

## Where to look next

- [`datum-commands`](../datum-commands/) consumes the intent tree.
- [`datum-core`](../datum-core/) re-exports [`time`] so consumers can
  write `datum_core::time::DatumTime` without depending on this crate
  twice.
- `docs/parity-audit/parsing.md` for the datum-Node compatibility map.
