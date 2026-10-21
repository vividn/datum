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
# A sync daemon already installed is restarted onto the new binary.
# `scripts/uninstall.sh` undoes all of it and keeps the record.
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

# An installed sync daemon (`datum sync daemon install`) still runs the old
# binary: restart it on the new one.
if [ -f "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/datum-sync.service" ]; then
  systemctl --user restart datum-sync.service ||
    echo "warning: the sync daemon was not restarted; retry with: systemctl --user restart datum-sync" >&2
elif [ -f "$HOME/Library/LaunchAgents/day.datum.sync.plist" ]; then
  launchctl kickstart -k "gui/$(id -u)/day.datum.sync" ||
    echo "warning: the sync daemon was not restarted; retry with: launchctl kickstart -k gui/$(id -u)/day.datum.sync" >&2
fi

if [ "$completions" = 1 ]; then
  datum="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin/datum"
  "$datum" completions install ||
    echo "warning: tab completion not installed (datum is); retry with: datum completions install" >&2
fi
