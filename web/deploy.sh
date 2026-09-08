#!/usr/bin/env bash
# Deploy datum.day: build the static bundle and sync it to an S3-compatible bucket.
#
# Dry-run by default. Pass --yes to actually write.
#
#   ./web/deploy.sh              # build + show exactly what would change
#   ./web/deploy.sh --yes        # build + upload
#
# Required environment (see DEPLOY.md):
#   DATUM_DAY_BUCKET     bucket name, e.g. datum-day
#   DATUM_DAY_ENDPOINT   S3 endpoint, e.g. https://s3.fr-par.scw.cloud
#   AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY   credentials for that bucket
set -euo pipefail

DRY="--dryrun"
[ "${1:-}" = "--yes" ] && DRY=""

: "${DATUM_DAY_BUCKET:?set DATUM_DAY_BUCKET (see web/DEPLOY.md)}"
: "${DATUM_DAY_ENDPOINT:?set DATUM_DAY_ENDPOINT (see web/DEPLOY.md)}"
command -v aws >/dev/null || { echo "aws CLI not found; see web/DEPLOY.md" >&2; exit 1; }

cd "$(dirname "$0")"
# The wasm is COMMITTED, not built here -- deliberately, so a deploy needs no
# Rust toolchain (vividn's concern: "will this continue to work on ci on other
# servers"). The cost is that static/*.wasm can silently fall behind the Rust it
# was built from, and the deploy would ship a parser that is not main's. Not
# hypothetical: on 2026-09-04 the committed datum-parse.wasm was 1.317.259 bytes
# and the same source rebuilt to 1.318.965 -- the conflicts command had landed in
# Rust and never reached the artifact.
#
# Refuse rather than rebuild: refusing keeps the no-toolchain property, and this
# runs BEFORE the install so it costs nothing when it fires.
#
# *** DO NOT DELETE THIS AS REDUNDANT WITH CI. *** As of 9454289 the wasm-check
# job rebuilds and compares BYTES against the committed copies, which is
# strictly stronger than the mtime test below -- for what is COMMITTED. This
# check guards a different thing: the DEPLOYER'S WORKING TREE. `pnpm run build`
# below does not rebuild wasm, so someone with uncommitted Rust edits deploys
# the committed artifact without knowing it, and CI never saw that tree.
#
# I wrote in da-nig7 that a CI comparison would make this redundant and it
# should be removed in the same change. That was written before the CI check
# existed and it was wrong: they cover the repository and the deployer's
# machine, which are not the same place. Two checks for one property is a real
# hazard; two checks for two properties is not.
#
# Paths are relative to web/ (cd'd above). Floors first, because `find` on a
# missing directory returns empty, which reads exactly like "the wasm is fresh".
[ -f static/datum-parse.wasm ] || {
  echo "REFUSING: static/datum-parse.wasm does not exist; nothing to compare." >&2
  echo "  Build it first: pnpm run build:wasm" >&2
  exit 1
}
[ -d ../crates ] || {
  echo "REFUSING: ../crates not found, so the staleness check cannot run." >&2
  echo "  A silent pass here would ship whatever wasm happens to be committed." >&2
  exit 1
}
# BUILD the wasm rather than police it. The check below compares TIMESTAMPS,
# and a source change does not imply a different wasm -- a change behind a
# `native` feature gate cannot alter a build that excludes it -- so as a gate it
# refuses correct deploys, which is exactly what it did on 2026-09-08.
#
# Building removes the question instead of detecting it. Safe to do here since
# the rust-src pin (#138): the output is byte-reproducible across machines, so a
# no-op build leaves the tree clean rather than churning the committed artifact.
if command -v cargo >/dev/null 2>&1; then
  echo "==> building wasm"
  pnpm run build:wasm
