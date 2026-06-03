//! Cross-side slot allocation for multi-entry aliases (da-6l4x +
//! da-vo0y Feature 3).
//!
//! Rule 13 used to say: an alias's call-site trailing args propagate to
//! *every* side via the implicit body-end `--`. That's right for flags
//! and `key=value` pairs, but wrong for bare positionals — `alias rans
//! "occur run -k distanceKm , switch body stretch"; rans 5` must put
//! `5` into `distanceKm`, not ALSO hand it to the switch side as a
//! duration.
//!
//! The reformed rule: call-site trailing **positionals** fill declared
//! slots left-to-right across the fully-expanded sides; everything else
//! (flags + their values, `key=value`) keeps the propagate-to-all
//! behavior. A side declares, in order:
//!
//! 1. its `-k`/`-K` keys (in declaration order). Same-named key slots
//!    across sides collapse to one slot at the leftmost declaring
//!    position; a fill propagates to every declaring side.
//! 2. a `state` slot, when the side is a `switch` with an unbound state
//!    (no bare positional after the field in its body).
//! 3. a `dur` slot, when the verb is duration-bearing (switch / start /
//!    stop / end). Verb-derived slots are per-side, never name-shared —
//!    `vx 5m sit 10m` fills v.dur=5m, x.state=sit, x.dur=10m.
//!
//! **`-I` / `-S -R <key>` greediness (da-vo0y F3):** a side whose body
//! declares a string-join (`-I`, or `-S` with `-R <key>`) and supplies
//! it **no** body positionals escapes to the call site: it becomes a
//! single greedy slot that claims ALL remaining positionals (the side's
//! own join + trailing-dur peek then digest them). Multiple greedy
//! sides share the same window — `alias both "switch tv -I , switch
//! movie -I"; both blueberry muffins` lands the state on both. A
//! body-satisfied join (`alias adti "tv adventure time"`) declares no
//! greedy slot — call-site positionals flow to the remaining slots
//! (`adti 15` → dur 15; `adti extra 15` → extra data error downstream).
//!
//! A bare `.` skips its slot (for a shared slot, on all declaring
//! sides); one dot consumes one skip.
//!
//! The walk happens here — after per-side alias expansion and
//! multi-of-multi flattening — because slot declarations only exist on
//! fully-expanded sides. The alias layer marks the call-site trailing
//! with [`TRAILING_MARKER`] instead of blanket-appending it (see
//! `expand::apply_alias_dd_distribution`).

use crate::error::{ParseError, Result};

/// Sentinel separating a side's local tokens from the alias call-site
/// trailing bucket. U+0001 can't be typed as a shell token in practice,
/// so user data never collides with it.
pub(super) const TRAILING_MARKER: &str = "\u{1}DD\u{1}";

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
/// command layer (da-ooe2), so the allocator reserves a per-side dur
/// position for them.
const DUR_VERBS: &[&str] = &["switch", "start", "stop", "end"];

fn is_value_flag(tok: &str) -> bool {
    VALUE_FLAGS.contains(&tok)
}

