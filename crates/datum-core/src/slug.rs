//! Slug field-name validation.
//!
//! Slugs anchor every entry with a human reference (`field:<ident>`) that
//! stays stable across devices. Construction lives at the call sites
//! ([`crate::entry::build_slug`] for the default field+timestamp case,
//! and the `id_structure` template interpolation in `datum-commands`);
//! this module owns the shared field-name guard those paths rely on.
//!
//! da-uxgv retired the one slug shape a call site could not invent for
//! itself — the "parked" `<slug>_<hid>` form a sync merge moved a loser to.
//! Merge state now lives in `entries.superseded_by`, so a slug is purely
//! derived again: every path that re-derives one (`retime`, `datum edit`,
//! `datum check --fix`) can do so with no exception to remember.

use crate::error::{DatumError, Result};

/// Reject field names that would break slug parsing.
///
/// Slugs are `<field>:<id>` and [`crate::doc_ref`] splits on the first `:`,
/// so a colon in `field` makes the boundary ambiguous. Empty field names
/// are also rejected — they collapse the slug to `:id` and are never a
/// user intent.
///
/// This is the *storage* guard, and it stays this permissive on purpose:
/// tightening it would strand entries already written. The stricter rule
/// for a name being *declared* is `datum_parse::field_name::check`, which
/// `setup` runs — it is a superset of this one, so anything that may be
/// declared may also be stored (da-b049). A field template's placeholders
/// are not its text (`issue__%hid:8%`, da-38obu): only its literal text is
/// held to the `:`.
pub fn validate_field_name(field: &str) -> Result<()> {
    if field.is_empty() {
        return Err(DatumError::InvalidFieldSpec(
            "field name cannot be empty".to_string(),
        ));
    }
    if datum_parse::field_template::literal(field).contains(':') {
        return Err(DatumError::InvalidFieldSpec(format!(
            "field name cannot contain ':' (got {field:?})"
        )));
    }
    Ok(())
}

/// Reject a `,` in a field name being chosen for an entry (da-6p3du): the
/// CLI splits `undo a,b` and `history a,b` on it and reads a lone `,` as
/// the entry separator, so such a field could never be named in a list.
/// Checked where a name is chosen — a logged entry's field once aliases and
/// its template have expanded, a field an update or `edit` moves it to, and a spec
/// written for a field that has none (`track`, `setup`, a kit's
/// `[[field]]`) — not in
/// [`validate_field_name`], which a restore writes through too, so a row
/// that already holds one stays restorable. Spaces stay allowed; an ASCII
/// control character (`\n`, `\t`, the C0 range, DEL) is refused the same
/// way, since every line- and tab-separated surface — `tail`, `fields`, the
/// shell completion's `value<TAB>description` — breaks on it (da-fi2vu).
/// The C1 range is not: `tests/fixtures/field-name.json` accepts U+0085.
pub fn validate_new_field_name(field: &str) -> Result<()> {
    if field.contains(',') {
        return Err(DatumError::InvalidFieldSpec(format!(
            "field name cannot contain ',' (got {field:?})"
        )));
    }
    if let Some(c) = field.chars().find(char::is_ascii_control) {
        let name = match c {
            '\n' => "a newline",
            '\r' => "a carriage return",
            '\t' => "a tab",
            _ => "a control character",
        };
        return Err(DatumError::InvalidFieldSpec(format!(
            "field name cannot contain {name} (U+{:04X}): {field:?}",
            c as u32
        )));
    }
    Ok(())
}

/// Reject a slug containing `::`, which `datum get <id>::<name>` reads as
/// the boundary between an id and the value it names (da-u7mgc). Refused
/// rather than escaped: no slug in any database we hold contained one when
/// the syntax arrived. Only a slug being written is checked, so a row that
/// already holds one stays readable and deletable. A sync merge
/// (`upsert_lww`) does not call it on purpose: a row a peer wrote is stored
/// as it arrives, or the devices would stop converging.
pub fn validate_slug(slug: &str) -> Result<()> {
    if slug.contains("::") {
        return Err(DatumError::Other(format!(
            "an id cannot contain '::', which `get <id>::<name>` reads as its boundary (got {slug:?})"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_field_name_accepts_normal_names() {
        validate_field_name("coffee").unwrap();
        validate_field_name("mood").unwrap();
        validate_field_name("word").unwrap();
        validate_field_name("a-b_c.0").unwrap();
    }

    #[test]
    fn validate_field_name_rejects_colon() {
        let err = validate_field_name("weird:field").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("':'"), "got: {msg}");
        // Just a leading or trailing colon also rejected.
        assert!(validate_field_name(":x").is_err());
        assert!(validate_field_name("x:").is_err());
    }

    #[test]
    fn validate_field_name_rejects_empty() {
        let err = validate_field_name("").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }
}
