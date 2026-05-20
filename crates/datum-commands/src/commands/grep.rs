//! `datum grep <pattern>` — regex search over entries.

use anyhow::{Context, Result};
use datum_core::{db::Db, entry::Entry, time::DatumTime};
use regex_lite::Regex;

use crate::{
    args::{GrepArgs, OutputFlagsExt},
    cmd::{build_render_contexts, load_registry_opt},
    render::{print_table_with, TableArgs, TimeMetricCol},
};

pub fn grep(args: GrepArgs, db: &Db) -> Result<()> {
    let regexes: Vec<Regex> = args
        .patterns
        .iter()
        .map(|p| Regex::new(p).with_context(|| format!("Invalid regex pattern: {p}")))
        .collect::<Result<_>>()?;

    let candidates = if let Some(field) = &args.field {
        db.entries_for_field(field, None)?
    } else {
        let far_future = chrono::Utc::now() + chrono::Duration::days(365);
        let far_past = chrono::DateTime::UNIX_EPOCH;
        db.entries_in_range(far_past, far_future)?
    };

    let matches: Vec<Entry> = candidates
        .into_iter()
        .filter(|entry| {
            let json = match serde_json::to_string(entry) {
                Ok(s) => s,
                Err(_) => return false,
            };
            regexes.iter().all(|r| r.is_match(&json))
        })
        .collect();

    let registry = load_registry_opt(db);
    let tbl = TableArgs {
        output: args.output(),
        metric: TimeMetricCol::Hybrid,
        columns: vec![],
        suppress_header: false,
    };
    let now = DatumTime::now();
    let ctxs = build_render_contexts(&matches, db);
    print_table_with(&matches, Some(&ctxs), Some(&now), registry.as_ref(), &tbl);
    Ok(())
}

#[cfg(test)]
mod tests {
    use datum_core::entry::{Entry, Verb};
    use datum_core::time::DatumTime;
    use regex_lite::Regex;

    use super::*;

    fn make_grep_entry(
        field: &str,
        secs_offset: i64,
        value: serde_json::Value,
        extra: Option<serde_json::Value>,
    ) -> Entry {
        let base: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
        let utc = base + chrono::Duration::seconds(secs_offset);
        let t = DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        Entry::new_legacy(field, Verb::Point, Some(value), extra, Some(t), None, "d1")
    }

    fn run_grep(db: &datum_core::db::Db, patterns: &[&str], field: Option<&str>) -> Vec<Entry> {
        let regexes: Vec<Regex> = patterns.iter().map(|p| Regex::new(p).unwrap()).collect();
        let candidates = if let Some(f) = field {
            db.entries_for_field(f, None).unwrap()
        } else {
            let far_future = chrono::Utc::now() + chrono::Duration::days(365);
            let far_past = chrono::DateTime::UNIX_EPOCH;
            db.entries_in_range(far_past, far_future).unwrap()
        };
        candidates
            .into_iter()
            .filter(|e| {
                let j = serde_json::to_string(e).unwrap();
                regexes.iter().all(|r| r.is_match(&j))
            })
            .collect()
    }

