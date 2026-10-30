#!/usr/bin/env bash
# The merge queue's check, path-gated.
#
# Until 2026-09-15 the queue ran `cargo build --all-targets`, `cargo test
# --workspace` and clippy on this box for EVERY landing, web-only ones
# included — twelve full workspace builds on 09-15 alone, each beside the
# cans' own `scripts/check.sh` run of the same battery — and never ran the
# web suite, so a web failure reached main (19841c4) and was caught by CI
# only after the merge. vividn, 18:15: "make those improvements".
#
# So: the Rust battery runs when Rust changed; the web suite runs when web/
# changed; a docs-only landing runs nothing.
#
# And the Rust battery is scoped to the crates the diff touches plus their
# workspace dependents (`scripts/check-changed.sh`, which falls back to the
# full `scripts/check.sh` on a root-config change or a closure of >=75% of
# the crates). This is the gate every can runs before `ct done`, and the
# full battery made a one-crate change pay ~25 minutes of workspace build
# for 4 seconds of signal (da-kf28). The full workspace still runs on
# staging, whose green commit main is promoted to (`.forgejo/workflows/ci.yml`).
#
# And a can's tests are what CI runs for its PR, read from the same selections
# (da-678rb, vividn, 2026-10-30: «run the smaller subset that ci runs rather than
# everything for PRs in general … full runs should be saved for staging»): the
# test map's picks, the permit's `web_tests` on an integration branch. Where a
# selection says the whole suite, or picks more web files than
# MAX_LOCAL_WEB_FILES, a can runs the PR's own new and edited tests, says why,
# and passes: CI is the verdict and the box is shared. `--full` (or MQ_FULL=1)
# is the whole web suite and the closure's whole crates, which the refinery's
# verdict always runs. `--dry-run` prints what would run and runs no check.
#
# The queue's environment may not have node on PATH (it is nvm's, loaded by
# an interactive shell's rc file), so the web half puts it there itself.
set -euo pipefail

cd "$(dirname "$0")/.."

# Above this many web files a selection is treated as the whole suite: the
# selections come in two groups, 6 to 13 files and 152 to 176 (40 PR runs,
# vremya, 2026-10-30). vividn's call: set it past the file count to turn it off.
MAX_LOCAL_WEB_FILES=20

full="${MQ_FULL:-}"
dry=
for arg in "$@"; do
  case "$arg" in
    --full) full=1 ;;
    --dry-run) dry=1 ;;
    *) echo "usage: mq-check.sh [--full] [--dry-run]" >&2; exit 2 ;;
  esac
done

# Who runs it decides how hard it may press on the box. The refinery's verdict
# (role `<kiez>/alchemiter`) is the one run that must not wait behind the cans',
# so it names its own numbers. A can sharing the box runs the same gate before
# `ct done`, and when it named them too, three cans at once took the 24-core box
# to load 98 (10-29): so a can's web suite reads the load, and its nextest takes
# a quarter of the cores, not all of them (da-5f20z). Anyone else, CI included
# (which never runs this script), keeps the tools' defaults.
role="${CT_ROLE:-${GT_ROLE:-}}"
case "$role" in
  */alchemiter | */refinery) refinery=1; full=1 ;;
  *) refinery= ;;
