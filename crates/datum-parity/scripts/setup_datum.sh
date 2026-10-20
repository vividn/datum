#!/usr/bin/env bash
# setup_datum.sh — clone vividn/datum's v2 at the pinned SHA and install deps.
#
# The pin is the first v2 commit that reads its field specs from
# $DATUM_SPECS (`runtime` points it at specs/my_specs.toml). It is on the
# forge's v2 branch, which is private: CI clones it with the job's token
# (parity.yml), a developer with their own access. Once v2 is on GitHub,
# REPO_URL can be GitHub's.
#
# Used by `--features parity-runtime` tests in this crate. CI must run
# this once before invoking the gated test suite.
#
# The pinned SHA must match `DATUM_PINNED_SHA` in `src/lib.rs`, so a stale
# clone can't silently regress goldens. `pinned_sha_agrees_with_setup_script`
# in `src/lib.rs` reads this file and enforces that.

set -euo pipefail

PINNED_SHA="d866811f3438f82b3c8c5887a3b0454b659e946c"
REPO_URL="https://git.vividn.dev/vividn/datum.git"
REPO_BRANCH="v2"
# The pnpm major that wrote the lockfile at PINNED_SHA. `--frozen-lockfile`
# compares the lockfile against what the *running* pnpm believes the config
# should be, so this pin is half of the SHA pin, not a nicety.
PNPM_MAJOR="11"

# Resolve workspace root from this script's location:
# crates/datum-parity/scripts/setup_datum.sh → workspace root is ../../..
# Physical paths (`pwd -P`): the repository test below compares against
# git's own `--show-toplevel`, which resolves symlinks.
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd -P)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd -P)"
CACHE_DIR="$WORKSPACE_ROOT/target/parity-cache"
DATUM_DIR="$CACHE_DIR/datum"

mkdir -p "$CACHE_DIR"

# Test that $DATUM_DIR IS a repository, not that it has a `.git` (da-3p8xi).
# A `.git` git does not recognise (rust-cache restored one with no HEAD) makes
# `git -C` walk up to the enclosing workspace, so every command below ran
# against daydumb's own repo.
if [ "$(git -C "$DATUM_DIR" rev-parse --show-toplevel 2>/dev/null)" != "$DATUM_DIR" ]; then
    echo "[setup_datum] cloning $REPO_URL → $DATUM_DIR"
    rm -rf "$DATUM_DIR"
    git clone --single-branch --branch "$REPO_BRANCH" "$REPO_URL" "$DATUM_DIR"
fi

# A cache cloned from an earlier REPO_URL fetches from this one.
git -C "$DATUM_DIR" remote set-url origin "$REPO_URL"
echo "[setup_datum] fetching origin $REPO_BRANCH"
git -C "$DATUM_DIR" fetch --quiet origin "$REPO_BRANCH"

CURRENT_SHA="$(git -C "$DATUM_DIR" rev-parse HEAD)"
if [ "$CURRENT_SHA" != "$PINNED_SHA" ]; then
    echo "[setup_datum] checking out pinned SHA $PINNED_SHA (was $CURRENT_SHA)"
    git -C "$DATUM_DIR" checkout --quiet "$PINNED_SHA"
fi
CURRENT_SHA="$(git -C "$DATUM_DIR" rev-parse HEAD)"
if [ "$CURRENT_SHA" != "$PINNED_SHA" ]; then
    echo "[setup_datum] ERROR: $DATUM_DIR is at $CURRENT_SHA after checkout, not $PINNED_SHA" >&2
    exit 1
fi
# The last git command is above. CI hands it the forge token through these
# (parity.yml); no install script below needs them, so none inherits them.
unset GIT_CONFIG_COUNT GIT_CONFIG_KEY_0 GIT_CONFIG_VALUE_0

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
