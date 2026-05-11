//! Alias-expansion pre-pass.
//!
//! Ported from `datum-commands::args::try_expand_alias*`. The logic is
//! DB-agnostic: callers implement [`Resolver`] for their backing store
//! (SQLite on native, in-memory map in tests and in the WASM frontend).
//!
//! The expansion is argv-level: given the raw argv (e.g.
//! `["datum", "vbp", "correr"]`), we look up `argv[1]` in the resolver.
//! If it's an alias, we splice the expansion in, keeping argv[0]
//! intact. We keep looping until we hit a known subcommand, a missing
//! alias, or [`MAX_ALIAS_HOPS`].
//!
//! ## Positional placeholders
//!
//! An alias's `expands_to` may reference positional arguments via `$N`
//! or `${N}` (1-based). At expansion time, the highest referenced index
//! is consumed from the trailing argv and substituted into the alias
//! tokens before the remaining args are spliced in. Example:
//!
//! ```text
//! alias 'chore' → 'alias set $1 "occur -k next_date done=true $1"'
//! datum chore laundry  →  datum alias set laundry "occur -k next_date done=true laundry"
//! ```
//!
//! Substitution applies inside tokens (so `$1` inside a quoted string
//! resolves correctly). If `$N` references a position with no
//! corresponding argv entry, expansion fails with [`ParseError::InvalidArg`].

/// Maximum number of alias-expansion hops before we give up (guard
/// against user-defined cycles).
pub const MAX_ALIAS_HOPS: usize = 16;

/// One resolved alias: the definition we got back from the resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAlias {
    /// The expansion string, as stored by the user (e.g.
    /// `"note word srcLang=pt pos=verb"`).
    pub expands_to: String,
    /// Already-tokenised version of `expands_to`, spliced into argv in
    /// place of the alias name. `datum-commands` supplies this via a
    /// shell-style splitter.
    pub tokens: Vec<String>,
}

/// Lookup interface for alias resolution. Implementors return the
/// resolved alias or `None` if the name is not aliased. Errors are
/// swallowed (treated as "no alias") to match the pre-R2 behaviour.
pub trait Resolver {
    fn lookup(&self, name: &str) -> Option<ResolvedAlias>;
}

/// Always returns `None` — used by callers that want to skip alias
/// resolution entirely (e.g. when the DB is not yet open).
pub struct NoResolver;
impl Resolver for NoResolver {
    fn lookup(&self, _name: &str) -> Option<ResolvedAlias> {
        None
    }
}

/// Trivial in-memory resolver. Useful in tests and in the web frontend.
pub struct MapResolver<'a>(pub &'a std::collections::HashMap<String, ResolvedAlias>);
impl<'a> Resolver for MapResolver<'a> {
    fn lookup(&self, name: &str) -> Option<ResolvedAlias> {
        self.0.get(name).cloned()
    }
}

/// Known subcommand names we stop expanding at. Kept in one place so
/// both datum-parse and datum-commands agree.
pub const KNOWN_SUBCOMMANDS: &[&str] = &[
    "occur",
    "start",
    "stop",
    "end",
    "switch",
    "add",
    "tail",
    "head",
    "now",
    "spec",
    "alias",
    "get",
    "delete",
    "del",
    "undo",
    "help",
    "history",
    "why",
    "track",
    "ignore",
    "grep",
    "check",
    "count",
    "ids",
    "update",
    "merge",
    "rekey",
    "edit",
    "retime",
    "rt",
    "dayview",
    "info",
    "db",
    "config",
    "sync",
    "conflicts",
    "reparse",
];

/// Terminal verb tokens the override pre-pass recognises. `end`/`stop`
/// are synonymous — `<field> end` and `<field> stop` both rewrite to
/// `end <field>`.
pub const TERMINAL_VERBS: &[&str] = &["end", "stop"];

/// Verbs that, when supplied as the state argument to `switch`, promote
/// the entire entry to that verb instead of being recorded as a literal
/// state string. `switch <field> end|stop|start` is rewritten to
/// `end|stop|start <field>` so the user's intent ("end the switch
/// block") routes through the correct verb body.
pub const SWITCH_PROMOTE_VERBS: &[&str] = &["end", "stop", "start"];

/// A single alias-expansion event. Rendered onto the entry's
/// `alias_chain` column at write time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasStep {
    pub name: String,
    pub expansion: String,
}

/// Apply the full pre-pass pipeline:
/// 1. Terminal-verb override: `<field> end|stop` → `end|stop <field>`.
/// 2. Alias expansion (DB-backed via [`Resolver`]).
/// 3. Field/verb swap: `<field> <verb>` → `<verb> <field>`.
/// 4. Stacked short-flag expansion: `-yy` → `-y -y`.
///
/// Returns the expanded argv plus the list of alias steps that were
/// applied (for provenance stamping onto the entry). Returns
/// [`ParseError::InvalidArg`] if a positional placeholder (`$N`) in an
/// alias expansion references a slot with no argv value.
pub fn expand_argv(
    argv: Vec<String>,
    resolver: &dyn Resolver,
) -> crate::Result<(Vec<String>, Vec<AliasStep>)> {
    let argv = apply_terminal_verb_override(argv);
    let (argv, chain) = try_expand_alias(argv, resolver)?;
    let argv = maybe_swap_field_verb(argv);
    let argv = apply_switch_promote_override(argv);
    let argv = expand_stacked_flags(argv);
    Ok((argv, chain))
}

