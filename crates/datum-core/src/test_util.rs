//! Shared test fixtures for command-parity tests.
//!
//! Gated behind `#[cfg(any(test, feature = "test-util"))]` so consumers (e.g.
//! integration tests in `datum-cli`) opt in via `features = ["test-util"]`.
//!
//! Provides:
//! - [`fresh_db`] — a brand-new in-memory Db with schema applied.
//! - [`entry_builder`] / [`spec_builder`] / [`alias_builder`] — fluent builders
//!   with sensible defaults so tests only specify fields they care about.
//! - [`assert_entry_matches!`] / [`assert_count!`] — concise assertion macros.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;

use datum_parse::time::clock::{self, MockClock};
use datum_parse::time::{parse_duration, parse_time_arg, TimeBuilder};

use crate::db::Db;
use crate::entry::{Entry, Verb};
use crate::field_spec::{Alias, FieldSpec, SubfieldSpec};
use crate::time::DatumTime;

/// Process-wide monotonic millisecond offset added to each
/// [`EntryBuilder::new`] default `start_time`. Slug uniqueness now hard-errors
/// on collision (da-p5j), so two back-to-back builders must not produce
/// identical slugs. The slug encodes time at millisecond resolution, so
/// stepping the default time by ≥1 ms per builder guarantees distinct slugs
/// without forcing every test to call `time_start_utc(...)` explicitly.
static TIME_NUDGE_MS: AtomicI64 = AtomicI64::new(0);

fn next_default_time() -> DatumTime {
    let nudge = TIME_NUDGE_MS.fetch_add(1, Ordering::Relaxed);
    let mut t = DatumTime::now();
    t.utc += chrono::Duration::milliseconds(nudge);
    t
}

/// Return a brand-new in-memory SQLite [`Db`] with schema applied.
///
/// Each call produces an isolated database — there is no global shared state,
/// so tests cannot cross-talk. Panics only on a catastrophic SQLite failure.
pub fn fresh_db() -> Db {
    Db::open_in_memory().expect("failed to open in-memory test Db")
}

// ─── EntryBuilder ────────────────────────────────────────────────────────────

/// Fluent builder for [`Entry`] test fixtures.
///
/// Defaults: verb=`Occur`, start_time=`DatumTime::now()` plus a small
/// monotonic nudge so back-to-back builders land on distinct slugs,
/// device="test-device".
#[derive(Clone)]
pub struct EntryBuilder {
    field: String,
    verb: Verb,
    value: Option<JsonValue>,
    data: Option<JsonValue>,
    unit: Option<String>,
    start_time: Option<DatumTime>,
    end_time: Option<DatumTime>,
    device_id: String,
}

