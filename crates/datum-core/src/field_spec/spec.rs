//! [`FieldSpec`] — the schema record for a tracked field.
//!
//! A spec carries the data-model half of a field: its verb, subfields,
//! id template, color, allowed states with per-state color overrides,
//! derived aliases, slug/data key declarations, and free-form tags.
//! Renderer-only knobs (vertical position, anchored layout, per-surface
//! visibility) live in a sibling [`PresentationSpec`] held as
//! [`FieldSpec::presentation`] and flattened into the same JSON blob.
//!
//! Tracking state (tracked / ignored) is NOT stored on the spec — it's
//! derived from the entry log via [`Db::state_at`](crate::db::Db::state_at).

use datum_parse::alias::shell_join;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

use crate::action::Action;
pub use datum_parse::spec_keys::{parse_key_decl, DeclaredKey, KeyKind};

use super::presentation::PresentationSpec;
use super::DerivedAlias;
use super::DerivedAliasKind;

/// A subfield is a named key within extra_data for a specific field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubfieldSpec {
    /// If true, the CLI will error if this key is not provided.
    #[serde(default)]
    pub required: bool,
    /// Expected JSON type hint (for documentation and basic validation).
    pub value_type: SubfieldType,
    /// If true, this key's value is incorporated into the entry slug.
    #[serde(default)]
    pub in_slug: bool,
    /// Default value if not supplied.
    pub default: Option<serde_json::Value>,
    /// If true, `default` is ASSIGNED — written onto every entry as a
    /// `key=value` would be — rather than offered as a slot a positional
    /// can fill (da-9s98k).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub assign: bool,
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
    /// What a bare press of this field means — one of the canonical
    /// actions `occur | start | switch | add`. `stop`/`end`/`note` are
    /// not permitted (they're terminal/compat-only, not defaults).
    ///
    /// Required (da-c8mh, reversing da-emze): one place says what a field
    /// is, and it is written down the first time the field is used. The
    /// first log of an unspecced field mints the spec with the verb that
    /// log implies and writes the TRACK document beside it, so no spec
    /// ever exists without an answer to this question.
    ///
    /// JSON key `verb`. A spec stored before this bead wrote the same
    /// value under `default_verb`, and the alias reads it, so an existing
    /// database needs no migration. (The FLAG `--default-verb` is still
    /// an error naming `--verb`; only the stored key is forgiven. A spec
    /// with neither key — da-emze allowed those — is filled from the
    /// field's own log by [`Db::load_spec`](crate::db::Db::load_spec).)
    #[serde(alias = "default_verb")]
    pub verb: Action,
    /// Named subfields with their constraints.
    #[serde(default)]
    pub subfields: HashMap<String, SubfieldSpec>,
    /// Human description of this field.
    #[serde(default)]
    pub description: Option<String>,

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
    /// Per-state human descriptions (da-zwqk). Mirrors `state_colors`'s
    /// per-state pattern — empty by default, absent in JSON when empty.
    /// Surfaced by `datum spec show <field>` and `datum spec describe
    /// <field>` so the meaning of each state lives alongside its name
    /// rather than only in prose.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub state_descriptions: HashMap<String, String>,
    /// Per-state keys (da-cej7q): a state's own declarations, in the
    /// `data_keys` spellings, which join the field's keys when an entry is
    /// in that state — a name the field declares too takes the state's
    /// kind. Absent in JSON when empty.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub state_keys: HashMap<String, Vec<String>>,
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
    /// over): `derived_aliases` is then the user's list, kept as written
    /// rather than rebuilt on save (da-cap5m).
    #[serde(default)]
    pub no_autoalias: bool,

    /// When true, `datum check` flags undeclared extra_data keys on entries
    /// for this field (the `orphaned_extra_data_key` warning). When false
    /// (the default), undeclared keys are accepted silently — most fields
    /// are freeform and their data shape evolves; flagging by default is
    /// over-aggressive. Strict mode lets the user lock down a spec once
    /// the shape stabilizes.
    #[serde(default)]
    pub strict: bool,

    /// Free-form tags. Used by views and kit commands to group fields by a
    /// schema-level marker (e.g. `["chore"]`); `datum spec list --tag`
    /// filters on it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Renderer-only knobs (position, anchor triple, surface visibility).
    /// Flattened into the same JSON object so a stored spec is a single
    /// blob. Visual surfaces consume this sub-struct directly via
    /// `spec.presentation.resolve_position(...)` / `.renders_on(...)`.
    #[serde(default, flatten)]
    pub presentation: PresentationSpec,
}

/// The subset of actions permitted as a field's [`FieldSpec::verb`].
pub const VERB_CHOICES: &[Action] = &[Action::Occur, Action::Start, Action::Switch, Action::Add];

/// The one sentence every "which verb?" error says, so the four choices
/// are spelled the same everywhere the user meets them.
pub const VERB_CHOICES_TEXT: &str = "occur|start|switch|add";

