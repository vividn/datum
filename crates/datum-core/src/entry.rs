//! Core entry type — the fundamental unit of data in datum.

use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::time::DatumTime;

/// The verb describes what kind of event an entry represents.
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
    /// Non-time-bound data chunk (e.g. word learned, person met)
    Note,
}

impl std::fmt::Display for Verb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verb::Occur => write!(f, "occur"),
            Verb::Start => write!(f, "start"),
            Verb::Stop => write!(f, "stop"),
            Verb::Switch => write!(f, "switch"),
            Verb::Note => write!(f, "note"),
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
            "note" | "add" => Ok(Verb::Note),
            _ => Err(format!("Unknown verb: '{s}'")),
        }
    }
}

/// Length of the stored `hid` column (base36 characters).
pub const HID_LEN: usize = 12;

/// Length of the short hid displayed in action output and used by `%uid%`.
pub const SHORT_HID_LEN: usize = 5;

/// Generate a new random 12-char base36 hid. The [0-9a-z] alphabet gives
/// ~36^12 ≈ 4.7 quintillion distinct values, so collisions under normal use
/// are astronomically unlikely; the DB still enforces uniqueness on the
/// column so an accidental collision simply forces a regeneration.
pub fn new_hid() -> String {
    let mut rng = rand::thread_rng();
    (0..HID_LEN)
        .map(|_| {
            let n = rng.gen_range(0..36u8);
            if n < 10 {
                (b'0' + n) as char
            } else {
                (b'a' + (n - 10)) as char
            }
        })
        .collect()
}

/// An entry is the core data unit. All user-logged data becomes an Entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Primary key — datum-compatible "field:timestamp" human reference.
    pub slug: String,

    /// Random 12-char base36 id — prefix-addressable quick reference, unique.
    pub hid: String,

    /// SHA-256 of (field + time_start_utc + time_end_utc + value + extra_data) for dedup.
    pub content_hash: String,

    // --- What ---
    pub field: String,
    pub verb: Verb,
    /// Primary value (the state for switch, quantity for occur, etc.)
    pub value: Option<JsonValue>,
    /// Arbitrarily nested JSON. Individual leaf values may use {"v": x, "u": "unit"}.
    pub extra_data: Option<JsonValue>,
    /// Top-level unit for `value` (e.g. "cups", "kg").
    pub unit: Option<String>,

    // --- When ---
    /// Start of event (or the moment of occurrence for `occur`).
    pub time_start: Option<DatumTime>,
    /// End of event. Duration = time_end - time_start (always derived, never stored).
    pub time_end: Option<DatumTime>,

    // --- Where ---
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub location: Option<String>,

    // --- Bookkeeping ---
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub device_id: String,

    // --- Template metadata (mirrors datum's meta.idStructure / meta.fieldStructure) ---
    /// Template used to produce `slug`, e.g. `"%field%:%uid%"`.
    #[serde(default)]
    pub id_structure: Option<String>,
    /// Template used to produce `field`, e.g. `"%srcLang%"`. Daydumb sets
    /// `field` directly on most paths; this preserves the template for
    /// round-trip compatibility with migrated datum data.
    #[serde(default)]
    pub field_structure: Option<String>,

    /// Soft-delete tombstone. `None` = live row.
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Entry {
    /// Build a new entry with auto-generated hid, slug, and content_hash.
    pub fn new(
        field: impl Into<String>,
        verb: Verb,
        value: Option<JsonValue>,
        extra_data: Option<JsonValue>,
        unit: Option<String>,
        time_start: Option<DatumTime>,
        time_end: Option<DatumTime>,
        device_id: impl Into<String>,
    ) -> Self {
        let field = field.into();
        let device_id = device_id.into();
        let now = Utc::now();

        let hid = new_hid();

        let time_start_str = time_start.as_ref().map(|t| t.utc.to_rfc3339()).unwrap_or_default();
        let time_end_str = time_end.as_ref().map(|t| t.utc.to_rfc3339()).unwrap_or_default();
        let extra_str = extra_data.as_ref().map(|v| v.to_string()).unwrap_or_default();
        let value_str = value.as_ref().map(|v| v.to_string()).unwrap_or_default();

        let content_hash = compute_hash(&field, &time_start_str, &time_end_str, &value_str, &extra_str);

        let slug = build_slug(&field, time_start.as_ref().or(time_end.as_ref()));

        Entry {
            slug,
            hid,
            content_hash,
            field,
            verb,
            value,
            extra_data,
            unit,
            time_start,
            time_end,
            lat: None,
            lon: None,
            location: None,
            created_at: now,
            modified_at: now,
            device_id,
            id_structure: None,
            field_structure: None,
            deleted_at: None,
        }
    }

    /// Short hid — first [`SHORT_HID_LEN`] chars of the stored hid.
    ///
    /// Used by the `%uid%` template placeholder and for compact display in
    /// action output. The full hid remains prefix-addressable via `datum get`.
    pub fn short_uid(&self) -> String {
        self.hid.chars().take(SHORT_HID_LEN).collect()
    }

    /// Duration in seconds (derived from time_start and time_end).
    pub fn duration_seconds(&self) -> Option<i64> {
        match (&self.time_start, &self.time_end) {
            (Some(start), Some(end)) => {
                Some((end.utc - start.utc).num_seconds())
            }
            _ => None,
        }
    }

    /// Recompute the content_hash from the entry's current fields.
    ///
    /// Used by consistency checks to detect rows whose stored `content_hash`
    /// diverges from what the current field values would produce.
    pub fn expected_content_hash(&self) -> String {
        let time_start_str = self.time_start.as_ref().map(|t| t.utc.to_rfc3339()).unwrap_or_default();
        let time_end_str = self.time_end.as_ref().map(|t| t.utc.to_rfc3339()).unwrap_or_default();
        let extra_str = self.extra_data.as_ref().map(|v| v.to_string()).unwrap_or_default();
        let value_str = self.value.as_ref().map(|v| v.to_string()).unwrap_or_default();
        compute_hash(&self.field, &time_start_str, &time_end_str, &value_str, &extra_str)
    }
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
fn build_slug(field: &str, time: Option<&DatumTime>) -> String {
    match time {
        Some(t) => format!("{}:{}", field, t.utc.format("%Y-%m-%dT%H:%M:%S%.3fZ")),
        None => format!("{}:unset", field),
    }
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
        assert_eq!(e.hid.len(), HID_LEN);
        assert!(e.hid.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
        assert!(e.slug.starts_with("coffee:"));
    }

    #[test]
    fn test_verb_roundtrip() {
        for v in ["occur", "start", "stop", "switch", "note"] {
            let parsed: Verb = v.parse().unwrap();
            let displayed = format!("{parsed}");
            assert!(!displayed.is_empty());
        }
    }

    #[test]
    fn test_dedup_via_content_hash() {
        let t = DatumTime::now();
        let e1 = Entry::new("mood", Verb::Switch, Some(serde_json::json!("happy")), None, None, Some(t.clone()), None, "d1");
        let e2 = Entry::new("mood", Verb::Switch, Some(serde_json::json!("happy")), None, None, Some(t.clone()), None, "d2");
        // Same content from different devices → same hash → dedup catches it
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
}
