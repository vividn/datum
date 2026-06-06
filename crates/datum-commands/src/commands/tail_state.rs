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
    entry::Entry,
    state::{entry_state_events, normalize_state, StateValue},
    time::DatumTime,
};
use serde_json::Value as JsonValue;

use crate::render::action::{compress_offset_minutes, format_duration, format_timestamp_ref};

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
    print_blocks(&select_blocks(&blocks, n, oldest_first));
    Ok(())
}

/// Pick which blocks to display. `oldest_first` (head) selects the oldest
/// `n` blocks; tail selects the newest `n`. Either way the output reads
/// oldest-to-newest (matching how entry-mode tail prints — top row is the
/// oldest of the N).
pub fn select_blocks(blocks: &[StateBlock], n: usize, oldest_first: bool) -> Vec<&StateBlock> {
    if oldest_first {
        // `compute_blocks` yields oldest-first already.
        blocks.iter().take(n).collect()
    } else {
        let mut newest: Vec<&StateBlock> = blocks.iter().rev().take(n).collect();
        newest.reverse();
        newest
    }
}

/// Walk state-bearing entries oldest-first, accumulating contiguous blocks
/// of equal normalized state. A block whose state matches `filter` is kept;
/// everything else is dropped. The final block stays open (`end = None`)
/// when the field's most recent state equals the filter — that's the
/// "ongoing" case the renderer needs to display.
pub fn compute_blocks(entries: &[Entry], filter: &JsonValue) -> Vec<StateBlock> {
    // The canonical event extractor (`state::entry_state_events`, da-eybr)
    // emits BOTH the primary event at occur_time and — for block-switch /
    // point-switch — the trailing `following_state` event at occur_time +
    // dur. A trailing event's originating entry is the block-switch row
    // itself: it both closes the primary block and opens the follower, so
    // opener/closer refs point at that row.
    let mut events: Vec<(DateTime<Utc>, &Entry, JsonValue)> = entries
        .iter()
        .flat_map(|e| {
            entry_state_events(e)
                .into_iter()
                .map(move |(ts, state)| (ts, e, state))
        })
        .collect();
    // Stable secondary sort by create_time so same-instant events fold
    // into a deterministic order (matches FieldStateEvents). Stability
    // also keeps a point-switch's trailing event after its primary.
    events.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.create_time.cmp(&b.1.create_time)));

    let mut blocks: Vec<StateBlock> = Vec::new();
    let mut current: Option<StateBlock> = None;

    for (t, entry, state) in events {
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
                prev.end = Some(event_time(entry, t));
                prev.closer = Some(entry.clone());
                if &prev.state == filter {
                    blocks.push(prev);
                }
                current = Some(StateBlock {
                    state,
                    start: event_time(entry, t),
                    end: None,
                    opener: entry.clone(),
                    closer: None,
                });
            }
            None => {
                current = Some(StateBlock {
                    state,
                    start: event_time(entry, t),
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

/// Lift an event timestamp back into the originating entry's timezone.
/// Primary events land exactly on `occur_time`; trailing
/// `following_state` events land at occur_time + dur, so re-derive the
/// offset at that instant (DST-safe), keeping the entry's offset if the
/// timezone name fails to parse.
fn event_time(entry: &Entry, ts: DateTime<Utc>) -> DatumTime {
    let occ = entry
        .occur_time
        .as_ref()
        .expect("state event implies occur_time");
    if ts == occ.utc {
        occ.clone()
    } else {
        DatumTime::from_utc_and_tz(ts, &occ.timezone).unwrap_or_else(|_| DatumTime {
            utc: ts,
            ..occ.clone()
        })
    }
}

fn print_blocks(blocks: &[&StateBlock]) {
    if blocks.is_empty() {
        println!("[No data]");
        return;
    }

    let now = DatumTime::now();

    // da-lkji: same header-reference treatment as the entry table — the
    // most common UTC offset across all displayed start/end stamps (tie →
    // the newest such stamp) prints once in the headers; only differing
    // rows carry a gray `(±off)` prefix.
    let mut votes: std::collections::HashMap<i32, (usize, DateTime<Utc>)> =
        std::collections::HashMap::new();
    for t in blocks
        .iter()
        .flat_map(|b| std::iter::once(&b.start).chain(b.end.as_ref()))
    {
        let slot = votes.entry(t.offset_minutes).or_insert((0, t.utc));
        slot.0 += 1;
        slot.1 = slot.1.max(t.utc);
    }
    let reference = votes
        .into_iter()
        .max_by_key(|&(_, (count, newest))| (count, newest))
        .map(|(off, _)| off);

    struct Row {
        start: String,
        end: String,
        dur: String,
        state: String,
        hid: String,
    }

    let mut rows: Vec<Row> = Vec::with_capacity(blocks.len());
    for b in blocks {
        let start = format_timestamp_ref(&b.start, &now, None, reference);
        let (end, dur_secs) = match &b.end {
            Some(end) => (
                format_timestamp_ref(end, &now, None, reference),
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
    // Each time column states the shared reference offset once (da-lkji).
    let annotate = |label: &str| match reference {
        Some(r) => format!(
            "{label} {}",
            crate::render::colors::paint(
                crate::render::colors::GRAY_FG,
                &format!("({})", compress_offset_minutes(r))
            )
        ),
        None => label.to_string(),
    };
    let start_header = annotate("start");
    let end_header = annotate("end");
    let start_w = std::iter::once(vw(&start_header))
        .chain(rows.iter().map(|r| vw(&r.start)))
        .max()
        .unwrap_or(0);
    let end_w = std::iter::once(vw(&end_header))
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
    push_cell(&mut header, &start_header, start_w, true);
    push_cell(&mut header, &end_header, end_w, false);
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
    use datum_core::entry::Verb;
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
            .state(json!("happy"))
            .time_start_utc(t(0))
            .insert(&db);
        entry_builder("mood")
            .verb(Verb::Switch)
            .state(json!("sad"))
            .time_start_utc(t(30))
            .insert(&db);
        entry_builder("mood")
            .verb(Verb::Switch)
            .state(json!("happy"))
            .time_start_utc(t(60))
            .insert(&db);
        entry_builder("mood")
            .verb(Verb::Switch)
            .state(json!("sad"))
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
            .state(json!("happy"))
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
            .state(json!("sad"))
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
            .state(json!("happy"))
            .time_start_utc(t(0))
            .insert(&db);
        entry_builder("mood")
            .verb(Verb::Switch)
            .state(json!("happy"))
            .time_start_utc(t(30))
            .insert(&db);
        entry_builder("mood")
            .verb(Verb::Switch)
            .state(json!("sad"))
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
            .state(json!(["vertical", "sit"]))
            .time_start_utc(t(0))
            .insert(&db);
        entry_builder("body")
            .verb(Verb::Switch)
            .state(json!("vertical"))
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

    /// A block-switch row carrying dur + following_state (the builder
    /// doesn't expose those, so construct directly — same shape as the
    /// state.rs da-2i8g fixtures).
    fn block_switch(
        field: &str,
        occur: DateTime<Utc>,
        primary: &str,
        dur_mins: i64,
        following: &str,
    ) -> Entry {
        use datum_core::dur::Dur;
        let mut e = Entry::new(
            field,
            Verb::BlockSwitch,
            Some(json!(primary)),
            None,
            Some(DatumTime {
                utc: occur,
                offset_minutes: 0,
                timezone: "UTC".into(),
            }),
            Some(Dur::Of(Duration::minutes(dur_mins))),
            "dev",
        );
        e.following_state = Some(json!(following));
        e
    }

    #[test]
    fn blocks_block_switch_opens_follower_at_occur_plus_dur() {
        // The vremya pattern from da-eybr: `switch activity end 14m , researching`
        // then a plain switch later. Canonically 'end' runs 0–14 and
        // 'researching' 14–60; pre-fix, block mode dropped the trailing
        // event so 'end' ran to t(60) and 'researching' never existed.
        let bs = block_switch("activity", t(0), "end", 14, "researching");
        let mut lunch = entry_builder("activity")
            .verb(Verb::Switch)
            .state(json!("lunch"))
            .time_start_utc(t(60))
            .build();
        lunch.create_time = bs.create_time + Duration::seconds(1);
        let entries = vec![bs.clone(), lunch];

        // "end" is a normalize_state keyword (→ false); go through
        // build_filter exactly like the real command does.
        let end_blocks = compute_blocks(&entries, &build_filter(&["end".to_string()]));
        assert_eq!(end_blocks.len(), 1);
        assert_eq!(end_blocks[0].start.utc, t(0));
        assert_eq!(
            end_blocks[0].end.as_ref().map(|e| e.utc),
            Some(t(14)),
            "primary block closes at occur+dur, not at the next entry"
        );

        let follower = compute_blocks(&entries, &json!("researching"));
        assert_eq!(follower.len(), 1, "follower block exists");
        assert_eq!(follower[0].start.utc, t(14), "follower opens at occur+dur");
        assert_eq!(follower[0].end.as_ref().map(|e| e.utc), Some(t(60)));
        // Both opener and the primary's closer are the block-switch row itself.
        assert_eq!(follower[0].opener.hid, bs.hid);
        assert_eq!(end_blocks[0].closer.as_ref().map(|c| &c.hid), Some(&bs.hid));
    }

    #[test]
    fn head_selects_oldest_blocks_tail_selects_newest() {
        let db = fresh_db();
        // Three happy runs: 0–10, 20–30, 40–50.
        for (start, gap_state) in [(0, 10), (20, 30), (40, 50)] {
            entry_builder("mood")
                .verb(Verb::Switch)
                .state(json!("happy"))
                .time_start_utc(t(start))
                .insert(&db);
            entry_builder("mood")
                .verb(Verb::Switch)
                .state(json!("sad"))
                .time_start_utc(t(gap_state))
                .insert(&db);
        }

        let entries = db.entries_for_field_asc("mood", None).unwrap();
        let blocks = compute_blocks(&entries, &json!("happy"));
        assert_eq!(blocks.len(), 3);

        // Both render oldest-to-newest, but head takes the oldest N and
        // tail the newest N.
        let head = select_blocks(&blocks, 2, true);
        assert_eq!(head.len(), 2);
        assert_eq!(head[0].start.utc, t(0));
        assert_eq!(head[1].start.utc, t(20));

        let tail = select_blocks(&blocks, 2, false);
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].start.utc, t(20));
        assert_eq!(tail[1].start.utc, t(40));
    }
}
