# Flake registry

> **Last re-tested 2026-09-12** by can onyx (da-tb9v) — 3 entries checked,
> 3 removed, 0 surviving. Every entry was re-run 10× under the conditions it
> named; none reproduced. The registry is empty: a failure you hit today is
> not a known flake. The removals' evidence is in da-tb9v; the entries
> themselves are in git (`git log -p FLAKES.md`).

This file catalogues tests in the daydumb workspace that are known to fail
without indicating a real regression. Polecats and contributors consult it
during the pre-merge gate (`scripts/check.sh`) so a known-noise failure
does not derail an otherwise-good submission.

## Categories

- **FLAKE** — non-deterministic, retry-passes. **Action:** retry once or
  twice; only investigate further if it stops retry-passing.
- **BRITTLE_GOLDEN** — deterministic, but the golden fixture is sensitive
  to incidental output changes (color, whitespace, struct ordering).
  **Action:** verify the output is semantically correct, then regenerate
  the golden; do not revert the code change.
- **ENV_DEPENDENT** — passes locally but fails in CI (or vice versa).
  **Action:** check `RUST_TEST_THREADS`, env vars, OS-specific behaviour.
- **TIMING_DEPENDENT** — race conditions, timezone boundaries.
  **Action:** pin the clock (`MockClock`), serialise the test, or skip
  in parallel runs.

## Read protocol

Before submitting to the merge queue, run `./scripts/check.sh`. If
anything fails, look up the failing test name below.

- If the entry's recommended action is **retry**, retry once or twice.
  If it then passes, bump the entry's **Last seen** date and commit the
  bump alongside your work.
- If it is **regenerate golden**, verify the new output is semantically
  correct (visual inspection / manual diff) and regenerate the golden
  in the same commit.
- If the failing test is **not** listed here and you suspect noise,
  retry once. If it retry-passes, add a new entry below as part of the
  same diff (see Append protocol).

## Append protocol

- **Polecat observes a flake mid-work:** edit this file in the WIP
  branch, add or update the entry, commit alongside the actual work. No
  cross-agent coordination needed — the registry update lands with the
  merge.
- **Polecat trips on a known FLAKE entry:** consult the recommended
  action, retry, and update **Last seen** if retry passed.
- **Witness / refinery observes failure-then-pass during MQ
  processing:** post a brief note to mayor mail with the test name and
  verdict (e.g. `retry-passed, looks like a flake`). Mayor batches
  these into FLAKES.md updates on patrol cycles.

## Entry format

````markdown
## <fully-qualified test name>

**Test:** `<crate-path>::<module>::<test_fn>`
**Type:** FLAKE | BRITTLE_GOLDEN | ENV_DEPENDENT | TIMING_DEPENDENT
**Symptom:** one-line description of how the failure presents
**Recommended action:** retry, regenerate golden, investigate, …
**Last seen:** YYYY-MM-DD
**Notes:** optional — root cause hypothesis, related beads, anything else
````

---

_No live entries._ The last three were removed on 2026-09-12 after 10 clean
runs each (da-tb9v). Add one via the Append protocol above when you see a
failure retry-pass — and date it, so the next re-test can tell live noise
from archaeology.