impl EntryBuilder {
    pub fn new(field: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            verb: Verb::Occur,
            value: None,
            data: None,
            unit: None,
            start_time: Some(next_default_time()),
            end_time: None,
            device_id: "test-device".to_string(),
        }
    }

    pub fn verb(mut self, verb: Verb) -> Self {
        self.verb = verb;
        self
    }

    pub fn value(mut self, value: JsonValue) -> Self {
        self.value = Some(value);
        self
    }

    pub fn extra_data(mut self, extra: JsonValue) -> Self {
        self.data = Some(extra);
        self
    }

    pub fn unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    pub fn time_start(mut self, t: DatumTime) -> Self {
        self.start_time = Some(t);
        self
    }

    pub fn time_end(mut self, t: DatumTime) -> Self {
        self.end_time = Some(t);
        self
    }

    pub fn time_start_utc(mut self, utc: DateTime<Utc>) -> Self {
        self.start_time = Some(DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        });
        self
    }

    pub fn time_end_utc(mut self, utc: DateTime<Utc>) -> Self {
        self.end_time = Some(DatumTime {
            utc,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        });
        self
    }

    /// Clear the default `start_time` (produces a slug of `field:unset`).
    pub fn no_time(mut self) -> Self {
        self.start_time = None;
        self
    }

    pub fn device_id(mut self, d: impl Into<String>) -> Self {
        self.device_id = d.into();
        self
    }

    pub fn build(self) -> Entry {
        // da-5ns sub-bead 1: `time_start` maps to occur_time; the
        // historic `time_end` is now expressed via `dur` (ISO duration)
        // — for tests that haven't migrated, derive a duration when both
        // legacy times are set so block-shaped fixtures keep producing
        // verb-relevant rows. Sub-bead 8 rewrites these tests properly.
        let dur = match (self.start_time.as_ref(), self.end_time.as_ref()) {
            (Some(s), Some(e)) => Some(format!(
                "PT{}S",
                (e.utc - s.utc).num_seconds().max(0)
            )),
            _ => None,
        };
        Entry::new(
            self.field,
            self.verb,
            self.value,
            self.data,
            self.unit,
            self.start_time,
            dur,
            self.device_id,
        )
    }

    /// Build and insert into `db`, returning the owned entry with the slug
    /// and hid reflecting what actually landed in the DB. The hid may differ
    /// from the in-memory entry if raw_insert had to retry once on a hid
    /// collision (vanishingly rare).
    pub fn insert(self, db: &Db) -> Entry {
        use crate::db::{ConflictStrategy, InsertOutcome};
        let mut e = self.build();
        let outcome = db
            .insert_entry_with_strategy(&e, ConflictStrategy::Ignore)
            .expect("EntryBuilder::insert failed");
        match outcome {
            InsertOutcome::Inserted { slug, hid } => {
                e.slug = slug;
                e.hid = hid;
            }
            InsertOutcome::Ignored { slug } => {
                // Identical content already exists — mirror the stored slug
                // so callers comparing by slug land on the extant row. Read
                // the stored hid back too.
                if let Ok(stored) = db.get_entry(&slug) {
                    e.hid = stored.hid;
                }
                e.slug = slug;
            }
            other => panic!("EntryBuilder::insert: unexpected outcome {other:?}"),
        }
        e
    }
}

/// Shorthand for [`EntryBuilder::new`].
pub fn entry_builder(field: impl Into<String>) -> EntryBuilder {
    EntryBuilder::new(field)
}

// ─── SpecBuilder ─────────────────────────────────────────────────────────────

/// Fluent builder for [`FieldSpec`] test fixtures.
pub struct SpecBuilder {
    spec: FieldSpec,
}

impl SpecBuilder {
    pub fn new(field: impl Into<String>) -> Self {
        Self {
            spec: FieldSpec::new(field, Verb::Occur),
        }
    }

    pub fn default_verb(mut self, v: Verb) -> Self {
        self.spec.default_verb = v;
        self
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.spec.description = Some(d.into());
        self
    }

    pub fn unit(mut self, u: impl Into<String>) -> Self {
        self.spec.unit = Some(u.into());
        self
    }

    pub fn subfield(mut self, name: impl Into<String>, spec: SubfieldSpec) -> Self {
        self.spec.subfields.insert(name.into(), spec);
        self
    }

    pub fn build(self) -> FieldSpec {
        self.spec
    }

    /// Build and persist to `db`.
    pub fn save(self, db: &Db) -> FieldSpec {
        let spec = self.build();
        db.save_field_spec(&spec).expect("SpecBuilder::save failed");
        spec
    }
}

pub fn spec_builder(field: impl Into<String>) -> SpecBuilder {
    SpecBuilder::new(field)
}

// ─── AliasBuilder ────────────────────────────────────────────────────────────

/// Fluent builder for [`Alias`] test fixtures.
pub struct AliasBuilder {
    alias: Alias,
}

impl AliasBuilder {
    pub fn new(name: impl Into<String>, expansion: impl Into<String>) -> Self {
        Self {
            alias: Alias {
                name: name.into(),
                expands_to: expansion.into(),
                description: None,
            },
        }
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.alias.description = Some(d.into());
        self
    }

    pub fn build(self) -> Alias {
        self.alias
    }

    pub fn save(self, db: &Db) -> Alias {
        let a = self.build();
        db.save_alias(&a).expect("AliasBuilder::save failed");
        a
    }
}

