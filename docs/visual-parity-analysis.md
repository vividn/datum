# Visual Parity Analysis — Output Rendering vs. Original datum

Source of truth: `/tmp/datum-survey/src/output/output.ts` and
`/tmp/datum-survey/src/field/fieldColor.ts` (original datum, TypeScript).

This note captures the visual decisions the original datum makes when rendering
a write action (CREATE/UPDATE/…/SWITCH/START/STOP) so daydumb's
`datum-commands/src/render/` can match the intent. Readers of later audits
should treat this as descriptive, not prescriptive — when daydumb diverges, a
short rationale is included.

## Two-line record

Every write produces exactly two lines (plus optional extra_data block):

```
 CREATE  hHID  docId
occurTime field stateBlock dur
```

Line 1 (`showHeaderLine`): inverse-coloured action tag + 5-char humanId + full
id, all in the action's colour.

Line 2 (`showMainInfoLine`): occurTime + field + state + dur, joined by single
spaces, omitting empty columns.

daydumb matches this layout already (renders `slug` in place of CouchDB `_id`;
`hid` is the daydumb 12-char base36 preserved in the same role).

## Action colours

`ACTION_CHALK` in `output.ts`:

| Action  | Colour    |
| ------- | --------- |
| CREATE  | green     |
| DELETE  | red       |
| EXISTS  | yellow    |
| UPDATE  | cyan      |
| OWRITE  | blue      |
| RENAME  | cyan      |
| NODIFF  | `#ffa500` (orange) |
| FAILED  | red       |

Tag is rendered with `chalk.inverse(" ACTION ")` — i.e. an inverse-video pill.
daydumb already matches this table and uses the same inverse pill treatment.

## Field and state colours — **background-first**

This is the biggest drift point. Original datum uses the field's colour as a
**background** via `chalk.bgHex`, pairing it with a computed contrast-preserving
foreground (`getContrastTextColor`). Current daydumb uses the colour as a
foreground only, producing a dim, pipe-friendly look that loses datum's dense,
timeline-like feel.

```ts
// fieldColor.ts
export function fieldChalk({ field }: { field?: string }) {
  const color = getFieldColor(field);
  const fg = getContrastTextColor(color);
  return chalk.bgHex(color).hex(fg);
}
```

State text is likewise rendered with `bgHex(stateColor)` + contrasting fg,
except when state is `false`/`undefined` (end/empty): then the text is rendered
without any background (default terminal colours).

Colour sources (in precedence order):
1. `FieldSpec.color` or `FieldSpec.states[state].color` — hex string.
2. `md5Color(name)` fallback — `#` + first 6 hex chars of the MD5 of the name.

daydumb uses a `hash_to_ansi256(sha256(name))` fallback that keeps one channel
≥3. That's fine for unstyled fields but for **spec-defined** colours we should
emit truecolor background escapes (`\x1b[48;2;R;G;Bm`) and a contrasting
foreground — not just a dim 256-colour foreground.

## State block: before / current / after

`formatState` in `output.ts:93` is the core visual motif. It composes three
text fragments using two chalk instances:

```
beforeText     currentText     afterText
(lastState     (state colour)   (state or
 colour bg)                      field colour bg)
```

Fragments:

| Case                                     | beforeText                      | currentText           | afterText                    |
| ---------------------------------------- | ------------------------------- | --------------------- | ---------------------------- |
| non-occur note (`occurTime === undef`)    | `¢` in field colour             | state or empty        | empty                        |
| point (`dur===null` or no dur+no state)   | `●` on `stateColor` bg          | state or empty        | empty                        |
| no-state-change with positive dur         | `!` bold + warning fg on prior bg | (current)             | trailing space on current bg |
| state-change with negative dur            | `!` bold + warning fg on prior bg | (current)             | trailing space on current bg |
| prior state was `null`                    | `∅` on grey-grey                 | (current)             | (normal)                     |
| prior state was an array                  | `▞` on prior-state bg           | (current)             | (normal)                     |
| normal prior state                        | single space on prior-state bg   | state text on state bg | trailing space on state bg   |
| false state (end)                         | prior-state space               | `end` (no bg)         | empty                        |

