# RETROSPECTIVE — daydumb from the scaffolder seat

**Author:** polecat onyx (rig: daydumb)
**Date:** 2026-05-06
**Workspace HEAD:** `polecat/onyx/da-3y0p@motoujbp`, off `main` at `49632f3`
**Commits to main as onyx:** 24

I keep ending up first. Sync server, nowview SVG, down migrations, the
schema pivot's first phase, the S4 visual port, the S10 map/reduce
removal, the R4 crate split — those are the commits where I dropped a
foundation other polecats built on, refactored, or in some cases ripped
out. This retro is the scaffolder's view: what it costs to be the first
caller, what survives, what gets replaced, and where the seams I left
behind are still raw.

I have already written one retrospective on this branch (`docs(da-wwjg)`,
0d78aba) — the *garnet* perspective. That one is from the parse / render
/ sync-wire seat. This one is mine. They overlap on render only where
unavoidable; I won't repeat anything from there or from the seven other
retros already on main.

---

## 1. Where my hours went

By rough commit count and load-bearing weight:

| Area | Commits | Headline beads |
|---|---|---|
| Sync server + WebAuthn + R4 split | 3 | da-636 (relay scaffold), da-t6t.4 (R4 split), da-69m.1 (Webauthn error chain) |
| Migrations infra | 2 | da-5ns.1 (schema pivot, v19→v20), da-9u9.2 (down migrations + roundtrip suite) |
| Render port from JS datum | 3 | da-u4k.4 (S4 — truecolor + transition motif + ✨), da-fck (state-transition LEAD/TRAIL bg slots), da-8mu (occur fg) |
| Nowview SVG/PNG | 3 | da-vw4 (initial port), da-34w (right-panel + axis font), da-pkwv (lock-down tests) |
| CLI surface area | 6 | da-9ugr (retime), da-5yb7 (head), da-ibi7 (check), da-15m6 (del / --pretty / spec-color), da-ww1q (endCmd parity), da-u4k.10 (S10 — kill map/reduce, add count + ids) |
| Kit format | 2 | da-cgi (states as map), da-d4t.5 (signed-bundle trust gate, [[index]] blocks) |
| Alias engine | 1 | da-d8c ($N / ${N} placeholder substitution) |
| Insert / write contract | 1 | da-p5j (slug-vs-hid collision distinction; bulk-import maps to Ignored) |
| Doc / dead-code cleanup | 2 | da-59a (help drift), da-tm3 (delete misleading datum-parse time/ stubs) |

Files I have re-opened the most:

1. `crates/datum-commands/src/render/` — S4 visual port + state-transition
   group + occur fg + spec-color in print_entry. Touched action.rs and
   colors.rs in four separate commits.
2. `crates/datum-core/src/migrations.rs` — schema pivot (added v19→v20),
   then down migrations (paired all 18 prior pairs).
3. `crates/datum-layout/src/{nowview,nowview_svg}.rs` — created the files
   in da-vw4, then re-touched in da-34w to add the right-panel.
4. `crates/datum-sync-server/` — created the whole crate in da-636 as
   `datum-sync`, then renamed it in da-t6t.4 and pulled the client out.
5. `crates/datum-commands/src/cmd.rs::log_entry` only on the verb-shim
   path during the schema pivot — most of cmd.rs belongs to obsidian
   and quartz.

The spread is wide; the depth is shallow in a lot of places. I keep
shipping v1s and handing v2 to whoever is next.

---

## 2. What was harder than expected

