#!/usr/bin/env bash
#
# Crate-aware pre-submit check — fast variant of `scripts/check.sh`.
#
# Runs the same battery (fmt, clippy, build, tests, doc tests) as
# `scripts/check.sh`, but scoped to the workspace crates the local diff
# actually touches plus their transitive workspace dependents. Intended
# for polecat pre-submit on small, single-crate changes.
#
# Trade-off: only catches breakage in crates reachable from the changed
# code. The merge queue, refinery, and `.github/workflows/ci.yml` all
# run the full `scripts/check.sh` battery as the safety net, so this
# script is a velocity tool, not a replacement.
#
# Falls back to invoking `scripts/check.sh` (full battery) when:
#   * a workspace-root config file changes (Cargo.toml, Cargo.lock,
#     rust-toolchain*, .cargo/, .github/) — could affect every crate
#   * no workspace crate is touched (e.g. docs-only diff) — trivial gain
#   * the dependent closure already covers >=75% of workspace crates
#
# Usage:
#   ./scripts/check-changed.sh                  # diff vs origin/main
#   ./scripts/check-changed.sh --base <ref>     # diff vs custom ref
#
# Requires: git, jq, cargo. Uses cargo-nextest when installed (matching
# scripts/check.sh); otherwise falls back to `cargo test`.
#
# If something fails that you suspect is noise, consult FLAKES.md.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BASE_REF=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --base)
            [[ $# -ge 2 ]] || { printf '--base requires an argument\n' >&2; exit 2; }
            BASE_REF="$2"
            shift 2
            ;;
        -h|--help)
            sed -n '2,/^set -euo/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//; s/^#$//'
            exit 0
            ;;
        *)
            printf 'Unknown argument: %s\n' "$1" >&2
            exit 2
            ;;
    esac
done

if ! command -v jq >/dev/null 2>&1; then
    printf 'check-changed.sh requires `jq` (not found on PATH)\n' >&2
    exit 2
fi

step() { printf '\n\033[1m== %s ==\033[0m\n' "$1"; }
note() { printf '\033[36m%s\033[0m\n' "$1"; }

fallback() {
    note "$1 — running full battery via scripts/check.sh."
    exec "$ROOT/scripts/check.sh"
}

# --- Resolve base ref --------------------------------------------------------

resolve_base() {
    if [[ -n "$BASE_REF" ]]; then
        echo "$BASE_REF"
        return
    fi
    if git rev-parse --verify --quiet origin/main >/dev/null; then
        echo origin/main
        return
    fi
    if git rev-parse --verify --quiet main >/dev/null; then
        echo main
        return
    fi
    # Last-resort: previous commit (lets the script still work in a
    # detached checkout without an obvious base).
    echo HEAD~1
}

BASE="$(resolve_base)"
if ! MERGE_BASE="$(git merge-base "$BASE" HEAD 2>/dev/null)"; then
    fallback "Could not resolve merge base with '$BASE'"
fi

# --- Collect changed paths (committed + staged + unstaged + untracked) -------

CHANGED_FILE="$(mktemp)"
trap 'rm -f "$CHANGED_FILE"' EXIT
{
    git diff --name-only "$MERGE_BASE"...HEAD
    git diff --name-only
    git diff --name-only --cached
    git ls-files --others --exclude-standard
} | sed '/^$/d' | sort -u > "$CHANGED_FILE"

CHANGED_COUNT=$(wc -l < "$CHANGED_FILE" | tr -d ' ')
note "Changed paths vs $BASE ($MERGE_BASE): $CHANGED_COUNT"

if [[ "$CHANGED_COUNT" -eq 0 ]]; then
    fallback "No changes detected (clean tree vs $BASE)"
fi

# --- Workspace-root triggers force full battery ------------------------------

ROOT_TRIGGERS='^(Cargo\.toml|Cargo\.lock|rust-toolchain(\.toml)?|\.cargo/|\.github/)'
if grep -Eq "$ROOT_TRIGGERS" "$CHANGED_FILE"; then
    fallback "Workspace-root config changed"
fi

# --- Workspace metadata ------------------------------------------------------

META_RAW="$(cargo metadata --format-version 1 --no-deps)"

# Lines of "<crate-name>\t<manifest-dir-relative-to-root>",
# sorted by directory-path length (descending) for longest-prefix match.
MEMBERS_TSV="$(jq -r --arg root "$ROOT/" '
    .packages[]
    | .manifest_path as $mp
    | "\(.name)\t\($mp | sub("/Cargo.toml$"; "") | sub($root; ""))"
