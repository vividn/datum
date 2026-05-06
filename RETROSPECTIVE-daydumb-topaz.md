# RETROSPECTIVE — daydumb from the small-surgical-fix seat

**Author:** polecat topaz (rig: daydumb)
**Date:** 2026-05-06
**Workspace HEAD:** `polecat/topaz/da-sugk@motpmn4o`, off `main` at `1890975`
**Commits to main as topaz:** 5 (`git log --author=topaz origin/main`)

| commit | bead | file delta |
|---|---|---|
| `7e1fac2` | da-0mht — `datum grep` regex search over entry JSON | +204 / 5 files |
| `8c6a346` | da-u4k.7 (S7) — JSONL backup metadata header + schema-version gate | +483 / 1 file |
| `244218e` | da-7r4 — render Switch entries as colored regions, not dots | +298 / 2 files |
| `6bcd11b` | da-d5w.1 — unify time fmt + delete 3 dupes in render/ | +46 / −130 |
| `00763e1` | da-afk — `test_cmd_track_and_ignore` freeze the test clock | +10 |

I am the small-commit polecat. None of my work is foundational — every commit is a one-bead-one-fix on a surface other polecats had already shaped. The total LOC is small; the scope is the **edges where a user sees something wrong and we can fix it without moving the building**. This retro is from inside that seat. The other eight retros covered the floors and the load-bearing walls. I'm writing about the trim.

---

## 1. Where I lived

Three files contain ~95% of my surviving work:

- `crates/datum-commands/src/cmd.rs` — added `cmd_grep` (1765-1802), `test_cmd_track_and_ignore` (3806+), and the time-fmt deletions that landed in render/ originated as edits adjacent to here.
- `crates/datum-commands/src/commands/backup_restore.rs` — S7 lives at `BackupHeader` (40-53), `read_header_line` (347-378), `migrate_entry_json` (386-399), `restore_from_reader` schema-version gate (412-450). The file was 516 LOC of cmd.rs when I shipped; ruby/onyx/quartz later moved it to its own file. The header design and the legacy-fallback path are mine; the file split is not.
- `crates/datum-commands/src/commands/dayview.rs` + `nowview.rs` — the Switch-as-region logic at `is_unrendered_switch_value` (489-494), `switch_region_end_ms` (506-518), and `is_switching_verb` (521-526). The verb set in `is_switching_verb` was extended later (`BlockSwitch | PointSwitch | Start | Stop` were folded in by da-5ns.3), but the *shape* — "build a region from this switch's occur_time to the next switching event" — is mine.

A fourth file held my time-fmt work pre-cleanup; today it lives in:

- `crates/datum-commands/src/render/action.rs` — `format_timestamp` (349) and `synthesize_local_datumtime` (378) are the two functions I unified to. The deletes (three duplicates of `format_duration` / `visible_width` / `strip_ansi` and two duplicates of `format_human_time`) all happened in `render/mod.rs`.

So my territory is **read-side commands** (grep), **backup format** (header + restore gate), **dayview/nowview state-region rendering**, and one **render-layer dedup pass**. No write path. No schema. No sync. No layout crate. No CLI scaffolding. Five small surgical commits in five different parts of the body, each one a user-visible behavior the codebase had wrong.

---

## 2. What was harder than expected

### Switch-as-dot was a six-week-old visual regression that nobody filed

Before da-7r4, `dayview` dispatched every `EntryShape::Point` entry to the `DayviewPoint` path — a 5px dot. `Verb::Switch` is a state-bearing point: it has no duration, but it carries forward state until the next switch. So a sequence of three `mood: happy / sad / happy` switches at 8am/12pm/4pm rendered as **three dots stacked at the top of the row**. The whole point of state visualization — a colored region per state — was gone.

The reason this took six weeks to file: Block entries with explicit `dur` rendered correctly through the Block-shape branch, and *all the demo data* used Block entries. Switches were a minority shape in test fixtures. The bug was "what if you log state without a duration, the way the original datum's `dm switch mood happy` always did" — and nobody had a backup full of those entries to restore.

