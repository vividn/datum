//! Side-scoped flag distribution + time-flag composition for
//! multi-entry aliases (spec v2.1: `docs/trailing-and-slots.md`;
//! historically da-gh0.D).
//!
//! The layer above [`super::expand::try_expand_alias`]: it expands all
//! aliases (including per-side aliases reached via comma split),
//! flattens multi-of-multi, applies the `,,` trailing regions (via
//! [`super::regions`]), and exposes a [`TimeFlags`] type that composes
//! alias-body-bound time flags with call-site time flags (Rule 15).
//!
//! Spec ground rules as they land here:
//!
//! - Time flags (and flags generally) are side-scoped in side text;
//!   inside a trailing region they blanket the region's bound sides.
//!   Time-irrelevant verbs silently ignore time flags (Rule 4's second
//!   sentence survives).
//! - Multi-of-multi flattens to N sides (Rule 7).
//! - A `,,` region binds all sides to its left within its innermost
//!   enclosing group (spec v2.1 rule 2). Every alias resolve wraps its
//!   body in `( … )`, so emitted regions are scoped lexically — this
//!   layer never tracks region provenance.
//! - `--` is unix end-of-flags, scoped to its side or region (spec
//!   rule 4) — the old promote-to-all `--` semantics are retired.
//! - **Rule 15** — Call-site time flags compose with alias-bound time
//!   flags (relative numeric `-t` offsets sum; `-q`/`-y` counters
//!   accumulate; absolute call-site values win).

use crate::error::{ParseError, Result};

use super::expand::{
    apply_switch_promote_override, apply_terminal_verb_override, expand_stacked_flags,
    maybe_collapse_switch_chain, maybe_swap_field_verb, split_argv_at_commas, try_expand_alias,
    AliasStep, Resolver, KNOWN_SUBCOMMANDS, MAX_ALIAS_HOPS, MULTI_COMMA,
};

/// Result of [`expand_into_sides`]: N per-side argvs (each without the
/// leading binary name) plus the chain of alias expansions that fired.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExpandedSides {
    pub sides: Vec<Vec<String>>,
    pub chain: Vec<AliasStep>,
}

/// Steps 1–4 of the spec v2.1 pipeline: terminal-verb override, alias
/// expansion (every resolve wraps its body in `( body ,, own-trailing )`),
/// comma split, and per-side fixpoint expansion with multi-of-multi
/// flattening.
///
/// The returned sides keep the wrapper groups and `,,` boundaries
/// INTACT — this is the spec's *assembled wrapped expansion*, the
/// self-describing token stream `datum why` surfaces (da-g5su).
/// [`expand_into_sides`] continues from here by applying regions and
/// the per-side normalisation passes.
pub fn expand_wrapped(argv: Vec<String>, resolver: &dyn Resolver) -> Result<ExpandedSides> {
    if argv.is_empty() {
        return Ok(ExpandedSides::default());
    }
    let bin = argv[0].clone();

    // Step 1 + 2: terminal verb override + outermost alias expansion.
    let argv = apply_terminal_verb_override(argv);
    let (argv, mut chain) = try_expand_alias(argv, resolver)?;

    // Step 3: comma-split (skip the leading binary token).
    let rest: Vec<String> = argv.into_iter().skip(1).collect();
    let sides = split_argv_at_commas(rest)?;

    // Steps 4 + 5: per-side alias expansion with multi-of-multi
    // flattening. Each resolve wraps its expansion in `( … )` (spec
    // v2.1), so the structure a region needs is IN the stream — no
    // staging or deferred bookkeeping here.
    let mut sides = sides;
    for _ in 0..MAX_ALIAS_HOPS {
        let mut next_sides: Vec<Vec<String>> = Vec::with_capacity(sides.len());
        let mut grew_or_changed = false;
        for side in sides {
            let mut side_argv = vec![bin.clone()];
            side_argv.extend(side);
            let before = side_argv.clone();
            let (side_argv, side_chain) = try_expand_alias(side_argv, resolver)?;
            if side_argv != before {
                grew_or_changed = true;
            }
            chain.extend(side_chain);
            let side: Vec<String> = side_argv.into_iter().skip(1).collect();
            if side.iter().any(|t| t == MULTI_COMMA) {
                grew_or_changed = true;
                next_sides.extend(split_argv_at_commas(side)?);
            } else {
                next_sides.push(side);
            }
        }
        sides = next_sides;
        if !grew_or_changed {
            break;
        }
    }

    Ok(ExpandedSides { sides, chain })
}

