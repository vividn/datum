# Alias system redesign — unified table with provenance

## Problem

Aliases today come from two sources merged at resolve time (`db/spec_alias_index.rs` builds one map): manual aliases (`alias set`) and spec-derived ones (`derived_aliases`, regenerated on every spec save). This breaks down on three fronts:

- **Homing.** Field/state aliases belong to a field's spec, but many aliases don't — multi-field comma-aliases (`oov → switch environment outside , switch body vertical`) and field-less ones (`tpd → update proj=datum`). No field can own those.
- **Editability.** Derived aliases are regenerated on save, so a user edit or removal is clobbered next save.
- **Collisions.** State names collide across fields; today the first-saved spec wins (arbitrary, order-dependent) and the loss is resolved invisibly at load.

## Design: one table, provenance-tagged

A single **alias table** is the source of truth, resolved by the CLI pre-pass. Each row:

- `name` — the alias token.
- `expansion` — the command body it expands to.
- `source` — provenance: `manual` | `spec:<field>` | `spec:<field>/state:<state>`.

Manual rows are user-authored (including multi-field and field-less aliases). Generated rows are *projected* from field specs.

This is mostly **consolidation**: datum already merges manual + derived at load; this makes the merge an explicit stored table and tags each row with where it came from.

## Field specs project generated aliases

On `track` / spec-save, a field projects into the table:

- **Field alias (auto, default-on):** `<field> → <default_verb> <field>`, source `spec:<field>`. Collision-safe — field names are unique.
- **Terminal alias:** `<field>e → end <field>` (duration-capable verbs), source `spec:<field>`. A user may add a nicer manual one (e.g. `over → end activity`).
- **State aliases (opt-in):** only when the field's `auto_alias_states` flag is on, each state projects `<state> → switch <field> <state>`, source `spec:<field>/state:<state>`.

## Regeneration — scoped by source

On spec save: **drop rows whose `source` matches this field, then re-project the current set.** Manual rows are never in the blast radius. Idempotent.

## Collision precedence

- **Manual > generated** — a hand-set alias shadows a generated one (user intent wins).
- **Generated vs generated** — warn (or error) at projection time; visible because it's all in one inspectable table. Rarer now that state-aliasing is opt-in.

## Edit & delete of a generated row

- **Edit promotes to manual** — editing a generated row flips its `source` to `manual`, so regen leaves it alone ("modifiable to the user's liking").
- **Delete needs suppression** — pure re-projection would resurrect a deleted generated row. Deletion must either turn that source off in the spec, or record a suppression tombstone. A suppression set is simplest and uniform.

## The `track` wizard

`track <field>` gains an interactive mode; the flag form stays as the express path.

1. Field alias — default **on** (confirm / skip).
2. If switch-type: "auto-alias new states?" → sets `auto_alias_states`.
3. Collect allowed states — one per line, **or** a pasted comma-list (don't force nine prompts).
4. Optional: color, tag.

Non-interactive `track <field> --state … --alias …` remains for scripting and re-runs.

## Keep it lean

Provenance is just the `source` field + the promote-on-edit rule + a suppression set. Resist a richer ownership lattice — it won't earn its keep, and lean is datum's whole thing.
