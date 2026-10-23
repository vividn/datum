//! Document-reference detection — parse `<field>:<id>` colon-pattern strings
//! produced by `datum add … --id <name>`.
//!
//! Entries whose slug is `<field>:<id>` act as documents; later entries can
//! reference them by using that slug as a value (`datum switch consume
//! book:goldmund`). At read time, colon-pattern values can be detected by
//! [`parse_ref`] and dereferenced against the database. References are *live*
//! — resolved on read, not stored inline — so updates to the document
//! propagate to all referencing entries automatically.
//!
//! [`parse_ref`] is intentionally liberal: it only does the cheap string
//! split. Callers are responsible for confirming the referenced entry exists
//! (a missing target is legal — the reference is just unresolved until the
//! document is created or synced in).
//!
//! A STATE is the stricter case: [`state_refs`] reads `:` inside it as a
//! reserved token, so there it is never a guess. A DATA value refers only
//! through [`data_refs`]: the reserved `ref://` prefix, or a key the spec
//! types `ref`.

use std::borrow::Cow;

use serde_json::Value as JsonValue;

use crate::field_spec::{FieldSpec, SubfieldType};
use crate::shape::{IGNORED_STATE_VALUE, REF_MARK, STR_MARK, TRACKED_STATE_VALUE};

/// Split `s` on the first `:` and return `(field, id)` when both sides are
/// non-empty and the field portion is a plausible identifier.
///
/// The field must begin with an ASCII letter or underscore and contain only
/// ASCII letters/digits/underscore/dash. This excludes time-only strings
/// (`:30`) and similar accidental colon-bearing payloads. The id portion
/// only needs to be non-empty and free of path separators — it can carry
/// colons, digits, hyphens, and timestamp punctuation so canonical slugs
/// like `coffee:2024-03-16T09:15:00.000Z` also parse as refs. Rejecting
/// `/` in the id keeps URL schemes (`https://…`) out of the result.
///
/// Returns `None` when the string is not in `<field>:<id>` shape. Does NOT
/// check that the target exists in any database.
pub fn parse_ref(s: &str) -> Option<(&str, &str)> {
    let (field, id) = s.split_once(':')?;
    if field.is_empty() || id.is_empty() {
        return None;
    }
    if !is_field_ident(field) {
        return None;
    }
    if id.contains('/') {
        return None;
    }
    Some((field, id))
}

/// The slugs a state refers to (da-f9jyv). `:` inside a state is a
/// reserved token, not a shape to guess at:
///
/// 1. a string holding `:` is a full slug `field:id`, whatever field logged
///    it — `switch media book:great_gatsby` names `book:great_gatsby`, and
///    `switch clock 12:30` names `12:30`, which dangles;
/// 2. a string without one, on a field whose spec has a
///    [`state_lookup_field`](FieldSpec::state_lookup_field) `F`, names
///    `F:<state>`;
/// 3. anything else is a plain state and names nothing.
///
/// An array state takes the rule per element. A non-string (`false`, a
/// number) and the TRACKED / IGNORED sentinels never refer. Unlike
/// [`parse_ref`] this is a grammar rule, not a guess; nothing here says
/// whether the slug exists.
pub fn state_refs(spec: Option<&FieldSpec>, state: &JsonValue) -> Vec<String> {
    match state {
        JsonValue::Array(items) => items.iter().flat_map(|s| state_refs(spec, s)).collect(),
        JsonValue::String(s) if s.contains(':') => vec![s.clone()],
        JsonValue::String(s)
            if s.is_empty() || s == TRACKED_STATE_VALUE || s == IGNORED_STATE_VALUE =>
        {
            vec![]
        }
        JsonValue::String(s) => spec
            .and_then(|sp| sp.state_lookup_field.as_deref())
            .map(|f| vec![format!("{f}:{s}")])
            .unwrap_or_default(),
        _ => vec![],
    }
}