/// Full alias-expansion + comma-split + trailing-region pipeline
/// (spec v2.1).
///
/// Steps:
///
/// 1. Apply the terminal-verb override (`<field> end` → `end <field>`).
/// 2. Run [`try_expand_alias`] on the whole argv — every resolve wraps
///    its body in `( … ,, own-trailing )`.
/// 3. Comma-split the resulting flat argv into N sides (groups and
///    boundaries ride inside their sides; commas split at any depth).
/// 4. Per side, expand head alias chains to a fixpoint, flattening
///    multi-of-multi — each resolve adds its own nested group, so
///    region scope stays lexical in the stream.
///    (1–4 live in [`expand_wrapped`], whose output preserves the
///    wrapper groups.)
/// 5. Canonicalize stacked short flags, then ONE
///    [`super::regions::apply_regions`] pass: each `,,` binds the
///    sides to its left within its innermost enclosing group.
/// 6. (retired — `--` is unix end-of-flags now, spec rule 4.)
/// 7. Per side, apply the field/verb swap, BlockSwitch-chain collapse,
///    switch-promote override, and stacked-short-flag expansion.
pub fn expand_into_sides(argv: Vec<String>, resolver: &dyn Resolver) -> Result<ExpandedSides> {
    if argv.is_empty() {
        return Ok(ExpandedSides::default());
    }
    let bin = argv[0].clone();

    // Steps 1–4: alias expansion with wrapper groups intact.
    let ExpandedSides { sides, chain } = expand_wrapped(argv, resolver)?;

    // Step 5b: canonical flag shapes, then ONE region pass — a `,,`
    // binds the sides to its left within its innermost group, typed
    // top-level boundaries included (the outermost group is the whole
    // command).
    //
    // Stacked short flags expand BEFORE region application (per the #45
    // review): a bundle with a value-taking terminal (`-yyt 3`,
    // `-SR notes`) must reach the allocator as canonical `-t 3` /
    // `-S -R notes`, or the value token would misclassify as an
    // allocatable positional. Step 7c's expansion below is a no-op on
    // already-expanded sides.
    let sides: Vec<Vec<String>> = sides
        .into_iter()
        .map(|side| {
            let mut side_argv = vec![bin.clone()];
            side_argv.extend(side);
            expand_stacked_flags(side_argv)
                .into_iter()
                .skip(1)
                .collect()
        })
        .collect();
    let sides = super::regions::apply_regions(sides)?;

    // An empty side here means a trailing/doubled comma in the original
    // or an alias body like `a ,` — the same error the comma splitter
    // raises for bare empties (a boundary-only side slips past it
    // because the `,,` token made the side non-empty at split time).
    if sides.iter().any(|s| s.is_empty()) {
        return Err(ParseError::InvalidArg(
            "empty side in multi-entry command (trailing or doubled comma?)".to_string(),
        ));
    }

    // (Step 6 retired by spec v2: `--` is plain unix end-of-flags,
    // side-scoped, handled by the downstream parser — the old top-level
    // promote-to-all behavior is gone. Trailing regions (`,,`) replaced
    // it above.)

    // Step 7a (da-b4k5): per-side `maybe_swap_field_verb` so any
    // field-first form is normalized to verb-first BEFORE the
    // BlockSwitch-chain collapse runs — the collapse keys off
    // `sides[0][0] == "switch"`, which a pre-swap field-first side
    // (`["activity", "switch", "end", "14m"]`) wouldn't match.
    let mut swapped_sides: Vec<Vec<String>> = Vec::with_capacity(sides.len());
    for side in sides {
        let mut side_argv = vec![bin.clone()];
        side_argv.extend(side);
        let side_argv = maybe_swap_field_verb(side_argv);
        swapped_sides.push(side_argv.into_iter().skip(1).collect());
    }

    // Step 7b (da-b4k5): collapse a two-sided `switch f X , Y` chain
    // into a single side BEFORE switch-promote-override fires. Running
    // promote first would rewrite `switch f end <D>` → `end f <D>` on
    // side 0, hiding the `switch` lede the chain detector needs.
    let collapsed_sides = maybe_collapse_switch_chain(swapped_sides);

    // Step 7c: per-side switch-promote + stacked flags. The promote step
    // now no-ops on a side that carries a standalone `,` (the chain
    // marker), so the collapsed BlockSwitch side passes through with
    // `end`/`stop`/`start` intact for the chain handler to translate.
    let mut final_sides: Vec<Vec<String>> = Vec::with_capacity(collapsed_sides.len());
    for side in collapsed_sides {
        let mut side_argv = vec![bin.clone()];
        side_argv.extend(side);
        let side_argv = apply_switch_promote_override(side_argv);
        let side_argv = expand_stacked_flags(side_argv);
        final_sides.push(side_argv.into_iter().skip(1).collect());
    }

    Ok(ExpandedSides {
        sides: final_sides,
        chain,
    })
}

// ─────────────────────────────────────────────────────────────────────
// Time-flag composition (Rule 15)
// ─────────────────────────────────────────────────────────────────────

/// Composable timing-flag bundle. Drained out of a side's argv by
/// [`TimeFlags::extract`]; merged across body and call-site by
/// [`TimeFlags::compose`].
///
/// Differs from [`crate::input::time_args::TimeArgs`] in one respect:
/// relative `-t` offsets (`+N` / `-N` minutes) accumulate into
/// `relative_offset_minutes` instead of last-wins overwriting the
/// `time` field. That's the key for Rule 15 — `vvcaf -t +10` with
/// alias-bound `-t +5` composes to `+15`, not `+10`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TimeFlags {
    pub date: Option<String>,
    pub yesterday: u32,
    pub time: Option<String>,
    pub relative_offset_minutes: i64,
    pub quick: u32,
    pub timezone: Option<String>,
    pub full_day: bool,
    pub omit_timestamp: bool,
}

