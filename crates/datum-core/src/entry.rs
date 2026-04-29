//! Core entry type — the fundamental unit of data in datum.

use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::ids::{Hid, Slug};
use crate::time::DatumTime;

/// The verb describes what kind of event an entry represents. Read-back
/// values come from the `verb` generated column in SQLite; on write, verb
/// is derived from `entry_shape` + `value` + `intent`.
///
/// `note` is no longer a first-class verb — legacy `note` invocations are
/// routed to `add` via a seeded alias. `end` and `stop` are synonymous at
/// the CLI layer (both map to `Verb::Stop` for intent storage).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verb {
    /// Single point-in-time event (e.g. "drank coffee")
    Occur,
    /// Begin a duration block (e.g. "started working out")
    Start,
    /// End a duration block (e.g. "stopped working out")
    Stop,
    /// Transition to a new state (e.g. "project = daydumb")
    Switch,
    /// Completed block with a value (e.g. "slept 8h with mood=tired")
    Block,
    /// Canonical add — shape (point | block | ongoing) is inferred from the
    /// temporal keys present on the invocation.
    Add,
}

impl std::fmt::Display for Verb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verb::Occur => write!(f, "occur"),
            Verb::Start => write!(f, "start"),
            Verb::Stop => write!(f, "stop"),
            Verb::Switch => write!(f, "switch"),
            Verb::Block => write!(f, "block"),
            Verb::Add => write!(f, "add"),
        }
    }
}

impl std::str::FromStr for Verb {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "occur" => Ok(Verb::Occur),
            "start" => Ok(Verb::Start),
            "stop" | "end" => Ok(Verb::Stop),
            "switch" => Ok(Verb::Switch),
            "block" => Ok(Verb::Block),
            "add" | "note" => Ok(Verb::Add),
            _ => Err(format!("Unknown verb: '{s}'")),
        }
    }
}

/// Shape of an entry's time footprint. Stored explicitly so the generated
/// `verb` column (and read-time queries) can classify entries without
/// re-parsing timestamps or value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryShape {
    /// Instantaneous event at `start_time`.
    Point,
    /// Closed interval: `start_time` and `end_time` both set.
    Block,
    /// Open-ended: `start_time` is set, `end_time` is absent (still running).
    Ongoing,
}

impl std::fmt::Display for EntryShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryShape::Point => write!(f, "point"),
            EntryShape::Block => write!(f, "block"),
            EntryShape::Ongoing => write!(f, "ongoing"),
        }
    }
}

impl std::str::FromStr for EntryShape {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "point" => Ok(EntryShape::Point),
            "block" => Ok(EntryShape::Block),
            "ongoing" => Ok(EntryShape::Ongoing),
            _ => Err(format!("Unknown entry_shape: '{s}'")),
        }
    }
}

/// Granularity at which a timestamp is meaningful — re-exported from
/// `datum-parse` so `entry::PrecisionUnit` remains a valid path. Definition
/// lives in `datum_parse::time::precision`.
pub use datum_parse::PrecisionUnit;

/// Length of the stored `hid` column (base36 characters).
pub const HID_LEN: usize = 12;

/// Length of the short hid displayed in action output and used by `%uid%`.
pub const SHORT_HID_LEN: usize = 5;

/// Generate a new random 12-char base36 hid. The [0-9a-z] alphabet gives
/// ~36^12 ≈ 4.7 quintillion distinct values, so collisions under normal use
/// are astronomically unlikely; the DB still enforces uniqueness on the
/// column so an accidental collision simply forces a regeneration.
pub fn new_hid() -> Hid {
    let mut rng = rand::thread_rng();
    let s: String = (0..HID_LEN)
        .map(|_| {
            let n = rng.gen_range(0..36u8);
            if n < 10 {
                (b'0' + n) as char
            } else {
                (b'a' + (n - 10)) as char
            }
        })
        .collect();
    Hid(s)
}

