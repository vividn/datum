//! Field specs and the alias system.
//!
//! A FieldSpec is the source of truth for a field's schema: default verb,
//! subfields, id template, color, allowed states with per-state color
//! overrides, and derived aliases (auto-created from allowed states).
//!
//! Tracking state (tracked / ignored) is NOT stored on the spec — it's
//! derived from the entry log via [`Db::state_at`](crate::db::Db::state_at).
//! A field is ignored iff its most recent state-bearing entry has
//! `value = "IGNORED"`; any other state (including null) means tracked.
//!
//! Aliases are pre-filled argument sets that resolve to a full datum command.
//! "Derived" aliases are auto-generated from a field's allowed_states; they
//! live in the aliases table but are owned by (and cascade-deleted with) the
//! owning FieldSpec.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

use crate::entry::{Entry, Verb};

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

/// An alias auto-generated from a FieldSpec. Lives inside FieldSpec and is
/// mirrored into the aliases table on save. Covers three kinds: bare-field
/// (`sleep` → `start sleep`), terminal-end (`sleepe` → `end sleep`), and
/// per-state (`happy` → `switch mood happy`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DerivedAlias {
    /// Alias name — the invocation token the user types. Named `state` for
    /// historical reasons (pre-D3 it only ever came from a state).
    pub state: String,
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

/// Full specification for a tracked field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    pub field: String,
    /// Default verb when none is specified. Must be one of the canonical
    /// verbs: `occur | start | switch | add`. `stop`/`end`/`note` are not
    /// permitted (they're terminal/compat-only, not defaults).
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
}

/// The subset of verbs permitted as a field's `default_verb`.
pub const DEFAULT_VERB_CHOICES: &[Verb] = &[Verb::Occur, Verb::Start, Verb::Switch, Verb::Add];

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
            id_keys: Vec::new(),
            data_keys: Vec::new(),
            no_autoalias: false,
        }
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
        if matches!(self.default_verb, Verb::Start | Verb::Switch) {
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

// ─────────────────────────────────────────────────────────────────────────────
// Template interpolation
// ─────────────────────────────────────────────────────────────────────────────

/// Default delimiter used when `--id-delimiter` is not provided.
pub const DEFAULT_ID_DELIMITER: &str = ":";

/// Lookup context for [`interpolate`]. Keeps the helper decoupled from the
/// full [`Entry`] shape so it is easy to test and reuse.
pub struct InterpolationContext<'a> {
    pub field: &'a str,
    pub verb: &'a Verb,
    pub value: Option<&'a JsonValue>,
    pub extra_data: Option<&'a JsonValue>,
    pub uid: &'a str,
}

impl<'a> InterpolationContext<'a> {
    /// Build an [`InterpolationContext`] and the associated short-uid buffer
    /// from an [`Entry`]. The uid buffer is returned separately so the
    /// borrow-checker can keep the context tied to it.
    pub fn from_entry(entry: &'a Entry, uid_buf: &'a str) -> Self {
        Self {
            field: &entry.field,
            verb: &entry.verb,
            value: entry.value.as_ref(),
            extra_data: entry.data.as_ref(),
            uid: uid_buf,
        }
    }
}

/// Interpolate a template like `"%field%:%uid%"` into a string using the
/// lookup context. Unknown keys resolve to the empty string; runs of
/// `delimiter` produced by empty substitutions collapse to a single delimiter
/// (leading/trailing delimiters are also stripped). `\%` is a literal `%`.
pub fn interpolate(template: &str, ctx: &InterpolationContext, delimiter: &str) -> String {
    let parts = split_raw_and_fields(template);
    let mut out = String::new();
    for (idx, part) in parts.iter().enumerate() {
        if idx % 2 == 0 {
            out.push_str(part);
        } else {
            let val = lookup_field(part, ctx);
            out.push_str(&val);
        }
    }
    collapse_delimiters(&out, delimiter)
}