/// Like [`expand_argv`] but skips alias resolution (no resolver required).
pub fn expand_argv_no_alias(argv: Vec<String>) -> Vec<String> {
    let argv = apply_terminal_verb_override(argv);
    let argv = maybe_swap_field_verb(argv);
    let argv = apply_switch_promote_override(argv);
    expand_stacked_flags(argv)
}

/// Rewrite `datum <field> end [args...]` → `datum end <field> [args...]`.
pub fn apply_terminal_verb_override(mut argv: Vec<String>) -> Vec<String> {
    if argv.len() < 3 {
        return argv;
    }
    let terminal_is_second = TERMINAL_VERBS.contains(&argv[2].as_str());
    let first_is_known_subcommand = KNOWN_SUBCOMMANDS.contains(&argv[1].as_str());
    if terminal_is_second && !first_is_known_subcommand {
        argv.swap(1, 2);
    }
    argv
}

/// Rewrite `datum switch <field> <verb> [args...]` → `datum <verb> <field>
/// [args...]` where `<verb>` ∈ [`SWITCH_PROMOTE_VERBS`] (currently
/// `end`/`stop`/`start`) and `<field>` is not itself a known subcommand.
/// Without this rewrite, the switch parser would seal `end`/`stop`/`start`
/// as a literal state string rather than routing through the verb body.
pub fn apply_switch_promote_override(argv: Vec<String>) -> Vec<String> {
    if argv.len() < 4 {
        return argv;
    }
    if argv[1] != "switch" {
        return argv;
    }
    if KNOWN_SUBCOMMANDS.contains(&argv[2].as_str()) {
        return argv;
    }
    if !SWITCH_PROMOTE_VERBS.contains(&argv[3].as_str()) {
        return argv;
    }
    let mut iter = argv.into_iter();
    let bin = iter.next().expect("argv[0]");
    let _switch = iter.next();
    let field = iter.next().expect("argv[2]");
    let verb = iter.next().expect("argv[3]");
    let mut out = Vec::with_capacity(iter.size_hint().0 + 3);
    out.push(bin);
    out.push(verb);
    out.push(field);
    out.extend(iter);
    out
}

/// Resolve argv[1] as an alias from the resolver, recording each
/// expansion step. Stops on a known subcommand or missing alias.
/// Returns [`ParseError::InvalidArg`] if a positional placeholder
/// references an argv slot that does not exist, or
/// [`ParseError::AliasCycle`] if expansion revisits a previously
/// expanded name (e.g. `set a 'b'; set b 'a'; datum a`).
pub fn try_expand_alias(
    mut argv: Vec<String>,
    resolver: &dyn Resolver,
) -> crate::Result<(Vec<String>, Vec<AliasStep>)> {
    let mut chain: Vec<AliasStep> = Vec::new();
    let mut visited: Vec<String> = Vec::new();
    for _ in 0..MAX_ALIAS_HOPS {
        if argv.len() < 2 || KNOWN_SUBCOMMANDS.contains(&argv[1].as_str()) {
            return Ok((argv, chain));
        }
        let name = argv[1].clone();
        if visited.iter().any(|v| v == &name) {
            let mut path = visited;
            path.push(name);
            return Err(crate::error::ParseError::AliasCycle(path));
        }
        match resolver.lookup(&name) {
            Some(alias) => {
                let trailing = &argv[2..];
                let max_n = max_placeholder(&alias.tokens);
                if max_n > trailing.len() {
                    return Err(crate::error::ParseError::InvalidArg(format!(
                        "alias '{name}': $${max_n} but only {} positional arg(s) provided",
                        trailing.len()
                    )));
                }
                let consumed: Vec<String> = trailing[..max_n].to_vec();
                let remaining: Vec<String> = trailing[max_n..].to_vec();
                let substituted: Vec<String> = alias
                    .tokens
                    .iter()
                    .map(|t| substitute_placeholders(t, &consumed))
                    .collect();
                let expansion = if max_n == 0 {
                    alias.expands_to.clone()
                } else {
                    shell_join(&substituted)
                };
                chain.push(AliasStep {
                    name: name.clone(),
                    expansion,
                });
                visited.push(name);

                // Apply this alias's `--` semantics over its body + the
                // tail of `remaining` that's still inside this alias's
                // scope (i.e. up to the first standalone comma that
                // belongs to an *outer* alias's split). See da-gh0
                // Rules 4/8/13: post-`--` tokens promote to all sides of
                // *this* alias's body, and the alias's implicit
                // body-end `--` makes its trailing args do the same.
                let (real_trailing, post_trailing) = split_at_first_comma(remaining);
                let distributed = if substituted.iter().any(|t| t == MULTI_COMMA) {
                    apply_alias_dd_distribution(substituted, real_trailing)?
                } else {
                    // Single-side body: literal `--`s inside are scoped
                    // to this alias's only side. Consume them — the
                    // tokens after `--` simply continue on this side.
                    let mut out = consume_inner_double_dash(substituted);
                    out.extend(real_trailing);
                    out
                };
                let mut result = vec![argv[0].clone()];
                result.extend(distributed);
                result.extend(post_trailing);
                argv = result;
            }
            None => return Ok((argv, chain)),
        }
    }
    Ok((argv, chain))
}

