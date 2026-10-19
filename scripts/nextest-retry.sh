#!/usr/bin/env bash
# RETRY ONCE, FOR NAMED RUST TESTS ONLY (da-4q1wa): the cargo side of
# da-8bmqe's web list. `DATUM_CARGO_RETRY` holds `<test> (<bead>)` lines
# (ci.yml). This runs the `cargo nextest run …` it is given with each listed
# test allowed ONE retry, nextest's own, set by a tool config, and after it
# prints `RETRIED <test> (<bead>): retry passed|failed; first run: <its
# panic>` for each listed test that failed once. An unlisted test is never
# retried, and a listed one failing twice fails the run as before. The exit
# status is nextest's.
#
#   DATUM_CARGO_RETRY='my_test (da-x)' scripts/nextest-retry.sh cargo nextest run … [-- …]
set -uo pipefail

names=() beads=() filter=
while read -r line; do
    [ -n "$line" ] || continue
    [[ $line =~ ^([A-Za-z0-9_:]+)\ \(([a-z]+-[a-z0-9.]+)\)$ ]] ||
        { echo "DATUM_CARGO_RETRY wants '<test> (<bead>)' lines; got: $line" >&2; exit 2; }
    names+=("${BASH_REMATCH[1]}") beads+=("${BASH_REMATCH[2]}")
    filter+="${filter:+ | }test(=${BASH_REMATCH[1]})"
done <<< "${DATUM_CARGO_RETRY:-}"
[ ${#names[@]} -gt 0 ] || exec "$@"

# A name is nextest's, module path and all (`sync_net::…` in datum-cli's
# `main`): one matching no test would retry nothing, silently.
known=$(cargo nextest list --workspace --color never --message-format oneline -E "$filter") || exit
for t in "${names[@]}"; do
    grep -q " $t\$" <<< "$known" || { echo "DATUM_CARGO_RETRY names $t, which is no test in the workspace" >&2; exit 2; }
done

dir=$(mktemp -d) && trap 'rm -rf "$dir"' EXIT
printf "[[profile.default.overrides]]\nfilter = '%s'\nretries = 1\n" "$filter" > "$dir/retry.toml"
# The flag goes before any `--`, past which the words are the test binary's.
args=() put=
for a in "$@"; do
    [ -z "$put" ] && [ "$a" = -- ] && args+=(--tool-config-file "datum-ci:$dir/retry.toml") put=1
    args+=("$a")
done
[ -n "$put" ] || args+=(--tool-config-file "datum-ci:$dir/retry.toml")

"${args[@]}" 2>&1 | tee "$dir/log"
status=${PIPESTATUS[0]}

# nextest's human output, its colours (CARGO_TERM_COLOR) taken off: `TRY 1
# FAIL [ 1.2s] (2/4) crate::bin <test>`, the failure's output indented under
# it, up to the next status line.
sed 's/\x1b\[[0-9;]*m//g' "$dir/log" > "$dir/plain" && mv "$dir/plain" "$dir/log"
for i in "${!names[@]}"; do
    t=${names[$i]}
    grep -Eq "^ *TRY 1 [A-Z]+ .* $t\$" "$dir/log" || continue
    if grep -Eq "^ *TRY 2 PASS .* $t\$" "$dir/log"; then how=passed; else how=failed; fi
    first=$(awk -v t=" $t" '
        !on && /^ *TRY 1 [A-Z]+ / && substr($0, length($0) - length(t) + 1) == t { on = 1; next }
        on && (/^ *[A-Z][A-Z0-9 \/]* \[ *[0-9.]+s\]/ || /^─/) { exit }
        on { sub(/^ +/, ""); if (/panicked at/) p = 1; else if (/^(note: |stack backtrace:)/) p = 0; else if (p && $0 != "") m = m (m ? " / " : "") $0
             if ($0 != "") { l3 = l2; l2 = l1; l1 = $0 } }
        END { print (m ? m : l3 (l3 ? " / " : "") l2 (l2 ? " / " : "") l1) }' "$dir/log")
    echo "RETRIED $t (${beads[$i]}): retry $how; first run: $first"
done
exit "$status"
