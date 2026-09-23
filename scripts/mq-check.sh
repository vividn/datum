#!/usr/bin/env bash
# The merge queue's check, path-gated.
#
# Until 2026-09-15 the queue ran `cargo build --all-targets`, `cargo test
# --workspace` and clippy on this box for EVERY landing, web-only ones
# included — twelve full workspace builds on 09-15 alone, each beside the
# cans' own `scripts/check.sh` run of the same battery — and never ran the
# web suite, so a web failure reached main (19841c4) and was caught by CI
# only after the merge. vividn, 18:15: "make those improvements".
#
# So: the Rust battery runs when Rust changed; the web suite runs when web/
# changed; a docs-only landing runs nothing.
#
# And the Rust battery is scoped to the crates the diff touches plus their
# workspace dependents (`scripts/check-changed.sh`, which falls back to the
# full `scripts/check.sh` on a root-config change or a closure of >=75% of
# the crates). This is the gate every can runs before `ct done`, and the
# full battery made a one-crate change pay ~25 minutes of workspace build
# for 4 seconds of signal (da-kf28). The full workspace still runs where
# the merge is decided: CI on the PR (`.github/workflows/ci.yml`).
#
# The queue's environment may not have node on PATH (it is nvm's, loaded by
# an interactive shell's rc file), so the web half puts it there itself.
set -euo pipefail

cd "$(dirname "$0")/.."

base="${MQ_BASE:-origin/main}"
merge_base="$(git merge-base "$base" HEAD)"
changed="$(git diff --name-only "$merge_base" HEAD)"

rust="$(printf '%s\n' "$changed" | grep -E '^(crates/|xtask/|Cargo\.(toml|lock)$|rust-toolchain|scripts/check(-changed)?\.sh$)' || true)"
web="$(printf '%s\n' "$changed" | grep -E '^web/' || true)"

if [ -n "$rust" ]; then
  echo "mq-check: Rust changed — running scripts/check-changed.sh --base $merge_base"
  bash scripts/check-changed.sh --base "$merge_base"
fi

if [ -n "$web" ]; then
  if ! command -v pnpm >/dev/null 2>&1; then
    node_bin="$(ls -d "$HOME"/.nvm/versions/node/*/bin 2>/dev/null | tail -1 || true)"
    export PATH="${node_bin:+$node_bin:}$HOME/.local/share/pnpm:$PATH"
  fi
  echo "mq-check: web/ changed — running the web check and suite"
  # The suite otherwise reads the box's load average and yields to it, which is
  # right for a can sharing the box and wrong here: the refinery's verdict is
  # the one run that must not wait behind the cans'. So the queue names its own
  # number and everyone else reads the load (da-x38v).
  export DATUM_TEST_CONCURRENCY=6
  (cd web && pnpm install --frozen-lockfile && pnpm run check && pnpm test)
fi

if [ -z "$rust$web" ]; then
  echo "mq-check: nothing under crates/ or web/ changed since $merge_base — nothing to run"
fi
