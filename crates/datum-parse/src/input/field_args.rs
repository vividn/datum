//! `fieldArgs` — positional `field` + `-F/--fieldless`. Port of datum's
//! `src/input/fieldArgs.ts`.
//!
//! The TS module is two flags wired through argparse; in Rust we carry the
//! same two values on a struct and expose a [`FieldArgs::resolved`] helper
//! that returns `None` when `--fieldless` is set. Callers that want to feed
//! the field into a `DataArgs` stream can chain [`FieldArgs::resolved`]
//! through [`crate::input::flexible_positional`].
//!
//! See `docs/parity-audit/parsing.md` ("`fieldArgs.ts`") for the full spec.

/// Parsed shape of the `field` / `--fieldless` flag pair.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldArgs {
    /// Positional `field` argument (e.g. `coffee` in `datum occur coffee`).
    pub field: Option<String>,
    /// `-F`/`--fieldless`: suppress the positional. When set, callers
    /// should behave as though no field was supplied even if `field` has
    /// a value — matching datum's argparse-level suppression.
    pub fieldless: bool,
}

impl FieldArgs {
    /// Effective field value after `--fieldless` is applied. Returns
    /// `None` when the user opted out of the positional, regardless of
    /// whether a stale `field` value is sitting on the struct.
    pub fn resolved(&self) -> Option<&str> {
        if self.fieldless {
            None
        } else {
            self.field.as_deref()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_defaults_to_no_field() {
        assert_eq!(FieldArgs::default().resolved(), None);
    }

    #[test]
    fn plain_field_passes_through() {
        let a = FieldArgs { field: Some("coffee".into()), fieldless: false };
        assert_eq!(a.resolved(), Some("coffee"));
    }

    #[test]
    fn fieldless_flag_suppresses_positional() {
        let a = FieldArgs { field: Some("coffee".into()), fieldless: true };
        assert_eq!(a.resolved(), None);
    }

    #[test]
    fn fieldless_flag_without_value_still_returns_none() {
        let a = FieldArgs { field: None, fieldless: true };
        assert_eq!(a.resolved(), None);
    }
}
