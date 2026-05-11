//! [`PresentationSpec`] — visual layout + per-surface visibility for a field.
//!
//! Renderer concerns live here, intentionally separated from
//! [`super::FieldSpec`]'s shape/state/aliases. `PresentationSpec` carries:
//!
//! - **Position triple** (`y`, `height`, `z_index`) — fractional positions of
//!   a field's row in a dayview/nowview surface.
//! - **Anchor triple** (`relative_to`, `anchor_pos_ratio`, `anchor_size_ratio`)
//!   — chains a field's effective position off another field's resolved
//!   position.
//! - **`render`** — per-surface visibility (hide on `dayview` only, etc.).
//!
//! Stored inline in [`super::FieldSpec`] via `#[serde(flatten)]` so a spec is
//! still a single JSON blob with a single SQL row; the split is structural
//! (Rust-side) rather than persistence-level. Visual renderers consume
//! `&PresentationSpec` directly (`spec.presentation.resolve_position(...)`)
//! instead of reaching through the full [`super::FieldSpec`].

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Visual layout + visibility for a single field.
///
/// Held as `FieldSpec.presentation` and flattened into the spec's JSON form,
/// so on-disk and wire shapes are unchanged from when these fields lived
/// directly on [`super::FieldSpec`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PresentationSpec {
    /// Vertical position of the field's row, expressed as a fraction
    /// (0.0 = top, 1.0 = bottom) of the dayview's data area. When None,
    /// `resolve_position` falls back to a deterministic hash of the field
    /// name (port of original datum's
    /// `parseInt(md5(field).slice(0, 8), 16) / Math.pow(2, 32)`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,

    /// Vertical height of the field's row, expressed as a fraction
    /// (0.0 to 1.0) of the dayview's data area. When None, defaults to 0.12
    /// (matching original datum's default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,

    /// Stacking order. Higher values render on top. When None, defaults to
    /// the resolved `y` value (so lower-positioned fields render above
    /// higher ones by default — matches original's `spec.zIndex ?? pY`
    /// behavior).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_index: Option<f64>,

    /// Name of an anchor field whose resolved position drives this field's
    /// effective `y`/`height`. When set, callers must use the batch
    /// [`super::resolve_positions`] API; the single-spec
    /// [`PresentationSpec::resolve_position`] degrades to absolute (hash) fallback
    /// because it has no view of the anchor's spec.
    ///
    /// Invariant: `relative_to.is_some() == anchor_pos_ratio.is_some() ==
    /// anchor_size_ratio.is_some()`. Validated at install time and tolerated
    /// (with a warning) at render time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_to: Option<String>,

    /// Offset ratio along the anchor's perpendicular axis. Effective y is
    /// `anchor.y + anchor.height * anchor_pos_ratio`. Allowed to be negative
    /// or > 1.0 (stacked layouts above/below an anchor).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_pos_ratio: Option<f64>,

    /// Size ratio against the anchor's height. Effective height is
    /// `anchor.height * anchor_size_ratio`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_size_ratio: Option<f64>,

    /// Per-surface render visibility. `None` means visible on every surface
    /// (the implicit default — most fields don't need an explicit setting).
    /// `Some(Render::All(false))` hides the field everywhere visual; a
    /// `PerSurface` map names individual surfaces (`dayview`, `nowview`,
    /// future renderers) and treats absent surfaces as visible. Affects
    /// only visual presentation: data still flows through `tail`, `get`,
    /// `grep`, `view`, sync, and export. Mirrored into
    /// `field_specs.render_json` at save time (NULL = visible everywhere).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<Render>,
}

/// Visibility setting for a [`super::FieldSpec`] across visual surfaces.
///
/// Two acceptable shapes:
/// - [`Render::All`] — bool applied to every surface. `Render::All(false)`
///   hides the field on all visual renderers.
/// - [`Render::PerSurface`] — map keyed by surface name; missing keys
///   default to visible (`true`).
///
/// Untagged so the TOML form `render = false` parses as [`Render::All`]
/// and `render.dayview = false` parses as [`Render::PerSurface`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Render {
    All(bool),
    PerSurface(HashMap<String, bool>),
}

impl Render {
    /// True when this setting permits drawing on the named surface.
    /// Per-surface maps treat absent keys as visible.
    pub fn is_visible_on(&self, surface: &str) -> bool {
        match self {
            Render::All(b) => *b,
            Render::PerSurface(map) => map.get(surface).copied().unwrap_or(true),
        }
    }
}

