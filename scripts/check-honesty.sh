#!/usr/bin/env bash
# Pre-v1 honesty gates (da-8y2q.7).
#
# Runs every `cargo xtask check ...` subcommand on the current tree:
#
#   * no-cfg-any       — locks in da-8y2q.1: no dead `#[cfg(any())]` blocks.
#   * no-r2-stubs      — locks in da-8y2q.3: no `bail!("stub — R2 extract ...")` stubs.
#   * no-legacy-verbs  — locks in da-8y2q.2: no `Verb::{Occur,Start,Stop,Add}`
#                        references outside `migrations.rs` (which still
#                        decodes them from persisted strings).
#   * max-file-size    — gate further growth in monolithic source files.
#
# Exits non-zero on the first regression so CI / pre-commit hooks can
# short-circuit on failure.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
exec cargo run -q -p xtask -- check all
