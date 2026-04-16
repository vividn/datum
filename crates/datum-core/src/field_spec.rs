//! Field specs and the alias system.
//!
//! A FieldSpec defines the schema for a tracked field — what verb it uses by
//! default, what extra keys it requires or accepts, and what units to expect.
//! Aliases are pre-filled argument sets that resolve to a full datum command.

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

/// Full specification for a tracked field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    pub field: String,
    /// Default verb when none is specified.
    pub default_verb: Verb,
    /// Named subfields with their constraints.
    pub subfields: HashMap<String, SubfieldSpec>,
    /// Human description of this field.
    pub description: Option<String>,
    /// Default unit for the primary value.
    pub unit: Option<String>,
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
}