/// Resolved triple of `(y, height, z_index)` after applying defaults and the
/// deterministic-hash fallback for missing `y`. Consumed by dayview/nowview
/// layout in `datum-layout`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedPosition {
    pub y: f64,
    pub height: f64,
    pub z_index: f64,
}

/// Default row height when [`PresentationSpec::height`] is unset. Matches
/// original datum's `DEFAULT_HEIGHT`.
pub const DEFAULT_FIELD_HEIGHT: f64 = 0.12;

/// Deterministic-hash fallback for a field's `y` position. Stable across runs
/// and platforms so unconfigured fields land at the same vertical position
/// every time. Direct port of the original datum:
/// `parseInt(md5(field).slice(0, 8), 16) / Math.pow(2, 32)` — takes the first
/// 4 bytes of md5(field) as a big-endian u32 and divides by 2^32 to get a
/// value in `[0.0, 1.0)`.
pub(super) fn hash_y(field: &str) -> f64 {
    use md5::{Digest, Md5};
    let digest = Md5::digest(field.as_bytes());
    let n = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    f64::from(n) / (f64::from(u32::MAX) + 1.0)
}

impl PresentationSpec {
    /// True if any anchor field is set. Used for cycle and well-formedness
    /// checks.
    pub fn has_anchor(&self) -> bool {
        self.relative_to.is_some()
            || self.anchor_pos_ratio.is_some()
            || self.anchor_size_ratio.is_some()
    }

    /// Validate that the anchor-triple invariant holds: either all three of
    /// `relative_to` / `anchor_pos_ratio` / `anchor_size_ratio` are set, or
    /// all three are unset. `field_name` is interpolated into the error
    /// message (presentation has no view of its owning field's name).
    pub fn validate_anchor_triple(&self, field_name: &str) -> std::result::Result<(), String> {
        let r = self.relative_to.is_some();
        let p = self.anchor_pos_ratio.is_some();
        let s = self.anchor_size_ratio.is_some();
        if r == p && p == s {
            return Ok(());
        }
        Err(format!(
            "field '{field_name}': anchor triple inconsistent (relative_to={r}, anchor_pos_ratio={p}, anchor_size_ratio={s}); \
             all three must be set together or all three left unset",
        ))
    }

    /// Resolve `(y, height, z_index)` for this field, applying defaults and
    /// the deterministic-hash fallback for an unset `y`. The `field_name`
    /// argument exists so the resolver can compute the hash fallback without
    /// the spec carrying its own name.
    pub fn resolve_position(&self, field_name: &str) -> ResolvedPosition {
        let y = self.y.unwrap_or_else(|| hash_y(field_name));
        let height = self.height.unwrap_or(DEFAULT_FIELD_HEIGHT);
        let z_index = self.z_index.unwrap_or(y);
        ResolvedPosition { y, height, z_index }
    }

