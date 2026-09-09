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
use serde::Serialize;
use serde_json::Value as JsonValue;

use super::state_truncate::state_label;
use super::{RenderContext, TimeMetricCol};

/// The `--json` document. The rows sit under a key rather than at the top
/// level so anything later worth reporting alongside them — a count, the
/// resolved window — can be added without breaking the readers this flag
/// exists to protect.
#[derive(Debug, Serialize)]
pub struct TailDocument {
    pub rows: Vec<TailRow>,
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
    /// The state value as stored — a string, bool, number, object, array,
    /// or `null`.
    pub state: JsonValue,
    /// The plain text the table's state cell shows for `state`: `true` and
    /// `false` read as `start` and `end`, a structured state as `{id}`.
    /// `null` when the row carries no state at all.
    pub state_label: Option<String>,
    /// Trailing-edge state of a `block-switch` / `point-switch`; `null`
    /// otherwise.
    pub following_state: JsonValue,
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
) -> Vec<TailRow> {
    entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let (time, time_metric) = row_time(e, metric);
            TailRow {
                hid: e.hid.to_string(),
                slug: e.slug.to_string(),
                field: e.field.clone(),
                verb: e.verb.to_string(),
                time,
                time_metric,
                time_uncertainty: e.occur_uncertainty().map(str::to_string),
                state: e.state.clone().unwrap_or(JsonValue::Null),
                state_label: e.state.as_ref().map(state_label),
                following_state: e.following_state.clone().unwrap_or(JsonValue::Null),
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
    use datum_core::time::DatumTime;
    use serde_json::json;

    fn t_at(iso: &str) -> DatumTime {
        let utc: DateTime<Utc> = iso.parse().unwrap();
        DatumTime::from_utc_and_tz(utc, "America/Chicago").unwrap()
    }

    fn entry(verb: Verb, state: Option<serde_json::Value>, dur: Option<Dur>) -> Entry {
        Entry::new("activity", verb, state, None, Some(t_at(OCCUR)), dur, "d1")
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
                "hid",
                "modify_time",
                "quick_id",
                "slug",
                "state",
                "state_label",
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
        let occur = &tail_rows(std::slice::from_ref(&e), None, TimeMetricCol::Hybrid)[0];
        assert_eq!(occur.time.as_deref(), Some("2024-03-16T10:15:00-05:00"));
        assert_eq!(occur.time_metric, Some("occur"));

        let modify = &tail_rows(std::slice::from_ref(&e), None, TimeMetricCol::Modify)[0];
        assert_eq!(modify.time_metric, Some("modify"));
        assert_eq!(modify.time.as_deref(), Some(&*e.modify_time.to_rfc3339()));

        let mut timeless = e.clone();
        timeless.occur_time = None;
        let hybrid = &tail_rows(std::slice::from_ref(&timeless), None, TimeMetricCol::Hybrid)[0];
        assert_eq!(hybrid.time_metric, Some("create"));
        assert_eq!(hybrid.time.as_deref(), Some(&*e.create_time.to_rfc3339()));

        let occur_only = &tail_rows(std::slice::from_ref(&timeless), None, TimeMetricCol::Occur)[0];
        assert_eq!(occur_only.time, None);
        assert_eq!(occur_only.time_metric, None);
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
        let rows = tail_rows(&entries, Some(&ctxs), TimeMetricCol::Hybrid);
        assert_eq!(rows[0].quick_id.as_deref(), Some("_2"));
        // Past the ordinal cap there is no shorthand — `null`, not a
        // guess a consumer could hand back to the CLI.
        assert_eq!(rows[1].quick_id, None);
    }
}
