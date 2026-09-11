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
   `widgets.ts`, `stats.ts`, `tail.ts`, `panes.ts`, `bar.ts`, `cursor.ts`, `fileargv.ts`,
   `dbio.ts`. The `.svelte` file holds the markup and the wiring.
9. **One widget kind, one name.** A kind's id in `registry.ts` is a saved-layout key and cannot
   change without a migration; the title, the file names and the tests should all read as the
   thing the reader sees. (Today `fields` is also `widgetbar`, `WidgetBar.svelte`,
   `BarWidget.svelte` and `widgets.ts` — the id stays, the files are being renamed.)

## Looks

10. **Colours come from the palette, not from a literal.** Decided 2026-09-12 and NOT yet done:
    48 literals are in the components today, the top five repeated 15–36 times across 8–18 files.
    One `:root` set names the roles — ground, rule, accent, muted, quiet, alarm, error — and the
    components use them.
11. **A control a finger can hit is 24px** (`reachable.test.mjs` holds every `…` badge, every
    submenu icon and every root-bar button to it). The rule set that makes one — the box, the
    6px radius, the pane ground, the 1px rule — belongs in one class, not copied into six
    components as it is today.

## Tests

12. **Drive the app the way a person does**: type into a prompt (`app.submit`) and read what the
    panes show — 15 test files do. `window.datum.run` writes without bumping the epoch, so a
    pane will NOT refresh after it: it is for reading state, not for driving behaviour (this
    cost a false bug report on 2026-09-11).
13. **Never assert a delta** ("more rows than before"): assert the thing itself — the count, the
    position, the text. A delta-blind probe cannot see output identical to what was there.
14. **A browser leg whose browser cannot be launched is skipped with its reason**, never failed:
    CI installs chromium's headless shell only, and a file whose every leg is skipped fails
    loudly rather than passing having examined nothing.
15. **Wait for the thing, not for a second.** No fixed sleeps: poll the condition with a
    deadline, so a slow machine does not turn a real assertion into a flake.
