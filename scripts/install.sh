#!/usr/bin/env bash
#
# Install (or upgrade) the `datum` CLI from this checkout.
#
# A `--path` install always builds and installs, replacing the `datum`
# cargo installed before, whatever its version or source (another path,
# `--git`), so this doubles as the upgrade: `git pull && scripts/install.sh`.
# `datum --version` names the commit it was built from. No `--force`: the
# only case it changes is a `datum` in the install root that cargo did not
# put there (copied by hand, or another package's), which cargo refuses to
# overwrite rather than clobber — remove that file and run this again.
# `--locked` builds exactly the versions in `Cargo.lock`, the ones CI
# tested, and fails if the lockfile is out of date instead of re-resolving.
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

cargo install --path "$repo_root/crates/datum-cli" --locked

if [ "$completions" = 1 ]; then
  datum="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin/datum"
  "$datum" completions install ||
    echo "warning: tab completion not installed (datum is); retry with: datum completions install" >&2
fi
