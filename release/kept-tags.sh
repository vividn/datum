#!/usr/bin/env bash
#
# The kept releases a stretch of main makes (da-ec8y8; vividn, 2026-11-04: «we
# might want to publish releases at every sync floor at the very least right
# now»): each commit that raises the sync floor (SYNC_FLOORS in
# crates/datum-core/src/migrations/mod.rs, da-ryhmi) and the commit before it,
# the last datum that syncs with a copy below it. Each is named for the version
# its build prints, v<major>.<minor>.<height> (crates/datum-cli/build_id.rs). A
# floor raises the minor, and the last build of each minor is tagged
# v<major>.<minor> as well.
#
#   release/kept-tags.sh <before> <after>   # the first-parent commits after <before> up to <after>
#   release/kept-tags.sh '' <after>         # the whole history
#
# Prints `<tag> <sha> <floor>` a line, oldest first, and makes no tag:
# .forgejo/workflows/kept-releases.yml pushes each that does not exist, never
# moving one, and the mirror carries them to GitHub, where release.yml and
# deploy.yml build each. A <before> that is not an ancestor of <after> prints
# nothing and fails.
set -euo pipefail

before=${1:-}
after=${2:?usage: kept-tags.sh <before|''> <after>}

# The last floor in SYNC_FLOORS at a commit, 0 where it has none. A test holds
# this to datum_core's SYNC_FLOOR on the tree it runs in (kept_release.rs).
file=crates/datum-core/src/migrations/mod.rs
floor() {
  { git show "$1:$file" 2> /dev/null || true; } |
    awk '/^pub const SYNC_FLOORS/ { on = 1; next }
         on && /^\];/ { exit }
         on { sub(/\/\/.*/, ""); if (match($0, /[0-9]+/)) last = substr($0, RSTART, RLENGTH) }
         END { print last + 0 }'
}

# The workspace's <major>.<minor> at a commit, empty where it has none.
minor() {
  { git show "$1:Cargo.toml" 2> /dev/null || true; } |
    awk -F'"' '/^\[workspace\.package\]/ { w = 1; next } /^\[/ { w = 0 } w && /^version/ { split($2, v, "."); print v[1] "." v[2]; exit }'
}

tag() {
  echo "v$(minor "$1").$(git rev-list --count "$1")"
}

range=("$after")
if [ -n "$before" ]; then
  # A push whose old tip is not under the new one (a force push, or a clone
  # that lacks it) tags nothing rather than guess at what it brought.
  git merge-base --is-ancestor "$before" "$after" || {
    echo "kept-tags: $before is not an ancestor of $after: no tag made; run the whole history ('' $after) once it is sorted out" >&2
    exit 1
  }
  range=("$before..$after")
fi

# The floor and the minor move only where their files do: those commits alone,
# each against the commit before it on main. Where the minor moves, the commit
# before is also tagged v<major>.<minor>, the last build of its minor, which
# `datum upgrade --to <major>.<minor>` installs: the minor's newest build, made
# once its successor starts, so it never moves either.
for c in $(git rev-list --first-parent --reverse "${range[@]}" -- "$file" Cargo.toml); do
  parent=$(git rev-parse -q --verify "$c^1") || continue
  was=$(floor "$parent")
  now=$(floor "$c")
  if [ "$now" -gt "$was" ]; then
    echo "$(tag "$parent") $parent $was"
    echo "$(tag "$c") $c $now"
  fi
  had=$(minor "$parent") has=$(minor "$c")
  [ -z "$had" ] || [ -z "$has" ] || [ "$had" = "$has" ] || echo "v$had $parent $was"
done
