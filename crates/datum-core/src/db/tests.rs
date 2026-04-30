use super::*;
use crate::entry::{new_hid, Entry, EntryShape, PrecisionUnit, Verb};
use crate::error::DatumError;
use crate::field_spec::{Alias, DerivedAlias, FieldSpec};
use crate::time::DatumTime;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value as JsonValue;

#[test]
fn test_insert_and_fetch() {
    let db = Db::open_in_memory().unwrap();
    let t = DatumTime::now();
    let entry = Entry::new(
        "coffee",
        Verb::Occur,
        Some(serde_json::json!(2)),
        None,
        Some("cups".to_string()),
        Some(t),
        None,
        "test-device",
    );
    let slug = entry.slug.clone();
    let hid = entry.hid.clone();

    db.insert_entry(&entry).unwrap();

    // Look up by slug (primary) and by hid (secondary).
    let fetched = db.get_entry(&slug).unwrap();
    assert_eq!(fetched.field, "coffee");
    assert_eq!(fetched.unit, Some("cups".to_string()));

    let fetched_by_hid = db.get_entry(&hid).unwrap();
    assert_eq!(fetched_by_hid.slug, slug);
}

#[test]
fn test_duplicate_rejected() {
    let db = Db::open_in_memory().unwrap();
    let t = DatumTime::now();
    let entry = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("happy")),
        None,
        None,
        Some(t),
        None,
        "d1",
    );
    db.insert_entry(&entry).unwrap();
    // Same content_hash → should fail
    let err = db.insert_entry(&entry);
    assert!(matches!(err, Err(DatumError::Duplicate(_))));
}

#[test]
fn test_entries_for_field() {
    let db = Db::open_in_memory().unwrap();
    // Distinct per-state timestamps — slug collision on identical
    // start_time hard-errors post-da-p5j, and `DatumTime::now()` in a
    // tight loop returns the same value on fast machines.
    let base = chrono::Utc::now();
    for (i, state) in ["happy", "sad", "focused"].iter().enumerate() {
        let utc = base + chrono::Duration::milliseconds(i as i64 + 1);
        let t = DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let e = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!(state)),
            None,
            None,
            Some(t),
            None,
            "d1",
        );
        db.insert_entry(&e).unwrap();
    }
    let entries = db.entries_for_field("mood", None).unwrap();
    assert_eq!(entries.len(), 3);
}

#[test]
fn test_entries_for_field_asc() {
    let db = Db::open_in_memory().unwrap();
    let base: chrono::DateTime<chrono::Utc> = "2024-01-01T00:00:00Z".parse().unwrap();
    for secs in &[300i64, 100, 200] {
        let utc = base + chrono::Duration::seconds(*secs);
        let t = DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let e = Entry::new(
            "mood",
            Verb::Occur,
            Some(serde_json::json!(*secs)),
            None,
            None,
            Some(t),
            None,
            "d1",
        );
        db.insert_entry(&e).unwrap();
    }
    let entries = db.entries_for_field_asc("mood", None).unwrap();
    assert_eq!(entries.len(), 3);
    let values: Vec<i64> = entries
        .iter()
        .map(|e| e.value.as_ref().unwrap().as_i64().unwrap())
        .collect();
    assert_eq!(values, vec![100, 200, 300]);

    let limited = db.entries_for_field_asc("mood", Some(2)).unwrap();
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0].value.as_ref().unwrap().as_i64().unwrap(), 100);
    assert_eq!(limited[1].value.as_ref().unwrap().as_i64().unwrap(), 200);
}

fn switch_at(field: &str, value: JsonValue, t: DatumTime) -> Entry {
    Entry::new(
        field,
        Verb::Switch,
        Some(value),
        None,
        None,
        Some(t),
        None,
        "d1",
    )
}

#[test]
fn test_get_tracked_fields_derives_from_log() {
    // Three fields, one explicitly ignored. get_tracked_fields walks
    // distinct fields and labels each via state_at(now).
    let db = Db::open_in_memory().unwrap();
    let t = DatumTime::now();
    db.insert_entry(&switch_at("coffee", JsonValue::Null, t.clone()))
        .unwrap();
    // 'mood' has a real state (happy) — tracked, not ignored.
    db.insert_entry(&switch_at("mood", serde_json::json!("happy"), t.clone()))
        .unwrap();
    // Stash a color spec on mood so the projection picks it up.
    let mut spec = FieldSpec::new("mood", Verb::Switch);
    spec.color = Some("ff6600".to_string());
    db.save_field_spec(&spec).unwrap();
    // 'youtube' is explicitly ignored.
    db.insert_entry(&switch_at("youtube", serde_json::json!("IGNORED"), t))
        .unwrap();

    let fields = db.get_tracked_fields().unwrap();
    assert_eq!(fields.len(), 3);

    let coffee = fields.iter().find(|f| f.field == "coffee").unwrap();
    assert!(!coffee.ignored, "track-with-null is tracked, not ignored");

    let mood = fields.iter().find(|f| f.field == "mood").unwrap();
    assert!(!mood.ignored);
    assert_eq!(mood.color.as_deref(), Some("ff6600"));

    let yt = fields.iter().find(|f| f.field == "youtube").unwrap();
    assert!(yt.ignored, "switch IGNORED must surface as ignored");
}

#[test]
fn test_list_distinct_fields_excludes_tombstones() {
    let db = Db::open_in_memory().unwrap();
    let t = DatumTime::now();
    db.insert_entry(&switch_at("a", serde_json::json!(1), t.clone()))
        .unwrap();
    let b_entry = switch_at("b", serde_json::json!(1), t.clone());
    db.insert_entry(&b_entry).unwrap();
    db.insert_entry(&switch_at("c", serde_json::json!(1), t))
        .unwrap();
    // Tombstone 'b'.
    db.delete_entry(&b_entry.slug).unwrap();

    let fields = db.list_distinct_fields().unwrap();
    assert_eq!(fields, vec!["a".to_string(), "c".to_string()]);
}

#[test]
fn test_state_colors() {
    let db = Db::open_in_memory().unwrap();
    db.set_state_color("mood", "happy", "00ff00").unwrap();
    db.set_state_color("mood", "sad", "ff0000").unwrap();

    let map = db.get_all_state_colors().unwrap();
    assert_eq!(
        map.get(&("mood".to_string(), "happy".to_string()))
            .map(|s| s.as_str()),
        Some("00ff00")
    );
    assert_eq!(
        map.get(&("mood".to_string(), "sad".to_string()))
            .map(|s| s.as_str()),
        Some("ff0000")
    );
}

// ------------------------------------------------------------------
// get_active_state — A5 acceptance tests
// ------------------------------------------------------------------

fn utc_at(h: u32, m: u32) -> DatumTime {
    let utc = chrono::DateTime::parse_from_rfc3339(&format!("2024-01-01T{h:02}:{m:02}:00Z"))
        .unwrap()
        .with_timezone(&Utc);
    DatumTime {
        utc,
        offset_minutes: 0,
        timezone: "UTC".to_string(),
    }
}

fn as_utc(h: u32, m: u32) -> chrono::DateTime<Utc> {
    utc_at(h, m).utc
}

#[test]
fn active_state_returns_none_before_any_entry() {
    let db = Db::open_in_memory().unwrap();
    let got = db.get_active_state("mood", as_utc(12, 0)).unwrap();
    assert_eq!(got, JsonValue::Null);
}

#[test]
fn active_state_returns_last_switch_before_time() {
    let db = Db::open_in_memory().unwrap();
    let e1 = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("happy")),
        None,
        None,
        Some(utc_at(9, 0)),
        None,
        "d",
    );
    let e2 = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("focused")),
        None,
        None,
        Some(utc_at(10, 0)),
        None,
        "d",
    );
    let e3 = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("tired")),
        None,
        None,
        Some(utc_at(15, 0)),
        None,
        "d",
    );
    db.insert_entry(&e1).unwrap();
    db.insert_entry(&e2).unwrap();
    db.insert_entry(&e3).unwrap();

    // At 12:00 → "focused"; at 09:30 → "happy"; at 16:00 → "tired"; at 08:59 → null
    assert_eq!(
        db.get_active_state("mood", as_utc(12, 0)).unwrap(),
        serde_json::json!("focused")
    );
    assert_eq!(
        db.get_active_state("mood", as_utc(9, 30)).unwrap(),
        serde_json::json!("happy")
    );
    assert_eq!(
        db.get_active_state("mood", as_utc(16, 0)).unwrap(),
        serde_json::json!("tired")
    );
    assert_eq!(
        db.get_active_state("mood", as_utc(8, 59)).unwrap(),
        JsonValue::Null
    );
}

#[test]
fn active_state_returns_none_after_stop() {
    let db = Db::open_in_memory().unwrap();
    let start = Entry::new(
        "run",
        Verb::Start,
        None,
        None,
        None,
        Some(utc_at(9, 0)),
        None,
        "d",
    );
    let stop = Entry::new(
        "run",
        Verb::Stop,
        None,
        None,
        None,
        Some(utc_at(10, 0)),
        None,
        "d",
    );
    db.insert_entry(&start).unwrap();
    db.insert_entry(&stop).unwrap();

    // During the run → true
    assert_eq!(
        db.get_active_state("run", as_utc(9, 30)).unwrap(),
        serde_json::json!(true),
    );
    // After stop → false (not null — "stopped" is a distinct state)
    assert_eq!(
        db.get_active_state("run", as_utc(10, 30)).unwrap(),
        serde_json::json!(false),
    );
}

#[test]
fn active_state_handles_concurrent_blocks() {
    // Two overlapping start entries: the LATER start wins while both are
    // active; when one stops, the other's state still governs.
    let db = Db::open_in_memory().unwrap();
    // First start: switch to "a" at 09:00, ends 12:00
    let a = Entry::new(
        "task",
        Verb::Switch,
        Some(serde_json::json!("a")),
        None,
        None,
        Some(utc_at(9, 0)),
        Some(utc_at(12, 0)),
        "d",
    );
    // Second start: switch to "b" at 10:00, ends 11:00
    let b = Entry::new(
        "task",
        Verb::Switch,
        Some(serde_json::json!("b")),
        None,
        None,
        Some(utc_at(10, 0)),
        Some(utc_at(11, 0)),
        "d",
    );
    db.insert_entry(&a).unwrap();
    db.insert_entry(&b).unwrap();

    // 09:30: only "a" active → "a"
    assert_eq!(
        db.get_active_state("task", as_utc(9, 30)).unwrap(),
        serde_json::json!("a"),
    );
    // 10:30: "b" started most recently → "b"
    assert_eq!(
        db.get_active_state("task", as_utc(10, 30)).unwrap(),
        serde_json::json!("b"),
    );
    // 11:30: "b" ended (→ false), "a" still running but its start is older → false wins as latest event
    assert_eq!(
        db.get_active_state("task", as_utc(11, 30)).unwrap(),
        serde_json::json!(false),
    );
    // 12:30: both ended → still false
    assert_eq!(
        db.get_active_state("task", as_utc(12, 30)).unwrap(),
        serde_json::json!(false),
    );
}

