//! Core entry type — the fundamental unit of data in datum.
//!
//! da-5ns time-shape pivot (2026-05-01): the entry's time/state shape is
//! `occur_time` + `state` + `dur` + `following_state`, with `verb` stored
//! (not derived) as a classifier. Sub-bead 1 lands the schema and the
//! Rust struct; sub-beads 2/3 add derive_verb() and the strict CLI
//! parsing that fills these columns from user input. Until sub-bead 3
//! lands, `Entry::new` writes a stub `Verb::Point` for the verb column.

use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::dur::Dur;
use crate::ids::{Hid, Slug};
use crate::time::DatumTime;

/// The verb classifies an entry's (state, dur, following_state) shape.
/// Stored as plain text in the `verb` column (the SQL CHECK does not
/// constrain values; `dm check verbs` (sub-bead 7) detects drift
/// between stored and re-derived verbs).
///
/// Storage-only — five canonical shapes derived from the truth table by
/// [`crate::verb::derive_verb`]. Command intent (Occur/Start/Stop/Add) is
/// expressed at the parse boundary via [`crate::action::Action`]; the
/// legacy v19 variants were dropped in da-8y2q.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[derive(Default)]
pub enum Verb {
    /// Single point-in-time event with no state change (`dur is NULL,
    /// state is null/true, following_state is NULL`).
    #[default]
    Point,
    /// Transition to a new state (`dur is NULL, state is set,
    /// following_state is NULL`).
    Switch,
    /// Duration block (`dur is set/positive, following_state is NULL`).
    Block,
    /// Block + switch at the trailing edge (`dur is set/positive,
    /// following_state is set`).
    BlockSwitch,
    /// Point + switch at the same instant (`dur is "PT0S",
    /// following_state is set`).
    PointSwitch,
}

impl std::fmt::Display for Verb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verb::Point => write!(f, "point"),
            Verb::Switch => write!(f, "switch"),
            Verb::Block => write!(f, "block"),
            Verb::BlockSwitch => write!(f, "block-switch"),
            Verb::PointSwitch => write!(f, "point-switch"),
        }
    }
}

impl std::str::FromStr for Verb {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "point" => Ok(Verb::Point),
            "switch" => Ok(Verb::Switch),
            "block" => Ok(Verb::Block),
            "block-switch" => Ok(Verb::BlockSwitch),
            "point-switch" => Ok(Verb::PointSwitch),
            _ => Err(format!("Unknown verb: '{s}'")),
        }
    }
}

/// Granularity at which a timestamp is meaningful — re-exported from
/// `datum-parse` so `entry::PrecisionUnit` remains a valid path. Definition
/// lives in `datum_parse::time::precision`.
pub use datum_parse::PrecisionUnit;

fn deserialize_optional_nonempty_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.filter(|s| !s.is_empty()))
}

