//! Matcher tokens for `datum get` search semantics (da-yhh1).
//!
//! Two equivalent surfaces — symbol form (discoverable, requires shell
//! quoting for `<`/`>`/`|`) and named form (`k=op=v`, shell-safe):
//!
//! | Symbol  | Named     | Meaning            |
//! |---------|-----------|--------------------|
//! | `k=v`   | `k=eq=v`  | equality           |
//! | `k!=v`  | `k=ne=v`  | inequality         |
//! | `k>v`   | `k=gt=v`  | greater-than       |
//! | `k<v`   | `k=lt=v`  | less-than          |
//! | `k>=v`  | `k=gte=v` | greater-or-equal   |
//! | `k<=v`  | `k=lte=v` | less-or-equal      |
//! | `k~v`   | `k=re=v`  | regex match        |
//!
//! Token recognition is intentionally narrow: the key must be a
//! non-empty prefix of one of the operator characters (`=<>!~`). If
//! the leading char is not a key char, the token isn't a matcher and
//! the caller routes it as a regular positional. This keeps the
//! disambiguation rule from the bead ("first non-key char picks
//! matcher vs value") in one place.

/// The seven operator variants. Equality + the six comparison/regex
/// flavours described in the bead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatcherOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Gte,
    Lte,
    Regex,
}

impl MatcherOp {
    /// Canonical symbol form (the one we print back in error messages
    /// and reuse in tests).
    pub fn as_symbol(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Ne => "!=",
            Self::Gt => ">",
            Self::Lt => "<",
            Self::Gte => ">=",
            Self::Lte => "<=",
            Self::Regex => "~",
        }
    }
}

/// A single `key OP value` predicate to apply over an entry's columns
/// or extra_data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matcher {
    pub key: String,
    pub op: MatcherOp,
    pub value: String,
}

/// Characters that mark the boundary between a matcher key and its
/// operator. Per the bead these are reserved at the data-write boundary;
/// here they're the alphabet for operator detection at the read
/// boundary.
const OP_CHARS: &[char] = &['=', '<', '>', '!', '~'];

/// Recognise a matcher token and split it into `(key, op, value)`.
///
/// Returns `Ok(Some(_))` when the token unambiguously parses as a
/// matcher, `Ok(None)` when the token has no operator (caller treats
/// it as a regular positional), and `Err` when it looks like a matcher
/// but the body is malformed (empty key, dangling operator, etc.).
pub fn parse_matcher_token(tok: &str) -> Result<Option<Matcher>, String> {
    // A token leading with an operator char has no key — that's not a
    // matcher, just a stray positional (`>5` from a user who forgot
    // the key). The caller routes it as a positional and the get
    // dispatcher will reject it downstream.
    let Some(first) = tok.chars().next() else {
        return Ok(None);
    };
    if OP_CHARS.contains(&first) {
        return Ok(None);
    }
    // First operator-char position is the boundary between key and the
    // rest. The key is non-empty by construction above.
    let Some(boundary) = tok.find(OP_CHARS) else {
        return Ok(None);
    };
    let key = &tok[..boundary];
    let tail = &tok[boundary..];

    // Six explicit named forms first (`=eq=`, `=ne=`, `=gt=`, `=lt=`,
    // `=gte=`, `=lte=`, `=re=`) so `count=ne=5` doesn't get parsed as
    // `count=ne` equality.
    if let Some(rest) = tail.strip_prefix("=") {
        for (tag, op) in NAMED_FORMS {
            if let Some(value) = rest.strip_prefix(tag).and_then(|r| r.strip_prefix('=')) {
                return Ok(Some(Matcher {
                    key: key.to_string(),
                    op: *op,
                    value: value.to_string(),
                }));
            }
        }
    }

    // Otherwise fall through to the symbol forms. Order matters: the
    // two-char operators must match before their one-char prefixes.
    for (sym, op) in SYMBOL_FORMS {
        if let Some(value) = tail.strip_prefix(*sym) {
            return Ok(Some(Matcher {
                key: key.to_string(),
                op: *op,
                value: value.to_string(),
            }));
        }
    }
    Err(format!(
        "unrecognised matcher operator in token '{tok}' — expected one of =, !=, >, <, >=, <=, ~ (or =eq= / =ne= / =gt= / =lt= / =gte= / =lte= / =re=)"
    ))
}

