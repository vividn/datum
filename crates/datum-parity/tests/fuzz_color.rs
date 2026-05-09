//! Property tests for the color algorithm port.
//!
//! These tests don't require the `parity-runtime` feature — they verify
//! invariants that any RFC-1321 md5 implementation guarantees, plus the
//! branch coverage of `state_color` against known-good reference outputs.
//!
//! ≥200 cases per property satisfies the bead's CI fuzz floor.

use datum_parity::colors::{
    contrast_text_color, field_color, md5_color, parse_hex_rgb, state_color,
};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// `md5_color` always produces a 7-character `#` + 6-hex string.
    /// Property: structural validity holds for any input, including
    /// non-UTF8-clean bytes (we accept &str so the input is UTF-8 by
    /// construction, but we still want to assert the output shape).
    #[test]
    fn md5_color_always_valid_hex(input in any::<String>()) {
        let c = md5_color(&input);
        prop_assert_eq!(c.len(), 7);
        prop_assert!(c.starts_with('#'));
        let parsed = parse_hex_rgb(&c);
        prop_assert!(
            parsed.is_some(),
            "md5_color({:?}) = {} is not parseable",
            input,
            c,
        );
    }

    /// Determinism: same input → same output, every time.
    #[test]
    fn md5_color_is_deterministic(input in any::<String>()) {
        let a = md5_color(&input);
        let b = md5_color(&input);
        prop_assert_eq!(a, b);
    }

    /// `field_color` with `None` always returns white, regardless of
    /// whether a spec color was passed (consistent with datum's
    /// `getFieldColor(undefined) = "#ffffff"` short-circuit).
    #[test]
    fn field_color_none_is_white(spec in proptest::option::of("#[0-9a-f]{6}")) {
        let c = field_color(None, spec.as_deref());
        prop_assert_eq!(c, "#ffffff");
    }

    /// Spec override always wins over md5 for non-None field.
    #[test]
    fn field_color_spec_overrides_md5(
        field in "[a-z][a-z0-9_]{0,15}",
        spec in "#[0-9a-f]{6}"
    ) {
        let c = field_color(Some(&field), Some(&spec));
        prop_assert_eq!(c, spec);
    }

    /// `state_color(false)` is always `#000000`, regardless of field
    /// or spec context (datum's `getStateColor` hard-codes this).
    #[test]
    fn state_color_false_is_black(
        field in proptest::option::of("[a-z][a-z0-9_]{0,15}"),
        field_spec in proptest::option::of("#[0-9a-f]{6}"),
        state_spec in proptest::option::of("#[0-9a-f]{6}"),
    ) {
        let c = state_color(Some("false"), field.as_deref(), field_spec.as_deref(), state_spec.as_deref());
        prop_assert_eq!(c, "#000000");
    }

    /// `state_color(null)` is always `#888888` (datum's IGNORED grey).
    #[test]
    fn state_color_null_is_grey(
        field in proptest::option::of("[a-z][a-z0-9_]{0,15}"),
        field_spec in proptest::option::of("#[0-9a-f]{6}"),
        state_spec in proptest::option::of("#[0-9a-f]{6}"),
    ) {
        let c = state_color(Some("null"), field.as_deref(), field_spec.as_deref(), state_spec.as_deref());
        prop_assert_eq!(c, "#888888");
    }

    /// `state_color(named)` with a state-spec override wins over md5
    /// AND over the field-color path. This is the algorithm path that
    /// da-438y broke.
    #[test]
    fn state_color_state_spec_wins(
        field in "[a-z][a-z0-9_]{0,15}",
        state in "[a-z][a-z0-9_]{0,15}",
        field_spec in "#[0-9a-f]{6}",
        state_spec in "#[0-9a-f]{6}",
    ) {
        let c = state_color(Some(&state), Some(&field), Some(&field_spec), Some(&state_spec));
        prop_assert_eq!(c, state_spec);
    }

    /// `state_color(true)` resolves to the field color (spec OR md5 of
    /// field name), NOT to md5("true"). Branch order matters here:
    /// datum's `simplifyState` collapses `true` to the field color
    /// before the named-state branch runs.
    #[test]
    fn state_color_true_falls_through_to_field(field in "[a-z][a-z0-9_]{0,15}") {
        let c = state_color(Some("true"), Some(&field), None, None);
        prop_assert_eq!(c.clone(), md5_color(&field));
        prop_assert_ne!(c, md5_color("true"));
    }

    /// `contrast_text_color` always returns either pure black or pure
    /// white, never any other color in the default-message branch.
    #[test]
    fn contrast_is_binary(hex in "#[0-9a-f]{6}") {
        let fg = contrast_text_color(&hex);
        prop_assert!(fg == "#000000" || fg == "#ffffff", "got {fg}");
    }

    /// `contrast_text_color` is monotonic in brightness: brighter
    /// backgrounds get black foregrounds, darker ones get white. We
    /// can't easily express this with random hex, but we can pin
    /// either-way for the boundary cases.
    #[test]
    fn contrast_chooses_black_for_pure_white(_unit in 0u8..=255) {
        prop_assert_eq!(contrast_text_color("#ffffff"), "#000000");
        prop_assert_eq!(contrast_text_color("#000000"), "#ffffff");
    }
}
