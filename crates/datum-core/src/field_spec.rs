//! Field specs and the alias system.
//!
//! A FieldSpec is the source of truth for a tracked field. It defines:
//! default verb, subfields, id template, color, allowed states with per-state
//! color overrides, derived aliases (auto-created from allowed states), and
//! tracking state (tracked / ignored flags in the `now` view).
//!
//! Aliases are pre-filled argument sets that resolve to a full datum command.
//! "Derived" aliases are auto-generated from a field's allowed_states; they
//! live in the aliases table but are owned by (and cascade-deleted with) the
//! owning FieldSpec.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::entry::Verb;

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

/// An alias auto-generated from a FieldSpec state. Lives inside FieldSpec
/// and is mirrored into the aliases table on save.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DerivedAlias {
    /// State name that produced this alias (also used as the alias name).
    pub state: String,
    /// The argv expansion (without leading "datum"), e.g. "switch mood happy".
    pub expands_to: String,
}

/// Full specification for a tracked field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    pub field: String,
    /// Default verb when none is specified.
    pub default_verb: Verb,
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
    /// Auto-derived aliases (one per allowed state). Rebuilt on save.
    #[serde(default)]
    pub derived_aliases: Vec<DerivedAlias>,

    /// True when this field should appear in `datum now`.
    #[serde(default)]
    pub tracked: bool,
    /// True when this field is explicitly hidden from `datum now`.
    #[serde(default)]
    pub ignored: bool,
}

impl FieldSpec {
    /// New bare spec with sensible defaults for the given field name and verb.
    pub fn new(field: impl Into<String>, default_verb: Verb) -> Self {
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
            tracked: false,
            ignored: false,
        }
    }

    /// Regenerate derived_aliases from allowed_states. One alias per state,
    /// expanding to `switch <field> <state>`. Idempotent.
    pub fn regenerate_derived_aliases(&mut self) {
        self.derived_aliases = match &self.allowed_states {
            None => Vec::new(),
            Some(states) => states
                .iter()
                .map(|s| DerivedAlias {
                    state: s.clone(),
                    expands_to: format!("switch {} {}", self.field, s),
                })
                .collect(),
        };
    }
}

/// An alias expands to a full datum command prefix.
/// Example: alias "vbp" → "datum note word srcLang=pt pos=verb"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alias {
    pub name: String,
    /// The expansion, without the leading "datum ".
    /// E.g. "note word srcLang=pt pos=verb"
    pub expands_to: String,
    pub description: Option<String>,
}

impl Alias {
    /// Expand this alias with the given trailing args.
    /// Returns a Vec<String> as if they were argv (without "datum").
    pub fn expand(&self, trailing: &[String]) -> Vec<String> {
        let mut parts: Vec<String> = self
            .expands_to
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        parts.extend_from_slice(trailing);
        parts
    }
}

/// In-memory registry of field specs and aliases.
/// In production this is backed by the database; this struct is used for
/// in-process resolution.
#[derive(Debug, Default)]
pub struct SpecRegistry {
    pub fields: HashMap<String, FieldSpec>,
    pub aliases: HashMap<String, Alias>,
}

impl SpecRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_field(&mut self, spec: FieldSpec) {
        self.fields.insert(spec.field.clone(), spec);
    }

    pub fn register_alias(&mut self, alias: Alias) {
        self.aliases.insert(alias.name.clone(), alias);
    }

    /// Resolve an alias name, returning expanded argv or None.
    pub fn resolve_alias(&self, name: &str, trailing: &[String]) -> Option<Vec<String>> {
        self.aliases.get(name).map(|a| a.expand(trailing))
    }

    /// Look up a field spec by name.
    pub fn field_spec(&self, field: &str) -> Option<&FieldSpec> {
        self.fields.get(field)
    }

    /// Returns the list of required subfield names for a field.
    pub fn required_subfields(&self, field: &str) -> Vec<String> {
        self.fields
            .get(field)
            .map(|spec| {
                spec.subfields
                    .iter()
                    .filter(|(_, s)| s.required)
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word_spec() -> FieldSpec {
        let mut subfields = HashMap::new();
        subfields.insert("srcLang".to_string(), SubfieldSpec {
            required: true,
            value_type: SubfieldType::String,
            in_slug: true,
            default: None,
            unit: None,
        });
        subfields.insert("pos".to_string(), SubfieldSpec {
            required: false,
            value_type: SubfieldType::String,
            in_slug: false,
            default: None,
            unit: None,
        });
        subfields.insert("translations".to_string(), SubfieldSpec {
            required: true,
            value_type: SubfieldType::Array,
            in_slug: false,
            default: None,
            unit: None,
        });
        FieldSpec {
            field: "word".to_string(),
            default_verb: Verb::Note,
            subfields,
            description: Some("Vocabulary word in a foreign language".to_string()),
            unit: None,
            id_template: None,
            color: None,
            allowed_states: None,
            state_colors: HashMap::new(),
            derived_aliases: Vec::new(),
            tracked: false,
            ignored: false,
        }
    }

    #[test]
    fn test_alias_expansion() {
        let alias = Alias {
            name: "vbp".to_string(),
            expands_to: "note word srcLang=pt pos=verb".to_string(),
            description: None,
        };
        let trailing = vec!["correr".to_string(), "to run".to_string()];
        let expanded = alias.expand(&trailing);
        assert_eq!(expanded, vec!["note", "word", "srcLang=pt", "pos=verb", "correr", "to run"]);
    }

    #[test]
    fn test_registry_resolves_alias() {
        let mut reg = SpecRegistry::new();
        reg.register_alias(Alias {
            name: "vbp".to_string(),
            expands_to: "note word srcLang=pt pos=verb".to_string(),
            description: None,
        });
        reg.register_field(word_spec());

        let expanded = reg.resolve_alias("vbp", &["correr".to_string()]).unwrap();
        assert_eq!(expanded[0], "note");
        assert_eq!(expanded[1], "word");

        let required = reg.required_subfields("word");
        assert!(required.contains(&"srcLang".to_string()));
        assert!(required.contains(&"translations".to_string()));
        assert!(!required.contains(&"pos".to_string()));
    }

    #[test]
    fn regenerate_derived_aliases_builds_one_per_state() {
        let mut spec = FieldSpec::new("mood", Verb::Switch);
        spec.allowed_states = Some(vec!["happy".into(), "sad".into()]);
        spec.regenerate_derived_aliases();
        assert_eq!(spec.derived_aliases.len(), 2);
        let happy = spec.derived_aliases.iter().find(|a| a.state == "happy").unwrap();
        assert_eq!(happy.expands_to, "switch mood happy");
    }

    #[test]
    fn regenerate_derived_aliases_empty_when_no_states() {
        let mut spec = FieldSpec::new("coffee", Verb::Occur);
        spec.regenerate_derived_aliases();
        assert!(spec.derived_aliases.is_empty());
    }

    #[test]
    fn legacy_spec_json_deserializes_with_defaults() {
        // Old field_specs JSON (phase-1 shape) must still deserialize.
        let legacy = r#"{
            "field": "mood",
            "default_verb": "switch",
            "subfields": {},
            "description": null,
            "unit": null
        }"#;
        let spec: FieldSpec = serde_json::from_str(legacy).unwrap();
        assert_eq!(spec.field, "mood");
        assert!(!spec.tracked);
        assert!(!spec.ignored);
        assert!(spec.allowed_states.is_none());
        assert!(spec.derived_aliases.is_empty());
    }
}
