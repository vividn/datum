//! Color resolution for field specs: a kit's `color` and `spec set --color`.
//!
//! A colour value can be one of three forms:
//!
//! 1. **Hex or ANSI-256** — `#RRGGBB` or `#RRGGBBAA`, `#` optional. Stored with its
//!    `#`, lowercased; an ANSI-256 number (`196`) is kept as typed.
//! 2. **Hue name** — `red`, `orange`, `yellow`, `lime`, `green`, `mint`,
//!    `cyan`, `azure`, `blue`, `violet`, `magenta`, `pink`. Daydumb
//!    extension (no datum equivalent). Resolved at install time using the
//!    field name's md5 to pick a deterministic saturation/lightness within
//!    the named hue's 30°-wide range.
//! 3. **Missing** — `None`. No color is stored on the spec. The renderer
//!    derives one at paint time via the datum-compatible `md5_rgb` (first
//!    three bytes of md5(name) as RGB), matching original datum's
//!    `md5Color = "#" + md5(name).slice(0, 6)`.
//!
//! For named hues, saturation range is `[15%, 100%]` and lightness range is
//! `[15%, 85%]`. The three components are pulled from the first 12 bytes of
//! md5(field_name) (4 bytes per component) so resolution is stable across
//! runs/platforms.

use md5::{Digest, Md5};

use crate::error::{DatumError, Result};

/// 12 base hues at 30° intervals, named by their visible-spectrum landmark.
/// Index `i` maps to hue degree `i * 30`.
pub const NAMED_HUES: &[(&str, f64)] = &[
    ("red", 0.0),
    ("orange", 30.0),
    ("yellow", 60.0),
    ("lime", 90.0),
    ("green", 120.0),
    ("mint", 150.0),
    ("cyan", 180.0),
    ("azure", 210.0),
    ("blue", 240.0),
    ("violet", 270.0),
    ("magenta", 300.0),
    ("pink", 330.0),
];

const SAT_MIN: f64 = 15.0;
const SAT_MAX: f64 = 100.0;
const LIGHT_MIN: f64 = 15.0;
const LIGHT_MAX: f64 = 85.0;
const HUE_RANGE_DEG: f64 = 30.0;

/// Resolve a user-typed color into what the spec stores: the one colour
/// rule for a kit's `color` and `spec set --color` / `--state-color` /
/// `add-state`'s COLOR alike (da-xw1rx), and only what the renderer draws.
///
/// * hex, six or eight digits, `#` optional → lowercase `#rrggbb[aa]`.
/// * an ANSI-256 number → kept as the number (`196`).
/// * `Some(hue_name)` → HSL-from-md5 within the named hue's 30°-wide band
///   (daydumb extension — see module docs).
/// * anything else is refused, naming the accepted forms.
/// * `None` → returns `Ok(None)`. The caller leaves `spec.color` unset so
///   the renderer derives the color via the datum-compatible `md5_rgb`
///   (first three bytes of md5(name) as RGB).
pub fn resolve_color(input: Option<&str>, field_name: &str) -> Result<Option<String>> {
    input.map(|s| resolve(s, field_name)).transpose()
}

