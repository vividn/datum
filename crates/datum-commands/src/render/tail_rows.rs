//! `datum tail --json` — the machine-readable row surface (da-c7uw).
//!
//! In `render` rather than beside the command (da-6ec2): it names what a
//! rendered row SAYS, needs no database to say it, and the browser's tail
//! table is built from these rows too.
//!
//! The human table's column set is a display decision: da-8sx8 slid a `_n`
//! column in between `state` and `hid` and every script that read the table
//! positionally started reading the wrong cell — silently, because a
//! `split()` has no failure mode. This module is the supported alternative:
//! one named object per rendered row, so a consumer asks for `state` rather
//! than for "the second-to-last column".
//!
//! The key set here is therefore the contract, not the table's. Adding a
//! column to the table must not change it; adding a key here is additive and
//! safe; renaming or removing one is a breaking change and needs the same
//! care any other public interface would get.

use datum_core::entry::Entry;
use datum_core::field_spec::SpecRegistry;
use serde::Serialize;
use serde_json::Value as JsonValue;

use super::action::{
    draws_state_badge, format_lead_cell, format_state_body, format_trail_cell, slot_hex,
    StateTransition,
};
use super::state_truncate::state_label;
use super::{RenderContext, TimeMetricCol};

/// The `--json` document. The rows sit under a key rather than at the top
/// level so anything later worth reporting alongside them — a count, the
/// resolved window — can be added without breaking the readers this flag
/// exists to protect.
#[derive(Debug, Serialize)]
pub struct TailDocument {
    pub rows: Vec<TailRow>,
    /// The table's own columns for the same rows — key, header word, one
    /// cell per row in `rows` order — painted when colour is on and never
    /// padded. For a caller with a grid of its own (datum.day's tail pane,
    /// da-6ec2), which lays these out rather than re-deriving them.
    pub columns: Vec<super::TableColumn>,
}

