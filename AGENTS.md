# daydumb: a guide for contributors and coding agents

## What this is

**daydumb** is a personal life-data tracker. The CLI command is `datum`.
Inspired by https://github.com/vividn/datum (a 10-year personal project by the author).

Core purpose: let users manually log data about their own lives and review it as visualized timelines and statistics. The act of logging forces mindfulness. The visualization is "data as art."

## Repo layout

```
crates/
  datum-core/         # Rust library: data model, SQLite, field specs
  datum-parse/        # Pure: argv + time parsing, verb truth table, entry identity
  datum-cli/          # `datum` binary: clap CLI, commands, output formatting
  datum-commands/     # Command-layer crate: typed + string APIs, dispatched by datum-cli
  datum-layout/       # Pure: dayview/nowview geometry + SVG render
  datum-parity/       # Cross-runtime parity tests against the original datum (vividn/datum, TypeScript)
  datum-sync/         # Easy Sync natively: the wire crate over `Db`, `sync run`, the QR
  datum-sync-wire/    # Easy Sync's session protocol and iroh endpoint over a `Store` trait; also wasm32-unknown-unknown (datum.day's sync worker)
  datum-test-util/    # Shared harness for tests that mutate process-global state (env vars, TZ)
web/            # SvelteKit PWA: static, the real CLI as WASI on OPFS, offline-capable
                # its conventions — the seam, the beat, the panes, the tests: docs/web-conventions.md
```

(Flutter/Android/Wear OS clients are planned but the directories don't
exist yet; legacy datum migration logic lives in
`crates/datum-core/src/migrations/` and `commands/db/import.rs`, not
a top-level `migrations/` folder.)

## Core principles (never violate these)

1. **CLI startup must be near-instantaneous** — the original datum (Node.js) took ~1s which is unacceptable. Rust gives us <5ms. Never add synchronous overhead to the hot path (logging an entry).
2. **Local first** — all data stored on device. Network only for sync.
3. **End-to-end sync** — copies sync peer to peer (Easy Sync, over iroh), and a relay only forwards traffic it cannot read. Never put decryption logic on a relay.
4. **Flexible data model** — both freeform and schema-defined data must work without friction.

## Data model fundamentals

### Entry fields
- `id` (`hid`): internal, prefix-addressable, sortable. **Primary key and the durable identity, local and cross-device** — sync merges key on it and never rewrite it.
- `slug`: `field:2024-03-16T09:15Z` (human reference, datum-compatible). Derived from field + occur_time, so `retime` rewrites it — **mutable, not an identity**.
- `content_hash`: SHA-256 dedup key
- `field`: what is being tracked (e.g. "coffee", "mood", "project", "word")
- `verb`: the STORED classification, derived from state + dur — `point` | `switch` | `block` | `block-switch` | `point-switch` (`crates/datum-parse/src/verb.rs`). The words a person types — `occur`, `start`, `stop`/`end`, `switch`, `note`, `add` — are the COMMANDS; they set the state and the duration the verb is derived from.
- `state`: the primary JSON value (the state for a switch, the quantity for an occur). **There is no `value` field** — `state` is the canonical name, and a `value` appearing in new code is a bug, not a synonym.
- `following_state`: the trailing-edge state of a switch-style verb; meaningful only alongside a `dur`.
- `data`: arbitrarily nested JSON. (Not `extra_data`.) Nothing infers a unit: `weight=75.2kg` stores the string `"75.2kg"`.
  datum has no units anywhere, in specs, subfields or entries.
  The only planned form is a typed value (`u://`/`unit://`), not built.
- `occur_time` + `dur`: the one instant and the span it covers — see the time model below. There are no `time_start` / `time_end` columns.
- the zone travels with the time (`occur_time_tz`): an IANA name, e.g. "America/Chicago"
- `create_time` / `modify_time` / `client_id`

### Time model
All four combinations are valid as INPUT:
- `time_start` only — open/ongoing event
- `time_start` + `time_end` — complete block, dur derived
- `time_start` + a duration — end derived
- `time_end` + a duration — start derived

What the row actually stores is `occur_time` + `dur`, and `occur_time` is
the block's END: the duration is the span that PRECEDES it (matching the original datum's `durationBlocks` view). So a `(start, end)`
pair lands as `occur_time = end`, and a bare `start` with no end — an
open block — is the one shape whose `occur_time` is its leading edge,
because it has no other instant to store.

