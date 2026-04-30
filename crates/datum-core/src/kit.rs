//! Kit format — installable, exportable, shareable bundles of field specs
//! (slice 1: field-spec subset only).
//!
//! A kit is a TOML file declaring named fields with absolute layout (pos /
//! size / color / states). On install, each `[[field]]` is converted into a
//! [`FieldSpec`] row in `field_specs` with `kit_id` set to the kit's name;
//! on uninstall, every row tagged with that `kit_id` is deleted. Round-trip
//! (install → export → diff) is required to be clean.
//!
//! Slice 2+ adds `relative_to` anchor positioning, aliases, views, indices,
//! and Level-1/2 commands. This module only handles the `[[field]]` subset.

use serde::{Deserialize, Serialize};

use crate::entry::Verb;
use crate::error::{DatumError, Result};
use crate::field_spec::{FieldSpec, DEFAULT_FIELD_HEIGHT};

/// Top-level TOML schema. `[kit]` is required; `[[field]]` may be empty.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KitToml {
    pub kit: KitMeta,
    #[serde(default, rename = "field")]
    pub fields: Vec<FieldEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KitMeta {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// A `[[field]]` entry as it appears in the TOML.
///
/// Unlike [`FieldSpec`], everything optional in TOML is `Option<T>` so the
/// presence/absence is preserved through round-trip and merge with defaults
/// happens at install time only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldEntry {
    pub name: String,
    /// `occur | start | switch | add`. Defaults to `occur` when absent.
    #[serde(default)]
    pub default_verb: Option<String>,
    /// `point | block | ongoing`. Documentation hint at this slice — the
    /// actual entry shape is decided at log-time. Reserved for slice 2+
    /// where layout uses it.
    #[serde(default)]
    pub kind: Option<String>,
    /// Position perpendicular to the time axis, fraction `[0.0, 1.0]`.
    /// Maps to [`FieldSpec::y`] for the current axis-orientation.
    #[serde(default)]
    pub pos: Option<f64>,
    /// Row size as a fraction `[0.0, 1.0]`. Maps to [`FieldSpec::height`].
    #[serde(default)]
    pub size: Option<f64>,
    /// `#RRGGBB`, `#RRGGBBAA`, or a hue name (`red`, `blue`, …).
    #[serde(default)]
    pub color: Option<String>,
    /// When true, the field is hidden from views by default.
    #[serde(default)]
    pub hidden: bool,
    /// Default state position within the field band (slice 1: round-trip
    /// only; consumed by views in slice 2+).
    #[serde(default)]
    pub state_pos: Option<f64>,
    /// Default state size within the field band.
    #[serde(default)]
    pub state_size: Option<f64>,
    /// Stacking order. Higher renders on top.
    #[serde(default, rename = "zIndex")]
    pub z_index: Option<f64>,
    /// Identity-keys declaration (`-K` flag surface).
    #[serde(default)]
    pub id_keys: Vec<String>,
    /// Data-keys declaration (`-k` flag surface).
    #[serde(default)]
    pub data_keys: Vec<String>,
    /// Allowed states with per-state overrides.
    #[serde(default, rename = "states")]
    pub states: Vec<StateEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateEntry {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub state_pos: Option<f64>,
    #[serde(default)]
    pub state_size: Option<f64>,
}

impl KitToml {
    /// Parse TOML text into a [`KitToml`]. Surface parse errors as
    /// `DatumError::Other` so the dispatcher can render them with a path.
    pub fn from_str(s: &str) -> Result<Self> {
        toml::from_str(s).map_err(|e| DatumError::Other(format!("kit TOML parse: {e}")))
    }

    /// Serialize back to TOML. Round-trip should be diff-clean for any
    /// kit produced by [`KitToml::from_field_specs`].
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|e| DatumError::Other(format!("kit TOML emit: {e}")))
    }

    /// Build a [`KitToml`] from the kit's stored [`FieldSpec`] rows. Used
    /// by `datum kit export <name>`.
    pub fn from_field_specs(name: &str, version: Option<&str>, specs: &[FieldSpec]) -> Self {
        let fields = specs.iter().map(FieldEntry::from_spec).collect();
        Self {
            kit: KitMeta {
                name: name.to_string(),
                version: version.map(str::to_string),
                description: None,
            },
            fields,
        }
    }
}

