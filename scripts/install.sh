#!/usr/bin/env bash
#
# Install (or upgrade) the `datum` CLI from this checkout.
#
# `datum` is pre-release and not on crates.io, so the workspace version
# stays fixed across builds. `cargo install` skips overwriting a binary
# of the same version, so a plain install silently no-ops once datum is
# already on your PATH. `--force` is the right idiom for a from-source
# pre-release tool: it always overwrites, so this doubles as the upgrade
# command.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

exec cargo install --path "$repo_root/crates/datum-cli" --force