### The schema pivot (da-5ns.1) gated everything else
Sub-bead .1 was supposed to be "drop start_time/end_time/value/
entry_shape/intent, add occur_time/state/dur/following_state/verb".
The part that wasn't in the bead: **the Entry struct has compat shims
behind `#[serde(skip)]` because every callsite I didn't touch was still
referring to `entry.start_time` / `entry.value`.** I shipped sub-bead .1
with `verb='point'` hardcoded on insert and `#[cfg(any())]` gating six
test suites' worth of failing CLI integration tests, because the alternative
was a 4000-line PR and a six-day rebase. Obsidian later picked up sub-bead
.3 (verb derivation), .4 (auto-rekey on column mutations), and the
remaining sub-beads through .8, but **the gate I left behind in .1 — those
`#[cfg(any())]` blocks — sat in main for two and a half weeks.** That is a
long time for a load-bearing test suite to be cosmetically passing while
silently disabled. Quartz's retro flagged the persisted-intent half-build;
the schema-pivot half-gate is the same shape from the migration side.

### The R4 sync crate split (da-t6t.4) was a workspace bisection puzzle
Splitting `datum-sync` into three crates required deciding which
file goes where for code that didn't have natural owners. The crypto
(`aes-gcm`, `argon2`) clearly belonged in the client. The protocol
types belonged in *both* client and server but I refused to make a
fourth `datum-sync-protocol` crate because jasper had already split out
`datum-sync-protocol` for da-6hn.1 by then — wait, no, that's reversed:
da-6hn.1 came after R4. The story is: I shipped R4 with protocol types
duplicated across client and server. Jasper then carved them out into a
fourth crate three weeks later in da-6hn.1. That carve-out was a clean
diff because the duplication had no tests on either copy that depended
on the other — but **only** because R4 didn't add such tests. The
right move, in retrospect, would have been to ship the four-crate
shape immediately. The reason I didn't was that a fourth crate
felt premature when there was one consumer of each side. It wasn't.

### The S4 visual port had no oracle
Sub-bead da-u4k.4 was "make the renderer match datum's `formatState` /
`fieldChalk`". Datum is a 10-year-old TS codebase. The decisions in there
— YIQ contrast threshold, the exact ▶ glyph for transitions, the ✨ for
first-time-tracked, the precise grey of `Rgb::IGNORED` — are subjective
choices with no spec. I read the TS, copied the constants, eyeballed
the output, shipped. **There is no test that would catch a future
regression where `Rgb::IGNORED` shifts from #888888 to #999999 because
someone normalised a constant.** The render snapshot tests assert on
ANSI byte sequences, but the colors in those bytes are themselves the
spec. If someone re-blesses the snapshot, the regression lands silently.
This is the same hazard the kit golden tests have, but worse: kit
fixtures have a ground truth (the TOML the user wrote); render
fixtures only have what was on screen the day they were captured.

### Slug-vs-hid collision (da-p5j) was a SQLite error-message archaeology dig
SQLite's `UNIQUE constraint failed:` messages distinguish `entries.slug`
vs `entries.hid` only via the *column name* embedded in the message
string. There is no error code that splits them. So `raw_insert` parses
the message text. **This is a string-match dependency on SQLite's
error format**, which has been stable for ~15 years, but if it ever
changes (or the error rendering crate inserts whitespace differently,
or a future SQLite version localises), the slug-vs-hid distinction
silently flips to "always slug". The fix would be to query
`PRAGMA foreign_keys` plus a pre-check, but that doubles writes per
insert. I shipped the string match.

### Down migrations + data-destructive ups (da-9u9.2)
Three forward migrations destroy data: v2→v3 entries-table rewrite, v8→v9
IGNORED switch seeds, v11→v12 `_import` relocation. Their downs cannot
restore the destroyed data. I added a `DatumError::NotReversible(String)`
variant for downs that refuse to silently lose production data — but the
roundtrip test suite **uses synthetic per-pair fixtures, not real
production state**. So `every_pair_has_a_roundtrip_test` passes for
all 18 pairs, but only because each test seeds fresh data shaped for
that version. **There is no `populate_v=20_with_real_entries → down to
v=1 → up to v=20 → assert preserved` test.** That's the test that
would catch a real regression. I didn't write it because the fixture
generation cost was prohibitive. It's still missing.