const NAMED_FORMS: &[(&str, MatcherOp)] = &[
    ("eq", MatcherOp::Eq),
    ("ne", MatcherOp::Ne),
    ("gte", MatcherOp::Gte),
    ("lte", MatcherOp::Lte),
    ("gt", MatcherOp::Gt),
    ("lt", MatcherOp::Lt),
    ("re", MatcherOp::Regex),
];

const SYMBOL_FORMS: &[(&str, MatcherOp)] = &[
    (">=", MatcherOp::Gte),
    ("<=", MatcherOp::Lte),
    ("!=", MatcherOp::Ne),
    ("=", MatcherOp::Eq),
    (">", MatcherOp::Gt),
    ("<", MatcherOp::Lt),
    ("~", MatcherOp::Regex),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn m(key: &str, op: MatcherOp, value: &str) -> Matcher {
        Matcher {
            key: key.to_string(),
            op,
            value: value.to_string(),
        }
    }

    #[test]
    fn symbol_form_equality() {
        assert_eq!(
            parse_matcher_token("state=happy").unwrap(),
            Some(m("state", MatcherOp::Eq, "happy"))
        );
    }

    #[test]
    fn symbol_form_two_char_ops_beat_one_char_prefix() {
        assert_eq!(
            parse_matcher_token("count>=5").unwrap(),
            Some(m("count", MatcherOp::Gte, "5"))
        );
        assert_eq!(
            parse_matcher_token("count<=5").unwrap(),
            Some(m("count", MatcherOp::Lte, "5"))
        );
        assert_eq!(
            parse_matcher_token("count!=5").unwrap(),
            Some(m("count", MatcherOp::Ne, "5"))
        );
    }

    #[test]
    fn symbol_form_regex_keeps_slashes_in_value() {
        // The /…/ delimiters are a user convention to survive the shell
        // — they pass through as part of the matcher's value and the
        // executor strips them before regex compile.
        assert_eq!(
            parse_matcher_token("state~/happy|content/").unwrap(),
            Some(m("state", MatcherOp::Regex, "/happy|content/"))
        );
    }

    #[test]
    fn named_form_all_seven_operators() {
        let cases = [
            ("k=eq=v", MatcherOp::Eq),
            ("k=ne=v", MatcherOp::Ne),
            ("k=gt=v", MatcherOp::Gt),
            ("k=lt=v", MatcherOp::Lt),
            ("k=gte=v", MatcherOp::Gte),
            ("k=lte=v", MatcherOp::Lte),
            ("k=re=v", MatcherOp::Regex),
        ];
        for (tok, op) in cases {
            assert_eq!(
                parse_matcher_token(tok).unwrap(),
                Some(m("k", op, "v")),
                "token={tok}"
            );
        }
    }

    /// `k=eq=v` must NOT be parsed as `k=eq=v` equality matcher with
    /// value `eq=v` — the named form wins so users can write `k=ne=5`
    /// without quoting.
    #[test]
    fn named_form_does_not_collide_with_equality() {
        let m1 = parse_matcher_token("count=ne=5").unwrap().unwrap();
        assert_eq!(m1.op, MatcherOp::Ne);
        assert_eq!(m1.value, "5");
    }

    /// `k=ne` (no second `=`) is still equality, not a malformed named
    /// form. The named form requires the trailing `=v` to commit.
    #[test]
    fn incomplete_named_form_falls_through_to_equality() {
        let m1 = parse_matcher_token("count=ne").unwrap().unwrap();
        assert_eq!(m1.op, MatcherOp::Eq);
        assert_eq!(m1.value, "ne");
    }

    #[test]
    fn no_operator_returns_none() {
        assert_eq!(parse_matcher_token("sleep").unwrap(), None);
        assert_eq!(parse_matcher_token("u1hrr").unwrap(), None);
    }

    #[test]
    fn token_leading_with_operator_is_not_a_matcher() {
        // `>5` has no key — caller will route it as a positional.
        assert_eq!(parse_matcher_token(">5").unwrap(), None);
        assert_eq!(parse_matcher_token("=v").unwrap(), None);
    }

    #[test]
    fn value_can_be_empty() {
        // `k=` is a legitimate "value is the empty string" matcher; the
        // executor decides whether that means anything semantically.
        let got = parse_matcher_token("note=").unwrap().unwrap();
        assert_eq!(got.value, "");
        assert_eq!(got.op, MatcherOp::Eq);
    }
}
