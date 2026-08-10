#!/usr/bin/env bash
# Pre-v1 honesty gates (da-8y2q.7).
#
# Runs the repo-hygiene gates (`cargo xtask check lint`) on the
# current tree:
#
#   * no-cfg-any       — locks in da-8y2q.1: no dead `#[cfg(any())]` blocks.
#   * no-r2-stubs      — locks in da-8y2q.3: no `bail!("stub — R2 extract ...")` stubs.
#   * no-legacy-verbs  — locks in da-8y2q.2: no `Verb::{Occur,Start,Stop,Add}`
#                        references outside `migrations.rs` (which still
#                        decodes them from persisted strings).
#   * max-file-size    — gate further growth in monolithic source files.
#   * toolchain-pin    — every Dockerfile's `FROM rust:<tag>` agrees with
#                        rust-toolchain.toml, which is the single source for
#                        the Rust version. A stale tag still builds (rustup
#                        obeys the pin), so nothing else goes red on it.
#
# These compile nothing, so the script stays fast enough for a
# pre-commit hook. It is NOT a pre-submit check: it does not build, test,
# format-check or lint the code. For that use `cargo xtask check all`
# (or `scripts/check.sh`), which runs these gates and then the
# compiling half of CI.
#
# Exits non-zero when any gate regresses.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
exec cargo run -q -p xtask -- check lint