/// The slugs the data value under `key` refers to (da-s5k2s):
///
/// 1. a string beginning `ref://` refers to the slug after it, on any key —
///    the prefix is reserved, so a literal one is stored `str://ref://…`;
/// 2. any other string, on a key the spec types [`SubfieldType::Ref`] or
///    [`SubfieldType::RefList`], is itself a slug — except that literal;
/// 3. anything else refers to nothing.
///
/// An array takes the rule per element, and an object per member under
/// `key.member`, the dotted path a subfield is named by. Nothing here says
/// whether the slug exists.
pub fn data_refs(spec: Option<&FieldSpec>, key: &str, value: &JsonValue) -> Vec<String> {
    match value {
        JsonValue::Array(items) => items.iter().flat_map(|v| data_refs(spec, key, v)).collect(),
        JsonValue::Object(map) => map
            .iter()
            .flat_map(|(k, v)| data_refs(spec, &format!("{key}.{k}"), v))
            .collect(),
        JsonValue::String(s) => match s.strip_prefix(REF_MARK) {
            Some(slug) if !slug.is_empty() => vec![slug.to_string()],
            Some(_) => vec![],
            None if s.is_empty() || literal(s).is_some() => vec![],
            None => spec
                .and_then(|sp| sp.subfields.get(key))
                .filter(|sf| matches!(sf.value_type, SubfieldType::Ref | SubfieldType::RefList))
                .map(|_| vec![s.clone()])
                .unwrap_or_default(),
        },
        _ => vec![],
    }
}

/// A value as it reads: a `str://ref://…` literal without its `str://`
/// (da-s5k2s), per element in an array. Everything else is itself.
pub fn shown(value: &JsonValue) -> Cow<'_, JsonValue> {
    match value {
        JsonValue::String(s) => match literal(s) {
            Some(lit) => Cow::Owned(lit.into()),
            None => Cow::Borrowed(value),
        },
        JsonValue::Array(a) if a.iter().any(|v| matches!(shown(v), Cow::Owned(_))) => {
            Cow::Owned(a.iter().map(|v| shown(v).into_owned()).collect())
        }
        _ => Cow::Borrowed(value),
    }
}

/// `ref://…` when `s` is its stored literal `str://ref://…`. `str://`
/// before anything else is not (yet) a mark: the string as it is.
fn literal(s: &str) -> Option<&str> {
    s.strip_prefix(STR_MARK).filter(|l| l.starts_with(REF_MARK))
}

