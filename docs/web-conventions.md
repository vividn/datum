# web/ conventions — how datum.day is built

What the code already does, written down so the next pane, widget or test keeps it.
Every claim here was read off `web/src` on 2026-09-12, and §1, §2, §4 and §9 were driven
against it again on 2026-09-22 (da-hm8p1); the rest still carry the older date. Where a
convention is decided but not yet everywhere, it says so and names the bead. If you change one
of these, change this file in the same commit — a convention nobody can read is a convention
nobody keeps.

**State a rule as a PREDICATE, never as a closed list of files** (da-hm8p1). The predicate is
usually already written, with its mechanism and its bead, in the module it governs — so cite
that module and let the names below be examples. A list has to be remembered to be obeyed, and
what is remembered drifts: §1, §2 and §9 were each rewritten from a roster into their rule
after their rosters went wrong, §9's in both directions at once. §4 is the counter-example and
the reason to believe this: *“the argv that runs is the argv that is echoed”* is one reactive
binding in `TrackModal.svelte` read by both the submit and the preview, so no second array
exists for the doc to be out of date about. Where a rule can be made structural, that beats
writing it down.

## The seam: the CLI is the app

1. **Every read and every write is a datum command.** `run(argv, stdin?)` (`$lib/run.ts`) is the
   one door; behind it `db.ts` → `db.worker.ts` → `cli.ts` runs the real CLI as one WASI process
   per command against the SQLite file on OPFS.
   **No TypeScript reads or writes a database's CONTENTS.** The CLI is the only thing that does;
   the TypeScript around it only hands it the files — `cli.ts` takes a sync access handle on every
   file SQLite will touch and `wasiFs.ts` maps those handles onto the preopened directory the
   process runs against, with `storageState.ts` asking the browser whether it may keep them.
   The MECHANISM is the lock, and it is what makes this a predicate rather than a roster: a running
   command holds those handles, so contending with them is the failure (da-dvzx). Whatever takes no
   lock is fine. `databases.ts` is the worked example — it lists the pool from the PAGE and by NAME
   only, and its own header says why: *“a directory listing takes no lock, where reading a file's
   contents would contend with the sync access handle a running command is holding. Nothing here
   opens, creates or writes anything.”*
   ⚠ *This line read “only `cli.ts` opens OPFS” until 2026-09-22, which is a closed list and made
   `databases.ts` — correct code, with its reason written in it — look like a violation. Ask the
   predicate, not the list.*
   If a pane needs something the CLI cannot say yet, the change belongs in the CLI.
2. **`localStorage` is for what belongs to this device and this device only, and never
   entries.** That is the whole rule, and a new key earns its place by being able to say that
   sentence about itself — not by joining the list below, which will go stale. Two modules
   already say it in their own words: `log.ts` and `tuning.ts` each annotate their key *“never
   synced: it is a screen's”*, which is the predicate, written twice, in the code.
   The keys, as examples (driven 2026-09-22 — nine files mention `localStorage`, six really call
   it): the layout draft, this device's open root tabs and its per-layout locks
   (`layout/draft.ts`, da-wpgy, da-0a5o, da-gshh); the custom pane's command line
   (`CustomPane.svelte`); the REPL scrollback (`replStore.ts`); the device id (`deviceId.ts`);
   how far the output bar is open (`OUTPUT_KEY`, `log.ts`); and the bar's feel — TAU and DETENT,
   which only a real thumb on a real phone can judge (`TUNING_KEY`, `tuning.ts`, da-o2wm8).
   `layout/current.ts`, `layout/shell.ts` and `layout/registry.ts` name it in prose only.
   `+layout.svelte` keeps one flag in `sessionStorage` — the same rule one scope tighter: a
   tab's, not a device's.
   ⚠ *A closed list rots at BOTH ends, and this one had done both by the time it was read again.
   `log.ts` and `tuning.ts` were missing, and they are the rule's best statements of itself. And
   “the widget picker's last choice” was never real: no key for it exists today, and none existed
   at `4719c3a4` — the commit that wrote this line — either. An omission reads as a violation and
   an invention reads as a rule; neither end announces itself.*