/// [`resolve_color`] of a colour that is given.
pub fn resolve(s: &str, field_name: &str) -> Result<String> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    if matches!(hex.len(), 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(format!("#{}", hex.to_lowercase()));
    }
    if s.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(n) = s.parse::<u8>() {
            return Ok(n.to_string());
        }
    }
    let lc = s.to_lowercase();
    match NAMED_HUES.iter().find(|(n, _)| *n == lc) {
        Some((_, base_hue)) => Ok(hsl_from_md5(field_name, *base_hue)),
        None => Err(DatumError::Other(format!(
            "'{s}' is not a colour: expected six hex digits (`#` optional), \
             an ANSI-256 number (0-255) or a hue name ({})",
            NAMED_HUES
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Hold the colours a write of a spec's JSON CHANGES to the rule
/// (da-eciuy): `color`, and each of `state_colors`, that differs from
/// `old`'s is resolved by [`resolve`] or refused, naming its key. One
/// stored before the rule and sent back untouched — datum.day's field form
/// saves a spec whole — is kept as it is, and so is an empty one.
pub fn resolve_changed(
    spec: &mut serde_json::Value,
    old: Option<&serde_json::Value>,
) -> Result<()> {
    let field = spec["field"].as_str().unwrap_or_default().to_string();
    let fix = |c: &mut serde_json::Value, path: &[&str], seed: &str| match c {
        serde_json::Value::String(s)
            if !s.is_empty()
                && path
                    .iter()
                    .fold(old, |v, k| v.and_then(|v| v.get(k)))
                    .and_then(|v| v.as_str())
                    != Some(s) =>
        {
            resolve(s, seed)
                .map(|r| *s = r)
                .map_err(|e| DatumError::Other(format!("{}: {e}", path.join("."))))
        }
        _ => Ok(()),
    };
    if let Some(c) = spec.get_mut("color") {
        fix(c, &["color"], &field)?;
    }
    if let Some(serde_json::Value::Object(m)) = spec.get_mut("state_colors") {
        for (state, c) in m.iter_mut() {
            fix(c, &["state_colors", state], state)?;
        }
    }
    Ok(())
}

/// Build an HSL color from md5(field_name) constrained to the named hue's
/// 30°-wide band starting at `base_hue`. Used for the daydumb-extension
/// named-hue path (`color = "blue"` etc.).
fn hsl_from_md5(field_name: &str, base_hue: f64) -> String {
    let digest = Md5::digest(field_name.as_bytes());
    // 4 bytes per component → u32 each, divide by 2^32 to get [0, 1).
    let h_raw = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    let s_raw = u32::from_be_bytes([digest[4], digest[5], digest[6], digest[7]]);
    let l_raw = u32::from_be_bytes([digest[8], digest[9], digest[10], digest[11]]);
    let h_unit = f64::from(h_raw) / (f64::from(u32::MAX) + 1.0);
    let s_unit = f64::from(s_raw) / (f64::from(u32::MAX) + 1.0);
    let l_unit = f64::from(l_raw) / (f64::from(u32::MAX) + 1.0);

    let hue_deg = base_hue + h_unit * HUE_RANGE_DEG;
    let sat_pct = SAT_MIN + s_unit * (SAT_MAX - SAT_MIN);
    let light_pct = LIGHT_MIN + l_unit * (LIGHT_MAX - LIGHT_MIN);

    let (r, g, b) = hsl_to_rgb(hue_deg, sat_pct / 100.0, light_pct / 100.0);
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

/// Standard HSL → RGB conversion. Hue in `[0, 360)`, sat/light in `[0, 1]`.
/// Returns 8-bit RGB.
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = (h % 360.0) / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match h_prime as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        5 => (c, 0.0, x),
        _ => (0.0, 0.0, 0.0),
    };
    let m = l - c / 2.0;
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_passthrough_lowercases() {
        assert_eq!(
            resolve_color(Some("#1A3D6E"), "sleep").unwrap().as_deref(),
            Some("#1a3d6e")
        );
        assert_eq!(
            resolve_color(Some("#1a3d6eFF"), "sleep")
                .unwrap()
                .as_deref(),
            Some("#1a3d6eff")
        );
    }

    #[test]
    fn rejects_bad_hex() {
        assert!(resolve_color(Some("#xyz"), "f").is_err());
        assert!(resolve_color(Some("#12"), "f").is_err());
        assert!(resolve_color(Some("#1234567"), "f").is_err());
    }

    #[test]
    fn named_hue_is_deterministic() {
        let a = resolve_color(Some("blue"), "sleep").unwrap().unwrap();
        let b = resolve_color(Some("blue"), "sleep").unwrap().unwrap();
        assert_eq!(a, b);
        assert!(a.starts_with('#'));
        assert_eq!(a.len(), 7);
    }

    #[test]
    fn rejects_unknown_hue_name() {
        let err = resolve_color(Some("aubergine"), "f").unwrap_err();
        assert!(format!("{err}").contains("is not a colour"), "{err}");
    }

    #[test]
    fn bare_hex_and_ansi_numbers_are_colours_the_renderer_draws() {
        let c = |s| resolve_color(Some(s), "f").unwrap();
        assert_eq!(c("B87333").as_deref(), Some("#b87333"));
        assert_eq!(c("000000").as_deref(), Some("#000000"));
        assert_eq!(c("196").as_deref(), Some("196"));
        assert_eq!(c("0").as_deref(), Some("0"));
        for bad in ["256", "-1", "+5", "#196", "abc", ""] {
            assert!(resolve_color(Some(bad), "f").is_err(), "{bad}");
        }
    }

    #[test]
    fn missing_color_returns_none_so_renderer_uses_datum_md5() {
        // Missing color must NOT pre-resolve at install time. Returning None
        // leaves spec.color unset, and the renderer's md5_rgb (first 3 bytes
        // of md5(name) as RGB) produces colors identical to original datum's
        // `md5Color = "#" + md5(str).slice(0, 6)`.
        assert_eq!(resolve_color(None, "sleep").unwrap(), None);
        assert_eq!(resolve_color(None, "coffee").unwrap(), None);
        assert_eq!(resolve_color(None, "").unwrap(), None);
    }

    #[test]
    fn named_hues_span_correct_band() {
        // Pick "red" (0°) and verify the resolved RGB has dominant red.
        let hex = resolve_color(Some("red"), "alpha-zulu").unwrap().unwrap();
        let r = u8::from_str_radix(&hex[1..3], 16).unwrap();
        let g = u8::from_str_radix(&hex[3..5], 16).unwrap();
        let b = u8::from_str_radix(&hex[5..7], 16).unwrap();
        // Red band [0°, 30°): R should be the dominant channel.
        assert!(
            r >= g && r >= b,
            "red-band hex {hex} should have R dominant (r={r} g={g} b={b})"
        );
    }

    #[test]
    fn named_hue_case_insensitive() {
        assert_eq!(
            resolve_color(Some("Blue"), "f").unwrap(),
            resolve_color(Some("blue"), "f").unwrap(),
        );
    }

    #[test]
    fn hsl_to_rgb_known_values() {
        // Pure red: H=0, S=1, L=0.5 → (255, 0, 0)
        let (r, g, b) = hsl_to_rgb(0.0, 1.0, 0.5);
        assert_eq!((r, g, b), (255, 0, 0));
        // Pure green: H=120
        let (r, g, b) = hsl_to_rgb(120.0, 1.0, 0.5);
        assert_eq!((r, g, b), (0, 255, 0));
        // Pure blue: H=240
        let (r, g, b) = hsl_to_rgb(240.0, 1.0, 0.5);
        assert_eq!((r, g, b), (0, 0, 255));
    }
}
