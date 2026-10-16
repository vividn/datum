#!/usr/bin/env bash
# Puts FILEs on the `testmap` branch (testmap.yml), each at its basename, in
# ONE orphan commit that keeps every other file the branch holds: the Rust
# map (da-fnt3v) and the web map (da-e0gtq) are pushed by two jobs. The push
# is leased on the tip it read, and read again when the other job got there
# first.
#
#   scripts/testmap-push.sh FILE…
set -euo pipefail
: "${1:?usage: scripts/testmap-push.sh FILE…}"
export GIT_INDEX_FILE
GIT_INDEX_FILE=$(mktemp -u)
for try in 1 2 3 4 5; do
  if git fetch -q origin +refs/heads/testmap:refs/remotes/origin/testmap 2>/dev/null; then
    old=$(git rev-parse refs/remotes/origin/testmap)
    git read-tree "$old"
  else
    old=
    git read-tree --empty
  fi
  for f in "$@"; do
    git update-index --add --cacheinfo "100644,$(git hash-object -w "$f"),$(basename "$f")"
  done
  commit=$(git commit-tree "$(git write-tree)" -m "testmap for $(git rev-parse HEAD)")
  if git push -q --force-with-lease="refs/heads/testmap:$old" origin "$commit:refs/heads/testmap"; then
    echo "testmap $commit for $(git rev-parse HEAD): $*"
    exit 0
  fi
  echo "the testmap branch moved (try $try); reading it again"
  sleep $((try * 5))
done
exit 1
