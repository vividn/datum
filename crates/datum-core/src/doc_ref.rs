//! Document-reference detection — parse `<field>:<id>` colon-pattern strings
//! produced by the `datum add … id=<name>` convention.
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

/// Plausible field identifier: starts with `[A-Za-z_]`, continues with
/// `[A-Za-z0-9_-]`.
fn is_field_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else { return false };
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
    fn id_with_colons_permitted() {
        // Namespaced-id case — everything after the first ':' is the id.
        assert_eq!(
            parse_ref("book:series:dune"),
            Some(("book", "series:dune")),
        );
    }
}
