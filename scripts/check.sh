#!/usr/bin/env bash
#
# Pre-submit check — run the same battery as `.github/workflows/ci.yml`
# locally before pushing. Polecat sessions invoke this before submitting
# to the merge queue (replaces unavailable branch protection on a private
# repo); contributors should run it before opening a PR.
#
# Mirrors CI exactly:
#   * build-and-test job: cargo build, nextest run, doc tests
#   * lint job:           cargo fmt --check, cargo clippy -D warnings
#
# Falls back to `cargo test` when cargo-nextest isn't installed so the
# script stays usable on a fresh checkout. CI itself always uses nextest.

set -euo pipefail

# Resolve repo root regardless of where the script is invoked from.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

step() {
    printf '\n\033[1m== %s ==\033[0m\n' "$1"
}

step 'cargo fmt --all -- --check'
cargo fmt --all -- --check

step 'cargo clippy --workspace --all-targets -- -D warnings'
cargo clippy --workspace --all-targets -- -D warnings

step 'cargo build --workspace --all-targets'
cargo build --workspace --all-targets

if command -v cargo-nextest >/dev/null 2>&1; then
    step 'cargo nextest run --workspace --no-fail-fast'
    cargo nextest run --workspace --no-fail-fast
else
    step 'cargo test --workspace  (cargo-nextest not installed; falling back)'
    cargo test --workspace
fi

step 'cargo test --workspace --doc'
cargo test --workspace --doc

printf '\n\033[1;32m✓ all pre-submit checks passed\033[0m\n'