/// Legacy shape classifier kept on the public surface as a compatibility
/// shim for sub-bead 1: callers that were matching on
/// `entry.entry_shape` still compile, but the value is derived from
/// `dur` rather than stored. Sub-beads 2/5/8 replace shape-based
/// switches with the verb-based truth table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum EntryShape {
    /// `dur is NULL`.
    #[default]
    Point,
    /// `dur is set` (non-NULL, non-zero).
    Block,
    /// Pre-pivot only. Never derived from a v=20 entry. Kept so legacy
    /// match arms still type-check.
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
///
/// Time-shape pivot (da-5ns): replaces start_time/end_time/entry_shape
/// /intent/value with occur_time/state/dur/following_state plus a stored
/// `verb` classifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Random 12-char base36 id — prefix-addressable, unique. PRIMARY KEY.
    ///
    /// **The durable identity, local and cross-device.** `upsert_lww` merges
    /// on `hid` and never rewrites it (da-zlug), so it is stable across a
    /// sync merge, and `entry_history` / `integrity_warnings` / prefix
    /// addressing can all key on it safely.
    pub hid: Hid,

    /// datum-compatible human reference (`field:timestamp` or similar).
    /// UNIQUE-indexed, derived at write time from field + id-keys.
    ///
    /// Human-facing, and **mutable**: it is derived from `field` +
    /// `occur_time`, so `retime` rewrites it in place. Do not treat it as an
    /// identity — a sync merge sets it from the LWW winner, and two distinct
    /// entries contending for one slug leave the loser at `<slug>_<hid>`.
    pub slug: Slug,

    /// SHA-256 of the identity-defining fields for dedup.
    pub content_hash: String,

    // --- What ---
    pub field: String,
    /// Verb classification — stored, not derived (da-5ns). Sub-bead 2
    /// adds `derive_verb` which the insert/update path (sub-bead 3)
    /// calls; until then this is a stub `Verb::Point`.
    pub verb: Verb,
    /// Permissive JSON. `None` (column NULL) means undefined; on
    /// insert/update a switch-style verb promotes None to literal
    /// `true` (sub-bead 3).
    #[serde(default)]
    pub state: Option<JsonValue>,
    /// Duration intent (da-ju71.3). `None` = column NULL (legacy point /
    /// no-duration shape; new writes default to `Some(Dur::Moment)` for
    /// occur). `Some(Dur::Of(PT0S))` = explicit zero span.
    /// Serializes/deserializes as the storage string (`"MOMENT"`,
    /// `"UNKNOWN"`, or an ISO-8601 duration like `"PT30M"`) so the JSON
    /// wire format and SQL TEXT column round-trip unchanged.
    #[serde(default)]
    pub dur: Option<Dur>,
    /// ISO-8601 duration string capturing the granularity at which `dur`
    /// is meaningful (e.g. `"PT1M"` for minute-resolution). `None` =
    /// unspecified.
    #[serde(default)]
    pub dur_precision: Option<String>,
    /// Trailing-edge state for switch-style verbs. Only meaningful when
    /// `dur` is set (the SQL CHECK enforces this).
    #[serde(default)]
    pub following_state: Option<JsonValue>,
    /// Arbitrarily nested JSON. Individual leaf values may use {"v": x, "u": "unit"}.
    pub data: Option<JsonValue>,

    // --- When ---
    /// When the event occurred. Required at the schema level (occur_time
    /// is NOT NULL). Optional in Rust to keep the construction surface
    /// uniform — the insert path errors on None.
    pub occur_time: Option<DatumTime>,
    /// Single ISO-8601 duration string (e.g. "PT1S", "P1D") capturing
    /// the granularity at which `occur_time` is meaningful. `None` =
    /// exact second.
    #[serde(default)]
    pub occur_time_precision: Option<String>,

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
    /// Literal argv of the command that produced this entry. `None` for
    /// entries whose source path didn't carry an argv at all — e.g.
    /// legacy datum CouchDB imports, which never went through the
    /// daydumb parser. Was `String` with empty-string sentinel pre-§5;
    /// nullable now per vividn 2026-06-02 (polecat §5 #11).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_nonempty_string"
    )]
    pub raw_input: Option<String>,
    /// Chain of alias expansions applied before argv was parsed.
    #[serde(default)]
    pub alias_chain: Option<JsonValue>,
    /// Version of the parser that produced this entry.
    #[serde(default = "default_parser_version")]
    pub parser_version: u32,

    /// S6: explicit user assertion from `-L/--last-state=<v>`. Normalized JSON
    /// value recorded verbatim; `None` for the common case (no assertion).
    #[serde(default)]
    pub user_asserted_last_state: Option<JsonValue>,

    /// da-tsw: system-managed provenance (e.g. `_import` for datum-migrate
    /// rows). Strictly disjoint from `data`.
    #[serde(default)]
    pub extra_metadata: Option<JsonValue>,
}

