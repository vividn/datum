//! Color derivation and ANSI painting for the visual-parity renderer.
//!
//! See `docs/visual-parity-analysis.md` §§1, 5, 8.
//!
//! * Fields and named states derive a stable RGB from the MD5 of their name
//!   (`r = hash[0]`, `g = hash[1]`, `b = hash[2]`) unless a spec override
//!   (`FieldSpec.color` / `state_colors`) is present.
//! * YIQ brightness picks black / white as the contrast foreground so the
//!   badge text is readable regardless of the derived background.
//! * Escapes are emitted as ANSI truecolor (`48;2;R;G;B` for bg,
//!   `38;2;R;G;B` for fg).

use datum_core::field_spec::FieldSpec;
use md5::{Digest, Md5};
use serde_json::Value as JsonValue;
use std::io::IsTerminal;

use super::characters::IGNORED_STATE;

// ─── ANSI primitives ─────────────────────────────────────────────────────────

pub(crate) const RESET: &str = "\x1b[0m";
pub(crate) const UNDERLINE: &str = "\x1b[4m";
pub(crate) const GRAY_FG: &str = "\x1b[90m";

// ─── RGB + YIQ ───────────────────────────────────────────────────────────────

/// RGB color triple.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };
    pub const WHITE: Rgb = Rgb {
        r: 255,
        g: 255,
        b: 255,
    };
    /// `#888888` — the shared "ignored / null" grey per §8. Doubles as the
    /// fg of the empty LEAD/TRAIL slot rendered when there's no prior or
    /// continuing state.
    pub const IGNORED: Rgb = Rgb {
        r: 0x88,
        g: 0x88,
        b: 0x88,
    };
    /// `#666666` — foreground over IGNORED bg per §5.
    pub const IGNORED_FG: Rgb = Rgb {
        r: 0x66,
        g: 0x66,
        b: 0x66,
    };

    pub fn fg(self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.r, self.g, self.b)
    }
    pub fn bg(self) -> String {
        format!("\x1b[48;2;{};{};{}m", self.r, self.g, self.b)
    }
}

/// YIQ perceived brightness, matching datum's `getColorBrightness`.
fn brightness(rgb: Rgb) -> u32 {
    (rgb.r as u32 * 299 + rgb.g as u32 * 587 + rgb.b as u32 * 114) / 1000
}

/// Pick black or white to contrast against `bg`, matching datum's
/// `getContrastTextColor` default path.
pub(crate) fn contrast_fg(bg: Rgb) -> Rgb {
    if brightness(bg) > 128 {
        Rgb::BLACK
    } else {
        Rgb::WHITE
    }
}

pub(crate) fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    Some(Rgb {
        r: u8::from_str_radix(&s[0..2], 16).ok()?,
        g: u8::from_str_radix(&s[2..4], 16).ok()?,
        b: u8::from_str_radix(&s[4..6], 16).ok()?,
    })
}

/// Derive a stable RGB color from `md5(name)` — §1. No re-rolling; every MD5
/// output is a valid background.
pub(crate) fn md5_rgb(name: &str) -> Rgb {
    let hash = Md5::digest(name.as_bytes());
    Rgb {
        r: hash[0],
        g: hash[1],
        b: hash[2],
    }
}

/// ANSI-256 cube index → approximate RGB. Used when a spec's `color` was
/// stored as a legacy ANSI-256 number instead of a hex string.
fn ansi256_rgb(n: u8) -> Rgb {
    if n < 16 {
        return Rgb {
            r: 128,
            g: 128,
            b: 128,
        };
    }
    if n >= 232 {
        let v = 8 + (n - 232) * 10;
        return Rgb { r: v, g: v, b: v };
    }
    let idx = n - 16;
    let r = idx / 36;
    let g = (idx % 36) / 6;
    let b = idx % 6;
    let q = |c: u8| -> u8 {
        if c == 0 {
            0
        } else {
            55 + 40 * c
        }
    };
    Rgb {
        r: q(r),
        g: q(g),
        b: q(b),
    }
}

/// Resolve a stored color string (hex / ANSI-256 number) or fall back to
/// `md5(fallback_name)`. Returns an RGB triple suitable for both fg and bg.
pub(crate) fn resolve_rgb(stored: Option<&str>, fallback_name: &str) -> Rgb {
    if let Some(s) = stored {
        if let Some(rgb) = parse_hex(s) {
            return rgb;
        }
        if let Ok(n) = s.trim_start_matches('#').parse::<u8>() {
            return ansi256_rgb(n);
        }
    }
    md5_rgb(fallback_name)
}

