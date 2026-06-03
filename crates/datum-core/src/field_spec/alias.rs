//! Aliases — both user-defined ([`Alias`]) and spec-derived ([`DerivedAlias`])
//! plus the in-memory [`SpecRegistry`] that pairs them with [`FieldSpec`]s.
//!
//! A user [`Alias`] is a pre-filled argument set that resolves to a full
//! datum command — `vbp` → `note word srcLang=pt pos=verb`. A
//! [`DerivedAlias`] is auto-generated from a field's [`FieldSpec`]
//! (bare-field, terminal-end, or per-state) and lives inside the spec; it's
//! mirrored into the aliases table on save and cascade-deleted with the
//! owning spec.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::FieldSpec;

/// An alias auto-generated from a [`FieldSpec`]. Lives inside the spec and
/// is mirrored into the aliases table on save. Covers three kinds:
/// bare-field (`sleep` → `start sleep`), terminal-end (`sleepe` → `end
/// sleep`), and per-state (`happy` → `switch mood happy`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DerivedAlias {
    /// Alias name — the invocation token the user types.
    #[serde(rename = "state")]
    pub name: String,
    /// The argv expansion (without leading "datum"), e.g. "switch mood happy".
    pub expands_to: String,
    /// What flavour of auto-alias this is. Drives conflict handling.
    #[serde(default)]
    pub kind: DerivedAliasKind,
}

/// Conflict-tag for a [`DerivedAlias`]. Bare-field and terminal-end aliases
/// are field-scoped by construction (no two specs share a field name); state
/// aliases can collide across fields and the first-saved spec wins.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DerivedAliasKind {
    /// `<field>` → `<default_verb> <field>`.
    #[default]
    BareField,
    /// `<field>e` → `end <field>`.
    TerminalEnd,
    /// `<state>` → `switch <field> <state>`.
    State,
}

/// A user-defined alias. Example: `vbp` → `note word srcLang=pt pos=verb`.
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
    use super::super::{SubfieldSpec, SubfieldType};
    use super::*;
    use crate::action::Action;

    fn word_spec() -> FieldSpec {
        let mut subfields = HashMap::new();
        subfields.insert(
            "srcLang".to_string(),
            SubfieldSpec {
                required: true,
                value_type: SubfieldType::String,
                in_slug: true,
                default: None,
                unit: None,
            },
        );
        subfields.insert(
            "pos".to_string(),
            SubfieldSpec {
                required: false,
                value_type: SubfieldType::String,
                in_slug: false,
                default: None,
                unit: None,
            },
        );
        subfields.insert(
            "translations".to_string(),
            SubfieldSpec {
                required: true,
                value_type: SubfieldType::Array,
                in_slug: false,
                default: None,
                unit: None,
            },
        );
        let mut spec = FieldSpec::new("word", Action::Add);
        spec.subfields = subfields;
        spec.description = Some("Vocabulary word in a foreign language".to_string());
        spec
    }

    #[test]
    fn derived_alias_wire_format_uses_state_key() {
        // The `name` field serializes under the legacy JSON key `state`
        // (`#[serde(rename = "state")]`) so stored field-spec blobs keep
        // deserializing across the rename. Guard both directions.
        let alias = DerivedAlias {
            name: "happy".to_string(),
            expands_to: "switch mood happy".to_string(),
            kind: DerivedAliasKind::State,
        };
        let json = serde_json::to_value(&alias).unwrap();
        assert_eq!(json["state"], "happy");
        assert!(json.get("name").is_none());

        let restored: DerivedAlias =
            serde_json::from_value(serde_json::json!({"state": "happy", "expands_to": "x"}))
                .unwrap();
        assert_eq!(restored.name, "happy");
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
        assert_eq!(
            expanded,
            vec!["note", "word", "srcLang=pt", "pos=verb", "correr", "to run"]
        );
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
