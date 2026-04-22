//! Unicode glyph constants used by the visual-parity renderer.
//!
//! See `docs/visual-parity-analysis.md` §10 for the full table.

/// Point-occur / bullet glyph (filled bullet).
pub const DOT: &str = "●";

/// `add`-verb marker — mirrors the `c` suffix in the generated slug id.
pub const NON_OCCUR: &str = "¢";

/// First-tracked marker — prepended as a left-char when no prior entry exists.
pub const NULL_SET: &str = "∅";

/// Repeat / overlap warning — rendered yellow.
pub const WARNING: &str = "!";

/// Checkerboard fill for array states (upper-right + lower-left triangle).
pub const SHADING: &str = "▞";

/// Right-arrow emitted after a state badge when the state continues.
pub const ARROW_RIGHT: &str = "→";

/// Left-arrow emitted before a state badge when the prior state differs.
pub const ARROW_LEFT: &str = "←";

/// Legacy transition arrow (between prior and current state blocks).
/// Retained for callers that still use the compact arrow motif; new code
/// uses [`ARROW_LEFT`] / [`ARROW_RIGHT`].
pub const TRANSITION_ARROW: &str = "▶";

/// First-time-seen sparkle prefix (printable, survives `strip_ansi`).
pub const FIRST_TIME_MARKER: &str = "✨ ";

/// Sentinel state-value for `datum ignore <field>` — §8.
pub const IGNORED_STATE: &str = "IGNORED";