impl FieldEntry {
    /// Convert the parsed TOML form to a [`FieldSpec`] suitable for storage.
    ///
    /// `color_resolved` is the hex value produced by
    /// [`crate::kit_color::resolve_color`] from the user-typed value; we
    /// store the resolved hex so subsequent loads don't need to repeat the
    /// resolution. Per-state colors get the same treatment via
    /// [`StateEntry::resolve_color`].
    pub fn into_spec(self, color_resolved: Option<String>) -> Result<FieldSpec> {
        let verb = match self.default_verb.as_deref().unwrap_or("occur") {
            "occur" => Verb::Occur,
            "start" => Verb::Start,
            "switch" => Verb::Switch,
            "add" => Verb::Add,
            other => {
                return Err(DatumError::Other(format!(
                    "kit field '{}': invalid default_verb '{}' (must be occur|start|switch|add)",
                    self.name, other
                )));
            }
        };

        let mut spec = FieldSpec::new(&self.name, verb);
        spec.color = color_resolved;
        spec.y = self.pos;
        spec.height = self.size;
        spec.z_index = self.z_index;
        spec.id_keys = self.id_keys;
        spec.data_keys = self.data_keys;

        if !self.states.is_empty() {
            let allowed: Vec<String> = self.states.iter().map(|s| s.name.clone()).collect();
            for s in &self.states {
                if let Some(c) = &s.color {
                    spec.state_colors.insert(s.name.clone(), c.clone());
                }
            }
            spec.allowed_states = Some(allowed);
        }

        spec.regenerate_derived_aliases();
        Ok(spec)
    }

    /// Project a stored [`FieldSpec`] back into the TOML shape. Defaults
    /// that were never set on install survive as `None` so the round-trip
    /// is clean.
    pub fn from_spec(spec: &FieldSpec) -> Self {
        let states = if let Some(allowed) = &spec.allowed_states {
            allowed
                .iter()
                .map(|name| StateEntry {
                    name: name.clone(),
                    color: spec.state_colors.get(name).cloned(),
                    state_pos: None,
                    state_size: None,
                })
                .collect()
        } else {
            Vec::new()
        };

        let default_verb = match spec.default_verb {
            Verb::Occur => Some("occur".to_string()),
            Verb::Start => Some("start".to_string()),
            Verb::Switch => Some("switch".to_string()),
            Verb::Add => Some("add".to_string()),
            _ => None,
        };

        Self {
            name: spec.field.clone(),
            default_verb,
            kind: None,
            pos: spec.y,
            size: spec.height,
            color: spec.color.clone(),
            hidden: false,
            state_pos: None,
            state_size: None,
            z_index: spec.z_index,
            id_keys: spec.id_keys.clone(),
            data_keys: spec.data_keys.clone(),
            states,
        }
    }
}