/// One row of `datum tail`, named.
#[derive(Debug, Serialize)]
pub struct TailRow {
    /// Full 12-char hid — the durable identity. Prefix-addressable, so it
    /// can be handed straight back to `datum get` / `update` / `del`.
    pub hid: String,
    /// `field:<ident>` handle. Mutable (`retime` rewrites it); use `hid` to
    /// name a row across time.
    pub slug: String,
    pub field: String,
    /// `point` | `switch` | `block` | `block-switch` | `point-switch`.
    pub verb: String,
    /// The stamp the `time` column would show, RFC-3339. Occur times keep
    /// the offset they were recorded at; create/modify times are UTC.
    /// `null` for a timeless row (or under `-m none`).
    pub time: Option<String>,
    /// Which stamp `time` came from — `occur` | `create` | `modify`. Under
    /// the default `-m hybrid` a row with no occur time falls back to its
    /// create time, and this is how a consumer tells the two apart (the
    /// table's marker for it is a gray `c` prefix).
    pub time_metric: Option<&'static str>,
    /// ISO-8601 ± radius on the occur time (`PT5M` for `-q`), or `null`
    /// when unstated. The table renders this as the gray `±5m` suffix.
    pub time_uncertainty: Option<String>,
    /// The field's state immediately BEFORE this row — the first of the
    /// three values a transition is made of (da-fknn8). The IGNORED
    /// sentinel where the field was not tracked there, so "nothing before
    /// this" is a state like any other rather than an absence a reader has
    /// to interpret. This is the value the terminal's LEAD slot paints,
    /// and it is where the `∅` comes from: a row whose `previous_state` is
    /// `"IGNORED"` is a row that begins tracking.
    pub previous_state: JsonValue,
    /// The plain text for `previous_state`, on the same rules as
    /// `state_label`.
    pub previous_state_label: Option<String>,
    /// The hex `previous_state` is painted with — the colour the terminal's
    /// LEAD slot carries. `null` ONLY where the state paints nothing at
    /// all (`false`, the "field has ended" sentinel); an untracked or
    /// IGNORED previous state is grey, not absent.
    pub previous_state_color: Option<String>,
    /// The terminal's LEAD SLOT ITSELF, painted — one character of the
    /// previous state's colour, or the grey `∅` a first entry shows, or
    /// the repeated-state `!` (da-qfv0), or empty where the group draws
    /// no lead at all.
    ///
    /// ⚑ vividn, on da-fknn8: *"The previous and following states should
    /// just have the colors, not the full text — just like the CLI.
    /// Really mirror the CLI here."* So this is the slot rather than the
    /// ingredients for one: a consumer draws THIS and adds only what a
    /// terminal cannot — `previous_state_label` on a hover, and a box it
    /// can select. Every shape the terminal has (a parallel state's
    /// two-colour `▞`, an ended field's unpainted space) arrives already
    /// resolved, so no browser has to re-derive one.
    pub previous_state_cell: String,
    /// The state value as stored — a string, bool, number, object, array,
    /// or `null`. The CURRENT state: the middle of the three. Unchanged.
    pub state: JsonValue,
    /// The plain text the table's state cell shows for `state`: `true` and
    /// `false` read as `start` and `end`, a structured state as `{id}`.
    /// `null` when the row carries no state at all.
    pub state_label: Option<String>,
    /// The state cell's BODY as the terminal paints it — the state chip,
    /// or the occur dot — WITHOUT the LEAD and TRAIL slots that bracket it
    /// in the `state` column, and without the `∅` the dot borrows from its
    /// prior state. Painted when colour is on, exactly as a `columns` cell
    /// is.
    ///
    /// da-fknn8: those slots are this row's transition, and the two states
    /// they stand for are `previous_state` and `following_state` above. A
    /// consumer laying the cell out itself draws them as it likes — words,
    /// not one-character coloured spaces — and puts this between them.
    pub state_body: String,
    /// The state that CONTINUES FORWARD past this row — the third value.
    /// For a block, what resumes when it closes; for a point-shaped
    /// switch, the row's own emitted state; for a `block-switch` /
    /// `point-switch`, its trailing-edge state. This is what the
    /// terminal's TRAIL slot paints (da-fknn8): before that it was the
    /// entry's own `following_state` column alone, which is `null` on a
    /// plain switch and so said nothing about what followed.
    pub following_state: JsonValue,
    /// The plain text for `following_state`, on the same rules as
    /// `state_label`.
    pub following_state_label: Option<String>,
    /// The hex `following_state` is painted with — the terminal's TRAIL
    /// slot colour, on the same rules as `previous_state_color`.
    pub following_state_color: Option<String>,
    /// The terminal's TRAIL SLOT ITSELF, painted. See
    /// `previous_state_cell`; empty for a row with no trailing slot.
    ///
    /// `previous_state_cell` + `state_body` + `following_state_cell` is
    /// the `state` column's cell, byte for byte, for every verb.
    pub following_state_cell: String,
    /// This row's state is the state it was already in: the transition
    /// went nowhere (da-qfv0). The terminal marks it with a bold `!` in
    /// the LEAD slot — the signal that an intermediate transition is
    /// missing and should be backfilled while it is still remembered.
    ///
    /// Reported rather than left to be re-derived, because "the same
    /// state" is set-aware: `["vertical","sit"]` and `["sit","vertical"]`
    /// are one state, and `["vertical"]` → `["vertical","sit"]` is a
    /// transition (da-g1b8). A consumer comparing the two values itself
    /// would get those two cases wrong.
    pub state_repeat: bool,
    /// Stored duration intent: `MOMENT`, `UNKNOWN`, or an ISO-8601 span.
    pub dur: Option<String>,
    /// `dur` as whole seconds — `null` for `MOMENT` / `UNKNOWN`, which are
    /// not spans.
    pub dur_seconds: Option<i64>,
    /// The `_n` shorthand this row currently answers to (`"_3"`), or `null`
    /// past the ordinal cap. Relative to *now*, so it is a handle for the
    /// next command, not an identity to store.
    pub quick_id: Option<String>,
    /// Everything `--column NAME` reads from: the entry's own JSON payload.
    pub data: JsonValue,
    pub create_time: String,
    pub modify_time: String,
}