#[test]
fn active_state_skips_tombstoned_rows() {
    let db = Db::open_in_memory().unwrap();
    let older = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("calm")),
        None,
        None,
        Some(utc_at(9, 0)),
        None,
        "d",
    );
    let newer = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("angry")),
        None,
        None,
        Some(utc_at(12, 0)),
        None,
        "d",
    );
    let newer_slug = newer.slug.clone();
    db.insert_entry(&older).unwrap();
    db.insert_entry(&newer).unwrap();

    // Before tombstone: active at 13:00 is "angry"
    assert_eq!(
        db.get_active_state("mood", as_utc(13, 0)).unwrap(),
        serde_json::json!("angry"),
    );

    db.soft_delete_entry(&newer_slug).unwrap();

    // After tombstone: newer entry excluded → falls back to "calm"
    assert_eq!(
        db.get_active_state("mood", as_utc(13, 0)).unwrap(),
        serde_json::json!("calm"),
    );
}

#[test]
fn active_state_ignores_other_fields() {
    let db = Db::open_in_memory().unwrap();
    let other = Entry::new(
        "different",
        Verb::Switch,
        Some(serde_json::json!("x")),
        None,
        None,
        Some(utc_at(9, 0)),
        None,
        "d",
    );
    db.insert_entry(&other).unwrap();
    assert_eq!(
        db.get_active_state("mood", as_utc(12, 0)).unwrap(),
        JsonValue::Null,
    );
}

#[test]
fn test_latest_entry_for_now_skips_note() {
    let db = Db::open_in_memory().unwrap();
    // Distinct timestamps — slug uses millisecond precision and two
    // `DatumTime::now()` calls on fast machines land in the same ms,
    // collapsing to the same slug (hard-errors post-da-p5j).
    let base = Utc::now();
    let mk = |offset_ms: i64| DatumTime {
        utc: base + Duration::milliseconds(offset_ms),
        offset_minutes: 0,
        timezone: "UTC".to_string(),
    };
    let t1 = mk(0);
    let switch_entry = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("happy")),
        None,
        None,
        Some(t1),
        None,
        "d1",
    );
    db.insert_entry(&switch_entry).unwrap();
    let t2 = mk(1);
    let note_entry = Entry::new(
        "mood",
        Verb::Add,
        Some(serde_json::json!("journal entry")),
        None,
        None,
        Some(t2),
        None,
        "d1",
    );
    db.insert_entry(&note_entry).unwrap();

    let latest = db.latest_entry_for_now("mood").unwrap();
    assert!(latest.is_some());
    assert_eq!(latest.unwrap().verb, Verb::Switch);
}

// ─── New FieldSpec-source-of-truth tests (A0 acceptance) ──────────────────

#[test]
fn field_spec_stores_id_template() {
    let db = Db::open_in_memory().unwrap();
    let mut spec = FieldSpec::new("mood", Verb::Switch);
    spec.id_template = Some("%field%:%state%".to_string());
    db.save_field_spec(&spec).unwrap();

    let loaded = db.load_spec("mood").unwrap().expect("spec present");
    assert_eq!(loaded.id_template.as_deref(), Some("%field%:%state%"));
}

#[test]
fn field_spec_stores_color_and_states() {
    let db = Db::open_in_memory().unwrap();
    let mut spec = FieldSpec::new("mood", Verb::Switch);
    spec.color = Some("ff6600".into());
    spec.allowed_states = Some(vec!["happy".into(), "sad".into()]);
    spec.state_colors.insert("happy".into(), "00ff00".into());
    db.save_field_spec(&spec).unwrap();

    let loaded = db.load_spec("mood").unwrap().unwrap();
    assert_eq!(loaded.color.as_deref(), Some("ff6600"));
    let states = loaded.allowed_states.as_ref().unwrap();
    assert!(states.contains(&"happy".to_string()));
    assert!(states.contains(&"sad".to_string()));
    assert_eq!(
        loaded.state_colors.get("happy").map(|s| s.as_str()),
        Some("00ff00")
    );
}

#[test]
fn adding_state_creates_derived_alias() {
    let db = Db::open_in_memory().unwrap();
    let mut spec = FieldSpec::new("mood", Verb::Switch);
    spec.allowed_states = Some(vec!["happy".into()]);
    spec.regenerate_derived_aliases();
    db.save_field_spec(&spec).unwrap();

    let alias = db
        .lookup_alias("happy")
        .unwrap()
        .expect("derived alias present");
    assert_eq!(alias.expands_to, "switch mood happy");

    // Registry also sees it as an alias.
    let reg = db.load_registry().unwrap();
    assert!(reg.aliases.contains_key("happy"));
}

#[test]
fn removing_state_drops_derived_alias() {
    let db = Db::open_in_memory().unwrap();
    let mut spec = FieldSpec::new("mood", Verb::Switch);
    spec.allowed_states = Some(vec!["happy".into(), "sad".into()]);
    spec.regenerate_derived_aliases();
    db.save_field_spec(&spec).unwrap();
    assert!(db.lookup_alias("sad").unwrap().is_some());

    // Drop "sad" from allowed_states and re-save.
    let mut updated = db.load_spec("mood").unwrap().unwrap();
    updated.allowed_states = Some(vec!["happy".into()]);
    updated.regenerate_derived_aliases();
    db.save_field_spec(&updated).unwrap();

    assert!(
        db.lookup_alias("sad").unwrap().is_none(),
        "sad alias must be dropped"
    );
    assert!(
        db.lookup_alias("happy").unwrap().is_some(),
        "happy alias must remain"
    );
}

#[test]
fn deleting_spec_cascades_to_derived_aliases() {
    let db = Db::open_in_memory().unwrap();
    let mut spec = FieldSpec::new("mood", Verb::Switch);
    spec.allowed_states = Some(vec!["happy".into()]);
    spec.regenerate_derived_aliases();
    db.save_field_spec(&spec).unwrap();

    db.delete_field_spec("mood").unwrap();
    assert!(db.lookup_alias("happy").unwrap().is_none());
    assert!(db.load_spec("mood").unwrap().is_none());
}

#[test]
fn track_ignore_round_trip_via_log() {
    // da-2sr: tracking state lives in the entry log, not the spec.
    // Track → ignore → track at three timepoints; state_at returns the
    // value of the most recent state-bearing entry at each cutoff.
    let db = Db::open_in_memory().unwrap();
    let t1 = utc_at(9, 0);
    let t2 = utc_at(10, 0);
    let t3 = utc_at(11, 0);

    // t1: track (switch with NULL value)
    db.insert_entry(&Entry::new(
        "mood",
        Verb::Switch,
        None,
        None,
        None,
        Some(t1.clone()),
        None,
        "d1",
    ))
    .unwrap();
    // t2: ignore (switch IGNORED)
    db.insert_entry(&Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("IGNORED")),
        None,
        None,
        Some(t2.clone()),
        None,
        "d1",
    ))
    .unwrap();
    // t3: track again (switch with NULL value)
    db.insert_entry(&Entry::new(
        "mood",
        Verb::Switch,
        None,
        None,
        None,
        Some(t3.clone()),
        None,
        "d1",
    ))
    .unwrap();

    // At t1: tracked (NULL value)
    assert_eq!(db.state_at("mood", t1.utc).unwrap(), Some(JsonValue::Null),);
    // At t2: ignored
    assert_eq!(
        db.state_at("mood", t2.utc).unwrap(),
        Some(JsonValue::String("IGNORED".to_string())),
    );
    // At t3: tracked again (NULL value)
    assert_eq!(db.state_at("mood", t3.utc).unwrap(), Some(JsonValue::Null),);
    // Before any entry: None (the IGNORED default).
    assert_eq!(db.state_at("mood", as_utc(8, 0)).unwrap(), None,);
}

// ─── state_at — da-2sr acceptance tests ──────────────────────────────────

#[test]
fn state_at_returns_none_for_never_written_field() {
    let db = Db::open_in_memory().unwrap();
    assert_eq!(db.state_at("ghost", Utc::now()).unwrap(), None);
}

#[test]
fn state_at_latest_switch_wins() {
    let db = Db::open_in_memory().unwrap();
    for (h, v) in &[(9u32, "happy"), (10, "sad"), (11, "focused")] {
        db.insert_entry(&Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!(*v)),
            None,
            None,
            Some(utc_at(*h, 0)),
            None,
            "d1",
        ))
        .unwrap();
    }
    assert_eq!(
        db.state_at("mood", as_utc(12, 0)).unwrap(),
        Some(serde_json::json!("focused")),
    );
}

#[test]
fn state_at_occur_contributes() {
    let db = Db::open_in_memory().unwrap();
    // Switch then occur — most recent is the occur.
    db.insert_entry(&Entry::new(
        "coffee",
        Verb::Switch,
        Some(serde_json::json!("on")),
        None,
        None,
        Some(utc_at(9, 0)),
        None,
        "d1",
    ))
    .unwrap();
    db.insert_entry(&Entry::new(
        "coffee",
        Verb::Occur,
        Some(serde_json::json!(2)),
        None,
        None,
        Some(utc_at(10, 0)),
        None,
        "d1",
    ))
    .unwrap();
    assert_eq!(
        db.state_at("coffee", as_utc(11, 0)).unwrap(),
        Some(serde_json::json!(2)),
    );
}

#[test]
fn state_at_excludes_deleted_entries() {
    let db = Db::open_in_memory().unwrap();
    let e = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("IGNORED")),
        None,
        None,
        Some(utc_at(10, 0)),
        None,
        "d1",
    );
    db.insert_entry(&e).unwrap();
    db.delete_entry(&e.slug).unwrap();
    assert_eq!(db.state_at("mood", as_utc(11, 0)).unwrap(), None);
}

