//! A field's declared keys as the slots of its log line (da-9s98k).
//!
//! vividn, 2026-09-27: «for each key in the spec or an alias, the user should
//! be able to specify required, optional, default, or assign, which then
//! nicely maps to -k key, -k key=, -k key=default, key=value». A spec's
//! keys join a log line as if an alias had written them after the `-k`/`-K`
//! the line already carries, so logging a field behaves as an alias with
//! those slots. The alias expansion and the log path both go through
//! [`spec_key_words`], so an aliased line and a typed one bind alike.

use crate::cli::LogArgs;
use crate::shape::{IGNORED_STATE_VALUE, TRACKED_STATE_VALUE};

/// What a declared key asks of a log: the four ways an alias line names a
/// key — `-k key`, `-k key=`, `-k key=default`, `key=value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyKind {
    Required,
    Optional,
    Default(String),
    Assign(String),
}

impl std::fmt::Display for KeyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyKind::Required => write!(f, "required"),
            KeyKind::Optional => write!(f, "optional"),
            KeyKind::Default(v) => write!(f, "default {v}"),
            KeyKind::Assign(v) => write!(f, "assign {v}"),
        }
    }
}

/// Parse a key declaration as `id_keys`, `data_keys` and `track --key`
/// write it: `key` (required), `key=` (optional), `key=default`, or
/// `key:=value` (assign).
pub fn parse_key_decl(s: &str) -> (&str, KeyKind) {
    match s.split_once('=') {
        None => (s, KeyKind::Required),
        Some((name, v)) => match name.strip_suffix(':') {
            Some(name) => (name, KeyKind::Assign(v.to_string())),
            None if v.is_empty() => (name, KeyKind::Optional),
            None => (name, KeyKind::Default(v.to_string())),
        },
    }
}

/// One key a spec declares, as a log line binds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredKey {
    pub name: String,
    pub kind: KeyKind,
    /// A `-K` key: its value goes into the slug.
    pub id: bool,
}

/// What a field's declared keys add to a log line.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SpecKeyWords {
    /// `-K` slots.
    pub slug_keys: Vec<String>,
    /// `-k` slots.
    pub keys: Vec<String>,
    /// `key=value` words, to stand ahead of the line's own.
    pub assigned: Vec<String>,
}

/// Whether a `verb` line takes its field's keys: only an ACTIVE entry does
/// (vividn, 09-27) — an occur, a start, a switch to a named state, and an
/// add (vividn, 09-28: «add is the primary use case for spec based
/// fields»); never an end, `false`, `null`, IGNORED or TRACKED. It reads the state as typed —
/// the last `state=`, else the first bare word — and under `-I` the joined
/// words are a named state whatever they spell.
pub fn takes_spec_keys(verb: &str, line: &LogArgs) -> bool {
    match verb {
        "occur" | "start" | "add" => true,
        "switch" => {
            // Under `-I` with no `state=`, the joined words are a named
            // state whatever they spell — `-I false` is the string "false".
            let literal = line.string_state && !line.rest.iter().any(|t| t.starts_with("state="));
            literal
                || !matches!(
                    typed_state(line).as_deref(),
                    Some("false" | "null" | IGNORED_STATE_VALUE | TRACKED_STATE_VALUE)
                )
        }
        _ => false,
    }
}

/// The state a switch line names, as typed: the last `state=`, else the
/// first bare word — under `-I` the bare words joined, which are the state
/// whatever they spell. Whose keys join the field's (da-cej7q).
pub fn typed_state(line: &LogArgs) -> Option<String> {
    let named = line
        .rest
        .iter()
        .rev()
        .find_map(|t| t.strip_prefix("state="));
    let mut bare = line.rest.iter().filter(|t| !t.contains('='));
    let state = match named {
        Some(s) => s.to_string(),
        None if line.string_state => bare.map(String::as_str).collect::<Vec<_>>().join(" "),
        None => bare.next()?.clone(),
    };
    Some(state).filter(|s| !s.is_empty())
}

/// The slots `declared` adds to `line`: a required, optional or default
/// key as its `-k`/`-K` slot, an assigned one as a `key=value` — a `-K`
/// one also as its slot, which the value fills. A key the line already
/// names, as a slot or a `key=value`, is left to the line.
pub fn spec_key_words(line: &LogArgs, declared: &[DeclaredKey]) -> SpecKeyWords {
    let slots = line
        .keys
        .iter()
        .chain(&line.slug_keys)
        .chain(&line.field_keys)
        .map(String::as_str);
    let set = line
        .rest
        .iter()
        .filter_map(|t| t.split_once('=').map(|(n, _)| n));
    let named: Vec<&str> = slots.map(|k| parse_key_decl(k).0).chain(set).collect();
    let mut out = SpecKeyWords::default();
    for key in declared
        .iter()
        .filter(|k| !named.contains(&k.name.as_str()))
    {
        let name = &key.name;
        let slot = match &key.kind {
            KeyKind::Assign(v) => {
                out.assigned.push(format!("{name}={v}"));
                if !key.id {
                    continue;
                }
                name.clone()
            }
            KeyKind::Required => name.clone(),
            KeyKind::Optional => format!("{name}="),
            KeyKind::Default(d) => format!("{name}={d}"),
        };
        if key.id {
            out.slug_keys.push(slot);
        } else {
            out.keys.push(slot);
        }
    }
    out
}

impl SpecKeyWords {
    /// The words as a line would spell them, flags first.
    pub fn into_argv(self) -> Vec<String> {
        let flags =
            |f: &'static str, v: Vec<String>| v.into_iter().flat_map(move |k| [f.into(), k]);
        let mut out: Vec<String> = flags("-K", self.slug_keys).collect();
        out.extend(flags("-k", self.keys));
        out.extend(self.assigned);
        out
    }
}
