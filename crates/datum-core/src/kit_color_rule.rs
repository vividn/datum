//! Tiny expression grammar for `[[command.color_rules]]` (slice 6).
//!
//! Grammar:
//!
//! ```text
//! expr     := term (('&&' | '||') term)*
//! term     := atom CMP atom
//! atom     := IDENT | STRING | NUMBER | 'now' | 'today' | dateexpr
//! dateexpr := ('now' | 'today') ('+' | '-') NUMBER ('d' | 'h' | 'm' | 's')
//! CMP      := '<' | '<=' | '>' | '>=' | '=' | '!='
//! ```
//!
//! No control flow, no function calls, no nested parentheses. Evaluation
//! takes a row keyed by column name plus the current "now" timestamp; if a
//! comparison's atom is a column name the row's cell is read and coerced.
//! Only date/numeric/string atoms are recognised. The first matching rule
//! wins; subsequent rules are ignored (so authors order most-specific
//! first).

use chrono::{DateTime, Duration, Utc};
use serde_json::Value as JsonValue;

use crate::error::{DatumError, Result};

/// One parsed `color_rules = [{ when = "...", color = "..." }]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorRule {
    pub when: Expr,
    pub color: String,
}

/// Top-level expression — a chain of comparison terms joined by `&&`/`||`.
/// Operators are left-associative and share the same precedence (datum
/// keeps the grammar deliberately tiny; authors can use multiple rules
/// for "or" or sequence comparisons in priority order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expr {
    pub head: Term,
    pub tail: Vec<(BoolOp, Term)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolOp {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    pub left: Atom,
    pub op: CmpOp,
    pub right: Atom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Atom {
    /// Bare identifier — interpreted as a column name at eval time.
    Ident(String),
    /// Quoted string literal.
    StringLit(String),
    /// Decimal number literal. Stored as the original token for round-trip.
    NumberLit(String),
    /// `now` — instant of evaluation.
    Now,
    /// `today` — local-midnight of evaluation.
    Today,
    /// `now ± Nd` / `today ± Nh` etc. The `bool` is true for `+`.
    DateExpr {
        anchor: DateAnchor,
        plus: bool,
        amount: i64,
        unit: DateUnit,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateAnchor {
    Now,
    Today,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateUnit {
    Days,
    Hours,
    Minutes,
    Seconds,
}

impl ColorRule {
    /// Parse the `when` source plus the resolved color string.
    pub fn parse(when: &str, color: &str) -> Result<Self> {
        let expr = parse_expr(when)?;
        Ok(ColorRule {
            when: expr,
            color: color.to_string(),
        })
    }
}

/// Parse a color expression, surfacing readable error messages.
pub fn parse_expr(src: &str) -> Result<Expr> {
    let toks = tokenise(src)?;
    let mut p = Parser { toks, pos: 0 };
    let expr = p.parse_expr()?;
    if p.pos < p.toks.len() {
        return Err(DatumError::Other(format!(
            "color rule: trailing tokens after expression: {}",
            p.toks[p.pos..]
                .iter()
                .map(|t| t.text.clone())
                .collect::<Vec<_>>()
                .join(" ")
        )));
    }
    Ok(expr)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    kind: Tok,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    Ident,
    String,
    Number,
    Plus,
    Minus,
    Cmp(CmpOp),
    BoolOp(BoolOp),
}

fn tokenise(src: &str) -> Result<Vec<Token>> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '+' => {
                out.push(Token {
                    kind: Tok::Plus,
                    text: "+".to_string(),
                });
                i += 1;
            }
            '-' => {
                out.push(Token {
                    kind: Tok::Minus,
                    text: "-".to_string(),
                });
                i += 1;
            }
            '<' | '>' | '=' | '!' => {
                let two = if i + 1 < bytes.len() {
                    let nx = bytes[i + 1] as char;
                    Some((c, nx))
                } else {
                    None
                };
                let (op, len) = match two {
                    Some(('<', '=')) => (CmpOp::Le, 2),
                    Some(('>', '=')) => (CmpOp::Ge, 2),
                    Some(('!', '=')) => (CmpOp::Ne, 2),
                    _ => match c {
                        '<' => (CmpOp::Lt, 1),
                        '>' => (CmpOp::Gt, 1),
                        '=' => (CmpOp::Eq, 1),
                        _ => {
                            return Err(DatumError::Other(format!(
                                "color rule: unexpected `!` without `=` at offset {i}"
                            )))
                        }
                    },
                };
                out.push(Token {
                    kind: Tok::Cmp(op),
                    text: src[i..i + len].to_string(),
                });
                i += len;
            }
            '&' | '|' => {
                if i + 1 >= bytes.len() || bytes[i + 1] as char != c {
                    return Err(DatumError::Other(format!(
                        "color rule: expected `{c}{c}` at offset {i}"
                    )));
                }
                let op = if c == '&' { BoolOp::And } else { BoolOp::Or };
                out.push(Token {
                    kind: Tok::BoolOp(op),
                    text: format!("{c}{c}"),
                });
                i += 2;
            }
            '\'' | '"' => {
                let quote = c;
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() && bytes[j] as char != quote {
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err(DatumError::Other(
                        "color rule: unterminated string literal".to_string(),
                    ));
                }
                out.push(Token {
                    kind: Tok::String,
                    text: src[start..j].to_string(),
                });
                i = j + 1;
            }
            c if c.is_ascii_digit() => {
                let start = i;
                while i < bytes.len() && (bytes[i] as char).is_ascii_digit()
                    || (i < bytes.len() && bytes[i] as char == '.')
                {
                    i += 1;
                }
                out.push(Token {
                    kind: Tok::Number,
                    text: src[start..i].to_string(),
                });
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < bytes.len() {
                    let nx = bytes[i] as char;
                    if nx.is_ascii_alphanumeric() || nx == '_' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                out.push(Token {
                    kind: Tok::Ident,
                    text: src[start..i].to_string(),
                });
            }
            _ => {
                return Err(DatumError::Other(format!(
                    "color rule: unexpected character `{c}` at offset {i}"
                )));
            }
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.toks.get(self.pos)
    }

    fn bump(&mut self) -> Option<Token> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        let head = self.parse_term()?;
        let mut tail = Vec::new();
        while let Some(t) = self.peek() {
            if let Tok::BoolOp(op) = t.kind {
                self.bump();
                let term = self.parse_term()?;
                tail.push((op, term));
            } else {
                break;
            }
        }
        Ok(Expr { head, tail })
    }

    fn parse_term(&mut self) -> Result<Term> {
        let left = self.parse_atom()?;
        let op = match self.bump() {
            Some(Token {
                kind: Tok::Cmp(op), ..
            }) => op,
            other => {
                return Err(DatumError::Other(format!(
                    "color rule: expected comparison operator, got {other:?}"
                )))
            }
        };
        let right = self.parse_atom()?;
        Ok(Term { left, op, right })
    }

    fn parse_atom(&mut self) -> Result<Atom> {
        let t = self
            .bump()
            .ok_or_else(|| DatumError::Other("color rule: unexpected end of input".to_string()))?;
        match t.kind {
            Tok::String => Ok(Atom::StringLit(t.text)),
            Tok::Number => Ok(Atom::NumberLit(t.text)),
            Tok::Ident => {
                let anchor = match t.text.as_str() {
                    "now" => Some(DateAnchor::Now),
                    "today" => Some(DateAnchor::Today),
                    _ => None,
                };
                let Some(anchor) = anchor else {
                    return Ok(Atom::Ident(t.text));
                };
                // Optional `± N<unit>` — peek for a Plus/Minus.
                let plus = match self.peek().map(|t| t.kind) {
                    Some(Tok::Plus) => {
                        self.bump();
                        true
                    }
                    Some(Tok::Minus) => {
                        self.bump();
                        false
                    }
                    _ => {
                        return Ok(match anchor {
                            DateAnchor::Now => Atom::Now,
                            DateAnchor::Today => Atom::Today,
                        });
                    }
                };
                let num_tok = self.bump().ok_or_else(|| {
                    DatumError::Other("color rule: expected number after +/-".to_string())
                })?;
                if num_tok.kind != Tok::Number {
                    return Err(DatumError::Other(format!(
                        "color rule: expected number after +/-, got `{}`",
                        num_tok.text
                    )));
                }
                let amount: i64 = num_tok.text.parse().map_err(|_| {
                    DatumError::Other(format!("color rule: bad number `{}`", num_tok.text))
                })?;
                let unit_tok = self.bump().ok_or_else(|| {
                    DatumError::Other("color rule: expected unit suffix (d|h|m|s)".to_string())
                })?;
                let unit = match unit_tok.text.as_str() {
                    "d" => DateUnit::Days,
                    "h" => DateUnit::Hours,
                    "m" => DateUnit::Minutes,
                    "s" => DateUnit::Seconds,
                    other => {
                        return Err(DatumError::Other(format!(
                            "color rule: unknown date unit `{other}` (use d|h|m|s)"
                        )))
                    }
                };
                Ok(Atom::DateExpr {
                    anchor,
                    plus,
                    amount,
                    unit,
                })
            }
            other => Err(DatumError::Other(format!(
                "color rule: expected atom, got {other:?} (`{}`)",
                t.text
            ))),
        }
    }
}

/// Coerced value used during evaluation.
#[derive(Debug, Clone, PartialEq)]
enum Val {
    Null,
    Number(f64),
    String(String),
    DateTime(DateTime<Utc>),
}

fn atom_to_val(atom: &Atom, row: &dyn RowLookup, now: DateTime<Utc>) -> Val {
    match atom {
        Atom::Ident(col) => {
            let cell = row.cell(col);
            json_to_val(cell.unwrap_or(&JsonValue::Null))
        }
        Atom::StringLit(s) => Val::String(s.clone()),
        Atom::NumberLit(n) => n
            .parse::<f64>()
            .map(Val::Number)
            .unwrap_or(Val::String(n.clone())),
        Atom::Now => Val::DateTime(now),
        Atom::Today => Val::DateTime(midnight_today(now)),
        Atom::DateExpr {
            anchor,
            plus,
            amount,
            unit,
        } => {
            let base = match anchor {
                DateAnchor::Now => now,
                DateAnchor::Today => midnight_today(now),
            };
            let dur = match unit {
                DateUnit::Days => Duration::days(*amount),
                DateUnit::Hours => Duration::hours(*amount),
                DateUnit::Minutes => Duration::minutes(*amount),
                DateUnit::Seconds => Duration::seconds(*amount),
            };
            Val::DateTime(if *plus { base + dur } else { base - dur })
        }
    }
}

fn json_to_val(v: &JsonValue) -> Val {
    match v {
        JsonValue::Null => Val::Null,
        JsonValue::Bool(b) => Val::String(b.to_string()),
        JsonValue::Number(n) => n
            .as_f64()
            .map(Val::Number)
            .unwrap_or(Val::String(n.to_string())),
        JsonValue::String(s) => {
            // Strings that look like RFC3339 timestamps coerce to DateTime
            // so a column like `next_due_raw` storing ISO text compares
            // against `now`/`today` correctly.
            if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                Val::DateTime(dt.with_timezone(&Utc))
            } else if let Ok(n) = s.parse::<f64>() {
                Val::Number(n)
            } else {
                Val::String(s.clone())
            }
        }
        other => Val::String(other.to_string()),
    }
}

fn midnight_today(now: DateTime<Utc>) -> DateTime<Utc> {
    use chrono::TimeZone;
    let date = now.date_naive();
    Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
}

fn cmp_vals(a: &Val, op: CmpOp, b: &Val) -> bool {
    use std::cmp::Ordering;
    let ord = match (a, b) {
        (Val::Null, Val::Null) => Some(Ordering::Equal),
        (Val::Null, _) | (_, Val::Null) => None,
        (Val::Number(x), Val::Number(y)) => x.partial_cmp(y),
        (Val::DateTime(x), Val::DateTime(y)) => Some(x.cmp(y)),
        (Val::String(x), Val::String(y)) => Some(x.cmp(y)),
        (Val::Number(x), Val::String(y)) | (Val::String(y), Val::Number(x)) => {
            // Mixed number/string: parse the string; otherwise lex compare.
            match y.parse::<f64>() {
                Ok(yn) => x.partial_cmp(&yn),
                Err(_) => Some(x.to_string().cmp(y)),
            }
        }
        _ => None,
    };
    match ord {
        Some(Ordering::Less) => matches!(op, CmpOp::Lt | CmpOp::Le | CmpOp::Ne),
        Some(Ordering::Equal) => matches!(op, CmpOp::Le | CmpOp::Ge | CmpOp::Eq),
        Some(Ordering::Greater) => matches!(op, CmpOp::Gt | CmpOp::Ge | CmpOp::Ne),
        None => false,
    }
}

/// Read-side hook the renderer implements for the row currently being
/// evaluated. Boxed so we can mock in tests.
pub trait RowLookup {
    fn cell(&self, name: &str) -> Option<&JsonValue>;
}

impl<'a> RowLookup for std::collections::HashMap<&'a str, &'a JsonValue> {
    fn cell(&self, name: &str) -> Option<&JsonValue> {
        self.get(name).copied()
    }
}