#[test]
fn state_at_respects_time_cutoff() {
    let db = Db::open_in_memory().unwrap();
    db.insert_entry(&Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("happy")),
        None,
        None,
        Some(utc_at(10, 0)),
        None,
        "d1",
    ))
    .unwrap();
    // Cutoff before the entry → None.
    assert_eq!(db.state_at("mood", as_utc(9, 0)).unwrap(), None);
    // Cutoff after → Some.
    assert_eq!(
        db.state_at("mood", as_utc(11, 0)).unwrap(),
        Some(serde_json::json!("happy")),
    );
}

#[test]
fn state_at_skips_add_entries() {
    // `add` (notes) doesn't carry state — it must not appear as the
    // "most recent state-bearing entry" even when it's the latest write.
    let db = Db::open_in_memory().unwrap();
    db.insert_entry(&Entry::new(
        "journal",
        Verb::Switch,
        Some(serde_json::json!("active")),
        None,
        None,
        Some(utc_at(9, 0)),
        None,
        "d1",
    ))
    .unwrap();
    db.insert_entry(&Entry::new(
        "journal",
        Verb::Add,
        Some(serde_json::json!("a journal note")),
        None,
        None,
        Some(utc_at(10, 0)),
        None,
        "d1",
    ))
    .unwrap();
    assert_eq!(
        db.state_at("journal", as_utc(11, 0)).unwrap(),
        Some(serde_json::json!("active")),
    );
}

#[test]
fn user_alias_survives_derived_alias_save() {
    // D3 policy: user aliases are never overwritten by auto-derived
    // aliases. The user wins the name; the derived alias is skipped and
    // surfaced via the SaveSpecReport.
    let db = Db::open_in_memory().unwrap();
    db.save_alias(&Alias {
        name: "happy".into(),
        expands_to: "add journal happy".into(),
        description: None,
    })
    .unwrap();

    let mut spec = FieldSpec::new("mood", Verb::Switch);
    spec.allowed_states = Some(vec!["happy".into()]);
    spec.derived_aliases = vec![DerivedAlias {
        state: "happy".into(),
        expands_to: "switch mood happy".into(),
        kind: crate::field_spec::DerivedAliasKind::State,
    }];
    let report = db.save_field_spec_with_report(&spec).unwrap();

    let alias = db.lookup_alias("happy").unwrap().unwrap();
    assert_eq!(
        alias.expands_to, "add journal happy",
        "user alias wins over derived"
    );

    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].name, "happy");
    assert!(matches!(
        report.skipped[0].reason,
        crate::db::AliasConflictReason::UserAlias
    ));
}

// ────────────────────────────────────────────────────────────────────────
// A4: Conflict strategies and soft-delete
// ────────────────────────────────────────────────────────────────────────

/// Build two entries whose content_hash collides (same field / time /
/// value), with different hids so we can tell them apart. Their default
/// slugs also collide — the insert path's retry-with-suffix logic will
/// disambiguate on Rekey.
fn colliding_pair() -> (Entry, Entry) {
    let t = utc_at(9, 0);
    let first = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("happy")),
        Some(serde_json::json!({"source": "morning"})),
        None,
        Some(t.clone()),
        None,
        "device-a",
    );
    let second = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("happy")),
        Some(serde_json::json!({"source": "morning"})),
        None,
        Some(t),
        None,
        "device-b",
    );
    assert_eq!(
        first.content_hash, second.content_hash,
        "fixture sanity: colliding entries must share a content_hash"
    );
    assert_ne!(first.hid, second.hid, "fixture sanity: hids must differ");
    (first, second)
}

#[test]
fn insert_conflict_update_mutates_in_place() {
    let db = Db::open_in_memory().unwrap();
    let (first, second) = colliding_pair();
    let first_slug = first.slug.clone();
    db.insert_entry(&first).unwrap();

    let before_modified = db.get_entry(&first_slug).unwrap().modify_time;
    // Sleep a tick so modify_time can advance even on fast clocks.
    std::thread::sleep(std::time::Duration::from_millis(5));

    let outcome = db
        .insert_entry_with_strategy(&second, ConflictStrategy::Update)
        .unwrap();
    assert_eq!(
        outcome,
        InsertOutcome::Updated {
            slug: first_slug.clone()
        }
    );

    // Still exactly one row; it mutated in place (original slug preserved).
    let all = db.entries_for_field("mood", None).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].slug, first_slug);
    // The incoming `device_id` wins on Update.
    assert_eq!(all[0].device_id, "device-b");
    // modify_time advanced.
    assert!(
        all[0].modify_time > before_modified,
        "Update must bump modify_time"
    );
}

#[test]
fn insert_conflict_merge_unions_extra_data() {
    let db = Db::open_in_memory().unwrap();
    // Existing row carries one tag; incoming brings another. Merge must keep
    // both and, on scalar conflicts, let the incoming value win.
    let t = utc_at(10, 0);
    let existing = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("happy")),
        Some(serde_json::json!({"source": "morning", "shared": "old"})),
        None,
        Some(t.clone()),
        None,
        "device-a",
    );
    let existing_slug = existing.slug.clone();
    db.insert_entry(&existing).unwrap();

    // Craft an incoming entry with the same content_hash but different
    // extra. Starting from a clone keeps field/time/value identical;
    // swap the extra after construction so merge has distinct payloads
    // to combine while content_hash stays equal.
    let mut incoming = existing.clone();
    incoming.hid = new_hid();
    incoming.data = Some(serde_json::json!({"shared": "new", "weather": "sunny"}));

    let outcome = db
        .insert_entry_with_strategy(&incoming, ConflictStrategy::Merge)
        .unwrap();
    assert_eq!(
        outcome,
        InsertOutcome::Merged {
            slug: existing_slug.clone()
        }
    );

    // Still one row. data = union, scalar conflict → incoming wins.
    let all = db.entries_for_field("mood", None).unwrap();
    assert_eq!(all.len(), 1);
    let merged_extra = all[0].data.as_ref().unwrap();
    assert_eq!(merged_extra["source"], serde_json::json!("morning"));
    assert_eq!(merged_extra["shared"], serde_json::json!("new"));
    assert_eq!(merged_extra["weather"], serde_json::json!("sunny"));
}

#[test]
fn insert_conflict_rekey_creates_new_slug_keeps_old() {
    let db = Db::open_in_memory().unwrap();
    let (first, second) = colliding_pair();
    let first_slug = first.slug.clone();
    db.insert_entry(&first).unwrap();

    let outcome = db
        .insert_entry_with_strategy(&second, ConflictStrategy::Rekey)
        .unwrap();
    // Rekey inserts a fresh row; because the default slug collides, the
    // raw insert retries with a suffixed slug (`<slug>_xxxx`). The
    // outcome carries that concrete stored slug.
    let (old_slug, new_slug) = match outcome {
        InsertOutcome::Rekeyed {
            old_slug, new_slug, ..
        } => (old_slug, new_slug),
        other => panic!("expected Rekeyed, got {other:?}"),
    };
    assert_eq!(old_slug, first_slug);
    assert_ne!(new_slug, first_slug, "rekeyed slug must be distinct");
    assert!(
        new_slug.starts_with(first_slug.as_str()),
        "rekeyed slug should start with the original: {new_slug}"
    );

    // Both rows coexist, same content_hash, distinct slugs.
    let all = db.entries_for_field("mood", None).unwrap();
    assert_eq!(all.len(), 2);
    let slugs: std::collections::HashSet<_> = all.iter().map(|e| e.slug.clone()).collect();
    assert!(slugs.contains(&first_slug));
    assert!(slugs.contains(&new_slug));
    assert_eq!(all[0].content_hash, all[1].content_hash);
}

#[test]
fn insert_conflict_ignore_returns_existing() {
    let db = Db::open_in_memory().unwrap();
    let (first, second) = colliding_pair();
    let first_slug = first.slug.clone();
    db.insert_entry(&first).unwrap();

    let outcome = db
        .insert_entry_with_strategy(&second, ConflictStrategy::Ignore)
        .unwrap();
    assert_eq!(
        outcome,
        InsertOutcome::Ignored {
            slug: first_slug.clone()
        }
    );

    // Original row untouched; second row not inserted.
    let all = db.entries_for_field("mood", None).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].slug, first_slug);
    assert_eq!(all[0].device_id, "device-a");
}

#[test]
fn soft_deleted_rows_excluded_from_list() {
    let db = Db::open_in_memory().unwrap();
    let live = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("happy")),
        None,
        None,
        Some(utc_at(9, 0)),
        None,
        "d",
    );
    let doomed = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("sad")),
        None,
        None,
        Some(utc_at(10, 0)),
        None,
        "d",
    );
    let doomed_slug = doomed.slug.clone();
    db.insert_entry(&live).unwrap();
    db.insert_entry(&doomed).unwrap();

    db.soft_delete_entry(&doomed_slug).unwrap();

    // All the live-read paths hide the tombstoned row.
    let by_field = db.entries_for_field("mood", None).unwrap();
    assert_eq!(by_field.len(), 1);
    assert_ne!(by_field[0].slug, doomed_slug);

    let by_field_asc = db.entries_for_field_asc("mood", None).unwrap();
    assert_eq!(by_field_asc.len(), 1);
    assert_ne!(by_field_asc[0].slug, doomed_slug);

    let in_range = db.entries_in_range(as_utc(0, 0), as_utc(23, 59)).unwrap();
    assert_eq!(in_range.len(), 1);
    assert_ne!(in_range[0].slug, doomed_slug);

    let got = db.get_entry(&doomed_slug);
    assert!(matches!(got, Err(DatumError::NotFound(_))));
    assert_eq!(db.count_entries().unwrap(), 1);

    // ...but the "including deleted" view keeps the row.
    let including = db.list_including_deleted().unwrap();
    assert_eq!(including.len(), 2);
    assert!(including.iter().any(|e| e.slug == doomed_slug));
    let tombstoned = db.get_entry_including_deleted(&doomed_slug).unwrap();
    assert!(tombstoned.deleted_at.is_some());
}

