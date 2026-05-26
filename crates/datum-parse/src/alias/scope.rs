//! Side-scoped flag distribution + time-flag composition for
//! multi-entry aliases (da-gh0.D).
//!
//! The layer above [`super::expand::try_expand_alias`]: it expands all
//! aliases (including per-side aliases reached via comma split),
//! flattens multi-of-multi, applies any user-typed top-level `--`
//! semantics, and exposes a [`TimeFlags`] type that composes
//! alias-body-bound time flags with call-site time flags (Rule 15).
//!
//! Rules covered (from da-gh0):
//!
//! - **Rule 4** — Time flags are side-scoped by default; propagate to
//!   all sides only after `--` (implicit at body end, explicit at call
//!   site). Time-irrelevant verbs silently ignore time flags.
//! - **Rule 7** — Multi-of-multi flattens to N sides.
//! - **Rule 8** — `--` scope ends at the alias boundary. Inner alias's
//!   `--` consumed at expansion time (see
//!   [`super::expand::try_expand_alias`]).
//! - **Rule 13** — `--` is implicit at the end of an alias body.
//!   Call-site keys/flags apply to all sides.
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

/// Full alias-expansion + comma-split + `--` distribution pipeline.
///
/// Steps:
///
/// 1. Apply the terminal-verb override (`<field> end` → `end <field>`).
/// 2. Run [`try_expand_alias`] on the whole argv, which expands the
///    outermost alias and, if its body is multi-sided, distributes its
///    body's `--` and the trailing call-site args across its sides
///    (Rule 4/13). Inner aliases' internal `--`s are consumed at that
///    layer (Rule 8).
/// 3. Comma-split the resulting flat argv into N sides.
/// 4. Per side, run [`try_expand_alias`] again — the first token of a
///    side that wasn't argv\[1\] in step 2 still needs resolution
///    (e.g. `m = vertical , sit` expands `vertical` in step 2 but not
///    `sit`, which is on side 1).
/// 5. Flatten multi-of-multi: any side that, after expansion, contains
///    a standalone-comma token re-splits. Loops until convergent so
///    three-layer chains (`y = x , c` where `x = a , b`) flatten fully.
/// 6. Apply the call-site / user-typed top-level `--`: within each
///    side, partition at the first `--` token; post-`--` tokens from
///    every side concatenate into a single "promoted" bucket that
///    appends onto every side.
/// 7. Per side, apply the field/verb swap and stacked-short-flag
///    expansion.
pub fn expand_into_sides(argv: Vec<String>, resolver: &dyn Resolver) -> Result<ExpandedSides> {
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

    // Step 4 + 5: per-side alias expansion with multi-of-multi flattening.
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
                let sub = split_argv_at_commas(side)?;
                next_sides.extend(sub);
            } else {
                next_sides.push(side);
            }
        }
        sides = next_sides;
        if !grew_or_changed {
            break;
        }
    }

    // Step 6: top-level `--` distribution.
    let mut local_sides: Vec<Vec<String>> = Vec::with_capacity(sides.len());
    let mut promoted: Vec<String> = Vec::new();
    for side in sides {
        match side.iter().position(|t| t == "--") {
            Some(idx) => {
                let mut iter = side.into_iter();
                let local: Vec<String> = iter.by_ref().take(idx).collect();
                let _dash = iter.next();
                // Drop any further `--` tokens (idempotency).
                promoted.extend(iter.filter(|t| t != "--"));
                local_sides.push(local);
            }
            None => local_sides.push(side),
        }
    }

    // Step 7a (da-b4k5): per-side `maybe_swap_field_verb` so any
    // field-first form is normalized to verb-first BEFORE the
    // BlockSwitch-chain collapse runs — the collapse keys off
    // `sides[0][0] == "switch"`, which a pre-swap field-first side
    // (`["activity", "switch", "end", "14m"]`) wouldn't match.
    let mut swapped_sides: Vec<Vec<String>> = Vec::with_capacity(local_sides.len());
    for mut side in local_sides {
        side.extend(promoted.iter().cloned());
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
    fn keyed_after_explicit_double_dash_in_body_applies_to_all_sides_inside_alias() {
        // Explicit `--` in alias body → post tokens promote to all
        // sides of *this* alias.
        let mut a = HashMap::new();
        a.insert(
            "m".into(),
            alias("switch body sit , occur caffeine -- shared=Y"),
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
    fn keyed_split_per_side_then_promoted_after_dash() {
        // `vertical abc=1 , sit ghi=2 -- xyz=3`. User-typed top-level
        // `--`: per-side keys stay scoped; post-`--` promotes to all.
        let r = expand(&[
            "datum", "vertical", "abc=1", ",", "sit", "ghi=2", "--", "xyz=3",
        ]);
        assert_eq!(r.sides.len(), 2);
        assert!(r.sides[0].iter().any(|t| t == "abc=1"));
        assert!(!r.sides[0].iter().any(|t| t == "ghi=2"));
        assert!(r.sides[0].iter().any(|t| t == "xyz=3"));
        assert!(!r.sides[1].iter().any(|t| t == "abc=1"));
        assert!(r.sides[1].iter().any(|t| t == "ghi=2"));
        assert!(r.sides[1].iter().any(|t| t == "xyz=3"));
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
    fn explicit_double_dash_promotes_per_side_time_flag_to_all() {
        // `-t +5 -- -t +10`: per-side `-t +5` ends up on each side
        // local; `-t +10` after the `--` promotes to all. Composed
        // offset per side: +5 (side-local) + +10 (promoted) = +15.
        let r = expand(&[
            "datum", "vertical", "-t", "+5", ",", "caffeine", "--", "-t", "+10",
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

    // ── Section M subset ─────────────────────────────────────────────────

    #[test]
    fn extra_positional_no_slot_errors() {
        // `scaff` has slots [dur, doseMg]. Three positionals → extra.
        use super::super::body::{build_slot_list, parse_alias_body, walk_slots};
        let r = expand(&["datum", "scaff", "5", "100", "7"]);
        assert_eq!(r.sides.len(), 2);
        let sides: Vec<_> = r
            .sides
            .iter()
            .map(|s| parse_alias_body(s).unwrap())
            .collect();
        let slots = build_slot_list(&sides).unwrap();
        // Positionals come from the side argvs themselves; walk
        // through the explicit invocation positionals (`5`, `100`, `7`).
        let err = walk_slots(&sides, &slots, &v(&["5", "100", "7"])).unwrap_err();
        assert!(matches!(err, ParseError::ExtraData(_)));
    }

    #[test]
    fn extra_positional_past_dur_filled_errors() {
        // f12 = f1 , f2 abc. f1 has -k arg1, -k arg2=, -k arg3=def.
        // f2 has -k arg2 plus the bound positional `abc`. The bound
        // positional pre-fills f2's first slot. Then dur slots from
        // start/switch verbs append. Too many invocation positionals
        // errors.
        use super::super::body::{build_slot_list, parse_alias_body, walk_slots};
        let r = expand(&["datum", "f12", "val1", "val2", "val3", "7", "8"]);
        let sides: Vec<_> = r
            .sides
            .iter()
            .map(|s| parse_alias_body(s).unwrap())
            .collect();
        let slots = build_slot_list(&sides).unwrap();
        let err = walk_slots(&sides, &slots, &v(&["val1", "val2", "val3", "7", "8"])).unwrap_err();
        assert!(matches!(err, ParseError::ExtraData(_)));
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
}
