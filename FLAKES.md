# Flake registry

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

## sync_config_set_server_global

**Test:** `crates/datum-cli/tests/sync_config.rs::sync_config_set_server_global`
**Type:** FLAKE
**Symptom:** Fails on first run when prior tests leave stale state in
the shared global config file.
**Recommended action:** Retry once. If it fails on retry, the
underlying test-isolation problem still needs fixing.
**Last seen:** 2026-04-22
**Notes:** Observed by refinery during merge of da-wisp-ces (garnet's
D2 parser MR, commit 70ab3a2). A real fix would use per-test isolated
config rather than mutating the shared global.

## color_mode_emits_truecolor_backgrounds

**Test:** `crates/datum-commands/tests/render_fixtures.rs::color_mode_emits_truecolor_backgrounds`
**Type:** BRITTLE_GOLDEN
**Symptom:** Truecolor escape byte sequence drifts when switch/state
rendering is touched, even when visual output is unchanged.
**Recommended action:** Compare visual output (terminal preview /
manual inspection). If correct, regenerate the golden. Do not revert.
**Last seen:** 2026-04-28
**Notes:** Tripped on da-7r4 (topaz, "render Switch entries as coloured
regions") and da-uuu (obsidian, state.rs perf rework). Polecats
touching `state.rs`, switch rendering, or color-mode handling should
run this test early so a regen lands in the same diff.