#[test]
fn undelete_restores_row() {
    let db = Db::open_in_memory().unwrap();
    let e = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("happy")),
        None,
        None,
        Some(utc_at(9, 0)),
        None,
        "d",
    );
    let slug = e.slug.clone();
    db.insert_entry(&e).unwrap();
    db.soft_delete_entry(&slug).unwrap();
    assert!(matches!(db.get_entry(&slug), Err(DatumError::NotFound(_))));

    db.undelete(&slug).unwrap();
    // Row is visible again, tombstone cleared.
    let back = db.get_entry(&slug).unwrap();
    assert_eq!(back.slug, slug);
    assert!(back.deleted_at.is_none());

    // Undeleting something that isn't tombstoned (or doesn't exist) errors.
    let err = db.undelete(&slug);
    assert!(matches!(err, Err(DatumError::NotFound(_))));
    let err = db.undelete("nonexistent");
    assert!(matches!(err, Err(DatumError::NotFound(_))));
}

#[test]
fn tombstoned_row_does_not_block_distinct_new_insert() {
    // Soft-deleting an entry must not interfere with subsequent inserts
    // at *distinct* slugs: the tombstone is invisible to live queries,
    // and the dedup-by-content_hash query filters tombstones, so a
    // fresh write on a different slug lands as Inserted regardless of
    // what the soft-deleted predecessor looked like.
    //
    // Re-inserting at the *same* slug as a tombstoned row is a separate
    // case — the slug index is non-partial, so the tombstone retains
    // ownership of the slug until the row is hard-deleted or undeleted.
    // Callers that want to recycle a slug must `undelete` first.
    let db = Db::open_in_memory().unwrap();
    let first = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("happy")),
        Some(serde_json::json!({"source": "morning"})),
        None,
        Some(utc_at(9, 0)),
        None,
        "device-a",
    );
    let first_slug = first.slug.clone();
    let second = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("focused")),
        Some(serde_json::json!({"source": "afternoon"})),
        None,
        Some(utc_at(10, 0)),
        None,
        "device-b",
    );
    db.insert_entry(&first).unwrap();
    db.soft_delete_entry(&first_slug).unwrap();

    let outcome = db
        .insert_entry_with_strategy(&second, ConflictStrategy::Ignore)
        .unwrap();
    assert!(matches!(outcome, InsertOutcome::Inserted { .. }));
    // Original tombstone + new row = 2 total rows in the store.
    assert_eq!(db.list_including_deleted().unwrap().len(), 2);
    // But only the new one is live.
    assert_eq!(db.entries_for_field("mood", None).unwrap().len(), 1);
}

#[test]
fn migrates_pre_a0_aliases_table_adds_derived_from_field() {
    // Regression (da-ndd.6): DBs created before a0 (da-n4bi) have an
    // `aliases` table with only (name, alias_json). `Db::open` must add
    // the `derived_from_field` column and index without erroring, so that
    // downstream queries like `lookup_alias` keep working.
    let path = std::env::temp_dir().join(format!("datum-migrate-{}.db", new_hid()));
    let path_str = path.to_str().unwrap().to_string();

    {
        // Hand-craft the pre-a0 schema on a raw connection.
        let raw = Connection::open(&path_str).unwrap();
        raw.execute_batch(
            "CREATE TABLE aliases (\
                name       TEXT NOT NULL PRIMARY KEY, \
                alias_json TEXT NOT NULL);",
        )
        .unwrap();
        raw.execute(
            "INSERT INTO aliases (name, alias_json) VALUES (?1, ?2)",
            params![
                "legacy",
                r#"{"name":"legacy","expands_to":"note legacy","description":null}"#,
            ],
        )
        .unwrap();
    }

    let db = Db::open(&path_str).expect("Db::open must migrate a pre-a0 aliases table");

    // The pre-existing row is still queryable (this hits the SELECT that
    // surfaced the original user-facing error).
    let legacy = db
        .lookup_alias("legacy")
        .expect("lookup_alias must not error on migrated DB")
        .expect("pre-existing alias row must survive migration");
    assert_eq!(legacy.name, "legacy");
    assert_eq!(legacy.expands_to, "note legacy");

    // New derived aliases can now be written (column is live).
    let derived = crate::field_spec::Alias {
        name: "happy".to_string(),
        expands_to: "switch mood happy".to_string(),
        description: None,
    };
    let derived_json = serde_json::to_string(&derived).unwrap();
    db.conn
        .execute(
            "INSERT INTO aliases (name, alias_json, derived_from_field) \
             VALUES (?1, ?2, ?3)",
            params!["happy", derived_json, "mood"],
        )
        .expect("insert into migrated aliases table must succeed");

    // And the supporting index exists (sqlite_master lookup).
    let has_index: bool = db
        .conn
        .query_row(
            "SELECT 1 FROM sqlite_master \
             WHERE type = 'index' AND name = 'idx_aliases_derived_from_field'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .unwrap()
        .is_some();
    assert!(
        has_index,
        "idx_aliases_derived_from_field must be created during migration"
    );

    // Reopening the migrated DB is a no-op (idempotent).
    drop(db);
    let db2 = Db::open(&path_str).expect("reopen of migrated DB must succeed");
    assert!(db2.lookup_alias("legacy").unwrap().is_some());
    assert!(db2.lookup_alias("happy").unwrap().is_some());

    std::fs::remove_file(&path_str).ok();
}

// ─── resolve_prefix: slug and hid prefix matching ──────────────────────

/// Insert an entry with a handcrafted `slug` + `hid` so the tests can
/// reason about exact prefix matches instead of random strings.
fn insert_fixture(db: &Db, slug: &str, hid: &str, field: &str) -> Entry {
    use chrono::Utc;
    let now = Utc::now();
    let e = Entry {
        hid: crate::ids::Hid(hid.to_string()),
        slug: crate::ids::Slug(slug.to_string()),
        content_hash: format!("hash-{slug}"),
        field: field.to_string(),
        entry_shape: EntryShape::Point,
        intent: Some("note".to_string()),
        verb: Verb::Add,
        value: None,
        data: None,
        unit: None,
        start_time: None,
        end_time: None,
        start_time_precision: 0,
        start_time_precision_unit: PrecisionUnit::Exact,
        end_time_precision: 0,
        end_time_precision_unit: PrecisionUnit::Exact,
        lat: None,
        lon: None,
        location: None,
        create_time: now,
        modify_time: now,
        device_id: "test".to_string(),
        id_structure: None,
        field_structure: None,
        deleted_at: None,
        raw_input: String::new(),
        alias_chain: None,
        parser_version: 1,
        user_asserted_last_state: None,
        extra_metadata: None,
    };
    db.insert_entry(&e).unwrap();
    e
}

#[test]
fn resolve_unique_prefix() {
    let db = Db::open_in_memory().unwrap();
    let e = insert_fixture(&db, "coffee:a1b2c", "abcdefghijkl", "coffee");
    // Another entry that doesn't share the prefix being tested.
    insert_fixture(&db, "tea:qrstu", "qrstuvwxyz01", "tea");

    // Slug prefix → unique hit.
    let got = db.resolve_prefix("coffee:a1").unwrap();
    assert_eq!(got.slug, e.slug);

    // Hid prefix → unique hit.
    let got = db.resolve_prefix("abcd").unwrap();
    assert_eq!(got.slug, e.slug);

    // Tombstoned rows are excluded — creating a ghost with the same prefix
    // would otherwise make this ambiguous.
    let ghost = insert_fixture(&db, "coffee:abgst", "abcdghost000", "coffee");
    db.soft_delete_entry(&ghost.slug).unwrap();
    let got = db.resolve_prefix("coffee:a1").unwrap();
    assert_eq!(got.slug, e.slug);
}

#[test]
fn resolve_ambiguous_prefix_errors_with_candidates() {
    let db = Db::open_in_memory().unwrap();
    let e1 = insert_fixture(&db, "coffee:abcde", "aaaabbbbcccc", "coffee");
    let e2 = insert_fixture(&db, "coffee:abxyz", "ddddeeeeffff", "coffee");

    let err = db.resolve_prefix("coffee:ab").unwrap_err();
    match err {
        DatumError::AmbiguousPrefix { prefix, candidates } => {
            assert_eq!(prefix, "coffee:ab");
            let got_slugs: std::collections::HashSet<_> =
                candidates.iter().map(|c| c.slug.clone()).collect();
            assert_eq!(
                got_slugs,
                std::collections::HashSet::from([e1.slug.clone(), e2.slug.clone()]),
            );
            let got_hids: std::collections::HashSet<_> =
                candidates.iter().map(|c| c.hid.clone()).collect();
            assert_eq!(
                got_hids,
                std::collections::HashSet::from([e1.hid.clone(), e2.hid.clone()]),
            );
        }
        other => panic!("expected AmbiguousPrefix, got {other:?}"),
    }

    // The Display of the error must surface each candidate (slug + hid)
    // so the CLI output is self-disambiguating.
    let err = db.resolve_prefix("coffee:ab").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("ambiguous"), "expected 'ambiguous' in: {msg}");
    assert!(
        msg.contains(e1.slug.as_str()),
        "candidate slug missing: {msg}"
    );
    assert!(
        msg.contains(e2.slug.as_str()),
        "candidate slug missing: {msg}"
    );
    assert!(
        msg.contains(e1.hid.as_str()),
        "candidate hid missing: {msg}"
    );
    assert!(
        msg.contains(e2.hid.as_str()),
        "candidate hid missing: {msg}"
    );
}

#[test]
fn resolve_full_slug_or_hid() {
    let db = Db::open_in_memory().unwrap();
    let e = insert_fixture(&db, "coffee:abcde", "aaaabbbbcccc", "coffee");
    // Another entry that shares the slug's field prefix but not the full slug.
    insert_fixture(&db, "coffee:qrstu", "qqqqrrrrssss", "coffee");

    // A full slug is a prefix of itself → must resolve uniquely.
    let got = db.resolve_prefix(&e.slug).unwrap();
    assert_eq!(got.slug, e.slug);

    // A full hid resolves too.
    let got = db.resolve_prefix(&e.hid).unwrap();
    assert_eq!(got.slug, e.slug);
}

#[test]
fn resolve_unknown_errors() {
    let db = Db::open_in_memory().unwrap();
    insert_fixture(&db, "coffee:abcde", "aaaabbbbcccc", "coffee");

    let err = db.resolve_prefix("nope").unwrap_err();
    assert!(
        matches!(err, DatumError::NotFound(ref s) if s == "nope"),
        "expected NotFound, got {err:?}",
    );
    // The message should read as "no such entry" for humans.
    let msg = err.to_string().to_lowercase();
    assert!(msg.contains("not found"), "expected 'not found' in: {msg}");
}

