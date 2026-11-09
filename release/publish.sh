#!/usr/bin/env bash
#
# Publishes the binaries in <dist> (datum-<arch>-<os>, one a platform) to the
# rolling `latest` release, for .github/workflows/release.yml (da-mzhau).
#
# Each binary goes up under its build's own name, datum-<arch>-<os>-<sha7>,
# with SHA256SUMS's signature as SHA256SUMS-<sha7>.minisig, and SHA256SUMS
# goes up last: it is the pointer. install.sh and `datum upgrade` read the
# binary's name from it, and fetch the signature of the build it names. So a
# run cut short before it, or GitHub's download cache serving the previous
# SHA256SUMS after it, leaves installs on the previous build, whose files
# are still there.
# An upload cut short leaves a hidden asset in state `starter` that
# `--clobber` cannot see; each run deletes those first, and once the pointer
# moves, every build but the newest three.
#
# The fixed names datum-<arch>-<os> and SHA256SUMS.minisig go up too, after
# the pointer, the binaries allowed to fail, and SHA256SUMS lists them: a
# datum from before build names upgrades by them, once.
#
# `publish.sh <dist> <tag>` publishes a release that STAYS instead (da-ec8y8):
# the release of the forge's tag v<major>.<minor>.<height> (kept-tags.sh), the
# build-named binaries, SHA256SUMS and its signature alone, never pruned and
# never GitHub's latest. `datum upgrade --to <version>` installs from it, and
# install.sh with DATUM_RELEASE_URL=…/releases/download/<tag>.
#
# Needs gh (GH_TOKEN, GH_REPO), minisign, GITHUB_SHA and MINISIGN_SECRET_KEY.
set -euo pipefail

pub="$(cd "$(dirname "$0")" && pwd)/minisign.pub"
cd "$1"
tag=${2:-}
build=${GITHUB_SHA:0:7}

# The `latest` tag is the forge's: its push mirror pushes every tag with
# --mirror, so it deletes one GitHub alone holds, and GitHub drafts the
# release (the downloads 404). So this never creates or moves it, and refuses
# to publish when the mirror has not brought it. Its commit is whatever the
# forge's says; the notes and `datum --version` name the build's.
gh api "repos/$GH_REPO/git/ref/tags/${tag:-latest}" > /dev/null || {
  [ -z "$tag" ] || { echo "::error::No '$tag' tag on GitHub: kept tags come from the forge (.forgejo/workflows/kept-releases.yml) through its mirror."; exit 1; }
  echo "::error::No 'latest' tag on GitHub. It must come from the forge, whose push mirror deletes a tag only GitHub has: push it there (git push origin <main sha>:refs/tags/latest) and let the mirror sync."
  exit 1
}

fixed=(datum-*)
for f in "${fixed[@]}"; do cp "$f" "$f-$build"; done
# A kept release has no datum from before build names to serve.
[ -z "$tag" ] || rm "${fixed[@]}"
sha256sum datum-* > SHA256SUMS
cat SHA256SUMS
key=$(mktemp)
trap 'rm -f "$key"' EXIT
printf '%s\n' "$MINISIGN_SECRET_KEY" > "$key"
# The key has no password (`minisign -G -W`); one that has fails here at
# once, where an open stdin would wait for it.
minisign -S -s "$key" -m SHA256SUMS -x SHA256SUMS.minisig -t "datum ${GITHUB_SHA}" < /dev/null
# As datum verifies it, before anything is uploaded: prehashed (-H; datum
# refuses minisign's legacy form) and by the key it is built with, so a
# secret that is not that key fails here and publishes nothing that new
# binaries refuse.
minisign -VHq -m SHA256SUMS -P "$(sed -n 2p "$pub")" || {
  echo "::error::SHA256SUMS.minisig does not verify, prehashed, with release/minisign.pub: MINISIGN_SECRET_KEY is not its secret key, or this minisign signed in the legacy form. Nothing was uploaded."
  exit 1
}
cp SHA256SUMS.minisig "SHA256SUMS-$build.minisig"

