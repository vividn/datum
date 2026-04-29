# datum-cli

The `datum` user-facing binary. The crate is intentionally thin: parsing
lives in [`datum-parse`](../datum-parse/), commands live in
[`datum-commands`](../datum-commands/), and the data model lives in
[`datum-core`](../datum-core/). All this crate does is:

1. Read argv + env.
2. Run the alias pre-pass.
3. Hand off to [`datum_parse::parse_argv`].
4. Dispatch the resulting [`datum_parse::Cli`] through
   [`datum_commands::cmd`].
5. Format errors for the terminal and set the exit code.

The library target (this `lib.rs`) re-exports the same modules under
their pre-R1 names so existing integration tests keep compiling without
chasing renames; new tests should import from `datum-commands` directly.

## Hot path

CLI startup is part of the user experience. The original
Node.js datum took ~1s; the Rust rewrite is <5ms. Don't add anything
synchronous to the path between `fn main` and the first SQLite open
unless it has been measured. Lazy work belongs after dispatch, never
before.

## Command surface

For the full user-facing command list — verbs, time flags, alias
syntax, output formats — see the **[top-level README](../../README.md)**.
That is the single source of truth for command docs; this crate's docs
exist only for the integration boundaries above.

## Test boundary

`datum-cli/tests/` exercises the full argv → dispatch → SQLite path
in-process via the library re-exports, with the optional `test-util`
feature pulling in the shared fixtures from
[`datum-commands`](../datum-commands/) and
[`datum-core`](../datum-core/). Subprocess tests are reserved for
behaviors the in-process path cannot model (real exit codes, real TTY
attachment, etc.).

## Where to look next

- [Top-level README](../../README.md) — user-facing command docs.
- [`datum-parse`](../datum-parse/) — argv → intent.
- [`datum-commands`](../datum-commands/) — intent → SQLite + render.
- [`datum-core`](../datum-core/) — schema + invariants.