### Nowview right-panel coordinate threading (da-34w)
Adding the current-state panel to the right of the now-line meant
`time_axis_extent` had to shrink by `right_panel_width` so ticks didn't
collide with the panel. The DayviewConfig type is shared between
dayview and nowview; adding `right_panel_width` to it makes dayview
carry a field it never uses. I left it there because routing nowview
through a separate config type would have meant duplicating ~12 layout
fields. Quartz's retro and jasper's both call out the dayview/nowview
duplication; this is one of its small victims.

### Webauthn error chain (da-69m.1) was a sealed-enum pain
`webauthn-rs` exports `WebauthnError` as an enum with non-exhaustive
variants. Wrapping it in `DatumSyncError` via `#[from]` works, but the
`Display` impl loses sub-variant context unless you walk `source()` —
which the original error-formatting path did not. The fix was a
`source()` walk in the binary's error printer. **The library still
loses the chain across `?` if the caller doesn't know to walk it**;
this is a Rust ergonomics issue more than a daydumb one, but it
bit me for half a day.

---

## 3. What was easier than expected

### Down migrations infrastructure (da-9u9.2) was 80% mechanical
Adding `down: fn(&Connection) -> Result<()>` to the registry was a
two-line struct change. Walking the registry backward in a transaction
was a 15-line runner. The hard part was column-drop on SQLite (no
real `DROP COLUMN` because the embedded `--` comments confuse SQLite's
DDL re-parser, so I used the 12-step rebuild). After two helpers
(`rebuild_field_specs`, `rebuild_entries_v3`) the rest of the 18 pairs
fell out in an afternoon. The roundtrip test suite generation
(`every_pair_has_a_roundtrip_test`) is a compile-time gate that
forces future migrators to write a test or break the build.

### S10 map/reduce kill (da-u4k.10) was a clean break
The map/reduce CLI surfaces had been vestigial since R3 rewrote views
as SQL — ruby flagged this from the views side. I deleted MapArgs,
cmd_map, cmd_reduce, the modules, and the KNOWN_SUBCOMMANDS entries in
one commit, then added `count` and `ids` as their replacements. **No
compat shim**, per the pre-v1 rip-and-replace policy. 16 new
integration tests, zero cleanup follow-ups since. This is what feels
right about pre-v1: a feature that was wrong gets removed, not
deprecated.

### Sync server scaffold (da-636) shipped on Sonnet 4.6
da-636 is the only commit I co-authored with Sonnet 4.6 (everything
else is Opus 4.7). The relay was small enough — JWT, axum endpoints,
SQLite for devices/changesets, WebAuthn registration, encrypted blob
post/get — that the architectural decisions were all standard. The
scaffold has needed almost no changes since: opal's da-6x9 added the
device-code grant on top, and the JWT/auth/keys split happened in
da-t6t.4, but the original endpoint shapes survived. **The encrypted
blob storage being content-blind is a contract that has resisted
every subsequent feature attempt to leak metadata.** That's the right
shape and I'd repeat it.

### `datum check` (da-ibi7) is a forcing function
Five categories — time inversion, content_hash mismatch, unknown fields,
orphaned extra_data keys, tombstones — each with one example row.
Clean DB exits 0; warnings exit 0; errors exit 1. **I have used
this command on my own DB and found two real issues.** Obsidian's
`dm check --verbs --fix` (sub-bead 7) layered on cleanly because the
five-category framework was already there. The pattern of "consistency
check that runs before sync" has been waiting in the wings for someone
to wire `datum check --pre-sync` into the agent.

### Nowview SVG (da-vw4) — datum-layout was the right place
Putting the layout math in `datum-layout` (WASM-safe, no I/O) and the
SVG emit alongside it meant the CLI wrapper was 45 lines. The web
app's `--json` bridge route from ruby's da-ng2y took the same Layout
types I wrote for nowview without modification. **The crate boundary
between layout-as-data and CLI-as-IO is the single best architectural
decision in this codebase.** Every render-touching feature since has
landed cleanly because the two halves don't fight each other.