    /// True when this presentation permits drawing on `surface`. `None` (the
    /// default) means visible on every surface. Used by visual renderers
    /// (`dayview`, `nowview`) to skip emitting a band; non-visual paths
    /// (`tail`, `get`, `grep`, `view`, sync) ignore this setting.
    pub fn renders_on(&self, surface: &str) -> bool {
        self.render
            .as_ref()
            .map(|r| r.is_visible_on(surface))
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_position_uses_explicit_values_when_set() {
        let pres = PresentationSpec {
            y: Some(0.05),
            height: Some(0.20),
            z_index: Some(7.0),
            ..PresentationSpec::default()
        };
        let r = pres.resolve_position("sleep");
        assert_eq!(r.y, 0.05);
        assert_eq!(r.height, 0.20);
        assert_eq!(r.z_index, 7.0);
    }

    #[test]
    fn resolve_position_falls_back_to_hash_y_and_default_height() {
        let pres = PresentationSpec::default();
        let r = pres.resolve_position("coffee");
        assert!(r.y >= 0.0 && r.y < 1.0);
        assert_eq!(r.height, DEFAULT_FIELD_HEIGHT);
        assert_eq!(r.z_index, r.y);
    }

    #[test]
    fn resolve_position_z_index_defaults_to_y_when_y_is_explicit() {
        let pres = PresentationSpec {
            y: Some(0.42),
            ..PresentationSpec::default()
        };
        let r = pres.resolve_position("focus");
        assert_eq!(r.y, 0.42);
        assert_eq!(r.height, DEFAULT_FIELD_HEIGHT);
        assert_eq!(r.z_index, 0.42);
    }

    #[test]
    fn resolve_position_partial_overrides() {
        let pres = PresentationSpec {
            height: Some(0.30),
            ..PresentationSpec::default()
        };
        let r = pres.resolve_position("dream");
        assert_eq!(r.height, 0.30);
        assert!(r.y >= 0.0 && r.y < 1.0);
        assert_eq!(r.z_index, r.y);

        let pres = PresentationSpec {
            z_index: Some(99.0),
            ..PresentationSpec::default()
        };
        let r = pres.resolve_position("dream");
        assert_eq!(r.z_index, 99.0);
        assert_eq!(r.height, DEFAULT_FIELD_HEIGHT);
    }

    #[test]
    fn hash_y_is_deterministic_and_in_range() {
        let a = hash_y("sleep");
        let b = hash_y("sleep");
        assert_eq!(a, b);
        assert!((0.0..1.0).contains(&a));
        assert_ne!(hash_y("sleep"), hash_y("coffee"));
        assert_ne!(hash_y("coffee"), hash_y("project"));
    }

    #[test]
    fn hash_y_matches_original_datum_reference_value() {
        // md5("sleep") = c9fab33e9458412c527c3fe8a13ee37d → 0xc9fab33e
        let got = hash_y("sleep");
        let expected = f64::from(0xc9fa_b33e_u32) / (f64::from(u32::MAX) + 1.0);
        assert!(
            (got - expected).abs() < 1e-12,
            "got {got}, expected {expected}"
        );
    }

    #[test]
    fn overlap_resolves_with_explicit_z_index_on_top() {
        let sleep = PresentationSpec {
            y: Some(0.05),
            height: Some(0.20),
            z_index: Some(1.0),
            ..PresentationSpec::default()
        };
        let focus = PresentationSpec {
            y: Some(0.05),
            height: Some(0.20),
            z_index: Some(2.0),
            ..PresentationSpec::default()
        };
        let rs = sleep.resolve_position("sleep");
        let rf = focus.resolve_position("focus");
        assert_eq!(rs.y, rf.y);
        assert_eq!(rs.height, rf.height);
        assert!(rf.z_index > rs.z_index);
    }

    // ─── Render visibility ───────────────────────────────────────────────────

    #[test]
    fn renders_on_defaults_to_true_when_render_is_none() {
        let pres = PresentationSpec::default();
        assert!(pres.renders_on("dayview"));
        assert!(pres.renders_on("nowview"));
        assert!(pres.renders_on("future_surface"));
    }

    #[test]
    fn render_all_false_hides_every_surface() {
        let pres = PresentationSpec {
            render: Some(Render::All(false)),
            ..PresentationSpec::default()
        };
        assert!(!pres.renders_on("dayview"));
        assert!(!pres.renders_on("nowview"));
        assert!(!pres.renders_on("anything"));
    }

    #[test]
    fn render_all_true_shows_every_surface() {
        let pres = PresentationSpec {
            render: Some(Render::All(true)),
            ..PresentationSpec::default()
        };
        assert!(pres.renders_on("dayview"));
        assert!(pres.renders_on("nowview"));
    }

    #[test]
    fn render_per_surface_hides_only_named_surface() {
        let mut map = HashMap::new();
        map.insert("dayview".to_string(), false);
        let pres = PresentationSpec {
            render: Some(Render::PerSurface(map)),
            ..PresentationSpec::default()
        };
        assert!(!pres.renders_on("dayview"));
        assert!(pres.renders_on("nowview"));
        assert!(pres.renders_on("future"));
    }

    #[test]
    fn render_per_surface_explicit_overrides() {
        let mut map = HashMap::new();
        map.insert("dayview".to_string(), false);
        map.insert("nowview".to_string(), false);
        let pres = PresentationSpec {
            render: Some(Render::PerSurface(map)),
            ..PresentationSpec::default()
        };
        assert!(!pres.renders_on("dayview"));
        assert!(!pres.renders_on("nowview"));
    }

    #[test]
    fn validate_anchor_triple_accepts_all_unset_or_all_set() {
        let mut pres = PresentationSpec::default();
        assert!(pres.validate_anchor_triple("a").is_ok());
        pres.relative_to = Some("b".into());
        pres.anchor_pos_ratio = Some(0.5);
        pres.anchor_size_ratio = Some(0.2);
        assert!(pres.validate_anchor_triple("a").is_ok());
        pres.anchor_size_ratio = None;
        assert!(pres.validate_anchor_triple("a").is_err());
    }
}
