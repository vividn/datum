#!/usr/bin/env bash
#
# Install (or upgrade) the `datum` CLI from this checkout: build it, then
# let it set itself up with `datum install` — tab completion, and the sync
# daemon when a database syncs, restarted onto the new binary (`datum help
# install`). Arguments go to `datum install`: `--no-completions`, say.
# `datum uninstall` undoes it all and keeps the record.
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
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Build with the toolchain rust-toolchain.toml pins, named on the command
# line: rustup picks it from the file anyway, but then warns «default
# toolchain implicitly overridden» on every machine whose default differs.
# It is installed first when missing, since rustup's auto-install can be
# off. A cargo without rustup takes no `+toolchain` and builds as it is.
cargo=(cargo)
if command -v rustup >/dev/null; then
  channel="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$repo_root/rust-toolchain.toml")"
  rustup toolchain list | grep -q "^$channel-" || rustup toolchain install --no-self-update "$channel"
  cargo=(cargo "+$channel")
fi
"${cargo[@]}" install --path "$repo_root/crates/datum-cli" --locked
"${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin/datum" install "$@"
