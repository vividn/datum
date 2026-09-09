# datum — task runner
#
# The package manager lives in one variable so switching it is one line here
# plus the lockfile and CI, rather than a sweep across every recipe.
pm := "pnpm"

web := justfile_directory() / "web"

set shell := ["bash", "-c"]
set dotenv-load := true

# List the recipes (default when you type bare `just`).
default:
    @just --list

# --- web app -------------------------------------------------------------

# ~10s warm. The output is byte-reproducible across machines as of the rust-src
# pin, so a no-op run leaves the tree clean — if it does not, the committed
# artifact was genuinely behind and wants committing.
#
# Rebuild the browser's Rust modules into web/src/lib/wasm/.
wasm:
    cd {{web}} && {{pm}} run build:wasm

build:
    cd {{web}} && {{pm}} run build

dev:
    cd {{web}} && {{pm}} run dev

preview: build
    cd {{web}} && {{pm}} run preview

test:
    cd {{web}} && {{pm}} test

check:
    cd {{web}} && {{pm}} run check

install:
    cd {{web}} && {{pm}} install

# --- deploy --------------------------------------------------------------

# Build and upload to the bucket. Run `just verify` after.
deploy: build
    {{web}}/deploy.sh --yes

# The same thing without writing: prints exactly what would change.
deploy-dry: build
    {{web}}/deploy.sh

# What a deep link must do is SERVE THE APP, which is not the same as
# returning 200: S3 website hosting serves the error document with the ERROR
# status, so `/now` comes back as 404 carrying the full app HTML, and the
# browser routes client-side. Asserting 200 there failed against a correctly
# configured bucket for an hour on 2026-09-08 and sent vividn to re-check a
# setting that was already right.
#
# So: `/` must be 200, and `/now` must return the app — checked by content,
# with `/`'s own body as the reference rather than a string I chose.
#
# Check what the live site serves, from outside. Run after every deploy.
verify:
    #!/usr/bin/env sh
    # No `set -e`: curl exits non-zero when the host never answers, and under
    # `set -e` that killed the recipe before it printed anything — losing the
    # diagnostic in exactly the case it exists for.
    set -u
    root_code=$(curl -sS -o /tmp/dd-root.html -m 10 -w '%{http_code}' https://datum.day/ 2>/dev/null || true)
    deep_code=$(curl -sS -o /tmp/dd-deep.html -m 10 -w '%{http_code}' https://datum.day/now 2>/dev/null || true)
    same=$(cmp -s /tmp/dd-root.html /tmp/dd-deep.html && echo yes || echo no)
    app=$(grep -c '_app/immutable' /tmp/dd-root.html 2>/dev/null || echo 0)
    printf '  /       %s\n' "${root_code:-000}"
    printf '  /now    %s   (a 404 here is EXPECTED — see above)\n' "${deep_code:-000}"
    printf '  /now serves the same bytes as /   %s\n' "$same"
    printf '  / looks like the built app        %s script refs\n' "$app"
    printf '\n  000 -> nothing answered: DNS, the CNAME, or the origin is not up\n'
    printf '  403 -> objects are not publicly readable: fix the bucket policy\n'
    printf '  /now differing from / -> the bucket error document is not index.html\n'
    [ "${root_code:-000}" = 200 ] && [ "$same" = yes ] && [ "$app" -gt 0 ]
