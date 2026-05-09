//! [`FieldSpec`] — the schema record for a tracked field.
//!
//! A spec carries: the default verb, subfields, id template, color, allowed
//! states with per-state color overrides, derived aliases, slug/data key
//! declarations, vertical-position fields (y/height/z_index plus
//! relative-to anchoring), free-form tags, and per-surface render visibility.
//!
//! Tracking state (tracked / ignored) is NOT stored on the spec — it's
//! derived from the entry log via [`Db::state_at`](crate::db::Db::state_at).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::action::Action;

use super::DerivedAlias;
use super::DerivedAliasKind;

/// A subfield is a named key within extra_data for a specific field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubfieldSpec {
    /// If true, the CLI will error if this key is not provided.
    pub required: bool,
    /// Expected JSON type hint (for documentation and basic validation).
    pub value_type: SubfieldType,
    /// If true, this key's value is incorporated into the entry slug.
    pub in_slug: bool,
    /// Default value if not supplied.
    pub default: Option<serde_json::Value>,
    /// Unit for this subfield's value (if numeric).
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubfieldType {
    String,
    Number,
    Bool,
    Array,
    Any,
}

/// Full specification for a tracked field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    pub field: String,
    /// Default action when none is specified. Must be one of the canonical
    /// actions: `occur | start | switch | add`. `stop`/`end`/`note` are not
    /// permitted (they're terminal/compat-only, not defaults).
    ///
    /// JSON key stays `default_verb` for wire-format compatibility with
    /// stored field-spec rows; the type changed from [`Verb`] to [`Action`]
    /// in da-8y2q.2 when intent was split out from storage classification.
    pub default_verb: Action,
    /// Named subfields with their constraints.
    #[serde(default)]
    pub subfields: HashMap<String, SubfieldSpec>,
    /// Human description of this field.
    #[serde(default)]
    pub description: Option<String>,
    /// Default unit for the primary value.
    #[serde(default)]
    pub unit: Option<String>,

    /// Template for entry ids/slugs, e.g. "%field%:%state%".
    #[serde(default)]
    pub id_template: Option<String>,
    /// Overall field color (hex or ANSI-256), used in the `now` view.
    #[serde(default)]
    pub color: Option<String>,
    /// Allowed state values (for Switch-verb fields). None = unconstrained.
    #[serde(default)]
    pub allowed_states: Option<Vec<String>>,
    /// Per-state color overrides.
    #[serde(default)]
    pub state_colors: HashMap<String, String>,
    /// Auto-derived aliases (bare-field, terminal-end, and per-state).
    /// Rebuilt on save via [`FieldSpec::regenerate_derived_aliases`].
    #[serde(default)]
    pub derived_aliases: Vec<DerivedAlias>,

    /// Spec-level declaration of the `-K` identity keys (slug-bearing).
    /// Each entry may be `"key"` or `"key=default"`; parser semantics match
    /// the `-K` flag surface.
    #[serde(default)]
    pub id_keys: Vec<String>,
    /// Spec-level declaration of the `-k` data keys (extra_data only). Each
    /// entry may be `"key"` or `"key=default"`.
    #[serde(default)]
    pub data_keys: Vec<String>,
    /// When true, no auto-aliases are emitted for this field (the user takes
    /// over).
    #[serde(default)]
    pub no_autoalias: bool,

    /// Vertical position of the field's row, expressed as a fraction
    /// (0.0 = top, 1.0 = bottom) of the dayview's data area. When None,
    /// `resolve_position` falls back to a deterministic hash of the field
    /// name (port of original datum's
    /// `parseInt(md5(field).slice(0, 8), 16) / Math.pow(2, 32)`).
    #[serde(default)]
    pub y: Option<f64>,

    /// Vertical height of the field's row, expressed as a fraction
    /// (0.0 to 1.0) of the dayview's data area. When None, defaults to 0.12
    /// (matching original datum's default).
    #[serde(default)]
    pub height: Option<f64>,

    /// Stacking order. Higher values render on top. When None, defaults to
    /// the resolved `y` value (so lower-positioned fields render above
    /// higher ones by default — matches original's `spec.zIndex ?? pY`
    /// behavior).
    #[serde(default)]
    pub z_index: Option<f64>,

    /// Name of an anchor field whose resolved position drives this field's
    /// effective `y`/`height`. When set, callers must use the batch
    /// [`super::resolve_positions`] API; the single-spec
    /// [`FieldSpec::resolve_position`] degrades to absolute (hash) fallback
    /// because it has no view of the anchor's spec.
    ///
    /// Invariant: `relative_to.is_some() == anchor_pos_ratio.is_some() ==
    /// anchor_size_ratio.is_some()`. Validated at install time and tolerated
    /// (with a warning) at render time.
    #[serde(default)]
    pub relative_to: Option<String>,

    /// Offset ratio along the anchor's perpendicular axis. Effective y is
    /// `anchor.y + anchor.height * anchor_pos_ratio`. Allowed to be negative
    /// or > 1.0 (stacked layouts above/below an anchor).
    #[serde(default)]
    pub anchor_pos_ratio: Option<f64>,

    /// Size ratio against the anchor's height. Effective height is
    /// `anchor.height * anchor_size_ratio`.
    #[serde(default)]
    pub anchor_size_ratio: Option<f64>,

    /// Free-form tags. Used by views and kit commands to group fields by a
    /// schema-level marker (e.g. `["chore"]`). Mirrored into the
    /// `field_specs.tags` column at save time so SQL filters
    /// (`WHERE tags LIKE '%"chore"%'`) and the
    /// `datum spec list --tag` filter can avoid a JSON walk.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

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

