# Retrospectives

Per-polecat (and per-crew) retrospectives written at the end of work
cycles. Each `RETROSPECTIVE-daydumb-<name>.md` captures what one author
learned about the codebase from their seat — recurring pitfalls, the
shape of bugs they kept hitting, and one non-obvious improvement they'd
take if given a free week.

New retros follow the naming convention
`RETROSPECTIVE-daydumb-<name>.md`, where `<name>` is the polecat color
(`quartz`, `onyx`, `jade`, …) or the crew name (`vremya`).

## Form

Active retros live in this directory. The expected form (varies, but
this is the template most polecats converge on):

1. **Where I lived** — files touched, commits, headline beads.
2. **What was harder than expected.**
3. **What was easier than expected.**
4. **Bugs I noticed but wasn't asked to fix** — `path:line` form.
5. **The non-obvious improvement (the only section that matters)** —
   one concrete refactor or addition the author would prioritize.
6. **My area's relationship to the whole.**
7. **Closing.**

§5 is the most load-bearing part — mayor uses these to seed epics. Past
retros' §5s became `da-0lp3` (aliases-derived-on-read), `da-5895`
(render-layer unification), `da-jflh` (mega-file carve-up), and others.

## Closing a retro

When a retro's work-items have substantially shipped (or have been
filed as tracked epics), close it:

1. Append a `## Status (as of YYYY-MM-DD)` section to the file
   listing what shipped from §5 / §4 / §2, what's still open (with
   bead IDs), and which polecats actioned what.
2. Move the file to [`archive/`](./archive/).
3. The author voice is preserved in archive — these are durable
   artefacts of what the codebase looked like from a specific seat
   at a specific time, even after the work-items they recommended
   are done.

The first batch (the May 2026 polecat retros + the polecat-meta retro)
was archived on 2026-06-02. See [`archive/`](./archive/).