impl TimeFlags {
    /// Drain timing flags from `tokens`. Tokens not recognised as a
    /// timing flag are returned in `leftover` in their original order.
    ///
    /// Recognised forms:
    ///
    /// * `-y` / `--yesterday` — switch, stackable
    /// * `-q` / `--quick`     — switch, stackable, **never** consumes a
    ///   positional even if the next token is numeric (Rule 4 spec
    ///   carve-out: `-q 3` → `3` is positional)
    /// * `-d <val>` / `--date <val>` / `--date=<val>` — value flag
    /// * `-t <val>` / `--time <val>` / `--time=<val>` — value flag.
    ///   `+N` / `-N` numeric values land in
    ///   `relative_offset_minutes`; everything else lands in `time`
    ///   (last absolute wins).
    /// * `-z <val>` / `--timezone <val>` / `--timezone=<val>`
    /// * `-D` / `--full-day` — switch
    /// * `-T` / `--no-timestamp` — switch
    pub fn extract(tokens: Vec<String>) -> Result<(Self, Vec<String>)> {
        let mut flags = Self::default();
        let mut leftover: Vec<String> = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let tok = &tokens[i];
            match tok.as_str() {
                "-y" | "--yesterday" => {
                    flags.yesterday = flags.yesterday.saturating_add(1);
                }
                "-q" | "--quick" => {
                    flags.quick = flags.quick.saturating_add(1);
                }
                "-D" | "--full-day" => {
                    flags.full_day = true;
                }
                "-T" | "--no-timestamp" => {
                    flags.omit_timestamp = true;
                }
                "-d" | "--date" => {
                    i += 1;
                    let val = tokens
                        .get(i)
                        .cloned()
                        .ok_or_else(|| ParseError::InvalidArg("`-d` requires a value".into()))?;
                    flags.date = Some(val);
                }
                "-t" | "--time" => {
                    i += 1;
                    let val = tokens
                        .get(i)
                        .cloned()
                        .ok_or_else(|| ParseError::InvalidArg("`-t` requires a value".into()))?;
                    apply_time_value(&mut flags, &val);
                }
                "-z" | "--timezone" => {
                    i += 1;
                    let val = tokens
                        .get(i)
                        .cloned()
                        .ok_or_else(|| ParseError::InvalidArg("`-z` requires a value".into()))?;
                    flags.timezone = Some(val);
                }
                other if other.starts_with("--date=") => {
                    flags.date = Some(other.trim_start_matches("--date=").to_string());
                }
                other if other.starts_with("--time=") => {
                    let val = other.trim_start_matches("--time=").to_string();
                    apply_time_value(&mut flags, &val);
                }
                other if other.starts_with("--timezone=") => {
                    flags.timezone = Some(other.trim_start_matches("--timezone=").to_string());
                }
                _ => leftover.push(tok.clone()),
            }
            i += 1;
        }
        Ok((flags, leftover))
    }

    /// Compose two [`TimeFlags`] bundles. `other` typically represents
    /// the call-site (promoted) flags being layered onto `self` (the
    /// per-side / alias-body-bound flags). Rule 15:
    ///
    /// * Counters (`-y`, `-q`) add.
    /// * `relative_offset_minutes` adds — relative `-t` offsets compose.
    /// * Absolute `time` and `date`: `other` wins if set.
    /// * `timezone`: `other` wins if set.
    /// * Booleans OR together.
    pub fn compose(self, other: Self) -> Self {
        Self {
            date: other.date.or(self.date),
            yesterday: self.yesterday + other.yesterday,
            time: other.time.or(self.time),
            relative_offset_minutes: self.relative_offset_minutes + other.relative_offset_minutes,
            quick: self.quick + other.quick,
            timezone: other.timezone.or(self.timezone),
            full_day: self.full_day || other.full_day,
            omit_timestamp: self.omit_timestamp || other.omit_timestamp,
        }
    }

    /// True when no timing modifier is set. Useful for the per-side
    /// "time-irrelevant verb silently ignores time flags" path: callers
    /// extract flags, then drop the result if the verb doesn't accept
    /// them — but the `leftover` argv is unchanged, so the side still
    /// parses normally.
    pub fn is_empty(&self) -> bool {
        self.date.is_none()
            && self.yesterday == 0
            && self.time.is_none()
            && self.relative_offset_minutes == 0
            && self.quick == 0
            && self.timezone.is_none()
            && !self.full_day
            && !self.omit_timestamp
    }
}

/// `add` and other verbs that don't bind to a moment. The slot walker
/// silently drops time flags for these (Rule 4 second sentence).
const TIME_IRRELEVANT_VERBS: &[&str] = &[
    "add", "alias", "rekey", "edit", "config", "db", "spec", "help", "ids", "count", "check",
    "grep", "info",
];

/// Does this verb consume time flags? `false` for `add`, `alias`, etc.
pub fn verb_is_time_relevant(verb: &str) -> bool {
    if !KNOWN_SUBCOMMANDS.contains(&verb) {
        return true;
    }
    !TIME_IRRELEVANT_VERBS.contains(&verb)
}

fn apply_time_value(flags: &mut TimeFlags, val: &str) {
    if let Some(mins) = parse_pure_relative_minutes(val) {
        flags.relative_offset_minutes += mins;
    } else {
        flags.time = Some(val.to_string());
    }
}