/// Visibility setting for a [`FieldSpec`] across visual surfaces.
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

/// Default row height when [`FieldSpec::height`] is unset. Matches original
/// datum's `DEFAULT_HEIGHT`.
pub const DEFAULT_FIELD_HEIGHT: f64 = 0.12;

/// The subset of actions permitted as a field's `default_verb` (intent slot).
pub const DEFAULT_VERB_CHOICES: &[Action] =
    &[Action::Occur, Action::Start, Action::Switch, Action::Add];

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

impl FieldSpec {
    /// New bare spec with sensible defaults for the given field name and action.
    pub fn new(field: impl Into<String>, default_verb: Action) -> Self {
        Self {
            field: field.into(),
            default_verb,
            subfields: HashMap::new(),
            description: None,
            unit: None,
            id_template: None,
            color: None,
            allowed_states: None,
            state_colors: HashMap::new(),
            derived_aliases: Vec::new(),
            id_keys: Vec::new(),
            data_keys: Vec::new(),
            no_autoalias: false,
            y: None,
            height: None,
            z_index: None,
            relative_to: None,
            anchor_pos_ratio: None,
            anchor_size_ratio: None,
            tags: Vec::new(),
            render: None,
        }
    }

    /// True when this spec permits drawing on `surface`. `None` (the
    /// default) means visible on every surface. Used by visual renderers
    /// (`dayview`, `nowview`) to skip emitting a band; non-visual paths
    /// (`tail`, `get`, `grep`, `view`, sync) ignore this setting.
    pub fn renders_on(&self, surface: &str) -> bool {
        self.render
            .as_ref()
            .map(|r| r.is_visible_on(surface))
            .unwrap_or(true)
    }

    /// Append `tag` to `tags` if it isn't already present. Returns true when
    /// the tag was newly added (caller wants to know if a save is needed).
    pub fn add_tag(&mut self, tag: impl Into<String>) -> bool {
        let tag = tag.into();
        if self.tags.iter().any(|t| t == &tag) {
            return false;
        }
        self.tags.push(tag);
        true
    }

