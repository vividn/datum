//! What a field name may be.
//!
//! A field name is written as one argv token — `datum occur coffee`, and
//! the widget bar's `spec coffee …`. A name that does not survive that
//! round trip is a name nothing can log to: `cups=2` is read as a
//! `KEY=VALUE` data arg and never reaches the field slot, `-weird` is read
//! as a flag, and a leading `,` makes the token a comma-list that coerces
//! to an array (da-b049). Every one of those could be *declared* through
//! `spec`, which created a field the CLI and the browser then both
//! refused to log to.
//!
//! Whitespace is allowed inside a name (da-7h73h, vividn: «Allow white
//! space, disallow commas»): `my coffee` is one token once it is quoted,
//! and everything that writes a field into a typed line quotes it —
//! `shell_join` here, `shellJoin` in the web. Only a name that starts or
//! ends with a space is refused, because it reads as a different field
//! from the one it looks like.
//!
//! So the rule is enforced where names are chosen — [`check`] runs inside
//! `spec`'s parser, the one door both the terminal and datum.day go
//! through. Logging itself stays permissive, so no entry already in a
//! database becomes unreachable, and `datum_core::slug::validate_field_name`
//! — the storage-layer guard, which rejects only an empty name and a `:` —
//! stays as it is. This rule is the stricter of the two, so a name that may
//! be declared can always be stored.
//!
//! Nothing is migrated. A database that already holds a `cups=2` spec keeps
//! it, and its chip goes on erroring when pressed — the parser cannot
//! address that name from any surface, so there is no argv a migration
//! could rewrite the press into. What there is, is a way out: `spec F --delete`
//! takes a bare positional and reaches all three names, from the terminal
//! and from datum.day alike, and (+) can no longer make another.
//!
//! The rule is deliberately about *syntax*, not about alphabet. `café`,
//! `日本語`, `emoji☕` and `très-bien` are all fine, and must stay fine.
//!
//! `web/src/lib/fieldName.ts` mirrors this for the `+` form's inline
//! hint, and `tests/fixtures/field-name.json` is the case list both sides'
//! tests read, so the two cannot drift apart in silence.

use crate::alias::expand::is_datum_space;
use crate::error::{ParseError, Result};

/// The characters a field name may not contain: `=` opens a data key, `,` opens a list or an entry chain, and `:` is the
/// slug's own boundary (`field:2024-03-16T09:15Z`).
const FORBIDDEN: [char; 3] = ['=', ',', ':'];

/// The rule, as a sentence to show someone who broke it.
pub const RULE: &str = "a field name has no equals, commas or colons, and \
                        cannot start with a dash or start or end with a space";

/// Is `name` a field name that can be logged to?
///
/// The error message states [`RULE`] and, where one exists, offers the
/// nearest name that keeps it — see [`suggest`].
pub fn check(name: &str) -> Result<()> {
    if !name.is_empty() && !breaks_the_rule(name) {
        return Ok(());
    }
    let mut message = format!("cannot declare '{name}' — {RULE}");
    if let Some(fixed) = suggest(name) {
        message.push_str(&format!(". Try {fixed}"));
    }
    Err(ParseError::InvalidArg(message))
}

/// The nearest usable name, or `None` when nothing usable is left.
///
/// Every run of forbidden characters becomes a single `_`, and spaces and
/// `_` at either end and a leading `-` are dropped: `cups=2` → `cups_2`,
/// `-weird` → `weird`, ` my coffee ` → `my coffee`.
pub fn suggest(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if FORBIDDEN.contains(&c) {
            if !out.ends_with('_') {
                out.push('_');
            }
        } else {
            out.push(c);
        }
    }
    let fixed = out
        .trim_start_matches(|c| is_datum_space(c) || c == '-' || c == '_')
        .trim_end_matches(|c| is_datum_space(c) || c == '_');
    (!fixed.is_empty()).then(|| fixed.to_string())
}

fn breaks_the_rule(name: &str) -> bool {
    name.starts_with('-')
        || name.starts_with(is_datum_space)
        || name.ends_with(is_datum_space)
        || name.contains(FORBIDDEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_names_the_rule_and_the_fix() {
        let e = check("cups=2").unwrap_err().to_string();
        assert!(e.contains(RULE), "{e}");
        assert!(e.contains("Try cups_2"), "{e}");
    }

    /// A name with nothing usable left has no suggestion to offer, and the
    /// message must still be a message rather than a dangling "Try ".
    #[test]
    fn a_name_that_is_all_punctuation_suggests_nothing() {
        assert_eq!(suggest(" , = "), None);
        let e = check(",").unwrap_err().to_string();
        assert!(!e.contains("Try"), "{e}");
    }
}