3. **One beat.** After any command, `engine.ran()` bumps `epoch` and rebuilds the fields widget;
   every pane takes `export let epoch` and re-reads. A write in another tab arrives through
   `changes.ts` and joins the same beat — COALESCED (da-s3pt): while one such refresh is
   outstanding, a burst next door marks one more after it rather than one each, so the handler
   passed to `onExternalChange` must be the whole refresh and must be awaitable. Do not invent a
   second refresh path, and do not try to decide which verbs write — the CLI's list would drift
   from ours.
   The one beat BESIDE it is the minute clock (da-r61i, `minute` in `$lib/clock.ts`): nowview and
   dayview are pictures of NOW — an open block's elapsed duration, the now-line — and a pane runs
   its command once, so they stood still between writes. One timer for the app (a `readable`,
   started by its first subscriber), on the wall-clock minute rather than 60 s from load, stopped
   while the document is hidden and ticking once on return if the minute turned. It is NOT the
   epoch: a pane folds the tick into its own request only when `dependsOnNow(argv)`
   (`$lib/panes.ts`) says the command reads the clock — `now`, `nowview`, `dayview`, unless pinned
   with an absolute `-t` or `--end-date` — and the day strip re-draws today's band and the geometry beside
   it. The tail is not re-run and the fields widget is not rebuilt. A test that counts a drawn
   pane's runs over a stretch of real time therefore pins the clock (`clock:` in `withApp`): a
   fixed `Date.now()` never turns a minute.
4. **The argv that runs is the argv that is echoed.** A widget composes an argv and dispatches it;
   the prompt shows THAT argv. Nothing rebuilds a display string from a press — a
   reconstruction can drift from the command, and the echo is how a person learns the CLI.
   It is the argv the CALLER asked for, not `result.argv`: the web appends flags of its own on
   the way through (`--width`, `--show json`, below), and those are the web's plumbing, not a
   command to teach (da-xfa4).
   It is shown BESIDE the prompt and never as the input's value (`bar.ghost`, da-jprg): a box
   that fills itself reads as text the reader typed and may edit. A line the reader really did
   type is cleared on Enter and does not come back at all.
5. **The bar's pills go after the verb** (`engine.pressWithPills`): the CLI takes no flag before
   its verb, so a time cursor lands on a chip press exactly as on a typed line. They are drawn
   as pills BESIDE the ghost rather than as words in it, and a press spends its unlocked ones —
   out of the live set at once, fading on screen with the ghost (`bar.spent`, da-xfa4).
6. **The web asks every command it runs for its echo as data** — `--show json` appended the way
   `fitArgv` appends `--width` (`echoArgv` in `$lib/echo.ts`, at `Repl.runArgv` and
   `engine.alone`). The bottom bar and the REPL then PLACE the CLI's own strings — action, hid,
   id, info, and its `data` items one to a row in a brace beside the two lines (da-f4mw) — and
   compose none of them; `data_inline` is the terminal's own join and the web does not use it. The document carries the CLI's
   own COLOURS too (da-yu9s): `field` and `state` as logged, `field_color` and `state_color` as
   `fields --json` resolves them, and `info_segments` — `info` split into `{text, color}` runs,
   which is what `Doc.svelte` paints the line from. The web never looks for the field inside
   `info` (`switch mood mood` names it twice) and never derives a colour of its own.
   `NO_ECHO` in that file is the
   denylist of commands with no action echo: it is a denylist because an argv whose first word
   is not a command (`mood switch happy`, an alias) is a write under another name and must get
   the flag, and `tests/echo.test.mjs` holds the list to what the CLI actually does. A stdout
   with no document in it falls back to the lines the CLI printed.

## Panes that render at their own size

7. A pane that asks the CLI for a picture at its own width **measures its box through
   `settle`** (`layout/resizing.ts`) — a border drag resizes every frame, and one render per
   frame is jitter — **guards its runs so the last STARTED wins** (not the last to return), and
   **re-reads on `epoch`**. `CliPane`, `DayStrip`, `TailTable` and `Stats` each keep this rule;
   they are not one abstraction, and three similar things is not a reason to make one.

## Modules

8. **Every module opens with a sentence saying what it is, and why it is the way it is.**
   **28 of 28 `.svelte` files and 40 of 40 `.ts` files do** — re-counted 2026-09-12 11:2x, and
   the three this line used to name as the gap (`changes.ts`, `deviceId.ts`, `types.ts`) have
   all been filled since. ⚠ *Count it accepting every comment syntax these files use: a `.svelte`
   module's purpose sentence usually sits inside its `<script>` as a `/* */`, not as an `<!-- -->`
   before it. A first pass looking only for `<!--` reported a file short and named `AnsiText.svelte`,
   which in fact opens with eleven lines of exactly this.*
