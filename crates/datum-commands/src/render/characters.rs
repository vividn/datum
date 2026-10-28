//! Unicode glyph constants used by the visual-parity renderer.
//!
//! See `docs/visual-parity-analysis.md` §10 for the full table.

/// Point-occur / bullet glyph (filled bullet): an occur's mark when its
/// field has no symbol.
pub const DOT: &str = "●";

/// A LEAD or TRAIL slot: two columns (da-pc7x7, vividn: «it actually looks
/// nicer and suggests more about the change»), so a two-column occur symbol
/// sits where a transition's LEAD does.
pub const SLOT: &str = "  ";

/// `add`-verb marker — mirrors the `c` suffix in the generated slug id.
pub const NON_OCCUR: &str = "¢";

/// IGNORED's mark: drawn grey before an occur dot whose field was absent
/// or IGNORED just before it. Never a `null` state's — see [`NULL_STATE`].
pub const NULL_SET: &str = "∅";

/// A `null` state's body: the word, dark grey (`Rgb::DARK_GREY`) on the
/// grey ground, since in v3 null is a state someone can assign and not
/// IGNORED (da-pqdcv, vividn: «literal word null»). Where a state body is cut
/// to fit, `null` is cut like any word (`nu…`); a one-cell LEAD/TRAIL slot
/// carries no state's text, so there null is its grey ground alone — never
/// a glyph, and never IGNORED's `∅`.
pub const NULL_STATE: &str = "null";

/// A parallel set's slot halves, split on the `/` diagonal the dayview's
/// 45° stripes run along (da-pc7x7): the fg fills the lower-right of
/// [`LOWER_RIGHT`] and the upper-left of [`UPPER_LEFT`], the bg the rest.
pub const LOWER_RIGHT: &str = "◢";
pub const UPPER_LEFT: &str = "◤";

/// Sentinel state-value for `datum ignore <field>` — §8.
pub const IGNORED_STATE: &str = "IGNORED";

/// Warning glyph for the repeated-state LEAD marker (da-qfv0). Painted in
/// bold warning-yellow over the prior-state bg when a switch entry leaves
/// the field's compiled state unchanged — the visual signal that an
/// intermediate transition is missing (most often), a duplicate has been
/// logged, or a sync peer's offsetting entry hasn't arrived yet.
pub const WARNING_GLYPH: &str = "!";