### `stats`' per-day cache
`stats` reads a window's whole closed days from `stats_cache` (one row per
field, local day and state) and computes only its partial edges and today
live (`commands/stats/cache.rs`), so all time costs what a day does. A
field's periods line stays one read of the year it reaches back, which is
cheaper than its nine windows' cached days and edges; the cache serves a
field's window only when it starts before that year. It is
local and derived: never synced, wiped when its stamp (`_meta`
`stats_cache`: version, host zone, schema version) differs, refilled from
the first day a write reached. Triggers on `entries` record that day for
every write path, and a row that changes the database's zone forgets every
field from it, since it moves their midnights. The tables, triggers and the
index they read are `db/stats_cache.sql`, laid down by the v55→v56 migration
and after `schema.sql` at every open; a later migration that drops or
rebuilds an `entries` column they name must drop them first. A span's seconds are `floor(end) − floor(start)`, which adds up
exactly across midnights. `datum check stats` recomputes a sample of days and says how many.

## CLI conventions

### Time flags (datum-compatible)
- `-y` / `-yy` / `-yyy` = 1/2/3 days ago (stackable)
- `-q` / `-qq` / `-qqqq` = 5/10/20 min ago (stackable, 5 min each)
- `-d -1` = yesterday, `-d +1` = tomorrow (day offset)
- `-t -8` = 8 min ago, `-t +5` = 5 min ahead (minute offset)
- `-t 1513` = 15:13 today, `-t 9:15am` = natural time
- `-d yesterday` / `today` / `tomorrow`; `-d sunday` = the most recent
  Sunday, `-d sunday+` = the coming one (trailing `+`/`-` forces the
  direction, and applies to `apr29`-style dates too)
- `-t` accepts anything `-d` does — day words, weekdays, `2024-03-16` —
  so a caller holding "either a time or a date" needs only one flag
- `-t` also takes any date it accepts, then any time of day it accepts,
  joined by a space or `T` (`'2026-09-28 10:00'`, `'yesterday 10am'`,
  `2026-09-28T10`), and ISO with an offset (`…T10:00+02:00`).
  `datum help time` lists every form, and a test parses each example on it
