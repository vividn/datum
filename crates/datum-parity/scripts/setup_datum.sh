#!/usr/bin/env bash
# setup_datum.sh — clone vividn/datum at the pinned SHA and install deps.
#
# Used by `--features parity-runtime` tests in this crate. CI must run
# this once before invoking the gated test suite.
#
# The pinned SHA must match `DATUM_PINNED_SHA` in `src/lib.rs`, so a stale
# clone can't silently regress goldens. `pinned_sha_agrees_with_setup_script`
# in `src/lib.rs` reads this file and enforces that.

set -euo pipefail

PINNED_SHA="f5e7c1b82da18fb6f02d21efea67d40f9cf26f1d"
REPO_URL="https://github.com/vividn/datum.git"
# The pnpm major that wrote the lockfile at PINNED_SHA. `--frozen-lockfile`
# compares the lockfile against what the *running* pnpm believes the config
# should be, so this pin is half of the SHA pin, not a nicety.
PNPM_MAJOR="11"

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
    echo "[setup_datum] ERROR: pnpm not on PATH. Install it: npm install --global pnpm@$PNPM_MAJOR" >&2
    exit 1
fi

if [ ! -d "$DATUM_DIR/node_modules" ] || [ "$DATUM_DIR/pnpm-lock.yaml" -nt "$DATUM_DIR/node_modules" ]; then
    echo "[setup_datum] running pnpm install"
    if ! (cd "$DATUM_DIR" && pnpm install --frozen-lockfile); then
        echo >&2
        echo "[setup_datum] ERROR: the frozen install failed (da-g3za)." >&2
        echo "  The lockfile at $PINNED_SHA was written by pnpm $PNPM_MAJOR.x; you have $(pnpm --version)." >&2
        echo "  A pinned lockfile read by a floating package manager is a pin with one end" >&2
        echo "  loose — that is exactly how this path rotted unnoticed for a year." >&2
        echo "  Fix: npm install --global pnpm@$PNPM_MAJOR" >&2
        echo "  Do NOT pass --no-frozen-lockfile: it discards the supply-chain guarantee" >&2
        echo "  this script just printed a green line about." >&2
        exit 1
    fi
fi

echo "[setup_datum] datum is ready at $DATUM_DIR (SHA $PINNED_SHA)"
