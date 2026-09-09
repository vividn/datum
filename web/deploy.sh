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

# The layout rule the cache headers below rest on: a file whose CONTENT
# changes must not be served under a name that does not. Every runtime module
# is a Vite `?url` import now, so the build emits it content-hashed under
# _app/immutable/ — and a .wasm anywhere else is a deploy error rather than a
# header choice. It was a header choice once: the three datum modules sat in
# static/ under stable names and this script sent them a year of `immutable`,
# so a returning browser ran the new build's JavaScript against the previous
# build's module until the HTTP cache expired (da-tvg0).
check_dist() {
  local dist="$1" stray
  [ -d "$dist" ] || { echo "REFUSING: no $dist/ to check." >&2; return 1; }
  stray=$(find "$dist" -name '*.wasm' -not -path "$dist/_app/immutable/*")
  if [ -n "$stray" ]; then
    echo "REFUSING: .wasm outside _app/immutable/, so its name is stable and its bytes are not:" >&2
    echo "$stray" | sed 's/^/  /' >&2
    echo '  Import it from src/ with ?url so Vite hashes the name (da-tvg0).' >&2
    return 1
  fi
}

# `--check-dist <dir>`: that assertion alone, against an already-built tree.
# No credentials, no build, no aws — so the test suite can hold a build to the
# same rule the deploy does, rather than to a copy of it.
if [ "${1:-}" = "--check-dist" ]; then
  check_dist "${2:?usage: deploy.sh --check-dist <dist-dir>}"
  echo "OK: every .wasm under ${2} is content-hashed beneath _app/immutable/"
  exit 0
fi

DRY="--dryrun"
[ "${1:-}" = "--yes" ] && DRY=""

: "${DATUM_DAY_BUCKET:?set DATUM_DAY_BUCKET (see web/DEPLOY.md)}"
: "${DATUM_DAY_ENDPOINT:?set DATUM_DAY_ENDPOINT (see web/DEPLOY.md)}"
command -v aws >/dev/null || { echo "aws CLI not found; see web/DEPLOY.md" >&2; exit 1; }

cd "$(dirname "$0")"
# The wasm is COMMITTED, not built here -- deliberately, so a deploy needs no
# Rust toolchain (vividn's concern: "will this continue to work on ci on other
# servers"). The cost is that src/lib/wasm/*.wasm can silently fall behind the Rust it
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
[ -f src/lib/wasm/datum-parse.wasm ] || {
  echo "REFUSING: src/lib/wasm/datum-parse.wasm does not exist; nothing to compare." >&2
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
  newest_rust=$(find ../crates -name '*.rs' -newer src/lib/wasm/datum-parse.wasm -print -quit 2>/dev/null)
  if [ -n "$newest_rust" ]; then
    echo "REFUSING: src/lib/wasm/*.wasm is older than Rust source ($newest_rust)." >&2
    echo "  Install a Rust toolchain and re-run, so the deploy builds it, or:" >&2
    echo "    just wasm && git add web/src/lib/wasm/*.wasm" >&2
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
check_dist "$DIST"
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
#
# The cache header follows the NAME, not the extension: a year of `immutable`
# is a promise that this URL will never mean anything else, which only a
# content hash can keep. `check_dist` above has already refused anything
# outside _app/immutable/, so the second branch is unreachable today and is
# here so that the next stable-named asset is merely revalidated rather than
# frozen for a year.
echo "==> fixing wasm content-type"
while IFS= read -r f; do
  rel="${f#"$DIST/"}"
  case "$rel" in
    _app/immutable/*) cc="public,max-age=31536000,immutable" ;;
    *) cc="no-cache,must-revalidate" ;;
  esac
  "${AWS[@]}" cp "$f" "$S3/$rel" $DRY \
    --content-type application/wasm \
    --cache-control "$cc"
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