/// Current parser version. Bump when parser behaviour changes so stored
/// entries can be replayed through the new parser if we ever discover a
/// parse bug and want to re-derive the canonical fields.
pub const CURRENT_PARSER_VERSION: u32 = 2;

fn default_parser_version() -> u32 {
    CURRENT_PARSER_VERSION
}

impl Entry {
    /// Build a new entry with auto-generated hid, slug, and content_hash.
    ///
    /// **Stub for sub-bead 1.** The `verb` argument is currently ignored
    /// by the insert path which writes a literal `"point"` until
    /// sub-bead 3 wires `derive_verb`. Callers that pass a meaningful
    /// verb still see it round-tripped through the struct itself.
    // Entry construction is a hot path with eight load-bearing inputs;
    // grouping them into a builder hurts call-site readability without
    // shrinking the contract. Revisit if `Entry::new` itself grows.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        field: impl Into<String>,
        verb: Verb,
        state: Option<JsonValue>,
        data: Option<JsonValue>,
        occur_time: Option<DatumTime>,
        dur: Option<Dur>,
        device_id: impl Into<String>,
    ) -> Self {
        let field = field.into();
        let device_id = device_id.into();
        let now = Utc::now();

        let hid = new_hid();

        let occur_str = occur_time
            .as_ref()
            .map(|t| t.utc.to_rfc3339())
            .unwrap_or_default();
        let dur_str = dur.as_ref().map(Dur::to_storage_string).unwrap_or_default();
        let data_str = data.as_ref().map(|v| v.to_string()).unwrap_or_default();
        let state_str = state.as_ref().map(|v| v.to_string()).unwrap_or_default();

        let content_hash = compute_hash(&field, &occur_str, &dur_str, &state_str, &data_str);

        let slug = build_slug(&field, occur_time.as_ref(), &now);

        Entry {
            hid,
            slug,
            content_hash,
            field,
            verb,
            state,
            dur,
            dur_precision: None,
            following_state: None,
            data,
            occur_time,
            occur_time_precision: None,
            lat: None,
            lon: None,
            location: None,
            create_time: now,
            modify_time: now,
            device_id,
            id_structure: None,
            field_structure: None,
            deleted_at: None,
            raw_input: None,
            alias_chain: None,
            parser_version: CURRENT_PARSER_VERSION,
            user_asserted_last_state: None,
            extra_metadata: None,
        }
    }

    /// Compatibility constructor mirroring the v=19 (start_time,
    /// end_time) signature. Computes `dur` from the time delta, maps
    /// `value` → `state`, and otherwise delegates to [`Self::new`].
    /// Sub-bead 8 sweeps remaining call sites onto the v=20
    /// signature; until then this keeps `datum-commands` compiling.
    #[allow(clippy::too_many_arguments)]
    pub fn new_legacy(
        field: impl Into<String>,
        verb: Verb,
        value: Option<JsonValue>,
        data: Option<JsonValue>,
        start_time: Option<DatumTime>,
        end_time: Option<DatumTime>,
        device_id: impl Into<String>,
    ) -> Self {
        let dur = match (start_time.as_ref(), end_time.as_ref()) {
            (Some(s), Some(e)) => {
                let secs = (e.utc - s.utc).num_seconds().max(0);
                Some(Dur::Of(chrono::Duration::seconds(secs)))
            }
            _ => None,
        };
        Self::new(field, verb, value, data, start_time, dur, device_id)
    }

    /// Short hid — first [`SHORT_HID_LEN`] chars of the stored hid.
    pub fn short_uid(&self) -> String {
        self.hid.chars().take(SHORT_HID_LEN).collect()
    }

    /// Duration in seconds. Returns `None` for entries without a span
    /// (`None`, `Dur::Moment`, `Dur::Unknown`).
    pub fn duration_seconds(&self) -> Option<i64> {
        self.dur.as_ref()?.as_seconds()
    }

    /// Trailing-edge timestamp derived from `occur_time + dur`. `None`
    /// when either is unset (point/switch shapes have no end). Replaces
    /// the v=19 `end_time` column-as-field access in tests / render code.
    pub fn computed_end_time(&self) -> Option<DatumTime> {
        let occur = self.occur_time.as_ref()?;
        let secs = self.dur.as_ref()?.as_seconds()?;
        let mut e = occur.clone();
        e.utc += chrono::Duration::seconds(secs);
        // The `utc` bump desyncs `offset_minutes` (see the DatumTime
        // doc-comment); re-derive it so an end that crosses a DST
        // boundary renders the correct wall-clock instead of the start's
        // stale offset. Render-only path — drop to None on unknown-tz.
        e.refresh_offset().ok()?;
        Some(e)
    }

    /// Coarse shape classifier derived from `dur`. Replaces the v=19
    /// `entry_shape` column-as-field: `Block` when `dur` carries a real
    /// span, `Point` otherwise (`None` / zero-span / `Moment` / `Unknown`).
    /// `Ongoing` is never produced from a v=20 entry — open blocks are
    /// expressed via `verb`, not shape.
    pub fn shape(&self) -> EntryShape {
        match self.dur.as_ref() {
            None => EntryShape::Point,
            Some(d) if d.is_zero_span() => EntryShape::Point,
            Some(Dur::Moment) | Some(Dur::Unknown) => EntryShape::Point,
            Some(_) => EntryShape::Block,
        }
    }

    /// Recompute the content_hash from the entry's current fields.
    pub fn expected_content_hash(&self) -> String {
        let occur_str = self
            .occur_time
            .as_ref()
            .map(|t| t.utc.to_rfc3339())
            .unwrap_or_default();
        let dur_str = self
            .dur
            .as_ref()
            .map(Dur::to_storage_string)
            .unwrap_or_default();
        let data_str = self
            .data
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_default();
        let state_str = self
            .state
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_default();
        compute_hash(&self.field, &occur_str, &dur_str, &state_str, &data_str)
    }
}

