# Cross-version changeset goldens (da-4zoa)

Each `changeset_*.json` in this directory is a **frozen record of what some
build actually put on the wire**. `tests/changeset_goldens.rs` feeds each one to
today's `datum_sync_client::ops::apply_changeset` and asserts, column by column,
what ends up in the entries table.

The class of bug it watches: nothing on the changeset payload sets
`deny_unknown_fields`, so a build that stops reading a field drops it in
**silence**. No error, no count change — two peers on the same build agree
perfectly while both lose the same column, which is why neither a count
comparison nor a same-build content comparison can see it. The fold in
`crates/datum-sync-client/src/ops.rs::adopt_legacy_uncertainty` exists because
this already happened once (da-b8rs moved `occur_time_precision` /
`dur_precision` onto the values they describe). These goldens are the thing that
goes red the day a rename lands without its fold.

## ⚠ NOTHING MAY REGENERATE THESE FILES

**A regeneration script here is a defect, not a convenience.** The instinct to
add one is strong and wrong.

`web/tests/fixtures/cli-parity.json` has a regeneration gate
(`.github/workflows/ci.yml`) and that is correct *for a parity fixture*: it
asserts today's code still produces today's recorded output, so when the code
legitimately changes you regenerate and review the diff.

A **cross-version** golden asserts the opposite thing — that a payload written
by an *older* build still decodes. Its entire value is that no current code
produced it. Regenerate it and the artefact under test has been replaced by the
thing it was tested against; every future run passes, proving only that today's
writer agrees with today's reader. Nothing goes red. The suite grows a test that
can never fail, which is worse than no test at all, because the gap it was built
to watch now looks watched.

## How to add a wire shape

**Add a new golden; never edit an old one.** The set only grows: it is the set
of wire shapes that must still be understood, and the filenames are the
compatibility claim.

To capture one, build the payload with the build that is the shape you are
recording — check that commit out, pack a changeset, and save the bytes *after*
the 4-byte `DSC1` magic (see `docs/sync-protocol.md`). Then add its block to
`expected.json`. Both files here were captured that way, from a worktree at the
recorded commit.

## What `expected.json` may and may not change

The payloads are immutable. The *expectations* are not, and there is exactly one
legitimate reason to touch one: a new column was added to `Entry`, so an old
payload — which cannot carry it — now stores a default. `Row` in the test denies
unknown keys *and* requires every known one, so a new column forces every
expectation block to state what the old payload means for it. That edit is the
review moment this design wants; it cannot be automated away, and it must not
be, because "what should a v0 payload mean for a column invented after it" is a
human decision every single time.

An expectation may also flip `outcome` from `decodes` to `refused` — that is
what a deliberate compatibility break looks like, and it should be as visible as
this file makes it.

## The goldens

| File | Built at | Outcome today |
|---|---|---|
| `changeset_v0_2026-09-05_hash-v1.json` | `1840fb8e` (da-engu) | decodes |
| `changeset_v0_2026-09-05_no-hash-version.json` | `3a551f94` (da-uxgv, the commit before da-engu) | refused |

The two payloads are byte-identical but for `hash_version`, which is the point:
the only thing standing between an undeclared payload and a silent merge across
a `content_hash` definition skew is `check_hash_version`, and the second golden
is a real pre-declaration payload proving it holds.

Both were produced under protocol `v0` with the `DSC1` changeset magic. The
golden is the **decrypted** payload, not the sealed blob: the encryption
envelope is a separate version axis (its own magic, its own key format), and
pinning a ciphertext here would freeze key material into the repo forever to
test a layer that cannot drop a field — an AEAD either opens or does not. What
this directory watches lives entirely inside the JSON.