/// Split `tokens` into (before-first-standalone-comma, comma-and-after).
/// Used by [`try_expand_alias`] to determine how much of an alias's
/// trailing args belong to *this* alias's scope (everything before the
/// first comma — anything past it is an outer-alias side boundary).
fn split_at_first_comma(tokens: Vec<String>) -> (Vec<String>, Vec<String>) {
    match tokens.iter().position(|t| t == MULTI_COMMA) {
        Some(idx) => {
            let mut iter = tokens.into_iter();
            let before: Vec<String> = iter.by_ref().take(idx).collect();
            let from: Vec<String> = iter.collect();
            (before, from)
        }
        None => (tokens, Vec::new()),
    }
}

/// Apply this alias's `--` distribution over its multi-sided body and
/// trailing call-site args. Implements Rule 4/13 inside a single
/// alias's scope:
///
/// 1. Comma-split the body into N sides.
/// 2. For each side, partition at the first literal `--` token.
///    Tokens before stay local; tokens after collect into the
///    "promoted" bucket.
/// 3. Append `trailing` to the promoted bucket (the implicit
///    body-end `--` — Rule 13).
/// 4. Append the promoted bucket to every side. Rejoin with `,`.
///
/// The result is a flat token sequence with comma separators but no
/// `--` markers — this alias's `--` semantics have been consumed.
/// Outer alias scopes (Rule 8) only see clean tokens.
fn apply_alias_dd_distribution(
    body: Vec<String>,
    trailing: Vec<String>,
) -> crate::Result<Vec<String>> {
    let sides = split_argv_at_commas(body)?;
    let mut local_sides: Vec<Vec<String>> = Vec::with_capacity(sides.len());
    let mut promoted: Vec<String> = Vec::new();
    for side in sides {
        match side.iter().position(|t| t == "--") {
            Some(idx) => {
                let mut iter = side.into_iter();
                let local: Vec<String> = iter.by_ref().take(idx).collect();
                let _dash = iter.next();
                promoted.extend(iter);
                local_sides.push(local);
            }
            None => local_sides.push(side),
        }
    }
    // Strip stray `--` markers from trailing — at this layer everything
    // in `trailing` is already post-implicit-body-end-DD by Rule 13, so
    // an explicit user-typed `--` at the call site is idempotent.
    promoted.extend(trailing.into_iter().filter(|t| t != "--"));

    let mut out: Vec<String> = Vec::new();
    for (i, mut side) in local_sides.into_iter().enumerate() {
        if i > 0 {
            out.push(MULTI_COMMA.to_string());
        }
        side.extend(promoted.iter().cloned());
        out.extend(side);
    }
    Ok(out)
}

/// Remove literal `--` tokens from a single-side alias body. Rule 8:
/// `--` inside an inner alias's body doesn't propagate to outer multi
/// sides. For a single-side alias the post-`--` tokens were already on
/// the only side, so consuming the marker is a no-op semantically —
/// it just keeps the marker from leaking into outer scope.
fn consume_inner_double_dash(body: Vec<String>) -> Vec<String> {
    body.into_iter().filter(|t| t != "--").collect()
}

/// Highest `$N` (or `${N}`) placeholder index referenced across the
/// given tokens. Returns 0 when no placeholder is present.
fn max_placeholder(tokens: &[String]) -> usize {
    let mut max_n = 0usize;
    for t in tokens {
        scan_placeholders(t, |n| {
            if n > max_n {
                max_n = n;
            }
        });
    }
    max_n
}

/// Substitute `$N` / `${N}` references in `s` using `args` (1-indexed).
/// Indices outside `1..=args.len()` are left as literal text. Callers
/// must validate via [`max_placeholder`] before calling if they want to
/// reject out-of-range references.
fn substitute_placeholders(s: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let Some(&(_, next)) = chars.peek() else {
            out.push('$');
            continue;
        };
        if next == '{' {
            chars.next();
            let mut digits = String::new();
            let mut closed = false;
            while let Some(&(_, c2)) = chars.peek() {
                if c2 == '}' {
                    chars.next();
                    closed = true;
                    break;
                }
                if !c2.is_ascii_digit() {
                    break;
                }
                digits.push(c2);
                chars.next();
            }
            if closed && !digits.is_empty() {
                if let Ok(n) = digits.parse::<usize>() {
                    if (1..=args.len()).contains(&n) {
                        out.push_str(&args[n - 1]);
                        continue;
                    }
                }
            }
            out.push('$');
            out.push('{');
            out.push_str(&digits);
            if closed {
                out.push('}');
            }
        } else if next.is_ascii_digit() {
            let mut digits = String::new();
            while let Some(&(_, c2)) = chars.peek() {
                if !c2.is_ascii_digit() {
                    break;
                }
                digits.push(c2);
                chars.next();
            }
            if let Ok(n) = digits.parse::<usize>() {
                if (1..=args.len()).contains(&n) {
                    out.push_str(&args[n - 1]);
                    continue;
                }
            }
            out.push('$');
            out.push_str(&digits);
        } else {
            out.push('$');
        }
    }
    out
}

