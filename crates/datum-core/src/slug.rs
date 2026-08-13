//! Slug field-name validation and the parked-slug form.
//!
//! Slugs anchor every entry with a human reference (`field:<ident>`) that
//! stays stable across devices. Construction lives at the call sites
//! ([`crate::entry::build_slug`] for the default field+timestamp case,
//! and the `id_structure` template interpolation in `datum-commands`);
//! this module owns the shared field-name guard those paths rely on, plus
//! the one slug shape no call site may invent for itself — the parked slug
//! a sync merge moves a loser to ([`parked`] / [`is_parked`]).

use crate::error::{DatumError, Result};
use crate::ids::{Hid, Slug};

/// Reject field names that would break slug parsing.
///
/// Slugs are `<field>:<id>` and [`crate::doc_ref`] splits on the first `:`,
/// so a colon in `field` makes the boundary ambiguous. Empty field names
/// are also rejected — they collapse the slug to `:id` and are never a
/// user intent.
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

/// The parked form of `slug` for the row `hid`: the slug with `_<hid>`
/// appended.
///
/// A sync merge that finds two *different* rows contending for one slug keeps
/// both and moves the deterministic loser here rather than dropping it
/// (da-zlug). The suffix is the loser's own hid — unique (it is the primary
/// key) and RNG-free (unlike the insert path's `Rekey` suffix), which is what
/// lets two devices reach the same pair of slugs without coordinating.
pub fn parked(slug: &str, hid: &Hid) -> Slug {
    Slug(format!("{slug}_{hid}"))
}

/// Whether `slug` is already the [`parked`] form for `hid`.
///
/// Parking stores merge state in `slug`, a *derived* column that several
/// things recompute — `retime`, `datum edit`, `datum check --fix`. Each of
/// those asks this question first, so a re-derivation preserves the park
/// instead of rewriting the row back onto the slug parking moved it off
/// (da-l4uz). The hid suffix is its own marker, so parked-ness needs no
/// column of its own and nothing has to be migrated or synced to read it.
///
/// A template that interpolated the row's whole hid would answer yes here.
/// That is harmless: the re-derivation paths only *keep* the park when the
/// natural slug is genuinely held by another row, and such a row's natural
/// slug is held by nobody but itself.
pub fn is_parked(slug: &str, hid: &Hid) -> bool {
    slug.len() > hid.len() + 1 && slug.ends_with(&format!("_{hid}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parked_round_trips_through_is_parked() {
        let hid = Hid::from("x4kesway38ud");
        let parked_slug = parked("activity:2026-08-13T08:00:00.000Z", &hid);
        assert_eq!(
            parked_slug.as_str(),
            "activity:2026-08-13T08:00:00.000Z_x4kesway38ud"
        );
        assert!(is_parked(parked_slug.as_str(), &hid));
    }

    #[test]
    fn is_parked_rejects_unparked_and_other_hids() {
        let hid = Hid::from("x4kesway38ud");
        assert!(!is_parked("activity:2026-08-13T08:00:00.000Z", &hid));
        // Another row's park is not this row's park.
        assert!(!is_parked(
            "activity:2026-08-13T08:00:00.000Z_zzzzzzzzzzzz",
            &hid
        ));
        // A slug that is *only* the suffix has no base to park.
        assert!(!is_parked("_x4kesway38ud", &hid));
    }

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
