//! Kit format — installable, exportable, shareable bundles of field specs
//! (slice 1: field-spec subset; slice 5: indices).
//!
//! A kit is a TOML file declaring named fields with absolute layout (pos /
//! size / color / states). On install, each `[[field]]` is converted into a
//! [`FieldSpec`] row in `field_specs` with `kit_id` set to the kit's name;
//! on uninstall, every row tagged with that `kit_id` is deleted. Round-trip
//! (install → export → diff) is required to be clean.
//!
//! Slice 5 also accepts `[[index]]` entries; each becomes a `CREATE INDEX`
//! against the `entries` table, with the SQL text recorded in
//! `kit_indices` so uninstall can `DROP INDEX` cleanly. The DDL is gated
//! by [`validate_index_entry`] which restricts it to a narrow allowlist
//! (no triggers, no ATTACH, no statement chaining). Trust enforcement
//! (prompt the user for local files, reject URLs until signing arrives)
//! lives one layer up in `datum-commands::commands::kit`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::entry::Verb;
use crate::error::{DatumError, Result};
use crate::field_spec::{Alias, FieldSpec, Render, DEFAULT_FIELD_HEIGHT};

/// Top-level TOML schema. `[kit]` is required; `[[field]]`, `[[view]]`,
/// `[[alias]]`, `[[index]]`, and `[[command]]` may all be empty.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KitToml {
    pub kit: KitMeta,
    #[serde(default, rename = "field")]
    pub fields: Vec<FieldEntry>,
    /// `[[view]]` declarations. Each is a named read-only SELECT installed
    /// into the `views` table tagged with the kit's id. Powers
    /// `datum view <name>` (slice 4).
    #[serde(default, rename = "view", skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<ViewEntry>,
    /// `[[alias]]` declarations. Each row becomes an entry in the `aliases`
    /// table tagged with the kit's id. Round-trip preserves `expands_to`
    /// byte-for-byte. Slice 3 (da-d4t.3).
    #[serde(default, rename = "alias", skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<AliasEntry>,
    /// `[[index]]` declarations. Each becomes a `CREATE INDEX` against
    /// `entries` recorded in `kit_indices`. Trust-gated at install time;
    /// see slice 5 (da-d4t.5).
    #[serde(default, rename = "index")]
    pub indices: Vec<IndexEntry>,
    /// `[[command]]` declarations. Two flavours:
    /// - Level 1 (`expands_to`): alias-style splice before the parser runs.
    /// - Level 2 (`view` + `columns`/...): dispatch to a structured renderer.
    /// Level 3 (`script`) is parsed and reserved for forward-compat but the
    /// renderer errors at execution time. Slice 6 (da-d4t.6).
    #[serde(default, rename = "command")]
    pub commands: Vec<CommandEntry>,
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
    /// Anchor field whose resolved (pos, size) drive this field's effective
    /// position at render time. When set, `pos` is reinterpreted as the
    /// pos-ratio against the anchor and `size` as the size-ratio (mapping to
    /// [`FieldSpec::anchor_pos_ratio`] / [`FieldSpec::anchor_size_ratio`]),
    /// and the field's own `y`/`height` are not stored — resolution happens
    /// at render time via [`crate::field_spec::resolve_positions`].
    #[serde(default)]
    pub relative_to: Option<String>,
    /// Identity-keys declaration (`-K` flag surface).
    #[serde(default)]
    pub id_keys: Vec<String>,
    /// Data-keys declaration (`-k` flag surface).
    #[serde(default)]
    pub data_keys: Vec<String>,
    /// Allowed states with per-state overrides. Keyed by state name; emit
    /// order is alphabetical (BTreeMap) so round-trip is diff-clean.
    /// Per-state values use `pos`/`size` (not `state_pos`/`state_size`) —
    /// the field-level defaults retain the `state_pos`/`state_size`
    /// spelling because *those* describe the parent's child-defaults, not
    /// the parent's own band.
    #[serde(default, rename = "states", skip_serializing_if = "BTreeMap::is_empty")]
    pub states: BTreeMap<String, StateSpec>,
    /// Free-form tags used by views and kit commands to group fields
    /// (e.g. `tags = ["chore"]`). Round-trips into [`FieldSpec::tags`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Per-surface render visibility (da-2zd). Two acceptable shapes:
    /// `render = false` (bool — hidden everywhere) and
    /// `render.dayview = false` (table — per-surface override). Absent
    /// means "visible everywhere" and round-trips back as omitted TOML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<Render>,
}

/// A `[[view]]` entry — a named read-only SELECT.
///
/// Slice 4: views may join system tables (entries, entry_history,
/// field_specs, _meta). Read-only enforcement happens at execution time
/// (`PRAGMA query_only = 1`); this entry just round-trips the SQL text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewEntry {
    pub name: String,
    pub sql: String,
}