/// An entry is the core data unit. All user-logged data becomes an Entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Random 12-char base36 id — prefix-addressable, unique. PRIMARY KEY.
    pub hid: Hid,

    /// datum-compatible human reference (`field:timestamp` or similar).
    /// UNIQUE-indexed, derived at write time from field + id-keys.
    pub slug: Slug,

    /// SHA-256 of the identity-defining fields for dedup.
    pub content_hash: String,

    // --- What ---
    pub field: String,
    /// Shape of the entry's time footprint — stored, not derived.
    pub entry_shape: EntryShape,
    /// Optional verb-intent override. When set (e.g. "stop", "note"), the
    /// generated `verb` column returns this value verbatim. When None, verb
    /// is derived from `entry_shape` + `value`.
    #[serde(default)]
    pub intent: Option<String>,
    /// Verb classification — populated from the generated SQL column on read.
    /// On write this field is derived from entry_shape + value + intent.
    pub verb: Verb,
    /// Primary value (the state for switch, quantity for occur, etc.)
    pub value: Option<JsonValue>,
    /// Arbitrarily nested JSON. Individual leaf values may use {"v": x, "u": "unit"}.
    pub data: Option<JsonValue>,
    /// Top-level unit for `value` (e.g. "cups", "kg").
    pub unit: Option<String>,

    // --- When ---
    /// Start of event (or the moment of occurrence for `occur`).
    pub start_time: Option<DatumTime>,
    /// End of event. Duration = end_time - start_time (generated in DB).
    pub end_time: Option<DatumTime>,
    /// Absolute uncertainty in seconds on `start_time`.
    #[serde(default)]
    pub start_time_precision: u32,
    /// Granularity unit for `start_time`.
    #[serde(default)]
    pub start_time_precision_unit: PrecisionUnit,
    /// Absolute uncertainty in seconds on `end_time`.
    #[serde(default)]
    pub end_time_precision: u32,
    /// Granularity unit for `end_time`.
    #[serde(default)]
    pub end_time_precision_unit: PrecisionUnit,

    // --- Where ---
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub location: Option<String>,

    // --- Bookkeeping ---
    pub create_time: DateTime<Utc>,
    pub modify_time: DateTime<Utc>,
    pub device_id: String,

    // --- Template metadata (mirrors datum's meta.idStructure / meta.fieldStructure) ---
    /// Template used to produce `slug`, e.g. `"%field%:%uid%"`.
    #[serde(default)]
    pub id_structure: Option<String>,
    /// Template used to produce `field`, e.g. `"%srcLang%"`.
    #[serde(default)]
    pub field_structure: Option<String>,

    /// Soft-delete tombstone. `None` = live row.
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,

    // --- Intent capture ---
    /// Literal argv of the command that produced this entry.
    #[serde(default)]
    pub raw_input: String,
    /// Chain of alias expansions applied before argv was parsed.
    #[serde(default)]
    pub alias_chain: Option<JsonValue>,
    /// Version of the parser that produced this entry.
    #[serde(default = "default_parser_version")]
    pub parser_version: u32,

    /// S6: explicit user assertion from `-L/--last-state=<v>`. Normalized JSON
    /// value recorded verbatim; `None` for the common case (no assertion).
    /// Mismatches against the computed last-state file a `last_state_mismatch`
    /// conflict but never block the insert. Not included in `content_hash`.
    #[serde(default)]
    pub user_asserted_last_state: Option<JsonValue>,
}

/// Current parser version. Bump when parser behaviour changes so stored
/// entries can be replayed through the new parser if we ever discover a
/// parse bug and want to re-derive the canonical fields.
///
/// v1: initial shape-first parser (D1 / schema v3).
/// v2: D2 — uncertainty syntax (±N), `-q*` precision scaling, `-K`/`-k`
///     id-key chaining, retime precision re-derivation.
pub const CURRENT_PARSER_VERSION: u32 = 2;

fn default_parser_version() -> u32 {
    CURRENT_PARSER_VERSION
}

