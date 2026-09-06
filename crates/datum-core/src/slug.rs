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
/// `spec set` runs — it is a superset of this one, so anything that may be
/// declared may also be stored (da-b049).
pub fn validate_field_name(field: &str) -> Result<()> {
    if field.is_empty() {
        return Err(DatumError::InvalidFieldSpec(
            "field name cannot be empty".to_string(),
        ));
    }
    if field.contains(':') {
        return Err(DatumError::InvalidFieldSpec(format!(
            "field name cannot contain ':' (got {field:?})"
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
