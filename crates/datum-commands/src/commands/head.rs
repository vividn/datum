//! `datum head` — list the oldest entries.
//!
//! Thin dispatch wrapper: forces `--oldest-first` and routes through
//! `cmd_tail` so head and tail share one rendering / filtering codepath.

use anyhow::Result;
use datum_core::db::Db;

use crate::args::TailArgs;

pub fn head(mut args: TailArgs, db: &Db) -> Result<()> {
    args.oldest_first = true;
    crate::commands::tail::tail(args, db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tail::tail_entries;
    use datum_core::entry::Verb;

    /// Test-only helper: build a `Point` entry at a deterministic offset from
    /// a fixed base. Duplicated rather than imported because head's tests are
    /// self-contained and the cmd.rs sibling stays in use by other clusters
    /// (undo, last_docs) until those move out as well (da-iwcd.1 cluster #4).
    fn make_entry_at(field: &str, secs_offset: i64, device: &str) -> datum_core::entry::Entry {
        use datum_core::{entry::Entry, time::DatumTime};
        let base: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let utc = base + chrono::Duration::seconds(secs_offset);
        let t = DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        Entry::new_legacy(
            field,
            Verb::Point,
            Some(serde_json::json!(secs_offset)),
            None,
            Some(t),
            None,
            device,
        )
    }

    /// Build a `TailArgs` in head-mode (oldest-first) with the given field and count.
    /// Used by head regression tests to exercise the shared tail codepath.
    fn head_args(field: Option<&str>, count: Option<usize>) -> TailArgs {
        TailArgs {
            fields: field.into_iter().map(String::from).collect(),
            count,
            oldest_first: true,
            ..TailArgs::default()
        }
    }

    #[test]
    fn head_returns_oldest_first() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // Insert out-of-order: newest first, then oldest, then middle.
        db.insert_entry(&make_entry_at("mood", 300, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();

        crate::commands::tail::tail(head_args(None, Some(10)), &db).unwrap();

        // Verify via tail_entries that ordering is ASC by time_start (head mode).
        let entries = tail_entries(&head_args(None, Some(10)), &db).unwrap();
        let values: Vec<i64> = entries
            .iter()
            .map(|e| e.state.as_ref().unwrap().as_i64().unwrap())
            .collect();
        assert_eq!(values, vec![100, 200, 300]);
    }

    #[test]
    fn head_respects_limit() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        for i in 0..20 {
            db.insert_entry(&make_entry_at("mood", i * 60, "d1"))
                .unwrap();
        }

        crate::commands::tail::tail(head_args(Some("mood"), Some(5)), &db).unwrap();

        let got = tail_entries(&head_args(Some("mood"), Some(5)), &db).unwrap();
        assert_eq!(got.len(), 5);
        let values: Vec<i64> = got
            .iter()
            .map(|e| e.state.as_ref().unwrap().as_i64().unwrap())
            .collect();
        assert_eq!(values, vec![0, 60, 120, 180, 240]);
    }

    #[test]
    fn head_filters_by_field() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&make_entry_at("coffee", 100, "d1"))
            .unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();
        db.insert_entry(&make_entry_at("coffee", 300, "d1"))
            .unwrap();
        db.insert_entry(&make_entry_at("mood", 400, "d1")).unwrap();

        crate::commands::tail::tail(head_args(Some("coffee"), Some(10)), &db).unwrap();

        let got = tail_entries(&head_args(Some("coffee"), Some(10)), &db).unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|e| e.field == "coffee"));
        let values: Vec<i64> = got
            .iter()
            .map(|e| e.state.as_ref().unwrap().as_i64().unwrap())
            .collect();
        assert_eq!(values, vec![100, 300]);
    }

    #[test]
    fn head_default_limit_is_10() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        for i in 0..15 {
            db.insert_entry(&make_entry_at("mood", i * 60, "d1"))
                .unwrap();
        }
        // tail_params defaults count to 10 when no date window is set — same
        // default the old head command hardcoded.
        let got = tail_entries(&head_args(Some("mood"), None), &db).unwrap();
        assert_eq!(got.len(), 10);
        // First entry should be the oldest (offset=0).
        assert_eq!(got[0].state.as_ref().unwrap().as_i64().unwrap(), 0);
    }

    #[test]
    fn head_returns_all_when_fewer_than_limit() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();

        crate::commands::tail::tail(head_args(None, Some(10)), &db).unwrap();

        let got = tail_entries(&head_args(None, Some(10)), &db).unwrap();
        assert_eq!(got.len(), 2);
    }
}