pub(crate) fn compute_hash(
    field: &str,
    occur: &str,
    dur: &str,
    state: &str,
    extra: &str,
) -> String {
    let mut h = Sha256::new();
    h.update(field.as_bytes());
    h.update(b"\x00");
    h.update(occur.as_bytes());
    h.update(b"\x00");
    h.update(dur.as_bytes());
    h.update(b"\x00");
    h.update(state.as_bytes());
    h.update(b"\x00");
    h.update(extra.as_bytes());
    format!("{:x}", h.finalize())
}

/// Stored `dur` text → seconds. Handles ISO-8601 spans (`PT5M`, `P1D`,
/// `P3W`) and the sentinel forms `MOMENT` / `UNKNOWN` (both → `None`,
/// they have no measurable duration). Used by callers that read raw
/// dur strings from SQL projections that bypass [`Entry`]'s typed
/// `Option<Dur>` (e.g. `views/duration_sum.rs`); for typed callers
/// prefer [`Dur::as_seconds`] directly.
pub(crate) fn compat_dur_secs(s: &str) -> Option<i64> {
    crate::dur::Dur::parse(s).and_then(|d| d.as_seconds())
}

/// Default slug. A time-anchored entry slugs `<field>:<occur_time>`;
/// millisecond precision keeps same-field same-second inserts on distinct
/// slugs. A timeless entry (no `occur_time`) slugs `<field>:c<create_time>`
/// (da-r17h) — the `c` marker mirrors the display convention and `create_time`
/// keeps many-per-field timeless entries apart (the old `<field>:unset` made
/// every timeless entry collide on the UNIQUE slug). This is the default for
/// directly-constructed entries; the CLI insert path overrides it with the
/// field's `id_structure` template.
pub fn build_slug(field: &str, time: Option<&DatumTime>, create_time: &DateTime<Utc>) -> Slug {
    let s = match time {
        Some(t) => format!("{}:{}", field, t.utc.format("%Y-%m-%dT%H:%M:%S%.3fZ")),
        None => format!(
            "{}:c{}",
            field,
            create_time.format("%Y-%m-%dT%H:%M:%S%.3fZ")
        ),
    };
    Slug(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::DatumTime;

    fn fixed_time() -> DatumTime {
        DatumTime::from_utc_and_tz(
            chrono::DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
                .unwrap()
                .with_timezone(&Utc),
            "UTC",
        )
        .unwrap()
    }

    #[test]
    fn test_entry_creation() {
        let t = DatumTime::now();
        let e = Entry::new(
            "coffee",
            Verb::Point,
            Some(serde_json::json!(2)),
            None,
            Some(t),
            None,
            "device-test",
        );
        assert_eq!(e.field, "coffee");
        assert_eq!(e.verb, Verb::Point);
        assert_eq!(e.hid.len(), HID_LEN);
        assert!(e
            .hid
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
        assert!(e.slug.starts_with("coffee:"));
    }

    #[test]
    fn test_verb_roundtrip() {
        for v in ["point", "switch", "block", "block-switch", "point-switch"] {
            let parsed: Verb = v.parse().unwrap();
            let displayed = format!("{parsed}");
            assert_eq!(displayed, v);
        }
    }

    #[test]
    fn test_verb_legacy_strings_rejected() {
        // da-8y2q.2: pre-v1 hard cut. The legacy verb names (`occur`,
        // `start`, `stop`, `end`, `add`, `note`) are command intent
        // (`Action`), not storage classification. They no longer parse
        // into `Verb`.
        for legacy in ["occur", "start", "stop", "end", "add", "note"] {
            assert!(
                legacy.parse::<Verb>().is_err(),
                "legacy verb '{legacy}' must no longer parse"
            );
        }
    }

    #[test]
    fn test_dedup_via_content_hash() {
        let t = DatumTime::now();
        let e1 = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(t.clone()),
            None,
            "d1",
        );
        let e2 = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
            Some(t.clone()),
            None,
            "d2",
        );
        assert_eq!(e1.content_hash, e2.content_hash);
    }

    #[test]
    fn short_uid_is_first_five_of_hid() {
        let t = DatumTime::now();
        let e = Entry::new("coffee", Verb::Point, None, None, Some(t), None, "d");
        let uid = e.short_uid();
        assert_eq!(uid.len(), SHORT_HID_LEN);
        assert!(e.hid.starts_with(&uid));
    }

    #[test]
    fn new_hid_is_base36_and_correct_length() {
        for _ in 0..100 {
            let h = new_hid();
            assert_eq!(h.len(), HID_LEN);
            assert!(h
                .chars()
                .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
        }
    }

    #[test]
    fn raw_input_none_is_omitted_from_json() {
        let entry = Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(fixed_time()),
            None,
            "d",
        );
        let json = serde_json::to_value(&entry).unwrap();
        assert!(
            json.get("raw_input").is_none(),
            "raw_input should be omitted when absent, got {json:?}"
        );
    }

    #[test]
    fn raw_input_some_is_serialized_to_json() {
        let mut entry = Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(fixed_time()),
            None,
            "d",
        );
        entry.raw_input = Some("datum occur coffee".to_string());
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(
            json.get("raw_input"),
            Some(&serde_json::json!("datum occur coffee"))
        );
    }

    #[test]
    fn raw_input_empty_string_deserializes_to_none() {
        let mut json = serde_json::to_value(Entry::new(
            "coffee",
            Verb::Point,
            None,
            None,
            Some(fixed_time()),
            None,
            "d",
        ))
        .unwrap();
        json["raw_input"] = serde_json::json!("");
        let entry: Entry = serde_json::from_value(json).unwrap();
        assert_eq!(entry.raw_input, None);
    }
}