if [ -n "$tag" ]; then
  kept=(datum-*-"$build" SHA256SUMS "SHA256SUMS-$build.minisig")
  if gh release view "$tag" > /dev/null 2>&1; then
    # Once more on gh's 422 collision, as latest's upload below (da-c1gef).
    # SHA256SUMS goes up in the same pass as the binaries here, so a second
    # pass is safe as is (da-juqx4).
    gh release upload "$tag" "${kept[@]}" --clobber || gh release upload "$tag" "${kept[@]}" --clobber
  else
    gh release create "$tag" "${kept[@]}" --verify-tag --latest=false --title "datum ${tag#v}" \
      --notes "The datum CLI built from ${GITHUB_SHA}, kept at a sync floor. Install: curl -fsSL https://datum.day/install.sh | DATUM_RELEASE_URL=https://github.com/$GH_REPO/releases/download/$tag sh, or datum upgrade --to ${tag#v}"
  fi
  exit 0
fi

# Each asset's id, state and name.
assets() { gh api "repos/$GH_REPO/releases/tags/latest" --jq '.assets[] | [.id, .state, .name] | @tsv'; }
drop() { for id in "$@"; do gh api -X DELETE "repos/$GH_REPO/releases/assets/$id" > /dev/null; done; }
starters() { awk -F'\t' '$2 != "uploaded" { print $1 }'; }

notes="The datum CLI built from ${GITHUB_SHA}. Install: curl -fsSL https://datum.day/install.sh | sh"
if gh release view latest > /dev/null 2>&1; then
  drop $(assets | starters)
  # A run again at the same commit clobbers the binaries SHA256SUMS names:
  # gh deletes each before its upload, so an install meanwhile 404s and is
  # run again. Any other run uploads names no SHA256SUMS holds.
  # gh retries an upload itself, and its retry can meet an asset GitHub
  # already took: HTTP 422 «already exists», which --clobber, looked at once
  # before, does not see (da-c1gef). A second pass sees it and clobbers it.
  # Once, not a loop: what fails a second time is not that collision.
  up=(datum-*-"$build" "SHA256SUMS-$build.minisig")
  gh release upload latest "${up[@]}" --clobber || gh release upload latest "${up[@]}" --clobber
  gh release upload latest SHA256SUMS --clobber
  # A datum from before build names reads this one, and refuses the new
  # SHA256SUMS until it is up and both are served.
  gh release upload latest SHA256SUMS.minisig --clobber
  gh release upload latest "${fixed[@]}" --clobber ||
    echo "::warning::The fixed-name binaries did not all upload: a datum from before build names cannot upgrade until a run uploads them."
  gh release edit latest --notes "$notes"
else
  gh release create latest datum-*-"$build" "SHA256SUMS-$build.minisig" SHA256SUMS SHA256SUMS.minisig "${fixed[@]}" \
    --verify-tag --title "datum (latest main)" --notes "$notes" --latest
fi

# The pointer names this build, the newest: keep it and the two before it.
# `builds` sets b to an uploaded build-named asset's build (a binary or the
# signature). The first pass
# lists the three builds holding the highest asset ids, the newest uploads;
# the second drops every build-named asset of another build, and the starters.
list=$(assets)
builds='{ b = "" } $2 == "uploaded" && ($3 ~ /^datum-[^-]+-[^-]+-[0-9a-f]+$/ || $3 ~ /^SHA256SUMS-[0-9a-f]+\.minisig$/) { b = $3; sub(/\.minisig$/, "", b); sub(/.*-/, "", b) }'
keep=$(awk -F'\t' "$builds"' b { print $1, b }' <<< "$list" | sort -nr | awk '!seen[$2]++ && ++n <= 3 { print $2 }')
case " $(echo $keep) " in
  *" $build "*) ;;
  *)
    echo "::error::The prune would delete this build's own binaries, which SHA256SUMS names ($build among: $(echo $keep)): nothing pruned. Asset ids no longer order the builds: delete the stale datum-*-<sha7> and SHA256SUMS-<sha7>.minisig assets by hand, and the next run prunes again."
    exit 1
    ;;
esac
drop $(starters <<< "$list") $(awk -F'\t' -v keep=" $(echo $keep) " "$builds"' b && !index(keep, " " b " ") { print $1 }' <<< "$list")
