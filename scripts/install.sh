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
#
# Then it installs tab completion for your shell (`datum completions
# install`), which `cargo install` has no hook for. `--no-completions`
# skips that, and a failure there only warns: the binary is what matters.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

completions=1
for arg in "$@"; do
  case "$arg" in
    --no-completions) completions=0 ;;
    *) echo "usage: scripts/install.sh [--no-completions]" >&2; exit 2 ;;
  esac
done

cargo install --path "$repo_root/crates/datum-cli" --force

if [ "$completions" = 1 ]; then
  datum="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin/datum"
  "$datum" completions install ||
    echo "warning: tab completion not installed (datum is); retry with: datum completions install" >&2
fi