esac
case "$role" in
  */cans/*) cores="$(nproc)"; export NEXTEST_TEST_THREADS="${NEXTEST_TEST_THREADS:-$(( cores / 4 > 2 ? cores / 4 : 2 ))}" ;;
esac

# Cans branch from staging, which runs ahead of main: diffing against main
# made a one-line change pay for every crate staging touched (da-0k6rq). So by
# default the base is the NEARER of the two: `git merge-base HEAD A B` is the
# merge base of HEAD and a hypothetical merge of A and B. A branch whose fork
# point is on a listed integration branch and not on staging is that branch's
# PR. The tip of one is its final PR, into staging. `pr_base` is what the PR is
# read against, as CI's `origin/$BASE`.
integration=
if [ -n "${MQ_BASE:-}" ]; then
  bases=("$MQ_BASE")
  pr_base="$MQ_BASE"
  ! python3 scripts/integration-permit.py "${MQ_BASE#origin/}" >/dev/null || integration="${MQ_BASE#origin/}"
else
  # CI reads the tips; a selection read against stale refs answers «stale map».
  git fetch -q --no-tags origin +refs/heads/testmap:refs/remotes/origin/testmap \
    +refs/heads/staging:refs/remotes/origin/staging '+refs/heads/integration/*:refs/remotes/origin/integration/*' 2>/dev/null ||
    echo "mq-check: could not fetch; reading the refs this clone has"
  bases=()
  for ref in origin/staging origin/main; do
    git rev-parse --verify --quiet "$ref" >/dev/null && bases+=("$ref")
  done
  [ "${#bases[@]}" -gt 0 ] || { echo "mq-check: no base ref (origin/staging or origin/main) to read the PR against" >&2; exit 1; }
  pr_base="${bases[0]}"
  if [ "$pr_base" = origin/staging ]; then
    for ref in $(git for-each-ref --format='%(refname:short)' refs/remotes/origin/integration/); do
      fork="$(git merge-base HEAD "$ref" || true)"
      [ -n "$fork" ] && [ "$fork" != "$(git rev-parse HEAD)" ] || continue
      [ "$(git rev-list --count "origin/staging..$fork")" -gt 0 ] || continue
      python3 scripts/integration-permit.py "${ref#origin/}" >/dev/null || continue
      integration="${ref#origin/}" pr_base="$ref" bases=("$ref")
      break
    done
  fi
fi
merge_base="$(git merge-base HEAD "${bases[@]}")"
changed="$(git diff --name-only "$merge_base" HEAD)"

rust="$(printf '%s\n' "$changed" | grep -E '^(crates/|xtask/|Cargo\.(toml|lock)$|rust-toolchain|scripts/check(-changed)?\.sh$)' || true)"
web="$(printf '%s\n' "$changed" | grep -E '^web/' || true)"

sel="$(mktemp -d)"
trap 'rm -rf "$sel"' EXIT
[ -z "$integration" ] || echo "mq-check: this is a PR into $integration, read against $pr_base"

# What CI's `cargo nextest` steps run, read from `testmap select`'s lists: its
# picks, or on FULL the tests the PR adds or edits, by name.
rust_tests() {
  git show origin/testmap:testmap.json >"$sel/testmap.json" 2>/dev/null || { rm -f "$sel/testmap.json"; echo "mq-check: no testmap branch"; }
  ${MQ_XTASK:-cargo run -q -p xtask --} testmap select "$sel/testmap.json" "$pr_base" "$sel/rust" ||
    { mkdir -p "$sel/rust"; echo "the selection failed" >"$sel/rust/FULL"; }
  local cohort names new run
  for cohort in workspace e2e-full; do
    run=(cargo nextest run --workspace --no-fail-fast --no-tests=pass)
    [ "$cohort" = workspace ] || run=(cargo nextest run -p datum-commands --test e2e --features e2e-full --no-fail-fast --no-tests=pass)
    new=()
    [ ! -s "$sel/rust/new.tests" ] || mapfile -t new <"$sel/rust/new.tests"
    if [ -e "$sel/rust/FULL" ]; then
      [ "$cohort" = workspace ] && echo "mq-check: Rust tests: not the whole suite, which CI runs: $(paste -sd';' "$sel/rust/FULL")"
      if [ "${#new[@]}" -eq 0 ]; then echo "mq-check: Rust tests ($cohort): the PR adds or edits none"; continue; fi
      run+=("${new[@]}")
    else
      names=()
      [ ! -s "$sel/rust/$cohort.tests" ] || mapfile -t names <"$sel/rust/$cohort.tests"
      if [ "${#names[@]}" -eq 0 ] && [ "${#new[@]}" -eq 0 ]; then echo "mq-check: Rust tests ($cohort): no test reaches this change"; continue; fi
      run+=("${new[@]}" -- --exact "${names[@]}")
    fi
    echo "mq-check: Rust tests ($cohort): ${run[*]}"
    [ -n "$dry" ] || "${run[@]}"
  done
}

# Fills $sel/web.tests with what CI's web step runs, and the PR's own tests.
web_selection() {
  local own="$sel/own.tests" list="$sel/web/web.tests" count
  git diff --name-only --no-renames --diff-filter=d "$pr_base...HEAD" -- 'web/tests/*.test.mjs' | sed 's,^web/,,' >"$own"
  mkdir -p "$sel/web"
  if [ -n "$integration" ]; then
    # The `CI (integration)` job: the permit's list, red as it is red there.
    python3 scripts/integration-permit.py "$integration" "$list" "$sel/web.glob"
    local glob t
    glob="$(cat "$sel/web.glob")"
    for t in $(cd web && compgen -G "$glob" || true); do
      grep -qxF "$t" "$list" || { echo "mq-check: $t matches the permit's web_tests_glob and is not in its web_tests: list it" >&2; return 1; }
    done
    while read -r t; do
      [ -f "web/$t" ] || { echo "mq-check: $t is listed in the permit file and does not exist" >&2; return 1; }
    done <"$list"
    echo "mq-check: web tests: the permit's list for $integration, as CI (integration)"
  else
    git show origin/testmap:testmap-web.json >"$sel/testmap-web.json" 2>/dev/null || rm -f "$sel/testmap-web.json"
    CI="${CI:-1}" node web/scripts/testmap.mjs select "$sel/testmap-web.json" "$pr_base" "$sel/web" ||
      { echo "the selection failed" >"$sel/web/FULL"; }
    count="$([ -e "$list" ] && wc -l <"$list" || echo 0)"
    if [ -e "$sel/web/FULL" ] || [ "$count" -gt "$MAX_LOCAL_WEB_FILES" ]; then
      [ -e "$sel/web/FULL" ] || echo "$count files, more than MAX_LOCAL_WEB_FILES ($MAX_LOCAL_WEB_FILES)" >"$sel/web/FULL"
      echo "mq-check: web tests: not the selection, which CI runs: $(paste -sd';' "$sel/web/FULL")"
      echo "mq-check: web tests: the PR's own new and edited files; CI runs the rest"
      : >"$list"
    else
      echo "mq-check: web tests: the test map's selection, as CI"
    fi
  fi
  [ -e "$list" ] || : >"$list"
  sort -u "$list" "$own" >"$sel/web.tests"
  echo "mq-check: web tests: $(wc -l <"$sel/web.tests") files"
  sed 's/^/  /' "$sel/web.tests"
}

if [ -n "$rust" ]; then
  if [ -n "$dry" ]; then
    echo "mq-check: Rust changed — cargo xtask check lint, scripts/check-changed.sh --base $merge_base"
  else
    # CI's honesty gates (ci.yml, `cargo xtask check lint`): neither
    # check-changed.sh nor check.sh runs them, so #402 passed this gate and
    # failed CI on max-file-size (da-yl0oa). In-process scans, seconds.
    echo "mq-check: Rust changed — running cargo xtask check lint"
    cargo run -q -p xtask -- check lint
    echo "mq-check: Rust changed — running scripts/check-changed.sh --base $merge_base"
  fi
  if [ -n "$full" ] || ! command -v cargo-nextest >/dev/null 2>&1; then
    [ -n "$full" ] || echo "mq-check: cargo-nextest is not installed, and a selection needs it: the closure's whole crates run"
    [ -n "$dry" ] || bash scripts/check-changed.sh --base "$merge_base"
    [ -z "$dry" ] || echo "mq-check: Rust tests: the closure's whole crates"
  else
    [ -n "$dry" ] || bash scripts/check-changed.sh --base "$merge_base" --no-tests
    rust_tests
  fi
fi

# CI (integration) runs its list whatever changed; a crate is what the module
# those tests run on is built from.
if [ -n "$web" ] || { [ -n "$integration" ] && [ -n "$rust" ] && [ -z "$full" ]; }; then
  if ! command -v pnpm >/dev/null 2>&1 || ! command -v node >/dev/null 2>&1; then
    node_bin="$(ls -d "$HOME"/.nvm/versions/node/*/bin 2>/dev/null | tail -1 || true)"
    export PATH="${node_bin:+$node_bin:}$HOME/.local/share/pnpm/bin:$HOME/.local/share/pnpm:$PATH"
  fi
  # The suite otherwise reads the box's load average and yields to it, which is
  # right for a can sharing the box and wrong for the refinery's verdict. So the
  # queue names its own number and everyone else reads the load (da-x38v,
  # da-5f20z).
  [ -z "$refinery" ] || export DATUM_TEST_CONCURRENCY="${DATUM_TEST_CONCURRENCY:-6}"
  if [ -n "$full" ]; then
    echo "mq-check: web/ changed — running the web check and suite"
    if [ -z "$dry" ]; then
      # web/tests/sync*.test.mjs pair with a native `datum` from target/debug
      # (or $CARGO_TARGET_DIR/debug — datumBinary() reads it too),
      # which nothing else builds on a web-only diff (CI's cargo step does). Built
      # every time, not only when missing: a stale one tests another tree's CLI,
      # and on a warm tree the build is a no-op (da-df5g9).
      cargo build -p datum-cli
      (cd web && pnpm install --frozen-lockfile && pnpm run check && pnpm test)
    fi
  else
    web_selection
    if [ -z "$dry" ]; then
      (cd web && pnpm install --frozen-lockfile && pnpm run check)
      if [ -s "$sel/web.tests" ]; then
        cargo build -p datum-cli
        (cd web && DATUM_TEST_PR=1 DATUM_TEST_ONLY="$sel/web.tests" pnpm test)
      fi
    fi
  fi
fi

if [ -z "$rust$web" ]; then
  echo "mq-check: nothing under crates/ or web/ changed since $merge_base — nothing to run"
fi
