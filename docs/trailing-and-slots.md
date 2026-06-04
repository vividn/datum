# Trailing arguments & slot allocation — spec v2

Status: **ratified** (design session, vividn + Vremya, 2026-06-04 ~10:20–11:40
Berlin; boundary token amended `--` → `,,` and unix `--` restored, same day
~13:49). Supersedes the trailing/allocation behavior merged in #42/#44/#45
(da-vo0y features 1–3, da-6l4x). Companion to `docs/alias-system-design.md`.
The rework beads reference this document; where the merged code disagrees
with this spec, **this spec wins**.

User-facing note: `,,` is mostly machinery (every alias resolve emits one),
but it is *typeable* and must be documented in the user docs alongside `,`
— it is part of the command grammar, not an implementation detail.

## Grammar

1. **Sides.** `,` separates sides. Each alias resolution appends `,,` at
   the end of its expanded body — the boundary marker and the trailing
   marker are the same token.
2. **Trailing regions.** `,,` opens a *trailing region*; the next `,`,
   the next `,,`, or end of argv closes it. Empty regions are no-ops;
   they occur constantly (every bare alias call produces one). Regions
   apply left-to-right; later regions see earlier fills as satisfied.

   **Binding scope** (implementation reconciliation, 2026-06-04
   afternoon — both ratified statements hold):
   - A **typed** `,,` (written by the user in the original command)
     binds **all sides to its left** — the cumulative prefix, per the
     `t2 binds {a,b,c,d}` ruling.
   - An **alias-emitted** boundary binds **that alias's own expanded
     sides** ("the sides they are referencing") — so `vvcaf`-style
     composition (`vertical , caffeine -t +5`) keeps its inner trailing
     side-scoped, and `z , oov 30m` puts 30m on oov's two sides, not on
     z. Without this split, an inner alias's trailing would leak onto
     unrelated earlier sides.

   ```
   a , b ,, t1 , c , d ,, t2    # typed: t1 binds {a,b}; t2 binds {a,b,c,d}
   oov , switch project gardening
     # oov = "outside , vertical" expands to:
     # outside , vertical ,, , switch project gardening  (empty region, no-op)
   ax y
     # a  = "occur a -k keyX -k keyY";  ax = "a x" — expands to:
     # occur a -k keyX -k keyY ,, x ,, y   (x→keyX; keyX satisfied, y→keyY)
   ```
3. **Region semantics.** Within a region: flags and `key=value` tokens
   propagate to **every** prefix side ("blanket"); bare positionals walk
   the prefix's unfilled **slot list**.
4. **Unix `--` — end of option parsing, side- and alias-scoped.** `--`
   keeps its conventional meaning: every later token *in the same side or
   region* is positional data, even when dash-prefixed. The effect ends at
   the next `,` or `,,`, and a `--` inside an alias body applies only
   within that body — it never leaks to the call site or outer scopes.
   Dash-positionals participate in slot walks like any positional (they
   slot; they do not blanket).

   ```
   note cmd -- --verbose --dry-run        # stores two dash-positionals
   note cmd -- --verbose , occur coffee -q  # side 2's -q is still a flag
   ```

## Slot model

5. **Per-side slot array**, in verb-grammar order. One greedy mechanism;
   only its *placement* varies:

   | side shape          | slot array                                       |
   |---------------------|--------------------------------------------------|
   | `-I` present        | `[field?, state(greedy), -k keys…, dur?]`         |
   | `-S`/`-R` present   | `[field?, -k keys…, remainder(greedy), dur?]`     |
   | neither             | `[field?, state?, -k keys…, dur?]`                |

   `field?` exists when the body is verb-only (`note` → `add`: the field
   itself comes from the call site, lands right after the verb, strictly
   per-side). `state?` exists for switch-family verbs; `dur?` for
   dur-bearing verbs. There is **no** primary-value slot — `value` is a
   retired concept (vividn, 2026-06-04), and a bare positional beyond the
   declared slots is excess (rule 9): `occur coffee 2` errors with or
   without an alias in between. At most one greedy per side (`-I`
   conflicts with `-S/-R/-C` at validation, which guarantees it).
6. **Cross-side unification** (per region walk): `-k` slots unify **by
   name**; state and dur slots unify **by kind**. A unified slot fills
   once and propagates to every side that declares it. Satisfied slots —
   filled in-side, by explicit `key=value`, or by an earlier region — are
   skipped by the walk.

   ```
   occur f1 -k key1 -k key2 -k key3 , start f2 -k key2 -k key4 key5=val5 \
       ,, key6=val6 val1 val2 val3 val4 30m
   # f1: key1=val1 key2=val2 key3=val3 key6=val6
   # f2: dur=30m  key2=val2 key4=val4 key5=val5 key6=val6
   start run , start bike ,, 30m        # both get dur=30m (kind-shared)
   switch mood blue , switch outfit red # differing values: fill in-side
   ```
7. **Greedy consumption — count-from-the-end reservation.** A greedy slot
   takes all remaining positionals **minus** the last *n* tokens, where
   *n* = unfilled non-greedy slots after it in the walk. Reserved tokens
   fill those slots in order.
   - **Dur reservations are type-gated**: a reserved token that does not
     parse as a duration flows back into the greedy (the F2 peek,
     generalized). `switch f -I deep conversation 30m` → state="deep
     conversation", dur=30m; `switch f -I waiting for godot` → all state.
   - **Key reservations are not type-gated** (a key value is
     indistinguishable from a state word); the last token goes to the key
     unconditionally. `.` skips a slot; `\.` escapes a literal dot.
   - If reservations leave a walk zero tokens for a string **join**,
     error ("nothing left for the `-I` state after reserving for
     `-k …`") rather than emitting an empty join. A **sink** with
     nothing to absorb is simply at rest — no error.
   - This rule **supersedes greedy-window starvation**: named slots after
     a join are reserved for, never starved.
8. **Multiple greedies** (across sides): greedies with the **same target
   key** share the window — each receives the same joined words. Greedies
   with **different targets** (e.g. `-I` state vs `-S` remainder): the
   **first** greedy in walk order takes the window; later ones stay
   unfilled (downstream verb validation applies, e.g. a stateless switch
   errors at the log layer).
9. **Excess positionals** (all slots satisfied, no greedy): **error**,
   unless a sink exists (`-l`, plain `-R`, add-verb semantics). Same-named
   `-R` buckets on multiple sides all receive the extras; differing
   buckets → the **first** side's bucket takes them all.

## Retired by this spec

- The in-band `U+0001` `TRAILING_MARKER` sentinel (alias/allocate.rs) —
  `,,` is the only boundary/trailing token; expansion emits it, the
  region parser consumes it.
- `--` as a boundary, promote-to-all, or strippable-idempotent token
  (alias/expand.rs "Rule 13" stripping, scope.rs blanket promotion, the
  body-`--` promote semantics). `--` now means exactly its unix thing,
  side- and alias-scoped (rule 4). **Migration**: stored alias bodies
  that used `--` under the old promote semantics must be rewritten to
  `,,` — `datum check` should flag them during the transition.
- Greedy windows claiming all remaining positionals unconditionally —
  superseded by reservation (rule 7).
- Excess positionals silently landing in the last side's extra_data —
  superseded by rule 9.

## Open / deferred

- da-i7ws (single-side `adti extra 15` joins "extra"): resolved by this
  spec — single-side bodies get their `,,` boundary like everyone else;
  fold into the rework.
- Array states under `_` ordinals and `following_state` consultation
  remain deferred (unrelated to this spec; tracked in da-a0n3).