/// Build the JSON rows for a rendered tail listing. `contexts` are the same
/// per-row contexts the table renderer gets — the `_n` ordinals ride on them,
/// so what the table labels a row and what this reports agree by
/// construction.
pub fn tail_rows(
    entries: &[Entry],
    contexts: Option<&[RenderContext]>,
    metric: TimeMetricCol,
    registry: Option<&SpecRegistry>,
) -> Vec<TailRow> {
    entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let (time, time_metric) = row_time(e, metric);
            let spec = registry.and_then(|r| r.fields.get(&e.field));
            let default_ctx = RenderContext::default();
            let ctx = contexts.and_then(|c| c.get(i)).unwrap_or(&default_ctx);
            // The same trio the terminal's `[LEAD][body][TRAIL]` group
            // composes from (da-fknn8), so a reader of these rows and a
            // reader of the table are looking at the same transition.
            let t = StateTransition::of(ctx, e);
            // A TIMELESS row sits at no instant, so there is no state
            // before it and none after: the terminal gives it the `¢`
            // glyph and no slots at all (`format_state_group`), and the
            // row says the same by reporting no transition rather than an
            // IGNORED one a renderer would draw a null sign for.
            let timeless = e.occur_time.is_none() && !draws_state_badge(e);
            let (previous, following) = if timeless {
                (None, None)
            } else {
                (
                    ignored_when_absent(t.previous),
                    ignored_when_absent(t.following),
                )
            };
            TailRow {
                hid: e.hid.to_string(),
                slug: e.slug.to_string(),
                field: e.field.clone(),
                verb: e.verb.to_string(),
                time,
                time_metric,
                time_uncertainty: e.occur_uncertainty().map(str::to_string),
                previous_state_label: previous.as_ref().map(state_label),
                previous_state_color: previous
                    .as_ref()
                    .and_then(|v| slot_hex(Some(v), &e.field, spec)),
                previous_state_cell: format_lead_cell(e, spec, ctx),
                previous_state: previous.unwrap_or(JsonValue::Null),
                state: e.state.clone().unwrap_or(JsonValue::Null),
                state_label: e.state.as_ref().map(state_label),
                state_body: format_state_body(e, spec, ctx),
                following_state_label: following.as_ref().map(state_label),
                following_state_color: following
                    .as_ref()
                    .and_then(|v| slot_hex(Some(v), &e.field, spec)),
                following_state_cell: format_trail_cell(e, spec, ctx),
                following_state: following.unwrap_or(JsonValue::Null),
                state_repeat: ctx.is_state_repeat,
                dur: e.dur.as_ref().map(|d| d.to_storage_string()),
                dur_seconds: e.duration_seconds(),
                quick_id: contexts
                    .and_then(|c| c.get(i))
                    .and_then(|c| c.quick_ordinal)
                    .map(|n| format!("_{n}")),
                data: e.data.clone().unwrap_or(JsonValue::Null),
                create_time: e.create_time.to_rfc3339(),
                modify_time: e.modify_time.to_rfc3339(),
            }
        })
        .collect()
}

/// "No tracked state here" is the IGNORED state, not an absence: the
/// terminal paints `None` and the sentinel with the same grey slot, so the
/// row reports them as the same value rather than making every consumer
/// learn that `null` and `"IGNORED"` mean one thing. `false` — the
/// explicit "field has ended" sentinel — is a real value and stays as it
/// is.
fn ignored_when_absent(state: Option<JsonValue>) -> Option<JsonValue> {
    use datum_core::state::{normalize_state, StateValue};
    match state {
        Some(v) if normalize_state(&v) == StateValue::Ignored => {
            Some(StateValue::Ignored.to_json())
        }
        Some(v) => Some(v),
        None => Some(StateValue::Ignored.to_json()),
    }
}