- a duration is a trailing positional (`datum end work 30m`) or `dur=30m`
  (`1:30`, `2h`, ISO `PT30M` all work). It says how long the thing LASTED,
  so the block ENDS at the entry's time and the span precedes it —
  `datum start posture 4` means posture was happening for the past four
  minutes, and `-t 1400` with `30m` is 13:30–14:00 (the original
  datum's `durationBlocks` view). There is no `--dur` flag. It was
  removed because the three spellings disagreed about which side of
  `-t` the block fell on; `crates/datum-cli/tests/dur_flag_removed.rs`
  keeps it gone, and the two survivors now agree with the reference
- `start_time=` / `end_time=` on `add`/`note`/`start`/`switch`/`occur` are
  lifted onto `occur_time`/`dur`, parsed exactly as `-t` is: the
  pair is the block between them, `start_time=S` + a duration ends at
  `S + dur`, `end_time=E` alone on `start` becomes `end -t E` (on `switch
  STATE`, `end -t E -L=STATE`). Never combined with `-t`; nothing of the
  keys stays in `data`

### Syntax variants (both must work)
- `datum switch mood happy`  ← verb-first
- `datum mood switch happy`  ← field-first (swap in pre-pass before clap)

### Alias system (built-in, not shell aliases)
Stored in the DB. Resolved in the CLI pre-pass before clap sees argv.
- `datum alias set vbp "note word srcLang=pt pos=verb"`
- Then `datum vbp correr "to run"` expands to `datum note word srcLang=pt pos=verb correr "to run"`

## Sync architecture

The relay sync is deleted; p2p «Easy Sync» replaced it. Its core is in
`crates/datum-core/src/db/sync.rs`: every
entry change emits a signed op beside its `history::record` (endpoint key in
`sync_local`, HLC, the change's own `raw` line, its key-level `effect`), and
an entry's row is the FOLD of its ops in HLC order, so the same ops in any
order give the same row. Its conflicts are part of the fold (da-05rkv): the
`conflicts` column names each key two ops set without seeing each other's
(and a delete against an edit made without seeing it), derived from the ops
alone, so every copy holding them agrees; an edit that saw every side —
`datum conflicts resolve` — settles it on each copy it reaches. A key's sides
are lettered A, B, … in the column's order, the fold's, so the letters are
every copy's (da-07594): `resolve` takes `KEY=a|b|<value>` picks, and for the
keys not picked one of `--use-a | --use-b | --prefer-a | --prefer-b | --merge`
(`datum_parse::combine` over A and B, `deleted` a key like any other); with no
strategy named they keep what the entry shows, so a bare `resolve` settles and
changes no data. An entry's
history is read from the same ops (`db/sync/versions.rs`, da-s4erq): a version
per op in fold order, a `fork` where a key became contested and a `join` where
an edit settled it, so `datum history`, `why --full` and `undo --to-version`
number the same versions on every copy; `entry_history` is this copy's undo
journal alone. A copy's HLC passes only the ops it has applied, and a received op
more than five minutes ahead of its clock (`MAX_CLOCK_AHEAD`) is held until
the clock catches up, its device named by `sync status`. The hidden `datum sync pipe vector / ops / apply /
verify` move ops as JSON lines over a pipe. The transport is iroh (`crates/datum-sync`,
behind datum-commands' `net` feature, so the CLI's wasm build has no iroh): the iroh
endpoint key IS the db's endpoint key, copies join a sync chain with
one-time invites (`sync chain invite` / `connect`; the invite is a datum.day
join link, `/sync/join#<ticket>`, which `connect`, a browser and the page's
prompt bar all redeem, so a browser can join a chain as well as start one),
only members are served
(`sync_peers` is just where they were last seen), and bare `sync` / the
hidden `sync run` swap vectors and ops until neither moves. The tree:
`sync`, `status`, `pause | resume`, `chain invite | connect | leave |
list | rename | revoke | purge | ack | rekey`, `admin list | add | remove`, `daemon
install | uninstall | status` (a systemd user unit / launchd agent
running the hidden `sync daemon run`, which is `sync run` for every chained
db; a copy's first `chain invite` / `connect` installs it unless
`DATUM_SYNC_NO_DAEMON=1`, and `scripts/install.sh` restarts it).
`DATUM_SYNC_BIND=<ip:port>` binds one address with no relay or discovery (the
tests), `DATUM_SYNC_RELAY=<url>` one relay and nothing else. A copy of the file at another path mints its own key (`sync_local.path`,
v44); two copies found signing with one key are refused by name, and
`sync chain rekey` gives one a new key. Membership is the signed chain
(`db/sync/chain.rs`), and there is one mechanism: a
chain with no admin is OPEN — any member invites, revokes or renames, and
`invite` on a copy in no chain starts one — and the first `admin add`
LOCKS it, after which those acts take an admin key (passkey or key file).
Removing the last admin takes `--allow-unlock` and reopens it; every copy
keeps a notice of the unlock until `sync chain ack unlock`.
`sync chain purge <WHO>` (da-ry11c) previews, and with `--yes` writes a
`purge` record: every copy refuses every op that endpoint signed and folds
its entries again without them — what it made goes, its edits and deletes
are undone — and revokes it. There is no unpurge: a fresh invite brings it
back (below), though an old chain's `unpurge` record still folds as it did.
A copy belongs to one chain — its apply refuses another chain's records
(da-d8ort). `chain leave` leaves it from any chain: its ops, peers and
notices go, its entries stay — signed again as baselines on the next sync —
and it takes a new endpoint key, being a new device now. Redeeming another
chain's invite leaves on its own only a copy holding nothing but its own data
(no other live member, no other endpoint's op), keeping its key. A revoked
device rejoins by a fresh invite to its chain: the `admit` of a listed
endpoint RE-ADMITS it, clearing the revoke, so what it made while out folds
in (da-j8c4l) — and a purge too, so a purged device rejoins as at a first
connection, every op it made folding back in (da-q4rau).
A first sync carries the inviter's chain records after its history, so a join
cut short (a reload, a lost connection) leaves the joiner in no chain holding
part of it, or none (`sync_peers.redeemed_at`, da-38eye): `sync chain list` /
`status` then name the copy it is JOINING
through, which counts as its peer, and bare `sync` or datum.day's leader
resumes there (da-idw54).
A reader may be FILTERED: its invite's `caps` or a `role` record
carries `fields` (`--fields a,b`, or `--kit K` resolved to that kit's fields)
and `of`. Every member serving it sends only the entries that belong to those
fields — an op's `field`, a spec's being the field it specifies — one line per
entry, an entry that moved in whole, `{"remove": hid}` for one that moved out,
and a `caught_up` mark that answers the `since` it was given; the reader
applies with no gap wait, drops what falls outside, and its vector is its
marks (`sync_caught_up`, v50). `verify` hashes the filtered set on both sides.
`sync pause` is this copy's alone (`sync_local.paused_at`): its
handshake and pipe refuse, `sync run` and the page's leader push nothing, and
datum.day's root bar says «sync paused» in every tab until a tap resumes it.
A write on a copy with another member that has not synced for a day warns
once a day on stderr (`sync_local.warned_at`).
Each copy says what it runs in the handshake — `datum` (the build) and
`schema` on `Hello` and `Welcome`, defaulted so a copy one version back
still reads them — and the other records it (`sync_peers.datum` / `schema`,
in `sync chain list`). A copy behind a member's schema syncs on and warns to
upgrade, once per new version on stderr and for as long as it lasts in
`sync status` and `datum check` (da-3zqyv).

The session itself is `crates/datum-sync-wire`, generic over a `Store`,
and the handshake's storage side — members, invites, admission — is
datum-core's, so datum.day syncs too: `web/src/lib/sync.worker.ts` runs the
wire crate built for wasm32-unknown-unknown (relay-only iroh) with the CLI as
its store (hidden `sync handshake` / `new-invite` / `endpoint-key` /
`record-session`), and `$lib/sync.ts` elects ONE tab per database to run the
endpoint by a Web Lock (`datum-sync:<db>`), which pushes on every tab's writes
(`onAnyChange`). `web/tests/sync*.test.mjs` run on a pinned local `iroh-relay`.

## Web app (PWA)

- SvelteKit static adapter — no server-side rendering
- The real `datum` CLI is the only runner: `datum-cli` built for
  wasm32-wasip1, one WASI process per command in `web/src/lib/db.worker.ts`
  (`cli.ts`, `wasiFs.ts`), its SQLite file in OPFS. There is no SQL, parser
  or renderer in TypeScript — every pane, the widget bar (`fields --json`),
  the tail table (`tail --json`'s `columns` and, for its state cell alone,
  that row's three state values), /now (`now --bare`) and the
  gear's «Backups» (`db backup -`, and plans' `db backup --to` into a
  process's `/data/stage`, whose files a `run` hands in and gets back)
  are CLI commands through `run(argv)`. What TypeScript does
  hold is the transport: carrying those files to an S3-compatible bucket
  (`$lib/s3.ts`, SigV4 by aws4fetch, the browser straight to the bucket
  under its CORS), and merging a run's one slot into the index as the
  bucket has it now, so two devices keep each other's.
  A command a reader runs also carries `--show json` (`echoArgv`
  in `web/src/lib/echo.ts`), so the bottom bar and the REPL PLACE the
  CLI's own action echo — action, hid, id, info, data — instead of
  parsing its printed lines; a command with no echo falls back to them.
- The bottom bar is the page's command line AND its status line
  and the SEAM between the dashboard and the history: an INPUT
  section over an OUTPUT section, `--input-height` (2.9rem) and
  `--output-height` (3.6rem) on the shell. The input is one row — the mark,
  the clock with seconds at every width, `❯`, the box with its time-cursor
  pills, which stand to the RIGHT of the box and never over the `❯`, going
  off the screen's right edge and coming back on the box's own sideways
  scroll (text and pills are ONE strip in one scrollport, `.field` and
  `.run` in Prompt.svelte); the output is the last command's result as one
  section per document (`+N` opens a modal when they do not fit). The row's
  items are centred in the whole input section, and folded a command
  answers with its ACTION words along the bottom of it STARTING AT THE `❯`,
  in the colour the CLI paints the tag (`Outcome::hex`, carried as
  `action_color` on `--show json`), fading, or a red `ERROR` that opens the
  output on a tap. UNDER the bar, below the fold, is the newest-first REPL
  (`$lib/BarLog.svelte`).
- THE SCROLLBACK STANDS IN COLUMNS (`$lib/scrollback.ts`) — the aim:
  "the output of a single entry should be narrow enough to comfortably fit
  2 on mobile side by side". As many entries abreast as fit at the width an
  entry's lines WANT (270px, measured), and never fewer than two wherever
  two clear what an entry NEEDS (160px, measured against the width it
  starts being cut at): a 390px phone gets two, a 1280px screen four, a box
  narrower than two columns the single column it always had. The REPL cuts
  nothing to pay for it — it has the vertical room the bar's fixed band has
  not, so a line too long for its column wraps and the brace drops under it
  (`.doc.all` in Doc.svelte); a command whose answer is the CLI's own
  printed text — a table, a drawn day, an error — takes a whole row at the
  width it was printed for. The hairline between commands is the top edge
  of every cell on a row past the first, so two entries side by side share
  one rule and never stand either side of it. `tests/scrollback.test.mjs`
  takes both measurements again against the built app.
- THE BAR MOVES BY GESTURE ALONE — a drag anywhere on it, or a wheel over
  it. The `^` on its top rule is a HINT and not a control (`pointer-events:
  none`, `aria-hidden`): as the author put it, "The ^ button obscures the text… just put a
  little carrat above the input bar to suggest that it can be pulled up."
  HOW FAR OPEN it is, is ONE number of pixels (`reveal` in `$lib/log.ts`)
  that three boxes are sized from — the page itself never scrolls. The
  output opens over the first `--output-height` of it, which is the NOTCH;
  past it the log grows to whatever height the finger leaves it at, up to
  the whole page. A release CARRIES ITS SPEED: past 0.5 px/ms it is a
  FLING, which goes all the way up, or down as far as the notch and no
  further — the snag, so a fling closes the log and leaves the output open,
  and a second one shuts it. Slower than that the let-go rules apply, so a
  deliberate drag ending below the notch closes. Every settle is one
  ease-out cubic whose duration is set to leave at the finger's own speed.
  A phone starts folded, anything wider open, remembered per device.
  `return to top` sits pinned at the bottom of the screen and Escape does
  the same. NB `touch-action: pan-x` is on `.field` as well as the bar: the
  value resolves only up to the nearest scroll container, so without it the
  browser claims every drag that starts on the box.
- `?db=<name>` names the DATABASE the page runs against (`web/src/lib/database.ts`): the name becomes the CLI's own `--db` at the
  one seam (`run` in `run.ts`), so no caller carries it, and a caller that
  names its own database keeps it — which is what leaves the layout store's
  `--db _internal` alone, layouts being the device's and not the database's.
  Nothing is prefixed at the default, the query is the whole of the state
  (it survives a reload, links and `goto` carry it, nothing is stored), the
  tab is named after it, and `_internal` or a path is refused with a notice.
  The worker persists whatever database a command NAMES — it reads the name
  out of argv as the CLI does and opens a handle for `<name>.db` — and the
  cross-tab refresh is keyed by it, on the names `announceChange` already
  carries.
- A burst takes the database files for itself (sync access handles) under
  the `datum-cli-files` Web Lock, so tabs take turns fairly and none is the
  owner. Back-to-back commands share one acquisition: the handles
  and the lock are kept while jobs wait in the worker's queue, and given
  back when it drains or once a burst has held them 250 ms, so a tab beside
  it is never made to wait for the whole burst. What a process reads of a
  `.db` is kept for the next (`cached` in `wasiFs.ts`), across a release
  too while the file's size and change counter are unchanged, and the CLI
  holds SQLite's lock for a connection's life on wasm — a private window
  pays ~1 ms an OPFS call, and a command made hundreds (da-o3sns). The SQLite header's change
  counter (offset 24) decides whether a process wrote, which is what
  `web/src/lib/changes.ts` tells the other tabs. A browser that refuses the
  handles gets a database in memory for the tab, which keeps going back for
  the file and moves its rows across with `db backup -` / `db restore -
  --merge` (a bare restore REPLACES, tombstoning what the backup lacks).
  SQLite runs in rollback-journal mode under the shim (WASI has no WAL); a
  journal left by a killed process is rolled back by the next one.
- Service Worker caches app shell for offline use — the shell, everything
  `$service-worker` lists, and Vite's separate worker bundle (the database
  worker and datum-cli.wasm), which that list does not know about and which
  the `datum-worker-precache` plugin in `web/vite.config.ts` enumerates
  instead. `web/tests/offline.test.mjs` reloads the app with the network
  down and logs an entry, so the precache cannot silently lose a file
  again.
- The page never waits on the CLI to draw (da-bhudz): the first command
  after a deploy can be a schema upgrade, a minute on a big history. The
  shell draws the tree it last drew on this database (`loadShell`,
  `draft.ts`) before asking anything, each widget's last output is laid over
  it, dimmed, until its answer lands — the widget mounts once the bar can
  take a command, so the bar's reads queue first (`layout/Stale.svelte`, in a closed
  shadow root so no locator finds it; `ready()` in the tests waits the
  copies out), and `Waiting.svelte` says «Opening your data…» or
  «Upgrading your data…» once the first answer is a second late — the
  upgrade heard by the worker as the pre-migration backup is created,
  which the worker keeps in OPFS beside the database (da-hse93).
  It stays until the bar is ready, and at any time says «Another
  datum.day tab or window is using your data» while another tab has held
  the files' lock past a second (da-89kom): the one wait with no deadline,
  which a tab the phone froze mid-burst could make forever (suspected,
  not reproduced).
- Background Sync API queues sync when offline
- The page is a LAYOUT TREE, not a grid (`web/src/lib/layout/`):
  panes hold widgets or splits of panes, each pane has tabs and draggable
  borders, and its bar reads maximize · title-or-tabs · `+` · `▾` · `×`:
  the NAME opens what the pane IS and what it is
  saved as (Reset widget, the widget's OWN OPTIONS one item each, the
  layout's acts), the `▾` beside the `+` what is DONE to it (Change
  Widget, Rename tab, Split, Break out, Bar, Close), the `×` after them
  closes what the title names — the menu's own Close, promoted to the row
  — and a lock hides all three. One builder, `menu.ts`, one order, two
  filters — `named` and `managed`. A HOLD on the title — the app's one
  hold, `web/src/lib/hold.ts`: a 450ms long press, a right click, the
  menu key, shift+click — is a SECOND door on Change Widget,
  which keeps its home in the `▾`; the `contextmenu` is eaten on the
  title alone, so a reader still gets the browser's menu over a pane's
  content, where text is selected and copied.
  The lock that hides the three is the one from ABOVE: a pane
  HOLDING a sublayout belongs to the layout above it, so its `+`, its `▾`
  and its `×` follow the parent's lock and not the sublayout's own, which
  reaches only the subtree. The root's `+` is never hidden by any lock — a locked
  layout must not be able to stop you opening another beside it.
  A WIDGET OPTION IS SESSION STATE (`session.ts` + `options.ts`):
  the kind declares it in the registry, the menu draws it inline — a
  switch is a `menuitemcheckbox` that turns over without closing the menu,
  a choice a submenu of words — and setting it changes what THIS TAB shows
  and never the layout. So does a widget moving a setting of its own
  (nowview's − / +, the strip's handle): `Layout.svelte`'s `on:change`
  goes to the session too. The one writer to the tree from the widget's
  side is «Save options with layout», which only an UNLOCKED menu carries
  and which only appears when something is held — so «when locked, nothing
  modifies the layout» is true by construction rather than by a guard. The
  lock it reads is the SUBTREE's (`lockedWithin`), not the
  frame's: on a tab holding a locked one-widget layout the tree it would
  write is that locked layout's own. The
  overlay is the DIFFERENCE from the tree, keyed by layout + tab + widget,
  pruned when a tab closes or its widget is changed, and gone on a reload;
  `Reset widget` drops it along with writing the kind's defaults.
  The contract is `types.ts`; the pure operations and the canon (no
  single-child split, no empty pane, ratios sum to 1) are `tree.ts` (`tests/tree.test.mjs`, node); the
  widget kinds and their components,
  floors and settings are `registry.ts`; today's page as data is
  `defaults.ts` (a desktop and a phone tree), which a first visit opens as
  the third of three root tabs (da-u7vz8, `fresh` in `shell.ts`):
  `welcome`, a widget saying what datum is and how tabs and widgets are
  handled, `track`, the field widget, and `collect`, that page; the one live store is
  `current.ts` (`layout` + `apply`), which the header's save/restore
  controls fill from the draft or the saved layout. `engine.ts`
  owns the epoch and routes a widget-bar press through whichever REPL is
  mounted. `Pane.svelte` inside a frame renders body-only and hands its
  title slot and alarm up to `PaneFrame.svelte`. Tests find a pane by its
  `data-title`; the old grid's area names live on as classes of the cells.
  A saved layout lives in one of TWO stores (`store.ts`): the
  person's own in `_internal`, and whatever a `[[layout]]` block in a kit
  installed into the DATA db — an ordinary `layout` entry carrying its
  `kit_id`, so it syncs to every device of that database. A listing says
  which (`builtin` | `user` | `kit:<name>`); a save shadows a kit's or a
  built-in's name, `Reset layout` goes back to the kit's tree, and
  `designedFor` decides which of two same-named layouts wins on THIS
  device and orders the chooser — it never hides one.
  A NEW DEFAULT IS OFFERED, ONCE (da-cr0w2): a change to what a first
  visit opens is a sentence appended to `NEW_DEFAULTS` (`defaults.ts`), and
  a device that started before it is asked at boot (`offerDefaults`,
  `shell.ts`) — the sentence, then «Use the new layout», which is «Reset
  default layouts», or «Keep mine». Either is remembered per device; a
  first visit starts on the newest and is never asked.
- The tail pane is `tail -w` (`web/src/lib/TailTable.svelte`):
  its box decides the row count (`tail -n <n>`, n from a hidden probe
  row's height, never from the drawn table, and never under `tail`'s own
  10: a shorter pane scrolls, pinned at the newest, and agrees
  with the REPL's `tail`; nothing is asked until the box is taller than
  the header), scrolling to the top pages
  the rows above in (`tail -n <n+1> -t <oldest row's own time>`, the
  CLI's inclusive bound, deduped by hid, capped at 300 rows) with the
  reader's row anchored, and a write while scrolled up keeps the place
  and shows `↓ new`. `web/tests/tailwatch.test.mjs` holds it to that.
  Its STATE cell is the one cell not taken whole from `columns` (`web/src/lib/StateCell.svelte`), and the reason is NOT that the page
  renders it differently — it renders it the SAME. A state transition is
  THREE values in the row — `previous_state`, `state`, `following_state` —
  which on a terminal are the `[LEAD][body][TRAIL]` trio, and the row now
  carries all three PAINTED: `previous_state_cell`, `state_body`,
  `following_state_cell`, which concatenate to the `columns` cell byte for
  byte. The page draws those three in order and adds only what a terminal
  cannot — the state's name on HOVER, and a box it can select. The author's rule:
  "The previous and following states should just have the colors, not the
  full text — just like the CLI. Really mirror the CLI here. The
  rerendering in the web is just for some niceities and being able to
  select." An earlier take drew the edges as WORDS and cost the column its
  alignment (a transitioning row's chip sat 70px right of one that did
  not); a colour block is ONE CHARACTER WIDE whatever it stands for, so the
  raggedness has nowhere to come from. ⛔ `white-space: pre` on the edge is
  load-bearing: the CLI's slot is a painted SPACE, and a lone space in a
  `<td>` collapses to zero width with its background set and nothing drawn.
  `web/tests/statecell.test.mjs`.
- The dayview strip is a stack of days, not one band: today is
  pinned at the bottom and the days before it are stacked above, as many
  as the strip's box SHOWS (the height comes from the container,
  the day height is a `dayHeight` prop with a `change` event, and the
  strip keeps nothing in `localStorage`) — but the stack HOLDS a fortnight
  at least, however short the box, which is the past the reader scrolls
  back through (the two numbers were one, so the whole scroll
  range was the single partly-revealed day and the strip moved 38px and
  stopped), one `dayview` render per band through the seam (`bandArgv` in
  `web/src/lib/panes.ts`). Every band asks for the same `--width` and
  `--day-height`, which is what pins `label_width` and so makes the hour
  gridlines of one band land on those of the next; all but today's
  suppress the axis strip with `--time-axis-height 0`. When the band above
  today is not the day before it, a jagged `<button>` between them scrolls
  yesterday back down.

## Migration from original datum

The original datum uses CouchDB JSON documents. Backup format:
```json
{ "backupTime": "...", "docs": [...CouchDB docs...] }
```
Each doc has `data.field`, `data.occurTime`, `data.state`, `data.dur`, `meta.createTime`.
Migration: `datum db v2import --from backup.json`

## Running tests

```bash
cargo test              # all crates
cargo test -p datum-core
cargo test -p datum-cli
```

Add tests for new behaviour.

## Pre-submit

Before opening a PR, run the gate:

```bash
bash scripts/mq-check.sh
```

It runs only what the diff can break: for Rust,
`scripts/check-changed.sh` — `cargo fmt --check`, then clippy, build, tests
and doc tests scoped to the crates you touched plus their workspace
dependents, falling back to the full `scripts/check.sh` on a workspace-root
config change or when the closure is most of the workspace; for `web/`, the
web suite; for docs alone, nothing. Do not run `cargo test --workspace` or
`scripts/check.sh` for a localized change: CI runs the full workspace on
every PR, and that is the safety net for what a scoped run cannot reach.

## Building

```bash
cargo build --release -p datum-cli   # produces target/release/datum
```

## Web app dev

```bash
cd web && pnpm install && pnpm run dev   # localhost:5173
pnpm test                              # the seam suite (builds first)
pnpm run test:fixtures                 # recapture the CLI-parity fixtures
```

The browser runs `datum-cli` itself, compiled to wasm32-wasip1 by
`pnpm run build:cli-wasm` (pinned wasi-sdk and binaryen, fetched into
`target/toolchains/`) and loaded through `@bjorn3/browser_wasi_shim`. What
the page supplies is what a shell would: argv, stdin, `TZ`, a client id,
`FORCE_COLOR=1` (there is no tty), and `HOME`/`DATUM_DB` inside the
preopened `/data`. The module is NOT committed: a binary cannot
merge, and every CLI merge conflicted every other open CLI PR on it. Each
push to main builds one onto the `wasm` branch (`.forgejo/workflows/wasm.yml`)
— `datum-cli.wasm` plus `SOURCE`, the main sha it is for, one commit per
main commit — and `web/scripts/ensure-wasm.mjs`, the `predev`,
`prebuild`, `precheck` and `precheck:watch` of `web/`, puts one in `web/src/lib/wasm/` like a build product
in `target/`: it keeps the one there if its crates are this tree's, fetches
the `wasm` branch's if this tree's crates are the merge-base's with
`origin/main`, and builds otherwise, printing which and why. Deploy runs on
the push to main and does the same, so a web-only merge fetches and
installs no Rust, and nothing waits for a Rust build. It lives under `src/`
rather than `static/` so the app reaches it as a `?url` import and Vite
emits it content-hashed under `/_app/immutable/`: a runtime artifact must
not have a stable name, because the service worker serves precached names
cache-first and `deploy.sh` marks hashed assets `immutable` for a year, and
together those handed a returning browser the previous deploy's module to
run the current deploy's JavaScript against. `deploy.sh
--check-dist <dir>` is that rule on its own.

Reusing a module built elsewhere is honest because the build is
byte-reproducible: it remaps the absolute paths rustc and clang bake into
the bytes. The fixtures are still committed, and `build-and-test` re-runs
`pnpm run test:fixtures` and fails on any byte of difference.

A branch cut before the module stopped being committed still carries `datum-cli.wasm`, and rebasing it
conflicts once (modify/delete). The fix is `git rm
web/src/lib/wasm/datum-cli.wasm` — no rebuild; nothing commits the module
any more.

`web/tests` drives the built app in a real browser and asserts through
`run(argv)` — the one seam every region of the app goes through.
`tests/parity.test.mjs` compares what a given argv stores in the browser
against what the native `datum` binary stored for the same argv, and
`tests/stdout.test.mjs` what a script of commands PRINTS, byte for byte
after masking hids and write times. Both are hard assertions: the browser
runs the CLI, so there is nothing for an exemption to excuse. Needs a headless browser once: `npx
playwright install --only-shell chromium`.