/// Scan `s` for `$N` / `${N}` placeholders, invoking `f` with each
/// numeric index found. `$0` and malformed `${...}` runs are ignored.
fn scan_placeholders(s: &str, mut f: impl FnMut(usize)) {
    let mut chars = s.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c != '$' {
            continue;
        }
        let Some(&(_, next)) = chars.peek() else {
            continue;
        };
        if next == '{' {
            chars.next();
            let mut digits = String::new();
            let mut closed = false;
            while let Some(&(_, c2)) = chars.peek() {
                if c2 == '}' {
                    chars.next();
                    closed = true;
                    break;
                }
                if !c2.is_ascii_digit() {
                    break;
                }
                digits.push(c2);
                chars.next();
            }
            if closed {
                if let Ok(n) = digits.parse::<usize>() {
                    if n >= 1 {
                        f(n);
                    }
                }
            }
        } else if next.is_ascii_digit() {
            let mut digits = String::new();
            while let Some(&(_, c2)) = chars.peek() {
                if !c2.is_ascii_digit() {
                    break;
                }
                digits.push(c2);
                chars.next();
            }
            if let Ok(n) = digits.parse::<usize>() {
                if n >= 1 {
                    f(n);
                }
            }
        }
    }
}

/// Swap argv[1] and argv[2] when argv[1] is a field name and argv[2] is
/// a verb. E.g. `datum mood switch happy` → `datum switch mood happy`.
pub fn maybe_swap_field_verb(mut argv: Vec<String>) -> Vec<String> {
    if argv.len() >= 3
        && !KNOWN_SUBCOMMANDS.contains(&argv[1].as_str())
        && KNOWN_SUBCOMMANDS.contains(&argv[2].as_str())
    {
        argv.swap(1, 2);
    }
    argv
}

/// Expand stacked short flags. Same-letter runs (`-yy`, `-qqq`) and
/// stackable-prefix-plus-terminal forms (`-yyt`, `-qqqd`, `-yt`) both
/// split into their constituent single-letter flags. The terminal char
/// can be any short flag — the parser validates it downstream.
///
/// Also handles datum's `-t<t|q>+` recent-past shorthand: a leading
/// `-t` followed by `t` (1 min) and/or `q` (5 min) characters sums the
/// trailing contributions into a minutes-ago offset and rewrites to
/// `-t -<minutes>`. Examples: `-tt` → `-t -1`, `-tqtt` → `-t -7`.
pub fn expand_stacked_flags(argv: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(argv.len() + 4);
    for arg in argv {
        if let Some(expanded) = expand_stacked(&arg) {
            out.extend(expanded);
        } else {
            out.push(arg);
        }
    }
    out
}

fn expand_stacked(s: &str) -> Option<Vec<String>> {
    if !s.starts_with('-') || s.starts_with("--") || s.len() < 3 {
        return None;
    }
    let inner = &s[1..];
    let chars: Vec<char> = inner.chars().collect();

    // `-t<t|q>+` recent-past shorthand: leading `-t` is the flag, each
    // trailing `t` adds 1 min and each `q` adds 5 min. Sum of contributions
    // becomes a negative minutes-offset value attached to `-t`.
    if chars[0] == 't' && chars[1..].iter().all(|c| matches!(c, 't' | 'q')) {
        let minutes: u32 = chars[1..]
            .iter()
            .map(|c| if *c == 'q' { 5 } else { 1 })
            .sum();
        return Some(vec!["-t".to_string(), format!("-{minutes}")]);
    }

    let stackable = |c: char| matches!(c, 'y' | 'q');
    let prefix_len = chars.iter().take_while(|c| stackable(**c)).count();

    // All-stackable run (e.g. `-yy`, `-qqq`).
    if prefix_len == chars.len() {
        return Some(chars.iter().map(|c| format!("-{c}")).collect());
    }

    // Stackable prefix followed by exactly one terminal char
    // (e.g. `-yyt` → `-y -y -t`, `-qqqd` → `-q -q -q -d`, `-yt` → `-y -t`).
    if prefix_len >= 1 && prefix_len == chars.len() - 1 {
        return Some(chars.iter().map(|c| format!("-{c}")).collect());
    }

    None
}