/// A `[[alias]]` entry — a named expansion of a `datum` invocation.
///
/// Slice 3: round-trips byte-for-byte with the stored [`Alias`]; install
/// copies `expands_to` verbatim into the `aliases` row, export reads it
/// back unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasEntry {
    pub name: String,
    pub expands_to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl AliasEntry {
    /// Convert the parsed TOML form into the stored [`Alias`].
    pub fn into_alias(self) -> Alias {
        Alias {
            name: self.name,
            expands_to: self.expands_to,
            description: self.description,
        }
    }

    /// Project a stored [`Alias`] back into the TOML shape.
    pub fn from_alias(alias: &Alias) -> Self {
        Self {
            name: alias.name.clone(),
            expands_to: alias.expands_to.clone(),
            description: alias.description.clone(),
        }
    }
}

/// A `[[command]]` entry. Both Level 1 and Level 2 share this struct;
/// validation at install time enforces that exactly one of `expands_to`
/// / `view` / `script` is set (script is rejected with a clear error).
///
/// Level 1 fields:
/// - `expands_to`: alias-style argv splice text (shell-tokenised at install).
///
/// Level 2 fields (all driven by `view`):
/// - `view`: name of an installed view to query.
/// - `help`: short doc string.
/// - `columns`: ordered list of column names to project from the view's
///   result (defaults to all columns).
/// - `labels`: column-name → display-header overrides.
/// - `formats`: column-name → renderer hint (`smart_date`, `duration`, …).
/// - `sort`: column to sort by (ascending; descending sort is forward-compat).
/// - `flags`: pass-through verbs supported by this command (e.g. `watch`).
/// - `color_rules`: list of `{ when, color }` painters; first match wins.
///
/// Level 3 (forward-compat):
/// - `script`: reserved; the renderer errors with "scripted commands not
///   yet supported" at execution time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandEntry {
    pub name: String,
    #[serde(default)]
    pub expands_to: Option<String>,
    #[serde(default)]
    pub view: Option<String>,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub labels: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub formats: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub color_rules: Vec<ColorRuleEntry>,
    /// Level 3 placeholder. Round-trips but install rejects it with
    /// "scripted commands not yet supported (Level 3 deferred)".
    #[serde(default)]
    pub script: Option<String>,
}

/// One color-rule TOML entry: `{ when = "...", color = "..." }`. The
/// expression grammar is implemented in [`crate::kit_color_rule`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorRuleEntry {
    pub when: String,
    pub color: String,
}

