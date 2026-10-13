#!/usr/bin/env bash
# The test map (da-fnt3v): which tests reach which functions, from ONE
# instrumented run of the suite, so a PR's CI runs only the tests its change
# can reach (`cargo xtask testmap select`). Built on staging, never gating it.
#
#   scripts/testmap-build.sh OUT.json [NEXTEST ARGS…]   (e.g. -E 'test(/x/)')
#
# nextest runs one process per test and passes its binary id and name to the
# target runner, so scripts/testmap-runner.sh gives each test a profile
# directory of its own: no second pass, no cargo per test. The two cohorts are
# ci.yml's two nextest steps. A failing test is still mapped by what it ran.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
out=$(realpath -m "${1:?usage: scripts/testmap-build.sh OUT.json [NEXTEST ARGS…]}")
shift

export CARGO_TARGET_DIR="$root/target/testmap"
export TESTMAP_PROF="$CARGO_TARGET_DIR/prof"
rm -rf "$TESTMAP_PROF"
mkdir -p "$TESTMAP_PROF"
export RUSTC_WORKSPACE_WRAPPER="$root/scripts/testmap-rustc.sh"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER="$root/scripts/testmap-runner.sh"
# What runs outside a test (build scripts, nextest's listing) writes here and
# not into the tree, where an instrumented process drops default_*.profraw.
export LLVM_PROFILE_FILE="$TESTMAP_PROF/.other/%p-%m.profraw"

e2e=(-p datum-commands --test e2e --features e2e-full)
red=0
TESTMAP_COHORT=workspace cargo nextest run --workspace --no-fail-fast "$@" || red=1
TESTMAP_COHORT=e2e-full cargo nextest run "${e2e[@]}" --no-fail-fast "$@" || red=1
[ "$red" = 0 ] || echo "::warning::tests failed in the instrumented run; they are mapped by what they ran"

cargo nextest list --workspace --message-format json > "$TESTMAP_PROF/.list-workspace.json"
cargo nextest list "${e2e[@]}" --message-format json > "$TESTMAP_PROF/.list-e2e-full.json"

env -u RUSTC_WORKSPACE_WRAPPER -u CARGO_TARGET_DIR -u LLVM_PROFILE_FILE -u CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER \
  cargo run -q -p xtask -- testmap index "$TESTMAP_PROF" "$CARGO_TARGET_DIR/debug" "$out"
