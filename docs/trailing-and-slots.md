# Trailing arguments & slot allocation — spec v2.1

Status: **ratified** (design session, vividn + Vremya, 2026-06-04 ~10:20–11:40
Berlin; boundary token amended `--` → `,,` and unix `--` restored, same day
~13:49; paren groups added per vividn's proposal ~17:58, ratified ~18:05 —
dissolving the v2 emitted-vs-typed scope split into one lexical rule).
Supersedes the trailing/allocation behavior merged in #42/#44/#45
(da-vo0y features 1–3, da-6l4x). Companion to `docs/alias-system-design.md`.
The rework beads reference this document; where the merged code disagrees
with this spec, **this spec wins**.

User-facing note: `,,`, `(`, and `)` are mostly machinery (every alias
resolve emits them), but they are *typeable* and must be documented in the
user docs alongside `,` — they are part of the command grammar, not an
implementation detail.

## Grammar

1. **Sides and groups.** `,` separates sides — at any depth; groups do
   not hide commas. Every alias resolution wraps its expansion in an
   explicit **group**:

   ```
   ( body ,, own-trailing )
   ```

   *own-trailing* is the run of plain call-site trailing tokens up to
   the first **structural** token — the next `,`, the next `,,`, an
   enclosing `)`, or a typed `(` (which starts new grouped sides, never
   trailing args); anything past that rides outside the group with its
   own structure intact. Nested resolves nest groups. `(` and `)` are
   **reserved bare tokens, unconditionally structural**: the machinery
   emits them, and users may also type them (quoted past the shell) for
   manual grouping. The parens make the expanded token stream
   self-describing — a human can read the scope right off it.
   `raw_input` stores the literal invocation and `alias_chain` records
   per-step `expands_to` bodies; `datum why` additionally prints the
   assembled wrapped stream on an `expanded:` line (da-g5su),
   reconstructed by replaying `raw_input` against the stored chain.
   A literal bare-paren token is spelled `\(` /
   `\)` (exact-token escape, mirroring `\.`); multi-char tokens that
   merely contain parens (`(x)`, `:-)`) were never structural. `--`
   does **not** demote parens — it governs flag parsing, not grouping
   (rule 4). Unbalanced groups are an error pointing at the escape.
2. **Trailing regions — one rule.** `,,` opens a *trailing region*,
   closed by the next structural token (`,,`, `)`, `(`) or end of side.
   A region binds
   **all sides to its left within its innermost enclosing group** (top
   level counts as the outermost group). This single lexical rule yields
   both ratified v2 behaviors: a typed top-level `,,` binds the whole
   prefix (the `t2 binds {a,b,c,d}` ruling), while an alias-emitted `,,`
   sits inside the alias's own group and binds only the sides that alias
   produced — so `z , oov 30m` puts 30m on oov's two sides, not on z.
   Empty regions are no-ops; they occur constantly (every bare alias
   call produces one). Regions apply left-to-right; later regions see
   earlier fills as satisfied.

   ```
   a , b ,, t1 , c , d ,, t2    # top level: t1 binds {a,b}; t2 binds {a,b,c,d}
   a , ( b , c ,, t ) , d       # manual group: t binds {b,c} only
   oov , switch project gardening
     # oov = "outside , vertical" expands to:
     # ( outside , vertical ,, ) , switch project gardening  (empty region)
   ax y
     # a  = "occur a -k keyX -k keyY";  ax = "a x" — expands to:
     # ( ( occur a -k keyX -k keyY ,, x ) ,, y )  (x→keyX; keyX satisfied, y→keyY)
   z , oov 30m
     # the v2 corner that forced the typed/emitted split: oov's group
     # keeps 30m off z even when z is dur-bearing:
     # z , ( outside , vertical ,, 30m )
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
   slot; they do not blanket). `--` does **not** demote parens — `(`/`)`
   stay structural everywhere (rule 1), so a manual group can still
   close after a `--` (`( a , b ,, -- --verbose )` works). Literal
   bare parens are spelled `\(` / `\)`.

   ```
   note cmd -- --verbose --dry-run        # stores two dash-positionals
   note cmd -- --verbose , occur coffee -q  # side 2's -q is still a flag
   occur f -k expr ,, \(                  # expr="(" — escaped literal
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

- The v2 **typed-vs-emitted scope split** (two-stage region application,
  deferred prefix counting) — superseded by lexical groups (v2.1 rules
  1–2): one structural parse, one binding rule. The emitted parens carry
  the scope; provenance no longer matters.
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
