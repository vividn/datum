//! Consolidate repeated `-k` flags so the last-seen spec wins in the
//! first-seen position.
//!
//! Port of datum's `src/input/consolidateKeys.ts`. Same algorithm, same
//! semantics; see `docs/parity-audit/parsing.md` ("input/ coverage —
//! `consolidateKeys.ts`") for the specification.

/// Collapse a sequence of `-k`-style specs so that repeated key names
/// retain their *first-appearing position* but take the *last-appearing
/// form*. Each spec is either a bare name (`"foo"`) or a name with an
/// optional default (`"foo="`, `"foo=default"`). The key name is the
/// portion before the first `=`.
///
/// # Example
/// ```
/// use datum_parse::input::consolidate_keys;
/// let out = consolidate_keys(&[
///     "a".to_string(), "b".to_string(), "a=2".to_string(), "c".to_string(),
///     "d=abcd".to_string(), "a=4".to_string(), "d".to_string(),
///     "c=".to_string(), "e=".to_string(), "f".to_string(),
///     "f=".to_string(), "f".to_string(),
/// ]);
/// assert_eq!(out, vec!["a=4", "b", "c=", "d", "e=", "f"]);
/// ```
pub fn consolidate_keys(keys: &[String]) -> Vec<String> {
    let mut acc: Vec<String> = Vec::with_capacity(keys.len());
    for spec in keys {
        let name = spec.split_once('=').map(|(n, _)| n).unwrap_or(spec);
        let existing = acc.iter().position(|existing| {
            let existing_name = existing
                .split_once('=')
                .map(|(n, _)| n)
                .unwrap_or(existing.as_str());
            existing_name == name
        });
        match existing {
            Some(idx) => acc[idx] = spec.clone(),
            None => acc.push(spec.clone()),
        }
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_produces_empty_output() {
        let out = consolidate_keys(&[]);
        assert!(out.is_empty());
    }

    #[test]
    fn single_key_passes_through() {
        let out = consolidate_keys(&["abc".to_string()]);
        assert_eq!(out, vec!["abc"]);
    }

    #[test]
    fn last_seen_form_wins_in_first_seen_position() {
        // Direct port of the TS test in
        // /tmp/datum-survey/src/input/__test__/consolidateKeys.test.ts
        let out = consolidate_keys(&[
            "a".into(),
            "b".into(),
            "a=2".into(),
            "c".into(),
            "d=abcd".into(),
            "a=4".into(),
            "d".into(),
            "c=".into(),
            "e=".into(),
            "f".into(),
            "f=".into(),
            "f".into(),
        ]);
        assert_eq!(out, vec!["a=4", "b", "c=", "d", "e=", "f"]);
    }

    #[test]
    fn equals_in_default_value_does_not_confuse_name_extraction() {
        // The split is on the *first* '=' so `foo=a=b` has name `foo`
        // and default `a=b`; another `foo=x` replaces it.
        let out = consolidate_keys(&["foo=a=b".into(), "foo=x".into()]);
        assert_eq!(out, vec!["foo=x"]);
    }

    #[test]
    fn distinct_names_preserved_in_order() {
        let out = consolidate_keys(&["zebra".into(), "apple".into(), "mango".into()]);
        assert_eq!(out, vec!["zebra", "apple", "mango"]);
    }
}
