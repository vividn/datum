# The datum tracking protocol

Datum exists so you can log moments and see where your time and attention actually go. The logging *is* the point: naming what you're doing, as you do it, is a small act of mindfulness. This document is the protocol — what the pieces mean and how to keep the record honest.

## An entry, in one line

`field` + `verb` + `state`/value + `time`. *What* you're tracking (`coffee`, `mood`, `activity`), *how* it changes (`occur`, `switch`, …), the value, and *when*. Time is always derivable; duration is always *derived*, never stored twice.

## Verbs — choosing the right one

- **occur** — a point event happened. `coffee`, an arriving message, one rep. No duration (or a negligible one).
- **start / stop** — open a block now, close it later, when you don't yet know the end (a sleep, a focus session).
- **switch** — change the *current* state of a field. The new state runs until the next switch. This is how you track "what is true now": mood, posture, the activity you're in.
- **block-switch** (`switch f X <dur> , switch f Y`) — one-shot: "was in X for `<dur>`, now Y." Use when the block is already closed and you know what follows.
- **note** — an annotation that doesn't change state.

## Keeping time honest (the spine of the whole thing)

1. **Time comes from the record, not your head.** Before claiming *when* something happened, look it up — the entry, the log, the timestamp. Never approximate ("yesterday", "this morning"); an approximation is a guess wearing the costume of a fact.
2. **Record prospectively when you can.** Closing a block at the real moment you stop beats reconstructing it later. Reconstruction is the exact surface where time-errors creep in.
3. **When you must reconstruct, mark it.** Backfilled or approximate entries should carry a flag (`backfilled=true`, `approx=true`) so the data admits its own uncertainty.
4. **One timezone, consciously.** Pick the frame you live in and keep entries in it; a silent tz shift turns a clean record into a puzzle.

## The `activity` field — a worked example

`activity` is a `switch` field: at any moment exactly one state is active and runs until you switch. It answers "what am I doing right now?" Closing it (`switch activity end`) marks "nothing active" — a gap.

The states below are tuned for knowledge work. They earn their keep by being *distinguishable in the moment* — if you can't tell which of two you're in, merge them.

| state | what belongs here | not this |
|---|---|---|
| **thinking** | pure reasoning, planning, weighing a tradeoff — no input or output | reading (→ researching) |
| **researching** | reading to understand: code, docs, history, search; reviewing a diff | running things (→ testing) |
| **coding** | writing *new* code — a feature, a function, a test | reshaping existing code (→ refactoring) |
| **refactoring** | restructuring existing code without changing its behavior | net-new code (→ coding) |
| **testing** | running tests/binaries, inspecting output, reproducing a bug | reading code (→ researching) |
| **writing** | authoring prose: notes, docs, bead text | editing tracked entries (→ data) |
| **data** | tending the record itself: backfills, fixes, field specs, tz cleanup | tool/env setup (→ configuring) |
| **configuring** | setting up tools/env: hooks, settings, build, PATH | editing the record (→ data) |
| **talking** | conversation, replies, coordination messages | — |

`end` is not a state — it's how you close the active one. The span between an `end` and the next switch is honest "away" time.

## Where this is heading

These definitions are *protocol*: the meaning of a field and its states. Today that meaning lives in prose (here). The natural next step is for datum's **field spec** to carry it natively — a field declaring its purpose, and each state of an enum-field carrying a description. Then meaning travels *with* the data: queryable, validatable, renderable, instead of in a doc that drifts. (Tracked in bead **da-zwqk** — per-state descriptions on field specs.)