/// Evaluate a parsed expression against a row at the given `now`.
pub fn eval_expr(expr: &Expr, row: &dyn RowLookup, now: DateTime<Utc>) -> bool {
    let mut acc = eval_term(&expr.head, row, now);
    for (op, term) in &expr.tail {
        let v = eval_term(term, row, now);
        acc = match op {
            BoolOp::And => acc && v,
            BoolOp::Or => acc || v,
        };
    }
    acc
}

fn eval_term(term: &Term, row: &dyn RowLookup, now: DateTime<Utc>) -> bool {
    let l = atom_to_val(&term.left, row, now);
    let r = atom_to_val(&term.right, row, now);
    cmp_vals(&l, term.op, &r)
}

/// Walk a list of rules in order; first match wins.
pub fn pick_color<'a>(
    rules: &'a [ColorRule],
    row: &dyn RowLookup,
    now: DateTime<Utc>,
) -> Option<&'a str> {
    rules
        .iter()
        .find(|r| eval_expr(&r.when, row, now))
        .map(|r| r.color.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;
    use std::collections::HashMap;

    fn now_fixed() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 4, 30, 12, 0, 0).unwrap()
    }

    #[test]
    fn parse_simple_comparison() {
        let e = parse_expr("next_due < now").unwrap();
        assert!(matches!(e.head.left, Atom::Ident(ref s) if s == "next_due"));
        assert!(matches!(e.head.op, CmpOp::Lt));
        assert!(matches!(e.head.right, Atom::Now));
        assert!(e.tail.is_empty());
    }

    #[test]
    fn parse_date_arithmetic() {
        let e = parse_expr("next_due < today + 1d").unwrap();
        let Atom::DateExpr {
            anchor,
            plus,
            amount,
            unit,
        } = &e.head.right
        else {
            panic!("expected dateexpr");
        };
        assert!(matches!(anchor, DateAnchor::Today));
        assert!(plus);
        assert_eq!(*amount, 1);
        assert!(matches!(unit, DateUnit::Days));
    }

    #[test]
    fn parse_chain_and_or() {
        let e = parse_expr("a < 1 && b > 2 || c = 'x'").unwrap();
        assert_eq!(e.tail.len(), 2);
        assert!(matches!(e.tail[0].0, BoolOp::And));
        assert!(matches!(e.tail[1].0, BoolOp::Or));
    }

    #[test]
    fn parse_rejects_function_call() {
        assert!(parse_expr("foo(1) < now").is_err());
    }

    #[test]
    fn parse_rejects_trailing_garbage() {
        assert!(parse_expr("a < 1 garbage").is_err());
    }

    #[test]
    fn parse_rejects_lone_bang() {
        assert!(parse_expr("a ! 1").is_err());
    }

    #[test]
    fn eval_overdue_red_today_yellow() {
        let rules = vec![
            ColorRule::parse("next_due < now", "red").unwrap(),
            ColorRule::parse("next_due < today + 1d", "yellow").unwrap(),
        ];

        let now = now_fixed();
        let yesterday = json!("2026-04-29T08:00:00Z");
        let later_today = json!("2026-04-30T18:00:00Z");
        let next_week = json!("2026-05-08T08:00:00Z");

        let mut r1 = HashMap::new();
        r1.insert("next_due", &yesterday);
        assert_eq!(pick_color(&rules, &r1, now), Some("red"));

        let mut r2 = HashMap::new();
        r2.insert("next_due", &later_today);
        assert_eq!(pick_color(&rules, &r2, now), Some("yellow"));

        let mut r3 = HashMap::new();
        r3.insert("next_due", &next_week);
        assert_eq!(pick_color(&rules, &r3, now), None);
    }

    #[test]
    fn eval_handles_null_cell() {
        let rules = vec![ColorRule::parse("next_due < now", "red").unwrap()];
        let null = JsonValue::Null;
        let mut row = HashMap::new();
        row.insert("next_due", &null);
        assert_eq!(pick_color(&rules, &row, now_fixed()), None);
    }

    #[test]
    fn eval_string_equality() {
        let rules = vec![ColorRule::parse("status = 'open'", "blue").unwrap()];
        let v = json!("open");
        let mut row = HashMap::new();
        row.insert("status", &v);
        assert_eq!(pick_color(&rules, &row, now_fixed()), Some("blue"));
    }
}
