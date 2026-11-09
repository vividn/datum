#!/usr/bin/env bash
# The pinned cargo-nextest into ~/.cargo/bin, for ci.yml's two installs and
# testmap.yml's, one path (da-0515l). The caller holds ~/.cargo/.ci-install.lock
# (da-nmf83): taking it here would wait on the caller's own hold.
#
# The tarball is read from DATUM_TOOLCHAIN_CACHE first, by its versioned name,
# which the ships prefill read-only (cn-k7t), and on a miss downloaded; either
# way it is the pinned sha256's bytes or nothing is installed, as
# web/scripts/toolchain.mjs reads its tarballs. curl's --retry 3 (1+2+4 s) gave
# up inside a 504 outage of GitHub's release CDN (run #11162); --retry 8 doubles
# its wait to 255 s. It installs by RENAME, never untarring over the binary in
# place, so a job already running the old file keeps its inode (job 1100,
# «Text file busy»), and the temp dir is under ~/.cargo for the rename.
set -euo pipefail
v=0.9.146
sha256=682c21b777c333e96fd532e114d3a5a894e0729ab88d94c0a9f20f8419695428
name=cargo-nextest-$v-x86_64-unknown-linux-gnu.tar.gz
url=https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-$v/$name

if ! cargo nextest --version 2>/dev/null | grep -x "release: $v" >/dev/null; then
    tmp=$(mktemp -d "$HOME/.cargo/.nextest.XXXXXX")
    trap 'rm -rf "$tmp"' EXIT
    pinned() { echo "$sha256  $tmp/$name" | sha256sum -c --status; }
    cache=${DATUM_TOOLCHAIN_CACHE:-}
    if [ -n "$cache" ] && [ -f "$cache/$name" ] && cp "$cache/$name" "$tmp/$name" && pinned; then
        echo "$name from DATUM_TOOLCHAIN_CACHE"
    else
        [ -z "$cache" ] || [ ! -e "$cache/$name" ] || echo "$cache/$name: not sha256 $sha256; ignored, fetching" >&2
        curl -LsSf --retry 8 --retry-all-errors -o "$tmp/$name" "$url"
        pinned || { echo "$url: not sha256 $sha256" >&2; exit 1; }
    fi
    tar zxf "$tmp/$name" -C "$tmp"
    mv "$tmp/cargo-nextest" "$HOME/.cargo/bin/cargo-nextest"
fi
cargo nextest --version