' <<<"$META_RAW" | awk -F'\t' '{ print length($2)"\t"$0 }' | sort -rn | cut -f2-)"

ALL_COUNT=$(jq -r '.packages | length' <<<"$META_RAW")

# Forward edges (crate -> workspace path-dep, deduped, self-loops removed).
EDGES_TSV="$(jq -r '
    .packages[] as $p
    | $p.dependencies[]?
    | select(.path != null)
    | "\($p.name)\t\(.name)"
' <<<"$META_RAW" | awk -F'\t' '$1 != $2' | sort -u)"

# --- Map changed files to changed crates -------------------------------------

declare -A DIRECT_HIT=()
unmatched=0
while IFS= read -r file; do
    [[ -z "$file" ]] && continue
    matched=""
    while IFS=$'\t' read -r name dir; do
        [[ -z "$dir" ]] && continue
        if [[ "$file" == "$dir"/* || "$file" == "$dir" ]]; then
            matched="$name"
            break
        fi
    done <<<"$MEMBERS_TSV"
    if [[ -n "$matched" ]]; then
        DIRECT_HIT[$matched]=1
    else
        unmatched=$((unmatched + 1))
    fi
done < "$CHANGED_FILE"

DIRECT_LIST=("${!DIRECT_HIT[@]}")
if [[ ${#DIRECT_LIST[@]} -eq 0 ]]; then
    fallback "No workspace crate touched ($unmatched non-crate file(s) changed)"
fi

# --- BFS reverse-dependent closure ------------------------------------------

declare -A IN_SET=()
for c in "${DIRECT_LIST[@]}"; do IN_SET[$c]=1; done

queue=("${DIRECT_LIST[@]}")
while [[ ${#queue[@]} -gt 0 ]]; do
    cur="${queue[0]}"
    queue=("${queue[@]:1}")
    while IFS=$'\t' read -r src dst; do
        [[ "$dst" == "$cur" ]] || continue
        [[ -z "${IN_SET[$src]:-}" ]] || continue
        IN_SET[$src]=1
        queue+=("$src")
    done <<<"$EDGES_TSV"
done

SELECTED=("${!IN_SET[@]}")
SELECTED_COUNT=${#SELECTED[@]}

# Sort for stable output.
mapfile -t DIRECT_SORTED < <(printf '%s\n' "${DIRECT_LIST[@]}" | sort)
mapfile -t SELECTED_SORTED < <(printf '%s\n' "${SELECTED[@]}"   | sort)

note "Direct crates (${#DIRECT_SORTED[@]}): ${DIRECT_SORTED[*]}"
note "With reverse deps ($SELECTED_COUNT/$ALL_COUNT): ${SELECTED_SORTED[*]}"
if [[ $unmatched -gt 0 ]]; then
    note "(Also $unmatched non-crate file(s) changed — covered by full-fmt step.)"
fi

# --- >=75% coverage → fallback ----------------------------------------------

THRESHOLD=$(( (ALL_COUNT * 3 + 3) / 4 ))   # ceil(0.75 * ALL_COUNT)
if (( SELECTED_COUNT >= THRESHOLD )); then
    fallback "Closure covers $SELECTED_COUNT/$ALL_COUNT crates (>=75%)"
fi

# --- Scoped battery ----------------------------------------------------------

P_ARGS=()
for c in "${SELECTED_SORTED[@]}"; do P_ARGS+=(-p "$c"); done

step 'cargo fmt --all -- --check'
cargo fmt --all -- --check

step "cargo clippy ${P_ARGS[*]} --all-targets -- -D warnings"
cargo clippy "${P_ARGS[@]}" --all-targets -- -D warnings

step "cargo build ${P_ARGS[*]} --all-targets"
cargo build "${P_ARGS[@]}" --all-targets

if command -v cargo-nextest >/dev/null 2>&1; then
    step "cargo nextest run ${P_ARGS[*]} --no-fail-fast"
    cargo nextest run "${P_ARGS[@]}" --no-fail-fast
else
    step "cargo test ${P_ARGS[*]}  (cargo-nextest not installed; falling back)"
    cargo test "${P_ARGS[@]}"
fi

step "cargo test ${P_ARGS[*]} --doc"
cargo test "${P_ARGS[@]}" --doc

printf '\n\033[1;32m✓ scoped pre-submit checks passed (%d/%d crates)\033[0m\n' \
    "$SELECTED_COUNT" "$ALL_COUNT"
printf '\033[2m  (full workspace battery still runs in CI + refinery.)\033[0m\n'