impl Entry {
    /// Build a new entry with auto-generated hid, slug, and content_hash.
    ///
    /// `verb` is a convenience input: callers that haven't migrated to the
    /// shape-first API still think in terms of Occur/Start/Stop/Switch/Note
    /// and this constructor picks an `entry_shape` + `intent` that matches
    /// the verb's intended semantics.
    pub fn new(
        field: impl Into<String>,
        verb: Verb,
        value: Option<JsonValue>,
        data: Option<JsonValue>,
        unit: Option<String>,
        start_time: Option<DatumTime>,
        end_time: Option<DatumTime>,
        device_id: impl Into<String>,
    ) -> Self {
        let field = field.into();
        let device_id = device_id.into();
        let now = Utc::now();

        let hid = new_hid();

        let (entry_shape, intent) = infer_shape_and_intent(
            &verb,
            value.as_ref(),
            start_time.as_ref(),
            end_time.as_ref(),
        );

        let start_str = start_time.as_ref().map(|t| t.utc.to_rfc3339()).unwrap_or_default();
        let end_str = end_time.as_ref().map(|t| t.utc.to_rfc3339()).unwrap_or_default();
        let data_str = data.as_ref().map(|v| v.to_string()).unwrap_or_default();
        let value_str = value.as_ref().map(|v| v.to_string()).unwrap_or_default();

        let content_hash = compute_hash(&field, &start_str, &end_str, &value_str, &data_str);

        let slug = build_slug(&field, start_time.as_ref().or(end_time.as_ref()));

        Entry {
            hid,
            slug,
            content_hash,
            field,
            entry_shape,
            intent,
            verb,
            value,
            data,
            unit,
            start_time,
            end_time,
            start_time_precision: 0,
            start_time_precision_unit: PrecisionUnit::Exact,
            end_time_precision: 0,
            end_time_precision_unit: PrecisionUnit::Exact,
            lat: None,
            lon: None,
            location: None,
            create_time: now,
            modify_time: now,
            device_id,
            id_structure: None,
            field_structure: None,
            deleted_at: None,
            raw_input: String::new(),
            alias_chain: None,
            parser_version: CURRENT_PARSER_VERSION,
            user_asserted_last_state: None,
        }
    }

    /// Short hid — first [`SHORT_HID_LEN`] chars of the stored hid.
    pub fn short_uid(&self) -> String {
        self.hid.chars().take(SHORT_HID_LEN).collect()
    }

    /// Duration in seconds (derived from start_time and end_time).
    pub fn duration_seconds(&self) -> Option<i64> {
        match (&self.start_time, &self.end_time) {
            (Some(start), Some(end)) => {
                Some((end.utc - start.utc).num_seconds())
            }
            _ => None,
        }
    }

    /// Recompute the content_hash from the entry's current fields.
    pub fn expected_content_hash(&self) -> String {
        let start_str = self.start_time.as_ref().map(|t| t.utc.to_rfc3339()).unwrap_or_default();
        let end_str = self.end_time.as_ref().map(|t| t.utc.to_rfc3339()).unwrap_or_default();
        let data_str = self.data.as_ref().map(|v| v.to_string()).unwrap_or_default();
        let value_str = self.value.as_ref().map(|v| v.to_string()).unwrap_or_default();
        compute_hash(&self.field, &start_str, &end_str, &value_str, &data_str)
    }
}

/// Pick an `(entry_shape, intent)` pair that reconstructs the caller's verb
/// when fed through the SQL generated column. Verbs that don't survive the
/// shape-based projection (Stop, Note, Add, and Occur-with-value) ride along
/// in `intent`.
fn infer_shape_and_intent(
    verb: &Verb,
    value: Option<&JsonValue>,
    start_time: Option<&DatumTime>,
    end_time: Option<&DatumTime>,
) -> (EntryShape, Option<String>) {
    let shape = match (verb, start_time, end_time) {
        (Verb::Start, Some(_), None) => EntryShape::Ongoing,
        (Verb::Start, Some(_), Some(_)) => EntryShape::Block,
        (Verb::Block, _, _) => EntryShape::Block,
        (_, Some(_), Some(_)) => EntryShape::Block,
        _ => EntryShape::Point,
    };
    let intent = match verb {
        // These verbs can't be recovered from shape + value alone.
        Verb::Stop => Some("stop".to_string()),
        Verb::Add => Some("add".to_string()),
        // point + non-null value generates 'switch' in SQL; an explicit
        // Occur with a quantity (e.g. "drank 2 cups") needs the intent
        // override to survive round-trip.
        Verb::Occur if value.is_some() => Some("occur".to_string()),
        // Switch with no value would generate 'occur'; pin the intent.
        Verb::Switch if value.is_none() => Some("switch".to_string()),
        // Switch with both times → shape=block → generated 'block';
        // preserve the caller's Switch identity.
        Verb::Switch if start_time.is_some() && end_time.is_some() => Some("switch".to_string()),
        // `datum start sleep 5` is a block; the spec's acceptance expects
        // verb='block' there, so Start with both times rides shape=block
        // without an intent override and becomes Verb::Block on read-back.
        _ => None,
    };
    (shape, intent)
}

