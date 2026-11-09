#!/bin/sh
#
# Install (or upgrade) the `datum` CLI, prebuilt, with no Rust needed:
#
#   curl -fsSL https://datum.day/install.sh | sh
#
# It downloads SHA256SUMS and the binary it names for this machine (Linux or
# macOS, x86_64 or aarch64) from the `latest` release, which .github/workflows/
# release.yml publishes on every push to main, and refuses a binary whose
# checksum does not match. With minisign on PATH it also checks that
# SHA256SUMS is signed by the release key below (release/minisign.pub),
# and refuses it if not; without, it says so and goes on. It installs to
# ~/.local/bin, or $DATUM_INSTALL_DIR, and says if that is not on your
# PATH. Then, as scripts/install.sh does after its build, the binary sets
# itself up: `datum install`, which takes this script's arguments (`curl
# -fsSL https://datum.day/install.sh | sh -s -- --no-completions`). Once it
# is installed, `datum upgrade` upgrades it, and checks the signature
# always. `datum --version` names the commit it was built from.
#
# To build from a checkout instead: docs/install.md.
set -eu

release="${DATUM_RELEASE_URL:-https://github.com/vividn/datum/releases/download/latest}"
dir="${DATUM_INSTALL_DIR:-$HOME/.local/bin}"
key=RWRF2CkFpZMRWdty5S4larI59ahqK7RzW+XkOHQ/M+Fzwn8tJKXvgBSp

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
# Every download tries again (--retry 5: a timeout, 408, 429 or 5xx, about
# 31 s in all), as `datum upgrade`'s curl() does: GitHub answered 504 for one
# asset, the latest SHA256SUMS, on and off for hours on 2026-11-09 (da-dbe3g).
# A 404 still fails at once.
curl -fsSL --retry 5 -o "$tmp/SHA256SUMS" "$release/SHA256SUMS" || fail "could not download $release/SHA256SUMS"

# SHA256SUMS names the build's own binary, datum-<arch>-<os>-<sha7>, and is
# signed by that build's SHA256SUMS-<sha7>.minisig: a release cut short, or a
# cache serving the previous SHA256SUMS, gives the previous build's, still
# there (release/publish.sh). One from before build names lists only
# datum-<arch>-<os>, read where no build name is, under SHA256SUMS.minisig.
line=$(awk -v f="^$asset-[0-9a-f]+\$" '$2 ~ f { print $1, $2; exit }' "$tmp/SHA256SUMS")
[ -n "$line" ] || line=$(awk -v f="$asset" '$2 == f { print $1, $2; exit }' "$tmp/SHA256SUMS")
[ -n "$line" ] || fail "SHA256SUMS lists no $asset; nothing installed"
want=${line% *}
name=${line#* }
sig=SHA256SUMS.minisig
[ "$name" = "$asset" ] || sig="SHA256SUMS-${name#"$asset"-}.minisig"
asset=$name

if command -v minisign > /dev/null; then
  curl -fsSL --retry 5 -o "$tmp/SHA256SUMS.minisig" "$release/$sig" ||
    fail "SHA256SUMS is not signed: could not download $release/$sig; nothing installed"
  minisign -Vq -m "$tmp/SHA256SUMS" -P "$key" ||
    fail "SHA256SUMS's signature does not verify with the release key; nothing installed"
else
  echo "minisign is not installed, so SHA256SUMS's signature was not checked (by hand: minisign -Vm SHA256SUMS -x $sig -P $key)" >&2
fi

curl -fsSL --retry 5 -o "$tmp/$asset" "$release/$asset" || fail "could not download $release/$asset"
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
