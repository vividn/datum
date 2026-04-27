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
    "occur", "start", "stop", "end", "switch", "add",
    "tail", "head", "now", "spec", "alias", "migrate", "get", "delete", "del", "undo", "help",
    "history", "why",
    "track", "ignore", "grep", "check", "count", "ids",
    "update", "merge", "rekey", "edit", "retime", "rt",
    "backup", "restore", "dayview",
    "info", "setup", "config", "sync",
    "conflicts",
];

/// Terminal verb tokens the override pre-pass recognises. `end`/`stop`
/// are synonymous — `<field> end` and `<field> stop` both rewrite to
/// `end <field>`.
pub const TERMINAL_VERBS: &[&str] = &["end", "stop"];

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
/// applied (for provenance stamping onto the entry).
pub fn expand_argv(
    argv: Vec<String>,
    resolver: &dyn Resolver,
) -> (Vec<String>, Vec<AliasStep>) {
    let argv = apply_terminal_verb_override(argv);
    let (argv, chain) = try_expand_alias(argv, resolver);
    let argv = maybe_swap_field_verb(argv);
    let argv = expand_stacked_flags(argv);
    (argv, chain)
}

/// Like [`expand_argv`] but skips alias resolution (no resolver required).
pub fn expand_argv_no_alias(argv: Vec<String>) -> Vec<String> {
    let argv = apply_terminal_verb_override(argv);
    let argv = maybe_swap_field_verb(argv);
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

/// Resolve argv[1] as an alias from the resolver, recording each
/// expansion step. On any failure returns argv unchanged (empty chain).
pub fn try_expand_alias(
    mut argv: Vec<String>,
    resolver: &dyn Resolver,
) -> (Vec<String>, Vec<AliasStep>) {
    let mut chain: Vec<AliasStep> = Vec::new();
    for _ in 0..MAX_ALIAS_HOPS {
        if argv.len() < 2 || KNOWN_SUBCOMMANDS.contains(&argv[1].as_str()) {
            return (argv, chain);
        }
        let name = argv[1].clone();
        match resolver.lookup(&name) {
            Some(alias) => {
                chain.push(AliasStep {
                    name: name.clone(),
                    expansion: alias.expands_to.clone(),
                });
                let mut result = vec![argv[0].clone()];
                result.extend(alias.tokens);
                result.extend(argv[2..].iter().cloned());
                argv = result;
            }
            None => return (argv, chain),
        }
    }
    (argv, chain)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn no_alias_with_known_subcommand() {
        let (argv, chain) = expand_argv(v(&["datum", "occur", "coffee"]), &NoResolver);
        assert_eq!(argv, v(&["datum", "occur", "coffee"]));
        assert!(chain.is_empty());
    }

    #[test]
    fn field_verb_swap() {
        let (argv, _) = expand_argv(v(&["datum", "mood", "switch", "happy"]), &NoResolver);
        assert_eq!(argv, v(&["datum", "switch", "mood", "happy"]));
    }

    #[test]
    fn terminal_verb_override() {
        let (argv, _) = expand_argv(v(&["datum", "work", "end"]), &NoResolver);
        assert_eq!(argv, v(&["datum", "end", "work"]));
    }

    #[test]
    fn stacked_flags_expand() {
        let (argv, _) = expand_argv(v(&["datum", "occur", "coffee", "-yy"]), &NoResolver);
        assert_eq!(argv, v(&["datum", "occur", "coffee", "-y", "-y"]));
    }

    #[test]
    fn stacked_with_terminal_flag() {
        // `-yyt 3` should mean `-y -y -t 3` (two days back at 3am).
        let (argv, _) = expand_argv(
            v(&["datum", "occur", "coffee", "-yyt", "3"]),
            &NoResolver,
        );
        assert_eq!(argv, v(&["datum", "occur", "coffee", "-y", "-y", "-t", "3"]));

        // Single stackable + terminal also works: `-yt 3` = `-y -t 3`.
        let (argv, _) = expand_argv(v(&["datum", "occur", "x", "-yt", "3"]), &NoResolver);
        assert_eq!(argv, v(&["datum", "occur", "x", "-y", "-t", "3"]));

        // Long stacked + terminal: `-qqqd -1` = `-q -q -q -d -1`.
        let (argv, _) = expand_argv(v(&["datum", "occur", "x", "-qqqd", "-1"]), &NoResolver);
        assert_eq!(
            argv,
            v(&["datum", "occur", "x", "-q", "-q", "-q", "-d", "-1"])
        );

        // Long flags untouched.
        let (argv, _) = expand_argv(v(&["datum", "occur", "x", "--help"]), &NoResolver);
        assert_eq!(argv, v(&["datum", "occur", "x", "--help"]));

        // Non-stackable cluster left alone (parser handles).
        let (argv, _) = expand_argv(v(&["datum", "occur", "x", "-tD"]), &NoResolver);
        assert_eq!(argv, v(&["datum", "occur", "x", "-tD"]));
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
        let (argv, chain) = expand_argv(v(&["datum", "c", "2"]), &MapResolver(&aliases));
        assert_eq!(argv, v(&["datum", "occur", "coffee", "2"]));
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].name, "c");
        assert_eq!(chain[0].expansion, "occur coffee");
    }
}