#[test]
fn resolve_prefix_escapes_like_wildcards() {
    // `_` and `%` are SQL LIKE wildcards; the prefix helper must escape them
    // so a user-typed underscore matches literally and doesn't sweep in
    // unrelated entries.
    let db = Db::open_in_memory().unwrap();
    let literal = insert_fixture(&db, "my_field:abcde", "aaaabbbbcccc", "my_field");
    // `myXfield:...` — would be matched by an un-escaped `_` wildcard.
    insert_fixture(&db, "myXfield:qrstu", "qqqqrrrrssss", "myXfield");

    let got = db.resolve_prefix("my_field:").unwrap();
    assert_eq!(got.slug, literal.slug);
}

#[test]
fn slug_collision_returns_duplicate() {
    // Two distinct entries that happen to produce identical default slugs
    // (same field + same timestamp, different content_hash). Slug
    // uniqueness is meant to enforce "one assertion per field per
    // instant"; two such inserts is essentially-duplicate data, so the
    // second insert must error rather than silently store under a
    // suffixed slug.
    let db = Db::open_in_memory().unwrap();
    let t = utc_at(9, 0);
    let a = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("a")),
        None,
        None,
        Some(t.clone()),
        None,
        "d",
    );
    let b = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("b")),
        None,
        None,
        Some(t),
        None,
        "d",
    );
    assert_eq!(a.slug, b.slug, "fixture: default slugs must collide");
    assert_ne!(a.content_hash, b.content_hash, "fixture: hashes distinct");

    db.insert_entry(&a).unwrap();
    let err = db
        .insert_entry(&b)
        .expect_err("second insert at same slug must fail");
    assert!(
        matches!(err, DatumError::Duplicate(_)),
        "expected Duplicate, got {err:?}",
    );

    // Only the first row landed; the second was rejected.
    let all = db.entries_for_field("mood", None).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].slug, a.slug);
}

#[test]
fn legacy_entries_schema_is_rejected() {
    // Databases created by pre-refactor datum carry an `id` column as
    // the entries PK. This version drops ULID; silent migration would
    // destroy identity semantics, so `Db::open` must error out instead.
    let path = std::env::temp_dir().join(format!("datum-legacy-{}.db", new_hid()));
    let path_str = path.to_str().unwrap().to_string();

    {
        let raw = Connection::open(&path_str).unwrap();
        raw.execute_batch(
            "CREATE TABLE entries (\
                id           TEXT NOT NULL PRIMARY KEY, \
                slug         TEXT NOT NULL, \
                content_hash TEXT NOT NULL, \
                field        TEXT NOT NULL, \
                verb         TEXT NOT NULL, \
                created_at   TEXT NOT NULL, \
                modified_at  TEXT NOT NULL, \
                device_id    TEXT NOT NULL\
            );",
        )
        .unwrap();
    }

    let result = Db::open(&path_str);
    let err = match result {
        Ok(_) => panic!("legacy schema must be rejected"),
        Err(e) => e,
    };
    assert!(
        matches!(err, DatumError::IncompatibleSchema(_)),
        "expected IncompatibleSchema, got {err:?}",
    );

    std::fs::remove_file(&path_str).ok();
}

// ─── D4: entry_history — acceptance tests ─────────────────────────────

/// Simple write of a fresh entry. The companion history row at version=1
/// must carry change_kind='create' with a snapshot matching the entry.
#[test]
fn insert_creates_version_1_history_row() {
    let db = Db::open_in_memory().unwrap();
    let t = DatumTime::now();
    let entry = Entry::new(
        "coffee",
        Verb::Occur,
        Some(serde_json::json!(2)),
        None,
        None,
        Some(t),
        None,
        "d1",
    );
    let hid = entry.hid.clone();
    db.insert_entry(&entry).unwrap();

    let versions = crate::history::list(db.conn(), &hid, false).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].version, 1);
    assert_eq!(versions[0].change_kind, crate::history::ChangeKind::Create);
    assert_eq!(versions[0].snapshot.hid, hid);
    assert_eq!(versions[0].snapshot.field, "coffee");
}

/// Update via collision must stamp version=2 whose snapshot reflects the
/// pre-edit state (not the incoming payload).
#[test]
fn update_creates_new_version_with_pre_edit_snapshot() {
    let db = Db::open_in_memory().unwrap();
    let t = DatumTime::now();

    let pre = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("happy")),
        None,
        None,
        Some(t.clone()),
        None,
        "d1",
    );
    let hid = pre.hid.clone();
    db.insert_entry(&pre).unwrap();

    // Same content_hash → Update strategy overwrites in place. Give the
    // incoming row a different device_id so we can see the pre-edit
    // snapshot preserves the original.
    let mut update = pre.clone();
    update.device_id = "d2".to_string();
    db.insert_entry_with_strategy(&update, ConflictStrategy::Update)
        .unwrap();

    let versions = crate::history::list(db.conn(), &hid, false).unwrap();
    assert_eq!(versions.len(), 2, "create + user_edit");
    assert_eq!(versions[0].change_kind, crate::history::ChangeKind::Create);
    assert_eq!(versions[1].version, 2);
    assert_eq!(
        versions[1].change_kind,
        crate::history::ChangeKind::UserEdit
    );
    // Snapshot at v2 = pre-edit state → device_id == d1 (the original).
    assert_eq!(versions[1].snapshot.device_id, "d1");
}

/// Retime must be recorded with change_kind='retime' and the snapshot
/// must capture the pre-shift timestamps.
#[test]
fn retime_recorded_as_change_kind_retime() {
    let db = Db::open_in_memory().unwrap();
    let t_at: DateTime<Utc> = "2024-01-01T10:00:00Z".parse().unwrap();
    let t = DatumTime {
        utc: t_at,
        offset_minutes: 0,
        timezone: "UTC".to_string(),
    };

    let e = Entry::new("nap", Verb::Occur, None, None, None, Some(t), None, "d1");
    let hid = e.hid.clone();
    db.insert_entry(&e).unwrap();

    let filter = RetimeFilter {
        slug_or_hid: Some(&hid),
        ..Default::default()
    };
    db.update_times(&filter, Duration::minutes(30)).unwrap();

    let versions = crate::history::list(db.conn(), &hid, false).unwrap();
    assert_eq!(versions.len(), 2, "create + retime");
    assert_eq!(versions[1].change_kind, crate::history::ChangeKind::Retime);
    // Pre-retime snapshot has the ORIGINAL start_time (10:00Z).
    let pre_start = versions[1].snapshot.start_time.as_ref().unwrap().utc;
    assert_eq!(pre_start, t_at);
}

/// Revert to version N-1 via `revert_entry_to` restores the pre-edit
/// state and records a new 'undo' history row.
#[test]
fn undo_reverts_to_previous_version_and_records_change() {
    let db = Db::open_in_memory().unwrap();
    let t = DatumTime::now();

    // v1: create mood=happy.
    let v1_entry = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("happy")),
        None,
        None,
        Some(t.clone()),
        None,
        "d1",
    );
    let hid = v1_entry.hid.clone();
    db.insert_entry(&v1_entry).unwrap();

    // v2: user_edit switches to mood=sad via Update strategy (same slug).
    // Preserve the identifying fields so the content_hash still collides.
    let mut v2_entry = v1_entry.clone();
    v2_entry.value = Some(serde_json::json!("sad"));
    v2_entry.content_hash = v1_entry.content_hash.clone();
    db.insert_entry_with_strategy(&v2_entry, ConflictStrategy::Update)
        .unwrap();

    // Sanity: current state is v2 (mood=sad).
    let before = db.get_entry(&v1_entry.slug).unwrap();
    assert_eq!(before.value, Some(serde_json::json!("sad")));

    // Revert to v1.
    let restored = db.revert_entry_to(&hid, 1).unwrap();
    assert_eq!(restored.value, Some(serde_json::json!("happy")));

    // History now has three rows; the newest is the undo.
    let versions = crate::history::list(db.conn(), &hid, false).unwrap();
    assert_eq!(versions.len(), 3);
    assert_eq!(versions[2].change_kind, crate::history::ChangeKind::Undo);
    // Pre-undo snapshot captures the mood=sad state we just left.
    assert_eq!(versions[2].snapshot.value, Some(serde_json::json!("sad")));
}

/// `--to-version N` points at an arbitrary version, not just N-1.
#[test]
fn undo_to_specific_version_works() {
    let db = Db::open_in_memory().unwrap();
    let t = DatumTime::now();

    let v1_entry = Entry::new(
        "mood",
        Verb::Switch,
        Some(serde_json::json!("happy")),
        None,
        None,
        Some(t.clone()),
        None,
        "d1",
    );
    let hid = v1_entry.hid.clone();
    let slug = v1_entry.slug.clone();
    db.insert_entry(&v1_entry).unwrap();

    // v2: sad
    let mut v2 = v1_entry.clone();
    v2.value = Some(serde_json::json!("sad"));
    db.insert_entry_with_strategy(&v2, ConflictStrategy::Update)
        .unwrap();

    // v3: focused
    let mut v3 = v1_entry.clone();
    v3.value = Some(serde_json::json!("focused"));
    db.insert_entry_with_strategy(&v3, ConflictStrategy::Update)
        .unwrap();

    // Revert directly to v1 by id_or_slug.
    let restored = db.revert_entry_to(&slug, 1).unwrap();
    assert_eq!(restored.value, Some(serde_json::json!("happy")));

    // History length should be 4 now (create, edit, edit, undo).
    let versions = crate::history::list(db.conn(), &hid, false).unwrap();
    assert_eq!(versions.len(), 4);
    assert_eq!(versions[3].change_kind, crate::history::ChangeKind::Undo);
}

