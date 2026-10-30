//! Field specs and the alias system.
//!
//! A [`FieldSpec`] is the source of truth for a field's schema: default verb,
//! subfields, id template, color, allowed states with per-state color
//! overrides, and derived aliases (auto-created from allowed states).
//! Renderer-only knobs (`y`/`height`/`z_index`, anchored layout, per-surface
//! visibility) live in a sibling [`PresentationSpec`] held as
//! `FieldSpec.presentation`. The split is structural: both halves still
//! serialize into a single flat JSON blob (`#[serde(flatten)]`) and persist
//! as the data of one `spec` entry (da-lc26c).
//!
//! Tracking state (tracked / ignored) is NOT stored on the spec — it's
//! derived from the entry log via [`Db::state_at`](crate::db::Db::state_at).
//! A field is ignored iff its most recent state-bearing entry has
//! `value = "IGNORED"`; any other state (including null) means tracked.
//!
//! Aliases are pre-filled argument sets that resolve to a full datum command.
//! "Derived" aliases are auto-generated from a field's allowed_states; they
//! live in the aliases table but are owned by (and cascade-deleted with) the
//! owning [`FieldSpec`].
//!
//! ## Module layout
//!
//! - [`spec`] — the [`FieldSpec`] struct, [`SubfieldSpec`], and the
//!   shape/alias regeneration logic.
//! - [`presentation`] — [`PresentationSpec`], [`Render`], [`ResolvedPosition`],
//!   and the position/visibility helpers consumed by the render layer.
//! - [`alias`] — user [`Alias`], [`DerivedAlias`] / [`DerivedAliasKind`],
//!   and the in-memory [`SpecRegistry`].
//! - [`position`] — anchored layout (`relative_to` chains, cycle detection,
//!   batch [`resolve_positions`]).
//! - [`interpolation`] — `%key%` template expansion for slug/id templates.

mod alias;
mod interpolation;
mod position;
mod presentation;
mod retired;
mod spec;

pub use alias::{
    alias_effect, shadow_warning, user_alias_to_save, Alias, AliasEffect, DerivedAlias,
    DerivedAliasKind, SpecAliasCollision, SpecAliasIndex, SpecRegistry,
};
pub use interpolation::{
    contains_template, copy_from_record, interpolate, interpolate_field, names_time, reads_record,
    record_keys, refuse_name_only_marker, strip_create_time_literal, strip_field_prefix, unfilled,
    InterpolationContext, DEFAULT_ID_DELIMITER,
};
pub use position::{detect_anchor_cycles, format_cycle, resolve_positions, PositionResolution};
pub use presentation::{
    PresentationSpec, Render, ResolvedPosition, SymbolBy, DEFAULT_FIELD_HEIGHT, GLYPH_KEYS,
};
pub use spec::{
    parse_key_decl, verb_choice, verb_from_log, Covered, DeclaredKey, FieldSpec, KeyKind,
    SubfieldSpec, SubfieldType, VERB_CHOICES, VERB_CHOICES_TEXT, VERB_NAMES_TEXT,
};