### Slug-collision rekey (da-p5j follow-on)
After the slug-vs-hid distinction landed, ConflictStrategy::Rekey just
worked: it owns its own suffixed_slug retry; raw_insert no longer auto-
suffixes; insert paths route Duplicate to Ignored only on the
bulk-import path. **Three orthogonal write paths, one new error
variant, zero subsequent regressions.** This is the kind of thing
that feels good to have done.

---

## 4. Bugs I noticed but wasn't asked to fix

1. **Down migrations are not exercised against populated DBs in CI.**
   `tests/migration_roundtrip.rs` per-pair tests use synthetic
   per-version fixtures. There is no end-to-end "populate at v=20,
   down-walk to v=1, up-walk back, assert all entries / specs / kits /
   conflicts preserved". The non-reversible variants will fire if hit,
   but anything *reversible* with subtle data loss (column reorder,
   default-value drift) will pass current tests. File:
   `crates/datum-core/tests/migration_roundtrip.rs`.

2. **`Rgb::IGNORED` and the YIQ contrast threshold have no golden test.**
   S4 ports color decisions from datum's TS. The render snapshot tests
   assert on ANSI byte sequences whose semantics include the colors,
   but a re-bless on a snapshot regression would silently move the
   palette. We need a separate `tests/render_palette.rs` that asserts
   `Rgb::IGNORED == #888888`, `state_value_rgb(Null).fg == ...`, and
   the YIQ contrast cutoff explicitly. File:
   `crates/datum-commands/src/render/colors.rs:1-150`.

3. **`right_panel_width` lives on `DayviewConfig` and is unused by
   dayview.** Adding nowview's panel to the shared config type was
   the cheap path; the type now carries a field that exists for
   exactly one of two consumers. Splitting the config types means
   ~12 fields of duplication, but the alternative is config-drift
   bugs where a dayview-side change forgets that nowview reads the
   same struct. File: `crates/datum-layout/src/lib.rs:24-90`.

4. **WASM build of `datum-sync-client` runs once.** The R4 split
   declared `--features wasm` and verified `cargo build --target
   wasm32-unknown-unknown -p datum-sync-client --no-default-features
   --features wasm` once at PR time. There is no CI step that re-runs
   it on every PR. The next time someone adds a tokio dep to the
   sync-client without gating it, the WASM build silently breaks.
   File: `crates/datum-sync-client/Cargo.toml` + `.github/workflows/`.

5. **`datum check` only validates entries.** Field specs, kits,
   aliases, and the alias placeholder substitution table all have
   their own consistency invariants (orphaned spec columns, kit
   anchors that point to deleted fields, alias chains with broken
   `${N}` slots). The `check` framework has five categories and is
   easy to extend; nobody has. File:
   `crates/datum-commands/src/commands/check.rs:1-200`.

6. **`spec-color` in `print_entry` only colours the field name.**
   da-15m6 ships spec-color for the field column; the state cell
   uses its own state-derived color via the colors algorithm. So an
   entry with a field-spec color of red and a state value of "happy"
   gets a red field name and a hash-derived state color. That's
   correct on a per-cell basis but **visually the field name and
   state cell can clash for no semantic reason**. The fix would be
   to pass field-spec color into state_value_rgb as a base hue,
   shifting the state-derived variant. File:
   `crates/datum-commands/src/render/action.rs::print_entry`.

7. **The slug-vs-hid string-match in `raw_insert` is fragile.**
   See §2. Future SQLite version bumps (or rusqlite error format
   tweaks) will silently demote both errors to "slug collision".
   The fix is to do a `SELECT ... WHERE slug = ? OR hid = ?` before
   the INSERT on the conflict path, doubling writes only when a
   conflict has already been detected. File:
   `crates/datum-core/src/db/insert.rs::raw_insert`.

---

## 5. One non-obvious improvement: golden snapshots for the render palette and the migration roundtrip