fn compute_hash(field: &str, start: &str, end: &str, value: &str, extra: &str) -> String {
    let mut h = Sha256::new();
    h.update(field.as_bytes());
    h.update(b"\x00");
    h.update(start.as_bytes());
    h.update(b"\x00");
    h.update(end.as_bytes());
    h.update(b"\x00");
    h.update(value.as_bytes());
    h.update(b"\x00");
    h.update(extra.as_bytes());
    format!("{:x}", h.finalize())
}

/// Default slug: `<field>:<timestamp>` — millisecond precision keeps
/// same-field same-second inserts on distinct slugs. Entries without a time
/// fall back to `<field>:unset`.
fn build_slug(field: &str, time: Option<&DatumTime>) -> Slug {
    let s = match time {
        Some(t) => format!("{}:{}", field, t.utc.format("%Y-%m-%dT%H:%M:%S%.3fZ")),
        None => format!("{}:unset", field),
    };
    Slug(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::DatumTime;

    #[test]
    fn test_entry_creation() {
        let t = DatumTime::now();
        let e = Entry::new(
            "coffee",
            Verb::Occur,
            Some(serde_json::json!(2)),
            None,
            Some("cups".to_string()),
            Some(t),
            None,
            "device-test",
        );
        assert_eq!(e.field, "coffee");
        assert_eq!(e.verb, Verb::Occur);
        assert_eq!(e.entry_shape, EntryShape::Point);
        assert_eq!(e.hid.len(), HID_LEN);
        assert!(e.hid.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
        assert!(e.slug.starts_with("coffee:"));
    }

    #[test]
    fn test_verb_roundtrip() {
        for v in ["occur", "start", "stop", "switch", "block", "add"] {
            let parsed: Verb = v.parse().unwrap();
            let displayed = format!("{parsed}");
            assert!(!displayed.is_empty());
        }
        // `note` parses to Verb::Add for backward compat; CLI shell reaches
        // the same place via a seeded alias, this covers direct string parse.
        assert_eq!("note".parse::<Verb>().unwrap(), Verb::Add);
        // `end` is a synonym for `stop` at parse time.
        assert_eq!("end".parse::<Verb>().unwrap(), Verb::Stop);
    }

    #[test]
    fn test_dedup_via_content_hash() {
        let t = DatumTime::now();
        let e1 = Entry::new("mood", Verb::Switch, Some(serde_json::json!("happy")), None, None, Some(t.clone()), None, "d1");
        let e2 = Entry::new("mood", Verb::Switch, Some(serde_json::json!("happy")), None, None, Some(t.clone()), None, "d2");
        assert_eq!(e1.content_hash, e2.content_hash);
    }

    #[test]
    fn short_uid_is_first_five_of_hid() {
        let t = DatumTime::now();
        let e = Entry::new("coffee", Verb::Occur, None, None, None, Some(t), None, "d");
        let uid = e.short_uid();
        assert_eq!(uid.len(), SHORT_HID_LEN);
        assert!(e.hid.starts_with(&uid));
    }

    #[test]
    fn new_hid_is_base36_and_correct_length() {
        for _ in 0..100 {
            let h = new_hid();
            assert_eq!(h.len(), HID_LEN);
            assert!(h.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
        }
    }

    #[test]
    fn shape_inference_matches_verb_and_times() {
        let t = DatumTime::now();
        // Start with no end → Ongoing
        let e = Entry::new("nap", Verb::Start, None, None, None, Some(t.clone()), None, "d");
        assert_eq!(e.entry_shape, EntryShape::Ongoing);
        assert_eq!(e.intent, None);

        // Start with end → Block
        let e = Entry::new("nap", Verb::Start, None, None, None, Some(t.clone()), Some(t.clone()), "d");
        assert_eq!(e.entry_shape, EntryShape::Block);

        // Stop rides along via intent so state compilation can still pick it up.
        let e = Entry::new("nap", Verb::Stop, None, None, None, Some(t.clone()), None, "d");
        assert_eq!(e.entry_shape, EntryShape::Point);
        assert_eq!(e.intent.as_deref(), Some("stop"));

        // Add carries an explicit intent so it survives shape-derivation round-trip.
        let e = Entry::new("journal", Verb::Add, Some(serde_json::json!("hello")), None, None, Some(t), None, "d");
        assert_eq!(e.intent.as_deref(), Some("add"));
    }
}
