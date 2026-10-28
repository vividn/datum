#!/bin/sh
# Builds the release `datum` at this checkout and prints the commit and its sha256 (da-8npnw).
# The release-sha256 workflow runs it at every push to main, and whoever installs datum as root runs
# it on the box: the same commit must print the same numbers on both.
#
# A release build keeps host paths: cargo's home (dependencies' panic locations), the toolchain's
# library sources, and the checkout for anything absolute under it (build scripts' OUT_DIR). The
# remaps write them as /cargo, /rustc/<rustc's commit> and /src, so the binary names neither the
# account that built it nor where; the C compiler's map (SQLite is compiled in) keeps it so, though
# no C source path is in the binary today. What it cannot take out is the C compiler and glibc
# (the crt objects are linked in), so it prints their versions too: two builds agree only on the
# same ones, and the workflow builds on trixie.
#
# `datum --version` holds the commit height, which a shallow clone cannot count: build in a clone
# with its whole history, and a clean tree (a dirty one's version says `+`).
set -eu
# -P: the physical path, which cargo sees, so a checkout reached through a symlink is remapped too.
cd -P "$(dirname "$0")/.."
dirty=$(git status --porcelain)
test -z "$dirty" || { echo "release-sha256.sh: the tree is not clean" >&2; exit 1; }
unset CARGO_TARGET_DIR
sysroot=$(rustc --print sysroot)
rustc_commit=$(rustc -vV | sed -n 's/^commit-hash: //p')
# The pipe's status is sed's, so a failed rustc -vV shows only as an empty commit, which this stops on.
test -n "$rustc_commit"
cargo_home=${CARGO_HOME:-$HOME/.cargo}
RUSTFLAGS="--remap-path-prefix=$cargo_home=/cargo --remap-path-prefix=$PWD=/src"
RUSTFLAGS="$RUSTFLAGS --remap-path-prefix=$sysroot/lib/rustlib/src/rust=/rustc/$rustc_commit"
CFLAGS="-ffile-prefix-map=$cargo_home=/cargo -ffile-prefix-map=$PWD=/src"
RUSTFLAGS=$RUSTFLAGS CFLAGS=$CFLAGS cargo build --release --locked -p datum-cli
cc --version | head -n 1 && ldd --version | head -n 1
echo "commit $(git rev-parse HEAD)"
sha256sum target/release/datum