fn split_raw_and_fields(template: &str) -> Vec<String> {
    // Replace unescaped `%` with NUL then split. `\%` → literal `%`.
    let mut buf = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'%') {
            chars.next();
            buf.push('%');
        } else if c == '%' {
            buf.push('\x00');
        } else {
            buf.push(c);
        }
    }
    buf.split('\x00').map(String::from).collect()
}

fn lookup_field(key: &str, ctx: &InterpolationContext) -> String {
    match key {
        "" => String::new(),
        "field" => ctx.field.to_string(),
        "uid" => ctx.uid.to_string(),
        "state" => {
            if let Some(s) = lookup_extra_data(ctx.extra_data, "state") {
                return s;
            }
            if matches!(ctx.verb, Verb::Switch) {
                if let Some(v) = ctx.value {
                    return json_to_plain_string(v);
                }
            }
            String::new()
        }
        _ => lookup_extra_data(ctx.extra_data, key).unwrap_or_default(),
    }
}

/// Walk `extra_data` using dot-path `key` (e.g. `"a.b"`). Returns None if any
/// segment is missing or not an object.
fn lookup_extra_data(extra: Option<&JsonValue>, key: &str) -> Option<String> {
    let mut cur = extra?;
    for segment in key.split('.') {
        match cur {
            JsonValue::Object(m) => {
                cur = m.get(segment)?;
            }
            _ => return None,
        }
    }
    Some(json_to_plain_string(cur))
}

fn json_to_plain_string(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => String::new(),
        other => other.to_string(),
    }
}

