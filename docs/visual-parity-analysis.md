# Visual Parity Design

Source of truth: `/tmp/datum-survey/src/output/output.ts` and
`/tmp/datum-survey/src/field/fieldColor.ts` (original datum, TypeScript).

This document is the committed specification for daydumb's write-action
renderer. It supersedes the earlier "Visual Parity Analysis" note. Changes
to the renderer must keep this doc and the fixture tests
(`crates/datum-commands/tests/render_fixtures.rs`) consistent.

## 1. Color derivation

- **Algorithm:** MD5 hash of the string (field name OR state name). First 3 bytes → RGB:
  - `r = hash[0]`, `g = hash[1]`, `b = hash[2]`
- **No re-rolling.** Every MD5 output is a valid background color.
- **Readable text via YIQ contrast** (port `getContrastTextColor` from `/tmp/datum-survey/src/utils/colorUtils.ts`):
  ```
  brightness = (R*299 + G*587 + B*114) / 1000
  text_color = brightness > 128 ? "#000000" : "#ffffff"
  ```
- **Spec override:** `spec set <field> --color <hex>` and `spec set <field> --state <name>=<hex>` take precedence over MD5. Check spec first; fall back to hash.
- **Emit via ANSI truecolor** (`\x1b[48;2;R;G;Bm` for bg, `\x1b[38;2;R;G;Bm` for fg).

## 2. Line layout

```
<time>  <field-badge>  <state-transition-group>  [duration]
                       └─ <LEAD cell><state-badge><TRAIL cell>
```

- Time, field, state-transition, and duration are all separated by whitespace.
- **Duration comes AFTER the TRAIL cell**, not inside the state group.
- The state-transition group is self-contained; the field badge stands alone.

## 3. Timestamp

- Format: `HH:MM:SS+TZ` (e.g. `17:10:00+2`)
- **Underlined** if `occur_time` is in the FUTURE relative to now-at-render. Helps prevent entry errors.
- **TZ offset** (`+0200` → compressed to `+2`; always drop leading zeros) rendered **dim** (ANSI 2) to de-emphasize.

## 4. Field badge

- Box: `<space><field_name><space>` with background = field color (MD5 or spec)
- Foreground = YIQ-contrast-computed black or white
- Stands alone; no prior-state indicator attached

## 5. State rendering helpers (port from fieldColor.ts)