/// Bulk fast path inserts every row but writes zero `entry_history` rows
/// and zero `sync_conflicts` rows — both are deliberately skipped to
/// keep migration imports off the per-row hot path.
#[test]
fn insert_entries_bulk_skips_history_and_conflicts() {
    let db = Db::open_in_memory().unwrap();
    let t0 = DatumTime::now();

    // Two overlapping blocks on the same field. Per-row conflict
    // detection would file an overlap conflict; the bulk path must
    // skip that and write a clean entries-only state.
    let block_a = Entry::new(
        "sleep",
        Verb::Block,
        None,
        None,
        None,
        Some(t0.clone()),
        Some(DatumTime {
            utc: t0.utc + chrono::Duration::hours(1),
            ..t0.clone()
        }),
        "d1",
    );
    let block_b = Entry::new(
        "sleep",
        Verb::Block,
        None,
        None,
        None,
        Some(DatumTime {
            utc: t0.utc + chrono::Duration::minutes(30),
            ..t0.clone()
        }),
        Some(DatumTime {
            utc: t0.utc + chrono::Duration::minutes(90),
            ..t0.clone()
        }),
        "d1",
    );
    let hid_a = block_a.hid.clone();
    let hid_b = block_b.hid.clone();

    let outcomes = db
        .insert_entries_bulk(&[block_a, block_b], ConflictStrategy::Ignore)
        .unwrap();
    assert_eq!(outcomes.len(), 2);
    assert!(matches!(outcomes[0], InsertOutcome::Inserted { .. }));
    assert!(matches!(outcomes[1], InsertOutcome::Inserted { .. }));

    // No history rows.
    let h_a = crate::history::list(db.conn(), &hid_a, false).unwrap();
    let h_b = crate::history::list(db.conn(), &hid_b, false).unwrap();
    assert!(h_a.is_empty(), "bulk path must not write history rows");
    assert!(h_b.is_empty(), "bulk path must not write history rows");

    // No sync_conflicts rows even though the two blocks overlap.
    let conflict_count: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM sync_conflicts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        conflict_count, 0,
        "bulk path must not run per-row conflict detection",
    );

    // But the entries themselves are present.
    let n: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(n, 2);
}

/// Re-importing the same entries dedups by `content_hash` — the second
/// pass returns `Ignored` outcomes and does not double-write.
#[test]
fn insert_entries_bulk_idempotent_on_content_hash() {
    let db = Db::open_in_memory().unwrap();
    let t = DatumTime::now();
    let entry = Entry::new(
        "coffee",
        Verb::Occur,
        Some(serde_json::json!(1)),
        None,
        Some("cup".to_string()),
        Some(t),
        None,
        "d1",
    );

    let first = db
        .insert_entries_bulk(&[entry.clone()], ConflictStrategy::Ignore)
        .unwrap();
    assert!(matches!(first[0], InsertOutcome::Inserted { .. }));

    let second = db
        .insert_entries_bulk(&[entry], ConflictStrategy::Ignore)
        .unwrap();
    assert!(matches!(second[0], InsertOutcome::Ignored { .. }));

    let n: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM entries WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(n, 1, "re-import must not duplicate rows");
}

/// da-tsw: `extra_metadata` round-trips through insert and read-back as
/// arbitrary JSON, completely separate from `data`.
#[test]
fn extra_metadata_round_trips_via_get_metadata() {
    let db = Db::open_in_memory().unwrap();
    let t = DatumTime::now();
    let mut entry = Entry::new(
        "coffee",
        Verb::Occur,
        Some(serde_json::json!(1)),
        Some(serde_json::json!({"mug": "blue"})),
        None,
        Some(t),
        None,
        "d1",
    );
    entry.extra_metadata = Some(serde_json::json!({
        "_import": {"source": "datum-v2-couchdb", "original_id": "abc"}
    }));
    db.insert_entry(&entry).unwrap();

    let fetched = db.get_entry(&entry.slug).unwrap();
    assert_eq!(
        fetched.extra_metadata.as_ref().unwrap()["_import"]["source"],
        serde_json::json!("datum-v2-couchdb"),
    );
    assert_eq!(
        fetched.data.as_ref().unwrap()["mug"],
        serde_json::json!("blue"),
        "user payload survives untouched alongside extra_metadata",
    );

    let direct = db.get_metadata(&entry.slug).unwrap().unwrap();
    assert_eq!(direct["_import"]["original_id"], serde_json::json!("abc"));

    // set_metadata overwrites without touching data, modify_time, or hash.
    let pre_modify = fetched.modify_time;
    let pre_hash = fetched.content_hash.clone();
    db.set_metadata(&entry.slug, Some(&serde_json::json!({"sync": "merged"})))
        .unwrap();
    let after = db.get_entry(&entry.slug).unwrap();
    assert_eq!(
        after.extra_metadata.as_ref().unwrap()["sync"],
        serde_json::json!("merged"),
    );
    assert_eq!(after.modify_time, pre_modify, "modify_time must NOT change");
    assert_eq!(after.content_hash, pre_hash, "content_hash must NOT change");
    assert_eq!(
        after.data.as_ref().unwrap()["mug"],
        serde_json::json!("blue"),
        "set_metadata must NOT touch data",
    );

    // Clear with None.
    db.set_metadata(&entry.slug, None).unwrap();
    assert!(db.get_metadata(&entry.slug).unwrap().is_none());

    // Missing rows error from set_metadata.
    let err = db.set_metadata("no-such-row", None).unwrap_err();
    assert!(matches!(err, DatumError::NotFound(_)));
}

/// Non-Ignore strategies are deliberately rejected — the fast path is
/// narrow on purpose and live writes should keep using the full
/// `insert_entries_batch` API.
#[test]
fn insert_entries_bulk_rejects_non_ignore_strategy() {
    let db = Db::open_in_memory().unwrap();
    let t = DatumTime::now();
    let entry = Entry::new(
        "coffee",
        Verb::Occur,
        Some(serde_json::json!(1)),
        None,
        Some("cup".to_string()),
        Some(t),
        None,
        "d1",
    );
    for s in [
        ConflictStrategy::Update,
        ConflictStrategy::Merge,
        ConflictStrategy::Rekey,
    ] {
        let err = db.insert_entries_bulk(&[entry.clone()], s).unwrap_err();
        assert!(
            matches!(err, DatumError::Other(ref m) if m.contains("Ignore")),
            "expected 'Ignore-only' error for {s:?}, got {err:?}",
        );
    }
}

// ─── Kit (da-d4t.1) ─────────────────────────────────────────────────────

#[test]
fn install_kit_persists_specs_with_kit_id() {
    let db = Db::open_in_memory().unwrap();
    let mut sleep = FieldSpec::new("sleep", Verb::Start);
    sleep.color = Some("#1a3d6e".into());
    sleep.regenerate_derived_aliases();
    let mut coffee = FieldSpec::new("coffee", Verb::Occur);
    coffee.regenerate_derived_aliases();

    let report = db
        .install_kit("core", &[sleep.clone(), coffee.clone()], &[], false)
        .unwrap();
    let installed: Vec<&str> = report.installed().collect();
    assert_eq!(installed, vec!["sleep", "coffee"]);
    assert_eq!(report.skipped().count(), 0);

    let kits = db.list_kits().unwrap();
    assert_eq!(kits.len(), 1);
    assert_eq!(kits[0].kit_id, "core");
    assert_eq!(kits[0].field_count, 2);

    let by_kit = db.load_specs_by_kit("core").unwrap();
    assert_eq!(by_kit.len(), 2);
    let names: Vec<&str> = by_kit.iter().map(|s| s.field.as_str()).collect();
    assert_eq!(names, vec!["coffee", "sleep"]); // alphabetical
}

#[test]
fn install_kit_skips_existing_field_without_force() {
    let db = Db::open_in_memory().unwrap();
    let mut existing = FieldSpec::new("sleep", Verb::Start);
    existing.color = Some("#user000".into());
    db.save_field_spec(&existing).unwrap();

    let mut kit_sleep = FieldSpec::new("sleep", Verb::Start);
    kit_sleep.color = Some("#kit0000".into());
    let report = db.install_kit("naps", &[kit_sleep], &[], false).unwrap();
    let skipped: Vec<&str> = report.skipped().collect();
    assert_eq!(skipped, vec!["sleep"]);

    // Existing color preserved; row not tagged with kit_id.
    let loaded = db.load_spec("sleep").unwrap().unwrap();
    assert_eq!(loaded.color.as_deref(), Some("#user000"));
    let kits = db.list_kits().unwrap();
    assert_eq!(kits.len(), 0, "skipped row should not register a kit");
}

#[test]
fn install_kit_force_overwrites_and_retags() {
    let db = Db::open_in_memory().unwrap();
    let mut existing = FieldSpec::new("sleep", Verb::Start);
    existing.color = Some("#user000".into());
    db.save_field_spec(&existing).unwrap();

    let mut kit_sleep = FieldSpec::new("sleep", Verb::Start);
    kit_sleep.color = Some("#kit0000".into());
    let report = db.install_kit("naps", &[kit_sleep], &[], true).unwrap();
    let installed: Vec<&str> = report.installed().collect();
    assert_eq!(installed, vec!["sleep"]);

    let loaded = db.load_spec("sleep").unwrap().unwrap();
    assert_eq!(loaded.color.as_deref(), Some("#kit0000"));
    let kits = db.list_kits().unwrap();
    assert_eq!(kits.len(), 1);
    assert_eq!(kits[0].kit_id, "naps");
}

#[test]
fn uninstall_kit_removes_only_owned_rows() {
    let db = Db::open_in_memory().unwrap();
    let mut user_field = FieldSpec::new("custom", Verb::Switch);
    user_field.allowed_states = Some(vec!["a".into(), "b".into()]);
    user_field.regenerate_derived_aliases();
    db.save_field_spec(&user_field).unwrap();

    let mut k1 = FieldSpec::new("sleep", Verb::Start);
    k1.regenerate_derived_aliases();
    let mut k2 = FieldSpec::new("coffee", Verb::Occur);
    k2.regenerate_derived_aliases();
    db.install_kit("core", &[k1, k2], &[], false).unwrap();

    let removed = db.uninstall_kit("core").unwrap();
    assert_eq!(removed, vec!["coffee", "sleep"]);

    // User-declared field still there.
    assert!(db.load_spec("custom").unwrap().is_some());
    assert!(db.load_spec("sleep").unwrap().is_none());
    assert!(db.load_spec("coffee").unwrap().is_none());

    // Derived aliases for the kit's fields are cleaned up; the user's
    // state aliases survive.
    let reg = db.load_registry().unwrap();
    assert!(reg.aliases.contains_key("a"));
    assert!(!reg.aliases.contains_key("sleep"));
    assert!(!reg.aliases.contains_key("coffee"));
}

#[test]
fn uninstall_unknown_kit_is_noop() {
    let db = Db::open_in_memory().unwrap();
    let removed = db.uninstall_kit("ghost").unwrap();
    assert!(removed.is_empty());
}