/// The `(time, time_metric)` pair for one row under the active `-m` metric.
/// Mirrors the table's time column, including hybrid's fall back to the
/// create stamp when a row has no occur time.
fn row_time(e: &Entry, metric: TimeMetricCol) -> (Option<String>, Option<&'static str>) {
    let occur = || {
        e.occur_time
            .as_ref()
            .map(|t| (Some(t.local_time().to_rfc3339()), Some("occur")))
    };
    match metric {
        TimeMetricCol::Occur => occur().unwrap_or((None, None)),
        TimeMetricCol::Hybrid => {
            occur().unwrap_or((Some(e.create_time.to_rfc3339()), Some("create")))
        }
        TimeMetricCol::Create => (Some(e.create_time.to_rfc3339()), Some("create")),
        TimeMetricCol::Modify => (Some(e.modify_time.to_rfc3339()), Some("modify")),
        TimeMetricCol::None => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use datum_core::dur::Dur;
    use datum_core::entry::Verb;
    use datum_core::state::FieldStateEvents;
    use datum_core::time::DatumTime;
    use serde_json::json;

    fn t_at(iso: &str) -> DatumTime {
        let utc: DateTime<Utc> = iso.parse().unwrap();
        DatumTime::from_utc_and_tz(utc, "America/Chicago").unwrap()
    }

    fn entry(verb: Verb, state: Option<serde_json::Value>, dur: Option<Dur>) -> Entry {
        Entry::new("activity", verb, state, None, Some(t_at(OCCUR)), dur, "d1")
    }

    /// A row that carries its own trailing state — the compound switching
    /// shape the truth table allows `following_state` on.
    fn compound(state: Option<serde_json::Value>, following: JsonValue, dur: Dur) -> Entry {
        let verb = if dur.is_moment() {
            Verb::PointSwitch
        } else {
            Verb::BlockSwitch
        };
        let mut e = Entry::new(
            "activity",
            verb,
            state,
            None,
            Some(t_at(OCCUR)),
            Some(dur),
            "d1",
        );
        e.following_state = Some(following);
        e
    }

    const OCCUR: &str = "2024-03-16T15:15:00Z";

    /// The key set is the contract this flag exists to provide — a
    /// consumer names a field instead of counting columns (da-c7uw).
    /// Adding a key is additive and safe; renaming or dropping one breaks
    /// every reader, so it has to be a deliberate edit here too.
    #[test]
    fn row_keys_are_the_documented_contract() {
        let rows = tail_rows(
            &[entry(Verb::Switch, Some(json!("writing")), None)],
            None,
            TimeMetricCol::Hybrid,
            None,
        );
        let v = serde_json::to_value(&rows[0]).unwrap();
        // `to_value` sorts; the emitted document keeps declaration order.
        // The set is the contract either way — a reader names its key.
        let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec![
                "create_time",
                "data",
                "dur",
                "dur_seconds",
                "field",
                "following_state",
                "following_state_cell",
                "following_state_color",
                "following_state_label",
                "hid",
                "modify_time",
                "previous_state",
                "previous_state_cell",
                "previous_state_color",
                "previous_state_label",
                "quick_id",
                "slug",
                "state",
                "state_body",
                "state_label",
                "state_repeat",
                "time",
                "time_metric",
                "time_uncertainty",
                "verb",
            ]
        );
    }

    /// The case that broke `activity-guard.py`: the row's state must be
    /// readable as "end" without knowing which column it landed in.
    #[test]
    fn bool_states_carry_the_table_labels() {
        let rows = tail_rows(
            &[
                entry(Verb::Switch, Some(json!(true)), None),
                entry(Verb::Switch, Some(json!(false)), None),
                entry(Verb::Switch, Some(json!("writing")), None),
                entry(Verb::Point, None, None),
            ],
            None,
            TimeMetricCol::Hybrid,
            None,
        );
        let labels: Vec<Option<&str>> = rows.iter().map(|r| r.state_label.as_deref()).collect();
        assert_eq!(
            labels,
            vec![Some("start"), Some("end"), Some("writing"), None]
        );
        // The raw value is there too, for a consumer that would rather
        // match on `false` than on the word.
        assert_eq!(rows[1].state, json!(false));
    }

    #[test]
    fn durations_report_both_the_stored_span_and_seconds() {
        let rows = tail_rows(
            &[
                entry(
                    Verb::Block,
                    Some(json!(false)),
                    Some(Dur::of(chrono::Duration::minutes(30))),
                ),
                entry(Verb::Point, None, Some(Dur::moment())),
                entry(Verb::Block, Some(json!(true)), Some(Dur::unknown())),
            ],
            None,
            TimeMetricCol::Hybrid,
            None,
        );
        assert_eq!(rows[0].dur.as_deref(), Some("PT30M"));
        assert_eq!(rows[0].dur_seconds, Some(1800));
        // MOMENT and UNKNOWN are not spans — they report themselves, and
        // no seconds. A consumer summing durations must skip them rather
        // than read a zero that was never claimed.
        assert_eq!(rows[1].dur.as_deref(), Some("MOMENT"));
        assert_eq!(rows[1].dur_seconds, None);
        assert_eq!(rows[2].dur.as_deref(), Some("UNKNOWN"));
        assert_eq!(rows[2].dur_seconds, None);
    }

    /// The occur stamp keeps the offset it was recorded at; `time_metric`
    /// names which stamp `time` came from, which is the only way to tell
    /// hybrid's create-time fallback from a real occur time.
    #[test]
    fn time_follows_the_metric_and_says_which_one_it_used() {
        let e = entry(Verb::Switch, Some(json!("writing")), None);
        let occur = &tail_rows(std::slice::from_ref(&e), None, TimeMetricCol::Hybrid, None)[0];
        assert_eq!(occur.time.as_deref(), Some("2024-03-16T10:15:00-05:00"));
        assert_eq!(occur.time_metric, Some("occur"));

        let modify = &tail_rows(std::slice::from_ref(&e), None, TimeMetricCol::Modify, None)[0];
        assert_eq!(modify.time_metric, Some("modify"));
        assert_eq!(modify.time.as_deref(), Some(&*e.modify_time.to_rfc3339()));

        let mut timeless = e.clone();
        timeless.occur_time = None;
        let hybrid = &tail_rows(
            std::slice::from_ref(&timeless),
            None,
            TimeMetricCol::Hybrid,
            None,
        )[0];
        assert_eq!(hybrid.time_metric, Some("create"));
        assert_eq!(hybrid.time.as_deref(), Some(&*e.create_time.to_rfc3339()));

        let occur_only = &tail_rows(
            std::slice::from_ref(&timeless),
            None,
            TimeMetricCol::Occur,
            None,
        )[0];
        assert_eq!(occur_only.time, None);
        assert_eq!(occur_only.time_metric, None);
    }

    /// The bead this exists for (da-fknn8): `a → b → c → a` reported
    /// nothing about its transitions — `following_state` was the entry's
    /// own column, `null` on every plain switch, and there was no
    /// previous state at all. The three values now come off the same
    /// context the painter's `[LEAD][body][TRAIL]` group composes from.
    #[test]
    fn a_to_b_carries_both_edges_of_the_transition() {
        let a = entry(Verb::Switch, Some(json!("a")), None);
        let b = entry(Verb::Switch, Some(json!("b")), None);
        let ctxs = vec![
            RenderContext {
                lead_state: None,
                trail_state: Some(json!("a")),
                ..RenderContext::default()
            },
            RenderContext {
                lead_state: Some(json!("a")),
                trail_state: Some(json!("b")),
                ..RenderContext::default()
            },
        ];
        let rows = tail_rows(&[a, b], Some(&ctxs), TimeMetricCol::Hybrid, None);
        // The first row begins tracking: nothing before it is the IGNORED
        // state, not an absence a reader has to interpret.
        assert_eq!(rows[0].previous_state, json!("IGNORED"));
        assert_eq!(rows[0].previous_state_label.as_deref(), Some("IGNORED"));
        assert_eq!(rows[0].following_state, json!("a"));
        // …and the second names both edges. `following_state` here is the
        // state that continues forward, which a plain switch never
        // reported before: its own `following_state` column is null.
        assert_eq!(rows[1].previous_state, json!("a"));
        assert_eq!(rows[1].state, json!("b"));
        assert_eq!(rows[1].following_state, json!("b"));
        assert_eq!(rows[1].previous_state_label.as_deref(), Some("a"));
        assert_eq!(rows[1].following_state_label.as_deref(), Some("b"));
    }

    /// A slot's colour, not a chip's: the two resolve differently, and the
    /// row reports what the terminal's slot paints. `TRACKED` is a
    /// coloured chip but a colourless slot; `false` — "the field has
    /// ended" — paints nothing at all, which is the one `null`.
    #[test]
    fn edge_colours_are_the_slot_colours() {
        let ctxs = vec![
            RenderContext {
                lead_state: Some(json!("a")),
                trail_state: Some(json!(false)),
                ..RenderContext::default()
            },
            RenderContext {
                lead_state: Some(json!("TRACKED")),
                trail_state: Some(json!("b")),
                ..RenderContext::default()
            },
        ];
        let rows = tail_rows(
            &[
                entry(Verb::Switch, Some(json!(false)), None),
                entry(Verb::Switch, Some(json!("b")), None),
            ],
            Some(&ctxs),
            TimeMetricCol::Hybrid,
            None,
        );
        assert!(rows[0].previous_state_color.is_some());
        assert_eq!(rows[0].following_state, json!(false));
        assert_eq!(rows[0].following_state_color, None);
        // IGNORED and TRACKED both paint the slot grey; neither is absent.
        assert_eq!(rows[1].previous_state_color.as_deref(), Some("#888888"));
        assert!(rows[1].following_state_color.is_some());
    }

    /// The `∅` comes out of the current state (da-fknn8, vividn: "the null
    /// sign shouldn't be attached as the current state at all in the
    /// json"). A point row's body is the dot alone; that the field was
    /// untracked before it is `previous_state`, which is where a renderer
    /// draws the null sign from.
    #[test]
    fn the_null_sign_is_not_part_of_the_body() {
        let rows = tail_rows(
            &[entry(Verb::Point, None, None)],
            Some(&[RenderContext {
                prior_state_ignored: true,
                ..RenderContext::default()
            }]),
            TimeMetricCol::Hybrid,
            None,
        );
        assert_eq!(rows[0].previous_state, json!("IGNORED"));
        assert!(
            !rows[0].state_body.contains('∅'),
            "the body is the dot alone: {:?}",
            rows[0].state_body
        );
        assert!(rows[0].state_body.contains('●'));
    }

    /// `state_body` is the painted cell minus its two slots — so a
    /// consumer that draws the transition itself puts this between what it
    /// draws, and nothing is shown twice.
    #[test]
    fn the_body_is_the_painted_cell_without_its_slots() {
        use crate::render::action::format_state_group;
        let _g = datum_test_util::no_color_scope();
        let e = entry(Verb::Switch, Some(json!("writing")), None);
        let ctx = RenderContext {
            lead_state: Some(json!("reading")),
            trail_state: Some(json!("writing")),
            ..RenderContext::default()
        };
        let rows = tail_rows(
            std::slice::from_ref(&e),
            Some(std::slice::from_ref(&ctx)),
            TimeMetricCol::Hybrid,
            None,
        );
        let group = format_state_group(&e, None, &ctx).unwrap();
        assert_eq!(group, format!(" {} ", rows[0].state_body));
    }

    /// The reassembly invariant on the shapes the verb/dur sweep cannot
    /// reach: a TIMELESS row (no slots at all, the `¢` glyph) and a bare
    /// dot whose field was not tracked before it (the grey `∅` lead).
    #[test]
    fn the_parts_reassemble_the_cell_for_the_slotless_shapes() {
        use crate::render::action::format_state_group;
        let _g = datum_test_util::no_color_scope();
        let mut timeless = entry(Verb::Point, None, None);
        timeless.occur_time = None;
        let ignored_dot = entry(Verb::Point, None, None);
        for (e, ctx, what) in [
            (timeless, RenderContext::default(), "timeless"),
            (
                ignored_dot,
                RenderContext {
                    prior_state_ignored: true,
                    ..RenderContext::default()
                },
                "a dot with no prior state",
            ),
        ] {
            let rows = tail_rows(
                std::slice::from_ref(&e),
                Some(std::slice::from_ref(&ctx)),
                TimeMetricCol::Hybrid,
                None,
            );
            assert_eq!(
                format!(
                    "{}{}{}",
                    rows[0].previous_state_cell, rows[0].state_body, rows[0].following_state_cell
                ),
                format_state_group(&e, None, &ctx).unwrap(),
                "the parts do not reassemble the cell: {what}"
            );
        }
    }

    /// A compound switch carries the state it switches TO, and THAT is
    /// what continues forward past it — in the row AND in the painter's
    /// TRAIL slot (opal, on #74; da-qxmqe).
    ///
    /// Measured before da-qxmqe: `switch activity writing` then `switch
    /// activity end 14m , switch activity researching` stores
    /// `following_state = "researching"`, and the TRAIL painted `writing` —
    /// the state the block INTERRUPTED — because the slot probed the
    /// field's history at the block's far edge EXCLUDING the row itself.
    /// The context is built from that history, not by hand, so the probe
    /// is the one the terminal runs.
    #[test]
    fn a_compound_switch_reports_the_state_it_switches_to() {
        use crate::render::action::StateTransition;
        let mut writing = entry(Verb::Switch, Some(json!("writing")), None);
        writing.occur_time = Some(t_at("2024-03-16T14:15:00Z"));
        for e in [
            compound(
                Some(json!(false)),
                json!("researching"),
                Dur::of(chrono::Duration::minutes(14)),
            ),
            // A zero span is a point, so the TRAIL is not the block
            // probe's but the row's own — the point-switch half of the bug.
            Entry {
                verb: Verb::PointSwitch,
                ..compound(
                    Some(json!(false)),
                    json!("researching"),
                    Dur::of(chrono::Duration::zero()),
                )
            },
        ] {
            let events = FieldStateEvents::from_entries([&writing, &e]);
            let ctx = RenderContext::from_events(Some(&events), &e);
            let what = format!("{} with {:?}", e.verb, e.dur);
            assert_eq!(
                StateTransition::of(&ctx, &e).following,
                Some(json!("researching")),
                "the painter's trail: {what}"
            );
            let rows = tail_rows(
                std::slice::from_ref(&e),
                Some(std::slice::from_ref(&ctx)),
                TimeMetricCol::Hybrid,
                None,
            );
            assert_eq!(rows[0].previous_state, json!("writing"), "{what}");
            assert_eq!(rows[0].following_state, json!("researching"), "{what}");
            assert_eq!(
                rows[0].following_state_label.as_deref(),
                Some("researching"),
                "{what}"
            );
        }
    }

    /// A first log's dot declares its field in its own `following_state`
    /// (da-gvcen): the ROW says `TRACKED` past it, though the dot has no
    /// slot to paint it in and the ambient state around it is untracked.
    #[test]
    fn a_first_log_dot_reports_the_field_it_declares() {
        use crate::render::action::format_state_group;
        let _g = datum_test_util::no_color_scope();
        let e = compound(None, json!("TRACKED"), Dur::moment());
        let ctx = RenderContext::from_events(None, &e);
        let rows = tail_rows(
            std::slice::from_ref(&e),
            Some(std::slice::from_ref(&ctx)),
            TimeMetricCol::Hybrid,
            None,
        );
        assert_eq!(rows[0].previous_state, json!("IGNORED"));
        assert_eq!(rows[0].following_state, json!("TRACKED"));
        assert_eq!(
            format!(
                "{}{}{}",
                rows[0].previous_state_cell, rows[0].state_body, rows[0].following_state_cell
            ),
            format_state_group(&e, None, &ctx).unwrap(),
            "the dot still has no slots to paint it in"
        );
    }

    /// THE MIRROR, for every verb and every duration: what the painter
    /// draws is what the row reports, decomposed.
    ///
    /// `StateTransition::of`, `format_state_body` and the `timeless` gate
    /// above all exist to mirror `format_state_group`'s dispatch, and they
    /// read it from one place (`draws_state_badge`) so it cannot be
    /// half-adjusted. This is what proves the mirror rather than the
    /// routing: a row the painter draws as a DOT while the row reports a
    /// badge — or the other way about — fails here.
    ///
    /// It is also the guard for da-gvcen (#68), which makes the dispatch
    /// `has_state_badge(verb, dur)`. If a rebase feeds the painter
    /// `entry.dur.as_ref()` and these three `None` — which compiles — a
    /// `PointSwitch` carrying `MOMENT` parts company here.
    ///
    /// A row carrying its own `following_state` is not swept here: its
    /// trail comes from the row rather than from the context, which a
    /// hand-built context cannot show — it has a test of its own above,
    /// built from the field's history (da-qxmqe).
    #[test]
    fn the_painter_and_the_row_agree_for_every_verb_and_duration() {
        use crate::render::action::{draws_state_badge, format_state_group};
        let _g = datum_test_util::no_color_scope();
        let ctx = RenderContext {
            lead_state: Some(json!("reading")),
            trail_state: Some(json!("writing")),
            ongoing_state: Some(json!("reading")),
            ..RenderContext::default()
        };
        for verb in [
            Verb::Switch,
            Verb::Block,
            Verb::BlockSwitch,
            Verb::PointSwitch,
            Verb::Point,
        ] {
            for dur in [
                None,
                Some(Dur::moment()),
                Some(Dur::of(chrono::Duration::minutes(30))),
            ] {
                let e = entry(verb, Some(json!("writing")), dur.clone());
                let rows = tail_rows(
                    std::slice::from_ref(&e),
                    Some(std::slice::from_ref(&ctx)),
                    TimeMetricCol::Hybrid,
                    None,
                );
                let painted = format_state_group(&e, None, &ctx).unwrap();
                let what = format!("{verb} with {dur:?}");
                // ⚑ THE INVARIANT THE WEB RESTS ON (vividn: "really mirror
                // the CLI here"): the three painted parts ARE the cell,
                // byte for byte. A consumer that draws lead, body and
                // trail in order has drawn exactly what the terminal drew
                // — so it cannot invent a shape the CLI never showed.
                assert_eq!(
                    format!(
                        "{}{}{}",
                        rows[0].previous_state_cell,
                        rows[0].state_body,
                        rows[0].following_state_cell
                    ),
                    painted,
                    "the parts do not reassemble the cell: {what}"
                );
                if draws_state_badge(&e) {
                    // A badge: the cell is the two slots around the body,
                    // and the row's three values are the three of them.
                    assert_eq!(painted, format!(" {} ", rows[0].state_body), "{what}");
                    assert_eq!(rows[0].previous_state, json!("reading"), "{what}");
                    assert_eq!(rows[0].following_state, json!("writing"), "{what}");
                } else {
                    // A dot: no slots at all, so the body IS the cell, and
                    // the row stands inside one ambient state rather than
                    // between two.
                    assert_eq!(painted, rows[0].state_body, "{what}");
                    assert_eq!(rows[0].previous_state, rows[0].following_state, "{what}");
                }
            }
        }
    }

    /// A switch that left the field's state unchanged is the terminal's
    /// `!` (da-qfv0). The row REPORTS it rather than leaving it to be
    /// re-derived: "the same state" is set-aware, and a consumer
    /// comparing `previous_state` to `state` itself would call
    /// `["a","b"]` → `["b","a"]` a transition when the CLI does not.
    #[test]
    fn a_switch_that_went_nowhere_says_so() {
        use crate::render::action::compute_is_state_repeat;
        let e = entry(Verb::Switch, Some(json!(["a", "b"])), None);
        // The renderer's own rule, on the prior state, is what the row
        // reports — here the same set in the other order, which the CLI
        // calls a repeat and a value comparison would call a transition.
        let ctx = |prior: JsonValue| RenderContext {
            lead_state: Some(prior.clone()),
            is_state_repeat: compute_is_state_repeat(Some(&prior), &e),
            ..RenderContext::default()
        };
        let row = |prior: JsonValue| {
            tail_rows(
                std::slice::from_ref(&e),
                Some(std::slice::from_ref(&ctx(prior))),
                TimeMetricCol::Hybrid,
                None,
            )
            .remove(0)
        };
        let reordered = row(json!(["b", "a"]));
        assert!(reordered.state_repeat, "a reordered set is the same state");
        assert_eq!(reordered.previous_state, json!(["b", "a"]));
        // …and adding a member to the set is a real transition (da-g1b8),
        // which a consumer must not draw the `!` on.
        assert!(!row(json!(["a"])).state_repeat);
    }

    /// A timeless row sits at no instant, so it has no state before it and
    /// none after — the terminal gives it `¢` and no slots. Reporting an
    /// IGNORED previous would have a renderer draw a null sign the
    /// terminal does not.
    #[test]
    fn a_timeless_row_has_no_transition() {
        let mut e = entry(Verb::Point, None, None);
        e.occur_time = None;
        let rows = tail_rows(
            std::slice::from_ref(&e),
            Some(&[RenderContext {
                prior_state_ignored: true,
                ..RenderContext::default()
            }]),
            TimeMetricCol::Hybrid,
            None,
        );
        assert_eq!(rows[0].previous_state, JsonValue::Null);
        assert_eq!(rows[0].previous_state_label, None);
        assert_eq!(rows[0].previous_state_color, None);
        assert_eq!(rows[0].following_state, JsonValue::Null);
    }

    /// `_n` is a handle relative to now, and it must be the same handle
    /// the table would print — both read it off the row's context.
    #[test]
    fn quick_id_comes_from_the_row_context() {
        let entries = vec![
            entry(Verb::Switch, Some(json!("a")), None),
            entry(Verb::Switch, Some(json!("b")), None),
        ];
        let ctxs = vec![
            RenderContext {
                quick_ordinal: Some(2),
                ..RenderContext::default()
            },
            RenderContext::default(),
        ];
        let rows = tail_rows(&entries, Some(&ctxs), TimeMetricCol::Hybrid, None);
        assert_eq!(rows[0].quick_id.as_deref(), Some("_2"));
        // Past the ordinal cap there is no shorthand — `null`, not a
        // guess a consumer could hand back to the CLI.
        assert_eq!(rows[1].quick_id, None);
    }
}
