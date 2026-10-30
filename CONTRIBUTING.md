# Contributing to datum

datum isn't taking pull requests yet. Issues are welcome: bugs, questions, ideas, and what you wished it did.

Coding agents start at [AGENTS.md](AGENTS.md), which holds the repo's conventions for them and for anyone.

## Man pages

To generate the man pages on disk for distro packaging, run:

```bash,no_run
cargo xtask manpages           # writes man/datum*.1
cargo xtask manpages /tmp/m    # custom output dir
```

The roff is generated from the same `CommandSpec` table that drives
`--help`, so it never drifts from the binary it ships with.

## Pre-submit check

Before opening a PR, run the gate:

```bash,no_run
bash scripts/mq-check.sh
```

It runs what CI would run for your PR, by the same test-map selections, and
prints which tests and why. A Rust change runs `scripts/check-changed.sh
--no-tests`: `cargo fmt --check`, then clippy, build and the doc tests, scoped to
the workspace crates the diff touches plus their transitive workspace dependents
(so a one-line change in `datum-layout` does not rebuild `datum-parity`), or the
whole workspace when a workspace-root config file changes, when nothing
crate-shaped is touched, or when the dependent closure already covers ≥75% of it;
then the tests `cargo xtask testmap select` picks. A `web/` change runs `pnpm run
check` and the files `web/scripts/testmap.mjs select` picks. Where a selection
says the whole suite, or picks more web files than `MAX_LOCAL_WEB_FILES`, the gate
runs your PR's own new and edited tests, says why, and passes, leaving the rest to
CI; `--full` runs the whole web suite and the closure's whole crates, and
`--dry-run` prints what would run. A docs-only change runs nothing here, and on a
PR CI runs the tests that read `README.md` and `docs/*.md` (`DOC_READERS` in
`xtask/src/testmap.rs`). CI runs, on a PR, the tests
its change reaches by per-test coverage, and the full workspace battery on
staging as the safety net; `./scripts/check.sh` is its local mirror when
you want it.

### `cargo xtask check`

The full battery is reachable from the task runner, which additionally
runs the four repo-hygiene gates:

```bash,no_run
cargo xtask check all     # hygiene gates, then fmt, clippy, build, tests, doc tests
cargo xtask check lint    # the four hygiene gates only — compiles nothing, seconds
```

`check all` prints each step, its result, and a closing summary naming
every step that ran — read the output rather than the command name to
know what was covered. Two CI jobs are **not** covered, and `check all`
says so on every run:

- `e2e-full` — the feature-gated E2E cohort
  (`cargo nextest run -p datum-commands --test e2e --features e2e-full`)
- `wasm-check` — the wasm32-wasip1 CLI module
  (`web/scripts/build-cli-wasm.mjs`, the pinned wasi-sdk and binaryen)

A step that cannot run fails the battery. If `cargo-nextest` is not
installed, `check all` exits non-zero and says so; it never substitutes
another command and reports green. (`scripts/check.sh` does fall back to
`cargo test` — deliberate for a convenience script, wrong for a gate.)

The individual gates stay addressable by name for debugging a single
regression: `cargo xtask check no-cfg-any`, `no-r2-stubs`,
`no-legacy-verbs`, `max-file-size`, `no-slug-keyed-writes`. `scripts/check-honesty.sh` is the
hygiene-only battery as a pre-commit hook.

To run the same battery automatically on every `git push`, enable the
tracked pre-push hook once per clone:

```bash,no_run
git config core.hooksPath .githooks
```

`.githooks/pre-push` invokes `scripts/check.sh`; a non-zero exit aborts
the push. Use `git push --no-verify` as an escape hatch (CI will still
gate the merge).

If a known-noisy test trips the pre-submit gate, consult
[`FLAKES.md`](FLAKES.md) — the registry of known-flaky and
brittle-golden tests, with retry / regen guidance and an append
protocol so the catalogue grows when new noise is observed.

## Repository layout

```
crates/
  datum-core/         data model, SQLite, time parsing, field specs
  datum-cli/          the `datum` binary (clap-free; bespoke parser)
  datum-commands/     command-layer crate dispatched by datum-cli
  datum-parse/        argv parser + help-text introspection
  datum-layout/       dayview / nowview layout primitives
web/                  SvelteKit PWA (OPFS + SQLite WASM)
xtask/                workspace task runner (`cargo xtask <task>`)
```
