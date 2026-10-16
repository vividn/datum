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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

/// Lazy index of derived aliases pulled out of every stored
/// [`FieldSpec`]. Built from the specs themselves by
/// [`SpecAliasIndex::from_specs`]; the CLI reaches it through
/// [`Db::spec_alias_index`](crate::db::Db::spec_alias_index), which caches
/// one over the `spec` entries until the next write on the connection. Replaces the materialized derived rows that lived in the
/// `aliases` table pre-v21 (da-8y2q.5).
#[derive(Debug, Clone, Default)]
pub struct SpecAliasIndex {
    /// Map from derived-alias name → resolved [`Alias`].
    /// Cross-spec collisions are resolved deterministically: the first
    /// spec offered wins; later collisions are recorded in
    /// [`SpecAliasIndex::collisions`].
    pub entries: HashMap<String, Alias>,
    /// `field` name → derived-alias names this spec contributed (for
    /// reverse lookup, e.g. so a spec save's report can ask "which of my
    /// aliases survived?").
    pub by_field: HashMap<String, Vec<String>>,
    /// Derived-alias name → the [`DerivedAliasKind`] it was derived as.
    pub kinds: HashMap<String, DerivedAliasKind>,
    /// Cross-spec collisions: a spec wanted to register a derived alias
    /// under a name another spec already owns. The first registrant wins.
    pub collisions: Vec<SpecAliasCollision>,
}

/// One cross-spec collision recorded by [`SpecAliasIndex`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecAliasCollision {
    /// Alias name that two specs both wanted.
    pub name: String,
    /// Field whose derived alias is currently surfaced for `name`.
    pub winner: String,
    /// Field whose derived alias was shadowed.
    pub loser: String,
}

impl SpecAliasIndex {
    /// Index the derived aliases of `specs`, in the order given — which
    /// is the order that decides a collision, so callers reading from the
    /// database pass rows in `rowid` order (first save wins).
    ///
    /// Pure: no database. That is what lets the browser build the same
    /// index from the specs it holds (da-5kbj).
    pub fn from_specs<'a>(specs: impl IntoIterator<Item = &'a FieldSpec>) -> Self {
        let mut index = Self::default();
        let mut owner: HashMap<String, String> = HashMap::new();
        for spec in specs {
            for d in &spec.derived_aliases {
                if let Some(winner) = owner.get(&d.name) {
                    index.collisions.push(SpecAliasCollision {
                        name: d.name.clone(),
                        winner: winner.clone(),
                        loser: spec.field.clone(),
                    });
                    continue;
                }
                index.entries.insert(
                    d.name.clone(),
                    Alias {
                        name: d.name.clone(),
                        expands_to: d.expands_to.clone(),
                        // The words a page shows for it (da-wxan9).
                        description: Some(match d.kind {
                            DerivedAliasKind::BareField => format!("{}'s own word", spec.field),
                            DerivedAliasKind::TerminalEnd => format!("ends {}", spec.field),
                            DerivedAliasKind::State => format!("a state of {}", spec.field),
                        }),
                    },
                );
                owner.insert(d.name.clone(), spec.field.clone());
                index.kinds.insert(d.name.clone(), d.kind);
                index
                    .by_field
                    .entry(spec.field.clone())
                    .or_default()
                    .push(d.name.clone());
            }
        }
        index
    }

    /// The field whose spec contributed `name`, if any. The reverse of
    /// [`SpecAliasIndex::by_field`], for the "that is a derived alias,
    /// edit the spec" diagnostic.
    pub fn owner_of(&self, name: &str) -> Option<&str> {
        self.by_field
            .iter()
            .find(|(_field, names)| names.iter().any(|n| n == name))
            .map(|(field, _names)| field.as_str())
    }
}

// ─── `datum alias <sub>`, decided without a database ─────────────────────

/// What one `datum alias` invocation does: the row to write (if any) and
/// the text to print.
///
/// Kept apart from the writing: the CLI loads the rows from SQLite and
/// performs the effect against the same connection (da-5kbj).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AliasEffect {
    /// Print `text`. Nothing is stored.
    Say(String),
    /// Store `alias` (an upsert on its name), then print `text`.
    /// `warning` belongs on stderr.
    Save {
        alias: Alias,
        text: String,
        warning: Option<String>,
    },
    /// Delete the row named `name`, then print `text`.
    Delete { name: String, text: String },
    /// Refuse, with the message the CLI would have failed with.
    Refuse(String),
    /// Print nothing, store nothing — `alias list --show none` (da-qt6z).
    Nothing,
}

/// The guards every user-alias write goes through, and the alias it
/// produces: an empty name is refused, and a name that shadows a built-in
/// subcommand is saved but warned about (da-0b4 — the alias exists, it
/// just will not fire where a verb is expected).
pub fn user_alias_to_save(
    name: &str,
    expands_to: String,
) -> std::result::Result<(Alias, Option<String>), String> {
    if name.is_empty() {
        return Err("alias name must be non-empty".to_string());
    }
    let alias = Alias {
        name: name.to_string(),
        expands_to,
        description: None,
    };
    let warning = datum_parse::alias::KNOWN_SUBCOMMANDS
        .contains(&name)
        .then(|| format!(
            "warning: alias '{name}' shadows a built-in subcommand and will not fire when a verb is expected"
        ));
    Ok((alias, warning))
}