/// Inverse of [`shell_split`]: join tokens into a single shell-style
/// string that round-trips back through `shell_split`. Tokens that
/// contain whitespace, quotes, or backslashes are wrapped in double
/// quotes with `\` and `"` escaped; the rest are emitted bare. Used by
/// the alias-flag pre-pass when persisting a canonical command form.
pub fn shell_join(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|t| shell_quote_token(t))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote_token(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".to_string();
    }
    let needs_quoting = s.chars().any(|c| {
        c.is_whitespace()
            || matches!(
                c,
                '\'' | '"'
                    | '\\'
                    | '$'
                    | '`'
                    | '*'
                    | '?'
                    | '#'
                    | '|'
                    | '&'
                    | ';'
                    | '<'
                    | '>'
                    | '('
                    | ')'
            )
    });
    if !needs_quoting {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '"' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Shell-style split for alias expansion strings (e.g. when reading the
/// alias's `expands_to` column). Single and double quotes, backslash
/// escapes; no env-var expansion. Matches the minimal splitter in
/// `crate::parse`.
pub fn shell_split(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    let mut has_tok = false;
    while let Some(c) = chars.next() {
        if in_single {
            if c == '\'' {
                in_single = false;
            } else {
                cur.push(c);
            }
        } else if in_double {
            if c == '"' {
                in_double = false;
            } else if c == '\\' {
                if let Some(nc) = chars.next() {
                    cur.push(nc);
                }
            } else {
                cur.push(c);
            }
        } else {
            match c {
                '\'' => {
                    in_single = true;
                    has_tok = true;
                }
                '"' => {
                    in_double = true;
                    has_tok = true;
                }
                '\\' => {
                    if let Some(nc) = chars.next() {
                        cur.push(nc);
                        has_tok = true;
                    }
                }
                c if c.is_whitespace() => {
                    if has_tok {
                        out.push(std::mem::take(&mut cur));
                        has_tok = false;
                    }
                }
                _ => {
                    cur.push(c);
                    has_tok = true;
                }
            }
        }
    }
    if has_tok {
        out.push(cur);
    }
    out
}

/// The standalone-comma token that triggers a multi-entry split.
/// Whitespace-bounded `,` arrives from [`shell_split`] (or its `parse.rs`
/// twin) as its own argv entry. Non-bounded commas — `vertical,sit`,
/// `vertical ,sit`, `key=a,b` — stay glued to their neighbouring chars
/// and do not trigger.
pub const MULTI_COMMA: &str = ",";

/// Split an argv (or any token slice) at standalone-comma tokens into N
/// sides. Implements the parser side of Rules 6, 7, 14 from `da-gh0`:
///
/// * **Rule 6** — only a token equal to [`MULTI_COMMA`] triggers a
///   split. Commas glued to surrounding chars stay inside their token.
/// * **Rule 7** — repeated invocation flattens multi-of-multi (callers
///   that re-expand alias bodies just call this again on the spliced
///   result; the splitter has no notion of nesting).
/// * **Rule 14** — the splitter is a top-level parser feature, not
///   alias-specific. Callers wire it in at every argv-rewrite seam.
///
/// Returns the per-side token vectors in order. Returns
/// [`ParseError::InvalidArg`] when any side is empty (leading, trailing,
/// or middle `,` with no tokens on one side), matching the
/// `empty_*_side_errors` cases in section M of the bead's test corpus.
///
/// **Single-side passthrough:** when argv contains no standalone-comma
/// token the result is `Ok(vec![argv])` — the original argv wrapped in a
/// one-element outer vec. Callers can branch on `sides.len() == 1` to
/// take the existing single-entry code path.
pub fn split_argv_at_commas(argv: Vec<String>) -> crate::Result<Vec<Vec<String>>> {
    let has_comma = argv.iter().any(|t| t == MULTI_COMMA);
    if !has_comma {
        return Ok(vec![argv]);
    }
    let mut sides: Vec<Vec<String>> = Vec::new();
    let mut cur: Vec<String> = Vec::new();
    for tok in argv {
        if tok == MULTI_COMMA {
            if cur.is_empty() {
                return Err(crate::error::ParseError::InvalidArg(
                    "empty side in multi-entry invocation (standalone `,` with no tokens on \
                     one side)"
                        .into(),
                ));
            }
            sides.push(std::mem::take(&mut cur));
        } else {
            cur.push(tok);
        }
    }
    if cur.is_empty() {
        return Err(crate::error::ParseError::InvalidArg(
            "empty trailing side in multi-entry invocation (argv ends with standalone `,`)".into(),
        ));
    }
    sides.push(cur);
    Ok(sides)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    // ─── da-gh0 comma trigger (Rules 6, 7, 14; Section A; Section M-empty) ──

    #[test]
    fn comma_whitespace_bounded_triggers_multi() {
        // After shell_split, "vertical , sit" → ["vertical", ",", "sit"].
        let sides = split_argv_at_commas(v(&["vertical", ",", "sit"])).unwrap();
        assert_eq!(sides, vec![v(&["vertical"]), v(&["sit"])]);
    }

    #[test]
    fn comma_no_whitespace_does_not_trigger() {
        // "vertical,sit" → single token; no split.
        let sides = split_argv_at_commas(v(&["vertical,sit"])).unwrap();
        assert_eq!(sides, vec![v(&["vertical,sit"])]);
    }

    #[test]
    fn comma_one_sided_whitespace_does_not_trigger() {
        // "vertical ,sit" → ["vertical", ",sit"]; "vertical, sit" → ["vertical,", "sit"].
        // Neither token equals "," so no split.
        let sides = split_argv_at_commas(v(&["vertical", ",sit"])).unwrap();
        assert_eq!(sides, vec![v(&["vertical", ",sit"])]);
        let sides = split_argv_at_commas(v(&["vertical,", "sit"])).unwrap();
        assert_eq!(sides, vec![v(&["vertical,", "sit"])]);
    }

    #[test]
    fn comma_three_sides_parses_as_three() {
        let sides = split_argv_at_commas(v(&["vertical", ",", "sit", ",", "supine"])).unwrap();
        assert_eq!(sides, vec![v(&["vertical"]), v(&["sit"]), v(&["supine"])]);
    }

    #[test]
    fn comma_top_level_cli_no_alias_required() {
        // Rule 14: direct invocation `dm vertical , sit` splits without any
        // alias defined — the splitter is alias-agnostic.
        let sides = split_argv_at_commas(v(&["vertical", ",", "sit"])).unwrap();
        assert_eq!(sides.len(), 2);
    }

    #[test]
    fn comma_inside_value_does_not_trigger_multi() {
        // `key=a,b` is a single shell token; embedded comma is not standalone.
        let sides = split_argv_at_commas(v(&["occur", "field", "key=a,b"])).unwrap();
        assert_eq!(sides, vec![v(&["occur", "field", "key=a,b"])]);
    }

    #[test]
    fn comma_no_commas_passes_through_as_single_side() {
        let sides = split_argv_at_commas(v(&["occur", "coffee"])).unwrap();
        assert_eq!(sides, vec![v(&["occur", "coffee"])]);
    }

    #[test]
    fn empty_leading_side_errors() {
        // `, sit` → ["", ",", "sit"] is impossible from shell_split; the
        // realistic form is argv = [",", "sit"] (leading standalone comma).
        let err = split_argv_at_commas(v(&[",", "sit"])).unwrap_err();
        assert!(format!("{err:?}").contains("empty side"));
    }

    #[test]
    fn empty_trailing_side_errors() {
        let err = split_argv_at_commas(v(&["sit", ","])).unwrap_err();
        assert!(format!("{err:?}").contains("empty trailing side"));
    }

    #[test]
    fn empty_middle_side_errors() {
        let err = split_argv_at_commas(v(&["a", ",", ",", "b"])).unwrap_err();
        assert!(format!("{err:?}").contains("empty side"));
    }

    #[test]
    fn multi_of_multi_flattens_when_callers_reapply() {
        // Rule 7 / Section K: callers that re-splice alias bodies just
        // re-invoke the splitter on the new argv. The splitter itself
        // has no nesting — flattening falls out of running it twice.
        // Simulate: alias `y = x , c` where `x = a , b` expands to
        // argv = ["a", ",", "b", ",", "c"] after substitution.
        let sides = split_argv_at_commas(v(&["a", ",", "b", ",", "c"])).unwrap();
        assert_eq!(sides, vec![v(&["a"]), v(&["b"]), v(&["c"])]);
    }

    // ─── existing alias pre-pass tests ──────────────────────────────────────

    #[test]
    fn no_alias_with_known_subcommand() {
        let (argv, chain) = expand_argv(v(&["datum", "occur", "coffee"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "coffee"]));
        assert!(chain.is_empty());
    }

    #[test]
    fn field_verb_swap() {
        let (argv, _) = expand_argv(v(&["datum", "mood", "switch", "happy"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "switch", "mood", "happy"]));
    }

    #[test]
    fn terminal_verb_override() {
        let (argv, _) = expand_argv(v(&["datum", "work", "end"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "end", "work"]));
    }

    #[test]
    fn switch_promote_end() {
        let (argv, _) =
            expand_argv(v(&["datum", "switch", "project", "end"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "end", "project"]));
    }

    #[test]
    fn switch_promote_stop() {
        let (argv, _) =
            expand_argv(v(&["datum", "switch", "project", "stop"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "stop", "project"]));
    }

    #[test]
    fn switch_promote_start() {
        let (argv, _) =
            expand_argv(v(&["datum", "switch", "project", "start"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "start", "project"]));
    }

    #[test]
    fn switch_promote_preserves_trailing_args() {
        let (argv, _) = expand_argv(
            v(&["datum", "switch", "project", "end", "-k", "score=10"]),
            &NoResolver,
        )
        .unwrap();
        assert_eq!(argv, v(&["datum", "end", "project", "-k", "score=10"]));
    }

    #[test]
    fn switch_promote_skips_non_reserved_state() {
        // `switch project happy` — `happy` is not reserved, so no rewrite.
        let (argv, _) =
            expand_argv(v(&["datum", "switch", "project", "happy"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "switch", "project", "happy"]));
    }

    #[test]
    fn switch_promote_via_field_verb_swap() {
        // `mood switch end` → swap → `switch mood end` → promote → `end mood`.
        let (argv, _) = expand_argv(v(&["datum", "mood", "switch", "end"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "end", "mood"]));
    }

    #[test]
    fn stacked_flags_expand() {
        let (argv, _) = expand_argv(v(&["datum", "occur", "coffee", "-yy"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "coffee", "-y", "-y"]));
    }

    #[test]
    fn stacked_with_terminal_flag() {
        // `-yyt 3` should mean `-y -y -t 3` (two days back at 3am).
        let (argv, _) =
            expand_argv(v(&["datum", "occur", "coffee", "-yyt", "3"]), &NoResolver).unwrap();
        assert_eq!(
            argv,
            v(&["datum", "occur", "coffee", "-y", "-y", "-t", "3"])
        );

        // Single stackable + terminal also works: `-yt 3` = `-y -t 3`.
        let (argv, _) = expand_argv(v(&["datum", "occur", "x", "-yt", "3"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "x", "-y", "-t", "3"]));

        // Long stacked + terminal: `-qqqd -1` = `-q -q -q -d -1`.
        let (argv, _) =
            expand_argv(v(&["datum", "occur", "x", "-qqqd", "-1"]), &NoResolver).unwrap();
        assert_eq!(
            argv,
            v(&["datum", "occur", "x", "-q", "-q", "-q", "-d", "-1"])
        );

        // Long flags untouched.
        let (argv, _) = expand_argv(v(&["datum", "occur", "x", "--help"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "x", "--help"]));

        // Non-stackable cluster left alone (parser handles).
        let (argv, _) = expand_argv(v(&["datum", "occur", "x", "-tD"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "x", "-tD"]));
    }

    #[test]
    fn t_flag_recent_past_shorthand() {
        // `-tt` = 1 min ago → `-t -1`.
        let (argv, _) = expand_argv(v(&["datum", "occur", "x", "-tt"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "x", "-t", "-1"]));

        // `-ttt` = 2 min ago → `-t -2`.
        let (argv, _) = expand_argv(v(&["datum", "occur", "x", "-ttt"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "x", "-t", "-2"]));

        // `-tttt` = 3 min ago → `-t -3`.
        let (argv, _) = expand_argv(v(&["datum", "occur", "x", "-tttt"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "x", "-t", "-3"]));

        // `-tqtt` = q+t+t = 7 min ago → `-t -7`.
        let (argv, _) = expand_argv(v(&["datum", "occur", "x", "-tqtt"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "x", "-t", "-7"]));

        // Trailing args follow the rewritten pair untouched.
        let (argv, _) =
            expand_argv(v(&["datum", "occur", "x", "-tqtt", "-k", "n"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "x", "-t", "-7", "-k", "n"]));

        // Bare `-t` is untouched (still consumes its positional value).
        let (argv, _) = expand_argv(v(&["datum", "occur", "x", "-t", "3"]), &NoResolver).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "x", "-t", "3"]));

        // `-qqqt` keeps the existing stackable+terminal split — leading `q`s
        // make this a `-q*` cluster, not the `-t<t|q>+` form.
        let (argv, _) =
            expand_argv(v(&["datum", "occur", "x", "-qqqt", "3"]), &NoResolver).unwrap();
        assert_eq!(
            argv,
            v(&["datum", "occur", "x", "-q", "-q", "-q", "-t", "3"])
        );
    }

    #[test]
    fn map_resolver_expansion() {
        let mut aliases = std::collections::HashMap::new();
        aliases.insert(
            "c".to_string(),
            ResolvedAlias {
                expands_to: "occur coffee".to_string(),
                tokens: v(&["occur", "coffee"]),
            },
        );
        let (argv, chain) = expand_argv(v(&["datum", "c", "2"]), &MapResolver(&aliases)).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "coffee", "2"]));
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].name, "c");
        assert_eq!(chain[0].expansion, "occur coffee");
    }

    fn alias(expands_to: &str) -> ResolvedAlias {
        ResolvedAlias {
            expands_to: expands_to.to_string(),
            tokens: shell_split(expands_to),
        }
    }

    #[test]
    fn placeholder_basic_dollar_one() {
        // `chore` → `alias set $1 "occur -k next_date done=true $1"`
        // `datum chore laundry` → `datum alias set laundry "occur -k next_date done=true laundry"`
        let mut aliases = std::collections::HashMap::new();
        aliases.insert(
            "chore".to_string(),
            alias("alias set $1 \"occur -k next_date done=true $1\""),
        );
        let (argv, chain) =
            expand_argv(v(&["datum", "chore", "laundry"]), &MapResolver(&aliases)).unwrap();
        assert_eq!(
            argv,
            v(&[
                "datum",
                "alias",
                "set",
                "laundry",
                "occur -k next_date done=true laundry",
            ])
        );
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].name, "chore");
        // Substituted form, not the original. Tokens with whitespace get re-quoted.
        assert!(
            chain[0].expansion.contains("laundry"),
            "expansion records substituted form: {}",
            chain[0].expansion
        );
        assert!(!chain[0].expansion.contains("$1"));
    }

    #[test]
    fn placeholder_multiple_positionals() {
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("pair".to_string(), alias("note word src=$1 tgt=$2"));
        let (argv, _) = expand_argv(
            v(&["datum", "pair", "pt", "en", "extra"]),
            &MapResolver(&aliases),
        )
        .unwrap();
        // $1=pt, $2=en consumed; "extra" passes through as trailing arg.
        assert_eq!(
            argv,
            v(&["datum", "note", "word", "src=pt", "tgt=en", "extra"])
        );
    }

    #[test]
    fn placeholder_repeated_reference_consumes_once() {
        // $1 appears twice but only consumes one positional arg.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("dup".to_string(), alias("occur $1 also=$1"));
        let (argv, _) =
            expand_argv(v(&["datum", "dup", "X", "Y"]), &MapResolver(&aliases)).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "X", "also=X", "Y"]));
    }

    #[test]
    fn placeholder_braced_form() {
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("br".to_string(), alias("occur ${1}_thing"));
        let (argv, _) = expand_argv(v(&["datum", "br", "foo"]), &MapResolver(&aliases)).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "foo_thing"]));
    }

    #[test]
    fn placeholder_missing_arg_errors() {
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("chore".to_string(), alias("alias set $1 \"x $1\""));
        let err = expand_argv(v(&["datum", "chore"]), &MapResolver(&aliases)).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("chore"),
            "msg should mention alias name: {msg}"
        );
        assert!(msg.contains("$1"), "msg should mention placeholder: {msg}");
    }

    #[test]
    fn placeholder_no_substitution_preserves_raw_expansion() {
        // No placeholders → chain expansion is the raw expands_to string,
        // matching pre-feature behavior.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("vbp".to_string(), alias("note word srcLang=pt pos=verb"));
        let (_, chain) =
            expand_argv(v(&["datum", "vbp", "correr"]), &MapResolver(&aliases)).unwrap();
        assert_eq!(chain[0].expansion, "note word srcLang=pt pos=verb");
    }

    #[test]
    fn placeholder_two_digit_index() {
        // ${10} should resolve to the 10th positional. Greedy digit match.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("ten".to_string(), alias("occur ${10}"));
        let args: Vec<&str> = vec![
            "datum", "ten", "1", "2", "3", "4", "5", "6", "7", "8", "9", "X",
        ];
        let (argv, _) = expand_argv(v(&args), &MapResolver(&aliases)).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "X"]));
    }

    #[test]
    fn placeholder_dollar_zero_left_literal() {
        // $0 has no positional meaning — left as literal text.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("z".to_string(), alias("occur $0"));
        let (argv, _) = expand_argv(v(&["datum", "z"]), &MapResolver(&aliases)).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "$0"]));
    }

    #[test]
    fn cycle_two_alias() {
        // a → b, b → a — must error, not stack-overflow or silently
        // truncate at MAX_ALIAS_HOPS.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("a".to_string(), alias("b"));
        aliases.insert("b".to_string(), alias("a"));
        let err = expand_argv(v(&["datum", "a"]), &MapResolver(&aliases)).unwrap_err();
        match err {
            crate::error::ParseError::AliasCycle(path) => {
                assert_eq!(path, v(&["a", "b", "a"]));
            }
            other => panic!("expected AliasCycle, got {other:?}"),
        }
    }

    #[test]
    fn cycle_self_reference() {
        // a → a — degenerate self-cycle.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("a".to_string(), alias("a"));
        let err = expand_argv(v(&["datum", "a"]), &MapResolver(&aliases)).unwrap_err();
        match err {
            crate::error::ParseError::AliasCycle(path) => {
                assert_eq!(path, v(&["a", "a"]));
            }
            other => panic!("expected AliasCycle, got {other:?}"),
        }
    }

    #[test]
    fn cycle_three_alias() {
        // a → b → c → a.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("a".to_string(), alias("b"));
        aliases.insert("b".to_string(), alias("c"));
        aliases.insert("c".to_string(), alias("a"));
        let err = expand_argv(v(&["datum", "a"]), &MapResolver(&aliases)).unwrap_err();
        match err {
            crate::error::ParseError::AliasCycle(path) => {
                assert_eq!(path, v(&["a", "b", "c", "a"]));
            }
            other => panic!("expected AliasCycle, got {other:?}"),
        }
    }

    #[test]
    fn cycle_mid_chain() {
        // a → b → c → b — cycle that doesn't revisit the start.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("a".to_string(), alias("b"));
        aliases.insert("b".to_string(), alias("c"));
        aliases.insert("c".to_string(), alias("b"));
        let err = expand_argv(v(&["datum", "a"]), &MapResolver(&aliases)).unwrap_err();
        match err {
            crate::error::ParseError::AliasCycle(path) => {
                assert_eq!(path, v(&["a", "b", "c", "b"]));
            }
            other => panic!("expected AliasCycle, got {other:?}"),
        }
    }

    #[test]
    fn cycle_error_message_renders_arrow_path() {
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("a".to_string(), alias("b"));
        aliases.insert("b".to_string(), alias("a"));
        let err = expand_argv(v(&["datum", "a"]), &MapResolver(&aliases)).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("Alias cycle"),
            "msg should label the failure: {msg}"
        );
        assert!(
            msg.contains("a → b → a"),
            "msg should render the path: {msg}"
        );
    }

    #[test]
    fn no_cycle_when_chain_terminates_at_subcommand() {
        // a → b → occur ... — legitimate two-step expansion, no cycle.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("a".to_string(), alias("b"));
        aliases.insert("b".to_string(), alias("occur coffee"));
        let (argv, chain) = expand_argv(v(&["datum", "a"]), &MapResolver(&aliases)).unwrap();
        assert_eq!(argv, v(&["datum", "occur", "coffee"]));
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].name, "a");
        assert_eq!(chain[1].name, "b");
    }

    #[test]
    fn cycle_through_placeholder_substitution() {
        // a → "b $1", b → "a $1" — placeholders consume positional args
        // but argv[1] still cycles between a and b.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("a".to_string(), alias("b $1"));
        aliases.insert("b".to_string(), alias("a $1"));
        let err = expand_argv(v(&["datum", "a", "x"]), &MapResolver(&aliases)).unwrap_err();
        assert!(matches!(err, crate::error::ParseError::AliasCycle(_)));
    }
}