fn is_flag(tok: &str) -> bool {
    tok.len() > 1 && tok.starts_with('-') && !tok.chars().nth(1).is_some_and(|c| c.is_ascii_digit())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Trailing {
    /// A bare positional (including the `.` skip sentinel).
    Positional(String),
    /// A flag (with its value, when it takes one) or a `key=value` pair —
    /// keeps the propagate-to-all behavior.
    Other(Vec<String>),
}

/// Split the trailing bucket into positionals (to allocate) and
/// everything else (to blanket-promote), preserving order within each
/// class.
fn classify_trailing(tokens: &[String]) -> (Vec<String>, Vec<String>) {
    let mut positionals: Vec<String> = Vec::new();
    let mut others: Vec<String> = Vec::new();
    let mut classified: Vec<Trailing> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        if is_flag(tok) {
            let mut group = vec![tok.clone()];
            if is_value_flag(tok) {
                if let Some(v) = tokens.get(i + 1) {
                    group.push(v.clone());
                    i += 1;
                }
            }
            classified.push(Trailing::Other(group));
        } else if tok.contains('=') {
            classified.push(Trailing::Other(vec![tok.clone()]));
        } else {
            classified.push(Trailing::Positional(tok.clone()));
        }
        i += 1;
    }
    for c in classified {
        match c {
            Trailing::Positional(p) => positionals.push(p),
            Trailing::Other(g) => others.extend(g),
        }
    }
    (positionals, others)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SlotKind {
    /// `-k name` / `-K name` — filled as `name=value`, shared by name.
    Key { name: String },
    /// Verb-derived unbound switch state — filled as a bare positional
    /// (the side's switch peel takes the first bare token).
    State,
    /// Verb-derived duration — filled as a trailing bare positional
    /// (the side's injected `dur=` slot takes it).
    Dur,
    /// Unsatisfied `-I` / `-S -R` join — claims ALL remaining
    /// positionals; the side's own join + dur-peek digest them.
    Greedy,
}

#[derive(Debug, Clone)]
struct Slot {
    kind: SlotKind,
    /// Sides this slot fills (one entry except for shared key slots and
    /// shared greedy windows).
    sides: Vec<usize>,
}

/// Extract a side's declared slots, in claim order.
fn side_slots(side_idx: usize, side: &[String]) -> Vec<Slot> {
    // Identify verb and field, tolerating field-first order.
    let verb: Option<&str> = match (side.first(), side.get(1)) {
        (Some(t0), _) if DUR_VERBS.contains(&t0.as_str()) || t0 == "occur" || t0 == "add" => {
            Some(t0.as_str())
        }
        (_, Some(t1)) if DUR_VERBS.contains(&t1.as_str()) || t1 == "occur" || t1 == "add" => {
            Some(t1.as_str())
        }
        _ => None,
    };
    // Body tokens after verb+field: scan for keys, join flags, and bare
    // positionals (flag values excluded).
    let mut keys: Vec<String> = Vec::new();
    let mut has_join = false; // -I / -S / -SR / -C — the stringify-joins
    let mut lenient = false;
    let mut has_r = false;
    let mut body_positionals = 0usize;
    let mut seen_field = false;
    let mut seen_verb = false;
    let mut i = 0;
    while i < side.len() {
        let tok = &side[i];
        // Skip the verb token wherever it sits (verb-first or
        // field-first order) so it never counts as a body positional.
        if !seen_verb && Some(tok.as_str()) == verb {
            seen_verb = true;
            i += 1;
            continue;
        }
        if is_flag(tok) {
            match tok.as_str() {
                "-I"
                | "--string-state"
                | "-S"
                | "--string-remainder"
                | "-SR"
                | "-C"
                | "--comment-remainder" => has_join = true,
                "-l" | "--lenient" => lenient = true,
                "-R" | "--remainder" => has_r = true,
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
        if tok.contains('=') || tok == TRAILING_MARKER {
            i += 1;
            continue;
        }
        // Bare token: first one is the field (or the verb in field-first
        // order, which the verb detection above already accounted for).
        if !seen_field {
            seen_field = true;
        } else {
            body_positionals += 1;
        }
        i += 1;
    }

    // A side whose lede isn't a log verb is some other command (`get`,
    // `tail`, …) — the slot model doesn't apply; pass everything
    // through (old behavior) by declaring one big sink.
    let Some(verb) = verb else {
        return vec![Slot {
            kind: SlotKind::Greedy,
            sides: vec![side_idx],
        }];
    };

    let mut slots: Vec<Slot> = Vec::new();
    for spec in keys {
        let name = spec.split_once('=').map(|(n, _)| n).unwrap_or(&spec);
        slots.push(Slot {
            kind: SlotKind::Key {
                name: name.to_string(),
            },
            sides: vec![side_idx],
        });
    }
    let is_switch = verb == "switch";
    let dur_bearing = DUR_VERBS.contains(&verb);
    let greedy = Slot {
        kind: SlotKind::Greedy,
        sides: vec![side_idx],
    };
    if has_join {
        if body_positionals == 0 {
            // da-vo0y F3: an unsatisfied join escapes to the call site as
            // one greedy window (it subsumes the state and dur positions —
            // the side's own join + trailing-dur peek consume the window).
            slots.push(greedy);
            return slots;
        }
        // Satisfied join: body positionals feed it, so call-site
        // positionals do NOT extend the join — only the remaining slots.
        if dur_bearing {
            slots.push(Slot {
                kind: SlotKind::Dur,
                sides: vec![side_idx],
            });
        }
        if lenient {
            slots.push(greedy);
        }
        return slots;
    }
    if is_switch && body_positionals == 0 {
        slots.push(Slot {
            kind: SlotKind::State,
            sides: vec![side_idx],
        });
    }
    if dur_bearing {
        slots.push(Slot {
            kind: SlotKind::Dur,
            sides: vec![side_idx],
        });
    }
    // Unconditional absorbers: `-l` (lenient extra_data), a plain `-R`
    // (array remainder), and `add` (primary-value remainder) take any
    // number of leftover positionals.
    if lenient || has_r || verb == "add" {
        slots.push(greedy);
    }
    slots
}

/// Build the global slot list: per-side slots in (side, declaration)
/// order, with same-named key slots merged onto the leftmost position
/// and greedy windows merged into one shared window.
fn global_slots(sides: &[Vec<String>]) -> Vec<Slot> {
    let mut out: Vec<Slot> = Vec::new();
    for (i, side) in sides.iter().enumerate() {
        for slot in side_slots(i, side) {
            match &slot.kind {
                SlotKind::Key { name } => {
                    if let Some(existing) = out
                        .iter_mut()
                        .find(|s| matches!(&s.kind, SlotKind::Key { name: n } if n == name))
                    {
                        existing.sides.push(i);
                        continue;
                    }
                    out.push(slot);
                }
                SlotKind::Greedy => {
                    if let Some(existing) =
                        out.iter_mut().find(|s| matches!(s.kind, SlotKind::Greedy))
                    {
                        existing.sides.push(i);
                        continue;
                    }
                    out.push(slot);
                }
                _ => out.push(slot),
            }
        }
    }
    out
}

/// Distribute an alias's call-site trailing bucket across its fully
/// expanded sides. `sides` carry [`TRAILING_MARKER`]-separated trailing
/// copies (identical across sides, appended by the alias layer);
/// returns the sides with positionals slot-allocated and the rest
/// blanket-promoted.
pub(super) fn distribute_marked_trailing(sides: Vec<Vec<String>>) -> Result<Vec<Vec<String>>> {
    // Peel the trailing bucket off each side (first marker; strip any
    // nested markers inside the bucket — flattened multi-of-multi can
    // stack them).
    let mut locals: Vec<Vec<String>> = Vec::with_capacity(sides.len());
    let mut bucket: Option<Vec<String>> = None;
    for side in sides {
        match side.iter().position(|t| t == TRAILING_MARKER) {
            Some(idx) => {
                let mut iter = side.into_iter();
                let local: Vec<String> = iter.by_ref().take(idx).collect();
                let rest: Vec<String> = iter.filter(|t| t != TRAILING_MARKER).collect();
                if bucket.is_none() {
                    bucket = Some(rest);
                }
                locals.push(local);
            }
            None => locals.push(side),
        }
    }
    let Some(trailing) = bucket else {
        return Ok(locals); // no alias trailing in play
    };

    let (positionals, others) = classify_trailing(&trailing);
    let slots = global_slots(&locals);

    // Per-side fill emissions, applied after the walk so flag/key
    // promotion order stays stable.
    let mut key_fills: Vec<(usize, String)> = Vec::new(); // (side, "k=v")
    let mut state_fills: Vec<(usize, String)> = Vec::new();
    let mut dur_fills: Vec<(usize, String)> = Vec::new();
    let mut greedy_fill: Vec<(Vec<usize>, Vec<String>)> = Vec::new();

    let mut pi = 0usize;
    for slot in &slots {
        if pi >= positionals.len() {
            break;
        }
        if matches!(slot.kind, SlotKind::Greedy) {
            // Claims everything left.
            let window: Vec<String> = positionals[pi..].to_vec();
            pi = positionals.len();
            greedy_fill.push((slot.sides.clone(), window));
            break;
        }
        let value = positionals[pi].clone();
        pi += 1;
        if value == "." {
            continue; // explicit skip — on all declaring sides
        }
        match &slot.kind {
            SlotKind::Key { name } => {
                for s in &slot.sides {
                    key_fills.push((*s, format!("{name}={value}")));
                }
            }
            SlotKind::State => state_fills.push((slot.sides[0], value)),
            SlotKind::Dur => dur_fills.push((slot.sides[0], value)),
            SlotKind::Greedy => unreachable!("handled above"),
        }
    }
    if pi < positionals.len() {
        return Err(ParseError::ExtraData(positionals[pi..].to_vec()));
    }

    // Emit: keys (as explicit assignments), then state positional, then
    // dur positional, then the blanket-promoted rest, then greedy
    // windows (bare, for the side's own join+peek to digest).
    for (s, kv) in key_fills {
        locals[s].push(kv);
    }
    for (s, v) in state_fills {
        locals[s].push(v);
    }
    for (s, v) in dur_fills {
        locals[s].push(v);
    }
    for side in locals.iter_mut() {
        side.extend(others.iter().cloned());
    }
    for (side_idxs, window) in greedy_fill {
        for s in side_idxs {
            locals[s].extend(window.iter().cloned());
        }
    }
    Ok(locals)
}