The visual effect is **a coloured stripe for the prior state, then the new
state's coloured stripe, with a one-character gap that is itself coloured by
the respective state**. This looks like a mini timeline tick in the CLI.

daydumb's current renderer does not emit `beforeText` or `afterText` at all —
it just colours the state word. To match datum's density we need to emit the
same three-fragment structure whenever `lastState` is known.

### `lastState` availability in daydumb

Original datum denormalises `lastState` onto every switch doc, so the renderer
can always read `data.lastState`. daydumb deliberately does **not** denormalise
(see `crates/datum-cli/tests/switch_cmd.rs` comments). Options:

1. **Pull from `extra_data.last_state` when present** — `stop --last-state`
   already writes this; use it.
2. **Accept an optional `prior_state` via a `RenderContext`** — callers that
   have a `Db` handy can pre-query `db.get_active_state(field, time)` cheaply
   and pass it in. Switch/start commands do this anyway for dedupe; the value
   is already in hand.
3. **Omit `beforeText` when prior state is unknown.** No-op fallback keeps
   freeform-logging output readable.

Chosen approach for this pass: (1) + (3). (2) is a follow-up — the
command-layer plumbing is the bulk of that change and can land separately
without regressing the visual.

## First-time-tracked marker

Original datum does **not** emit a first-time marker in its line-oriented
output. The "first time I saw this field" signal lives in the SVG/PNG timeline
renderer (which draws a brighter tick mark), not in CLI text.

The bead requests a new first-time marker for daydumb CLI. Minimal visual
motif, chosen to be easy to strip in `--no-color`:

- **`✨ `** prefix on the field column when this is the first entry daydumb
  has ever seen for this field (checked via `db.load_spec(field).is_none()`
  *before* `ensure_tracked`).

Spec-less fields always get the marker. Once a spec exists (either via
explicit `datum track` or implicit `ensure_tracked`), subsequent writes render
plainly. `strip_ansi` preserves the sparkle (it's a printable glyph, not an
ANSI escape) which is intentional: even in pipes the user sees that this is a
new field they just introduced.

## Duration rendering

`formatDuration` in `output.ts:183` uses Luxon `duration.toFormat("m'm'")` —
always minutes, no hours or composite units. daydumb's `format_duration`
composes seconds / minutes / `HhMm`, which is *more* informative. Keep as-is.

## `--no-color` / pipe friendliness

Original datum relies entirely on chalk's auto-detection (TTY + env), with no
explicit `--no-color` flag. daydumb already gates colour on TTY detection and
a private `DATUM_COLOR` env var used by the test harness.

Action for this pass: also honour the de-facto standard **`NO_COLOR`** env
var (see <https://no-color.org/>) so ordinary pipes / `| cat` / `| less -R`
behave correctly without needing to set `DATUM_COLOR=0`. `strip_ansi()` is
already exposed for snapshot tests.

## Summary of changes for S4

1. **Background-primary palette** — `field_color_code` / `state_color_code`
   return background escapes with a contrast-aware foreground companion, not
   a single foreground escape.
2. **Three-fragment state block** — when a prior state is known, render
   `beforeText + currentText + afterText` matching datum's `formatState`
   table above.
3. **First-time-tracked marker** — `✨ ` prefix on the field column for
   writes where `load_spec(field)` returned `None` before `ensure_tracked`.
4. **`NO_COLOR` env** — add to the colour gate alongside TTY/`DATUM_COLOR`.
5. **Tests** — add snapshot-style assertions for the background, transition,
   and marker cases, plus a `NO_COLOR=1` stripping test.

Non-goals for S4 (leave for follow-up): denormalising `lastState` onto every
switch entry; piping prior-state into every command callsite; refactoring
the `Extracted` struct layout.