/// Per-state override carried inside `states.<name> = { ... }` inline tables.
///
/// The `name` of the state is the map key in [`FieldEntry::states`], so it's
/// not duplicated here. `pos`/`size` describe this state's band within its
/// parent field (not the parent's own band — for that, see
/// [`FieldEntry::pos`] / [`FieldEntry::size`]). They are round-trip only at
/// this slice; layout consumes them in slice 2+.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct StateSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
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
    /// by `datum kit export <name>`. `indices` carries the rows from
    /// `kit_indices` for the same kit; pass `&[]` if the kit had no
    /// indices (or the caller doesn't want to round-trip them).
    pub fn from_field_specs(
        name: &str,
        version: Option<&str>,
        specs: &[FieldSpec],
        indices: &[IndexEntry],
    ) -> Self {
        let fields = specs.iter().map(FieldEntry::from_spec).collect();
        Self {
            kit: KitMeta {
                name: name.to_string(),
                version: version.map(str::to_string),
                description: None,
            },
            fields,
            views: Vec::new(),
            aliases: Vec::new(),
            indices: indices.to_vec(),
            commands: Vec::new(),
        }
    }

    /// Build a [`KitToml`] from stored field specs, view rows, and alias
    /// rows. Used by `datum kit export <name>` to round-trip the full kit
    /// shape introduced through slices 1/3/4.
    pub fn from_kit_rows(
        name: &str,
        version: Option<&str>,
        specs: &[FieldSpec],
        views: Vec<ViewEntry>,
        aliases: &[Alias],
    ) -> Self {
        let mut kit = Self::from_field_specs(name, version, specs, &[]);
        kit.views = views;
        kit.aliases = aliases.iter().map(AliasEntry::from_alias).collect();
        kit
    }

    /// Build a [`KitToml`] from field specs, view rows, alias rows, indices,
    /// and command rows. Used by `datum kit export <name>` with slices
    /// 1/3/4/5/6 all wired in.
    pub fn from_kit_rows_full(
        name: &str,
        version: Option<&str>,
        specs: &[FieldSpec],
        views: Vec<ViewEntry>,
        aliases: &[Alias],
        indices: &[IndexEntry],
        commands: Vec<CommandEntry>,
    ) -> Self {
        let mut kit = Self::from_field_specs(name, version, specs, indices);
        kit.views = views;
        kit.aliases = aliases.iter().map(AliasEntry::from_alias).collect();
        kit.commands = commands;
        kit
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
        let action = match self.default_verb.as_deref().unwrap_or("occur") {
            "occur" => Action::Occur,
            "start" => Action::Start,
            "switch" => Action::Switch,
            "add" => Action::Add,
            other => {
                return Err(DatumError::Other(format!(
                    "kit field '{}': invalid default_verb '{}' (must be occur|start|switch|add)",
                    self.name, other
                )));
            }
        };

        let mut spec = FieldSpec::new(&self.name, action);
        spec.color = color_resolved;
        spec.z_index = self.z_index;
        spec.id_keys = self.id_keys;
        spec.data_keys = self.data_keys;

        // When `relative_to` is set, `pos`/`size` are reinterpreted as ratios
        // against the anchor. The field's own `y`/`height` stay None — the
        // effective position is derived at render time from the anchor's
        // resolved triple. Triple invariant: relative_to/anchor_pos_ratio/
        // anchor_size_ratio are all-or-nothing, with sensible defaults of 0.0
        // for an anchored field that omits one ratio (covers the user who
        // writes `relative_to = "body"` and forgets `pos`).
        if let Some(anchor) = self.relative_to {
            spec.relative_to = Some(anchor);
            spec.anchor_pos_ratio = Some(self.pos.unwrap_or(0.0));
            spec.anchor_size_ratio = Some(self.size.unwrap_or(1.0));
        } else {
            spec.y = self.pos;
            spec.height = self.size;
        }

        if !self.states.is_empty() {
            let allowed: Vec<String> = self.states.keys().cloned().collect();
            for (name, s) in &self.states {
                if let Some(c) = &s.color {
                    spec.state_colors.insert(name.clone(), c.clone());
                }
            }
            spec.allowed_states = Some(allowed);
        }

        spec.tags = self.tags;
        spec.render = self.render;

        spec.regenerate_derived_aliases();
        Ok(spec)
    }

    /// Project a stored [`FieldSpec`] back into the TOML shape. Defaults
    /// that were never set on install survive as `None` so the round-trip
    /// is clean.
    pub fn from_spec(spec: &FieldSpec) -> Self {
        let states: BTreeMap<String, StateSpec> = if let Some(allowed) = &spec.allowed_states {
            allowed
                .iter()
                .map(|name| {
                    (
                        name.clone(),
                        StateSpec {
                            color: spec.state_colors.get(name).cloned(),
                            pos: None,
                            size: None,
                        },
                    )
                })
                .collect()
        } else {
            BTreeMap::new()
        };

        let default_verb = match spec.default_verb {
            Action::Occur => Some("occur".to_string()),
            Action::Start => Some("start".to_string()),
            Action::Switch => Some("switch".to_string()),
            Action::Add => Some("add".to_string()),
            _ => None,
        };

        // Anchored specs project `relative_to`/anchor_pos_ratio/anchor_size_ratio
        // back into TOML's `relative_to`/`pos`/`size` shape. Absolute specs
        // emit y/height as `pos`/`size`. An anchored spec never carries a
        // standalone y/height, so the projection is unambiguous.
        let (pos, size, relative_to) = if let Some(anchor) = &spec.relative_to {
            (
                spec.anchor_pos_ratio,
                spec.anchor_size_ratio,
                Some(anchor.clone()),
            )
        } else {
            (spec.y, spec.height, None)
        };

        Self {
            name: spec.field.clone(),
            default_verb,
            kind: None,
            pos,
            size,
            color: spec.color.clone(),
            hidden: false,
            state_pos: None,
            state_size: None,
            z_index: spec.z_index,
            relative_to,
            id_keys: spec.id_keys.clone(),
            data_keys: spec.data_keys.clone(),
            states,
            tags: spec.tags.clone(),
            render: spec.render.clone(),
        }
    }
}

/// Default field height when an entry omits `size`. Mirrors
/// [`crate::field_spec::DEFAULT_FIELD_HEIGHT`] so the layout fallback
/// matches between user-declared and kit-declared specs.
pub const KIT_DEFAULT_SIZE: f64 = DEFAULT_FIELD_HEIGHT;

// ─── Indices (slice 5) ────────────────────────────────────────────────────

/// A `[[index]]` entry as it appears in the TOML.
///
/// The shipped DDL is constructed by [`IndexEntry::build_ddl`] (which the
/// kit installer calls): we own the outer `CREATE INDEX … ON entries(…)`
/// shape, so a malicious kit can't sneak in a different statement form.
/// `expression` and `where_clause` are validated by
/// [`validate_index_entry`] before they reach SQLite.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexEntry {
    /// Index name. Must match `[a-zA-Z_][a-zA-Z0-9_]*`. Persisted under
    /// the kit's `kit_id` namespace so two kits can use the same name
    /// without colliding (the on-disk SQL name is prefixed at install
    /// time — see [`IndexEntry::sqlite_name`]).
    pub name: String,
    /// Target table. Slice 5 hard-codes `entries`; the field is part of
    /// the schema so future slices (e.g. indexing `field_specs`) can
    /// extend it without a TOML break.
    pub on: String,
    /// Index expression — what goes between the parens. Validated as
    /// SQL fragment; cannot contain `;`, comments, or DDL keywords.
    pub expression: String,
    /// Optional partial-index predicate. Same fragment validation.
    #[serde(default, rename = "where")]
    pub where_clause: Option<String>,
    /// Whether to emit `CREATE UNIQUE INDEX`.
    #[serde(default)]
    pub unique: bool,
}

