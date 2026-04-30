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
    /// [`resolve_positions`] API; the single-spec [`FieldSpec::resolve_position`]
    /// degrades to absolute (hash) fallback because it has no view of the
    /// anchor's spec.
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

/// Default row height when `FieldSpec::height` is unset. Matches original
/// datum's `DEFAULT_HEIGHT`.
pub const DEFAULT_FIELD_HEIGHT: f64 = 0.12;

/// The subset of verbs permitted as a field's `default_verb`.
pub const DEFAULT_VERB_CHOICES: &[Verb] = &[Verb::Occur, Verb::Start, Verb::Switch, Verb::Add];

/// Deterministic-hash fallback for a field's `y` position. Stable across runs
/// and platforms so unconfigured fields land at the same vertical position
/// every time. Direct port of the original datum:
/// `parseInt(md5(field).slice(0, 8), 16) / Math.pow(2, 32)` — takes the first
/// 4 bytes of md5(field) as a big-endian u32 and divides by 2^32 to get a
/// value in `[0.0, 1.0)`.
fn hash_y(field: &str) -> f64 {
    use md5::{Digest, Md5};
    let digest = Md5::digest(field.as_bytes());
    let n = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    f64::from(n) / (f64::from(u32::MAX) + 1.0)
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

    /// Absolute fallback used by [`resolve_positions`] when this field's
    /// anchor is missing or participates in a cycle. Equivalent to calling
    /// [`Self::resolve_position`] but with the anchor triple ignored.
    fn resolve_absolute(&self, field_name: &str) -> ResolvedPosition {
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

// ─────────────────────────────────────────────────────────────────────────────
// Anchored layout (relative_to) — kit slice 2
// ─────────────────────────────────────────────────────────────────────────────

/// Outcome of [`resolve_positions`]: per-field resolved triples plus
/// diagnostics surfaced for the caller (renderer / install command) to log.
#[derive(Debug, Clone, Default)]
pub struct PositionResolution {
    /// Resolved (y, height, z_index) per field name. Includes every input
    /// field — anchored fields whose anchor was missing or cyclic fall back
    /// to absolute defaults so callers always get a position.
    pub positions: HashMap<String, ResolvedPosition>,
    /// `(dependent_field, missing_anchor_name)` pairs. The dependent fell
    /// back to absolute. Empty when every anchor resolved.
    pub missing_anchors: Vec<(String, String)>,
    /// Anchor cycles. Each inner vec is the cycle path closed at both ends,
    /// e.g. `["a", "b", "c", "a"]`. Cycle members all fall back to absolute.
    pub cycles: Vec<Vec<String>>,
    /// Anchor-triple invariant violations (some of relative_to / pos_ratio /
    /// size_ratio set, others not). The offending field is treated as
    /// absolute.
    pub malformed: Vec<String>,
}

/// Resolve positions for a set of fields, honoring `relative_to` anchors
/// via topological sort. Anchors not present in `specs` (or whose triple is
/// malformed, or whose chain forms a cycle) are reported as diagnostics
/// while the dependent falls back to absolute resolution.
///
/// Resolution rules:
/// - Absolute (no `relative_to`): `y`/`height`/`z_index` with hash/default
///   fallback (same as [`FieldSpec::resolve_position`]).
/// - Anchored: `effective_y = anchor.y + anchor.height * anchor_pos_ratio`,
///   `effective_height = anchor.height * anchor_size_ratio`,
///   `z_index = self.z_index.unwrap_or(effective_y)` (anchored fields
///   default to rendering above their anchor since `effective_y` differs).
/// - Out-of-`[0, 1]` ratios are allowed and not clamped — useful for stacked
///   layouts above/below an anchor.
pub fn resolve_positions(
    specs: &HashMap<String, FieldSpec>,
    fields: impl IntoIterator<Item = String>,
) -> PositionResolution {
    let requested: Vec<String> = fields.into_iter().collect();
    let mut report = PositionResolution::default();

    // Detect cycles up front over the *full* spec graph so we know which
    // fields to short-circuit. Visiting only requested-field roots would
    // miss cycles that lie entirely in chained anchors not directly asked
    // for.
    let cycles = detect_anchor_cycles(specs);
    let mut in_cycle: std::collections::HashSet<String> = std::collections::HashSet::new();
    for c in &cycles {
        // Skip the duplicated tail when collecting members.
        for name in c.iter().take(c.len().saturating_sub(1)) {
            in_cycle.insert(name.clone());
        }
    }
    report.cycles = cycles;

    // Detect malformed anchor triples once across the full spec set.
    let mut malformed: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (name, spec) in specs {
        if spec.has_anchor() && spec.validate_anchor_triple().is_err() {
            malformed.insert(name.clone());
            report.malformed.push(name.clone());
        }
    }

    // Memoize so a chain visited from multiple roots only resolves once.
    let mut memo: HashMap<String, ResolvedPosition> = HashMap::new();
    let mut warned: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();

    // Cover both explicitly-requested fields and any spec mentioned via an
    // anchor chain — so dayview can hand us only the fields it has data for
    // and we still resolve their anchors transitively.
    let mut roots: Vec<String> = requested;
    roots.extend(specs.keys().cloned());
    roots.sort();
    roots.dedup();

    for root in roots {
        resolve_one(
            &root,
            specs,
            &in_cycle,
            &malformed,
            &mut memo,
            &mut warned,
            &mut report,
        );
    }

    report.positions = memo;
    report
}

/// Recursive walker for [`resolve_positions`]. Anchors are resolved
/// depth-first; results are memoized so each field resolves once. Fields in
/// a cycle, malformed, or whose anchor is missing fall back to the absolute
/// path via [`FieldSpec::resolve_absolute`] (or — when no spec exists — the
/// hash/default defaults synthesized via a transient default spec).
fn resolve_one(
    field: &str,
    specs: &HashMap<String, FieldSpec>,
    in_cycle: &std::collections::HashSet<String>,
    malformed: &std::collections::HashSet<String>,
    memo: &mut HashMap<String, ResolvedPosition>,
    warned: &mut std::collections::HashSet<(String, String)>,
    report: &mut PositionResolution,
) -> ResolvedPosition {
    if let Some(p) = memo.get(field) {
        return *p;
    }

    let spec = specs.get(field);

    // Path 1: no spec at all → hash/default fallback.
    let Some(spec) = spec else {
        let pos = FieldSpec::new(field, Verb::Occur).resolve_absolute(field);
        memo.insert(field.to_string(), pos);
        return pos;
    };

    // Path 2: in a cycle or malformed → absolute fallback. Cycles are
    // already collected in `report.cycles`; malformed is in `report.malformed`.
    if in_cycle.contains(field) || malformed.contains(field) {
        let pos = spec.resolve_absolute(field);
        memo.insert(field.to_string(), pos);
        return pos;
    }

    // Path 3: not anchored → absolute resolution. The triple invariant is
    // already enforced (we'd be in `malformed` otherwise); has_anchor()
    // covers the well-formed-anchored case below.
    if spec.relative_to.is_none() {
        let pos = spec.resolve_absolute(field);
        memo.insert(field.to_string(), pos);
        return pos;
    }

    // Path 4: well-formed anchor. Look up the anchor; missing → warn + fall
    // back to absolute. Triple invariant guarantees both ratios are Some.
    let anchor_name = spec.relative_to.as_deref().unwrap();
    let pos_ratio = spec.anchor_pos_ratio.unwrap();
    let size_ratio = spec.anchor_size_ratio.unwrap();

    if !specs.contains_key(anchor_name) {
        // First time we've seen this missing anchor for this dependent —
        // record once.
        let key = (field.to_string(), anchor_name.to_string());
        if warned.insert(key) {
            report
                .missing_anchors
                .push((field.to_string(), anchor_name.to_string()));
        }
        let pos = spec.resolve_absolute(field);
        memo.insert(field.to_string(), pos);
        return pos;
    }

    // Recurse for the anchor's resolved triple. Since cycles are pre-detected,
    // recursion terminates.
    let anchor_pos = resolve_one(
        anchor_name,
        specs,
        in_cycle,
        malformed,
        memo,
        warned,
        report,
    );

    let y = anchor_pos.y + anchor_pos.height * pos_ratio;
    let height = anchor_pos.height * size_ratio;
    let z_index = spec.z_index.unwrap_or(y);
    let resolved = ResolvedPosition { y, height, z_index };
    memo.insert(field.to_string(), resolved);
    resolved
}

/// Detect cycles in the (field → relative_to) graph. Returns a Vec of
/// cycle paths; each path is a Vec of field names with the start name
/// repeated at the end (e.g. `["a", "b", "a"]`). Empty when acyclic.
///
/// Implementation: DFS-based three-color scan (white = unvisited, gray =
/// on the current path, black = fully explored). When a gray node is
/// revisited we slice the current path from the first occurrence to record
/// the cycle.
pub fn detect_anchor_cycles(specs: &HashMap<String, FieldSpec>) -> Vec<Vec<String>> {
    use std::collections::HashSet;

    let mut color: HashMap<String, DfsColor> =
        specs.keys().map(|k| (k.clone(), DfsColor::White)).collect();
    let mut cycles: Vec<Vec<String>> = Vec::new();
    let mut seen_cycle_keys: HashSet<Vec<String>> = HashSet::new();

    // Iterate roots in sorted order so detected cycles come back in a
    // deterministic order (kit install error messages and tests).
    let mut roots: Vec<String> = specs.keys().cloned().collect();
    roots.sort();

    for root in roots {
        if matches!(color.get(&root).copied(), Some(DfsColor::White)) {
            let mut path: Vec<String> = Vec::new();
            dfs_anchor(
                &root,
                specs,
                &mut color,
                &mut path,
                &mut cycles,
                &mut seen_cycle_keys,
            );
        }
    }
    cycles
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum DfsColor {
    White,
    Gray,
    Black,
}

fn dfs_anchor(
    node: &str,
    specs: &HashMap<String, FieldSpec>,
    color: &mut HashMap<String, DfsColor>,
    path: &mut Vec<String>,
    cycles: &mut Vec<Vec<String>>,
    seen_cycle_keys: &mut std::collections::HashSet<Vec<String>>,
) {
    color.insert(node.to_string(), DfsColor::Gray);
    path.push(node.to_string());

    if let Some(spec) = specs.get(node) {
        if let Some(anchor) = spec.relative_to.as_deref() {
            // Edge skipped if the anchor is missing — that's a missing-anchor
            // diagnostic, not a cycle.
            if let Some(c) = color.get(anchor).copied() {
                match c {
                    DfsColor::White => {
                        dfs_anchor(anchor, specs, color, path, cycles, seen_cycle_keys);
                    }
                    DfsColor::Gray => {
                        // Cycle: slice from the first occurrence of `anchor`
                        // in `path` and append `anchor` to close the loop.
                        if let Some(idx) = path.iter().position(|n| n == anchor) {
                            let mut cycle: Vec<String> = path[idx..].to_vec();
                            cycle.push(anchor.to_string());
                            // Canonicalize so the same cycle reached from
                            // different start points dedups.
                            let key = canonical_cycle_key(&cycle);
                            if seen_cycle_keys.insert(key) {
                                cycles.push(cycle);
                            }
                        }
                    }
                    DfsColor::Black => { /* fully explored — no cycle */ }
                }
            }
        }
    }

    path.pop();
    color.insert(node.to_string(), DfsColor::Black);
}

/// Canonical key for a cycle path so two rotations of the same cycle dedup.
/// Drops the closing duplicate, rotates the path so the lexicographically
/// smallest member is first, and returns the rotated names.
fn canonical_cycle_key(cycle: &[String]) -> Vec<String> {
    let body = if cycle.first() == cycle.last() && cycle.len() > 1 {
        &cycle[..cycle.len() - 1]
    } else {
        cycle
    };
    if body.is_empty() {
        return Vec::new();
    }
    let min_idx = body
        .iter()
        .enumerate()
        .min_by_key(|(_, s)| s.as_str())
        .map(|(i, _)| i)
        .unwrap_or(0);
    let mut out: Vec<String> = Vec::with_capacity(body.len());
    out.extend_from_slice(&body[min_idx..]);
    out.extend_from_slice(&body[..min_idx]);
    out
}

/// Format a cycle path as `A → B → C → A` for user-facing error messages.
/// Mirrors the format expected by the kit-install error message.
pub fn format_cycle(cycle: &[String]) -> String {
    cycle.join(" → ")
}

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

    #[test]
    fn regenerate_derived_aliases_covers_bare_terminal_and_states() {
        let mut spec = FieldSpec::new("mood", Verb::Switch);
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

    // ─── Position resolution (da-dqb) ───────────────────────────────────────

    #[test]
    fn resolve_position_uses_explicit_values_when_set() {
        let mut spec = FieldSpec::new("sleep", Verb::Start);
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
        let spec = FieldSpec::new("coffee", Verb::Occur);
        let r = spec.resolve_position("coffee");
        // hash fallback in [0.0, 1.0)
        assert!(r.y >= 0.0 && r.y < 1.0);
        assert_eq!(r.height, DEFAULT_FIELD_HEIGHT);
        // z_index defaults to resolved y
        assert_eq!(r.z_index, r.y);
    }

    #[test]
    fn resolve_position_z_index_defaults_to_y_when_y_is_explicit() {
        let mut spec = FieldSpec::new("focus", Verb::Switch);
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
        let mut spec = FieldSpec::new("dream", Verb::Switch);
        spec.height = Some(0.30);
        let r = spec.resolve_position("dream");
        assert_eq!(r.height, 0.30);
        assert!(r.y >= 0.0 && r.y < 1.0);
        assert_eq!(r.z_index, r.y);

        // Only z_index set; y comes from hash, height from default.
        let mut spec = FieldSpec::new("dream", Verb::Switch);
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
        assert!(a >= 0.0 && a < 1.0);

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
        let mut spec = FieldSpec::new("sleep", Verb::Start);
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
        let mut sleep = FieldSpec::new("sleep", Verb::Start);
        sleep.y = Some(0.05);
        sleep.height = Some(0.20);
        sleep.z_index = Some(1.0);

        let mut focus = FieldSpec::new("focus", Verb::Switch);
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
        let spec = FieldSpec::new("plain", Verb::Occur);
        assert!(spec.renders_on("dayview"));
        assert!(spec.renders_on("nowview"));
        assert!(spec.renders_on("future_surface"));
    }

    #[test]
    fn render_all_false_hides_every_surface() {
        let mut spec = FieldSpec::new("background_telemetry", Verb::Occur);
        spec.render = Some(Render::All(false));
        assert!(!spec.renders_on("dayview"));
        assert!(!spec.renders_on("nowview"));
        assert!(!spec.renders_on("anything"));
    }

    #[test]
    fn render_all_true_shows_every_surface() {
        let mut spec = FieldSpec::new("loud", Verb::Occur);
        spec.render = Some(Render::All(true));
        assert!(spec.renders_on("dayview"));
        assert!(spec.renders_on("nowview"));
    }

    #[test]
    fn render_per_surface_hides_only_named_surface() {
        let mut spec = FieldSpec::new("stretch_routine", Verb::Occur);
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
        let mut spec = FieldSpec::new("experimental", Verb::Occur);
        let mut map = HashMap::new();
        map.insert("dayview".to_string(), false);
        map.insert("nowview".to_string(), false);
        spec.render = Some(Render::PerSurface(map));
        assert!(!spec.renders_on("dayview"));
        assert!(!spec.renders_on("nowview"));
    }

    #[test]
    fn render_serde_roundtrip_bool_form() {
        let mut spec = FieldSpec::new("hidden", Verb::Occur);
        spec.render = Some(Render::All(false));
        let json = serde_json::to_string(&spec).unwrap();
        // bool form serializes as a bare bool (untagged enum).
        assert!(json.contains("\"render\":false"), "json = {json}");
        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.render, Some(Render::All(false)));
    }

    #[test]
    fn render_serde_roundtrip_per_surface_form() {
        let mut spec = FieldSpec::new("partial", Verb::Occur);
        let mut map = HashMap::new();
        map.insert("dayview".to_string(), false);
        spec.render = Some(Render::PerSurface(map));
        let json = serde_json::to_string(&spec).unwrap();
        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.render, spec.render);
    }

    #[test]
    fn render_serde_omits_when_none() {
        let spec = FieldSpec::new("plain", Verb::Occur);
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
        let json = r#"{"field":"x","default_verb":"occur","subfields":{},"render":{"dayview":false}}"#;
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
        let mut spec = FieldSpec::new("dishes", Verb::Occur);
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
        let spec = FieldSpec::new("plain", Verb::Occur);
        let json = serde_json::to_string(&spec).unwrap();
        // Empty tags are skipped to keep the on-disk shape small + diffable.
        assert!(!json.contains("\"tags\""), "json = {json}");

        let mut spec = FieldSpec::new("dishes", Verb::Occur);
        spec.tags = vec!["chore".into(), "daily".into()];
        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("\"tags\":[\"chore\",\"daily\"]"));
        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tags, spec.tags);
    }

    // ─── Anchored layout tests (da-d4t.2) ────────────────────────────────────

    fn anchored_spec(name: &str, anchor: &str, pos_ratio: f64, size_ratio: f64) -> FieldSpec {
        let mut spec = FieldSpec::new(name, Verb::Occur);
        spec.relative_to = Some(anchor.into());
        spec.anchor_pos_ratio = Some(pos_ratio);
        spec.anchor_size_ratio = Some(size_ratio);
        spec
    }

    fn absolute_spec(name: &str, y: f64, h: f64) -> FieldSpec {
        let mut spec = FieldSpec::new(name, Verb::Occur);
        spec.y = Some(y);
        spec.height = Some(h);
        spec
    }

    #[test]
    fn validate_anchor_triple_accepts_all_unset_or_all_set() {
        let mut spec = FieldSpec::new("a", Verb::Occur);
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
    fn resolve_positions_chains_anchor_a_b_c() {
        // body absolute; contacts anchored to body; thumbnail anchored to
        // contacts. Resolution order doesn't matter — topological walk.
        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert("body".into(), absolute_spec("body", 0.20, 0.40));
        specs.insert(
            "contacts".into(),
            anchored_spec("contacts", "body", 0.10, 0.05),
        );
        specs.insert(
            "thumbnail".into(),
            anchored_spec("thumbnail", "contacts", 0.5, 0.5),
        );

        let report = resolve_positions(&specs, ["thumbnail".to_string()]);
        assert!(report.cycles.is_empty());
        assert!(report.missing_anchors.is_empty());

        let body = report.positions.get("body").unwrap();
        assert_eq!(body.y, 0.20);
        assert_eq!(body.height, 0.40);

        let contacts = report.positions.get("contacts").unwrap();
        // y = 0.20 + 0.40 * 0.10 = 0.24; height = 0.40 * 0.05 = 0.02
        assert!((contacts.y - 0.24).abs() < 1e-9);
        assert!((contacts.height - 0.02).abs() < 1e-9);

        let thumb = report.positions.get("thumbnail").unwrap();
        // y = 0.24 + 0.02 * 0.5 = 0.25; height = 0.02 * 0.5 = 0.01
        assert!((thumb.y - 0.25).abs() < 1e-9);
        assert!((thumb.height - 0.01).abs() < 1e-9);
    }

    #[test]
    fn resolve_positions_warns_and_falls_back_on_missing_anchor() {
        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert(
            "contacts".into(),
            anchored_spec("contacts", "body", 0.10, 0.05),
        );
        let report = resolve_positions(&specs, ["contacts".to_string()]);
        assert_eq!(report.missing_anchors.len(), 1);
        assert_eq!(report.missing_anchors[0].0, "contacts");
        assert_eq!(report.missing_anchors[0].1, "body");
        // Falls back to absolute → hash-based y for "contacts"
        let pos = report.positions.get("contacts").unwrap();
        assert_eq!(pos.y, hash_y("contacts"));
        assert_eq!(pos.height, DEFAULT_FIELD_HEIGHT);
    }

    #[test]
    fn detect_anchor_cycles_finds_two_node_cycle() {
        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert("a".into(), anchored_spec("a", "b", 0.0, 1.0));
        specs.insert("b".into(), anchored_spec("b", "a", 0.0, 1.0));
        let cycles = detect_anchor_cycles(&specs);
        assert_eq!(cycles.len(), 1, "expected one cycle, got {cycles:?}");
        let c = &cycles[0];
        // First and last names match — cycle closes on itself.
        assert_eq!(c.first(), c.last());
    }

    #[test]
    fn detect_anchor_cycles_finds_three_node_cycle() {
        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert("a".into(), anchored_spec("a", "b", 0.0, 1.0));
        specs.insert("b".into(), anchored_spec("b", "c", 0.0, 1.0));
        specs.insert("c".into(), anchored_spec("c", "a", 0.0, 1.0));
        let cycles = detect_anchor_cycles(&specs);
        assert_eq!(cycles.len(), 1);
        // Body should contain all three names.
        let body: std::collections::HashSet<&str> = cycles[0]
            .iter()
            .take(cycles[0].len() - 1)
            .map(String::as_str)
            .collect();
        assert!(body.contains("a"));
        assert!(body.contains("b"));
        assert!(body.contains("c"));
    }

    #[test]
    fn resolve_positions_falls_back_for_cycle_members() {
        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert("a".into(), anchored_spec("a", "b", 0.5, 0.5));
        specs.insert("b".into(), anchored_spec("b", "a", 0.5, 0.5));
        let report = resolve_positions(&specs, ["a".to_string(), "b".to_string()]);
        assert_eq!(report.cycles.len(), 1);
        // Both members fall back to absolute (hash y, default height).
        let a = report.positions.get("a").unwrap();
        let b = report.positions.get("b").unwrap();
        assert_eq!(a.y, hash_y("a"));
        assert_eq!(b.y, hash_y("b"));
        assert_eq!(a.height, DEFAULT_FIELD_HEIGHT);
        assert_eq!(b.height, DEFAULT_FIELD_HEIGHT);
    }

    #[test]
    fn resolve_positions_allows_out_of_unit_ratios() {
        // Negative pos_ratio → field bleeds *above* its anchor; size_ratio > 1.0
        // → field is taller than its anchor. Both are intentional layouts and
        // must not be clamped.
        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert("body".into(), absolute_spec("body", 0.50, 0.20));
        specs.insert("halo".into(), anchored_spec("halo", "body", -0.5, 1.5));
        let report = resolve_positions(&specs, ["halo".to_string()]);
        assert!(report.cycles.is_empty());
        let halo = report.positions.get("halo").unwrap();
        // y = 0.50 + 0.20 * -0.5 = 0.40; height = 0.20 * 1.5 = 0.30
        assert!((halo.y - 0.40).abs() < 1e-9);
        assert!((halo.height - 0.30).abs() < 1e-9);
    }

    #[test]
    fn resolve_positions_marks_malformed_triple_and_falls_back() {
        // anchor_pos_ratio set, anchor_size_ratio missing → malformed.
        let mut spec = FieldSpec::new("bad", Verb::Occur);
        spec.relative_to = Some("body".into());
        spec.anchor_pos_ratio = Some(0.1);
        // Intentionally omit anchor_size_ratio.

        let mut specs: HashMap<String, FieldSpec> = HashMap::new();
        specs.insert("body".into(), absolute_spec("body", 0.20, 0.40));
        specs.insert("bad".into(), spec);
        let report = resolve_positions(&specs, ["bad".to_string()]);
        assert!(report.malformed.iter().any(|n| n == "bad"));
        // Fell back to absolute (no y set on the spec → hash).
        let bad = report.positions.get("bad").unwrap();
        assert_eq!(bad.y, hash_y("bad"));
    }

    #[test]
    fn anchor_fields_serde_roundtrip() {
        let spec = anchored_spec("contacts", "body", 0.1, 0.05);
        let json = serde_json::to_string(&spec).unwrap();
        let back: FieldSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.relative_to.as_deref(), Some("body"));
        assert_eq!(back.anchor_pos_ratio, Some(0.1));
        assert_eq!(back.anchor_size_ratio, Some(0.05));
    }

    // ─── Template interpolation tests (A1 acceptance) ────────────────────────

    fn occur_ctx<'a>(
        field: &'a str,
        extra: Option<&'a JsonValue>,
        uid: &'a str,
    ) -> InterpolationContext<'a> {
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
