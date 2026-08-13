//! `datum grep <pattern>` — regex search over entries.
//!
//! da-lzrh: the haystack is the user-editable projection — the same
//! boundary `datum edit` exposes ([`UserEditableEntry`], da-4lov) — plus
//! the slug, which is user-addressable. System fields (`content_hash`,
//! `hid`, `device_id`, `parser_version`, `raw_input`, …) never enter it,
//! so a pattern can only match text the user can actually see.

use anyhow::{Context, Result};
use datum_core::{
    db::{Db, ListParams},
    entry::Entry,
    time::DatumTime,
};
use regex_lite::Regex;

use crate::{
    args::{GrepArgs, OutputFlagsExt},
    commands::{
        edit::UserEditableEntry, helpers::load_registry_opt, render_helpers::build_render_contexts,
    },
    render::{print_table_with, TableArgs, TimeMetricCol},
};

pub fn grep(args: GrepArgs, db: &Db) -> Result<()> {
    let regexes: Vec<Regex> = args
        .patterns
        .iter()
        .map(|p| Regex::new(p).with_context(|| format!("Invalid regex pattern: {p}")))
        .collect::<Result<_>>()?;

    let matches = grep_entries(db, &regexes, args.field.as_deref())?;

    let registry = load_registry_opt(db);
    let tbl = TableArgs {
        output: args.output()?,
        metric: TimeMetricCol::Hybrid,
        columns: vec![],
        suppress_header: false,
        max_total_width: None,
    };
    let now = DatumTime::now();
    let ctxs = build_render_contexts(&matches, db);
    print_table_with(&matches, Some(&ctxs), Some(&now), registry.as_ref(), &tbl);
    Ok(())
}

/// Collect live entries whose haystack matches every regex.
///
/// Without `--field`, candidates come from `list_entries` with no time
/// bounds, so entries arbitrarily far in the future (planning entries) are
/// searchable — the old path capped the window at now + 1 year (da-lzrh).
/// `include_meta: true` is deliberate: meta/flagged rows (da-ghlx) are
/// searchable here by design — a grep should search everything, even rows
/// the live tail/dayview surfaces hide by default.
fn grep_entries(db: &Db, regexes: &[Regex], field: Option<&str>) -> Result<Vec<Entry>> {
    let candidates = if let Some(field) = field {
        db.entries_for_field(field, None)?
    } else {
        db.list_entries(&ListParams {
            include_meta: true,
            ..Default::default()
        })?
    };

    Ok(candidates
        .into_iter()
        .filter(|entry| match haystack(entry) {
            Some(h) => regexes.iter().all(|r| r.is_match(&h)),
            None => false,
        })
        .collect())
}

/// The searchable text for one entry: its user-editable fields serialized
/// to JSON, plus the slug (user-addressable identity).
fn haystack(entry: &Entry) -> Option<String> {
    let view = UserEditableEntry::from_entry(entry);
    let mut s = serde_json::to_string(&view).ok()?;
    s.push(' ');
    s.push_str(entry.slug.as_str());
    Some(s)
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
            timezone: Some("UTC".to_string()),
        };
        Entry::new_legacy(field, Verb::Point, Some(value), extra, Some(t), None, "d1")
    }

    fn run_grep(db: &datum_core::db::Db, patterns: &[&str], field: Option<&str>) -> Vec<Entry> {
        let regexes: Vec<Regex> = patterns.iter().map(|p| Regex::new(p).unwrap()).collect();
        grep_entries(db, &regexes, field).unwrap()
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
        assert_eq!(got[0].state.as_ref().unwrap(), &serde_json::json!("happy"));

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

    // da-lzrh: the old no-field path capped candidates at now + 365 days,
    // silently hiding planning entries further ahead.
    #[test]
    fn grep_finds_far_future_entries() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let utc = chrono::Utc::now() + chrono::Duration::days(730);
        let t = DatumTime {
            utc,
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
        };
        db.insert_entry(&Entry::new_legacy(
            "plan",
            Verb::Point,
            Some(serde_json::json!("sabbatical")),
            None,
            Some(t),
            None,
            "d1",
        ))
        .unwrap();

        let got = run_grep(&db, &["sabbatical"], None);
        assert_eq!(got.len(), 1, "entries >1y in the future must be greppable");
        assert_eq!(got[0].field, "plan");
    }

    // da-lzrh: matching against invisible system fields gave inexplicable
    // hits — the rendered table shows nothing containing the pattern.
    #[test]
    fn grep_ignores_system_fields() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let e = make_grep_entry("mood", 100, serde_json::json!("happy"), None);
        db.insert_entry(&e).unwrap();

        assert!(
            run_grep(&db, &[e.content_hash.as_str()], None).is_empty(),
            "content_hash must not be searchable"
        );
        assert!(
            run_grep(&db, &[e.hid.as_str()], None).is_empty(),
            "hid must not be searchable"
        );

        // The slug IS user-addressable, so it stays in the haystack.
        let got = run_grep(&db, &[e.slug.as_str()], None);
        assert_eq!(got.len(), 1, "slug must remain searchable");
    }

    // Pins the documented decision (da-lzrh / da-ghlx): grep searches
    // everything, including meta rows that tail/dayview hide by default.
    #[test]
    fn grep_searches_meta_rows() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let mut meta = make_grep_entry("journal", 100, serde_json::json!("checkpoint"), None);
        meta.extra_metadata = Some(serde_json::json!({"meta": true, "text": "x"}));
        db.insert_entry(&meta).unwrap();

        let got = run_grep(&db, &["checkpoint"], None);
        assert_eq!(got.len(), 1, "meta rows are searchable by design");
        assert_eq!(got[0].slug, meta.slug);
    }
}
