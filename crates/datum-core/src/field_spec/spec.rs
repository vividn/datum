//! [`FieldSpec`] — the schema record for a tracked field.
//!
//! A spec carries the data-model half of a field: default verb, subfields,
//! id template, color, allowed states with per-state color overrides,
//! derived aliases, slug/data key declarations, and free-form tags.
//! Renderer-only knobs (vertical position, anchored layout, per-surface
//! visibility) live in a sibling [`PresentationSpec`] held as
//! [`FieldSpec::presentation`] and flattened into the same JSON blob.
//!
//! Tracking state (tracked / ignored) is NOT stored on the spec — it's
//! derived from the entry log via [`Db::state_at`](crate::db::Db::state_at).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::action::Action;

use super::presentation::PresentationSpec;
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
///
/// Presentation-only fields (y/height/z_index, anchor triple, render
/// visibility) live in [`FieldSpec::presentation`] and are flattened into
/// the same JSON object so on-disk and wire formats are unchanged.
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

    /// Free-form tags. Used by views and kit commands to group fields by a
    /// schema-level marker (e.g. `["chore"]`). Mirrored into the
    /// `field_specs.tags` column at save time so SQL filters
    /// (`WHERE tags LIKE '%"chore"%'`) and the
    /// `datum spec list --tag` filter can avoid a JSON walk.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Renderer-only knobs (position, anchor triple, surface visibility).
    /// Flattened into the same JSON object so a stored spec is a single
    /// blob. Visual surfaces consume this sub-struct directly via
    /// `spec.presentation.resolve_position(...)` / `.renders_on(...)`.
    #[serde(default, flatten)]
    pub presentation: PresentationSpec,
}

/// The subset of actions permitted as a field's `default_verb` (intent slot).
pub const DEFAULT_VERB_CHOICES: &[Action] =
    &[Action::Occur, Action::Start, Action::Switch, Action::Add];

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
            tags: Vec::new(),
            presentation: PresentationSpec::default(),
        }
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
    fn presentation_serde_flattens_into_field_spec_json() {
        // Position + render fields live in `presentation` but serialize as
        // top-level keys via #[serde(flatten)] — preserves on-disk shape
        // from before the split.
        let mut spec = FieldSpec::new("sleep", Action::Start);
        spec.presentation.y = Some(0.05);
        spec.presentation.height = Some(0.20);
        spec.presentation.z_index = Some(1.0);
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("\"y\":0.05"), "json = {json}");
        assert!(json.contains("\"height\":0.2"), "json = {json}");
        assert!(json.contains("\"z_index\":1.0"), "json = {json}");
        // No nested "presentation" object — the field is flattened away.
        assert!(!json.contains("\"presentation\""), "json = {json}");

        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.presentation.y, Some(0.05));
        assert_eq!(back.presentation.height, Some(0.20));
        assert_eq!(back.presentation.z_index, Some(1.0));
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
        assert!(spec.presentation.y.is_none());
        assert!(spec.presentation.height.is_none());
        assert!(spec.presentation.z_index.is_none());
        // da-a7g: tags default to empty when absent from the JSON.
        assert!(spec.tags.is_empty());
        // da-2zd: render defaults to None (visible everywhere) when absent.
        assert!(spec.presentation.render.is_none());
        assert!(spec.presentation.renders_on("dayview"));
        assert!(spec.presentation.renders_on("nowview"));
    }

    #[test]
    fn render_serde_roundtrip_bool_form() {
        use super::super::Render;
        let mut spec = FieldSpec::new("hidden", Action::Occur);
        spec.presentation.render = Some(Render::All(false));
        let json = serde_json::to_string(&spec).unwrap();
        // bool form serializes as a bare bool (untagged enum).
        assert!(json.contains("\"render\":false"), "json = {json}");
        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.presentation.render, Some(Render::All(false)));
    }

    #[test]
    fn render_serde_roundtrip_per_surface_form() {
        use super::super::Render;
        let mut spec = FieldSpec::new("partial", Action::Occur);
        let mut map = HashMap::new();
        map.insert("dayview".to_string(), false);
        spec.presentation.render = Some(Render::PerSurface(map));
        let json = serde_json::to_string(&spec).unwrap();
        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.presentation.render, spec.presentation.render);
    }

    #[test]
    fn render_serde_omits_when_none() {
        let spec = FieldSpec::new("plain", Action::Occur);
        let json = serde_json::to_string(&spec).unwrap();
        assert!(!json.contains("render"), "json = {json}");
    }

    #[test]
    fn render_deserializes_from_bare_bool_json() {
        use super::super::Render;
        let json = r#"{"field":"x","default_verb":"occur","subfields":{},"render":false}"#;
        let spec: FieldSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.presentation.render, Some(Render::All(false)));
    }

    #[test]
    fn render_deserializes_from_object_json() {
        use super::super::Render;
        let json =
            r#"{"field":"x","default_verb":"occur","subfields":{},"render":{"dayview":false}}"#;
        let spec: FieldSpec = serde_json::from_str(json).unwrap();
        match spec.presentation.render {
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
    fn anchor_fields_serde_roundtrip() {
        let mut spec = FieldSpec::new("contacts", Action::Occur);
        spec.presentation.relative_to = Some("body".into());
        spec.presentation.anchor_pos_ratio = Some(0.1);
        spec.presentation.anchor_size_ratio = Some(0.05);
        let json = serde_json::to_string(&spec).unwrap();
        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.presentation.relative_to.as_deref(), Some("body"));
        assert_eq!(back.presentation.anchor_pos_ratio, Some(0.1));
        assert_eq!(back.presentation.anchor_size_ratio, Some(0.05));
    }
}