/// Plausible field identifier: starts with `[A-Za-z_]`, continues with
/// `[A-Za-z0-9_-]`.
fn is_field_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_ref_splits_on_first_colon() {
        assert_eq!(parse_ref("book:goldmund"), Some(("book", "goldmund")));
        assert_eq!(parse_ref("person:vividn"), Some(("person", "vividn")));
        assert_eq!(parse_ref("project:daydumb"), Some(("project", "daydumb")));
    }

    #[test]
    fn timestamp_id_parses_as_ref() {
        // Canonical auto-generated slug shape — `coffee:2024-…`. Split on the
        // FIRST colon keeps everything after it in the id portion.
        assert_eq!(
            parse_ref("coffee:2024-03-16T09:15:00.000Z"),
            Some(("coffee", "2024-03-16T09:15:00.000Z")),
        );
    }

    #[test]
    fn no_colon_is_not_a_ref() {
        assert_eq!(parse_ref("plain"), None);
        assert_eq!(parse_ref(""), None);
    }

    #[test]
    fn empty_sides_rejected() {
        assert_eq!(parse_ref(":goldmund"), None);
        assert_eq!(parse_ref("book:"), None);
        assert_eq!(parse_ref(":"), None);
    }

    #[test]
    fn url_not_mistaken_for_ref() {
        // Field portion fails the ident check — no false positives on URLs.
        assert_eq!(parse_ref("https://example.com"), None);
        assert_eq!(parse_ref("ftp://host/path"), None);
    }

    #[test]
    fn field_with_digits_and_dashes_allowed() {
        assert_eq!(parse_ref("book-1:goldmund"), Some(("book-1", "goldmund")));
        assert_eq!(parse_ref("book_1:goldmund"), Some(("book_1", "goldmund")));
        assert_eq!(parse_ref("_hidden:x"), Some(("_hidden", "x")));
    }

    #[test]
    fn field_starting_with_digit_rejected() {
        // Avoids false positives on payloads like `"24:30"` (minute:second).
        assert_eq!(parse_ref("24:30"), None);
        assert_eq!(parse_ref("1:2"), None);
    }

    #[test]
    fn field_with_space_rejected() {
        assert_eq!(parse_ref("my book:goldmund"), None);
    }

    #[test]
    fn state_refs_follow_the_three_rules() {
        use crate::action::Action;
        use serde_json::json;
        let plain = FieldSpec::new("media", Action::Switch);
        let mut looked = FieldSpec::new("issue", Action::Switch);
        looked.state_lookup_field = Some("issue_state".into());
        // 1. `:` is a full slug, on any field, lookup or not.
        assert_eq!(
            state_refs(Some(&plain), &json!("book:gatsby")),
            ["book:gatsby"]
        );
        assert_eq!(
            state_refs(Some(&looked), &json!("book:gatsby")),
            ["book:gatsby"]
        );
        assert_eq!(state_refs(None, &json!("12:30")), ["12:30"]);
        // 2. a bare state through the lookup field.
        assert_eq!(
            state_refs(Some(&looked), &json!("queued")),
            ["issue_state:queued"]
        );
        // 3. control: a bare state without one names nothing.
        assert!(state_refs(Some(&plain), &json!("queued")).is_empty());
        assert!(state_refs(None, &json!("queued")).is_empty());
        // per element
        assert_eq!(
            state_refs(Some(&looked), &json!(["a:b", "c", true])),
            ["a:b", "issue_state:c"]
        );
        // never a reference
        for s in [
            json!(false),
            json!(true),
            json!(3),
            json!(null),
            json!("TRACKED"),
            json!("IGNORED"),
            json!(""),
        ] {
            assert!(state_refs(Some(&looked), &s).is_empty(), "{s}");
        }
    }

    #[test]
    fn data_refs_marker_and_ref_typed_key() {
        use crate::action::Action;
        use crate::field_spec::SubfieldSpec;
        use serde_json::json;
        let mut typed = FieldSpec::new("x", Action::Occur);
        for k in ["needs", "a.b"] {
            typed.subfields.insert(
                k.into(),
                SubfieldSpec {
                    required: false,
                    value_type: SubfieldType::Ref,
                    in_slug: false,
                    default: None,
                },
            );
        }
        // 1. the marker, on any key, spec or not.
        assert_eq!(data_refs(None, "k", &json!("ref://i:doc")), ["i:doc"]);
        assert_eq!(
            data_refs(Some(&typed), "needs", &json!("ref://i:doc")),
            ["i:doc"]
        );
        // 2. a bare slug on a ref-typed key; control: the same key untyped.
        assert_eq!(data_refs(Some(&typed), "needs", &json!("i:doc")), ["i:doc"]);
        assert!(data_refs(None, "needs", &json!("i:doc")).is_empty());
        assert!(data_refs(Some(&typed), "other", &json!("i:doc")).is_empty());
        // a literal never refers, typed key or not.
        assert!(data_refs(None, "k", &json!("str://ref://i:doc")).is_empty());
        assert!(data_refs(Some(&typed), "needs", &json!("str://ref://i:doc")).is_empty());
        // per element, and per member under its dotted path.
        assert_eq!(
            data_refs(None, "k", &json!(["ref://a:1", "plain", "ref://b:2"])),
            ["a:1", "b:2"]
        );
        assert_eq!(
            data_refs(Some(&typed), "a", &json!({"b": "c:3", "z": "d:4"})),
            ["c:3"]
        );
        for v in [
            json!(3),
            json!(true),
            json!(null),
            json!(""),
            json!("ref://"),
        ] {
            assert!(data_refs(Some(&typed), "needs", &v).is_empty(), "{v}");
        }
    }

    #[test]
    fn a_literal_reads_without_its_str_mark() {
        use serde_json::json;
        assert_eq!(*shown(&json!("str://ref://x")), json!("ref://x"));
        assert_eq!(
            *shown(&json!(["str://ref://x", "y"])),
            json!(["ref://x", "y"])
        );
        // controls: `str://` before anything else, a reference, a plain value.
        for v in [json!("str://other"), json!("ref://x"), json!("y"), json!(1)] {
            assert!(matches!(shown(&v), Cow::Borrowed(_)), "{v}");
        }
    }

    #[test]
    fn id_with_colons_permitted() {
        // Namespaced-id case — everything after the first ':' is the id.
        assert_eq!(parse_ref("book:series:dune"), Some(("book", "series:dune")),);
    }
}