    /// True when `tag` appears in this spec's tag list.
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }

    /// True if this spec has any of the anchor fields set. Used for cycle
    /// and well-formedness checks.
    pub fn has_anchor(&self) -> bool {
        self.relative_to.is_some()
            || self.anchor_pos_ratio.is_some()
            || self.anchor_size_ratio.is_some()
    }

    /// Validate that the anchor-triple invariant holds: either all three of
    /// `relative_to` / `anchor_pos_ratio` / `anchor_size_ratio` are set, or
    /// all three are unset. Returns an error message describing which fields
    /// are out of sync.
    pub fn validate_anchor_triple(&self) -> std::result::Result<(), String> {
        let r = self.relative_to.is_some();
        let p = self.anchor_pos_ratio.is_some();
        let s = self.anchor_size_ratio.is_some();
        if r == p && p == s {
            return Ok(());
        }
        Err(format!(
            "field '{}': anchor triple inconsistent (relative_to={}, anchor_pos_ratio={}, anchor_size_ratio={}); \
             all three must be set together or all three left unset",
            self.field, r, p, s
        ))
    }

    /// Resolve `(y, height, z_index)` for this field, applying defaults and
    /// the deterministic-hash fallback for an unset `y`. The `field_name`
    /// argument exists to keep the resolver callable from layout code that
    /// has the name in hand (avoids relying on `self.field` being populated
    /// in flight).
    pub fn resolve_position(&self, field_name: &str) -> ResolvedPosition {
        let y = self.y.unwrap_or_else(|| hash_y(field_name));
        let height = self.height.unwrap_or(DEFAULT_FIELD_HEIGHT);
        let z_index = self.z_index.unwrap_or(y);
        ResolvedPosition { y, height, z_index }
    }

    /// Validate that `default_verb` is one of the accepted canonical choices.
    /// Returns an error message if the verb is out of range (e.g. `Stop`,
    /// `Block`).
    pub fn validate_default_verb(&self) -> std::result::Result<(), String> {
        if DEFAULT_VERB_CHOICES.contains(&self.default_verb) {
            Ok(())
        } else {
            Err(format!(
                "invalid default_verb '{}' — must be one of: occur, start, switch, add",
                self.default_verb
            ))
        }
    }

    /// Regenerate `derived_aliases` from the current spec. Always produces:
    ///
    /// 1. `<field>` → `<default_verb> <field>` (kind=BareField)
    /// 2. `<field>e` → `end <field>` (kind=TerminalEnd) — only when
    ///    `default_verb ∈ {Start, Switch}` (duration-capable verbs).
    /// 3. One `<state>` → `switch <field> <state>` per allowed state
    ///    (kind=State).
    ///
    /// Idempotent. When `no_autoalias` is set, returns an empty list.
    /// Cross-field state collisions are resolved at save time (see
    /// `db::Db::save_field_spec`).
    pub fn regenerate_derived_aliases(&mut self) {
        if self.no_autoalias {
            self.derived_aliases = Vec::new();
            return;
        }
        let mut out: Vec<DerivedAlias> = Vec::new();

        // 1. Bare-field alias: `<field>` → `<default_verb> <field>`.
        out.push(DerivedAlias {
            state: self.field.clone(),
            expands_to: format!("{} {}", self.default_verb, self.field),
            kind: DerivedAliasKind::BareField,
        });

        // 2. Terminal-end alias: only for duration-capable default verbs.
        if matches!(self.default_verb, Action::Start | Action::Switch) {
            out.push(DerivedAlias {
                state: format!("{}e", self.field),
                expands_to: format!("end {}", self.field),
                kind: DerivedAliasKind::TerminalEnd,
            });
        }

        // 3. Per-state aliases.
        if let Some(states) = &self.allowed_states {
            for s in states {
                out.push(DerivedAlias {
                    state: s.clone(),
                    expands_to: format!("switch {} {}", self.field, s),
                    kind: DerivedAliasKind::State,
                });
            }
        }

        self.derived_aliases = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regenerate_derived_aliases_covers_bare_terminal_and_states() {
        let mut spec = FieldSpec::new("mood", Action::Switch);
        spec.allowed_states = Some(vec!["happy".into(), "sad".into()]);
        spec.regenerate_derived_aliases();
        // bare `mood` + terminal `moode` + two states
        assert_eq!(spec.derived_aliases.len(), 4);

        let bare = spec
            .derived_aliases
            .iter()
            .find(|a| a.state == "mood")
            .unwrap();
        assert_eq!(bare.expands_to, "switch mood");
        assert_eq!(bare.kind, DerivedAliasKind::BareField);

        let terminal = spec
            .derived_aliases
            .iter()
            .find(|a| a.state == "moode")
            .unwrap();
        assert_eq!(terminal.expands_to, "end mood");
        assert_eq!(terminal.kind, DerivedAliasKind::TerminalEnd);

        let happy = spec
            .derived_aliases
            .iter()
            .find(|a| a.state == "happy")
            .unwrap();
        assert_eq!(happy.expands_to, "switch mood happy");
        assert_eq!(happy.kind, DerivedAliasKind::State);
    }

    #[test]
    fn regenerate_derived_aliases_emits_bare_but_no_terminal_for_occur() {
        let mut spec = FieldSpec::new("coffee", Action::Occur);
        spec.regenerate_derived_aliases();
        // Only bare-field — Occur is not duration-capable, no terminal.
        assert_eq!(spec.derived_aliases.len(), 1);
        let bare = &spec.derived_aliases[0];
        assert_eq!(bare.state, "coffee");
        assert_eq!(bare.expands_to, "occur coffee");
        assert_eq!(bare.kind, DerivedAliasKind::BareField);
    }

    #[test]
    fn regenerate_derived_aliases_emits_terminal_for_start_default() {
        let mut spec = FieldSpec::new("sleep", Action::Start);
        spec.regenerate_derived_aliases();
        // bare `sleep` + terminal `sleepe`
        assert_eq!(spec.derived_aliases.len(), 2);
        let bare = spec
            .derived_aliases
            .iter()
            .find(|a| a.state == "sleep")
            .unwrap();
        assert_eq!(bare.expands_to, "start sleep");
        let terminal = spec
            .derived_aliases
            .iter()
            .find(|a| a.state == "sleepe")
            .unwrap();
        assert_eq!(terminal.expands_to, "end sleep");
    }

    #[test]
    fn no_autoalias_suppresses_all_derived_aliases() {
        let mut spec = FieldSpec::new("sleep", Action::Start);
        spec.no_autoalias = true;
        spec.regenerate_derived_aliases();
        assert!(spec.derived_aliases.is_empty());
    }

    #[test]
    fn validate_default_verb_rejects_non_canonical() {
        let mut spec = FieldSpec::new("mood", Action::Stop);
        assert!(spec.validate_default_verb().is_err());
        spec.default_verb = Action::Block;
        assert!(spec.validate_default_verb().is_err());
        spec.default_verb = Action::Switch;
        assert!(spec.validate_default_verb().is_ok());
    }

    #[test]
    fn resolve_position_uses_explicit_values_when_set() {
        let mut spec = FieldSpec::new("sleep", Action::Start);
        spec.y = Some(0.05);
        spec.height = Some(0.20);
        spec.z_index = Some(7.0);
        let r = spec.resolve_position("sleep");
        assert_eq!(r.y, 0.05);
        assert_eq!(r.height, 0.20);
        assert_eq!(r.z_index, 7.0);
    }

    #[test]
    fn resolve_position_falls_back_to_hash_y_and_default_height() {
        // None for every position field → all three come from defaults.
        let spec = FieldSpec::new("coffee", Action::Occur);
        let r = spec.resolve_position("coffee");
        // hash fallback in [0.0, 1.0)
        assert!(r.y >= 0.0 && r.y < 1.0);
        assert_eq!(r.height, DEFAULT_FIELD_HEIGHT);
        // z_index defaults to resolved y
        assert_eq!(r.z_index, r.y);
    }

    #[test]
    fn resolve_position_z_index_defaults_to_y_when_y_is_explicit() {
        let mut spec = FieldSpec::new("focus", Action::Switch);
        spec.y = Some(0.42);
        // height + z_index unset
        let r = spec.resolve_position("focus");
        assert_eq!(r.y, 0.42);
        assert_eq!(r.height, DEFAULT_FIELD_HEIGHT);
        assert_eq!(r.z_index, 0.42);
    }

    #[test]
    fn resolve_position_partial_overrides() {
        // Only height set; y and z_index come from defaults.
        let mut spec = FieldSpec::new("dream", Action::Switch);
        spec.height = Some(0.30);
        let r = spec.resolve_position("dream");
        assert_eq!(r.height, 0.30);
        assert!(r.y >= 0.0 && r.y < 1.0);
        assert_eq!(r.z_index, r.y);

        // Only z_index set; y comes from hash, height from default.
        let mut spec = FieldSpec::new("dream", Action::Switch);
        spec.z_index = Some(99.0);
        let r = spec.resolve_position("dream");
        assert_eq!(r.z_index, 99.0);
        assert_eq!(r.height, DEFAULT_FIELD_HEIGHT);
    }

    #[test]
    fn hash_y_is_deterministic_and_in_range() {
        // Same input → same output, every time.
        let a = hash_y("sleep");
        let b = hash_y("sleep");
        assert_eq!(a, b);
        assert!((0.0..1.0).contains(&a));

        // Different fields hash to different positions (probabilistically;
        // a collision on these specific names would be a regression).
        assert_ne!(hash_y("sleep"), hash_y("coffee"));
        assert_ne!(hash_y("coffee"), hash_y("project"));
    }

    #[test]
    fn hash_y_matches_original_datum_reference_value() {
        // Original datum: parseInt(md5(field).slice(0, 8), 16) / 2**32
        // md5("sleep") = c9fab33e9458412c527c3fe8a13ee37d
        // first 8 hex chars: 0xc9fab33e
        let got = hash_y("sleep");
        let expected = f64::from(0xc9fa_b33e_u32) / (f64::from(u32::MAX) + 1.0);
        assert!(
            (got - expected).abs() < 1e-12,
            "got {got}, expected {expected}"
        );
    }

    #[test]
    fn position_fields_serde_roundtrip() {
        let mut spec = FieldSpec::new("sleep", Action::Start);
        spec.y = Some(0.05);
        spec.height = Some(0.20);
        spec.z_index = Some(1.0);
        let json = serde_json::to_string(&spec).unwrap();
        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.y, Some(0.05));
        assert_eq!(back.height, Some(0.20));
        assert_eq!(back.z_index, Some(1.0));
    }

    #[test]
    fn overlap_resolves_with_explicit_z_index_on_top() {
        // Two fields share y/height; the one with the higher z_index renders
        // on top. Resolver just exposes the z_index — the rendering layer
        // owns the actual stacking — but we can verify the resolved triple.
        let mut sleep = FieldSpec::new("sleep", Action::Start);
        sleep.y = Some(0.05);
        sleep.height = Some(0.20);
        sleep.z_index = Some(1.0);

        let mut focus = FieldSpec::new("focus", Action::Switch);
        focus.y = Some(0.05);
        focus.height = Some(0.20);
        focus.z_index = Some(2.0);

        let rs = sleep.resolve_position("sleep");
        let rf = focus.resolve_position("focus");
        assert_eq!(rs.y, rf.y);
        assert_eq!(rs.height, rf.height);
        assert!(rf.z_index > rs.z_index, "focus must render above sleep");
    }

    #[test]
    fn legacy_spec_json_deserializes_with_defaults() {
        // Old field_specs JSON (phase-1 shape) must still deserialize.
        // Includes the now-removed `tracked`/`ignored` keys to prove serde
        // tolerates them as unknown fields on a v8 → v9 upgrade.
        let legacy = r#"{
            "field": "mood",
            "default_verb": "switch",
            "subfields": {},
            "description": null,
            "unit": null,
            "tracked": true,
            "ignored": true
        }"#;
        let spec: FieldSpec = serde_json::from_str(legacy).unwrap();
        assert_eq!(spec.field, "mood");
        assert!(spec.allowed_states.is_none());
        assert!(spec.derived_aliases.is_empty());
        assert!(spec.id_keys.is_empty());
        assert!(spec.data_keys.is_empty());
        assert!(!spec.no_autoalias);
        // da-dqb: position fields are forwards-compatible — default to None
        // on legacy JSON, with resolve_position falling back to hash/defaults.
        assert!(spec.y.is_none());
        assert!(spec.height.is_none());
        assert!(spec.z_index.is_none());
        // da-a7g: tags default to empty when absent from the JSON.
        assert!(spec.tags.is_empty());
        // da-2zd: render defaults to None (visible everywhere) when absent.
        assert!(spec.render.is_none());
        assert!(spec.renders_on("dayview"));
        assert!(spec.renders_on("nowview"));
    }

    // ─── Render visibility tests (da-2zd) ────────────────────────────────────

    #[test]
    fn renders_on_defaults_to_true_when_render_is_none() {
        let spec = FieldSpec::new("plain", Action::Occur);
        assert!(spec.renders_on("dayview"));
        assert!(spec.renders_on("nowview"));
        assert!(spec.renders_on("future_surface"));
    }

    #[test]
    fn render_all_false_hides_every_surface() {
        let mut spec = FieldSpec::new("background_telemetry", Action::Occur);
        spec.render = Some(Render::All(false));
        assert!(!spec.renders_on("dayview"));
        assert!(!spec.renders_on("nowview"));
        assert!(!spec.renders_on("anything"));
    }

    #[test]
    fn render_all_true_shows_every_surface() {
        let mut spec = FieldSpec::new("loud", Action::Occur);
        spec.render = Some(Render::All(true));
        assert!(spec.renders_on("dayview"));
        assert!(spec.renders_on("nowview"));
    }

    #[test]
    fn render_per_surface_hides_only_named_surface() {
        let mut spec = FieldSpec::new("stretch_routine", Action::Occur);
        let mut map = HashMap::new();
        map.insert("dayview".to_string(), false);
        spec.render = Some(Render::PerSurface(map));
        // dayview hidden, nowview defaults to visible.
        assert!(!spec.renders_on("dayview"));
        assert!(spec.renders_on("nowview"));
        // Unknown surfaces also default to visible.
        assert!(spec.renders_on("future"));
    }

    #[test]
    fn render_per_surface_explicit_overrides() {
        let mut spec = FieldSpec::new("experimental", Action::Occur);
        let mut map = HashMap::new();
        map.insert("dayview".to_string(), false);
        map.insert("nowview".to_string(), false);
        spec.render = Some(Render::PerSurface(map));
        assert!(!spec.renders_on("dayview"));
        assert!(!spec.renders_on("nowview"));
    }

    #[test]
    fn render_serde_roundtrip_bool_form() {
        let mut spec = FieldSpec::new("hidden", Action::Occur);
        spec.render = Some(Render::All(false));
        let json = serde_json::to_string(&spec).unwrap();
        // bool form serializes as a bare bool (untagged enum).
        assert!(json.contains("\"render\":false"), "json = {json}");
        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.render, Some(Render::All(false)));
    }

    #[test]
    fn render_serde_roundtrip_per_surface_form() {
        let mut spec = FieldSpec::new("partial", Action::Occur);
        let mut map = HashMap::new();
        map.insert("dayview".to_string(), false);
        spec.render = Some(Render::PerSurface(map));
        let json = serde_json::to_string(&spec).unwrap();
        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.render, spec.render);
    }

    #[test]
    fn render_serde_omits_when_none() {
        let spec = FieldSpec::new("plain", Action::Occur);
        let json = serde_json::to_string(&spec).unwrap();
        // skip_serializing_if = Option::is_none keeps the JSON tidy.
        assert!(!json.contains("render"), "json = {json}");
    }

    #[test]
    fn render_deserializes_from_bare_bool_json() {
        // Untagged enum picks the bool variant first.
        let json = r#"{"field":"x","default_verb":"occur","subfields":{},"render":false}"#;
        let spec: FieldSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.render, Some(Render::All(false)));
    }

    #[test]
    fn render_deserializes_from_object_json() {
        let json =
            r#"{"field":"x","default_verb":"occur","subfields":{},"render":{"dayview":false}}"#;
        let spec: FieldSpec = serde_json::from_str(json).unwrap();
        match spec.render {
            Some(Render::PerSurface(m)) => {
                assert_eq!(m.get("dayview").copied(), Some(false));
            }
            other => panic!("expected per-surface, got {other:?}"),
        }
    }

    // ─── Tag tests (da-a7g) ──────────────────────────────────────────────────

    #[test]
    fn add_tag_is_idempotent_and_preserves_insertion_order() {
        let mut spec = FieldSpec::new("dishes", Action::Occur);
        assert!(spec.add_tag("chore"));
        assert!(spec.add_tag("daily"));
        // Re-adding a known tag is a no-op and reports false.
        assert!(!spec.add_tag("chore"));
        assert_eq!(spec.tags, vec!["chore".to_string(), "daily".to_string()]);
        assert!(spec.has_tag("chore"));
        assert!(!spec.has_tag("chores"), "substring must not match");
    }

    #[test]
    fn tags_serde_roundtrip_omits_empty() {
        let spec = FieldSpec::new("plain", Action::Occur);
        let json = serde_json::to_string(&spec).unwrap();
        // Empty tags are skipped to keep the on-disk shape small + diffable.
        assert!(!json.contains("\"tags\""), "json = {json}");

        let mut spec = FieldSpec::new("dishes", Action::Occur);
        spec.tags = vec!["chore".into(), "daily".into()];
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("\"tags\":[\"chore\",\"daily\"]"));
        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tags, spec.tags);
    }

    #[test]
    fn validate_anchor_triple_accepts_all_unset_or_all_set() {
        let mut spec = FieldSpec::new("a", Action::Occur);
        assert!(spec.validate_anchor_triple().is_ok());
        spec.relative_to = Some("b".into());
        spec.anchor_pos_ratio = Some(0.5);
        spec.anchor_size_ratio = Some(0.2);
        assert!(spec.validate_anchor_triple().is_ok());
        // Drop one — invariant violated.
        spec.anchor_size_ratio = None;
        assert!(spec.validate_anchor_triple().is_err());
    }

    #[test]
    fn anchor_fields_serde_roundtrip() {
        let mut spec = FieldSpec::new("contacts", Action::Occur);
        spec.relative_to = Some("body".into());
        spec.anchor_pos_ratio = Some(0.1);
        spec.anchor_size_ratio = Some(0.05);
        let json = serde_json::to_string(&spec).unwrap();
        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.relative_to.as_deref(), Some("body"));
        assert_eq!(back.anchor_pos_ratio, Some(0.1));
        assert_eq!(back.anchor_size_ratio, Some(0.05));
    }
}