impl IndexEntry {
    /// Compute the on-disk SQLite index name for this entry under a
    /// given kit. Prefixed so two kits can declare the same TOML name
    /// without colliding. The prefix also makes orphaned indices easy
    /// to spot in `sqlite_master` if a kit row goes missing.
    pub fn sqlite_name(&self, kit_id: &str) -> String {
        format!("kit_{}__{}", sanitize_ident(kit_id), self.name)
    }

    /// Build the full validated `CREATE INDEX` statement for this entry
    /// under `kit_id`. Returns the SQL text the installer will execute
    /// and persist into `kit_indices.sql_text`.
    pub fn build_ddl(&self, kit_id: &str) -> Result<String> {
        validate_index_entry(self)?;
        let unique = if self.unique { "UNIQUE " } else { "" };
        let stmt = format!(
            "CREATE {unique}INDEX IF NOT EXISTS \"{name}\" ON {table}({expr}){where_part}",
            name = self.sqlite_name(kit_id),
            table = self.on,
            expr = self.expression.trim(),
            where_part = match &self.where_clause {
                Some(w) => format!(" WHERE {}", w.trim()),
                None => String::new(),
            }
        );
        Ok(stmt)
    }
}

/// Sanitize a kit_id into something that's safe to embed in an index
/// name — non-alphanumeric chars become `_`. Kit names are already
/// expected to be slugs in practice; this is just defense in depth so
/// we never paste an unescaped kit name into a SQL identifier.
fn sanitize_ident(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Tables a kit's index DDL is allowed to touch. Slice 5 ships only
/// `entries`; the WHERE clause may also reference `field_specs` for
/// schema-driven partial indices (see the chore-tracking example in the
/// design doc).
pub const ALLOWED_INDEX_TABLES: &[&str] = &["entries", "field_specs"];

/// SQL keywords that have no business appearing inside a kit's index
/// expression or WHERE clause. The validator rejects any of these as a
/// whole-word match (case-insensitive). Note that `SELECT`/`FROM` are
/// allowed: the bead's own design example uses a `field IN (SELECT …
/// FROM field_specs WHERE …)` partial-index predicate.
const FORBIDDEN_DDL_KEYWORDS: &[&str] = &[
    "ATTACH",
    "DETACH",
    "PRAGMA",
    "ALTER",
    "DROP",
    "INSERT",
    "UPDATE",
    "DELETE",
    "REPLACE",
    "CREATE",
    "TRIGGER",
    "RAISE",
    "BEGIN",
    "COMMIT",
    "ROLLBACK",
    "VACUUM",
    "EXPLAIN",
    "SAVEPOINT",
    "RELEASE",
    "REINDEX",
    "ANALYZE",
    "LOAD_EXTENSION",
];

/// Validate an [`IndexEntry`] against the slice-5 DDL allowlist.
///
/// Checks:
/// - `name` matches `[a-zA-Z_][a-zA-Z0-9_]*`
/// - `on` is in [`ALLOWED_INDEX_TABLES`]
/// - `expression` is non-empty and passes [`validate_sql_fragment`]
/// - `where_clause`, if present, also passes the fragment check
pub fn validate_index_entry(entry: &IndexEntry) -> Result<()> {
    if !is_safe_identifier(&entry.name) {
        return Err(DatumError::Other(format!(
            "kit index '{}': name must match [a-zA-Z_][a-zA-Z0-9_]*",
            entry.name
        )));
    }
    if !ALLOWED_INDEX_TABLES.iter().any(|t| *t == entry.on) {
        return Err(DatumError::Other(format!(
            "kit index '{}': only {:?} are allowed for `on` (got '{}')",
            entry.name, ALLOWED_INDEX_TABLES, entry.on
        )));
    }
    if entry.expression.trim().is_empty() {
        return Err(DatumError::Other(format!(
            "kit index '{}': expression is empty",
            entry.name
        )));
    }
    validate_sql_fragment(&entry.name, "expression", &entry.expression)?;
    if let Some(w) = &entry.where_clause {
        validate_sql_fragment(&entry.name, "where", w)?;
    }
    Ok(())
}

/// Run the safety scan on a piece of user-supplied SQL fragment.
///
/// We do not parse the SQL; we apply a tight blocklist that's sufficient
/// for v1's threat model (a malicious kit author who wants to do more
/// than declare an index). The grammar restriction sits at three levels:
/// (1) the outer `CREATE INDEX … ON entries(…)` shape is built by us;
/// (2) statement chaining via `;` and SQL comments is blocked here;
/// (3) DDL/DML keywords (CREATE/DROP/ATTACH/…) are rejected here.
fn validate_sql_fragment(index_name: &str, label: &str, s: &str) -> Result<()> {
    if s.contains(';') {
        return Err(DatumError::Other(format!(
            "kit index '{index_name}' {label}: ';' is not allowed (no statement chaining)"
        )));
    }
    if s.contains("--") || s.contains("/*") || s.contains("*/") {
        return Err(DatumError::Other(format!(
            "kit index '{index_name}' {label}: SQL comments are not allowed"
        )));
    }
    let upper = s.to_ascii_uppercase();
    for forbidden in FORBIDDEN_DDL_KEYWORDS {
        if contains_word(&upper, forbidden) {
            return Err(DatumError::Other(format!(
                "kit index '{index_name}' {label}: forbidden keyword '{forbidden}'"
            )));
        }
    }
    Ok(())
}

fn is_safe_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Whole-word case-insensitive containment check on already-uppercased
/// haystack vs uppercase needle. A "word" is bounded by non-`[A-Z0-9_]`
/// characters, mirroring SQL identifier conventions.
fn contains_word(upper_haystack: &str, upper_needle: &str) -> bool {
    let bytes = upper_haystack.as_bytes();
    let needle = upper_needle.as_bytes();
    let nlen = needle.len();
    let hlen = bytes.len();
    if nlen == 0 || hlen < nlen {
        return false;
    }
    let is_ident_byte = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut i = 0;
    while i + nlen <= hlen {
        if &bytes[i..i + nlen] == needle {
            let left_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let right_ok = i + nlen == hlen || !is_ident_byte(bytes[i + nlen]);
            if left_ok && right_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

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

states.germany = { color = "#303030", pos = 0.0, size = 1.0 }
states.japan = {}
"##;
        let kit = KitToml::from_str(src).unwrap();
        assert_eq!(kit.fields.len(), 1);
        let f = &kit.fields[0];
        assert_eq!(f.states.len(), 2);
        let germany = f.states.get("germany").unwrap();
        assert_eq!(germany.color.as_deref(), Some("#303030"));
        assert_eq!(germany.pos, Some(0.0));
        assert_eq!(germany.size, Some(1.0));
        let japan = f.states.get("japan").unwrap();
        assert_eq!(japan.color, None);
        assert_eq!(japan.pos, None);
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
            relative_to: None,
            id_keys: vec![],
            data_keys: vec![],
            states: BTreeMap::new(),
            tags: vec![],
            render: None,
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
            relative_to: None,
            id_keys: vec![],
            data_keys: vec![],
            states: {
                let mut m = BTreeMap::new();
                m.insert(
                    "happy".into(),
                    StateSpec {
                        color: Some("#00ff00".into()),
                        pos: None,
                        size: None,
                    },
                );
                m.insert("sad".into(), StateSpec::default());
                m
            },
            tags: vec![],
            render: None,
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
    fn parses_field_with_relative_to() {
        // `relative_to` reinterprets `pos` and `size` as anchor ratios.
        let src = r##"
[kit]
name = "anchored"

[[field]]
name = "body"
default_verb = "occur"
pos = 0.20
size = 0.40

[[field]]
name = "contacts"
default_verb = "occur"
relative_to = "body"
pos = 0.10
size = 0.05
"##;
        let kit = KitToml::from_str(src).unwrap();
        let contacts = kit.fields.iter().find(|f| f.name == "contacts").unwrap();
        assert_eq!(contacts.relative_to.as_deref(), Some("body"));
        assert_eq!(contacts.pos, Some(0.10));
        assert_eq!(contacts.size, Some(0.05));

        // Into-spec maps the ratios into the FieldSpec anchor triple and
        // leaves y/height unset (resolution happens at render time).
        let spec = contacts.clone().into_spec(None).unwrap();
        assert_eq!(spec.relative_to.as_deref(), Some("body"));
        assert_eq!(spec.anchor_pos_ratio, Some(0.10));
        assert_eq!(spec.anchor_size_ratio, Some(0.05));
        assert!(spec.y.is_none());
        assert!(spec.height.is_none());
    }

    #[test]
    fn anchored_round_trip_preserves_relative_to() {
        let mut anchor = FieldSpec::new("body", Action::Occur);
        anchor.y = Some(0.20);
        anchor.height = Some(0.40);

        let mut dependent = FieldSpec::new("contacts", Action::Occur);
        dependent.relative_to = Some("body".into());
        dependent.anchor_pos_ratio = Some(0.10);
        dependent.anchor_size_ratio = Some(0.05);

        let kit = KitToml::from_field_specs("anchored", None, &[anchor, dependent], &[]);
        let toml = kit.to_toml().unwrap();
        let back = KitToml::from_str(&toml).unwrap();
        let contacts = back.fields.iter().find(|f| f.name == "contacts").unwrap();
        assert_eq!(contacts.relative_to.as_deref(), Some("body"));
        assert_eq!(contacts.pos, Some(0.10));
        assert_eq!(contacts.size, Some(0.05));
    }

    #[test]
    fn round_trip_preserves_field_shape() {
        let mut spec = FieldSpec::new("sleep", Action::Start);
        spec.color = Some("#1a3d6e".into());
        spec.y = Some(0.10);
        spec.height = Some(0.30);
        spec.z_index = Some(0.0);
        spec.allowed_states = Some(vec!["asleep".into()]);
        spec.state_colors.insert("asleep".into(), "#000000".into());

        let kit = KitToml::from_field_specs("core", Some("0.1"), &[spec.clone()], &[]);
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
        let asleep = f.states.get("asleep").unwrap();
        assert_eq!(asleep.color.as_deref(), Some("#000000"));
    }

    #[test]
    fn states_map_round_trips_through_toml() {
        // Authoring-format → emit → re-parse preserves the map shape and
        // per-state pos/size. (FieldSpec doesn't store per-state pos/size
        // yet, so this checks the TOML layer only — see slice 2+ for the
        // install/export side.)
        let src = r##"
[kit]
name = "lang"

[[field]]
name = "vibe"
default_verb = "switch"

states.germany = { color = "#303030", pos = 0.0, size = 1.0 }
states.japan = { color = "#aa0000" }
states.empty = {}
"##;
        let kit = KitToml::from_str(src).unwrap();
        let toml = kit.to_toml().unwrap();
        let back = KitToml::from_str(&toml).unwrap();
        let f = &back.fields[0];
        assert_eq!(f.states.len(), 3);
        let germany = f.states.get("germany").unwrap();
        assert_eq!(germany.color.as_deref(), Some("#303030"));
        assert_eq!(germany.pos, Some(0.0));
        assert_eq!(germany.size, Some(1.0));
        let japan = f.states.get("japan").unwrap();
        assert_eq!(japan.color.as_deref(), Some("#aa0000"));
        assert_eq!(japan.pos, None);
        let empty = f.states.get("empty").unwrap();
        assert_eq!(empty, &StateSpec::default());
    }

    #[test]
    fn empty_states_omitted_from_emitted_toml() {
        let spec = FieldSpec::new("plain", Action::Occur);
        let kit = KitToml::from_field_specs("k", None, &[spec], &[]);
        let toml = kit.to_toml().unwrap();
        // skip_serializing_if keeps empty state maps out of the emitted TOML.
        assert!(!toml.contains("states"), "toml = {toml}");
    }

    // ─── Tags (da-a7g) ────────────────────────────────────────────────

    #[test]
    fn parses_field_with_tags() {
        let src = r##"
[kit]
name = "chores"

[[field]]
name = "dishes"
default_verb = "occur"
tags = ["chore", "daily"]
"##;
        let kit = KitToml::from_str(src).unwrap();
        let dishes = &kit.fields[0];
        assert_eq!(dishes.tags, vec!["chore".to_string(), "daily".to_string()]);

        let spec = dishes.clone().into_spec(None).unwrap();
        assert_eq!(spec.tags, vec!["chore".to_string(), "daily".to_string()]);
    }

    #[test]
    fn round_trip_preserves_field_tags() {
        let mut spec = FieldSpec::new("dishes", Action::Occur);
        spec.tags = vec!["chore".into(), "daily".into()];
        let kit = KitToml::from_field_specs("chores", None, &[spec.clone()], &[]);
        let toml = kit.to_toml().unwrap();
        let back = KitToml::from_str(&toml).unwrap();
        assert_eq!(back.fields[0].tags, spec.tags);
    }

    // ─── Render visibility (da-2zd) ───────────────────────────────────

    #[test]
    fn parses_field_with_render_false_bool() {
        let src = r##"
[kit]
name = "stealth"

[[field]]
name = "background_telemetry"
default_verb = "occur"
render = false
"##;
        let kit = KitToml::from_str(src).unwrap();
        let f = &kit.fields[0];
        assert_eq!(f.render, Some(Render::All(false)));

        let spec = f.clone().into_spec(None).unwrap();
        assert!(!spec.renders_on("dayview"));
        assert!(!spec.renders_on("nowview"));
    }

    #[test]
    fn parses_field_with_per_surface_render_table() {
        let src = r##"
[kit]
name = "stealth"

[[field]]
name = "stretch_routine"
default_verb = "occur"
render.dayview = false
"##;
        let kit = KitToml::from_str(src).unwrap();
        let f = &kit.fields[0];
        match &f.render {
            Some(Render::PerSurface(map)) => {
                assert_eq!(map.get("dayview").copied(), Some(false));
                assert!(!map.contains_key("nowview"));
            }
            other => panic!("expected per-surface, got {other:?}"),
        }

        let spec = f.clone().into_spec(None).unwrap();
        // dayview hidden, nowview defaults to visible.
        assert!(!spec.renders_on("dayview"));
        assert!(spec.renders_on("nowview"));
    }

    #[test]
    fn round_trip_preserves_render_bool() {
        let mut spec = FieldSpec::new("hidden", Action::Occur);
        spec.render = Some(Render::All(false));
        let kit = KitToml::from_field_specs("k", None, &[spec.clone()], &[]);
        let toml = kit.to_toml().unwrap();
        let back = KitToml::from_str(&toml).unwrap();
        assert_eq!(back.fields[0].render, Some(Render::All(false)));
    }

    #[test]
    fn round_trip_preserves_per_surface_render() {
        let mut spec = FieldSpec::new("partial", Action::Occur);
        let mut map = std::collections::HashMap::new();
        map.insert("dayview".to_string(), false);
        spec.render = Some(Render::PerSurface(map.clone()));
        let kit = KitToml::from_field_specs("k", None, &[spec.clone()], &[]);
        let toml = kit.to_toml().unwrap();
        let back = KitToml::from_str(&toml).unwrap();
        match &back.fields[0].render {
            Some(Render::PerSurface(m)) => {
                assert_eq!(m.get("dayview").copied(), Some(false));
            }
            other => panic!("expected per-surface, got {other:?}"),
        }
    }

    #[test]
    fn round_trip_omits_render_when_default() {
        let spec = FieldSpec::new("plain", Action::Occur);
        let kit = KitToml::from_field_specs("k", None, &[spec], &[]);
        let toml = kit.to_toml().unwrap();
        // skip_serializing_if = Option::is_none keeps the TOML diff-clean
        // for the common case where render is unset.
        assert!(!toml.contains("render"), "toml = {toml}");
    }

    #[test]
    fn round_trip_omits_empty_tags() {
        let spec = FieldSpec::new("plain", Action::Occur);
        let kit = KitToml::from_field_specs("k", None, &[spec], &[]);
        let toml = kit.to_toml().unwrap();
        // skip_serializing_if keeps empty tag arrays out of the emitted TOML.
        assert!(!toml.contains("tags"), "toml = {toml}");
    }

    // ─── slice 5: indices ─────────────────────────────────────────────

    #[test]
    fn parses_kit_with_indices() {
        let src = r##"
[kit]
name = "chores"

[[index]]
name = "chore_next_due"
on = "entries"
expression = "COALESCE(json_extract(data,'$.next_time'), start_time)"
where = "field IN (SELECT name FROM field_specs WHERE kit_id = 'chores')"
"##;
        let kit = KitToml::from_str(src).unwrap();
        assert_eq!(kit.indices.len(), 1);
        let i = &kit.indices[0];
        assert_eq!(i.name, "chore_next_due");
        assert_eq!(i.on, "entries");
        assert!(!i.unique);
        assert!(i.where_clause.as_deref().unwrap().contains("field_specs"));
    }

    #[test]
    fn build_ddl_emits_create_index_if_not_exists() {
        let entry = IndexEntry {
            name: "by_field".into(),
            on: "entries".into(),
            expression: "field, start_time DESC".into(),
            where_clause: Some("deleted_at IS NULL".into()),
            unique: false,
        };
        let ddl = entry.build_ddl("chores").unwrap();
        assert_eq!(
            ddl,
            "CREATE INDEX IF NOT EXISTS \"kit_chores__by_field\" \
             ON entries(field, start_time DESC) WHERE deleted_at IS NULL"
        );
    }

    #[test]
    fn build_ddl_emits_unique_when_requested() {
        let entry = IndexEntry {
            name: "uniq_slug".into(),
            on: "entries".into(),
            expression: "slug".into(),
            where_clause: None,
            unique: true,
        };
        let ddl = entry.build_ddl("k").unwrap();
        assert!(ddl.contains("CREATE UNIQUE INDEX IF NOT EXISTS"));
    }

    #[test]
    fn rejects_index_name_with_unsafe_chars() {
        let entry = IndexEntry {
            name: "bad name; DROP TABLE entries".into(),
            on: "entries".into(),
            expression: "field".into(),
            where_clause: None,
            unique: false,
        };
        let err = entry.build_ddl("k").unwrap_err().to_string();
        assert!(err.contains("name must match"), "msg = {err}");
    }

    #[test]
    fn rejects_disallowed_table() {
        let entry = IndexEntry {
            name: "ix".into(),
            on: "_meta".into(),
            expression: "key".into(),
            where_clause: None,
            unique: false,
        };
        let err = entry.build_ddl("k").unwrap_err().to_string();
        assert!(err.contains("only"), "msg = {err}");
    }

    #[test]
    fn rejects_semicolon_in_expression() {
        let entry = IndexEntry {
            name: "ix".into(),
            on: "entries".into(),
            expression: "field; SELECT 1".into(),
            where_clause: None,
            unique: false,
        };
        let err = entry.build_ddl("k").unwrap_err().to_string();
        assert!(err.contains("';'"), "msg = {err}");
    }

    #[test]
    fn rejects_sql_comment() {
        let entry = IndexEntry {
            name: "ix".into(),
            on: "entries".into(),
            expression: "field -- nasty".into(),
            where_clause: None,
            unique: false,
        };
        let err = entry.build_ddl("k").unwrap_err().to_string();
        assert!(err.contains("comments"), "msg = {err}");
    }

    #[test]
    fn rejects_attach_keyword() {
        let entry = IndexEntry {
            name: "ix".into(),
            on: "entries".into(),
            expression: "ATTACH DATABASE '/tmp/x' AS evil".into(),
            where_clause: None,
            unique: false,
        };
        let err = entry.build_ddl("k").unwrap_err().to_string();
        assert!(err.contains("ATTACH"), "msg = {err}");
    }

    #[test]
    fn rejects_drop_inside_where() {
        let entry = IndexEntry {
            name: "ix".into(),
            on: "entries".into(),
            expression: "field".into(),
            where_clause: Some("1=1 OR DROP TABLE entries".into()),
            unique: false,
        };
        let err = entry.build_ddl("k").unwrap_err().to_string();
        assert!(err.contains("DROP"), "msg = {err}");
    }

    #[test]
    fn allows_select_subquery_in_where() {
        // SELECT/FROM are deliberately *not* on the blocklist so the
        // bead's own design example (chore-tracking partial index)
        // remains expressible.
        let entry = IndexEntry {
            name: "chore_next_due".into(),
            on: "entries".into(),
            expression: "COALESCE(json_extract(data,'$.next_time'), start_time)".into(),
            where_clause: Some(
                "field IN (SELECT name FROM field_specs WHERE kit_id = 'chores')".into(),
            ),
            unique: false,
        };
        entry.build_ddl("chores").unwrap();
    }

    #[test]
    fn sqlite_name_sanitizes_kit_id() {
        let entry = IndexEntry {
            name: "ix".into(),
            on: "entries".into(),
            expression: "field".into(),
            where_clause: None,
            unique: false,
        };
        // A weird (but technically syntactically-valid in TOML) kit name
        // gets non-alphanumerics flattened to `_` so we never paste an
        // unescaped string into a SQL identifier.
        assert_eq!(entry.sqlite_name("my-kit.v2"), "kit_my_kit_v2__ix");
    }

    #[test]
    fn round_trip_preserves_indices() {
        let kit = KitToml {
            kit: KitMeta {
                name: "k".into(),
                version: None,
                description: None,
            },
            fields: vec![],
            views: vec![],
            aliases: vec![],
            indices: vec![IndexEntry {
                name: "by_field".into(),
                on: "entries".into(),
                expression: "field, start_time".into(),
                where_clause: Some("deleted_at IS NULL".into()),
                unique: false,
            }],
            commands: vec![],
        };
        let toml = kit.to_toml().unwrap();
        let back = KitToml::from_str(&toml).unwrap();
        assert_eq!(back.indices.len(), 1);
        assert_eq!(back.indices[0], kit.indices[0]);
    }

    #[test]
    fn parses_kit_with_level1_and_level2_commands() {
        let toml = r#"
[kit]
name = "chores"

[[command]]
name = "chore"
expands_to = "occur chore -k next_date"

[[command]]
name = "chorelist"
view = "chores_due"
help = "list chores by next-due date"
columns = ["field", "next_due_raw", "iti"]
labels = { next_due_raw = "due", iti = "freq" }
formats = { next_due_raw = "smart_date", iti = "duration" }
sort = "next_due_raw"
flags = ["watch"]

[[command.color_rules]]
when = "next_due_raw < now"
color = "red"

[[command.color_rules]]
when = "next_due_raw < today + 1d"
color = "yellow"
"#;
        let kit = KitToml::from_str(toml).unwrap();
        assert_eq!(kit.commands.len(), 2);
        assert_eq!(kit.commands[0].name, "chore");
        assert_eq!(
            kit.commands[0].expands_to.as_deref(),
            Some("occur chore -k next_date")
        );
        assert_eq!(kit.commands[1].name, "chorelist");
        assert_eq!(kit.commands[1].view.as_deref(), Some("chores_due"));
        assert_eq!(kit.commands[1].columns, vec!["field", "next_due_raw", "iti"]);
        assert_eq!(kit.commands[1].sort.as_deref(), Some("next_due_raw"));
        assert_eq!(kit.commands[1].flags, vec!["watch".to_string()]);
        assert_eq!(kit.commands[1].color_rules.len(), 2);
        assert_eq!(kit.commands[1].color_rules[0].color, "red");
    }

    #[test]
    fn round_trip_preserves_commands() {
        let toml_in = r#"
[kit]
name = "chores"

[[command]]
name = "chorelist"
view = "chores_due"
columns = ["field", "next_due_raw"]
sort = "next_due_raw"

[[command.color_rules]]
when = "next_due_raw < now"
color = "red"
"#;
        let kit = KitToml::from_str(toml_in).unwrap();
        let emitted = kit.to_toml().unwrap();
        let back = KitToml::from_str(&emitted).unwrap();
        assert_eq!(back.commands.len(), 1);
        assert_eq!(back.commands[0].view.as_deref(), Some("chores_due"));
        assert_eq!(back.commands[0].sort.as_deref(), Some("next_due_raw"));
        assert_eq!(back.commands[0].color_rules.len(), 1);
        assert_eq!(back.commands[0].color_rules[0].when, "next_due_raw < now");
    }

    #[test]
    fn level3_script_round_trips_for_forward_compat() {
        // The TOML can declare a script field; install rejects it but
        // round-trip preserves it (so future versions could honour it).
        let toml = r#"
[kit]
name = "future"

[[command]]
name = "doit"
script = "shellout"
"#;
        let kit = KitToml::from_str(toml).unwrap();
        assert_eq!(kit.commands.len(), 1);
        assert_eq!(kit.commands[0].script.as_deref(), Some("shellout"));
    }
}
