//! Unicode glyph constants used by the visual-parity renderer.
//!
//! See `docs/visual-parity-analysis.md` §10 for the full table.

/// Point-occur / bullet glyph (filled bullet).
pub const DOT: &str = "●";

/// `add`-verb marker — mirrors the `c` suffix in the generated slug id.
pub const NON_OCCUR: &str = "¢";

/// First-tracked / no-state marker — painted as the foreground of an empty
/// LEAD/TRAIL slot when there's no prior or continuing state to depict.
pub const NULL_SET: &str = "∅";

/// Checkerboard fill for array states (upper-right + lower-left triangle).
pub const SHADING: &str = "▞";

/// First-time-seen sparkle prefix (printable, survives `strip_ansi`).
pub const FIRST_TIME_MARKER: &str = "✨ ";

/// Sentinel state-value for `datum ignore <field>` — §8.
pub const IGNORED_STATE: &str = "IGNORED";