/// Recognise a bare `+N` / `-N` minutes offset. Returns the signed
/// minute count, or `None` if `val` carries a unit suffix, a clock-like
/// shape, or any other content the structured time parser would route
/// through [`crate::input::time_args::apply_time_arg`].
///
/// Conservative on purpose: we only fold cases where composition is
/// unambiguous (relative + relative → sum). Mixed forms (`10:00` +
/// `+5`) leave `time` as the absolute and `relative_offset_minutes` to
/// any additional relative — caller decides how to apply both.
fn parse_pure_relative_minutes(val: &str) -> Option<i64> {
    if val.len() < 2 {
        return None;
    }
    let (sign, body) = match val.as_bytes()[0] {
        b'+' => (1i64, &val[1..]),
        b'-' => (-1i64, &val[1..]),
        _ => return None,
    };
    // Must be a pure integer (no unit suffix, no direction marker).
    let n: i64 = body.parse().ok()?;
    Some(sign * n)
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::expand::{shell_split, MapResolver, ResolvedAlias};
    use super::*;
    use std::collections::HashMap;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    fn alias(expands_to: &str) -> ResolvedAlias {
        ResolvedAlias {
            expands_to: expands_to.to_string(),
            tokens: shell_split(expands_to),
        }
    }

    /// Build a resolver pre-seeded with the da-gh0 setup aliases used
    /// across most tests in this file.
    fn standard_resolver() -> HashMap<String, ResolvedAlias> {
        let mut a = HashMap::new();
        a.insert("vertical".into(), alias("switch body vertical"));
        a.insert("sit".into(), alias("switch body sit"));
        a.insert("supine".into(), alias("switch body supine"));
        a.insert("outside".into(), alias("switch environment outside"));
        a.insert("caffeine".into(), alias("occur caffeine -k doseMg="));
        a.insert(
            "f1".into(),
            alias("start field1 -k arg1 -k arg2= -k arg3=def_val_3"),
        );
        a.insert("f2".into(), alias("switch field2 -k arg2"));
        a.insert("f12".into(), alias("f1 , f2 abc"));
        a.insert("oov".into(), alias("outside , vertical"));
        a.insert("scaff".into(), alias("sit , caffeine"));
        a.insert("vvcaf".into(), alias("vertical , caffeine -t +5"));
        a.insert("x".into(), alias("switch body"));
        a.insert("vx".into(), alias("vertical , x"));
        a.insert("cx".into(), alias("caffeine , x"));
        a
    }

    fn expand(argv: &[&str]) -> ExpandedSides {
        let a = standard_resolver();
        expand_into_sides(v(argv), &MapResolver(&a)).expect("expand")
    }

    // ── expand_wrapped: the assembled wrapped stream (da-g5su) ───────────

    #[test]
    fn expand_wrapped_keeps_nested_groups_intact() {
        // Spec v2.1 rule 1's nested example:
        // `ax y` → `( ( occur a -k keyX -k keyY ,, x ) ,, y )`.
        let mut a = HashMap::new();
        a.insert("a".into(), alias("occur a -k keyX -k keyY"));
        a.insert("ax".into(), alias("a x"));
        let r = expand_wrapped(v(&["datum", "ax", "y"]), &MapResolver(&a)).expect("expand");
        assert_eq!(r.sides.len(), 1);
        assert_eq!(
            r.sides[0],
            v(&[
                "(", "(", "occur", "a", "-k", "keyX", "-k", "keyY", ",,", "x", ")", ",,", "y", ")",
            ]),
        );
        assert_eq!(r.chain.len(), 2, "ax and a both fire");
    }

    #[test]
    fn expand_wrapped_multi_alias_region_stays_inside_group() {
        // Spec v2.1 rule 2's v2-corner example: `occur z , oov 30m` —
        // oov's group keeps the 30m region off the z side, and each
        // inner resolve adds its own wrapper.
        let r = {
            let a = standard_resolver();
            expand_wrapped(
                v(&["datum", "occur", "z", ",", "oov", "30m"]),
                &MapResolver(&a),
            )
            .expect("expand")
        };
        // Sides split at the comma at any depth; parens balance across
        // the re-joined stream, not per side.
        assert_eq!(r.sides[0], v(&["occur", "z"]));
        let joined = r
            .sides
            .iter()
            .map(|s| s.join(" "))
            .collect::<Vec<_>>()
            .join(" , ");
        assert_eq!(
            joined,
            "occur z , ( ( switch environment outside ,, ) , \
             ( switch body vertical ,, ) ,, 30m )",
        );
    }

    // ── Section F: keyed args distribution ───────────────────────────────

    #[test]
    fn keyed_left_of_comma_scopes_to_left_side() {
        // User types directly: `dm vertical abc=1 , caffeine`.
        let r = expand(&["datum", "vertical", "abc=1", ",", "caffeine"]);
        assert_eq!(r.sides.len(), 2);
        // Side 0 gets abc=1; side 1 does not.
        assert!(r.sides[0].iter().any(|t| t == "abc=1"));
        assert!(!r.sides[1].iter().any(|t| t == "abc=1"));
    }

    #[test]
    fn keyed_right_of_comma_scopes_to_right_side() {
        let r = expand(&["datum", "vertical", ",", "caffeine", "xyz=2"]);
        assert!(!r.sides[0].iter().any(|t| t == "xyz=2"));
        assert!(r.sides[1].iter().any(|t| t == "xyz=2"));
    }

    #[test]
    fn keyed_after_implicit_double_dash_at_call_site_applies_to_all_sides() {
        // Implicit body-end DD: call-site `xyz=3` after a multi-alias
        // promotes to every side.
        let mut a = HashMap::new();
        a.insert("m".into(), alias("switch body sit , occur caffeine"));
        let r = expand_into_sides(v(&["datum", "m", "xyz=3"]), &MapResolver(&a)).unwrap();
        assert_eq!(r.sides.len(), 2);
        assert!(r.sides[0].iter().any(|t| t == "xyz=3"));
        assert!(r.sides[1].iter().any(|t| t == "xyz=3"));
    }

    #[test]
    fn keyed_after_boundary_in_body_applies_to_all_sides_inside_alias() {
        // A `,,` written in the alias body opens a region over the
        // body's sides — the author's own shared payload (spec v2).
        let mut a = HashMap::new();
        a.insert(
            "m".into(),
            alias("switch body sit , occur caffeine ,, shared=Y"),
        );
        let r = expand_into_sides(v(&["datum", "m"]), &MapResolver(&a)).unwrap();
        assert_eq!(r.sides.len(), 2);
        assert!(r.sides[0].iter().any(|t| t == "shared=Y"));
        assert!(r.sides[1].iter().any(|t| t == "shared=Y"));
    }

    #[test]
    fn keyed_after_explicit_double_dash_at_call_site_idempotent_with_implicit() {
        // Explicit `--` at call site is redundant with the implicit
        // body-end `--`; result must match the without-`--` form.
        let mut a = HashMap::new();
        a.insert("m".into(), alias("switch body sit , occur caffeine"));
        let with_dd = expand_into_sides(v(&["datum", "m", "--", "k=X"]), &MapResolver(&a)).unwrap();
        let without_dd = expand_into_sides(v(&["datum", "m", "k=X"]), &MapResolver(&a)).unwrap();
        assert_eq!(with_dd.sides, without_dd.sides);
        for side in &with_dd.sides {
            assert!(side.iter().any(|t| t == "k=X"));
            assert!(!side.iter().any(|t| t == "--"));
        }
    }

    #[test]
    fn keyed_split_per_side_then_promoted_after_boundary() {
        // `vertical abc=1 , sit ghi=2 ,, xyz=3`. Typed `,,`: per-side
        // keys stay scoped; the region's k=v blankets the prefix.
        let r = expand(&[
            "datum", "vertical", "abc=1", ",", "sit", "ghi=2", ",,", "xyz=3",
        ]);
        assert_eq!(r.sides.len(), 2);
        assert!(r.sides[0].iter().any(|t| t == "abc=1"));
        assert!(!r.sides[0].iter().any(|t| t == "ghi=2"));
        assert!(r.sides[0].iter().any(|t| t == "xyz=3"));
        assert!(!r.sides[1].iter().any(|t| t == "abc=1"));
        assert!(r.sides[1].iter().any(|t| t == "ghi=2"));
        assert!(r.sides[1].iter().any(|t| t == "xyz=3"));
    }

    #[test]
    fn unix_dash_dash_does_not_promote() {
        // Spec v2 rule 4: `--` is end-of-flags, side-scoped — the old
        // promote-to-all meaning is gone. k=v after it stays on its
        // side (k=v is data grammar, unaffected by the latch).
        let r = expand(&[
            "datum", "vertical", "abc=1", ",", "sit", "ghi=2", "--", "xyz=3",
        ]);
        assert_eq!(r.sides.len(), 2);
        assert!(!r.sides[0].iter().any(|t| t == "xyz=3"), "{:?}", r.sides[0]);
        assert!(r.sides[1].iter().any(|t| t == "xyz=3"), "{:?}", r.sides[1]);
    }

    // ── Section J: time flag scoping and composition ─────────────────────

    fn extract_time(side: &[String]) -> (TimeFlags, Vec<String>) {
        TimeFlags::extract(side.to_vec()).unwrap()
    }

    #[test]
    fn time_flag_in_alias_body_scoped_to_its_side() {
        // vvcaf = vertical , caffeine -t +5 → `-t +5` only on side 1.
        let r = expand(&["datum", "vvcaf"]);
        assert_eq!(r.sides.len(), 2);
        let (t0, _) = extract_time(&r.sides[0]);
        let (t1, _) = extract_time(&r.sides[1]);
        assert_eq!(t0.relative_offset_minutes, 0);
        assert_eq!(t1.relative_offset_minutes, 5);
    }

    #[test]
    fn time_flag_left_of_comma_scopes_to_left_side() {
        let r = expand(&["datum", "vertical", "-t", "+10", ",", "caffeine"]);
        let (t0, _) = extract_time(&r.sides[0]);
        let (t1, _) = extract_time(&r.sides[1]);
        assert_eq!(t0.relative_offset_minutes, 10);
        assert_eq!(t1.relative_offset_minutes, 0);
    }

    #[test]
    fn time_flag_right_of_comma_scopes_to_right_side() {
        let r = expand(&["datum", "vertical", ",", "caffeine", "-t", "+7"]);
        let (t0, _) = extract_time(&r.sides[0]);
        let (t1, _) = extract_time(&r.sides[1]);
        assert_eq!(t0.relative_offset_minutes, 0);
        assert_eq!(t1.relative_offset_minutes, 7);
    }

    #[test]
    fn time_flag_after_implicit_double_dash_applies_to_all_sides() {
        // oov = outside , vertical. `oov -q` → both sides get -q.
        let r = expand(&["datum", "oov", "-q"]);
        assert_eq!(r.sides.len(), 2);
        let (t0, _) = extract_time(&r.sides[0]);
        let (t1, _) = extract_time(&r.sides[1]);
        assert_eq!(t0.quick, 1);
        assert_eq!(t1.quick, 1);
    }

    #[test]
    fn time_flag_at_call_site_composes_with_alias_bound_time() {
        // vvcaf bound `-t +5` on caffeine side. `vvcaf -t +10` →
        // vertical +10, caffeine +15.
        let r = expand(&["datum", "vvcaf", "-t", "+10"]);
        let (t0, _) = extract_time(&r.sides[0]);
        let (t1, _) = extract_time(&r.sides[1]);
        assert_eq!(t0.relative_offset_minutes, 10);
        assert_eq!(t1.relative_offset_minutes, 15);
    }

    #[test]
    fn stacked_q_flag_composes_minutes_back() {
        // -qq → quick=2 → 10 min back. -qqq → quick=3 → 15 min.
        let r = expand(&["datum", "oov", "-qq"]);
        let (t0, _) = extract_time(&r.sides[0]);
        assert_eq!(t0.quick, 2);
        let r = expand(&["datum", "oov", "-qqq"]);
        let (t1, _) = extract_time(&r.sides[1]);
        assert_eq!(t1.quick, 3);
    }

    #[test]
    fn q_flag_takes_no_argument() {
        // `-q 3` → `3` is positional, not flag arg.
        let r = expand(&["datum", "occur", "f", "-q", "3"]);
        let (t, leftover) = extract_time(&r.sides[0]);
        assert_eq!(t.quick, 1);
        assert!(leftover.iter().any(|x| x == "3"));
    }

    #[test]
    fn time_irrelevant_verb_silently_ignores_time_flag() {
        // `add` doesn't consume time flags — the slot walker drops
        // them per Rule 4. We surface this via verb_is_time_relevant
        // so the caller can branch.
        assert!(verb_is_time_relevant("switch"));
        assert!(verb_is_time_relevant("occur"));
        assert!(verb_is_time_relevant("start"));
        assert!(!verb_is_time_relevant("add"));
        assert!(!verb_is_time_relevant("alias"));
    }

    #[test]
    fn typed_boundary_promotes_time_flag_to_all_sides() {
        // Spec v2: a typed `,,` opens a region binding the cumulative
        // prefix; region flags blanket. `-t +5` stays side-local;
        // `-t +10` in the region reaches both. (The old `--` promote
        // is gone — `--` is unix end-of-flags now, rule 4.)
        let r = expand(&[
            "datum", "vertical", "-t", "+5", ",", "caffeine", ",,", "-t", "+10",
        ]);
        let (t0, _) = extract_time(&r.sides[0]);
        let (t1, _) = extract_time(&r.sides[1]);
        assert_eq!(t0.relative_offset_minutes, 15);
        assert_eq!(t1.relative_offset_minutes, 10);
    }

    // ── Section K: composition ───────────────────────────────────────────

    #[test]
    fn multi_of_multi_flattens_to_n_sides() {
        // y = x_alias , c. x_alias = a , b. Three flat sides.
        let mut a = HashMap::new();
        a.insert("a".into(), alias("occur a_field"));
        a.insert("b".into(), alias("occur b_field"));
        a.insert("c".into(), alias("occur c_field"));
        a.insert("xx".into(), alias("a , b"));
        a.insert("yy".into(), alias("xx , c"));
        let r = expand_into_sides(v(&["datum", "yy"]), &MapResolver(&a)).unwrap();
        assert_eq!(r.sides.len(), 3);
        assert!(r.sides[0].iter().any(|t| t == "a_field"));
        assert!(r.sides[1].iter().any(|t| t == "b_field"));
        assert!(r.sides[2].iter().any(|t| t == "c_field"));
    }

    #[test]
    fn three_layer_alias_chain_flattens() {
        let mut a = HashMap::new();
        a.insert("L1a".into(), alias("occur l1a"));
        a.insert("L1b".into(), alias("occur l1b"));
        a.insert("L2".into(), alias("L1a , L1b"));
        a.insert("L3".into(), alias("L2 , occur l3"));
        let r = expand_into_sides(v(&["datum", "L3"]), &MapResolver(&a)).unwrap();
        assert_eq!(r.sides.len(), 3);
        assert!(r.sides[0].iter().any(|t| t == "l1a"));
        assert!(r.sides[1].iter().any(|t| t == "l1b"));
        assert!(r.sides[2].iter().any(|t| t == "l3"));
    }

    #[test]
    fn double_dash_inside_alias_body_scope_ends_at_alias_boundary() {
        // Rule 8: `inner = foo -- key=val` used inside `m = inner , c`.
        // key=val stays scoped to inner's expansion (side 0) — it must
        // NOT leak onto side 1.
        let mut a = HashMap::new();
        a.insert("inner".into(), alias("occur inner_field -- key=val"));
        a.insert("m".into(), alias("inner , occur outer_field"));
        let r = expand_into_sides(v(&["datum", "m"]), &MapResolver(&a)).unwrap();
        assert_eq!(r.sides.len(), 2);
        assert!(
            r.sides[0].iter().any(|t| t == "key=val"),
            "side 0 (inner) should have key=val: {:?}",
            r.sides[0]
        );
        assert!(
            !r.sides[1].iter().any(|t| t == "key=val"),
            "side 1 must NOT have key=val (Rule 8): {:?}",
            r.sides[1]
        );
    }

    #[test]
    fn alias_body_double_dash_implied_at_end_only() {
        // No explicit body `--`; call-site key promotes via implicit DD.
        let mut a = HashMap::new();
        a.insert("m".into(), alias("occur a , occur b"));
        let r = expand_into_sides(v(&["datum", "m", "shared=Z"]), &MapResolver(&a)).unwrap();
        for side in &r.sides {
            assert!(side.iter().any(|t| t == "shared=Z"));
        }
    }

    #[test]
    fn nested_alias_with_internal_keys_keeps_them_side_scoped() {
        // Inner alias declares `-k mykey`. Used as one side of an outer
        // multi. Per-side `-k` slot stays scoped to inner's side.
        let mut a = HashMap::new();
        a.insert("inner".into(), alias("occur inner_field -k mykey"));
        a.insert("m".into(), alias("inner , occur outer_field"));
        let r = expand_into_sides(v(&["datum", "m"]), &MapResolver(&a)).unwrap();
        assert!(r.sides[0].iter().any(|t| t == "-k"));
        assert!(r.sides[0].iter().any(|t| t == "mykey"));
        assert!(!r.sides[1].iter().any(|t| t == "mykey"));
    }

    #[test]
    fn alias_definition_must_have_at_least_two_sides() {
        // `m = a ,` parses (alias-set is just a string) but invoking
        // it must error at the comma-split step (empty trailing side).
        let mut a = HashMap::new();
        a.insert("m".into(), alias("occur a ,"));
        let err = expand_into_sides(v(&["datum", "m"]), &MapResolver(&a)).unwrap_err();
        assert!(
            matches!(err, ParseError::InvalidArg(_)),
            "expected InvalidArg for `m = a ,`: {err:?}"
        );
    }

    #[test]
    fn unknown_key_at_call_site_errors() {
        // Call-site `unknown=val` (promoted to all sides) with no
        // matching `-k` declaration on any side. The scope layer
        // surfaces it as `key=val` on every side — write-time policy
        // (.C) decides whether to error or silently drop. For the
        // parser-layer test we just confirm propagation.
        let mut a = HashMap::new();
        a.insert("m".into(), alias("occur a , occur b"));
        let r = expand_into_sides(v(&["datum", "m", "unknown=val"]), &MapResolver(&a)).unwrap();
        for side in &r.sides {
            assert!(side.iter().any(|t| t == "unknown=val"));
        }
    }

    #[test]
    fn switch_promote_in_multi_comma_per_side() {
        // `dm switch body sit , switch project end` — second side must
        // promote its `end` from literal state to the `end` verb.
        let r = expand(&[
            "datum", "switch", "body", "sit", ",", "switch", "project", "end",
        ]);
        assert_eq!(r.sides.len(), 2);
        assert_eq!(r.sides[0], v(&["switch", "body", "sit"]));
        assert_eq!(r.sides[1], v(&["end", "project"]));
    }

    #[test]
    fn switch_promote_in_multi_comma_left_side() {
        let r = expand(&[
            "datum", "switch", "project", "end", ",", "switch", "body", "sit",
        ]);
        assert_eq!(r.sides.len(), 2);
        assert_eq!(r.sides[0], v(&["end", "project"]));
        assert_eq!(r.sides[1], v(&["switch", "body", "sit"]));
    }

    #[test]
    fn verb_conflict_same_field_same_time_handled_via_merge_not_error() {
        // `vertical , sit` writes two switch-on-body entries that, at
        // write time, merge into one row via update's algorithm (.C
        // owns the merge). At the scope layer both sides must surface
        // independently with `body` as the field — no parse error.
        let r = expand(&["datum", "vertical", ",", "sit"]);
        assert_eq!(r.sides.len(), 2);
        // Both sides expanded to `switch body <state>`.
        assert!(r.sides[0].iter().any(|t| t == "switch"));
        assert!(r.sides[0].iter().any(|t| t == "body"));
        assert!(r.sides[0].iter().any(|t| t == "vertical"));
        assert!(r.sides[1].iter().any(|t| t == "switch"));
        assert!(r.sides[1].iter().any(|t| t == "body"));
        assert!(r.sides[1].iter().any(|t| t == "sit"));
    }

    // ── Time composition unit tests ──────────────────────────────────────

    #[test]
    fn time_flags_compose_relative_offsets_sum() {
        let a = TimeFlags {
            relative_offset_minutes: 5,
            ..Default::default()
        };
        let b = TimeFlags {
            relative_offset_minutes: 10,
            ..Default::default()
        };
        let c = a.compose(b);
        assert_eq!(c.relative_offset_minutes, 15);
    }

    #[test]
    fn time_flags_compose_counters_add() {
        let a = TimeFlags {
            quick: 2,
            yesterday: 1,
            ..Default::default()
        };
        let b = TimeFlags {
            quick: 3,
            yesterday: 2,
            ..Default::default()
        };
        let c = a.compose(b);
        assert_eq!(c.quick, 5);
        assert_eq!(c.yesterday, 3);
    }

    #[test]
    fn time_flags_extract_relative_t_into_offset_not_time() {
        let (t, _) = TimeFlags::extract(v(&["-t", "+5"])).unwrap();
        assert_eq!(t.relative_offset_minutes, 5);
        assert!(t.time.is_none());
    }

    #[test]
    fn time_flags_extract_absolute_t_into_time() {
        let (t, _) = TimeFlags::extract(v(&["-t", "10:00"])).unwrap();
        assert_eq!(t.relative_offset_minutes, 0);
        assert_eq!(t.time.as_deref(), Some("10:00"));
    }

    #[test]
    fn time_flags_extract_drops_known_flags_from_leftover() {
        let (_, leftover) =
            TimeFlags::extract(v(&["occur", "-t", "+5", "field", "-q", "rest"])).unwrap();
        assert_eq!(leftover, v(&["occur", "field", "rest"]));
    }

    // ── Section L: cross-side slot allocation (da-6l4x + da-vo0y F3) ─────

    #[test]
    fn trailing_positional_fills_leftmost_slot_not_all_sides() {
        // The da-6l4x headline: `rans 5` puts 5 into distanceKm on side 0
        // and does NOT hand it to the switch side as a duration.
        let mut a = HashMap::new();
        a.insert(
            "rans".into(),
            alias("occur run -k distanceKm , switch body stretch"),
        );
        let r = expand_into_sides(v(&["datum", "rans", "5"]), &MapResolver(&a)).unwrap();
        assert_eq!(r.sides.len(), 2);
        assert!(
            r.sides[0].iter().any(|t| t == "distanceKm=5"),
            "side0: {:?}",
            r.sides[0]
        );
        assert!(
            !r.sides[1].iter().any(|t| t == "5"),
            "side1 must not get the 5: {:?}",
            r.sides[1]
        );
    }

    #[test]
    fn scaff_no_args_no_fills() {
        let r = expand(&["datum", "scaff"]);
        assert_eq!(r.sides[0], v(&["switch", "body", "sit"]));
        assert_eq!(r.sides[1], v(&["occur", "caffeine", "-k", "doseMg="]));
    }

    #[test]
    fn scaff_one_arg_fills_first_side_dur() {
        let r = expand(&["datum", "scaff", "5"]);
        assert_eq!(r.sides[0], v(&["switch", "body", "sit", "5"]));
        assert!(!r.sides[1].iter().any(|t| t == "5"), "{:?}", r.sides[1]);
    }

    #[test]
    fn scaff_two_args_fill_dur_then_dose() {
        let r = expand(&["datum", "scaff", "5", "100"]);
        assert_eq!(r.sides[0], v(&["switch", "body", "sit", "5"]));
        assert!(
            r.sides[1].iter().any(|t| t == "doseMg=100"),
            "{:?}",
            r.sides[1]
        );
    }

    #[test]
    fn scaff_dot_skips_dur_slot() {
        let r = expand(&["datum", "scaff", ".", "100"]);
        assert_eq!(r.sides[0], v(&["switch", "body", "sit"]));
        assert!(r.sides[1].iter().any(|t| t == "doseMg=100"));
    }

    #[test]
    fn two_keys_across_sides_fill_in_order() {
        let mut a = HashMap::new();
        a.insert("m".into(), alias("occur a -k x= , occur b -k y="));
        let r = expand_into_sides(v(&["datum", "m", "1", "2"]), &MapResolver(&a)).unwrap();
        assert!(r.sides[0].iter().any(|t| t == "x=1"), "{:?}", r.sides[0]);
        assert!(r.sides[1].iter().any(|t| t == "y=2"), "{:?}", r.sides[1]);
    }

    #[test]
    fn shared_key_slot_fills_all_declaring_sides() {
        let mut a = HashMap::new();
        a.insert("m".into(), alias("occur a -k arg , occur b -k arg"));
        let r = expand_into_sides(v(&["datum", "m", "vv"]), &MapResolver(&a)).unwrap();
        assert!(r.sides[0].iter().any(|t| t == "arg=vv"));
        assert!(r.sides[1].iter().any(|t| t == "arg=vv"));
    }

    #[test]
    fn shared_key_dot_skips_on_all_sides() {
        let mut a = HashMap::new();
        a.insert("m".into(), alias("occur a -k arg= , occur b -k arg="));
        let r = expand_into_sides(v(&["datum", "m", "."]), &MapResolver(&a)).unwrap();
        // (`arg=` itself is the -k declaration token; a FILL would be
        // `arg=<value>` with a non-empty value.)
        assert!(!r.sides[0]
            .iter()
            .any(|t| t.starts_with("arg=") && t.len() > 4));
        assert!(!r.sides[1]
            .iter()
            .any(|t| t.starts_with("arg=") && t.len() > 4));
    }

    #[test]
    fn vx_kind_shares_dur_across_sides() {
        // Spec v2 rule 6: dur slots unify by kind — one fill propagates
        // to every dur-bearing side. vx = vertical , x.
        let r = expand(&["datum", "vx", "5m", "sit"]);
        assert_eq!(r.sides[0], v(&["switch", "body", "vertical", "5m"]));
        assert_eq!(r.sides[1], v(&["switch", "body", "sit", "5m"]));
    }

    #[test]
    fn vx_per_side_durs_from_trailing_are_excess() {
        // Differing per-side durs are expressed in-side, not from
        // trailing (spec rule 6); a second dur positional is excess.
        let a = standard_resolver();
        let r = expand_into_sides(v(&["datum", "vx", "5m", "sit", "10m"]), &MapResolver(&a));
        assert!(matches!(r, Err(ParseError::ExtraData(_))), "{r:?}");
    }

    #[test]
    fn cx_fills_dose_state_dur() {
        // cx = caffeine , x. Global slots: [c.doseMg, x.state, x.dur].
        let r = expand(&["datum", "cx", "100", "vertical", "5m"]);
        assert!(
            r.sides[0].iter().any(|t| t == "doseMg=100"),
            "{:?}",
            r.sides[0]
        );
        assert_eq!(r.sides[1], v(&["switch", "body", "vertical", "5m"]));
    }

    #[test]
    fn excess_trailing_positionals_error() {
        let a = standard_resolver();
        let r = expand_into_sides(
            v(&["datum", "scaff", "5", "100", "woops"]),
            &MapResolver(&a),
        );
        assert!(matches!(r, Err(ParseError::ExtraData(_))), "{r:?}");
    }

    #[test]
    fn flags_and_keyed_still_promote_to_all_sides() {
        // The reformed Rule 13 only reroutes bare positionals.
        let r = expand(&["datum", "scaff", "-q", "shared=Z"]);
        for side in &r.sides {
            assert!(side.iter().any(|t| t == "-q"), "{side:?}");
            assert!(side.iter().any(|t| t == "shared=Z"), "{side:?}");
        }
    }

    #[test]
    fn unsatisfied_string_state_sides_share_the_window() {
        // da-vo0y F3: `both = switch tv -I , switch movie -I` — neither
        // body supplies the join, so both escape and share the window.
        let mut a = HashMap::new();
        a.insert("both".into(), alias("switch tv -I , switch movie -I"));
        let r = expand_into_sides(
            v(&["datum", "both", "blueberry", "muffins"]),
            &MapResolver(&a),
        )
        .unwrap();
        for side in &r.sides {
            assert!(side.iter().any(|t| t == "blueberry"), "{side:?}");
            assert!(side.iter().any(|t| t == "muffins"), "{side:?}");
        }
    }

    #[test]
    fn satisfied_join_side_yields_slots_to_others() {
        // The -I side's join is satisfied by body positionals, so the
        // call-site positionals flow to the remaining slots: 15 to the
        // -I side's dur (its F2 peek digests it), 33 to the dose key.
        let mut a = HashMap::new();
        a.insert(
            "m".into(),
            alias("switch tv -I adventure time , occur c -k dose="),
        );
        let r = expand_into_sides(v(&["datum", "m", "15", "33"]), &MapResolver(&a)).unwrap();
        assert!(r.sides[0].iter().any(|t| t == "15"), "{:?}", r.sides[0]);
        assert!(!r.sides[0].iter().any(|t| t == "33"), "{:?}", r.sides[0]);
        assert!(
            r.sides[1].iter().any(|t| t == "dose=33"),
            "{:?}",
            r.sides[1]
        );
    }

    #[test]
    fn stacked_value_flag_in_trailing_is_not_misallocated() {
        // Review (#45): `-qqt 3` in the call-site trailing must expand to
        // `-q -q -t 3` BEFORE allocation, so `3` stays -t's value and
        // only `5` walks into the dur slot.
        let r = expand(&["datum", "scaff", "-qqt", "3", "5"]);
        assert_eq!(r.sides.len(), 2);
        assert!(r.sides[0].iter().any(|t| t == "5"), "{:?}", r.sides[0]);
        assert!(!r.sides[1].iter().any(|t| t == "5"), "{:?}", r.sides[1]);
        // The time flags — with their value — promote to both sides.
        for side in &r.sides {
            let (t, _) = extract_time(side);
            assert_eq!(t.quick, 2, "{side:?}");
            assert_eq!(t.time.as_deref(), Some("3"), "{side:?}");
        }
    }
}
