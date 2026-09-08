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

# The two failure modes point at their own cause: 403 on / means the bucket
# objects are not publicly readable; 404 on /now means the bucket's error
# document is not index.html, so the SPA fallback is missing.
#
# Check what the live site serves, from outside. Run after every deploy.
verify:
    #!/usr/bin/env sh
    # No `set -e`: curl exits non-zero when the host never answers, and under
    # `set -e` that killed the recipe before it printed anything — losing the
    # diagnostic in exactly the case it exists for. `%{http_code}` reports 000
    # for a dead connection, which is a perfectly good thing to print.
    set -u
    root=$(curl -sS -o /dev/null -m 10 -w '%{http_code}' https://datum.day/ 2>/dev/null || true)
    deep=$(curl -sS -o /dev/null -m 10 -w '%{http_code}' https://datum.day/now 2>/dev/null || true)
    printf '  /      %s\n' "${root:-000}"
    printf '  /now   %s\n' "${deep:-000}"
    printf '\n  000 -> nothing answered: DNS, the CNAME, or the origin is not up\n'
    printf '  403 -> objects are not publicly readable: fix the bucket policy\n'
    printf '  404 on /now only -> the bucket error document is not index.html\n'
    [ "${root:-000}" = 200 ] && [ "${deep:-000}" = 200 ]
