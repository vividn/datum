//! Trailing-region allocation — spec v2.1 (`docs/trailing-and-slots.md`).
//!
//! The grammar in one breath: `,` separates sides (at any depth); every
//! alias resolve wraps its expansion in a group, `( body ,, trailing )`;
//! a `,,` opens a *trailing region* bound to **all sides to its left
//! within its innermost enclosing group**, closed by the next structural
//! token (`,,`, `)`, `(`) or end of side. Within a region, flags and
//! `key=value` blanket to every bound side while bare positionals walk
//! the bound sides' unfilled slot list. `--` is plain unix end-of-flags,
//! scoped to the side or region it appears in; parens stay structural
//! even after `--` — literal paren data is spelled `\(` / `\)`.
//!
//! Slot model (spec rules 5–9):
//!
//! * per-side array in verb-grammar order — `-I` puts the greedy at the
//!   front (`[field?, state(greedy), keys…, dur?]`), `-S`/`-R` joins put
//!   it after the named keys (`[field?, keys…, remainder(greedy),
//!   dur?]`), plain verbs get `[field?, state?, keys…, dur?]`
//!   (`field?` for verb-only bodies like `note` → `add`);
//! * cross-side unification: `-k` slots merge **by name**, state and dur
//!   slots merge **by kind**, fields stay strictly per-side; a
//!   unified slot fills once and propagates to every declaring side;
//!   satisfied slots are skipped;
//! * greedy consumption reserves the *last n* tokens for the unfilled
//!   non-greedy slots after it (dur type-gated: a non-duration candidate
//!   flows back into the greedy);
//! * same-target greedies share the window, differing targets → first in
//!   walk order wins;
//! * excess positionals error unless a sink (`-l`, plain `-R`, `add`)
//!   exists — same-named `-R` buckets all absorb, differing buckets feed
//!   the first.

use crate::error::{ParseError, Result};
use crate::time::parse_duration_str::parse_duration;

/// The boundary / trailing-region token. Typeable user grammar, but
/// mostly emitted by the alias expander at every resolve.
pub const BOUNDARY: &str = ",,";

/// Group delimiters (spec v2.1). Every alias resolve wraps its
/// expansion in `( … )`, making region scope LEXICAL: a `,,` binds the
/// sides to its left within its innermost enclosing group. Users never
/// need to type them, but may (quoted past the shell) for manual
/// grouping. The wrapped stream is self-describing, though no display
/// surface shows it yet (surfacing it in `datum why` is da-g5su).
/// Unconditionally structural as bare tokens — literal paren data is
/// spelled `\(` / `\)` (see [`unescape_paren`]).
pub const GROUP_OPEN: &str = "(";
pub const GROUP_CLOSE: &str = ")";

/// Flags (at the token level) that consume the next token as a value.
/// Used to keep flag values out of the positional walk. Mirrors the
/// log-command surface; unknown `--long=value` forms are self-contained
/// and unknown bare flags are treated as switches.
const VALUE_FLAGS: &[&str] = &[
    "-k",
    "--key",
    "-K",
    "--slug-key",
    "-R",
    "--remainder",
    "-c",
    "--comment",
    "-d",
    "--date",
    "-t",
    "--time",
    "-z",
    "--timezone",
    "--dur",
    "--id",
    "--id-delimiter",
    "--alias",
    "--conflict",
    "-o",
    "--output",
];

/// Duration-bearing verbs — these get a trailing `dur=` slot at the
/// command layer (da-ooe2), so the allocator reserves a dur position
/// for them.
const DUR_VERBS: &[&str] = &["switch", "start", "stop", "end"];

fn is_value_flag(tok: &str) -> bool {
    VALUE_FLAGS.contains(&tok)
}

fn is_flag(tok: &str) -> bool {
    tok.len() > 1 && tok.starts_with('-') && !tok.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
}