#[test]
fn save_field_spec_preserves_kit_id() {
    let db = Db::open_in_memory().unwrap();
    let mut spec = FieldSpec::new("sleep", Verb::Start);
    spec.color = Some("#000000".into());
    spec.regenerate_derived_aliases();
    db.install_kit("core", &[spec.clone()], &[], false).unwrap();

    // User edits the spec via `datum spec set` — should NOT lose kit_id.
    let mut edited = spec.clone();
    edited.color = Some("#ffffff".into());
    db.save_field_spec(&edited).unwrap();

    let kits = db.list_kits().unwrap();
    assert_eq!(kits.len(), 1, "save_field_spec must not strip kit_id");
    assert_eq!(kits[0].kit_id, "core");
    let loaded = db.load_spec("sleep").unwrap().unwrap();
    assert_eq!(loaded.color.as_deref(), Some("#ffffff"));
}

// ─── Kit views (da-d4t.4) ───────────────────────────────────────────────

#[test]
fn install_kit_views_persists_with_kit_id() {
    let db = Db::open_in_memory().unwrap();
    let views = vec![
        crate::kit::ViewEntry {
            name: "field_counts".into(),
            sql: "SELECT field, COUNT(*) AS n FROM entries GROUP BY field".into(),
        },
        crate::kit::ViewEntry {
            name: "all_one".into(),
            sql: "SELECT 1 AS v".into(),
        },
    ];
    let report = db.install_kit_views("core", &views, false).unwrap();
    let installed: Vec<&str> = report.installed().collect();
    assert_eq!(installed, vec!["field_counts", "all_one"]);
    assert_eq!(report.skipped().count(), 0);

    let listed = db.list_views().unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|v| v.kit_id.as_deref() == Some("core")));

    // List_kits picks up a views-only kit even with no fields.
    let kits = db.list_kits().unwrap();
    assert_eq!(kits.len(), 1);
    assert_eq!(kits[0].kit_id, "core");
    assert_eq!(kits[0].field_count, 0);
    assert_eq!(kits[0].view_count, 2);
}

#[test]
fn install_kit_views_skips_existing_without_force() {
    let db = Db::open_in_memory().unwrap();
    // Pre-existing user view (kit_id NULL).
    db.conn()
        .execute(
            "INSERT INTO views (name, sql_text, kit_id) VALUES (?1, ?2, NULL)",
            rusqlite::params!["mine", "SELECT 1"],
        )
        .unwrap();

    let views = vec![crate::kit::ViewEntry {
        name: "mine".into(),
        sql: "SELECT 2".into(),
    }];
    let report = db.install_kit_views("core", &views, false).unwrap();
    let skipped: Vec<&str> = report.skipped().collect();
    assert_eq!(skipped, vec!["mine"]);

    // Existing SQL preserved; row not retagged.
    let row = db.get_view("mine").unwrap().unwrap();
    assert_eq!(row.sql_text, "SELECT 1");
    assert_eq!(row.kit_id, None);
}

#[test]
fn install_kit_views_force_overwrites_and_retags() {
    let db = Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO views (name, sql_text, kit_id) VALUES (?1, ?2, NULL)",
            rusqlite::params!["mine", "SELECT 1"],
        )
        .unwrap();

    let views = vec![crate::kit::ViewEntry {
        name: "mine".into(),
        sql: "SELECT 2 AS v".into(),
    }];
    let report = db.install_kit_views("core", &views, true).unwrap();
    let installed: Vec<&str> = report.installed().collect();
    assert_eq!(installed, vec!["mine"]);

    let row = db.get_view("mine").unwrap().unwrap();
    assert_eq!(row.sql_text, "SELECT 2 AS v");
    assert_eq!(row.kit_id.as_deref(), Some("core"));
}

#[test]
fn uninstall_kit_views_removes_only_owned_rows() {
    let db = Db::open_in_memory().unwrap();
    // User view + kit view.
    db.conn()
        .execute(
            "INSERT INTO views (name, sql_text, kit_id) VALUES (?1, ?2, NULL)",
            rusqlite::params!["user_one", "SELECT 1"],
        )
        .unwrap();
    db.install_kit_views(
        "core",
        &[crate::kit::ViewEntry {
            name: "kit_one".into(),
            sql: "SELECT 2".into(),
        }],
        false,
    )
    .unwrap();

    let removed = db.uninstall_kit_views("core").unwrap();
    assert_eq!(removed, vec!["kit_one"]);
    assert!(db.get_view("kit_one").unwrap().is_none());
    // User view untouched.
    assert!(db.get_view("user_one").unwrap().is_some());
}

#[test]
fn execute_view_returns_columns_and_rows() {
    let db = Db::open_in_memory().unwrap();
    let entry = Entry::new(
        "coffee",
        Verb::Occur,
        Some(JsonValue::from(2)),
        None,
        None,
        Some(DatumTime::now()),
        None,
        "test",
    );
    let _ = db.insert_entry(&entry).unwrap();

    let result = db
        .execute_view("SELECT field, COUNT(*) AS n FROM entries GROUP BY field")
        .unwrap();
    assert_eq!(result.columns, vec!["field".to_string(), "n".to_string()]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0][0], JsonValue::String("coffee".into()));
    assert_eq!(result.rows[0][1], JsonValue::from(1));
}

#[test]
fn execute_view_blocks_writes_via_query_only() {
    let db = Db::open_in_memory().unwrap();
    // INSERT against entries should fail under PRAGMA query_only=1 — that's
    // the whole point of the read-only enforcement.
    let err = db
        .execute_view(
            "INSERT INTO entries (hid, slug, content_hash, field, entry_shape) \
             VALUES ('h','s','c','f','point')",
        )
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("read")
            || msg.to_lowercase().contains("readonly")
            || msg.contains("query_only"),
        "expected read-only rejection, got: {msg}"
    );

    // After the failed query, writes through the normal path still work —
    // the PRAGMA was reset.
    let entry = Entry::new(
        "coffee",
        Verb::Occur,
        None,
        None,
        None,
        Some(DatumTime::now()),
        None,
        "test",
    );
    db.insert_entry(&entry).unwrap();
}

#[test]
fn execute_view_rejects_empty_sql() {
    let db = Db::open_in_memory().unwrap();
    let err = db.execute_view("   \n  ").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("empty"), "expected empty error, got: {msg}");
}

#[test]
fn load_views_by_kit_round_trips_for_export() {
    let db = Db::open_in_memory().unwrap();
    let views = vec![
        crate::kit::ViewEntry {
            name: "b".into(),
            sql: "SELECT 2".into(),
        },
        crate::kit::ViewEntry {
            name: "a".into(),
            sql: "SELECT 1".into(),
        },
    ];
    db.install_kit_views("core", &views, false).unwrap();
    let loaded = db.load_views_by_kit("core").unwrap();
    let names: Vec<&str> = loaded.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b"], "results should be ordered by name");
    assert_eq!(loaded[0].sql, "SELECT 1");
    assert_eq!(loaded[1].sql, "SELECT 2");
}

// ─── Kit aliases (da-d4t.3) ─────────────────────────────────────────────

#[test]
fn install_kit_aliases_persists_with_kit_id() {
    let db = Db::open_in_memory().unwrap();
    let aliases = vec![
        crate::kit::AliasEntry {
            name: "chore".into(),
            expands_to: "occur -k next_date".into(),
            description: None,
        },
        crate::kit::AliasEntry {
            name: "done".into(),
            expands_to: "occur".into(),
            description: Some("mark a chore done".into()),
        },
    ];
    let report = db.install_kit_aliases("core", &aliases, false).unwrap();
    let installed: Vec<&str> = report.installed().collect();
    assert_eq!(installed, vec!["chore", "done"]);
    assert_eq!(report.skipped().count(), 0);

    // Round-trips through lookup_alias with the kit's expansion text.
    let chore = db.lookup_alias("chore").unwrap().unwrap();
    assert_eq!(chore.expands_to, "occur -k next_date");
    let done = db.lookup_alias("done").unwrap().unwrap();
    assert_eq!(done.description.as_deref(), Some("mark a chore done"));

    // List_kits picks up an aliases-only kit even with no fields/views.
    let kits = db.list_kits().unwrap();
    assert_eq!(kits.len(), 1);
    assert_eq!(kits[0].kit_id, "core");
    assert_eq!(kits[0].field_count, 0);
    assert_eq!(kits[0].view_count, 0);
    assert_eq!(kits[0].alias_count, 2);
}

