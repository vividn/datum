#!/usr/bin/env bash
# A pull request's `edited` run tests its head only where no ready run has
# (da-283i3). A draft gets no run: each PR job skips while
# `pull_request.draft`, which Forgejo sets from the title (`WIP:` or `[WIP]`,
# what `ct pr hold` writes). It sends no ready_for_review, so a PR leaves
# draft by a title edit, an `edited` event, and a body edit is the same event
# with nothing saying which. A skip is no answer to it either: every run writes
# its jobs' commit statuses at the head, and the combined status reads a
# `skipped` as no worse than success, so a skipped run would hide a red one.
#
# So an edit ends as the newest finished pull_request run of this workflow on
# this head into this base branch ended, in seconds (a base change is an edit
# too, and tests in full): green, it writes TRAIN=1 to the env and
# skip=true to the step's outputs, which skip every later step or job; red, it
# fails. With none (the head was a draft, or its run was cancelled) it writes
# nothing and the run tests the head in full. Each workflow's concurrency
# queues an edit behind a run in progress, so that run has finished by now.
# An unreadable list tests in full too.
#
# Usage: scripts/ci-edited.sh <workflow file>, with TOKEN, SERVER, REPO, HEAD
# (the PR's head sha) and BASE (its base branch) in the env.
set -uo pipefail
url="${SERVER%/}/api/v1/repos/$REPO/actions/runs?head_sha=$HEAD&limit=50"
runs=$(printf 'header = "Authorization: token %s"\n' "$TOKEN" | curl -sSf -K - "$url" 2>&1) ||
  { echo "::warning::could not read $url ($runs); testing $HEAD in full"; exit 0; }
last=$(node -e '
  const r = JSON.parse(require("fs").readFileSync(0)).workflow_runs
    .filter(r => r.workflow_id === process.argv[1] && r.trigger_event === "pull_request" && ["success", "failure"].includes(r.status)
      && JSON.parse(r.event_payload).pull_request?.base?.ref === process.argv[2])
    .sort((a, b) => b.id - a.id)[0];
  if (r) console.log(r.status, `run ${r.id}`, r.html_url);' "$1" "$BASE" <<<"$runs" 2>&1) ||
  { echo "::warning::$url did not answer a run list ($last); testing $HEAD in full"; exit 0; }
case "$last" in
  success*)
    echo "TRAIN=1" >> "$GITHUB_ENV"
    echo "skip=true" >> "$GITHUB_OUTPUT"
    echo "an edit, and $HEAD was tested: green, as $last" ;;
  failure*)
    echo "::error::an edit, and $HEAD was tested: red, as $last"
    exit 1 ;;
  *)
    echo "no pull_request run of $1 into $BASE finished on $HEAD: testing it in full" ;;
esac
