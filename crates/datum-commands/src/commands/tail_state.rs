//! `datum tail field:state` — block-mode rendering (da-plbb).
//!
//! When the tail positional carries a `:state` suffix, tail abandons the
//! per-entry table and emits one row per *block* — a span during which the
//! field's compiled state equalled the filter. Each block is bounded by
//! two state transitions on the field (or by `now`, for a still-active
//! block).

use anyhow::Result;
use chrono::{DateTime, Utc};
use datum_core::{
    db::Db,
    entry::{Entry, Verb},
    state::{normalize_state, StateValue},
    time::DatumTime,
};
use serde_json::Value as JsonValue;

use crate::render::action::{format_duration, format_timestamp};

/// Translate the parser's raw `state_filter` strings (`Vec<String>` from
/// splitting on `,`) into a canonical JSON value, ready for structural
/// comparison against compiled states. Single-element specs collapse to a
/// scalar; multi-element specs become a parallel-state array.
pub fn build_filter(raw: &[String]) -> JsonValue {
    let initial = if raw.len() == 1 {
        JsonValue::String(raw[0].clone())
    } else {
        JsonValue::Array(raw.iter().cloned().map(JsonValue::String).collect())
    };
    normalize_state(&initial).to_json()
}

/// One block of contiguous matching state.
pub struct StateBlock {
    pub state: JsonValue,
    pub start: DatumTime,
    /// `None` means the block is still active as of `now`.
    pub end: Option<DatumTime>,
    pub opener: Entry,
    pub closer: Option<Entry>,
}

/// Render block rows for `tail field:state`. The orchestration matches
/// [`crate::commands::tail::tail`]: pull events, fold into blocks, filter,
/// apply `-n`, render.
pub fn tail_state(
    field: &str,
    raw_state: &[String],
    count: Option<usize>,
    oldest_first: bool,
    db: &Db,
) -> Result<()> {
    let filter = build_filter(raw_state);

    let entries = db.entries_for_field_asc(field, None)?;
    let blocks = compute_blocks(&entries, &filter);

    // Default to 10 most-recent matching blocks (parity with the entry-mode
    // tail default). `-n 0` (and any explicit value) wins.
    let n = count.unwrap_or(10);
    let mut to_show: Vec<&StateBlock> = blocks.iter().rev().take(n).collect();
    if oldest_first {
        // already newest-first → reverse for oldest-first display
        to_show.reverse();
    } else {
        // We took the last N newest-first; flip so output reads oldest-to-newest
        // (matching how entry-mode tail prints — top row is oldest of the N).
        to_show.reverse();
    }

    print_blocks(&to_show);
    Ok(())
}

/// Walk state-bearing entries oldest-first, accumulating contiguous blocks
/// of equal normalized state. A block whose state matches `filter` is kept;
/// everything else is dropped. The final block stays open (`end = None`)
/// when the field's most recent state equals the filter — that's the
/// "ongoing" case the renderer needs to display.
pub fn compute_blocks(entries: &[Entry], filter: &JsonValue) -> Vec<StateBlock> {
    let mut events: Vec<(DateTime<Utc>, &Entry, JsonValue)> = entries
        .iter()
        .filter_map(|e| {
            let ts = e.occur_time.as_ref()?.utc;
            // Mirror state::collect_raw_events: each state-affecting verb
            // produces exactly one transition event at occur_time. Note
            // `:unset` rows are excluded by the loader's slug filter
            // already (entries_for_field_asc has no such filter, so guard
            // here too).
            if e.slug.as_str().ends_with(":unset") {
                return None;
            }
            let state = event_state(e)?;
            Some((ts, e, state))
        })
        .collect();
    // Stable secondary sort by create_time so same-instant events fold
    // into a deterministic order (matches FieldStateEvents).
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.create_time.cmp(&b.1.create_time)));

    let mut blocks: Vec<StateBlock> = Vec::new();
    let mut current: Option<StateBlock> = None;

    for (_t, entry, state) in events {
        match current.as_mut() {
            Some(cur) if cur.state == state => {
                // No transition; the block continues. Keep the original
                // opener so the displayed `hid` always points at the
                // entry that *introduced* this state, not subsequent
                // confirmations.
                continue;
            }
            Some(_) => {
                let mut prev = current.take().unwrap();
                prev.end = entry.occur_time.clone();
                prev.closer = Some(entry.clone());
                if &prev.state == filter {
                    blocks.push(prev);
                }
                current = Some(StateBlock {
                    state,
                    start: entry.occur_time.clone().expect("event has occur_time"),
                    end: None,
                    opener: entry.clone(),
                    closer: None,
                });
            }
            None => {
                current = Some(StateBlock {
                    state,
                    start: entry.occur_time.clone().expect("event has occur_time"),
                    end: None,
                    opener: entry.clone(),
                    closer: None,
                });
            }
        }
    }
    if let Some(cur) = current {
        if &cur.state == filter {
            blocks.push(cur);
        }
    }
    blocks
}