/// Resolve a field's RGB color: stored spec override → md5(field).
pub(crate) fn field_rgb(field: &str, spec: Option<&FieldSpec>) -> Rgb {
    resolve_rgb(spec.and_then(|s| s.color.as_deref()), field)
}

/// Resolve the RGB color of a named/scalar state per §5.
///
/// Precedence: per-state spec override → md5(state). Note that the field's
/// color is **not** used as the fallback — the original TS `getStateColor`
/// for named states always hashes the state name (see fieldColor.ts:47).
pub(crate) fn state_rgb(state: &str, spec: Option<&FieldSpec>) -> Rgb {
    if let Some(s) = spec {
        if let Some(c) = s.state_colors.get(state) {
            return resolve_rgb(Some(c.as_str()), state);
        }
    }
    md5_rgb(state)
}

/// Resolve an arbitrary JSON state value to its `getStateColor` result per §5.
/// Returns `None` for `Bool(false)` — the "field has ended" sentinel — so
/// LEAD/TRAIL slots can render terminal-default whitespace instead of a
/// solid black bg cell. See `action::render_slot` for how the `None` case
/// flows into [`SlotKind::FieldEnded`].
pub(crate) fn state_value_rgb(
    value: &JsonValue,
    field: &str,
    spec: Option<&FieldSpec>,
) -> Option<Rgb> {
    match value {
        JsonValue::Null => Some(Rgb::IGNORED),
        JsonValue::Bool(true) => Some(field_rgb(field, spec)),
        JsonValue::Bool(false) => None,
        JsonValue::String(s) if s == IGNORED_STATE => Some(Rgb::IGNORED),
        JsonValue::String(s) => Some(state_rgb(s, spec)),
        JsonValue::Array(items) => {
            // Array → first element recursive; §5.
            items.first().and_then(|v| state_value_rgb(v, field, spec))
        }
        _ => Some(state_rgb(&value.to_string(), spec)),
    }
}

/// Resolve a JSON state value to one [`Rgb`] per parallel-state member.
///
/// Per da-9vop, a parallel state set (`switch body vertical,sit`) lands in
/// the entry as a JSON array; each member must keep its own colour rather
/// than the renderer borrowing only the first state's hue. Returns:
///
/// * Array → one `Rgb` per resolvable member (unresolvable members like
///   `Bool(false)` are skipped, so a 2-element array with one bogus value
///   degrades to a 1-element vec).
/// * Scalar → 1-element vec, or empty when the scalar resolves to `None`
///   (only `Bool(false)` does today).
pub(crate) fn state_value_colors(
    value: &JsonValue,
    field: &str,
    spec: Option<&FieldSpec>,
) -> Vec<Rgb> {
    match value {
        JsonValue::Array(items) => items
            .iter()
            .filter_map(|v| state_value_rgb(v, field, spec))
            .collect(),
        _ => state_value_rgb(value, field, spec).into_iter().collect(),
    }
}

// ─── TTY / color gating ──────────────────────────────────────────────────────