    #[test]
    fn grep_single_pattern_regex() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&make_grep_entry(
            "mood",
            100,
            serde_json::json!("happy"),
            None,
        ))
        .unwrap();
        db.insert_entry(&make_grep_entry(
            "mood",
            200,
            serde_json::json!("tired"),
            None,
        ))
        .unwrap();
        db.insert_entry(&make_grep_entry("coffee", 300, serde_json::json!(2), None))
            .unwrap();

        // Literal string match
        let got = run_grep(&db, &["happy"], None);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].value.as_ref().unwrap(), &serde_json::json!("happy"));

        // Regex alternation
        let got = run_grep(&db, &["happ|tire"], None);
        assert_eq!(got.len(), 2);

        // Regex anchoring — "^" applies to the serialized JSON start
        let got = run_grep(&db, &["nothing_here"], None);
        assert!(got.is_empty());

        // Exercise the command dispatch path for completeness
        grep(
            GrepArgs {
                patterns: vec!["happy".to_string()],
                field: None,
                output: Default::default(),
            },
            &db,
        )
        .unwrap();
    }

    #[test]
    fn grep_multi_pattern_and() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // Entry with both "foo" and "bar" in extra_data
        db.insert_entry(&make_grep_entry(
            "notes",
            100,
            serde_json::json!("hello"),
            Some(serde_json::json!({"tag": "foo", "project": "bar"})),
        ))
        .unwrap();
        // Entry with only "foo"
        db.insert_entry(&make_grep_entry(
            "notes",
            200,
            serde_json::json!("world"),
            Some(serde_json::json!({"tag": "foo"})),
        ))
        .unwrap();
        // Entry with only "bar"
        db.insert_entry(&make_grep_entry(
            "notes",
            300,
            serde_json::json!("world"),
            Some(serde_json::json!({"project": "bar"})),
        ))
        .unwrap();

        let got = run_grep(&db, &["foo", "bar"], None);
        assert_eq!(got.len(), 1);
        let ex = got[0].data.as_ref().unwrap();
        assert_eq!(ex["tag"], serde_json::json!("foo"));
        assert_eq!(ex["project"], serde_json::json!("bar"));

        // Three patterns, only first entry has all
        let got = run_grep(&db, &["foo", "bar", "hello"], None);
        assert_eq!(got.len(), 1);

        // Unmatched combination returns none
        let got = run_grep(&db, &["foo", "bar", "missing"], None);
        assert!(got.is_empty());
    }

    #[test]
    fn grep_respects_field_filter() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // "rust" appears in both a mood entry and a project entry
        db.insert_entry(&make_grep_entry(
            "mood",
            100,
            serde_json::json!("excited about rust"),
            None,
        ))
        .unwrap();
        db.insert_entry(&make_grep_entry(
            "project",
            200,
            serde_json::json!("learn rust"),
            None,
        ))
        .unwrap();
        db.insert_entry(&make_grep_entry(
            "mood",
            300,
            serde_json::json!("calm"),
            None,
        ))
        .unwrap();

        // No field filter → both "rust" entries match
        let got = run_grep(&db, &["rust"], None);
        assert_eq!(got.len(), 2);

        // Filter to mood → only the mood "rust" entry matches
        let got = run_grep(&db, &["rust"], Some("mood"));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].field, "mood");

        // Filter to project → only the project entry
        let got = run_grep(&db, &["rust"], Some("project"));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].field, "project");

        // Field filter with no matching pattern
        let got = run_grep(&db, &["missing"], Some("mood"));
        assert!(got.is_empty());

        // Dispatch path
        grep(
            GrepArgs {
                patterns: vec!["rust".to_string()],
                field: Some("mood".to_string()),
                output: Default::default(),
            },
            &db,
        )
        .unwrap();
    }

    // Locks in: `grep --field <name>` must exclude tombstoned entries.
    // The filter relies on `Db::entries_for_field`'s `deleted_at IS NULL`
    // clause — if a refactor reroutes grep through a path that returns
    // soft-deleted rows, this test catches the regression.
    #[test]
    fn grep_field_excludes_tombstoned_entries() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Two coffee entries: one will be tombstoned, one stays live.
        let doomed = make_grep_entry("coffee", 100, serde_json::json!("decaf"), None);
        let live = make_grep_entry("coffee", 200, serde_json::json!("espresso"), None);
        db.insert_entry(&doomed).unwrap();
        db.insert_entry(&live).unwrap();

        db.soft_delete_entry(&doomed.slug).unwrap();

        // Match-all regex with the field filter — only the live entry returns.
        let got = run_grep(&db, &[""], Some("coffee"));
        assert_eq!(got.len(), 1, "tombstoned entry must not appear");
        assert_eq!(got[0].slug, live.slug);
        assert!(got.iter().all(|e| e.slug != doomed.slug));

        // Pattern that only matched the tombstoned entry returns nothing.
        let got = run_grep(&db, &["decaf"], Some("coffee"));
        assert!(got.is_empty(), "tombstoned content must not match");

        // Dispatch path mirrors the same exclusion.
        grep(
            GrepArgs {
                patterns: vec!["decaf".to_string()],
                field: Some("coffee".to_string()),
                output: Default::default(),
            },
            &db,
        )
        .unwrap();
    }
}
