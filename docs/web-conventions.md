# web/ conventions — how datum.day is built

What the code already does, written down so the next pane, widget or test keeps it.
Every claim here was read off `web/src` on 2026-09-12; where a convention is decided but
not yet everywhere, it says so and names the bead. If you change one of these, change this
file in the same commit — a convention nobody can read is a convention nobody keeps.

## The seam: the CLI is the app

1. **Every read and every write is a datum command.** `run(argv, stdin?)` (`$lib/run.ts`) is the
   one door; behind it `db.ts` → `db.worker.ts` → `cli.ts` runs the real CLI as one WASI process
   per command against the SQLite file on OPFS. **No TypeScript touches the database** — only
   `cli.ts` opens OPFS (with `storageState.ts` asking the browser whether it may keep it).
   If a pane needs something the CLI cannot say yet, the change belongs in the CLI.
2. **`localStorage` is for what belongs to this device and this device only**: the layout draft
   (`layout/draft.ts`), which layouts are open (`layout/current.ts`), the command pane's line,
   the REPL scrollback, the device id, the widget picker's last choice. Never entries.
3. **One beat.** After any command, `engine.ran()` bumps `epoch` and rebuilds the fields widget;
   every pane takes `export let epoch` and re-reads. A write in another tab arrives through
   `changes.ts` and joins the same beat. Do not invent a second refresh path, and do not try to
   decide which verbs write — the CLI's list would drift from ours.
4. **The argv that runs is the argv that is echoed.** A widget composes an argv and dispatches it;
   the prompt shows `result.argv`. Nothing rebuilds a display string from a press — a
   reconstruction can drift from the command, and the echo is how a person learns the CLI.
5. **The bar's pills go after the verb** (`engine.pressWithPills`): the CLI takes no flag before
   its verb, so a time cursor lands on a chip press exactly as on a typed line.

## Panes that render at their own size

6. A pane that asks the CLI for a picture at its own width **measures its box through
   `settle`** (`layout/resizing.ts`) — a border drag resizes every frame, and one render per
   frame is jitter — **guards its runs so the last STARTED wins** (not the last to return), and
   **re-reads on `epoch`**. `CliPane`, `DayStrip`, `TailTable` and `Stats` each keep this rule;
   they are not one abstraction, and three similar things is not a reason to make one.

## Modules

7. **Every module opens with a sentence saying what it is, and why it is the way it is.**
   24 of 24 `.svelte` files and 34 of 37 `.ts` files do (`changes.ts`, `deviceId.ts` and
   `types.ts` are the gap).
8. **Pure logic lives beside its markup, in a `.ts` a test can call without a browser**:
   `fields.ts`, `stats.ts`, `tail.ts`, `panes.ts`, `bar.ts`, `cursor.ts`, `fileargv.ts`,
   `dbio.ts`. The `.svelte` file holds the markup and the wiring.
9. **One widget kind, one name.** A kind's id in `registry.ts` is a saved-layout key and cannot
   change without a migration; the title, the file names and the tests all read as the thing the
   reader sees. (`fields` is `FieldsWidget.svelte`, `layout/FieldsPane.svelte`, `fields.ts` and
   `fields.test.mjs` since da-850j; its id is `widgetbar` still, and a comment on the id says so.)

## Looks

10. **Colours come from the palette, not from a literal** (da-a2xl). `:root` in `app.css` names
    thirteen roles — `--page`, `--ground`, `--tint`, `--rule`, `--accent`, `--accent-bright`,
    `--muted`, `--quiet`, `--ink`, `--ink-bright`, `--alarm`, `--error`, `--warn` — each with a
    line saying what it is FOR, which is the contract: while a colour keeps doing that job,
    changing it is one line. 178 literals across 22 files became those tokens. What is left is
    left on purpose and says so where it stands: a one-off tint no role names (invent a token
    when a second place wants it, not before), a shadow, the beta mark's amber, a colour handed
    to an SVG attribute or parsed as hex from TypeScript — `var()` reaches neither —
    and `ansi.ts`'s sixteen, which are a terminal's slots and not this app's looks.
11. **A control a finger can hit is 24px** (`reachable.test.mjs` holds every `…` badge, every
    submenu icon and every root-bar button to it). `.control` in `app.css` is the rule set that
    makes one — the box, the 6px radius, the pane ground, the 1px rule, the muted ink — and
    `.control.badge` the square of them, sized by `--badge` (a pane's title row, a fingertip's
    24px under a coarse pointer). A component keeps only what is its own. It is global because
    svelte scopes a component's CSS, and for the same reason a hover stays LOCAL wherever
    another local rule has to lose to it: a scoped selector carries svelte's hash twice, so
    `.tab[aria-selected]` and `.act[aria-pressed]` outweigh anything `app.css` can say.

## Tests

12. **Drive the app the way a person does**: type into a prompt (`app.submit`) and read what the
    panes show — 15 test files do. The console runs commands the way the page does:
    `window.datum.run` / `runLine` go through `engine.press`, so they are echoed, receipted and
    followed by a refresh, exactly as typing is (da-6918 — a silent `run` cost a false bug
    report on 2026-09-11). `runQuiet` / `runQuietLine` are the exception and say so: the seam
    alone, nothing on the page moves, which is what the harness's `app.run` binds to so that a
    test can seed and read without disturbing what it is about to assert.
13. **Never assert a delta** ("more rows than before"): assert the thing itself — the count, the
    position, the text. A delta-blind probe cannot see output identical to what was there.
14. **A browser leg whose browser cannot be launched is skipped with its reason**, never failed:
    CI installs chromium's headless shell only, and a file whose every leg is skipped fails
    loudly rather than passing having examined nothing.
15. **Wait for the thing, not for a second.** No fixed sleeps: poll the condition with a
    deadline, so a slow machine does not turn a real assertion into a flake. **A wait's cap is a
    hang-stop, not a speed assertion** — it is there so a condition that will never come true
    reports itself instead of sitting until the runner's 120 s per-test budget expires. The suite
    has ONE tier: **30 s**, which is playwright's own default. So a playwright wait writes no
    `timeout:` at all, and the one hand-rolled poller (`tests/support/until.mjs`) defaults to the
    same number. A shorter cap asserts speed by accident and fails a test that had a hundred
    seconds left: nine sites at 2 s and 5 s were the whole of the suite's flake surface under load
    — seven failures across four runs, every one a short wait expiring, not one assertion (da-5t8q,
    da-eznx). If a test really does mean "and quickly", it says so in an assertion that names the
    number, never in a wait's cap.
16. **A test that seeds a time must say which day it means.** A bare `-t 0930` is "today at
    09:30" against whatever clock the app is holding, so before 10:00 local the same seed
    lands in the *future*: `datum now` answers that nothing is current and every pane that
    draws a current state draws none. Give the seed a date, or pin the app's clock with
    `withApp(fn, { clock: todayAt(15) })` — one instant for the page and for the worker the
    CLI runs in — and seed against it. Tests that skipped this passed every afternoon and
    failed every night, and the failure was twice charged to innocent branches (da-ldct).
