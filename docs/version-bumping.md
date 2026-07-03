# Version bumping (da-qlfk)

The workspace version (`[workspace.package] version` in `Cargo.toml`, inherited
by every crate via `version.workspace = true`) auto-tracks `main`: each merge
bumps it. This pairs with the on-merge auto-upgrade (`datum-upgrade`) — same
"bake-it-into-the-path" idea, so versions move without anyone remembering to.

## Policy

Conventional-commit driven, and **0.x until production** (the major is never
bumped automatically):

| Merged commit subject        | Bump  | Example         |
| ---------------------------- | ----- | --------------- |
| `feat: …` / `feat(scope): …` | minor | `0.1.4 → 0.2.0` |
| anything else (`fix:`, `docs:`, unprefixed, …) | patch | `0.1.4 → 0.1.5` |
| `chore(release): …` (our own bump) | none  | no-op           |

The bump kind is read from `HEAD`'s commit subject. Since the tool skips its
own `chore(release):` commits, a re-run — or a push that re-triggers the hook —
never chains a second bump.

Want a minor bump the policy wouldn't pick? Land the feature under a `feat:`
squash message. (History already uses conventional prefixes consistently, so
this is automatic in practice.)

## The command

```
cargo run -q -p xtask -- bump-version [--push] [--dry-run] [--subject "<msg>"]
```

Run it from the checkout root (it locates the workspace the same way the other
`xtask` tasks do). It rewrites `Cargo.toml` and keeps `Cargo.lock` in step
(every local crate shares the workspace version), then commits
`chore(release): vX.Y.Z`. `--push` also pushes. `--dry-run` prints the decision
and writes nothing. `--subject` overrides the commit-subject signal (otherwise
`git log -1 --format=%s`).

## Wiring into the merge path

Bump on the merge host **in a push-capable checkout of `main`**, right after the
merge lands and before/with the rebuild. The merge-queue `post_merge_cmd` hook
(rig `settings/config.json`) is the seam — the same hook that already runs
`datum-upgrade`. Point it at a wrapper that bumps first, then upgrades, e.g.:

```sh
cd <push-capable main checkout> \
  && git fetch origin && git reset --hard origin/main \
  && cargo run -q -p xtask -- bump-version --push \
  && datum-upgrade
```

`datum-upgrade`'s own `~/src/daydumb` clone is fetch-only (no push creds), so
the bump must run where `git push` to `main` works — not inside that clone.