/// What a greedy slot joins into. Same-target greedies share their
/// window (spec rule 8); differing targets → first wins.
#[derive(Debug, Clone, PartialEq, Eq)]
enum GreedyTarget {
    /// `-I` — the joined words become the switch state.
    State,
    /// `-S` with `-R <key>` — joined string under `key`. A plain `-S`
    /// (no `-R`) joins under the default remainder key.
    Remainder(Option<String>),
    /// `-C` — joined string into the comment.
    Comment,
    /// Sink absorbers (`-l`, plain `-R` array, `add` primary value) and
    /// non-log sides. Behaves like a greedy at the very end of the
    /// side's slots; unifies by the `-R` key when one exists.
    Sink(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SlotKind {
    /// The side's missing FIELD — a verb-only body (`note` → `add`)
    /// leaves its field to the call site; the very first region
    /// positional must land right after the verb. Strictly per-side
    /// (the side index is part of the identity; fields never share).
    Field { side: usize },
    /// `-k name` / `-K name` — filled as `name=value`, shared by name.
    Key { name: String },
    /// Verb-derived unbound switch state — filled as a bare positional.
    /// Shared by kind across sides (spec rule 6).
    State,
    /// Verb-derived duration — filled as a bare positional the side's
    /// injected `dur=` slot digests. Shared by kind. Type-gated when
    /// reserved behind a greedy.
    Dur,
    /// Unsatisfied string join / sink window.
    Greedy(GreedyTarget),
}

/// One declaring side of a slot, with its own satisfaction. Per-side
/// tracking matters when the prefix GROWS: a side joining a unified
/// slot after an earlier region already filled it must stay fillable
/// (Copilot review on PR #47) — the earlier sides keep their fills,
/// the newcomer's declaration re-opens the slot for the walk.
#[derive(Debug, Clone)]
struct SideRef {
    side: usize,
    satisfied: bool,
}

#[derive(Debug, Clone)]
struct Slot {
    kind: SlotKind,
    /// Declaring sides (multiple after unification), each with its own
    /// satisfaction state.
    sides: Vec<SideRef>,
}

impl Slot {
    fn is_greedy(&self) -> bool {
        matches!(self.kind, SlotKind::Greedy(_))
    }
    fn all_satisfied(&self) -> bool {
        self.sides.iter().all(|s| s.satisfied)
    }
    /// Side ids still awaiting a fill.
    fn open_sides(&self) -> Vec<usize> {
        self.sides
            .iter()
            .filter(|s| !s.satisfied)
            .map(|s| s.side)
            .collect()
    }
    fn mark_all_satisfied(&mut self) {
        for s in self.sides.iter_mut() {
            s.satisfied = true;
        }
    }
    fn single(kind: SlotKind, side: usize, satisfied: bool) -> Self {
        Slot {
            kind,
            sides: vec![SideRef { side, satisfied }],
        }
    }
}

/// One trailing region, located structurally: it binds sides
/// `start_side..=end_side` — everything to its left within its
/// innermost enclosing group (spec v2.1 rule 2).
#[derive(Debug, Clone)]
struct Region {
    start_side: usize,
    end_side: usize,
    tokens: Vec<String>,
}

/// `\(` / `\)` escape a literal paren token (exact-token, mirroring the
/// `\.` walk escape). Unescaped here as tokens route to locals/region
/// content, so downstream layers only ever see plain data parens —
/// after this parse no bare paren carries structure.
fn unescape_paren(tok: &str) -> &str {
    match tok {
        "\\(" => GROUP_OPEN,
        "\\)" => GROUP_CLOSE,
        _ => tok,
    }
}

/// Structural parse of the whole side sequence: strip `(`/`)`/`,,`
/// into group scopes and located regions; everything else is a side's
/// local token. Groups may span sides (commas split sides regardless
/// of depth); a region's content is intra-side, ending at the next
/// `,,`, `)`, `(`, or end of side. Unbalanced groups error.
///
/// Parens are structural UNCONDITIONALLY — `--` governs flag parsing,
/// not grouping (a `--` latch can't demote them without re-importing
/// the provenance problem the parens exist to solve: a typed `-- )` and
/// the emitted wrapper's `)` are the same bare token). Literal paren
/// data uses `\(` / `\)` instead (Copilot review on PR #49).
fn parse_structure(sides: Vec<Vec<String>>) -> Result<(Vec<Vec<String>>, Vec<Region>)> {
    let n = sides.len();
    let mut locals: Vec<Vec<String>> = vec![Vec::new(); n];
    let mut regions: Vec<Region> = Vec::new();
    let mut stack: Vec<usize> = Vec::new(); // open groups' start sides
    for (i, side) in sides.into_iter().enumerate() {
        let mut j = 0usize;
        while j < side.len() {
            let tok = &side[j];
            if tok == GROUP_OPEN {
                stack.push(i);
                j += 1;
                continue;
            }
            if tok == GROUP_CLOSE {
                if stack.pop().is_none() {
                    return Err(ParseError::InvalidArg(
                        "unbalanced ')' in expanded command (use '\\)' for a literal paren)"
                            .to_string(),
                    ));
                }
                j += 1;
                continue;
            }
            if tok == BOUNDARY {
                let start_side = stack.last().copied().unwrap_or(0);
                j += 1;
                let mut tokens: Vec<String> = Vec::new();
                while j < side.len()
                    && side[j] != BOUNDARY
                    && side[j] != GROUP_CLOSE
                    && side[j] != GROUP_OPEN
                {
                    tokens.push(unescape_paren(&side[j]).to_string());
                    j += 1;
                }
                // Empty regions are no-ops (every bare alias call
                // produces one).
                if !tokens.is_empty() {
                    regions.push(Region {
                        start_side,
                        end_side: i,
                        tokens,
                    });
                }
                continue;
            }
            locals[i].push(unescape_paren(tok).to_string());
            j += 1;
        }
    }
    if !stack.is_empty() {
        return Err(ParseError::InvalidArg(
            "unbalanced '(' in expanded command (use '\\(' for a literal paren)".to_string(),
        ));
    }
    Ok((locals, regions))
}

/// One side's one slot declaration — the global, range-independent
/// store. Per-region walks build merged views over the atoms in their
/// binding range; satisfaction lives here so it persists across
/// regions with per-side precision.
#[derive(Debug, Clone)]
struct Atom {
    kind: SlotKind,
    side: usize,
    satisfied: bool,
}

/// Flatten every side's declarations into atoms.
fn atoms_for(locals: &[Vec<String>]) -> Vec<Atom> {
    let mut out: Vec<Atom> = Vec::new();
    for (i, side) in locals.iter().enumerate() {
        for slot in side_slots(i, side) {
            for sr in slot.sides {
                out.push(Atom {
                    kind: slot.kind.clone(),
                    side: sr.side,
                    satisfied: sr.satisfied,
                });
            }
        }
    }
    out
}

/// Build the merged walk view over the atoms whose side lies in
/// `lo..=hi`: `-k` merge by name, state/dur by kind, greedies by
/// target, fields never (spec rules 6 + 8). Returns the view plus, per
/// view slot, the atom indices backing each of its sides (for
/// satisfaction write-back).
fn build_view(atoms: &[Atom], lo: usize, hi: usize) -> (Vec<Slot>, Vec<Vec<usize>>) {
    let mut view: Vec<Slot> = Vec::new();
    let mut backing: Vec<Vec<usize>> = Vec::new();
    for (ai, atom) in atoms.iter().enumerate() {
        if atom.side < lo || atom.side > hi {
            continue;
        }
        let merged = if matches!(atom.kind, SlotKind::Field { .. }) {
            None
        } else {
            view.iter()
                .position(|s| slot_identity_eq(&s.kind, &atom.kind))
        };
        match merged {
            Some(vi) => {
                view[vi].sides.push(SideRef {
                    side: atom.side,
                    satisfied: atom.satisfied,
                });
                backing[vi].push(ai);
            }
            None => {
                view.push(Slot::single(atom.kind.clone(), atom.side, atom.satisfied));
                backing.push(vec![ai]);
            }
        }
    }
    (view, backing)
}

/// Extract a side's declared slots from its local tokens, in spec
/// rule-5 order, with satisfaction derived from the body (positional
/// state, explicit `key=` assignments, body-fed joins).
fn side_slots(side_idx: usize, side: &[String]) -> Vec<Slot> {
    // Identify the verb, tolerating field-first order.
    let verb: Option<&str> = match (side.first(), side.get(1)) {
        (Some(t0), _) if DUR_VERBS.contains(&t0.as_str()) || t0 == "occur" || t0 == "add" => {
            Some(t0.as_str())
        }
        (_, Some(t1)) if DUR_VERBS.contains(&t1.as_str()) || t1 == "occur" || t1 == "add" => {
            Some(t1.as_str())
        }
        _ => None,
    };

    let mut keys: Vec<String> = Vec::new();
    let mut explicit_kv: Vec<String> = Vec::new(); // body `key=value` names
    let mut join: Option<GreedyTarget> = None;
    let mut lenient = false;
    let mut plain_r_key: Option<Option<String>> = None; // Some(key) when -R seen w/o join
    let mut has_dur_kv = false;
    let mut body_positionals = 0usize;
    let mut seen_field = false;
    let mut seen_verb = false;
    let mut no_more_flags = false;
    let mut pending_r: Option<String> = None;
    let mut has_s = false;
    let mut has_c = false;
    let mut i = 0;
    while i < side.len() {
        let tok = &side[i];
        if !seen_verb && Some(tok.as_str()) == verb {
            seen_verb = true;
            i += 1;
            continue;
        }
        if tok == "--" && !no_more_flags {
            no_more_flags = true;
            i += 1;
            continue;
        }
        if !no_more_flags && is_flag(tok) {
            match tok.as_str() {
                "-I" | "--string-state" => join = Some(GreedyTarget::State),
                "-S" | "--string-remainder" => has_s = true,
                "-C" | "--comment-remainder" => has_c = true,
                "-SR" => {
                    has_s = true;
                    if let Some(v) = side.get(i + 1) {
                        pending_r = Some(v.clone());
                    }
                }
                "-l" | "--lenient" => lenient = true,
                "-R" | "--remainder" => {
                    if let Some(v) = side.get(i + 1) {
                        pending_r = Some(v.clone());
                    }
                }
                _ => {}
            }
            if is_value_flag(tok) || tok == "-SR" {
                if let Some(v) = side.get(i + 1) {
                    if (tok == "-k" || tok == "--key" || tok == "-K" || tok == "--slug-key")
                        && !v.is_empty()
                    {
                        keys.push(v.clone());
                    }
                    i += 1;
                }
            }
            i += 1;
            continue;
        }
        if tok.contains('=') {
            if let Some((name, _)) = tok.split_once('=') {
                explicit_kv.push(name.to_string());
                if name == "dur" {
                    has_dur_kv = true;
                }
            }
            i += 1;
            continue;
        }
        // Bare token: the first is the field (the verb was skipped above).
        if !seen_field {
            seen_field = true;
        } else {
            body_positionals += 1;
        }
        i += 1;
    }

    // Resolve the join target now that -S/-R/-C combinations are known.
    if join.is_none() {
        if has_s {
            join = Some(GreedyTarget::Remainder(pending_r.clone()));
        } else if has_c {
            join = Some(GreedyTarget::Comment);
        }
    }
    if join.is_none() && pending_r.is_some() {
        // Plain `-R key` without a string join: an array-remainder sink.
        plain_r_key = Some(pending_r.clone());
    }

    // A side whose lede isn't a log verb is some other command (`get`,
    // `tail`, …) — the slot model doesn't apply; one big sink keeps the
    // pass-through behavior.
    let Some(verb) = verb else {
        return vec![Slot::single(
            SlotKind::Greedy(GreedyTarget::Sink(None)),
            side_idx,
            false,
        )];
    };

    let is_switch = verb == "switch";
    let dur_bearing = DUR_VERBS.contains(&verb);
    let join_unsatisfied = join.is_some() && body_positionals == 0;
    let needs_field = !seen_field;

    let key_slots: Vec<Slot> = keys
        .iter()
        .map(|spec| {
            let (name, has_default) = match spec.split_once('=') {
                Some((n, _)) => (n.to_string(), false), // default still overridable
                None => (spec.clone(), false),
            };
            let _ = has_default;
            Slot::single(
                SlotKind::Key { name: name.clone() },
                side_idx,
                explicit_kv.iter().any(|k| k == &name),
            )
        })
        .collect();

    let mut slots: Vec<Slot> = Vec::new();
    if needs_field {
        slots.push(Slot::single(
            SlotKind::Field { side: side_idx },
            side_idx,
            false,
        ));
    }

    // Spec rule 5: -I puts the greedy first; -S/-R joins go after keys.
    if join_unsatisfied && join == Some(GreedyTarget::State) {
        slots.push(Slot::single(
            SlotKind::Greedy(GreedyTarget::State),
            side_idx,
            false,
        ));
        slots.extend(key_slots);
    } else {
        // Plain switch with an unbound state gets a state slot (only
        // when no join is in play — a satisfied -I bound the state in
        // the body; an unsatisfied -S join doesn't bind state but the
        // -I-less switch state position stays first).
        if is_switch && body_positionals == 0 && join.is_none() {
            slots.push(Slot::single(SlotKind::State, side_idx, false));
        }
        slots.extend(key_slots);
        if join_unsatisfied {
            if let Some(target) = join.clone() {
                if target != GreedyTarget::State {
                    slots.push(Slot::single(SlotKind::Greedy(target), side_idx, false));
                }
            }
        }
    }

    if dur_bearing {
        slots.push(Slot::single(SlotKind::Dur, side_idx, has_dur_kv));
    }

    // Sinks at the very end: `-l`, plain `-R key`, and `add`'s
    // primary-value remainder absorb any leftovers (spec rule 9).
    if lenient {
        slots.push(Slot::single(
            SlotKind::Greedy(GreedyTarget::Sink(None)),
            side_idx,
            false,
        ));
    }
    if let Some(rkey) = plain_r_key {
        slots.push(Slot::single(
            SlotKind::Greedy(GreedyTarget::Sink(rkey)),
            side_idx,
            false,
        ));
    }
    if verb == "add" {
        slots.push(Slot::single(
            SlotKind::Greedy(GreedyTarget::Sink(None)),
            side_idx,
            false,
        ));
    }
    slots
}

/// Classified region content.
struct RegionContent {
    positionals: Vec<String>,
    /// Flag groups (flag + its value when it takes one) to blanket.
    flags: Vec<String>,
    /// `key=value` tokens to blanket (also satisfy matching key slots).
    kv: Vec<String>,
}

/// Split a region's tokens into positionals / flags / `key=value`,
/// honoring a region-scoped unix `--` (everything after it is
/// positional, even dash-prefixed — spec rule 4).
fn classify_region(tokens: &[String]) -> RegionContent {
    let mut positionals: Vec<String> = Vec::new();
    let mut flags: Vec<String> = Vec::new();
    let mut kv: Vec<String> = Vec::new();
    let mut no_more_flags = false;
    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        if tok == "--" && !no_more_flags {
            no_more_flags = true;
            i += 1;
            continue;
        }
        if !no_more_flags && is_flag(tok) {
            flags.push(tok.clone());
            if is_value_flag(tok) {
                if let Some(v) = tokens.get(i + 1) {
                    flags.push(v.clone());
                    i += 1;
                }
            }
        } else if tok.contains('=') {
            // `key=value` is datum's data grammar, not flag grammar — a
            // unix `--` de-flags but does not de-key (rule 4 note).
            kv.push(tok.clone());
        } else {
            positionals.push(tok.clone());
        }
        i += 1;
    }
    RegionContent {
        positionals,
        flags,
        kv,
    }
}

/// A single fill produced by the walk.
#[derive(Debug)]
enum Fill {
    Field {
        side: usize,
        value: String,
    },
    Key {
        sides: Vec<usize>,
        kv: String,
    },
    State {
        sides: Vec<usize>,
        value: String,
    },
    Dur {
        sides: Vec<usize>,
        value: String,
    },
    Window {
        sides: Vec<usize>,
        tokens: Vec<String>,
    },
}

/// Walk one region's positionals across the prefix's unified slots.
/// Implements reservation (rule 7), greedy sharing / first-wins (rule
/// 8), `.` skips, and the excess rules (rule 9). Returns the fills and
/// marks consumed slots satisfied in `slots`.
fn walk_region(positionals: Vec<String>, slots: &mut [Slot]) -> Result<Vec<Fill>> {
    let mut fills: Vec<Fill> = Vec::new();
    if positionals.is_empty() {
        return Ok(fills);
    }

    let unesc = |v: &str| -> String {
        // `\.` escapes a literal dot (only as the whole token).
        if v == "\\." {
            ".".to_string()
        } else {
            v.to_string()
        }
    };

    // Find the first unfilled greedy (walk order). Later greedies with
    // the same target were already unified into it; later greedies with
    // different targets stay unfilled (rule 8).
    let greedy_pos = slots
        .iter()
        .position(|s| !s.all_satisfied() && s.is_greedy());

    let mut queue = positionals;

    if let Some(g) = greedy_pos {
        // Fill the slots BEFORE the greedy front-to-back first.
        let mut qi = 0usize;
        for slot in slots[..g].iter_mut() {
            if slot.all_satisfied() || qi >= queue.len() {
                continue;
            }
            let value = queue[qi].clone();
            qi += 1;
            let open = slot.open_sides();
            slot.mark_all_satisfied();
            if value == "." {
                continue; // explicit skip
            }
            push_fill(&mut fills, &slot.kind, open, unesc(&value));
        }
        let mut rest: Vec<String> = queue.split_off(qi);
        // Reservation: peel from the END for the unfilled non-greedy
        // slots after the greedy, in reverse slot order. Dur candidates
        // are type-gated — a non-duration flows back into the window.
        let after: Vec<usize> = (g + 1..slots.len())
            .filter(|&i| !slots[i].all_satisfied() && !slots[i].is_greedy())
            .collect();
        let mut reserved: Vec<(usize, String)> = Vec::new(); // (slot, value)
        for &si in after.iter().rev() {
            if rest.is_empty() {
                break;
            }
            let candidate = rest.last().cloned().unwrap_or_default();
            let is_dur_slot = matches!(slots[si].kind, SlotKind::Dur);
            if is_dur_slot {
                // Type gate (rule 7): only a parseable duration — or the
                // explicit `.` skip — peels into the dur slot. `\.` is
                // the escaped literal dot and belongs to the window.
                let parses = candidate == "." || parse_duration(&candidate).is_ok();
                if !parses {
                    continue; // flows back into the window; dur stays unfilled
                }
            }
            rest.pop();
            reserved.push((si, candidate));
        }
        // Window emptiness: a walk that leaves a string JOIN nothing is
        // an error (rule 7) — but a sink with nothing to absorb is just
        // a sink at rest (it stays open for later regions).
        if rest.is_empty() {
            if matches!(&slots[g].kind, SlotKind::Greedy(GreedyTarget::Sink(_))) {
                return Ok(fills);
            }
            return Err(ParseError::InvalidArg(
                "nothing left for the string join after reserving for the remaining slots"
                    .to_string(),
            ));
        }
        // Apply reservations (they were peeled in reverse slot order).
        for (si, value) in reserved.into_iter().rev() {
            let open = slots[si].open_sides();
            slots[si].mark_all_satisfied();
            if value == "." {
                continue;
            }
            let v = unesc(&value);
            let kind = slots[si].kind.clone();
            push_fill(&mut fills, &kind, open, v);
        }
        // The window feeds the greedy's still-open declaring sides.
        let open = slots[g].open_sides();
        slots[g].mark_all_satisfied();
        fills.push(Fill::Window {
            sides: open,
            tokens: rest.iter().map(|t| unesc(t)).collect(),
        });
        return Ok(fills);
    }

    // No greedy: straight front-to-back walk.
    let mut qi = 0usize;
    for slot in slots.iter_mut() {
        if qi >= queue.len() {
            break;
        }
        if slot.all_satisfied() {
            continue;
        }
        let value = queue[qi].clone();
        qi += 1;
        let open = slot.open_sides();
        slot.mark_all_satisfied();
        if value == "." {
            continue;
        }
        push_fill(&mut fills, &slot.kind, open, unesc(&value));
    }
    if qi < queue.len() {
        return Err(ParseError::ExtraData(queue[qi..].to_vec()));
    }
    Ok(fills)
}

fn push_fill(fills: &mut Vec<Fill>, kind: &SlotKind, open_sides: Vec<usize>, value: String) {
    match kind {
        SlotKind::Field { side } => fills.push(Fill::Field { side: *side, value }),
        SlotKind::Key { name } => fills.push(Fill::Key {
            sides: open_sides,
            kv: format!("{name}={value}"),
        }),
        SlotKind::State => fills.push(Fill::State {
            sides: open_sides,
            value,
        }),
        SlotKind::Dur => fills.push(Fill::Dur {
            sides: open_sides,
            value,
        }),
        SlotKind::Greedy(_) => fills.push(Fill::Window {
            sides: open_sides,
            tokens: vec![value],
        }),
    }
}

/// Apply every trailing region over its lexical scope and emit the
/// final flat sides (spec v2.1). The entry point `expand_into_sides`
/// calls this once, after full expansion + stacked-flag normalization;
/// groups (`(`/`)`) and boundaries (`,,`) are consumed here.
pub(super) fn apply_regions(sides: Vec<Vec<String>>) -> Result<Vec<Vec<String>>> {
    let (locals, regions) = parse_structure(sides)?;
    if regions.is_empty() {
        return Ok(locals);
    }

    // The atom store spans the whole command; satisfaction persists
    // across regions per (slot, side), so a later region sees earlier
    // fills as satisfied while a side outside an earlier region's
    // scope stays fillable.
    let mut atoms = atoms_for(&locals);

    // Per-side emissions, applied at the end. The side parser's grammar
    // wants state before dur (`switch f STATE DUR`) regardless of slot
    // walk order, so positional fills bucket by kind.
    let n = locals.len();
    let mut field_fills: Vec<Option<String>> = vec![None; n];
    let mut blanket_flags: Vec<Vec<String>> = vec![Vec::new(); n];
    let mut blanket_kv: Vec<Vec<String>> = vec![Vec::new(); n];
    let mut key_fills: Vec<Vec<String>> = vec![Vec::new(); n];
    let mut state_fills: Vec<Vec<String>> = vec![Vec::new(); n];
    let mut dur_fills: Vec<Vec<String>> = vec![Vec::new(); n];
    let mut window_fills: Vec<Vec<String>> = vec![Vec::new(); n];

    for region in &regions {
        let (lo, hi) = (region.start_side, region.end_side);
        let content = classify_region(&region.tokens);
        // Blanket flags + k=v to every side in scope.
        for s in lo..=hi {
            blanket_flags[s].extend(content.flags.iter().cloned());
            blanket_kv[s].extend(content.kv.iter().cloned());
        }
        // Explicit k=v satisfies matching slots in scope (rule 6).
        for kv in &content.kv {
            if let Some((name, _)) = kv.split_once('=') {
                for atom in atoms.iter_mut().filter(|a| a.side >= lo && a.side <= hi) {
                    if matches!(&atom.kind, SlotKind::Key { name: n } if n == name) {
                        atom.satisfied = true;
                    }
                    if name == "dur" && matches!(atom.kind, SlotKind::Dur) {
                        atom.satisfied = true;
                    }
                }
            }
        }
        // Walk the positionals over the scope's merged view, then
        // write satisfaction back to the atoms.
        let (mut view, backing) = build_view(&atoms, lo, hi);
        let fills = walk_region(content.positionals.clone(), &mut view)?;
        for (vi, slot) in view.iter().enumerate() {
            for (si, sr) in slot.sides.iter().enumerate() {
                if sr.satisfied {
                    atoms[backing[vi][si]].satisfied = true;
                }
            }
        }
        for fill in fills {
            match fill {
                Fill::Field { side, value } => {
                    field_fills[side] = Some(value);
                }
                Fill::Key { sides, kv } => {
                    for s in sides {
                        key_fills[s].push(kv.clone());
                    }
                }
                Fill::State { sides, value } => {
                    for s in sides {
                        state_fills[s].push(value.clone());
                    }
                }
                Fill::Dur { sides, value } => {
                    for s in sides {
                        dur_fills[s].push(value.clone());
                    }
                }
                Fill::Window { sides, tokens } => {
                    for s in sides {
                        window_fills[s].extend(tokens.iter().cloned());
                    }
                }
            }
        }
    }

    // Emit: locals, then blanket flags (BEFORE any local `--` so they
    // stay flags), then blanket k=v + positional fills (positional-class,
    // protected by a `--` when a fill is dash-leading and the side has
    // none yet).
    let mut out: Vec<Vec<String>> = Vec::with_capacity(n);
    for (i, mut local) in locals.into_iter().enumerate() {
        if let Some(field) = field_fills[i].take() {
            // The field belongs immediately after the verb (`add` +
            // `word`), ahead of any flags or k=v the body carried.
            let at = 1.min(local.len());
            local.insert(at, field);
        }
        let mut side: Vec<String> = Vec::with_capacity(
            local.len() + blanket_flags[i].len() + blanket_kv[i].len() + key_fills[i].len() + 4,
        );
        match local.iter().position(|t| t == "--") {
            Some(dd) => {
                side.extend(local[..dd].iter().cloned());
                side.extend(blanket_flags[i].iter().cloned());
                side.extend(local[dd..].iter().cloned());
            }
            None => {
                side.extend(local);
                side.extend(blanket_flags[i].iter().cloned());
            }
        }
        side.extend(blanket_kv[i].iter().cloned());
        side.extend(key_fills[i].iter().cloned());
        let dash_fill = state_fills[i]
            .iter()
            .chain(dur_fills[i].iter())
            .chain(window_fills[i].iter())
            .any(|t| is_flag(t));
        if dash_fill && !side.iter().any(|t| t == "--") {
            side.push("--".to_string());
        }
        side.extend(state_fills[i].iter().cloned());
        side.extend(dur_fills[i].iter().cloned());
        side.extend(window_fills[i].iter().cloned());
        out.push(side);
    }
    Ok(out)
}

fn slot_identity_eq(a: &SlotKind, b: &SlotKind) -> bool {
    match (a, b) {
        (SlotKind::Field { side: x }, SlotKind::Field { side: y }) => x == y,
        (SlotKind::Key { name: x }, SlotKind::Key { name: y }) => x == y,
        (SlotKind::State, SlotKind::State) => true,
        (SlotKind::Dur, SlotKind::Dur) => true,
        (SlotKind::Greedy(x), SlotKind::Greedy(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(toks: &[&str]) -> Vec<String> {
        toks.iter().map(|s| s.to_string()).collect()
    }

    fn apply(sides: &[&[&str]]) -> Result<Vec<Vec<String>>> {
        apply_regions(sides.iter().map(|s| v(s)).collect())
    }

    fn has_seq(side: &[String], seq: &[&str]) -> bool {
        side.windows(seq.len())
            .any(|w| w.iter().zip(seq.iter()).all(|(a, b)| a == b))
    }

    // ── spec rule 2 examples ─────────────────────────────────────────

    #[test]
    fn empty_region_is_noop() {
        // `oov` alone: outside , vertical ,,
        let out = apply(&[&["occur", "outside", ",,"], &["switch", "vertical"]]).unwrap();
        assert_eq!(out[0], v(&["occur", "outside"]));
        assert_eq!(out[1], v(&["switch", "vertical"]));
    }

    #[test]
    fn chained_regions_fill_in_order() {
        // ax y → occur a -k keyX -k keyY ,, x ,, y
        let out = apply(&[&[
            "occur", "a", "-k", "keyX", "-k", "keyY", ",,", "x", ",,", "y",
        ]])
        .unwrap();
        assert!(has_seq(&out[0], &["keyX=x"]));
        assert!(has_seq(&out[0], &["keyY=y"]));
    }

    #[test]
    fn cumulative_prefix_binds_all_earlier_sides() {
        // a , b ,, k=v , c  → k=v blankets {a,b} only; c untouched.
        let out = apply(&[
            &["occur", "a"],
            &["occur", "b", ",,", "x=1"],
            &["occur", "c"],
        ])
        .unwrap();
        assert!(has_seq(&out[0], &["x=1"]));
        assert!(has_seq(&out[1], &["x=1"]));
        assert!(!has_seq(&out[2], &["x=1"]));
    }

    // ── spec rule 6: the ratified k1–k6 example ──────────────────────

    #[test]
    fn ratified_key_walk_example() {
        let out = apply(&[
            &["occur", "f1", "-k", "key1", "-k", "key2", "-k", "key3"],
            &[
                "start",
                "f2",
                "-k",
                "key2",
                "-k",
                "key4",
                "key5=val5",
                ",,",
                "key6=val6",
                "val1",
                "val2",
                "val3",
                "val4",
                "30m",
            ],
        ])
        .unwrap();
        let f1 = &out[0];
        let f2 = &out[1];
        assert!(has_seq(f1, &["key1=val1"]));
        assert!(has_seq(f1, &["key2=val2"]));
        assert!(has_seq(f1, &["key3=val3"]));
        assert!(has_seq(f1, &["key6=val6"]));
        assert!(!f1.iter().any(|t| t == "30m"), "f1 is occur — no dur slot");
        assert!(has_seq(f2, &["key2=val2"]));
        assert!(has_seq(f2, &["key4=val4"]));
        assert!(has_seq(f2, &["key5=val5"]));
        assert!(has_seq(f2, &["key6=val6"]));
        assert!(f2.iter().any(|t| t == "30m"), "f2 takes the dur");
    }

    #[test]
    fn kind_shared_dur_fills_all_dur_bearing_sides() {
        // start run , start bike ,, 30m → both get 30m
        let out = apply(&[&["start", "run"], &["start", "bike", ",,", "30m"]]).unwrap();
        assert!(out[0].iter().any(|t| t == "30m"));
        assert!(out[1].iter().any(|t| t == "30m"));
    }

    #[test]
    fn kind_shared_state_fills_all_unbound_switches() {
        let out = apply(&[&["switch", "mood"], &["switch", "outfit", ",,", "blue"]]).unwrap();
        assert!(out[0].iter().any(|t| t == "blue"));
        assert!(out[1].iter().any(|t| t == "blue"));
    }

    #[test]
    fn excess_after_shared_dur_errors() {
        // vx 5m sit 10m under v2: dur kind-shares, sit fills state, 10m is excess.
        let err = apply(&[
            &["switch", "v", "vertical"],
            &["switch", "x", ",,", "5m", "sit", "10m"],
        ])
        .unwrap_err();
        assert!(matches!(err, ParseError::ExtraData(_)));
    }

    #[test]
    fn satisfied_key_slot_skips_walk() {
        // Explicit k=v in the region satisfies the slot; positional walks past.
        let out = apply(&[&[
            "occur",
            "f",
            "-k",
            "a",
            "-k",
            "b",
            ",,",
            "a=explicit",
            "valb",
        ]])
        .unwrap();
        assert!(has_seq(&out[0], &["a=explicit"]));
        assert!(has_seq(&out[0], &["b=valb"]));
    }

    #[test]
    fn body_kv_satisfies_slot() {
        let out = apply(&[&["occur", "f", "-k", "a", "a=body", ",,", "valb", "-k", "b"]]);
        // -k b arrives via the region as a blanket flag — declared too
        // late for this walk; valb goes to... nothing unfilled → error.
        // (Keys declared in regions are flags, not slots — spec keeps
        // slot declarations in side bodies.)
        assert!(out.is_err());
    }

    // ── spec rule 7: greedy reservation ──────────────────────────────

    #[test]
    fn i_join_reserves_tail_for_keys_and_dur() {
        let out = apply(&[&[
            "switch", "f", "-I", "-k", "key1", ",,", "some", "words", "here", "val1", "30m",
        ]])
        .unwrap();
        let side = &out[0];
        assert!(has_seq(side, &["key1=val1"]));
        assert!(side.iter().any(|t| t == "30m"));
        // The window lands as bare tokens for the side's own -I join.
        assert!(has_seq(side, &["some", "words", "here"]));
        assert!(!has_seq(side, &["some", "words", "here", "val1"]));
    }

    #[test]
    fn dur_type_gate_flows_nondur_back_into_window() {
        let out = apply(&[&["switch", "f", "-I", ",,", "waiting", "for", "godot"]]).unwrap();
        let side = &out[0];
        assert!(has_seq(side, &["waiting", "for", "godot"]));
    }

    #[test]
    fn dur_peels_when_last_token_parses() {
        let out = apply(&[&["switch", "f", "-I", ",,", "deep", "conversation", "30m"]]).unwrap();
        let side = &out[0];
        assert!(has_seq(side, &["deep", "conversation"]));
        assert!(side.iter().any(|t| t == "30m"));
        assert!(!has_seq(side, &["conversation", "30m"]));
    }

    #[test]
    fn dot_skips_dur_reservation() {
        let out = apply(&[&["switch", "f", "-I", ",,", "waiting", "room", "."]]).unwrap();
        let side = &out[0];
        assert!(has_seq(side, &["waiting", "room"]));
        assert!(!side.iter().any(|t| t == "."));
    }

    #[test]
    fn escaped_dot_joins_window_literally() {
        let out = apply(&[&["switch", "f", "-I", ",,", "ends", "with", "\\."]]).unwrap();
        let side = &out[0];
        assert!(has_seq(side, &["ends", "with", "."]));
    }

    #[test]
    fn empty_window_after_reservation_errors() {
        let err = apply(&[&["switch", "f", "-I", "-k", "key1", ",,", "val1"]]).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("nothing left"), "got: {msg}");
    }

    #[test]
    fn sr_join_sits_after_keys() {
        // -S -R notes: keys fill first (front), join eats the middle, dur last.
        let out = apply(&[&[
            "start", "f", "-S", "-R", "notes", "-k", "key1", ",,", "v1", "long", "note", "text",
            "15m",
        ]])
        .unwrap();
        let side = &out[0];
        assert!(has_seq(side, &["key1=v1"]));
        assert!(side.iter().any(|t| t == "15m"));
        assert!(has_seq(side, &["long", "note", "text"]));
    }

    // ── spec rule 8: multiple greedies ───────────────────────────────

    #[test]
    fn same_target_greedies_share_window() {
        let out = apply(&[
            &["switch", "tv", "-I"],
            &["switch", "movie", "-I", ",,", "blueberry", "muffins"],
        ])
        .unwrap();
        assert!(has_seq(&out[0], &["blueberry", "muffins"]));
        assert!(has_seq(&out[1], &["blueberry", "muffins"]));
    }

    #[test]
    fn differing_target_first_greedy_wins() {
        let out = apply(&[
            &["switch", "a", "-I"],
            &["start", "b", "-S", ",,", "some", "words"],
        ])
        .unwrap();
        assert!(has_seq(&out[0], &["some", "words"]));
        assert!(!has_seq(&out[1], &["some", "words"]));
    }

    // ── spec rule 9: excess + sinks ──────────────────────────────────

    #[test]
    fn excess_without_sink_errors() {
        // Keyed occur exposes the key slot only — a second positional
        // has nowhere to go (rule 9).
        let err = apply(&[&["occur", "f", "-k", "a", ",,", "v1", "stray"]]).unwrap_err();
        assert!(matches!(err, ParseError::ExtraData(ref t) if t == &vec!["stray".to_string()]));
    }

    #[test]
    fn bare_occur_positional_is_excess() {
        // There is NO primary-value slot (vividn, 2026-06-04 ~15:31 —
        // "value is not a thing"; vestigial concept that keeps creeping
        // back). `occur coffee ,, 2` errors exactly like the direct
        // `occur coffee 2` does at the data_args layer.
        let err = apply(&[&["occur", "coffee", ",,", "2"]]).unwrap_err();
        assert!(matches!(err, ParseError::ExtraData(ref t) if t == &vec!["2".to_string()]));
    }
    #[test]
    fn lenient_sink_absorbs_excess() {
        let out = apply(&[&["occur", "f", "-l", ",,", "stray", "bits"]]).unwrap();
        assert!(has_seq(&out[0], &["stray", "bits"]));
    }

    #[test]
    fn same_named_r_buckets_both_absorb() {
        let out = apply(&[
            &["occur", "a", "-R", "notes"],
            &["occur", "b", "-R", "notes", ",,", "x", "y"],
        ])
        .unwrap();
        assert!(has_seq(&out[0], &["x", "y"]));
        assert!(has_seq(&out[1], &["x", "y"]));
    }

    #[test]
    fn differing_r_buckets_first_takes_all() {
        let out = apply(&[
            &["occur", "a", "-R", "left"],
            &["occur", "b", "-R", "right", ",,", "x", "y"],
        ])
        .unwrap();
        assert!(has_seq(&out[0], &["x", "y"]));
        assert!(!has_seq(&out[1], &["x", "y"]));
    }

    // ── spec rule 4: unix -- inside a region ─────────────────────────

    #[test]
    fn region_dash_dash_makes_flags_positional() {
        let out = apply(&[&["occur", "f", "-k", "cmd", ",,", "--", "--verbose"]]).unwrap();
        assert!(has_seq(&out[0], &["cmd=--verbose"]));
    }

    #[test]
    fn dash_positional_state_fill_gets_guard() {
        let out = apply(&[&["switch", "f", ",,", "--", "-weird"]]).unwrap();
        let side = &out[0];
        let dd = side.iter().position(|t| t == "--").expect("guard emitted");
        let val = side.iter().position(|t| t == "-weird").unwrap();
        assert!(dd < val, "guard must precede the dash positional");
    }

    #[test]
    fn blanket_flags_insert_before_local_dash_dash() {
        let out = apply(&[&["occur", "f", "--", "-data", ",,", "-q"]]).unwrap();
        let side = &out[0];
        let q = side.iter().position(|t| t == "-q").unwrap();
        let dd = side.iter().position(|t| t == "--").unwrap();
        assert!(
            q < dd,
            "blanket flag must stay a flag (before --): {side:?}"
        );
    }

    // ── pass-throughs ────────────────────────────────────────────────

    #[test]
    fn growing_prefix_reopens_kind_slot_for_new_side() {
        // Copilot review on PR #47: side 1 joins AFTER region 1 filled
        // the dur slot for side 0 — its own dur declaration re-opens
        // the unified slot, so region 2's 10m reaches side 1 only.
        let out = apply(&[&["start", "a", ",,", "5m"], &["start", "b", ",,", "10m"]]).unwrap();
        assert!(out[0].iter().any(|t| t == "5m"), "{:?}", out[0]);
        assert!(!out[0].iter().any(|t| t == "10m"), "{:?}", out[0]);
        assert!(out[1].iter().any(|t| t == "10m"), "{:?}", out[1]);
        assert!(!out[1].iter().any(|t| t == "5m"), "{:?}", out[1]);
    }

    // ── spec v2.1: lexical groups ────────────────────────────────────

    #[test]
    fn manual_group_scopes_region_to_its_sides() {
        // Power-user grouping: a , ( b , c ,, t ) , d — t binds {b,c}.
        let out = apply(&[
            &["occur", "a", "-k", "ka"],
            &["(", "occur", "b", "-k", "kb"],
            &["occur", "c", "-k", "kb", ",,", "vv", ")"],
            &["occur", "d", "-k", "ka"],
        ])
        .unwrap();
        assert!(!has_seq(&out[0], &["kb=vv"]), "{:?}", out[0]);
        assert!(has_seq(&out[1], &["kb=vv"]));
        assert!(has_seq(&out[2], &["kb=vv"]));
        assert!(!has_seq(&out[3], &["kb=vv"]), "{:?}", out[3]);
    }

    #[test]
    fn emitted_group_keeps_inner_trailing_off_earlier_sides() {
        // z , ( outside , vertical ,, 30m ) — the corner pure-cumulative
        // leaked: 30m must not reach z (here z is dur-bearing to prove
        // the point).
        let out = apply(&[
            &["start", "z"],
            &["(", "switch", "outside", "out"],
            &["switch", "vertical", "vert", ",,", "30m", ")"],
        ])
        .unwrap();
        assert!(!out[0].iter().any(|t| t == "30m"), "{:?}", out[0]);
        assert!(out[1].iter().any(|t| t == "30m"));
        assert!(out[2].iter().any(|t| t == "30m"));
    }

    #[test]
    fn unbalanced_groups_error() {
        let open = apply(&[&["(", "occur", "a"]]).unwrap_err();
        assert!(format!("{open}").contains("unbalanced"));
        let close = apply(&[&["occur", "a", ")"]]).unwrap_err();
        assert!(format!("{close}").contains("unbalanced"));
    }

    #[test]
    fn escaped_paren_fills_as_literal_data() {
        // `\(` / `\)` are the literal-paren escapes (mirroring `\.`);
        // parse_structure unescapes them on the way to the walk.
        let open = apply(&[&["occur", "f", "-k", "expr", ",,", "\\("]]).unwrap();
        assert!(has_seq(&open[0], &["expr=("]), "{:?}", open[0]);
        let close = apply(&[&["occur", "f", "-k", "expr", ",,", "\\)"]]).unwrap();
        assert!(has_seq(&close[0], &["expr=)"]), "{:?}", close[0]);
    }

    #[test]
    fn bare_paren_after_dash_dash_stays_structural() {
        // `--` governs flag parsing, not grouping: a bare paren is
        // structural even after `--` (demoting it would make a typed
        // `-- )` and the emitted wrapper's `)` ambiguous). The error
        // points at the escape.
        let err = apply(&[&["occur", "f", "-k", "expr", ",,", "--", "("]]).unwrap_err();
        assert!(format!("{err}").contains("literal paren"), "{err}");
    }

    #[test]
    fn no_regions_is_identity() {
        let out = apply(&[&["occur", "f", "x=1"], &["tail", "g"]]).unwrap();
        assert_eq!(out[0], v(&["occur", "f", "x=1"]));
        assert_eq!(out[1], v(&["tail", "g"]));
    }

    #[test]
    fn non_log_side_sinks_positionals() {
        let out = apply(&[&["tail", "f", ",,", "5"]]).unwrap();
        assert!(out[0].iter().any(|t| t == "5"));
    }
}