/// Default field height when an entry omits `size`. Mirrors
/// [`crate::field_spec::DEFAULT_FIELD_HEIGHT`] so the layout fallback
/// matches between user-declared and kit-declared specs.
pub const KIT_DEFAULT_SIZE: f64 = DEFAULT_FIELD_HEIGHT;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_kit() {
        let src = r##"
[kit]
name = "core"
version = "0.1"

[[field]]
name = "sleep"
default_verb = "start"
pos = 0.10
size = 0.30
color = "#1a3d6e"
"##;
        let kit = KitToml::from_str(src).unwrap();
        assert_eq!(kit.kit.name, "core");
        assert_eq!(kit.kit.version.as_deref(), Some("0.1"));
        assert_eq!(kit.fields.len(), 1);
        let f = &kit.fields[0];
        assert_eq!(f.name, "sleep");
        assert_eq!(f.default_verb.as_deref(), Some("start"));
        assert_eq!(f.pos, Some(0.10));
        assert_eq!(f.size, Some(0.30));
        assert_eq!(f.color.as_deref(), Some("#1a3d6e"));
    }

    #[test]
    fn parses_field_with_states() {
        let src = r##"
[kit]
name = "lang"

[[field]]
name = "vibe"
default_verb = "switch"

[[field.states]]
name = "germany"
color = "#303030"
state_pos = 0.0
state_size = 1.0

[[field.states]]
name = "japan"
"##;
        let kit = KitToml::from_str(src).unwrap();
        assert_eq!(kit.fields.len(), 1);
        let f = &kit.fields[0];
        assert_eq!(f.states.len(), 2);
        assert_eq!(f.states[0].name, "germany");
        assert_eq!(f.states[0].color.as_deref(), Some("#303030"));
        assert_eq!(f.states[0].state_pos, Some(0.0));
        assert_eq!(f.states[1].color, None);
    }

    #[test]
    fn rejects_invalid_default_verb() {
        let entry = FieldEntry {
            name: "x".into(),
            default_verb: Some("stop".into()),
            kind: None,
            pos: None,
            size: None,
            color: None,
            hidden: false,
            state_pos: None,
            state_size: None,
            z_index: None,
            id_keys: vec![],
            data_keys: vec![],
            states: vec![],
        };
        let err = entry.into_spec(None).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("default_verb"), "msg = {msg}");
    }

    #[test]
    fn into_spec_carries_states_and_colors() {
        let entry = FieldEntry {
            name: "mood".into(),
            default_verb: Some("switch".into()),
            kind: None,
            pos: Some(0.2),
            size: Some(0.1),
            color: Some("#ff0000".into()),
            hidden: false,
            state_pos: None,
            state_size: None,
            z_index: None,
            id_keys: vec![],
            data_keys: vec![],
            states: vec![
                StateEntry {
                    name: "happy".into(),
                    color: Some("#00ff00".into()),
                    state_pos: None,
                    state_size: None,
                },
                StateEntry {
                    name: "sad".into(),
                    color: None,
                    state_pos: None,
                    state_size: None,
                },
            ],
        };
        let spec = entry.into_spec(Some("#ff0000".into())).unwrap();
        assert_eq!(spec.field, "mood");
        assert_eq!(spec.color.as_deref(), Some("#ff0000"));
        assert_eq!(
            spec.allowed_states.as_deref(),
            Some(&["happy".to_string(), "sad".to_string()][..])
        );
        assert_eq!(
            spec.state_colors.get("happy").map(String::as_str),
            Some("#00ff00")
        );
        assert!(!spec.state_colors.contains_key("sad"));
        // Derived aliases populated from regenerate at the end of into_spec.
        assert!(!spec.derived_aliases.is_empty());
    }

    #[test]
    fn round_trip_preserves_field_shape() {
        let mut spec = FieldSpec::new("sleep", Verb::Start);
        spec.color = Some("#1a3d6e".into());
        spec.y = Some(0.10);
        spec.height = Some(0.30);
        spec.z_index = Some(0.0);
        spec.allowed_states = Some(vec!["asleep".into()]);
        spec.state_colors.insert("asleep".into(), "#000000".into());

        let kit = KitToml::from_field_specs("core", Some("0.1"), &[spec.clone()]);
        let toml = kit.to_toml().unwrap();
        let back = KitToml::from_str(&toml).unwrap();
        assert_eq!(back.kit.name, "core");
        assert_eq!(back.fields.len(), 1);
        let f = &back.fields[0];
        assert_eq!(f.name, "sleep");
        assert_eq!(f.default_verb.as_deref(), Some("start"));
        assert_eq!(f.pos, Some(0.10));
        assert_eq!(f.size, Some(0.30));
        assert_eq!(f.color.as_deref(), Some("#1a3d6e"));
        assert_eq!(f.states.len(), 1);
        assert_eq!(f.states[0].name, "asleep");
        assert_eq!(f.states[0].color.as_deref(), Some("#000000"));
    }
}