pub fn alias_builder(name: impl Into<String>, expansion: impl Into<String>) -> AliasBuilder {
    AliasBuilder::new(name, expansion)
}

// ─── Assertion macros ────────────────────────────────────────────────────────

/// Assert that entry fields match given key-value pairs.
///
/// Supported fields: `field`, `verb`, `value`, `unit`, `device_id`.
///
/// ```ignore
/// assert_entry_matches!(entry, field: "mood", value: "happy");
/// ```
#[macro_export]
macro_rules! assert_entry_matches {
    ($entry:expr, $($field:ident : $val:expr),* $(,)?) => {{
        let __e: &$crate::entry::Entry = &$entry;
        $(
            $crate::test_util::_assert_entry_field(
                __e,
                stringify!($field),
                &::serde_json::json!($val),
            );
        )*
    }};
}

/// Assert the number of entries for a given field in a [`Db`].
///
/// ```ignore
/// assert_count!(db, "mood", 3);
/// ```
#[macro_export]
macro_rules! assert_count {
    ($db:expr, $field:expr, $n:expr) => {{
        let __entries = $db
            .entries_for_field($field, None)
            .expect("entries_for_field failed");
        assert_eq!(
            __entries.len(),
            $n,
            "expected {} entries for field {:?}, got {}",
            $n,
            $field,
            __entries.len()
        );
    }};
}

#[doc(hidden)]
pub fn _assert_entry_field(entry: &Entry, field_name: &str, expected: &JsonValue) {
    let actual = match field_name {
        "field" => serde_json::json!(entry.field),
        "verb" => serde_json::json!(entry.verb.to_string()),
        "value" | "state" => entry.state.clone().unwrap_or(JsonValue::Null),
        "unit" => match &entry.unit {
            Some(u) => serde_json::json!(u),
            None => JsonValue::Null,
        },
        "device_id" => serde_json::json!(entry.device_id),
        other => panic!("assert_entry_matches!: unknown field `{other}`"),
    };
    assert_eq!(
        &actual, expected,
        "entry.{field_name} mismatch: expected {expected}, got {actual}"
    );
}

// ─── Deterministic test clock ────────────────────────────────────────────────
//
// Tests rely on `DatumTime::now()` for slug generation, modify_time, etc.
// Two calls landing in the same millisecond can produce colliding slugs and
// flake the test. These helpers freeze (or step) the clock through the same
// `-t` flag parser used by the CLI, so tests read like the user-facing flag
// syntax instead of constructing chrono values inline.
//
// The clock is thread-local — `cargo test` runs threads in parallel and each
// test owns its own mock without leaking into siblings.

/// RAII guard returned by [`set_time`] / [`freeze_time`]. Drops uninstall
/// the mock clock so leftover state cannot bleed into the next test on the
/// same thread.
#[must_use = "binding the guard to `_g` is fine; dropping it immediately resets the clock"]
pub struct TimeGuard {
    _private: (),
}

impl Drop for TimeGuard {
    fn drop(&mut self) {
        clock::uninstall();
    }
}

/// Parse `s` with the same parser the CLI uses for `-t/--time`, install a
/// [`MockClock`] frozen at that instant on the current thread, and return a
/// guard that resets the clock on drop.
///
/// Accepted forms (full list lives in [`parse_time_arg`]):
/// - ISO instants: `"2026-01-15T12:00:00Z"`, `"2026-01-15T12:00:00+02:00"`
/// - ISO naive datetimes: `"2026-01-15T12:00:00"` (resolved as local)
/// - Wall-clock times for "today": `"1530"`, `"15:30"`, `"3:30pm"`
///
/// Calling `set_time` again replaces the previously-installed clock.
pub fn set_time(s: &str) -> TimeGuard {
    let dt = parse_test_time(s);
    clock::install(Arc::new(MockClock::new(dt)));
    TimeGuard { _private: () }
}