/// The verb a field's LOG implies, for a field that has no spec yet.
///
/// The one rule: a field logged with named states is a `switch`, one
/// logged with on/off booleans is a `start`, anything else is an `occur`.
/// `states` is the field's distinct logged states with the
/// TRACKED/IGNORED statuses already dropped — they are statuses, not
/// states a field can be switched to (da-5ifp).
pub fn verb_from_log(states: &[JsonValue]) -> Action {
    if states.iter().any(JsonValue::is_string) {
        Action::Switch
    } else if states.iter().any(JsonValue::is_boolean) {
        Action::Start
    } else {
        Action::Occur
    }
}

impl FieldSpec {
    /// New bare spec for the given field name and verb. There is no
    /// verbless constructor: a spec always answers "what does a bare
    /// press of this field mean" (da-c8mh).
    pub fn new(field: impl Into<String>, verb: Action) -> Self {
        Self {
            field: field.into(),
            verb,
            subfields: HashMap::new(),
            description: None,
            id_template: None,
            color: None,
            allowed_states: None,
            state_colors: HashMap::new(),
            state_descriptions: HashMap::new(),
            state_keys: HashMap::new(),
            derived_aliases: Vec::new(),
            id_keys: Vec::new(),
            data_keys: Vec::new(),
            no_autoalias: false,
            strict: false,
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

    /// True when this is the spec a field's first log writes and nothing
    /// more (da-c8mh): its verb and the aliases derived from it, with
    /// nothing a person or a kit chose. A kit may replace such a spec
    /// (da-1mkej) — otherwise a kit could configure no field already logged.
    pub fn is_bare(&self) -> bool {
        let mut bare = FieldSpec::new(&self.field, self.verb);
        bare.regenerate_derived_aliases();
        serde_json::to_value(&bare).ok() == serde_json::to_value(self).ok()
    }

    /// Validate that `verb` is one of the accepted canonical choices.
    /// Returns an error message if the verb is out of range (e.g.
    /// `Stop`, `Block`).
    pub fn validate_verb(&self) -> std::result::Result<(), String> {
        if VERB_CHOICES.contains(&self.verb) {
            Ok(())
        } else {
            Err(format!(
                "invalid verb '{}' — must be one of: {VERB_CHOICES_TEXT}",
                self.verb
            ))
        }
    }

    /// Every key the spec declares: the `id_keys`, the `data_keys`, then
    /// the subfields by name (da-9s98k). As on any line, the `-K` slots
    /// among them bind before the `-k` ones, so an `in_slug` subfield takes
    /// a positional ahead of the `data_keys`. A name declared twice (`spec
    /// set --data-key` and `track --key` write different lists) is one key,
    /// as repeated `-k` are: the first place, the last kind, in the slug if
    /// either is. An entry in `state` takes that state's keys last
    /// (da-cej7q), so they override the field's by name.
    pub fn declared_keys(&self, state: Option<&str>) -> Vec<DeclaredKey> {
        let listed = |keys: &[String], id: bool| -> Vec<DeclaredKey> {
            keys.iter()
                .map(|k| {
                    let (name, kind) = parse_key_decl(k);
                    DeclaredKey {
                        name: name.to_string(),
                        kind,
                        id,
                    }
                })
                .collect()
        };
        let mut subs: Vec<_> = self.subfields.iter().collect();
        subs.sort_by_key(|(name, _)| name.as_str());
        let text = |v: &JsonValue| v.as_str().map_or_else(|| v.to_string(), str::to_string);
        let subs = subs.into_iter().map(|(name, s)| DeclaredKey {
            name: name.clone(),
            kind: match (&s.default, s.assign) {
                (Some(v), true) => KeyKind::Assign(text(v)),
                (Some(v), false) => KeyKind::Default(text(v)),
                (None, _) if s.required => KeyKind::Required,
                (None, _) => KeyKind::Optional,
            },
            id: s.in_slug,
        });
        let mut out: Vec<DeclaredKey> = Vec::new();
        let all = listed(&self.id_keys, true).into_iter();
        let own = state.and_then(|s| self.state_keys.get(s));
        let own = listed(own.map_or(&[], Vec::as_slice), false);
        for key in all
            .chain(listed(&self.data_keys, false))
            .chain(subs)
            .chain(own)
        {
            match out.iter_mut().find(|k| k.name == key.name) {
                Some(k) => {
                    *k = DeclaredKey {
                        id: k.id || key.id,
                        ..key
                    }
                }
                None => out.push(key),
            }
        }
        out
    }

    /// Regenerate `derived_aliases` from the current spec. Produces:
    ///
    /// 1. `<field>` → `<verb> <field>` (kind=BareField).
    /// 2. `<field>e` → `end <field>` (kind=TerminalEnd) — only when
    ///    `verb ∈ {Start, Switch}` (duration-capable verbs).
    /// 3. One `<state>` → `switch <field> <state>` per allowed state
    ///    (kind=State).
    ///
    /// Each expansion is `shell_join`ed, so a spaced field (da-7h73h) or
    /// state splits back into the one token it is.
    ///
    /// Idempotent. When `no_autoalias` is set, the list is the user's and is
    /// left as it stands (da-cap5m).
    /// Cross-field state collisions are resolved at save time (see
    /// `db::Db::save_field_spec`).
    pub fn regenerate_derived_aliases(&mut self) {
        if self.no_autoalias {
            return;
        }
        let mut out: Vec<DerivedAlias> = Vec::new();

        // 1. Bare-field alias: `<field>` → `<verb> <field>`.
        out.push(DerivedAlias {
            name: self.field.clone(),
            expands_to: shell_join(&[self.verb.to_string(), self.field.clone()]),
            kind: DerivedAliasKind::BareField,
        });

        // 2. Terminal-end alias: only for duration-capable verbs.
        if matches!(self.verb, Action::Start | Action::Switch) {
            out.push(DerivedAlias {
                name: format!("{}e", self.field),
                expands_to: shell_join(&["end".into(), self.field.clone()]),
                kind: DerivedAliasKind::TerminalEnd,
            });
        }

        // 3. Per-state aliases.
        if let Some(states) = &self.allowed_states {
            for s in states {
                out.push(DerivedAlias {
                    name: s.clone(),
                    expands_to: shell_join(&["switch".into(), self.field.clone(), s.clone()]),
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
            .find(|a| a.name == "mood")
            .unwrap();
        assert_eq!(bare.expands_to, "switch mood");
        assert_eq!(bare.kind, DerivedAliasKind::BareField);

        let terminal = spec
            .derived_aliases
            .iter()
            .find(|a| a.name == "moode")
            .unwrap();
        assert_eq!(terminal.expands_to, "end mood");
        assert_eq!(terminal.kind, DerivedAliasKind::TerminalEnd);

        let happy = spec
            .derived_aliases
            .iter()
            .find(|a| a.name == "happy")
            .unwrap();
        assert_eq!(happy.expands_to, "switch mood happy");
        assert_eq!(happy.kind, DerivedAliasKind::State);
    }

    /// A spaced field (da-7h73h) or state must split back into one token,
    /// or `my coffee` expands to a log on a field called `my`.
    #[test]
    fn derived_aliases_quote_a_spaced_field_and_state() {
        let mut spec = FieldSpec::new("tea time", Action::Switch);
        spec.allowed_states = Some(vec!["very hot".into()]);
        spec.regenerate_derived_aliases();
        let split: Vec<Vec<String>> = spec
            .derived_aliases
            .iter()
            .map(|a| datum_parse::shell_split(&a.expands_to))
            .collect();
        assert_eq!(
            split,
            [
                vec!["switch", "tea time"],
                vec!["end", "tea time"],
                vec!["switch", "tea time", "very hot"]
            ]
        );
    }

    #[test]
    fn regenerate_derived_aliases_emits_bare_but_no_terminal_for_occur() {
        let mut spec = FieldSpec::new("coffee", Action::Occur);
        spec.regenerate_derived_aliases();
        // Only bare-field — Occur is not duration-capable, no terminal.
        assert_eq!(spec.derived_aliases.len(), 1);
        let bare = &spec.derived_aliases[0];
        assert_eq!(bare.name, "coffee");
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
            .find(|a| a.name == "sleep")
            .unwrap();
        assert_eq!(bare.expands_to, "start sleep");
        let terminal = spec
            .derived_aliases
            .iter()
            .find(|a| a.name == "sleepe")
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
    fn no_autoalias_keeps_the_users_list() {
        let mut spec = FieldSpec::new("sleep", Action::Start);
        spec.no_autoalias = true;
        let mine = DerivedAlias {
            name: "zz".into(),
            expands_to: "start sleep".into(),
            kind: DerivedAliasKind::BareField,
        };
        spec.derived_aliases = vec![mine.clone()];
        spec.regenerate_derived_aliases();
        assert_eq!(spec.derived_aliases, vec![mine]);
    }

    #[test]
    fn validate_verb_rejects_non_canonical() {
        let mut spec = FieldSpec::new("mood", Action::Stop);
        assert!(spec.validate_verb().is_err());
        spec.verb = Action::Block;
        assert!(spec.validate_verb().is_err());
        spec.verb = Action::Switch;
        assert!(spec.validate_verb().is_ok());
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
        // Old stored spec JSON (phase-1 shape) must still deserialize.
        // Includes the now-removed `tracked`/`ignored` keys to prove serde
        // tolerates them as unknown fields on a v8 → v9 upgrade.
        let legacy = r#"{
            "field": "mood",
            "verb": "switch",
            "subfields": {},
            "description": null,
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
        let json = r#"{"field":"x","verb":"occur","subfields":{},"render":false}"#;
        let spec: FieldSpec = serde_json::from_str(json).unwrap();
        assert_eq!(spec.presentation.render, Some(Render::All(false)));
    }

    #[test]
    fn render_deserializes_from_object_json() {
        use super::super::Render;
        let json = r#"{"field":"x","verb":"occur","subfields":{},"render":{"dayview":false}}"#;
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
