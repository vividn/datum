//! Field specs and the alias system.
//!
//! A [`FieldSpec`] is the source of truth for a field's schema: default verb,
//! subfields, id template, color, allowed states with per-state color
//! overrides, and derived aliases (auto-created from allowed states).
//! Renderer-only knobs (`y`/`height`/`z_index`, anchored layout, per-surface
//! visibility) live in a sibling [`PresentationSpec`] held as
//! `FieldSpec.presentation`. The split is structural: both halves still
//! serialize into a single flat JSON blob (`#[serde(flatten)]`) and persist
//! in one `field_specs` row.
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
mod spec;

pub use alias::{Alias, DerivedAlias, DerivedAliasKind, SpecRegistry};
pub use interpolation::{
    contains_template, interpolate, InterpolationContext, DEFAULT_ID_DELIMITER,
};
pub use position::{detect_anchor_cycles, format_cycle, resolve_positions, PositionResolution};
pub use presentation::{PresentationSpec, Render, ResolvedPosition, DEFAULT_FIELD_HEIGHT};
pub use spec::{FieldSpec, SubfieldSpec, SubfieldType, DEFAULT_VERB_CHOICES};