/// Decide `sub` against the alias rows (`user`, in name order) and the
/// spec-derived index. `user` rows shadow derived ones, matching
/// `Db::lookup_alias`.
pub fn alias_effect(
    sub: &datum_parse::cli::AliasCommand,
    user: &[Alias],
    kits: &HashMap<String, String>,
    derived: &SpecAliasIndex,
) -> AliasEffect {
    use datum_parse::cli::AliasCommand;
    use datum_parse::input::Show;

    /// The effective namespace, sorted by name — derived first so a user
    /// row of the same name overwrites it. `true` marks a user row.
    fn namespace<'a>(user: &'a [Alias], derived: &'a SpecAliasIndex) -> Vec<(&'a Alias, bool)> {
        let mut merged: std::collections::BTreeMap<&str, (&Alias, bool)> = derived
            .entries
            .iter()
            .map(|(name, a)| (name.as_str(), (a, false)))
            .collect();
        for a in user {
            merged.insert(a.name.as_str(), (a, true));
        }
        merged.into_values().collect()
    }

    /// `NAME → datum EXPANSION`, then the kit it came from and, on a line
    /// of its own, its description (da-wxan9).
    fn line(a: &Alias, kit: Option<&String>, description: Option<&str>, indent: &str) -> String {
        let mut s = format!("{indent}{} → datum {}", a.name, a.expands_to);
        if let Some(kit) = kit {
            s.push_str(&format!("  (kit {kit})"));
        }
        if let Some(d) = description {
            s.push_str(&format!("\n{indent}    {d}"));
        }
        s
    }

    match sub {
        AliasCommand::Show { name } => AliasEffect::Say(
            match namespace(user, derived)
                .into_iter()
                .find(|(a, _)| &a.name == name)
            {
                Some((a, _)) => line(a, kits.get(name), a.description.as_deref(), ""),
                None => format!("No alias '{name}'"),
            },
        ),
        // One object per name in the namespace: a user row as `user`, with
        // its `kit` and `description` when it has them, a derived one as
        // `derived` with the field and kind it came from.
        AliasCommand::List { show: Show::Json } => AliasEffect::Say(
            serde_json::Value::Array(
                namespace(user, derived)
                    .into_iter()
                    .map(|(a, is_user)| {
                        let (name, expansion) = (a.name.as_str(), a.expands_to.as_str());
                        if is_user {
                            let mut o = serde_json::json!({ "name": name, "expansion": expansion, "source": "user" });
                            if let Some(kit) = kits.get(name) {
                                o["kit"] = kit.as_str().into();
                            }
                            if let Some(d) = &a.description {
                                o["description"] = d.as_str().into();
                            }
                            o
                        } else {
                            serde_json::json!({
                                "name": name,
                                "expansion": expansion,
                                "source": "derived",
                                "field": derived.owner_of(name),
                                "kind": derived.kinds.get(name),
                            })
                        }
                    })
                    .collect(),
            )
            .to_string(),
        ),
        // `--show none` asks for silence, and an empty `Say` is it.
        AliasCommand::List { show: Show::None } => AliasEffect::Nothing,
        // A derived alias's description only names its spec, which a list
        // of every state would repeat on each; `alias show` gives it.
        AliasCommand::List { .. } => {
            let all = namespace(user, derived);
            if all.is_empty() {
                return AliasEffect::Say("No aliases defined.".to_string());
            }
            AliasEffect::Say(
                all.into_iter()
                    .map(|(a, is_user)| {
                        let description = a.description.as_deref().filter(|_| is_user);
                        line(a, kits.get(&a.name), description, "  ")
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
        // `alias set` IS the explicit edit, so it overwrites silently —
        // unlike the inline `--alias` flag, which wants `--alias-overwrite`.
        AliasCommand::Set { name, expansion } => {
            match user_alias_to_save(name, expansion.clone()) {
                Ok((alias, warning)) => AliasEffect::Save {
                    alias,
                    text: format!("✓ Alias '{name}' saved."),
                    warning,
                },
                Err(message) => AliasEffect::Refuse(message),
            }
        }
        AliasCommand::Remove { name } => {
            if user.iter().any(|a| &a.name == name) {
                return AliasEffect::Delete {
                    name: name.clone(),
                    text: format!("✓ Alias '{name}' removed"),
                };
            }
            // Derived aliases live in the spec itself, so there is
            // no row to remove — say where the name actually comes from.
            if derived.entries.contains_key(name) {
                let field = derived.owner_of(name).unwrap_or_default();
                return AliasEffect::Refuse(format!(
                    "'{name}' is a derived alias from the '{field}' field spec; \
                     edit that spec to change it"
                ));
            }
            AliasEffect::Refuse(format!("no user alias '{name}'"))
        }
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
        let mut subfields = std::collections::BTreeMap::new();
        subfields.insert(
            "srcLang".to_string(),
            SubfieldSpec {
                required: true,
                value_type: SubfieldType::String,
                in_slug: true,
                default: None,
            },
        );
        subfields.insert(
            "pos".to_string(),
            SubfieldSpec {
                required: false,
                value_type: SubfieldType::String,
                in_slug: false,
                default: None,
            },
        );
        subfields.insert(
            "translations".to_string(),
            SubfieldSpec {
                required: true,
                value_type: SubfieldType::Array,
                in_slug: false,
                default: None,
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
