#!/usr/bin/env bash
# Only vividn changes the goldens' approvals (da-iuyyw).
#
#   check_approvals_signed.sh <allowed_signers> <base> <head>
#
# Every commit in <base>..<head> that touches a protected path must carry an
# SSH signature by a key in <allowed_signers>; any other such commit fails the
# check. An author name proves nothing — anyone can type `vividn` into
# `--author` — so only the signature is read.
#
# Run on a PR's own commits (.forgejo/workflows/approvals.yml), never on main:
# the train squash-merges through the forge's API, so the commit main keeps is
# the forge's and no signature survives onto it.
#
# The protected paths are the lock itself, the keys that may change it, and
# what enforces it: a check that a PR could rewrite unsigned would be a check
# the next PR runs without.
set -euo pipefail

if [ $# -ne 3 ]; then
  echo "usage: $0 <allowed_signers> <base> <head>" >&2
  exit 2
fi
signers=$1 base=$2 head=$3

protected=(
  crates/datum-parity/goldens/APPROVED
  crates/datum-parity/goldens/allowed_signers
  crates/datum-parity/scripts/check_approvals_signed.sh
  crates/datum-parity/tests/approvals.rs
  .forgejo/workflows/approvals.yml
)

# What main would receive: the squash applies merge-base..head.
changed=$(git diff --name-only "$base...$head" -- "${protected[@]}")
if [ -z "$changed" ]; then
  echo "no protected path differs between $base and $head"
  exit 0
fi
# `diff-tree --cc` below omits a path a merge took whole from one parent, and
# an old main commit is a parent no check reads (vremya, #441): a PR that
# changes these must be linear, so each commit's own diff is exact.
if [ -n "$(git rev-list --merges "$base..$head")" ]; then
  echo "FAIL  $base..$head changes $(echo $changed) and holds a merge commit — rebase onto main"
  exit 1
fi

bad=0 seen=0
for c in $(git rev-list "$base..$head"); do
  touched=$(git diff-tree -r --cc --no-commit-id --name-only "$c" -- "${protected[@]}")
  [ -n "$touched" ] || continue
  seen=$((seen + 1))
  # %G? is G only for a good signature by a key the allowed-signers file names.
  status=$(git -c gpg.ssh.allowedSignersFile="$signers" log -1 --format=%G? "$c")
  if [ "$status" = G ]; then
    echo "ok    $c signed by an allowed key: $(echo $touched)"
  else
    echo "FAIL  $c (signature status $status) touches $(echo $touched) — changing these needs vividn's signed commit"
    bad=1
  fi
done

echo "$seen commit(s) in $base..$head touch the approvals"
exit $bad
