#!/bin/sh
#
# Install (or upgrade) the `datum` CLI, prebuilt, with no Rust needed:
#
#   curl -fsSL https://datum.day/install.sh | sh
#
# It downloads this machine's binary (Linux or macOS, x86_64 or aarch64)
# and SHA256SUMS from the `latest` release, which .github/workflows/
# release.yml publishes on every push to main, and refuses a binary whose
# checksum does not match. It installs to ~/.local/bin, or
# $DATUM_INSTALL_DIR, and says if that is not on your PATH. Then, as
# scripts/install.sh does after its build, the binary sets itself up:
# `datum install`, which takes this script's arguments (`curl -fsSL
# https://datum.day/install.sh | sh -s -- --no-completions`). Once it is
# installed, `datum upgrade` upgrades it. `datum --version` names the
# commit it was built from.
#
# To build from a checkout instead: docs/install.md.
set -eu

release="${DATUM_RELEASE_URL:-https://github.com/vividn/datum/releases/download/latest}"
dir="${DATUM_INSTALL_DIR:-$HOME/.local/bin}"

fail() {
  echo "datum install: $*" >&2
  exit 1
}

case "$(uname -s)" in
  Linux) os=linux ;;
  Darwin) os=macos ;;
  *) os= ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) arch= ;;
esac
[ -n "$os" ] && [ -n "$arch" ] ||
  fail "no prebuilt datum for $(uname -s) $(uname -m); build it from a checkout: https://github.com/vividn/datum/blob/main/docs/install.md"
asset="datum-$arch-$os"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for file in "$asset" SHA256SUMS; do
  curl -fsSL -o "$tmp/$file" "$release/$file" || fail "could not download $release/$file"
done

want=$(awk -v f="$asset" '$2 == f { print $1 }' "$tmp/SHA256SUMS")
[ -n "$want" ] || fail "SHA256SUMS lists no $asset; nothing installed"
if command -v sha256sum > /dev/null; then
  got=$(sha256sum "$tmp/$asset")
else
  got=$(shasum -a 256 "$tmp/$asset")
fi
got=${got%% *}
[ "$got" = "$want" ] ||
  fail "$asset does not match SHA256SUMS (sha256 $got, expected $want); nothing installed"

mkdir -p "$dir"
chmod +x "$tmp/$asset"
mv -f "$tmp/$asset" "$dir/datum"
echo "installed $("$dir/datum" --version) at $dir/datum"
"$dir/datum" install "$@" ||
  echo "warning: datum is installed, but did not set itself up; retry with: $dir/datum install" >&2

case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "$dir is not on your PATH; add it to your shell's startup file: export PATH=\"$dir:\$PATH\"" ;;
esac
