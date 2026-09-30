//! The TRACK document — the row that says a field exists and what it is.
//!
//! `datum track <field>` has always written a switch row into the log
//! (da-2sr): tracking is evidence, not a spec flag. da-c8mh made that row
//! travel with the field's FIRST log too; da-gvcen took that half back —
//! a first log declares its field in its OWN `following_state`, and the
//! spec is what records that the field exists. What still writes a TRACK
//! row is the EXPLICIT act: `datum track`, which has no entry of its own
//! to carry a trailing state and whose row is what un-ignores a field
//! (ignored-ness is read off the log, da-d08c), and `db v2import`, which
//! declares each field it brings ahead of the history it brings.
//!
//! The row carries that answer in `extra_metadata.track`, which is also
//! how every reader recognises it: a TRACK row's state is `false` or the
//! TRACKED sentinel (da-5ifp), and `false` alone is indistinguishable from
//! a plain `end`. The marker is what lets `get` and the write echo print
//! `TRACK●` where they would otherwise print an action. A TABLE does not
//! (da-ftg8p): a column only a declaration fills is one row re-laying-out
//! the rows around it.

use serde_json::{json, Value as JsonValue};

use crate::action::Action;
use crate::entry::{Entry, Verb};
use crate::shape::TRACKED_STATE_VALUE;
use crate::time::DatumTime;

/// The `extra_metadata` key a TRACK row is marked with. Its value is the
/// verb the field was declared with, as a string.
pub const TRACK_META_KEY: &str = "track";

/// The SQL predicate for "this row is not a declaration". Every reader
/// that counts TRANSITIONS — opens and closes, the states a field has been
/// switched to, what `stats` measures — spells the exclusion with this one
/// clause rather than its own.
pub const NOT_A_DECLARATION_SQL: &str =
    "json_extract(COALESCE(extra_metadata, '{}'), '$.track') IS NULL";

/// The six-character action word a TRACK row renders as: `TRACK` plus one
/// glyph naming the verb — a dot for a moment, a filled block for a span,
/// the switch hatch, and `¢` for the timeless jot. Six characters wide,
/// like `CREATE`, and each glyph is one display column.
pub fn track_action_label(verb: Action) -> &'static str {
    match verb {
        Action::Start | Action::Stop | Action::Block => "TRACK█",
        Action::Switch => "TRACK▞",
        Action::Add | Action::Note => "TRACK¢",
        Action::Occur => "TRACK●",
    }
}

/// The verb a TRACK row declares, or `None` when the row is not one.
pub fn track_verb(entry: &Entry) -> Option<Action> {
    entry
        .extra_metadata
        .as_ref()?
        .get(TRACK_META_KEY)?
        .as_str()?
        .parse()
        .ok()
}

/// The verb a field is declared with when its first log used `action` —
/// the four canonical choices, reached from any command word. `end` on a
/// never-started field declares it a `start` field (vividn, 2026-09-15): an
/// end is evidence of a span, and the span is what the field is for.
pub fn verb_for_action(action: Action) -> Action {
    match action {
        Action::Start | Action::Stop | Action::Block => Action::Start,
        Action::Switch => Action::Switch,
        Action::Add | Action::Note => Action::Add,
        Action::Occur => Action::Occur,
    }
}

/// The state a declaration of `verb` sits in — da-5ifp: a field that
/// holds a state is tracked `false`, because it has an off state to sit
/// in, and one that does not gets the TRACKED sentinel. Either way it is
/// not IGNORED, which is what un-ignores a field.
///
/// One rule, two places: the row `datum track` writes, and the
/// `following_state` a field's FIRST log declares itself with (da-gvcen).
/// The declaration changed which column it lives in, not what it says.
pub fn declared_state(verb: Action) -> JsonValue {
    if matches!(verb, Action::Start | Action::Switch) {
        JsonValue::Bool(false)
    } else {
        JsonValue::String(TRACKED_STATE_VALUE.to_string())
    }
}

/// The TRACK document for `field` declared with `verb`, at `at`.
pub fn track_entry(field: &str, verb: Action, at: DatumTime, client_id: &str) -> Entry {
    let state = declared_state(verb);
    let mut entry = Entry::new_legacy(
        field,
        Verb::Switch,
        Some(state),
        None,
        Some(at),
        None,
        client_id,
    );
    entry.extra_metadata = Some(json!({ TRACK_META_KEY: verb.to_string() }));
    entry
}
