#!/usr/bin/env bash
#
# Uninstall the `datum` CLI that scripts/install.sh installed, in reverse:
# the sync daemon, tab completion and the shell alias loop, each by
# datum's own uninstall while the binary is still there, then the binary
# itself (`cargo uninstall`, from the install root install.sh used).
#
# It never touches the record: the databases and the config are kept, and
# the last lines say where they are and how to remove them by hand. A step
# that fails or has nothing to do says so and the rest still run, so
# running it twice is harmless.
set -uo pipefail

root="${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}"
datum="$root/bin/datum"

step() {
  local name="$1"
  shift
  echo "== $name"
  "$@" || echo "warning: removing the $name failed; the other steps still run" >&2
}

if [ -x "$datum" ]; then
  step "sync daemon" "$datum" sync daemon uninstall
  step "tab completion" "$datum" completions uninstall
  step "alias loop (zsh)" "$datum" alias uninstall zsh
  step "alias loop (bash)" "$datum" alias uninstall bash
  step "binary" cargo uninstall --root "$root" datum-cli
else
  echo "no datum in $root/bin: nothing to uninstall"
fi

# Where datum reads them (`config.rs`: data_dir_path, config_dir_path).
data="${DATUM_HOME-${XDG_DATA_HOME-$HOME/.local/share}/datum}"
config="${DATUM_CONFIG_HOME-${XDG_CONFIG_HOME-$HOME/.config}/datum}"
cat <<EOF

Kept your record:
  data    $data
  config  $config (dbs.toml there may name databases elsewhere)
To remove it too, by hand: rm -r '$data' '$config'
EOF
