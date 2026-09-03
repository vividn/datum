//! Colour derivation for the browser, byte-identical to the CLI's.
//!
//! `datum-layout` is deliberately colour-blind — it takes hex strings and
//! paints them. On the CLI those strings come from
//! `datum_commands::render::colors`, which is unreachable from
//! `wasm32-unknown-unknown` (it sits above `datum-core`, and `datum-core`
//! pulls rusqlite). This module is the same algorithm on the same inputs,
//! held to it by `crates/datum-commands/src/render/wasm_parity.rs`, which
//! compares the two implementations directly.
//!
//! See `docs/visual-parity-analysis.md` §§1, 5, 8 for the rules.

use md5::{Digest, Md5};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

/// The "ignored / null" sentinel state (`IGNORED_STATE_VALUE` in
/// `datum_core::db::types`).
pub const IGNORED_STATE: &str = "IGNORED";
/// `#888888` — the shared "ignored / null" grey per §8.
pub const IGNORED_HEX: &str = "#888888";
/// Black, the fallback background of a dot with no prevailing state.
pub const BLACK_HEX: &str = "#000000";

/// The parts of a field spec that affect how a field is drawn. The browser
/// reads these out of `field_specs.spec_json`, which is the same blob the
/// CLI deserializes into `FieldSpec`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FieldStyle {
    /// Spec colour override for the field itself.
    #[serde(default)]
    pub color: Option<String>,
    /// Per-state colour overrides.
    #[serde(default)]
    pub state_colors: BTreeMap<String, String>,
    /// Cross-axis position in `[0, 1)`. Unset → `hash_y(field)`.
    #[serde(default)]
    pub y: Option<f64>,
    /// Cross-axis extent as a fraction of the day's data area.
    #[serde(default)]
    pub height: Option<f64>,
    /// Stacking order. Unset → the resolved `y`.
    #[serde(default)]
    pub z_index: Option<f64>,
}

/// Default row height when a spec leaves `height` unset. Matches
/// `datum_core::field_spec::DEFAULT_FIELD_HEIGHT`.
pub const DEFAULT_FIELD_HEIGHT: f64 = 0.12;

/// Deterministic-hash fallback for a field's cross-axis position: the first
/// 4 bytes of `md5(field)` as a big-endian u32 over 2^32, giving `[0, 1)`.
/// Port of `datum_core::field_spec::presentation::hash_y`.
pub fn hash_y(field: &str) -> f64 {
    let digest = Md5::digest(field.as_bytes());
    let n = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    f64::from(n) / (f64::from(u32::MAX) + 1.0)
}

/// `#rrggbb` from the first three bytes of `md5(name)`.
pub fn md5_hex(name: &str) -> String {
    let h = Md5::digest(name.as_bytes());
    format!("#{:02x}{:02x}{:02x}", h[0], h[1], h[2])
}

/// A stored colour string → hex, falling back to `md5(fallback_name)`.
/// Accepts `#rrggbb` / `rrggbb` and a legacy bare ANSI-256 index.
fn resolve_hex(stored: Option<&str>, fallback_name: &str) -> String {
    if let Some(s) = stored {
        if let Some(hex) = parse_hex(s) {
            return hex;
        }
        if let Ok(n) = s.trim_start_matches('#').parse::<u8>() {
            return ansi256_hex(n);
        }
    }
    md5_hex(fallback_name)
}

fn parse_hex(s: &str) -> Option<String> {
    let body = s.trim_start_matches('#');
    if body.len() != 6 || !body.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("#{}", body.to_ascii_lowercase()))
}

/// ANSI-256 cube index → hex, for specs that stored a legacy colour number.
fn ansi256_hex(n: u8) -> String {
    let (r, g, b) = if n < 16 {
        (128, 128, 128)
    } else if n >= 232 {
        let v = 8 + (n - 232) * 10;
        (v, v, v)
    } else {
        let idx = n - 16;
        let q = |c: u8| -> u8 {
            if c == 0 {
                0
            } else {
                55 + 40 * c
            }
        };
        (q(idx / 36), q((idx % 36) / 6), q(idx % 6))
    };
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// A field's own colour: spec override → `md5(field)`.
pub fn field_hex(field: &str, style: Option<&FieldStyle>) -> String {
    resolve_hex(style.and_then(|s| s.color.as_deref()), field)
}

/// A named state's colour: per-state spec override → `md5(state)`. The
/// field's colour is deliberately *not* the fallback (fieldColor.ts:47).
fn state_hex(state: &str, style: Option<&FieldStyle>) -> String {
    if let Some(c) = style.and_then(|s| s.state_colors.get(state)) {
        return resolve_hex(Some(c.as_str()), state);
    }
    md5_hex(state)
}

/// A JSON state value's colour per §5. `None` for `false` — the "field has
/// ended" sentinel, which renders nothing.
pub fn state_value_hex(
    value: &JsonValue,
    field: &str,
    style: Option<&FieldStyle>,
) -> Option<String> {
    match value {
        JsonValue::Null => Some(IGNORED_HEX.to_string()),
        JsonValue::Bool(true) => Some(field_hex(field, style)),
        JsonValue::Bool(false) => None,
        JsonValue::String(s) if s == IGNORED_STATE => Some(IGNORED_HEX.to_string()),
        JsonValue::String(s) => Some(state_hex(s, style)),
        JsonValue::Array(items) => items.first().and_then(|v| state_value_hex(v, field, style)),
        JsonValue::Object(obj) => Some(match obj.get("id").and_then(JsonValue::as_str) {
            Some(id) => state_hex(id, style),
            None => state_hex(&JsonValue::Object(obj.clone()).to_string(), style),
        }),
        _ => Some(state_hex(&value.to_string(), style)),
    }
}

/// One colour per parallel-state member (da-9vop); a scalar yields one.
pub fn state_value_colors(
    value: &JsonValue,
    field: &str,
    style: Option<&FieldStyle>,
) -> Vec<String> {
    match value {
        JsonValue::Array(items) => items
            .iter()
            .filter_map(|v| state_value_hex(v, field, style))
            .collect(),
        _ => state_value_hex(value, field, style).into_iter().collect(),
    }
}

/// True for the values a switch region does not draw: absent, null, false,
/// or the IGNORED sentinel (`datum_core::shape::is_unrendered_switch_value`).
pub fn is_unrendered_switch_value(v: Option<&JsonValue>) -> bool {
    match v {
        None | Some(JsonValue::Null) | Some(JsonValue::Bool(false)) => true,
        Some(JsonValue::String(s)) => s == IGNORED_STATE,
        _ => false,
    }
}

/// The text datum shows for a state value: a string as itself, anything
/// else as its JSON form.
pub fn state_label(v: Option<&JsonValue>) -> Option<String> {
    v.map(|val| match val {
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    })
}