fn collapse_delimiters(s: &str, delim: &str) -> String {
    if delim.is_empty() {
        return s.to_string();
    }
    let double = format!("{delim}{delim}");
    let mut result = s.to_string();
    while result.contains(&double) {
        result = result.replace(&double, delim);
    }
    if let Some(stripped) = result.strip_prefix(delim) {
        result = stripped.to_string();
    }
    if let Some(stripped) = result.strip_suffix(delim) {
        result = stripped.to_string();
    }
    result
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
        let mut spec = FieldSpec::new("word", Verb::Add);
        spec.subfields = subfields;
        spec.description = Some("Vocabulary word in a foreign language".to_string());
        spec
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
    fn regenerate_derived_aliases_covers_bare_terminal_and_states() {
        let mut spec = FieldSpec::new("mood", Verb::Switch);
        spec.allowed_states = Some(vec!["happy".into(), "sad".into()]);
        spec.regenerate_derived_aliases();
        // bare `mood` + terminal `moode` + two states
        assert_eq!(spec.derived_aliases.len(), 4);

        let bare = spec.derived_aliases.iter().find(|a| a.state == "mood").unwrap();
        assert_eq!(bare.expands_to, "switch mood");
        assert_eq!(bare.kind, DerivedAliasKind::BareField);

        let terminal = spec.derived_aliases.iter().find(|a| a.state == "moode").unwrap();
        assert_eq!(terminal.expands_to, "end mood");
        assert_eq!(terminal.kind, DerivedAliasKind::TerminalEnd);

        let happy = spec.derived_aliases.iter().find(|a| a.state == "happy").unwrap();
        assert_eq!(happy.expands_to, "switch mood happy");
        assert_eq!(happy.kind, DerivedAliasKind::State);
    }

    #[test]
    fn regenerate_derived_aliases_emits_bare_but_no_terminal_for_occur() {
        let mut spec = FieldSpec::new("coffee", Verb::Occur);
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
        let mut spec = FieldSpec::new("sleep", Verb::Start);
        spec.regenerate_derived_aliases();
        // bare `sleep` + terminal `sleepe`
        assert_eq!(spec.derived_aliases.len(), 2);
        let bare = spec.derived_aliases.iter().find(|a| a.state == "sleep").unwrap();
        assert_eq!(bare.expands_to, "start sleep");
        let terminal = spec.derived_aliases.iter().find(|a| a.state == "sleepe").unwrap();
        assert_eq!(terminal.expands_to, "end sleep");
    }

    #[test]
    fn no_autoalias_suppresses_all_derived_aliases() {
        let mut spec = FieldSpec::new("sleep", Verb::Start);
        spec.no_autoalias = true;
        spec.regenerate_derived_aliases();
        assert!(spec.derived_aliases.is_empty());
    }

    #[test]
    fn validate_default_verb_rejects_non_canonical() {
        let mut spec = FieldSpec::new("mood", Verb::Stop);
        assert!(spec.validate_default_verb().is_err());
        spec.default_verb = Verb::Block;
        assert!(spec.validate_default_verb().is_err());
        spec.default_verb = Verb::Switch;
        assert!(spec.validate_default_verb().is_ok());
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
    }

    // ─── Template interpolation tests (A1 acceptance) ────────────────────────

    fn occur_ctx<'a>(field: &'a str, extra: Option<&'a JsonValue>, uid: &'a str) -> InterpolationContext<'a> {
        InterpolationContext {
            field,
            verb: &Verb::Occur,
            value: None,
            extra_data: extra,
            uid,
        }
    }

    #[test]
    fn interpolate_field_uid_template() {
        let ctx = occur_ctx("coffee", None, "abc12");
        let got = interpolate("%field%:%uid%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "coffee:abc12");
    }

    #[test]
    fn interpolate_state_from_extra_data() {
        // Non-switch verb: %state% pulls from extra_data.state
        let extra = serde_json::json!({"state": "happy"});
        let ctx = occur_ctx("mood", Some(&extra), "ZZZ");
        let got = interpolate("%field%:%state%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "mood:happy");

        // Switch verb: value is the state (no extra_data.state needed)
        let value = serde_json::json!("focused");
        let switch_ctx = InterpolationContext {
            field: "mood",
            verb: &Verb::Switch,
            value: Some(&value),
            extra_data: None,
            uid: "zz",
        };
        let got = interpolate("%field%:%state%", &switch_ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "mood:focused");
    }

    #[test]
    fn interpolate_missing_key_collapses_delimiter() {
        // state is missing → consecutive `:` collapse to one
        let ctx = occur_ctx("coffee", None, "abc12");
        let got = interpolate("%field%:%state%:%uid%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "coffee:abc12");

        // Leading delimiter from an empty head is trimmed.
        let got = interpolate("%state%:%field%:%uid%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "coffee:abc12");

        // Trailing delimiter from an empty tail is trimmed.
        let got = interpolate("%field%:%uid%:%state%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "coffee:abc12");

        // Custom delimiter
        let got = interpolate("%field%-%state%-%uid%", &ctx, "-");
        assert_eq!(got, "coffee-abc12");
    }

    #[test]
    fn interpolate_extra_data_arbitrary_key() {
        // %srcLang% and %pos% pull from extra_data
        let extra = serde_json::json!({"srcLang": "pt", "pos": "verb"});
        let ctx = InterpolationContext {
            field: "word",
            verb: &Verb::Add,
            value: None,
            extra_data: Some(&extra),
            uid: "xx",
        };
        let got = interpolate("%field%:%srcLang%:%pos%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "word:pt:verb");
    }

    #[test]
    fn interpolate_dot_path_reaches_nested_extra_data() {
        let extra = serde_json::json!({"a": {"b": "deep"}});
        let ctx = occur_ctx("f", Some(&extra), "u");
        let got = interpolate("%a.b%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "deep");
    }

    #[test]
    fn interpolate_escaped_percent_is_literal() {
        let ctx = occur_ctx("f", None, "u");
        let got = interpolate(r"100\% of %field%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "100% of f");
    }

    #[test]
    fn interpolate_numeric_and_bool_values_stringify() {
        let extra = serde_json::json!({"count": 3, "on": true});
        let ctx = occur_ctx("f", Some(&extra), "u");
        let got = interpolate("%count%:%on%", &ctx, DEFAULT_ID_DELIMITER);
        assert_eq!(got, "3:true");
    }
}