**getStateColor(state, field):**
- `undefined` → field color
- `true` → field color
- `false` → `#000000`
- `"IGNORED"` → `#888888` (replaces datum's `null` branch — we dropped null)
- Array → first element's color (recursive)
- Named state → spec override OR md5 hash

**stateChalk(state, field) — determines badge styling:**
- `undefined` → fg=field-color, no bg
- `true` → bg=field-color, fg=contrast (same as field badge)
- `false` → no styling at all (plain text)
- `"IGNORED"` → bg=`#888888`, fg=`#666666` (grey on grey)
- Array → bg=first element's color, fg=second element's color (this IS the checkerboard pair)
- Named state → bg=state color, fg=YIQ-contrast

## 6. State transition group — LEAD / state-badge / TRAIL

The LEAD, state-badge, and TRAIL form ONE visual group after the field badge. **Per da-25l the three segments are flush against each other** — no whitespace, colored or otherwise, between LEAD/cell/TRAIL.

**Rendering update (post-da-25l):** LEAD and TRAIL are rendered as **single bg-coloured cells** (a single space painted with the relevant state's bg color), **not as glyph characters**. The original draft of this spec described `∅` / `→` / `←` glyphs in those positions; the renderer chose colored cells instead because they're more compact and compose with the state-badge body without a visible seam. The implementation lives in `crates/datum-commands/src/render/action.rs::render_slot` (LEAD/TRAIL) and `::render_state_cell` (state-badge body).

The `∅` glyph is still rendered for `occur`'s LEAD (a tracked-but-never-seen field gets a grey-`∅` rather than a grey-cell, because occur has no state-badge and the glyph is the entire trio). For switch / start / stop / block, LEAD/TRAIL are colored cells.

**LEAD rules** (state active BEFORE this entry):
- Prior `true` → cell filled with `true`-color (= field color)
- Prior named state → cell filled with that state's color
- Prior `false` AND this entry is `end` → **yellow `!`** glyph (repeat/overlap detection — feeds the integrity queue)
- No prior entry exists (first tracked event for this field) → cell painted IGNORED-grey (`#888888`). The original spec called for a `∅` glyph here; the renderer uses a grey cell for consistency with the rest of the LEAD/TRAIL behavior.
- Prior state is array → cell painted with FIRST element's color, FG = SECOND element's color, body = `▞` checkerboard glyph (per da-9vop — both colors visible in 1 cell).
- Prior state is `"IGNORED"` → cell painted IGNORED-grey (`#888888`)
- Field has explicitly ended (`Bool(false)` trail after stop) → uncoloured space (no bg)

**State badge body:**
- Solid background = scalar state (use stateChalk above)
- **Checkerboard `▞`** with bg=state[0], fg=state[1] = array state (>2 states: truncate to first two for color)
- Dot/sliver character for point events (occur, add) — see §7

**TRAIL rules** (state active AFTER this entry):
- Switch with NO duration → cell painted with new state's color (state continues)
- Switch WITH duration (block) → cell painted with **prior state's color** (prior resumes after block)
- `start` → cell painted with `true`-color
- `end` → cell painted with `false`-color (dim/neutral). When the prior state was `false` (a stop-after-stop), the cell is uncoloured space.
- occur (point) → cell painted with prior state's color (doesn't change state)
- End-of-block where already ended → **yellow `!`** glyph (overlap detection)

Same `da-25l` colored-cell rule applies: TRAIL is a single cell, no glyph, painted with the appropriate state color.

## 7. Per-verb rendering

### `add <field> [keys]` (timeless)

- Field badge
- `¢` replaces the state-transition group (mirrors `c` suffix in the generated slug id)
- No state context chars, no `∅` (add is metadata, doesn't affect state)

### `occur <field>` (point)

- Field badge
- Single colored dot/square char as the state-badge:
  - Tracked field → field color (or state color if `-m` moment)
  - Ignored/untracked → grey bg, no field color
- `∅` glyph in LEAD slot if first-ever tracked entry for this field (note: occur is the only verb where LEAD is still rendered as a glyph; switch/start/stop use colored cells per §6)

### `switch <field> <state>` (no duration)

- Field badge
- LEAD cell: prior state's color (or IGNORED-grey if first; see §6)
- State badge solid (background = state color, foreground = state label)
- TRAIL cell: new state color (state continues)

### `switch <field> <state> <dur>` (block interruption)

- Field badge
- LEAD cell: prior state's color (or IGNORED-grey if first)
- State badge solid
- TRAIL cell: **prior state's color** (resumes after block)
- Duration `Nm` trailing after TRAIL cell

### `switch <field> <s1>,<s2>[,...]` (array)

- Field badge
- LEAD cell: prior state's color (FIRST element if prior was array)
- Two adjacent state badges with a comma between; comma colored like `state2`
- Both badges use checkerboard `▞` with bg=state1, fg=state2
- TRAIL cell: follows block/continue rules based on duration

### `start <field>` (opens block)

- Field badge
- LEAD cell: prior state's color (usually `false` → dim/neutral; yellow `!` if already active)
- State badge: solid `start`-green (or equivalent `true`-colored)
- TRAIL cell: `true`-color

### `end <field>` / `stop <field>`

- Field badge
- LEAD cell: prior state's color (yellow `!` if already ended)
- State badge: `end`-green with left-half prior-state colored (half-fill indicates "was active on left side")
- TRAIL cell: `false`-color

### `start <field> <dur>` (retroactive closed block)

- Field badge
- LEAD cell: prior-before-block
- State badge: solid
- TRAIL cell: prior-resumes color
- Duration trailing

## 8. Special states

- **`"IGNORED"`** — set by `datum ignore <field>`. Grey bg override (`#888888`), dimmed text.
- **`true` / `false`** — universal; `true` uses field color, `false` has no styling.
- **Array** — concurrent states; see §5 and §7.

## 9. Track / ignore command behavior

- `datum ignore <field>` — writes entry with `value = "IGNORED"`
- `datum track <field>` — writes entry with `value = false` (tracked but stopped)
- `datum now` filters: show if most-recent state != `"IGNORED"`
- `datum now -A` shows all, including ignored

## 10. UTF-8 character references

| Char | Codepoint | Purpose |
|---|---|---|
| `∅` | U+2205 | First-tracked / null-state marker |
| `¢` | U+00A2 | add/create marker |
| `!` (yellow) | U+0021 + ANSI yellow fg | Repeat/overlap warning |
| `▞` | U+259E | Checkerboard (upper-right + lower-left half) |

## 11. Test fixtures (20 cases)

Integration tests in `datum-commands/tests/render_fixtures.rs`. Assert exact rendered output (ANSI-stripped for readability + separate golden files for with-color).

| # | Input | Expected render (ANSI-stripped) |
|---|---|---|
| 1 | `datum add new_occur` | `16:58:47+2 [new_occur] ¢` |
| 2 | `datum occur new_occur` (first) | `16:58:56+2 [new_occur] ∅[·]` |
| 3 | `datum occur new_occur` (again) | `16:58:59+2 [new_occur] [·]` |
| 4 | `datum start new_start` (first) | `16:59:40+2 [new_start] ∅[start]→` |
| 5 | `datum end new_start` | `16:59:46+2 [new_start] ←[end]` |
| 6 | `datum start new_start` (after end) | `16:59:52+2 [new_start] [start]→` |
| 7 | `datum switch new_switch state1` (first) | `17:00:21+2 [new_switch] ∅[state1]→` |
| 8 | `datum switch new_switch state2` | `17:00:30+2 [new_switch] ←[state2]→` (left=state1) |
| 9 | `datum switch new_switch state3` | `17:00:38+2 [new_switch] ←[state3]→` (left=state2) |
| 10 | `datum switch new_switch repeat` | `17:00:52+2 [new_switch] ←[repeat]→` |
| 11 | `datum switch new_switch repeat` (SAME) | `17:00:55+2 [new_switch] ![repeat]→` (yellow `!` left — repeat) |
| 12 | `datum end new_switch` | `17:01:13+2 [new_switch] ←[end]` |
| 13 | `datum start repeat_start` (already active) | `... [repeat_start] ![start]→` |
| 14 | `datum switch dur_blocks state1` (first) | `17:02:26+2 [dur_blocks] ∅[state1]→` |
| 15 | `datum switch dur_blocks state2 4 -t 1710` | `17:10:00+2 [dur_blocks] ←[state2]→ 4m` (right=prior=state1; time underlined = future) |
| 16 | `datum switch field multi1,multi2` | `17:12:13+2 [field] ←[multi1],[multi2]→` (comma state2-colored; checkerboard `▞` in both badge bgs) |
| 17 | `datum switch field momentary_single 1 -t +3` | `17:15:44+2 [field] ▞[momentary_single]▞ 1m` (prior & next both checkerboard; time underlined) |
| 18 | `datum ignore foo; datum now` | `[foo]` with grey `#888888` bg |
| 19 | `datum track foo; datum now` | `[foo]` with field color, state=false |
| 20 | `datum add thing key=value` | `[thing] ¢` — no state group, no `∅` |

## 12. Implementation files

- `crates/datum-commands/src/render/action.rs` — action-output renderer (per-verb dispatch)
- `crates/datum-commands/src/render/colors.rs` — MD5 + YIQ contrast (replace SHA-256)
- `crates/datum-commands/src/render/characters.rs` — unicode char constants
- `crates/datum-commands/tests/render_fixtures.rs` — the 20 fixture tests
- `docs/visual-parity-analysis.md` — this document, committed

## 13. Dependencies

- **S6 (da-u4k.6)** — needs `last_state_at(field, time)` helper and generalized repeat detector
- **da-u4k.12 (array states)** — checkerboard/array rendering and comma parsing

## 14. User review gate

**Must not auto-close.** Final PR must include a side-by-side screenshot (or ANSI paste) of the 20 fixtures vs the reference image at `/tmp/datum_coloring_examples.png`. User must thumbs-up before close.

## Acceptance

- `docs/visual-parity-analysis.md` exists, matches this spec
- All 20 test fixtures pass (ANSI-stripped asserts + with-color golden files)
- MD5 + YIQ replaces any SHA-256 color code
- `¢` / `∅` / `!` / `▞` characters render correctly
- Timestamp underline only on future times
- TZ offset dim + compressed
- `ignore` / `track` commands write correct state values
- **User explicit approval** on review before merge