Lesson: **shape-keyed dispatch is a footgun when a verb's semantics cut across shape.** `Verb::Switch + EntryShape::Point` and `Verb::Switch + EntryShape::Block` both want the same colored-region rendering, but the dispatch tree was making the shape decision first. The fix promotes the verb decision above the shape decision for switching verbs (dayview.rs:521-526). Worth grepping `EntryShape::Point` everywhere it dispatches and asking "should the verb override this?"

### `simplifyState === null|false` is a tiny rule with a pile of consequences

Original datum's `fieldSvgBlocks.ts` had one line — `if (simplifyState(state) === null || simplifyState(state) === false) skip` — and ours had to mirror it exactly. A switch *to* `null` / `false` / `IGNORED` ends a state but does not start a colored region. That's three different sentinel values (JSON null, JSON false, the string `"IGNORED"`) at three layers (Rust `Option<JsonValue>`, `JsonValue::Bool(false)`, the constant `IGNORED_STATE_VALUE`). I wrote `is_unrendered_switch_value` (489-494) and `is_ignored_or_false` (480-483) to centralize this; it is **the kind of one-paragraph parity rule that costs a day to get right and looks like five lines in the diff**.

### The S7 backup-header decision was a forking-protocol choice in disguise

The header format question — JSONL header line vs. binary envelope vs. file-extension scheme — looks like a serialization choice. It isn't. **It's a forward-compat protocol** because the header is what tells future-daydumb how to read past-daydumb's backups. I picked "first non-empty line, parse as JSON, sniff the `type` discriminator" (`read_header_line`, backup_restore.rs:347-378). Reasons:

- A daydumb v0 backup (no header) is just `Entry`-shaped JSON per line. Sniff `type === "datum.backup.v1"` — if absent, fall back to legacy (warn + assume schema v1).
- Single-byte version discriminators are brittle (cf. amber's §2 note on the key-wrap one-byte version). Using a discriminator field inside a JSON envelope lets us add `schema_version` / `archive_version` / `kdf_id` independently without colliding format generations.
- The metadata stays human-readable. `head -1 backup.jsonl | jq` works.

What's still fragile: the **schema-version gate** (412-446) refuses to restore a newer-than-binary backup. That's correct, but the error string ("Upgrade daydumb to restore this backup") doesn't tell the user *which version* they need. Writing the binary's version into the message would be a one-line fix I didn't make.

### Test-clock flakes are a class, not a bug

`test_cmd_track_and_ignore` (`cmd.rs:3805+`) hard-failed ~33% of the time on a fast machine. Three back-to-back `cmd_track`/`cmd_ignore`/`cmd_track` calls each stamped their switch entry with `DatumTime::now()`. Slug encoding is millisecond-resolution — `mood:2026-01-15T12:00:00.000Z`. Two writes in the same millisecond hit the slug uniqueness constraint and bailed.

The fix is `set_time` + `advance_time("1m")` between calls (datum-core's test_util). The fix is mechanical. **The lesson is that any test that writes more than one entry to the same field should freeze the clock**, full stop. I noticed during the fix that `delete_cmd.rs` (da-bex) had the same pattern and had already been fixed; my commit just propagates the protocol. There's a likely third site somewhere I didn't grep for.

### `format_duration` / `visible_width` / `strip_ansi` had three identical copies for the same reason

When `output.rs` (1008 LOC) was carved into `render/{mod,action,colors,characters}` (R1, dd5ea54), three private helpers got *copied* into both `mod.rs` and either `action.rs` or `colors.rs` because their callers were on both sides of the cut. The duplication wasn't sloppiness; it was the cheapest way to make R1's split compile. That's fine for the moment of the refactor and **expensive afterwards** — every subsequent edit had to choose a copy. Six weeks later, one was DIM-styled, one was GRAY_FG-styled, and `dm tail` rendered differently per code path.

The cleanup (da-d5w.1) was easy mechanically. Picking *which copy* was canonical was the work — it required reading every test that anchored on the divergent output to see which behavior the user-visible parity audits had endorsed. The TZ-offset compression (`+0200` → `+2`) was the parity-audited choice; mod.rs's copy still had the verbose form. **R1's split was right; the carve-out left orphans, and the orphans were drifting.** The general lesson is: any time you copy a helper across a split, file a follow-up bead the same day. da-d5w.1 should have been opened by R1's author.

---

## 3. What was easier than expected

### Regex over `serde_json::to_string(entry)` is the right grep

I expected to write a custom predicate language: field selectors, value-shape matchers, JSON-path queries. I wrote `regex::Regex::is_match` over `serde_json::to_string(entry)`, with AND semantics across multiple `--patterns` (cmd.rs:1782-1789). Total: 38 LOC.

Rationale: every field a user wants to filter on is **already in the JSON**. A regex like `"field":"coffee"` filters by field; `"value":"happy"` filters by value; `"strength":[0-9]+\\.[0-9]g"` filters by unit-annotated extra_data. The `--field` pre-filter (1772-1778) is just an index hint — it doesn't have to be a separate language. The `print_entries` reuse means grep output is identical to `tail` output, which is what users actually want for "show me what matched."

What I didn't build and don't regret: structured query (`--where field=coffee --where value=happy`). The day someone needs that, they'll be writing a real query language, not extending grep.

### Generated-column sentinels carried Switch correctness across the schema pivot

When da-5ns.1 (onyx, two weeks after my Switch fix) repivoted the schema to `verb` as a generated column, `is_switching_verb` (`Verb::Switch | BlockSwitch | PointSwitch | Start | Stop`) extended naturally. The `BlockSwitch` and `PointSwitch` variants didn't exist when I shipped — they were synthesized by the new `derive_verb` SQL — but the *enum match* I wrote in dayview.rs accepted them the moment they existed in the type. I expected the schema pivot to break my Switch logic and it didn't. **Generated-column verbs gave the renderer correctness for free**, exactly the kind of leverage amber called out in their §3.

### `print_entries` / `print_table_with` is the right shared sink

`cmd_grep`'s last 5 lines (1791-1801) are `load_registry` + `TableArgs` + `build_render_contexts` + `print_table_with`. Every read-side command (`grep`, `tail`, `get`, `head`) ends with the same five lines. I expected to spend a day deciding which renderer to call; I spent ten minutes copying the call site from `cmd_tail`. **The duplication is a feature** — every read-side command shipping a different renderer would be the bigger problem.

### `tar::Archive` + `flate2` is a good Rust experience

`backup_targz` (135-207) is 73 lines for: stage entries.jsonl to tempfile (with a `Drop` guard at 128-133), build a tar with manifest.json first then entries.jsonl, gzip the result. No buffering the dataset in memory. The temp-file pattern is the only non-trivial bit. **The crate ecosystem held up** — I expected to fight `tar::Header::new_gnu` over mode/mtime/size and didn't.

---

## 4. Bugs I noticed but didn't file

In file:line detail.

1. **`commands/backup_restore.rs:422-427` — schema-version overshoot error doesn't surface the *binary's* version.** Says "Upgrade daydumb to restore this backup", omits "your binary supports v{N}, the backup is v{M}." Format the message with both numbers.

2. **`commands/backup_restore.rs:347-378` — `read_header_line` discards the original line on header parse failure.** If `serde_json::from_str::<BackupHeader>(&trimmed)` errors at line 372-373, the trimmed string is consumed by the `with_context`, then the function bails. The first-line content is lost from logs. For diagnosing "broken header" cases (truncated file, corruption), preserve the offending text in the error.

3. **`commands/backup_restore.rs:386-399` — `migrate_entry_json` is an identity function and the comment promises it, but the from-version is never validated against the migration table.** A future writer who lands a v8→v9 entry-shape migration must remember to gate on `from < 8`. There's no compiler-enforced reminder. A `match (from, to)` that panics on unknown spans would force the question; today the function silently accepts any `(from, to)` pair.

4. **`commands/dayview.rs:506-518` — `switch_region_end_ms` is O(n) over a sorted slice but uses a backwards walk with an `Option<i64>` accumulator.** Functionally correct. Hard to read. A two-pointer forward walk (`for_each` with peek-next) would be ~half the cognitive load. Cosmetic.

5. **`commands/dayview.rs:489-494` — `is_unrendered_switch_value` accepts `None` as an unrendered switch, but the call site already excludes `None` in most paths.** The `None` branch is only reached for explicitly null-valued JSON inputs and for `Switch` entries with no `value` column. The two cases want different rendering (JSON-null = "intentional state end", missing = "data error"), but we treat them identically. Probably fine pre-v1; once kit format is locked, JSON-null might want to render visibly different from missing.

6. **`cmd.rs:1772-1778` — `cmd_grep` falls back to `entries_in_range(UNIX_EPOCH, now+365d)` when `--field` is absent.** The 365-day clamp is a magic number. Backups restored from years ago will have entries outside the window and silently won't match. Either remove the clamp (the field-absent path is already an "all entries" search) or document it.

7. **No test exercises grep with `--field` against a tombstoned entry.** `cmd.rs:4214+` (`grep_single_pattern_regex`, `grep_multi_pattern_and`, `grep_respects_field_filter`) all run on live entries. Whether grep should match tombstoned rows (it currently doesn't, because `entries_for_field` and `entries_in_range` both filter them) is undocumented. Easy to break by accident.

8. **`render/action.rs:378 — synthesize_local_datumtime(utc)` reuses the system's local TZ for `create_time` / `modify_time` even when the entry's own `timezone` column says otherwise.** When you log "yesterday at 9am Berlin time" from your Chicago laptop, the `time_start` carries Berlin. The `create_time` / `modify_time` were stamped with Chicago. `format_info_line_at` (action.rs:444-445) reuses Chicago for both, which is right. But a user looking at `c14:23` next to `9:00+1` expects the create-time to be in their *current* TZ, not necessarily Chicago at write-time. This isn't wrong — it's just under-specified. Once cross-TZ logging is real, decide and document.

---

## 5. The non-obvious improvement: a verb-shape decision table

The bug in da-7r4 was a dispatch-tree mistake: shape-first, then verb. The fix was case-by-case (Switch + Point → region; Switch + Block → region; everything else → existing path). It works. **It's also written exactly once, in `commands/dayview.rs:521-526`, and only describes Switch.**

The same decision tree is implicit in `commands/nowview.rs`, in `crates/datum-layout/src/dayview.rs`, in the action-line state-glyph picker (`render/action.rs`), and in the SVG path (`crates/datum-layout/src/dayview_svg.rs`). Each renderer makes its own decision about "what shape is this entry, given verb V and shape S, in render surface R." Today most of them agree. The drift is what produced the original bug.

The improvement is a single tabular rule:

```rust
// crates/datum-commands/src/render/dispatch.rs (new file)
pub enum RenderShape { Region, Point, Block, Tick, Hidden }

pub fn render_shape(verb: Verb, entry_shape: EntryShape, value: &Option<JsonValue>) -> RenderShape {
    use Verb::*;
    use EntryShape::*;
    match (verb, entry_shape) {
        // State-bearing verbs render as regions, regardless of stored shape.
        (Switch | BlockSwitch | PointSwitch | Start | Stop, _)
            if !is_unrendered_switch_value(value.as_ref()) => RenderShape::Region,
        (Switch | BlockSwitch | PointSwitch | Start | Stop, _) => RenderShape::Hidden,

        // Non-switching verbs follow the entry's stored shape.
        (Add | Note, Block) => RenderShape::Block,
        (Add | Note, Point) => RenderShape::Point,
        (Add | Note, _) => RenderShape::Tick,

        // Add other verb arms here as they are introduced.
    }
}
```

…and every renderer calls it. The function is the **single source of truth for "how does an entry render, given its semantic shape"**. Color, position, glyph all flow downstream of `RenderShape`. New verbs add one match arm. New shapes add one variant. The data-model gate is at one site, not seven.

This is the same shape as amber's §5 `RenderContext::for_entry` proposal — a single constructor that the surfaces consume — but at a different level of abstraction. Amber's lifts context construction. This lifts shape dispatch. They compose: `RenderContext::for_entry` decides *what data goes into a render*; `render_shape` decides *what visual category that data lives in*. Both want to be one function. Both currently aren't.

The migration is mechanical: grep `EntryShape::Point` and `Verb::Switch` together across `crates/`, replace each branch with `match render_shape(...)`. Estimated half-day. Pre-v1 the cost is low; post-v1, every new verb (`UncertainAdd`, `Retime`, the kit-format-bound ones in da-d4t.5) will pile new branches into seven renderers.

---

## 6. My area's relationship to the whole

I'm a **read-path consumer**. Everything I touch sits downstream of write (obsidian's seat), schema (jade/amber/onyx's seat), parse (garnet's seat), and layout (ruby's seat). The five commits listed above are five places where the **user noticed** that the upstream layers shipped something almost-right but visibly wrong, and the fix lives at the leaf.

That's the small-commit polecat's seat. We don't shape the schema. We notice when the renderer stopped showing state changes, when the backup forgot to mark its version, when grep didn't exist, when three identical helpers drifted, when a test failed every third run. These are not load-bearing changes — none of my work would block v1 if I'd never landed it. They are **friction-shedding** changes — the cumulative texture of the product gets smoother because each one happened.

Three observations from this seat:

**Foundational polecats (onyx, ruby, amber, jade) ship features. Surgical polecats ship trust.** Users don't experience grep as a feature; they experience "the tool didn't have grep until last month, now it does, that's reassuring." The S7 backup header is invisible — its *presence* is invisible — until the day someone restores from a v3 backup onto a v8 binary and it Just Works. Trust accrues from places nobody is grateful for.

**Big rewrites under me didn't break my work, but small rewrites near me did.** The schema pivot (da-5ns) added two new switching verbs and my `is_switching_verb` match accepted them silently. R1's render carve-out (dd5ea54) created the duplicate-helpers problem that my da-d5w.1 cleaned up. The big moves were locally type-checked. The small moves left orphans. **Splits leak; pivots don't.** Worth keeping in mind for the R2/R3/R4 sequel: every split should ship with a "delete the orphans" follow-up.

**Test pollution is a downstream symptom of write-path determinism gaps.** da-afk's flake came from `DatumTime::now()` being non-deterministic in a write path that didn't take an injected clock. There are five other write paths I haven't audited. Every one of them is a future flake when their tests grow. Pre-v1, **the highest-leverage one-day investment for test stability is to make every write path take a `Clock` parameter and pass `Clock::system()` from the binary's main**. obsidian's seat would be the right one to take it.

---

## Closing — the trim's view

I look at this rig from the position of someone who fixed five user-visible things and watched the rest of the swarm refloor the building under me. None of it broke. None of my fixes got reverted. None of the bugs I noticed (§4) are blocking v1.

The places where my work would benefit most from another half-day:

1. The `render_shape` dispatch table (§5). Pre-v1, before kit-format verbs land.
2. The `migrate_entry_json` registration discipline (§4.3). Compile-enforced before the next entry-shape change.
3. The schema-version overshoot error message (§4.1). One-line UX fix.
4. A `Clock`-param audit of every write path (§6). Highest-ROI test-stability investment.

None of these are mine to grab next — they're scattered across seats. But they are **what someone in the small-commit seat would file as their next five beads** if there were a sixth commit waiting to be made.

— topaz, 2026-05-06