/// ANSI colors are suppressed when stdout is not a TTY, when `NO_COLOR` is
/// set, or when the test-only `DATUM_COLOR=0`/`never` override is in force.
///
/// Precedence: `DATUM_COLOR` (explicit) → `NO_COLOR` → TTY detection.
pub(crate) fn color_enabled() -> bool {
    if let Some(f) = force_color() {
        return f;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal()
}

fn force_color() -> Option<bool> {
    match std::env::var("DATUM_COLOR").ok().as_deref() {
        Some("1") | Some("always") => Some(true),
        Some("0") | Some("never") => Some(false),
        _ => None,
    }
}

/// Paint `body` with an ANSI sequence if color is enabled.
pub(crate) fn paint(code: &str, body: &str) -> String {
    if !color_enabled() || code.is_empty() {
        body.to_string()
    } else {
        format!("{code}{body}{RESET}")
    }
}

/// Paint with bg + contrast-derived fg. The "background is primary" style.
pub(crate) fn paint_bg(bg: Rgb, body: &str) -> String {
    if !color_enabled() {
        return body.to_string();
    }
    let fg = contrast_fg(bg);
    format!("{}{}{}{}", bg.bg(), fg.fg(), body, RESET)
}

/// Paint with both explicit bg + fg (no contrast derivation). Used for the
/// checkerboard case where fg and bg are both meaningful state colors.
pub(crate) fn paint_bg_fg(bg: Rgb, fg: Rgb, body: &str) -> String {
    if !color_enabled() {
        return body.to_string();
    }
    format!("{}{}{}{}", bg.bg(), fg.fg(), body, RESET)
}

// ─── ANSI helpers (round-trip safe for tests) ────────────────────────────────

/// Strip all ANSI escape sequences. Used by fixture tests to assert plain
/// layout independent of color.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_esc = false;
    for c in s.chars() {
        if in_esc {
            if c == 'm' {
                in_esc = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_esc = true;
            continue;
        }
        out.push(c);
    }
    out
}

/// Count visible characters, ignoring ANSI escapes.
pub fn visible_width(s: &str) -> usize {
    let mut in_esc = false;
    let mut width = 0usize;
    for c in s.chars() {
        if in_esc {
            if c == 'm' {
                in_esc = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_esc = true;
            continue;
        }
        width += 1;
    }
    width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_rgb_is_deterministic() {
        assert_eq!(md5_rgb("coffee"), md5_rgb("coffee"));
        assert_ne!(md5_rgb("coffee"), md5_rgb("tea"));
    }

    #[test]
    fn md5_rgb_matches_first_three_hash_bytes() {
        // md5("") = d41d8cd98f00b204e9800998ecf8427e → r=0xd4, g=0x1d, b=0x8c
        let rgb = md5_rgb("");
        assert_eq!(
            rgb,
            Rgb {
                r: 0xd4,
                g: 0x1d,
                b: 0x8c
            }
        );
    }

    #[test]
    fn contrast_flips_on_yiq_boundary() {
        assert_eq!(contrast_fg(Rgb::WHITE), Rgb::BLACK);
        assert_eq!(contrast_fg(Rgb::BLACK), Rgb::WHITE);
    }

    #[test]
    fn parse_hex_accepts_prefix() {
        assert_eq!(
            parse_hex("#ff6600"),
            Some(Rgb {
                r: 0xff,
                g: 0x66,
                b: 0x00
            })
        );
        assert_eq!(
            parse_hex("ff6600"),
            Some(Rgb {
                r: 0xff,
                g: 0x66,
                b: 0x00
            })
        );
        assert_eq!(parse_hex("xxx"), None);
    }

    #[test]
    fn strip_ansi_preserves_plain_text() {
        assert_eq!(strip_ansi("\x1b[32mhi\x1b[0m"), "hi");
    }

    #[test]
    fn visible_width_ignores_escapes() {
        assert_eq!(visible_width("\x1b[32m●\x1b[0m"), 1);
    }

    /// Per da-9vop, a parallel state set (`["vertical","sit"]`) must yield
    /// one [`Rgb`] per member so each state renders in its own colour
    /// rather than collapsing onto the first state's hue.
    #[test]
    fn state_value_colors_returns_one_per_parallel_member() {
        let value = JsonValue::Array(vec![
            JsonValue::String("vertical".into()),
            JsonValue::String("sit".into()),
        ]);
        let cs = state_value_colors(&value, "body", None);
        assert_eq!(cs.len(), 2, "two-member array → two colours");
        assert_eq!(cs[0], state_rgb("vertical", None), "first → vertical");
        assert_eq!(cs[1], state_rgb("sit", None), "second → sit");
        assert_ne!(cs[0], cs[1], "different states → different colours");
    }

    /// Single scalar input degrades to a one-element vec — the same
    /// rendering surface as today's solid-block path.
    #[test]
    fn state_value_colors_for_scalar_returns_single_element() {
        let value = JsonValue::String("happy".into());
        let cs = state_value_colors(&value, "mood", None);
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0], state_rgb("happy", None));
    }

    /// `Bool(false)` (the "field has ended" sentinel) resolves to no
    /// colour at all, matching `state_value_rgb`'s `None` return.
    #[test]
    fn state_value_colors_for_field_ended_is_empty() {
        let cs = state_value_colors(&JsonValue::Bool(false), "f", None);
        assert!(cs.is_empty(), "Bool(false) → no colour");
    }
}