#[test]
fn install_kit_aliases_skips_existing_user_alias_without_force() {
    let db = Db::open_in_memory().unwrap();
    // Pre-existing user-declared alias.
    db.save_alias(&crate::field_spec::Alias {
        name: "vbp".into(),
        expands_to: "note word srcLang=pt pos=verb".into(),
        description: None,
    })
    .unwrap();

    let aliases = vec![crate::kit::AliasEntry {
        name: "vbp".into(),
        expands_to: "occur".into(),
        description: None,
    }];
    let report = db.install_kit_aliases("kit-x", &aliases, false).unwrap();
    let skipped: Vec<&str> = report.skipped().collect();
    assert_eq!(skipped, vec!["vbp"]);

    // User's alias preserved; row not retagged.
    let row = db.lookup_alias("vbp").unwrap().unwrap();
    assert_eq!(row.expands_to, "note word srcLang=pt pos=verb");
    let owner: Option<String> = db
        .conn()
        .query_row("SELECT kit_id FROM aliases WHERE name = 'vbp'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(owner, None, "user alias must keep kit_id NULL");
}

#[test]
fn install_kit_aliases_force_overwrites_and_retags() {
    let db = Db::open_in_memory().unwrap();
    db.save_alias(&crate::field_spec::Alias {
        name: "vbp".into(),
        expands_to: "old expansion".into(),
        description: None,
    })
    .unwrap();

    let aliases = vec![crate::kit::AliasEntry {
        name: "vbp".into(),
        expands_to: "new expansion".into(),
        description: None,
    }];
    let report = db.install_kit_aliases("kit-x", &aliases, true).unwrap();
    let installed: Vec<&str> = report.installed().collect();
    assert_eq!(installed, vec!["vbp"]);

    let row = db.lookup_alias("vbp").unwrap().unwrap();
    assert_eq!(row.expands_to, "new expansion");
    let owner: Option<String> = db
        .conn()
        .query_row("SELECT kit_id FROM aliases WHERE name = 'vbp'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(owner.as_deref(), Some("kit-x"));
}

#[test]
fn uninstall_kit_aliases_removes_only_owned_rows() {
    let db = Db::open_in_memory().unwrap();
    db.save_alias(&crate::field_spec::Alias {
        name: "user_one".into(),
        expands_to: "occur".into(),
        description: None,
    })
    .unwrap();
    db.install_kit_aliases(
        "core",
        &[crate::kit::AliasEntry {
            name: "kit_one".into(),
            expands_to: "occur".into(),
            description: None,
        }],
        false,
    )
    .unwrap();

    let removed = db.uninstall_kit_aliases("core").unwrap();
    assert_eq!(removed, vec!["kit_one"]);
    assert!(db.lookup_alias("kit_one").unwrap().is_none());
    // User alias untouched.
    assert!(db.lookup_alias("user_one").unwrap().is_some());
}

#[test]
fn load_aliases_by_kit_round_trips_for_export() {
    let db = Db::open_in_memory().unwrap();
    let aliases = vec![
        crate::kit::AliasEntry {
            name: "b".into(),
            expands_to: "occur b".into(),
            description: None,
        },
        crate::kit::AliasEntry {
            name: "a".into(),
            expands_to: "occur a".into(),
            description: Some("first".into()),
        },
    ];
    db.install_kit_aliases("core", &aliases, false).unwrap();
    let loaded = db.load_aliases_by_kit("core").unwrap();
    let names: Vec<&str> = loaded.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b"], "results should be ordered by name");
    assert_eq!(loaded[0].expands_to, "occur a");
    assert_eq!(loaded[0].description.as_deref(), Some("first"));
    assert_eq!(loaded[1].expands_to, "occur b");
}

#[test]
fn install_kit_aliases_preserves_expansion_byte_for_byte() {
    let db = Db::open_in_memory().unwrap();
    // Whitespace and special chars survive the JSON round-trip exactly.
    let raw = "  occur  -k  next_date=2024-01-01T09:00Z  ";
    let aliases = vec![crate::kit::AliasEntry {
        name: "weirdspaces".into(),
        expands_to: raw.to_string(),
        description: None,
    }];
    db.install_kit_aliases("core", &aliases, false).unwrap();
    let loaded = db.load_aliases_by_kit("core").unwrap();
    assert_eq!(loaded[0].expands_to, raw);
}

#[test]
fn install_kit_command_level1_round_trip() {
    use crate::kit::CommandEntry;
    let db = Db::open_in_memory().unwrap();
    let cmds = vec![CommandEntry {
        name: "chore".to_string(),
        expands_to: Some("occur chore -k next_date".to_string()),
        view: None,
        help: Some("log a chore".to_string()),
        columns: vec![],
        labels: Default::default(),
        formats: Default::default(),
        sort: None,
        flags: vec![],
        color_rules: vec![],
        script: None,
    }];
    let report = db.install_kit_commands("chores", &cmds, false).unwrap();
    let names: Vec<&str> = report.installed().collect();
    assert_eq!(names, vec!["chore"]);

    let row = db.get_kit_command("chore").unwrap().unwrap();
    assert_eq!(row.kit_id.as_deref(), Some("chores"));
    assert_eq!(
        row.expands_to.as_deref(),
        Some("occur chore -k next_date")
    );
    assert!(row.view_name.is_none());
    assert_eq!(row.help.as_deref(), Some("log a chore"));

    let exported = db.load_commands_by_kit("chores").unwrap();
    assert_eq!(exported.len(), 1);
    assert_eq!(exported[0].name, "chore");
    assert_eq!(exported[0].expands_to.as_deref(), Some("occur chore -k next_date"));
}

#[test]
fn install_kit_command_level2_round_trip() {
    use crate::kit::{ColorRuleEntry, CommandEntry};
    let db = Db::open_in_memory().unwrap();
    let mut labels = std::collections::BTreeMap::new();
    labels.insert("next_due_raw".to_string(), "due".to_string());
    let mut formats = std::collections::BTreeMap::new();
    formats.insert("next_due_raw".to_string(), "smart_date".to_string());

    let cmds = vec![CommandEntry {
        name: "chorelist".to_string(),
        expands_to: None,
        view: Some("chores_due".to_string()),
        help: Some("list chores by next-due date".to_string()),
        columns: vec!["field".into(), "next_due_raw".into(), "iti".into()],
        labels,
        formats,
        sort: Some("next_due_raw".into()),
        flags: vec!["watch".into()],
        color_rules: vec![
            ColorRuleEntry {
                when: "next_due_raw < now".into(),
                color: "red".into(),
            },
            ColorRuleEntry {
                when: "next_due_raw < today + 1d".into(),
                color: "yellow".into(),
            },
        ],
        script: None,
    }];
    db.install_kit_commands("chores", &cmds, false).unwrap();

    let row = db.get_kit_command("chorelist").unwrap().unwrap();
    assert_eq!(row.view_name.as_deref(), Some("chores_due"));
    assert_eq!(row.columns, vec!["field".to_string(), "next_due_raw".into(), "iti".into()]);
    assert_eq!(row.sort_col.as_deref(), Some("next_due_raw"));
    assert_eq!(row.flags, vec!["watch".to_string()]);
    assert_eq!(row.color_rules.len(), 2);
    assert_eq!(row.color_rules[0].color, "red");

    let exported = db.load_commands_by_kit("chores").unwrap();
    assert_eq!(exported.len(), 1);
    assert_eq!(exported[0].view.as_deref(), Some("chores_due"));
    assert_eq!(exported[0].sort.as_deref(), Some("next_due_raw"));
}

#[test]
fn install_kit_command_rejects_script() {
    use crate::kit::CommandEntry;
    let db = Db::open_in_memory().unwrap();
    let cmds = vec![CommandEntry {
        name: "doit".into(),
        expands_to: None,
        view: None,
        help: None,
        columns: vec![],
        labels: Default::default(),
        formats: Default::default(),
        sort: None,
        flags: vec![],
        color_rules: vec![],
        script: Some("println('nope')".into()),
    }];
    let err = db.install_kit_commands("k", &cmds, false).unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("Level 3"), "got: {msg}");
}

#[test]
fn install_kit_command_rejects_both_or_neither() {
    use crate::kit::CommandEntry;
    let db = Db::open_in_memory().unwrap();
    // neither
    let cmds = vec![CommandEntry {
        name: "x".into(),
        expands_to: None,
        view: None,
        help: None,
        columns: vec![],
        labels: Default::default(),
        formats: Default::default(),
        sort: None,
        flags: vec![],
        color_rules: vec![],
        script: None,
    }];
    let err = db.install_kit_commands("k", &cmds, false).unwrap_err();
    assert!(format!("{err}").contains("exactly one"));

    // both
    let cmds = vec![CommandEntry {
        name: "x".into(),
        expands_to: Some("occur x".into()),
        view: Some("v".into()),
        help: None,
        columns: vec![],
        labels: Default::default(),
        formats: Default::default(),
        sort: None,
        flags: vec![],
        color_rules: vec![],
        script: None,
    }];
    let err = db.install_kit_commands("k", &cmds, false).unwrap_err();
    assert!(format!("{err}").contains("exactly one"));
}

#[test]
fn install_kit_command_rejects_bad_color_rule() {
    use crate::kit::{ColorRuleEntry, CommandEntry};
    let db = Db::open_in_memory().unwrap();
    let cmds = vec![CommandEntry {
        name: "list".into(),
        expands_to: None,
        view: Some("v".into()),
        help: None,
        columns: vec![],
        labels: Default::default(),
        formats: Default::default(),
        sort: None,
        flags: vec![],
        color_rules: vec![ColorRuleEntry {
            when: "trash garbage(x)".into(),
            color: "red".into(),
        }],
        script: None,
    }];
    let err = db.install_kit_commands("k", &cmds, false).unwrap_err();
    assert!(format!("{err}").contains("color rule"));
}

#[test]
fn uninstall_kit_commands_removes_only_owned_rows() {
    use crate::kit::CommandEntry;
    let db = Db::open_in_memory().unwrap();
    let cmds_a = vec![CommandEntry {
        name: "a_cmd".into(),
        expands_to: Some("occur a".into()),
        view: None,
        help: None,
        columns: vec![],
        labels: Default::default(),
        formats: Default::default(),
        sort: None,
        flags: vec![],
        color_rules: vec![],
        script: None,
    }];
    let cmds_b = vec![CommandEntry {
        name: "b_cmd".into(),
        expands_to: Some("occur b".into()),
        view: None,
        help: None,
        columns: vec![],
        labels: Default::default(),
        formats: Default::default(),
        sort: None,
        flags: vec![],
        color_rules: vec![],
        script: None,
    }];
    db.install_kit_commands("kit_a", &cmds_a, false).unwrap();
    db.install_kit_commands("kit_b", &cmds_b, false).unwrap();

    let removed = db.uninstall_kit_commands("kit_a").unwrap();
    assert_eq!(removed, vec!["a_cmd".to_string()]);
    assert!(db.get_kit_command("a_cmd").unwrap().is_none());
    assert!(db.get_kit_command("b_cmd").unwrap().is_some());
}

#[test]
fn list_kits_includes_command_count() {
    use crate::kit::CommandEntry;
    let db = Db::open_in_memory().unwrap();
    let cmds = vec![
        CommandEntry {
            name: "c1".into(),
            expands_to: Some("occur a".into()),
            view: None,
            help: None,
            columns: vec![],
            labels: Default::default(),
            formats: Default::default(),
            sort: None,
            flags: vec![],
            color_rules: vec![],
            script: None,
        },
        CommandEntry {
            name: "c2".into(),
            expands_to: Some("occur b".into()),
            view: None,
            help: None,
            columns: vec![],
            labels: Default::default(),
            formats: Default::default(),
            sort: None,
            flags: vec![],
            color_rules: vec![],
            script: None,
        },
    ];
    db.install_kit_commands("kx", &cmds, false).unwrap();
    let kits = db.list_kits().unwrap();
    let kx = kits.iter().find(|k| k.kit_id == "kx").unwrap();
    assert_eq!(kx.command_count, 2);
    assert_eq!(kx.field_count, 0);
    assert_eq!(kx.view_count, 0);
}
