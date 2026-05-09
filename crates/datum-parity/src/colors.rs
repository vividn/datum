//! Color derivation reimplemented from datum's TypeScript reference.
//!
//! Source files (cross-checked against `target/parity-cache/datum/src/`):
//! * `utils/md5Color.ts` — `md5Color(str) = "#" + md5(str).slice(0, 6)`
//! * `utils/colorUtils.ts` — `getContrastTextColor` (YIQ brightness, > 128 → black)
//! * `field/fieldColor.ts` — `getFieldColor`, `getStateColor`, `stateChalk`
//!
//! These are reimplementations, not bindings. Property tests in
//! `tests/colors_proptest.rs` cover the fact that "datum's md5" is RFC-1321
//! md5 (so npm's `md5` package and Rust's `md-5` crate produce identical
//! bytes for any input); we don't shell out per-comparison.

use md5::{Digest, Md5};

/// Datum's `md5Color`: `"#" + md5(str).substring(0, 6)`.
///
/// Matches `target/parity-cache/datum/src/utils/md5Color.ts` byte-for-byte
/// for any input. da-438y was a routing bug that bypassed this exact path
/// in daydumb; the parity goldens for `colors/` lock the algorithm.
pub fn md5_color(input: &str) -> String {
    let digest = Md5::digest(input.as_bytes());
    format!("#{:02x}{:02x}{:02x}", digest[0], digest[1], digest[2])
}

/// YIQ perceived brightness, matching datum's `getColorBrightness`.
///
/// `(R*299 + G*587 + B*114) / 1000`. Used by the contrast-text helper to
/// pick black or white for foreground text over a derived background.
fn brightness(rgb: (u8, u8, u8)) -> u32 {
    let (r, g, b) = rgb;
    (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000
}

/// Parse `#RRGGBB` (with or without leading `#`) into an `(r, g, b)` triple.
///
/// `None` for malformed input. Both daydumb and datum tolerate either casing
/// for the hex digits but always emit lowercase.
pub fn parse_hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let s = hex.strip_prefix('#').unwrap_or(hex);
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Datum's default `getContrastTextColor`: `brightness > 128 ? "#000000" : "#ffffff"`.
///
/// Datum has typed message-modes (`info` | `warning` | `error`) but
/// daydumb's renderer only uses the default branch, so we don't model the
/// other branches here — diverge if the renderer ever grows them.
pub fn contrast_text_color(bg_hex: &str) -> &'static str {
    let rgb = parse_hex_rgb(bg_hex).unwrap_or((0, 0, 0));
    if brightness(rgb) > 128 {
        "#000000"
    } else {
        "#ffffff"
    }
}

/// Datum's `getFieldColor`: spec override or md5(field).
///
/// `spec_color` is what `mySpec.color` would return for the named field; if
/// `None`, falls back to md5. Matches `field/fieldColor.ts:getFieldColor`.
pub fn field_color(field: Option<&str>, spec_color: Option<&str>) -> String {
    match field {
        None => "#ffffff".to_owned(),
        Some(name) => spec_color
            .map(str::to_owned)
            .unwrap_or_else(|| md5_color(name)),
    }
}

/// Datum's `getStateColor` for scalar (non-array) state.
///
/// Mirrors `field/fieldColor.ts:getStateColor`:
/// * `None`             → field color (spec or md5)
/// * `Some("true")`     → field color
/// * `Some("false")`    → `#000000`
/// * `Some("null")`     → `#888888`  (datum's `null` branch — daydumb
///                                   renames this to `IGNORED`, see
///                                   `docs/visual-parity-analysis.md` §5)
/// * named state        → spec override OR md5(state)
///
/// Array-valued states aren't covered here — the renderer's checkerboard
/// path picks the first element and recurses, which the caller can model
/// by calling this function on `state[0]`.
pub fn state_color(
    state: Option<&str>,
    field: Option<&str>,
    field_spec_color: Option<&str>,
    state_spec_color: Option<&str>,
) -> String {
    match state {
        None => field_color(field, field_spec_color),
        Some("true") => field_color(field, field_spec_color),
        Some("false") => "#000000".to_owned(),
        Some("null") => "#888888".to_owned(),
        Some(name) => state_spec_color
            .map(str::to_owned)
            .unwrap_or_else(|| md5_color(name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Anchor: the empty-string md5 has the well-known prefix `d41d8c`.
    #[test]
    fn md5_empty_string_anchor() {
        assert_eq!(md5_color(""), "#d41d8c");
    }

    /// `true` and `false` as field/state names get their own md5 colors —
    /// these strings are content-addressable and match the goldens that
    /// already exist in `crates/datum-cli/tests/parity/md5_colors`.
    #[test]
    fn md5_true_false_anchors() {
        assert_eq!(md5_color("true"), "#b326b5");
        assert_eq!(md5_color("false"), "#68934a");
    }

    #[test]
    fn contrast_picks_black_for_yellow() {
        // #ffff00 brightness = 0.886 * 1000 = 886 > 128 → black.
        assert_eq!(contrast_text_color("#ffff00"), "#000000");
    }

    #[test]
    fn contrast_picks_white_for_navy() {
        // #000080 brightness = 14.6 → white.
        assert_eq!(contrast_text_color("#000080"), "#ffffff");
    }

    #[test]
    fn state_color_branches_match_datum() {
        // No spec → md5 fallback.
        assert_eq!(
            state_color(Some("happy"), None, None, None),
            md5_color("happy")
        );
        assert_eq!(state_color(Some("false"), None, None, None), "#000000");
        assert_eq!(state_color(Some("null"), None, None, None), "#888888");
        // Spec override wins over md5.
        assert_eq!(
            state_color(Some("happy"), None, None, Some("#abcdef")),
            "#abcdef"
        );
        // Field color falls through for `true` and undefined state.
        assert_eq!(
            state_color(Some("true"), Some("mood"), None, None),
            md5_color("mood")
        );
        assert_eq!(
            state_color(None, Some("mood"), None, None),
            md5_color("mood")
        );
    }
}