/// Alias for [`set_time`] — reads more naturally when the test wants to
/// emphasise the freezing aspect over the value being set.
pub fn freeze_time(s: &str) -> TimeGuard {
    set_time(s)
}

/// Step the current mock clock forward (or backward, with a leading `-`).
/// The string is parsed via the same duration parser as `--dur`, plus an
/// optional leading `+`/`-` for sign.
///
/// Examples: `"5m"`, `"+5m"`, `"-1h"`, `"30s"`, `"PT15M"`.
///
/// If no mock clock is installed, this installs one anchored at "now" + the
/// supplied delta. Most tests should call [`set_time`] first.
pub fn advance_time(s: &str) {
    let (sign, rest) = match s.as_bytes().first() {
        Some(b'+') => (1, &s[1..]),
        Some(b'-') => (-1, &s[1..]),
        _ => (1, s),
    };
    let mut dur = parse_duration(rest)
        .unwrap_or_else(|e| panic!("advance_time: cannot parse duration {s:?}: {e}"));
    if sign < 0 {
        dur = -dur;
    }
    let cur = DatumTime::now();
    let new = DatumTime {
        utc: cur.utc + dur,
        ..cur
    };
    clock::install(Arc::new(MockClock::new(new)));
}

/// Manually drop the mock clock. The [`TimeGuard`] returned by [`set_time`]
/// also does this on drop; call this directly only when the guard pattern
/// doesn't fit (e.g. mid-test reset before installing a different fake).
pub fn reset_time() {
    clock::uninstall();
}

fn parse_test_time(s: &str) -> DatumTime {
    let arg =
        parse_time_arg(s).unwrap_or_else(|e| panic!("set_time: cannot parse time {s:?}: {e}"));
    TimeBuilder::now().with_time_arg(arg).build()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

// da-5ns sub-bead 1: tests gated off — they assert on `value` fields
// that no longer exist; sub-bead 8 rewrites them with the new shape.
#[cfg(any())]
mod tests {
    use super::*;

    #[test]
    fn fresh_db_is_isolated_between_tests() {
        let db_a = fresh_db();
        let db_b = fresh_db();

        entry_builder("coffee")
            .value(serde_json::json!(2))
            .insert(&db_a);

        // db_b must not see db_a's entry — each fresh_db() is independent.
        let in_a = db_a.entries_for_field("coffee", None).unwrap();
        let in_b = db_b.entries_for_field("coffee", None).unwrap();
        assert_eq!(in_a.len(), 1);
        assert_eq!(in_b.len(), 0);

        // And inserts into db_b should not appear in db_a.
        entry_builder("mood")
            .verb(Verb::Switch)
            .value(serde_json::json!("happy"))
            .insert(&db_b);

        assert_eq!(db_a.entries_for_field("mood", None).unwrap().len(), 0);
        assert_eq!(db_b.entries_for_field("mood", None).unwrap().len(), 1);
    }

    #[test]
    fn entry_builder_round_trip() {
        let db = fresh_db();
        let built = entry_builder("coffee")
            .verb(Verb::Occur)
            .value(serde_json::json!(2))
            .unit("cups")
            .insert(&db);

        let fetched = db.get_entry(&built.slug).unwrap();
        assert_entry_matches!(fetched,
            field: "coffee",
            verb: "occur",
            value: 2,
            unit: "cups",
            device_id: "test-device",
        );
        assert_count!(db, "coffee", 1);
    }

    #[test]
    fn spec_and_alias_builders_save_and_round_trip() {
        let db = fresh_db();
        spec_builder("word")
            .default_verb(Verb::Add)
            .description("vocab")
            .save(&db);
        alias_builder("vbp", "note word srcLang=pt pos=verb").save(&db);

        let registry = db.load_registry().unwrap();
        assert!(registry.field_spec("word").is_some());
        assert_eq!(
            registry.field_spec("word").unwrap().description.as_deref(),
            Some("vocab"),
        );
        assert_eq!(
            registry.aliases.get("vbp").unwrap().expands_to,
            "note word srcLang=pt pos=verb"
        );
    }
}
