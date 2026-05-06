#!/usr/bin/env bash
# setup_datum.sh — clone vividn/datum at the pinned SHA and install deps.
#
# Used by `--features parity-runtime` tests in this crate. CI must run
# this once before invoking the gated test suite.
#
# The pinned SHA must match `DATUM_PINNED_SHA` in `src/lib.rs`. Mismatch
# is rejected loudly so a stale clone can't silently regress goldens.

set -euo pipefail

PINNED_SHA="c8599d84baa6d26a633ce375f0a28bb1f2a9294a"
REPO_URL="https://github.com/vividn/datum.git"

# Resolve workspace root from this script's location:
# crates/datum-parity/scripts/setup_datum.sh → workspace root is ../../..
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
CACHE_DIR="$WORKSPACE_ROOT/target/parity-cache"
DATUM_DIR="$CACHE_DIR/datum"

mkdir -p "$CACHE_DIR"

if [ ! -d "$DATUM_DIR/.git" ]; then
    echo "[setup_datum] cloning $REPO_URL → $DATUM_DIR"
    git clone "$REPO_URL" "$DATUM_DIR"
fi

echo "[setup_datum] fetching origin"
git -C "$DATUM_DIR" fetch --quiet origin

CURRENT_SHA="$(git -C "$DATUM_DIR" rev-parse HEAD)"
if [ "$CURRENT_SHA" != "$PINNED_SHA" ]; then
    echo "[setup_datum] checking out pinned SHA $PINNED_SHA (was $CURRENT_SHA)"
    git -C "$DATUM_DIR" checkout --quiet "$PINNED_SHA"
fi

if ! command -v pnpm >/dev/null 2>&1; then
    echo "[setup_datum] ERROR: pnpm not on PATH. Install via corepack: 'corepack enable'." >&2
    exit 1
fi

if [ ! -d "$DATUM_DIR/node_modules" ] || [ "$DATUM_DIR/pnpm-lock.yaml" -nt "$DATUM_DIR/node_modules" ]; then
    echo "[setup_datum] running pnpm install"
    (cd "$DATUM_DIR" && pnpm install --frozen-lockfile)
fi

echo "[setup_datum] datum is ready at $DATUM_DIR (SHA $PINNED_SHA)"