9. **Pure logic lives beside its markup, in a `.ts` a test can call without a browser.** The
   `.svelte` file holds the markup and the wiring. What decides whether a module qualifies is
   not its name on a list but whether the function a test wants reaches the DOM or the seam:
   `fileargv.ts` states the predicate about itself — *“Pure and import-free, so node tests it as
   it is”* — and it is the sentence to write at the top of the next one.
   Sixteen modules are imported into node by the suite today (2026-09-22): `commands.ts`,
   `detent.ts`, `echo.ts`, `fields.ts`, `fileargv.ts`, `inspect.ts`, `lanes.ts`, `nowpoint.ts`,
   `scrollback.ts`, `selection.ts`, `track.ts`, `wasiFs.ts` and `layout/`'s `defaults.ts`,
   `draft.ts`, `registry.ts`, `tree.ts`. A module reaching a `.svelte` import is no obstacle:
   `tests/support/libhook.mjs` stubs those out and rewrites `$lib/x.js` to the source's `x.ts`,
   which is how `registry.ts` is node-testable at all.
   ⚠ *Being named here and being callable are two different claims, and this line was wrong in
   both directions. Of its nine names only three — `fields.ts`, `commands.ts`, `fileargv.ts` —
   are node-tested; thirteen that are do not appear. `stats.ts` and `panes.ts` import cleanly
   but no node test calls them. And `tail.ts`, `bar.ts`, `cursor.ts` and `dbio.ts` cannot be
   imported into node at all as the suite stands: each reaches `$app/environment` through
   `run.ts` / `database.ts` / `changes.ts`, which node cannot resolve and `libhook.mjs` does not
   stub. Stubbing `$app` would let the first three in — that, and an inventory test that simply
   RUNS this predicate instead of listing its members, is da-mgxdt.*
   `dbio.ts` is the one a stub would not save, and that is the module doing its job rather than
   breaking the rule: *“the browser's file dialog IS the command's stdin and a download IS its
   stdout”*. The dependence is isolated to `pick()`, a NON-exported helper that builds an
   `<input type="file">` — with `$app` stubbed, `fileCommand(['tail'])` returns `null` with no DOM
   at all, while `fileCommand(['db','restore','-'])` throws `document is not defined`. So the
   pure half already lives where this rule says it should, in `fileargv.ts`, and
   `tests/dbio.test.mjs` imports THAT from node while driving `dbio.ts` only through chromium and
   webkit, awaiting a real `filechooser`. Extracting the callable part and leaving the browser
   part where it belongs is the convention kept, not broken.
10. **One widget kind, one name.** A kind's id in `registry.ts` is a saved-layout key and cannot
   change without a migration; the title, the file names and the tests all read as the thing the
   reader sees. (`fields` is `FieldsWidget.svelte`, `layout/FieldsPane.svelte`, `fields.ts` and
   `fields.test.mjs` since da-850j; its id is `widgetbar` still, and a comment on the id says so.)
   The migration, when a kind GOES, is `RETIRED_KINDS` in `layout/shell.ts` — the id maps to the
   kind that took its work over, applied where layouts are read (`inline`) so a stored layout opens
   as the new kind rather than as an untitled empty pane, and is written back out by `extract`.
   The map is empty today: `input` was its one arm (→ `repl`, da-22vg) and the input widget came
   back under that very id (da-3zon), so a layout saved with one opens as one again — which is what
   a kind's id being a saved-layout key BUYS, and not only what it costs.
   A kind renamed BEFORE anyone has saved a layout naming it needs no arm at all — it is a plain
   rename, no migration, no alias, nothing to carry (`command` → `custom`, da-zqop, 2026-09-15,
   which freed `command` for the commands widget that took it, da-ltux); after that first saved
   layout it needs one.
   **A kind only one DATABASE offers says so with `db` on the kind** (da-o2wm8): the chooser lists
   `pickableIn(currentDb())`, that kind is out of `PICKABLE` and so out of `PICKABLE ⊆ BUILTIN` —
   the built-ins are every database's, and a gated kind's one-widget layout is the one its OWN
   database carries as a kit layout (`installKitLayout`, `layout/store.ts`), which `widgetInto`
   reaches exactly as it reaches a built-in. The gate is the database and not a build flag, so the
   widget reaches the deployed site, where the question it answers can be asked. And because a
   saved layout is the DEVICE's and opens in every database, the WIDGET says where it lives when it
   is drawn anywhere else — an empty pane and a crash are the two answers a reader cannot act on.