else
  # The toolchain-free path, which is why the artifacts are committed at all.
  # HERE the staleness check earns its keep: we are about to ship bytes we did
  # not just produce, so their age is the only evidence available.
  echo "==> cargo not found; shipping the COMMITTED wasm" >&2
  newest_rust=$(find ../crates -name '*.rs' -newer static/datum-parse.wasm -print -quit 2>/dev/null)
  if [ -n "$newest_rust" ]; then
    echo "REFUSING: static/*.wasm is older than Rust source ($newest_rust)." >&2
    echo "  Install a Rust toolchain and re-run, so the deploy builds it, or:" >&2
    echo "    just wasm && git add web/static/*.wasm" >&2
    echo "  Or set STALE_WASM_OK=1 to ship the committed one deliberately." >&2
    echo "" >&2
    echo "  NOTE: this compares TIMESTAMPS, not content. CI's WASM check rebuilds" >&2
    echo "  and compares bytes, so if CI is green on this commit the committed" >&2
    echo "  wasm is current and STALE_WASM_OK=1 is the right answer:" >&2
    echo "    gh run list -c \$(git rev-parse HEAD) --json name,conclusion" >&2
    [ "${STALE_WASM_OK:-}" = "1" ] || exit 1
  fi
fi

echo "==> building"
pnpm install --frozen-lockfile --silent
pnpm run build

DIST=build
[ -d "$DIST" ] || { echo "no $DIST/ — the build produced nothing" >&2; exit 1; }
S3="s3://$DATUM_DAY_BUCKET"
AWS=(aws s3 --endpoint-url "$DATUM_DAY_ENDPOINT")

[ -n "$DRY" ] && echo "==> DRY RUN (pass --yes to write)"

# Hashed assets are immutable and safe to cache for a year. Everything under
# _app/immutable/ carries a content hash in its filename, so a new build gets a
# new URL and never needs revalidation.
echo "==> syncing immutable assets"
# `.wasm` is excluded here and uploaded below with an explicit content-type;
# without the exclusion these files ship twice, and they are the largest in
# the bundle.
"${AWS[@]}" sync "$DIST/_app/immutable" "$S3/_app/immutable" $DRY \
  --exclude "*.wasm" \
  --cache-control "public,max-age=31536000,immutable"

# .wasm MUST be served as application/wasm. Browsers refuse to stream-compile
# an octet-stream, so a wrong content-type here costs a slow fallback at best
# and a hard failure at worst. aws-cli guesses from the extension and gets this
# wrong on some setups, so it is set explicitly rather than trusted.
echo "==> fixing wasm content-type"
while IFS= read -r f; do
  rel="${f#"$DIST/"}"
  "${AWS[@]}" cp "$f" "$S3/$rel" $DRY \
    --content-type application/wasm \
    --cache-control "public,max-age=31536000,immutable"
done < <(find "$DIST" -name '*.wasm')

# HTML and the service worker must NOT be cached, or a deploy is invisible to
# anyone who has visited before.
echo "==> syncing entry points (no-cache)"
"${AWS[@]}" sync "$DIST" "$S3" $DRY \
  --exclude "_app/immutable/*" --exclude "*.wasm" \
  --cache-control "no-cache,must-revalidate"

echo
if [ -n "$DRY" ]; then
  echo "Dry run complete. Nothing was written. Re-run with --yes to deploy."
else
  echo "Deployed."
  echo
  echo "VERIFY THE THING THAT MATTERS, which is storage, not headers:"
  echo "  open https://datum.day, log an entry, RELOAD, confirm it is still there."
  echo
  echo "The banner reports the storage the database actually opened with:"
  echo "  \"stored in this browser only\"  -> persistent, working"
  echo "  \"held in memory only\"          -> degraded; entries vanish on reload"
  echo "  \"could not be opened\"          -> broken; nothing is being saved"
  echo
  echo "Also check a deep link, which needs the bucket error document set to"
  echo "index.html (see web/DEPLOY.md):"
  echo "  curl -sI https://datum.day/now | head -1     # must be 200, not 404"
fi