/// What state does this entry assert? Mirrors `state::collect_raw_events`
/// without going through the FieldStateEvents loader (which discards the
/// originating `Entry`).
fn event_state(e: &Entry) -> Option<JsonValue> {
    match e.verb {
        Verb::Switch | Verb::BlockSwitch | Verb::PointSwitch => Some(
            e.state
                .as_ref()
                .map(|v| normalize_state(v).to_json())
                .unwrap_or(JsonValue::Null),
        ),
        Verb::Point => Some(JsonValue::Bool(false)),
        Verb::Block => Some(
            e.state
                .as_ref()
                .map(|v| normalize_state(v).to_json())
                .unwrap_or(JsonValue::Bool(true)),
        ),
    }
}

fn print_blocks(blocks: &[&StateBlock]) {
    if blocks.is_empty() {
        println!("[No data]");
        return;
    }

    let now = DatumTime::now();

    struct Row {
        start: String,
        end: String,
        dur: String,
        state: String,
        hid: String,
    }

    let mut rows: Vec<Row> = Vec::with_capacity(blocks.len());
    for b in blocks {
        let start = format_timestamp(&b.start, &now, None);
        let (end, dur_secs) = match &b.end {
            Some(end) => (
                format_timestamp(end, &now, None),
                (end.utc - b.start.utc).num_seconds(),
            ),
            None => ("ongoing".to_string(), (now.utc - b.start.utc).num_seconds()),
        };
        let dur = format_duration(dur_secs);
        let state = StateValue::from_json(&b.state)
            .map(|sv| sv.to_string())
            .unwrap_or_default();
        let hid = b.opener.hid.0.clone();
        rows.push(Row {
            start,
            end,
            dur,
            state,
            hid,
        });
    }

    let vw = crate::render::colors::visible_width;
    let start_w = std::iter::once("start".len())
        .chain(rows.iter().map(|r| vw(&r.start)))
        .max()
        .unwrap_or(0);
    let end_w = std::iter::once("end".len())
        .chain(rows.iter().map(|r| vw(&r.end)))
        .max()
        .unwrap_or(0);
    let dur_w = std::iter::once("dur".len())
        .chain(rows.iter().map(|r| vw(&r.dur)))
        .max()
        .unwrap_or(0);
    let state_w = std::iter::once("state".len())
        .chain(rows.iter().map(|r| vw(&r.state)))
        .max()
        .unwrap_or(0);
    let hid_w = std::iter::once("hid".len())
        .chain(rows.iter().map(|r| vw(&r.hid)))
        .max()
        .unwrap_or(0);

    let push_cell = |buf: &mut String, body: &str, width: usize, right: bool| {
        let w = vw(body);
        let pad = width.saturating_sub(w);
        if right {
            buf.push_str(&" ".repeat(pad));
            buf.push_str(body);
        } else {
            buf.push_str(body);
            buf.push_str(&" ".repeat(pad));
        }
        buf.push_str("  ");
    };

    // Header
    let mut header = String::new();
    push_cell(&mut header, "start", start_w, true);
    push_cell(&mut header, "end", end_w, false);
    push_cell(&mut header, "dur", dur_w, false);
    push_cell(&mut header, "state", state_w, false);
    push_cell(&mut header, "hid", hid_w, false);
    println!("{}", header.trim_end());

    for r in &rows {
        let mut line = String::new();
        push_cell(&mut line, &r.start, start_w, true);
        push_cell(&mut line, &r.end, end_w, false);
        push_cell(&mut line, &r.dur, dur_w, false);
        push_cell(&mut line, &r.state, state_w, false);
        push_cell(&mut line, &r.hid, hid_w, false);
        println!("{}", line.trim_end());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use datum_core::test_util::{entry_builder, fresh_db};
    use serde_json::json;

    fn t(mins: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 21, 8, 0, 0).unwrap() + Duration::minutes(mins)
    }

    #[test]
    fn build_filter_single_string_normalizes_to_simple() {
        let f = build_filter(&["happy".to_string()]);
        assert_eq!(f, json!("happy"));
    }

    #[test]
    fn build_filter_array_normalizes_to_parallel() {
        let f = build_filter(&["vertical".to_string(), "sit".to_string()]);
        assert_eq!(f, json!(["vertical", "sit"]));
    }

    #[test]
    fn blocks_emits_one_per_matching_run() {
        let db = fresh_db();
        // happy 0–30, sad 30–60, happy 60–90, sad 90–end
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("happy"))
            .time_start_utc(t(0))
            .insert(&db);
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("sad"))
            .time_start_utc(t(30))
            .insert(&db);
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("happy"))
            .time_start_utc(t(60))
            .insert(&db);
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("sad"))
            .time_start_utc(t(90))
            .insert(&db);

        let entries = db.entries_for_field_asc("mood", None).unwrap();
        let blocks = compute_blocks(&entries, &json!("happy"));
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].start.utc, t(0));
        assert_eq!(blocks[0].end.as_ref().map(|e| e.utc), Some(t(30)));
        assert_eq!(blocks[1].start.utc, t(60));
        assert_eq!(blocks[1].end.as_ref().map(|e| e.utc), Some(t(90)));
    }

    #[test]
    fn blocks_leaves_final_open_when_matching_is_current_state() {
        let db = fresh_db();
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("happy"))
            .time_start_utc(t(0))
            .insert(&db);

        let entries = db.entries_for_field_asc("mood", None).unwrap();
        let blocks = compute_blocks(&entries, &json!("happy"));
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].end.is_none(), "ongoing block has no end");
    }

    #[test]
    fn blocks_drops_non_matching_runs() {
        let db = fresh_db();
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("sad"))
            .time_start_utc(t(0))
            .insert(&db);

        let entries = db.entries_for_field_asc("mood", None).unwrap();
        let blocks = compute_blocks(&entries, &json!("happy"));
        assert!(blocks.is_empty());
    }

    #[test]
    fn blocks_treat_repeated_state_as_one_block() {
        let db = fresh_db();
        // Two `switch happy` in a row → one block, opener is the first.
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("happy"))
            .time_start_utc(t(0))
            .insert(&db);
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("happy"))
            .time_start_utc(t(30))
            .insert(&db);
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(json!("sad"))
            .time_start_utc(t(60))
            .insert(&db);

        let entries = db.entries_for_field_asc("mood", None).unwrap();
        let blocks = compute_blocks(&entries, &json!("happy"));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start.utc, t(0));
        assert_eq!(blocks[0].end.as_ref().map(|e| e.utc), Some(t(60)));
    }

    #[test]
    fn blocks_match_parallel_state_exactly() {
        let db = fresh_db();
        entry_builder("body")
            .verb(Verb::Switch)
            .value(json!(["vertical", "sit"]))
            .time_start_utc(t(0))
            .insert(&db);
        entry_builder("body")
            .verb(Verb::Switch)
            .value(json!("vertical"))
            .time_start_utc(t(30))
            .insert(&db);

        let entries = db.entries_for_field_asc("body", None).unwrap();
        // The parallel-state block matches, the bare "vertical" does not.
        let blocks = compute_blocks(&entries, &json!(["vertical", "sit"]));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start.utc, t(0));

        // Conversely, bare "vertical" filter matches only the bare-state run.
        let blocks2 = compute_blocks(&entries, &json!("vertical"));
        assert_eq!(blocks2.len(), 1);
        assert_eq!(blocks2[0].start.utc, t(30));
    }
}
