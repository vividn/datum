#!/usr/bin/env bash
# regen.sh — regenerate parity goldens.
#
# Usage:
#   bash regen.sh              # regenerate all categories
#   bash regen.sh md5_colors   # regenerate one category
#   bash regen.sh dayview
#
# Categories:
#   md5_colors    Recomputes inputs.txt → expected.txt via node's
#                 built-in crypto.md5. Drop-in compatible with the
#                 npm `md5` package the original datum binary uses
#                 (both implement RFC 1321; bytes are identical).
#   time_syntax   No regeneration — these TSVs are hand-curated to
#                 match documented daydumb parser semantics. If a
#                 parser behavior changes intentionally, edit the
#                 TSV by hand and the tests pin the new behavior.
#   dayview       Re-runs `cargo test --test parity` with PARITY_REGEN=1
#                 to overwrite expected.json with the freshly-rendered
#                 layout. Use this after deliberately changing the
#                 layout engine; audit the diff before committing.
#
# This script does NOT call a live original-datum binary. The goldens
# are designed to be content-addressable by md5 (which both impls
# share) and pinned snapshots of daydumb's current output (which we
# treat as the parity reference per da-ixsk).

set -euo pipefail

cd "$(dirname "$0")"

FIXTURE_ROOT="$(pwd)"
WORKSPACE_ROOT="$(cd ../../../.. && pwd)"

regen_md5_colors() {
    echo "[md5_colors] regenerating from inputs.txt via node's crypto.md5..."
    if ! command -v node >/dev/null 2>&1; then
        echo "  ERROR: node not on PATH (need >=14 for built-in crypto)" >&2
        return 1
    fi
    local infile="$FIXTURE_ROOT/md5_colors/inputs.txt"
    local outfile="$FIXTURE_ROOT/md5_colors/expected.txt"
    : > "$outfile"
    while IFS= read -r line || [ -n "$line" ]; do
        node -e "
            const crypto = require('crypto');
            const name = process.argv[1] ?? '';
            const h = crypto.createHash('md5').update(name).digest('hex');
            process.stdout.write('#' + h.slice(0, 6) + '\n');
        " "$line" >> "$outfile"
    done < "$infile"
    echo "  wrote $(wc -l < "$outfile") rows to ${outfile#$WORKSPACE_ROOT/}"
}

regen_dayview() {
    echo "[dayview] regenerating expected.json via cargo test PARITY_REGEN=1..."
    (cd "$WORKSPACE_ROOT" && \
        PARITY_REGEN=1 cargo test --test parity -p datum-cli \
            dayview_render_snapshot_matches_golden -- --nocapture)
    echo "  audit the diff: git diff -- ${FIXTURE_ROOT#$WORKSPACE_ROOT/}/dayview/expected.json"
}

regen_time_syntax() {
    cat <<'NOTE'
[time_syntax] no auto-regen — TSVs are hand-curated.
  See tests/parity/README.md for the editing protocol. To verify a
  hand-edit lines up with daydumb's behavior, just run:
    cargo test --test parity -p datum-cli
NOTE
}

case "${1:-all}" in
    all)
        regen_md5_colors
        regen_time_syntax
        regen_dayview
        ;;
    md5_colors)        regen_md5_colors ;;
    time_syntax)       regen_time_syntax ;;
    dayview)           regen_dayview ;;
    *)
        echo "unknown category: $1" >&2
        echo "usage: bash $0 [all|md5_colors|time_syntax|dayview]" >&2
        exit 2
        ;;
esac