## Looks

11. **Colours come from the palette, not from a literal** (da-a2xl). `:root` in `app.css` names
    thirteen roles — `--page`, `--ground`, `--tint`, `--rule`, `--accent`, `--accent-bright`,
    `--muted`, `--quiet`, `--ink`, `--ink-bright`, `--alarm`, `--error`, `--warn` — each with a
    line saying what it is FOR, which is the contract: while a colour keeps doing that job,
    changing it is one line. 178 literals across 22 files became those tokens. What is left is
    left on purpose and says so where it stands: a one-off tint no role names (invent a token
    when a second place wants it, not before), a shadow, the beta mark's amber, a colour handed
    to an SVG attribute or parsed as hex from TypeScript — `var()` reaches neither —
    and `ansi.ts`'s sixteen, which are a terminal's slots and not this app's looks.
12. **A control a finger can hit is 24px** (`reachable.test.mjs` holds every button that opens a
    menu — a pane's title and the root bar's active tab (da-9vtw, da-wjob) — every badge beside
    one, every submenu icon and every root-bar button to it). `.control` in `app.css` is the rule set that
    makes one — the box, the 6px radius, the pane ground, the 1px rule, the muted ink — and
    `.control.badge` the square of them, sized by `--badge` (a pane's title row, a fingertip's
    24px under a coarse pointer, which is what the title's own box is sized by too). A component keeps only what is its own. It is global because
    svelte scopes a component's CSS, and for the same reason a hover stays LOCAL wherever
    another local rule has to lose to it: a scoped selector carries svelte's hash twice, so
    `.tab[aria-selected]` outweighs anything `app.css` can say.

## Tests

13. **Drive the app the way a person does**: type into a prompt (`app.submit`) and read what the
    panes show — 15 test files do. The console runs commands the way the page does:
    `window.datum.run` / `runLine` go through `engine.press`, so they are echoed, receipted and
    followed by a refresh, exactly as typing is (da-6918 — a silent `run` cost a false bug
    report on 2026-09-11). `runQuiet` / `runQuietLine` are the exception and say so: the seam
    alone, nothing on the page moves, which is what the harness's `app.run` binds to so that a
    test can seed and read without disturbing what it is about to assert.
14. **Never assert a delta** ("more rows than before"): assert the thing itself — the count, the
    position, the text. A delta-blind probe cannot see output identical to what was there.
15. **A browser leg whose browser cannot be launched is skipped with its reason**, never failed:
    CI installs chromium's headless shell only, and a file whose every leg is skipped fails
    loudly rather than passing having examined nothing.
16. **Wait for the thing, not for a second.** No fixed sleeps: poll the condition with a
    deadline, so a slow machine does not turn a real assertion into a flake. **A wait's cap is a
    hang-stop, not a speed assertion** — it is there so a condition that will never come true
    reports itself instead of sitting until the runner's 300 s per-file budget expires. The suite
    has ONE tier: **30 s**, which is playwright's own default. So a playwright wait writes no
    `timeout:` at all, and the one hand-rolled poller (`tests/support/until.mjs`) defaults to the
    same number. A shorter cap asserts speed by accident and fails a test that had a hundred
    seconds left: nine sites at 2 s and 5 s were the whole of the suite's flake surface under load
    — seven failures across four runs, every one a short wait expiring, not one assertion (da-5t8q,
    da-eznx). If a test really does mean "and quickly", it says so in an assertion that names the
    number, never in a wait's cap.
17. **A test that seeds a time must say which day it means.** A bare `-t 0930` is "today at
    09:30" against whatever clock the app is holding, so before 10:00 local the same seed
    lands in the *future*: `datum now` answers that nothing is current and every pane that
    draws a current state draws none. Give the seed a date, or pin the app's clock with
    `withApp(fn, { clock: todayAt(15) })` — one instant for the page and for the worker the
    CLI runs in — and seed against it. Tests that skipped this passed every afternoon and
    failed every night, and the failure was twice charged to innocent branches (da-ldct).
18. **A defect that only shows across a MOUNT needs a test that remounts — and asserts
    twice.** Some state is per-mount while the thing it describes is persisted, so a
    single-session test cannot see the bug at all. `CustomPane`'s write guard is *learned,
    not declared* — `CliPane` discovers that a command writes by running it and seeing
    `changed` — while the command line lives in `localStorage`: the command survived a reload
    and the knowledge about it did not, so every mount re-learned by writing once, and a user
    got one unrequested entry per refresh. **A test that applied the command and asserted one
    row passed on the broken code.** What catches it: reload and count again; count the
    **store** (`entries`), never the pane's text, because the pane redraws either way and
    cannot tell a re-run from a redraw; assert a fresh field is **0 before anything**, or every
    number in the test is meaningless; and loop **two** reloads, because one cannot distinguish
    "runs once more" from "runs once per mount" (da-v36y — 1→2→3→4 across three reloads on
    `d8d7724`, 1→1→1→1 after the fix, and the new test confirmed to FAIL on the old components:
    *reload 1 ran `occur kettle` again, 2 !== 1*).
19. **Locate a control by its identity, not by a label that tracks state.** This app's
    aria-labels move with state, so a selector written from one state times out in another.
    The pane menu is "menu of the *active tab's* title pane", so `menu of the repl pane`
    disappears the moment that pane shows another tab — use
    `section[data-pane="repl"] button[aria-label^="menu of the"]`, since the identity does not
    move. The root bar's restore ICON read **"restore a version"** when the layout was clean
    and **"restore layout"** when it was dirty; da-wjob took that row away — the act is
    `Restore layout` in the root menu now, one label in both states, marked when the layout is
    modified. The lesson stands wherever a label still tracks state. And the menu labels carry a **trailing space**
    (`"Add tab "`, `"Close pane "`, `"Add widget "`), so an anchored text regex can never
    match, while `getByRole('button', …)` misses a `<button role="menuitem">` because an
    explicit `role` overrides the implicit one — ask for the role the element **declares**, by
    accessible name. Three timeouts and one false conclusion came from this on 2026-09-12/13:
    a failed anchor on `/^add widget$/i` produced the belief "the button is `+`", which then
    sat in working knowledge for a day and sent a later session hunting the wrong control.

20. **A test file is the unit of a run.** Each file gets its own Chromium and its own
    `before(startApp)`, so files are what CI splits across runners: `web/scripts/test.mjs
    --shard N/M` (or `DATUM_TEST_SHARD=N/M`) takes the sorted `tests/*.test.mjs` by
    `index % M === N - 1` and prints what it took, and GitHub's Web job runs four of those
    (da-buqa — 60 files at ~26 minutes had outgrown one 4-core runner's 30-minute cap, so main
    was red-or-cancelled for a day and every push deployed blind). Two things follow: **no file
    may depend on another file having run** — shard 3 does not see shard 1's database — and a
    file that grows past the per-file budget (300 s since 2026-09-16, `--test-timeout` in
    `test.mjs`, one number for every workflow) is still its own loud failure, not something a
    bigger runner hides.

21. **An app boot is the unit of cost; don't buy one you are not going to look at.** Measured
    on the 24-core build box (da-x6ea): `withApp` on the **day page is 2.4–2.8 s** — 0.13 s of
    context, page and navigation, ~0.9 s to the seam being ready (worker, wasm compile, OPFS,
    schema) and 1.1–1.3 s for the frame's six panes, the day strip alone being one `dayview`
    process per band it holds. On the **one-pane `repl` layout it is 1.65 s**, and
    `page.setViewportSize()` is **10 ms**. So a file is, to a first approximation, its number
    of `withApp`s times one of those two numbers, and the three ways to spend less are:
    - **`{ query: 'repl' }`** for a test the day page is only scenery for. The bottom bar is
      drawn by `+layout.svelte` above whatever tree is in front, so it is the same bar on every
      page; a test that reads the seam, the bar or the banner does not need the frame behind it.
      A test that reads a pane, the fields widget or the strip does, and says so.
    - **`atEachScreen(t, page, screens, body)`** (`tests/support/app.mjs`) for a size matrix: a
      subtest per screen — so the report still names the size that failed — against ONE app,
      resized, two animation frames apart so the ResizeObservers have run.
    - **`seedLines(app, lines)`** (same file) for seeding: every `submit` bumps the frame's
      epoch and re-runs every pane, so N lines through the REPL bought N whole redraws when
      only the last one is ever looked at. `seedLines` sends all but the last through the quiet
      door and the last through the bar.

    And a fixed sleep is worth reading against what it is waiting for: the in-memory fallback
    goes back for the files at 2 s and doubles (2, 6, 14 s after it gave up), so an outage
    scripted to end at 8 s is a coin toss between the 6 s try and the 14 s one — 5 s sits
    between two tries and asks the same question in half the time.