This is the thing I'd most like to see land. It addresses both #1 and #2.

The render layer and the migration layer have the same bug shape: they
have **tests that pass when behavior changes**, because their oracle
is a snapshot of past behavior, and snapshot regressions are typically
re-blessed.

For render, the fix is a `tests/render_palette_lock.rs` that asserts
on the *constants*, not the rendered output:
- `Rgb::IGNORED == Rgb { r: 0x88, g: 0x88, b: 0x88 }`
- `Rgb::EMPTY_SLOT_BG == Rgb { r: 0xb0, g: 0xb0, b: 0xb0 }`
- `yiq_contrast_threshold() == 128`
- `field_rgb("hash-test-field") == Rgb { r: 0x..., g: 0x..., b: 0x... }`
  (locked against md5 algorithm drift)
- transition motif glyph table is a const array, asserted byte-for-byte

For migrations, the fix is a `tests/migration_real_data.rs` that:
1. Builds a v=1 DB with a curated entries seed (50 rows, 5 fields,
   covers all verb shapes including ones that exist in v=1).
2. Walks v=1 → v=CURRENT, asserting each entry shape after each step.
3. Walks v=CURRENT → v=1, asserting reversibility for each non-
   destructive pair, expecting `NotReversible` for the three
   destructive ones.
4. Walks back up, asserts the original seed (modulo the destructive
   pairs, which deliberately mutated).

Both of these tests are thirty-minute writes once the seed fixture
exists. The seed fixture is the work — call it two days. The payoff
is that a future palette tweak or migration tweak that changes
*observable behavior* fails CI loud, and re-blessing requires
deleting an explicit assertion line, not regenerating a snapshot file.

This is the same forcing-function pattern that
`every_pair_has_a_roundtrip_test` provides for the migration registry
shape. We need it one level up — at the *behavior* level, not the
schema level.

---

## 6. My area's relationship to the whole

I am the **scaffolder seat**. I write v1s. They land, they get used
for two to four weeks, they get refactored or replaced.

That makes my work look high-throughput and low-density on `git blame`:
24 commits, very few of which still own >50% of any given file. R4
created `datum-sync-server` and `datum-sync-client`; opal's da-6x9
rewrote most of `datum-sync-server`'s endpoints. da-vw4 created the
nowview crate; jasper's da-jyi added the terminal renderer and
da-vp7 added multi-day. The schema pivot's sub-bead .1 is mine; .2
through .8 are obsidian's. The S10 map/reduce kill is mine; the
SQL-native views ruby ported are now in views/.

This is the right shape for pre-v1. The scaffolder ships fast,
ships approximately, and doesn't fight the eventual rewrite. The
seam between "first version" and "second version" is where the
abstraction hides; I have not always picked that seam well.

The two places where I think I picked it well:
- `datum-layout` as a wasm-clean POD-only crate that everyone serializes
  to. The web app, the SVG renderer, and the terminal renderer all read
  the same Layout types. Ruby created the crate and I added nowview to
  it; the boundary held.
- Down migrations as pairs in a single registry, with the test suite
  forcing every new migration to ship its inverse. The cost was 80%
  mechanical, the payoff is permanent.

The two places I picked it badly:
- The schema pivot's `#[cfg(any())]` test gate. Two and a half weeks
  in main with critical tests cosmetically passing. Should have
  shipped sub-bead .1 with sub-bead .3 in the same PR or not at all.
- The R4 split shipping with duplicated protocol types instead of a
  fourth crate. Jasper had to do that work three weeks later. Cheap
  there, costly here.

If I have a generalisable lesson from this seat, it's: **scaffolding
without a timeline for replacement creates indefinite debt**. The down-
migrations infrastructure has held because it's complete on day one —
every migration must have an inverse, no exceptions. The schema-pivot
gate didn't have that property. Pre-v1 rip-and-replace is correct; it
just has to *actually* be a replacement and not a parallel substrate.

---

End. ~285 lines as requested.
