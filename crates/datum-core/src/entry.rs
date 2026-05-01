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
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::ids::{Hid, Slug};
use crate::time::DatumTime;

/// The verb classifies an entry's (state, dur, following_state) shape.
/// Stored as plain text in the `verb` column (the SQL CHECK does not
/// constrain values; `dm check --verbs` (sub-bead 7) detects drift
/// between stored and re-derived verbs).
///
/// Sub-bead 2 (da-5ns.2) introduces the new five-verb set — Point,
/// Switch, Block, BlockSwitch, PointSwitch — and the derivation
/// function. Until then this enum carries both the new variants and
/// the legacy v19 ones (Occur/Start/Stop/Add) so callers that haven't
/// migrated still compile. The legacy variants serialize to their old
/// strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verb {
    /// Single point-in-time event with no state change (`dur is NULL,
    /// state is null/true, following_state is NULL`).
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
    // ─── Legacy v19 variants ─────────────────────────────────────────
    // Kept so call sites that haven't migrated to the new five-verb
    // set still compile. Sub-bead 8 sweeps these out alongside the
    // test rewrite.
    /// Legacy: single point-in-time event (e.g. "drank coffee").
    Occur,
    /// Legacy: begin a duration block (e.g. "started working out").
    Start,
    /// Legacy: end a duration block (e.g. "stopped working out").
    Stop,
    /// Legacy: canonical add — shape inferred from temporal keys.
    Add,
}

impl Default for Verb {
    fn default() -> Self {
        Verb::Point
    }
}

impl std::fmt::Display for Verb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verb::Point => write!(f, "point"),
            Verb::Switch => write!(f, "switch"),
            Verb::Block => write!(f, "block"),
            Verb::BlockSwitch => write!(f, "block-switch"),
            Verb::PointSwitch => write!(f, "point-switch"),
            Verb::Occur => write!(f, "occur"),
            Verb::Start => write!(f, "start"),
            Verb::Stop => write!(f, "stop"),
            Verb::Add => write!(f, "add"),
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
            "occur" => Ok(Verb::Occur),
            "start" => Ok(Verb::Start),
            "stop" | "end" => Ok(Verb::Stop),
            "add" | "note" => Ok(Verb::Add),
            _ => Err(format!("Unknown verb: '{s}'")),
        }
    }
}

/// Granularity at which a timestamp is meaningful — re-exported from
/// `datum-parse` so `entry::PrecisionUnit` remains a valid path. Definition
/// lives in `datum_parse::time::precision`.
pub use datum_parse::PrecisionUnit;

/// Legacy shape classifier kept on the public surface as a compatibility
/// shim for sub-bead 1: callers that were matching on
/// `entry.entry_shape` still compile, but the value is derived from
/// `dur` rather than stored. Sub-beads 2/5/8 replace shape-based
/// switches with the verb-based truth table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryShape {
    /// `dur is NULL`.
    Point,
    /// `dur is set` (non-NULL, non-zero).
    Block,
    /// Pre-pivot only. Never derived from a v=20 entry. Kept so legacy
    /// match arms still type-check.
    Ongoing,
}

impl Default for EntryShape {
    fn default() -> Self {
        EntryShape::Point
    }
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
    pub hid: Hid,

    /// datum-compatible human reference (`field:timestamp` or similar).
    /// UNIQUE-indexed, derived at write time from field + id-keys.
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
    /// ISO-8601 duration string (e.g. "PT30M"). `None` = no duration
    /// (point/switch). `"PT0S"` = explicit zero (point-with-state or
    /// point-switch).
    #[serde(default)]
    pub dur: Option<String>,
    /// Trailing-edge state for switch-style verbs. Only meaningful when
    /// `dur` is set (the SQL CHECK enforces this).
    #[serde(default)]
    pub following_state: Option<JsonValue>,
    /// Arbitrarily nested JSON. Individual leaf values may use {"v": x, "u": "unit"}.
    pub data: Option<JsonValue>,
    /// Top-level unit for `state` (e.g. "cups", "kg").
    pub unit: Option<String>,

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
    #[serde(default)]
    pub user_asserted_last_state: Option<JsonValue>,

    /// da-tsw: system-managed provenance (e.g. `_import` for datum-migrate
    /// rows). Strictly disjoint from `data`.
    #[serde(default)]
    pub extra_metadata: Option<JsonValue>,

    // ─── da-5ns sub-bead 1 compatibility shims ──────────────────────
    //
    // Pre-pivot Rust code reads `entry.start_time`, `entry.end_time`,
    // `entry.value`, `entry.entry_shape`, etc. directly. The v=20
    // schema dropped those columns, but Rust call sites are out of
    // scope for sub-bead 1 (sub-bead 3 wires the new flows; sub-bead
    // 8 sweeps the residue). To keep the crate compiling these fields
    // are kept on the struct as a compatibility layer:
    //
    // - On read (row_to_entry): populated from the new columns —
    //   start_time mirrors occur_time, value mirrors state, etc.
    // - On write (try_insert et al.): IGNORED. The SQL paths now
    //   write occur_time/state/dur/etc. exclusively.
    // - On serialization: skipped (`#[serde(skip)]`) so the JSON wire
    //   format reflects the v=20 shape and round-trips cleanly across
    //   sync.
    //
    // Sub-bead 8 deletes these fields and rewrites the residual call
    // sites to the new accessors.
    #[serde(skip)]
    pub start_time: Option<DatumTime>,
    #[serde(skip)]
    pub end_time: Option<DatumTime>,
    #[serde(skip)]
    pub start_time_precision: u32,
    #[serde(skip)]
    pub start_time_precision_unit: PrecisionUnit,
    #[serde(skip)]
    pub end_time_precision: u32,
    #[serde(skip)]
    pub end_time_precision_unit: PrecisionUnit,
    #[serde(skip)]
    pub entry_shape: EntryShape,
    #[serde(skip)]
    pub intent: Option<String>,
    #[serde(skip)]
    pub value: Option<JsonValue>,
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
    pub fn new(
        field: impl Into<String>,
        verb: Verb,
        state: Option<JsonValue>,
        data: Option<JsonValue>,
        unit: Option<String>,
        occur_time: Option<DatumTime>,
        dur: Option<String>,
        device_id: impl Into<String>,
    ) -> Self {
        let field = field.into();
        let device_id = device_id.into();
        let now = Utc::now();

        let hid = new_hid();

        let occur_str = occur_time.as_ref().map(|t| t.utc.to_rfc3339()).unwrap_or_default();
        let dur_str = dur.clone().unwrap_or_default();
        let data_str = data.as_ref().map(|v| v.to_string()).unwrap_or_default();
        let state_str = state.as_ref().map(|v| v.to_string()).unwrap_or_default();

        let content_hash = compute_hash(&field, &occur_str, &dur_str, &state_str, &data_str);

        let slug = build_slug(&field, occur_time.as_ref());

        // Populate the compat shims so call sites that read
        // entry.start_time / entry.value / entry.entry_shape see the
        // same data they did pre-pivot. See the comment on the field
        // block.
        let entry_shape = match dur.as_deref() {
            None => EntryShape::Point,
            Some(d) if matches!(d, "PT0S" | "P0D" | "PT0M" | "PT0H") => EntryShape::Point,
            Some(_) => EntryShape::Block,
        };
        let end_time_compat: Option<DatumTime> = match (occur_time.as_ref(), dur.as_deref()) {
            (Some(t), Some(d)) => parse_iso_duration_seconds(d).map(|secs| {
                let mut e = t.clone();
                e.utc += chrono::Duration::seconds(secs);
                e
            }),
            _ => None,
        };
        let start_time_compat = occur_time.clone();

        Entry {
            hid,
            slug,
            content_hash,
            field,
            verb,
            state: state.clone(),
            dur,
            following_state: None,
            data,
            unit,
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
            raw_input: String::new(),
            alias_chain: None,
            parser_version: CURRENT_PARSER_VERSION,
            user_asserted_last_state: None,
            extra_metadata: None,
            // Compat shims:
            start_time: start_time_compat,
            end_time: end_time_compat,
            start_time_precision: 0,
            start_time_precision_unit: PrecisionUnit::Exact,
            end_time_precision: 0,
            end_time_precision_unit: PrecisionUnit::Exact,
            entry_shape,
            intent: None,
            value: state,
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
        unit: Option<String>,
        start_time: Option<DatumTime>,
        end_time: Option<DatumTime>,
        device_id: impl Into<String>,
    ) -> Self {
        let dur = match (start_time.as_ref(), end_time.as_ref()) {
            (Some(s), Some(e)) => {
                let secs = (e.utc - s.utc).num_seconds().max(0);
                Some(format!("PT{}S", secs))
            }
            _ => None,
        };
        Self::new(field, verb, value, data, unit, start_time, dur, device_id)
    }

    /// Short hid — first [`SHORT_HID_LEN`] chars of the stored hid.
    pub fn short_uid(&self) -> String {
        self.hid.chars().take(SHORT_HID_LEN).collect()
    }

    /// Duration in seconds, parsed from the ISO-8601 `dur` string.
    /// Returns `None` for entries without a duration or with a malformed
    /// duration string. Sub-bead 2 lands a proper ISO-duration helper;
    /// for the sub-bead 1 stub this only handles "PT*S" / "PT*M" /
    /// "PT*H" / "P*D" — enough to keep callers compiling.
    pub fn duration_seconds(&self) -> Option<i64> {
        parse_iso_duration_seconds(self.dur.as_deref()?)
    }

    /// Recompute the content_hash from the entry's current fields.
    pub fn expected_content_hash(&self) -> String {
        let occur_str = self
            .occur_time
            .as_ref()
            .map(|t| t.utc.to_rfc3339())
            .unwrap_or_default();
        let dur_str = self.dur.clone().unwrap_or_default();
        let data_str = self.data.as_ref().map(|v| v.to_string()).unwrap_or_default();
        let state_str = self.state.as_ref().map(|v| v.to_string()).unwrap_or_default();
        compute_hash(
            &self.field,
            &occur_str,
            &dur_str,
            &state_str,
            &data_str,
        )
    }
}

fn compute_hash(field: &str, occur: &str, dur: &str, state: &str, extra: &str) -> String {
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

/// ISO-8601 duration → seconds. Handles composite forms (`P1DT2H30M`,
/// `PT1M30S`) and week-form (`P3W`). Years and months are approximated
/// (365d / 30d) — only used by callers that need an approximate seconds
/// count (the compat `end_time` shim, render-layer span calculations);
/// the canonical `dur` string is stored verbatim, never recomputed from
/// this number.
pub(crate) fn compat_dur_secs(s: &str) -> Option<i64> {
    parse_iso_duration_seconds(s)
}

fn parse_iso_duration_seconds(input: &str) -> Option<i64> {
    let (sign, body) = match input.strip_prefix('-') {
        Some(rest) => (-1f64, rest),
        None => (1.0, input),
    };
    let rest = body.strip_prefix('P')?;
    if rest.is_empty() {
        return None;
    }

    // Week form (PnW) is mutually exclusive with the rest of the grammar.
    if rest.ends_with('W') && !rest.contains('T') {
        let n_str = &rest[..rest.len() - 1];
        if n_str.is_empty() {
            return None;
        }
        let n: f64 = n_str.parse().ok()?;
        return Some((sign * n * 7.0 * 86400.0) as i64);
    }

    let mut total: f64 = 0.0;
    let mut buf = String::new();
    let mut seen_t = false;
    let mut had_any = false;

    for c in rest.chars() {
        if c == 'T' {
            if seen_t || !buf.is_empty() {
                return None;
            }
            seen_t = true;
        } else if c.is_ascii_digit() || c == '.' || c == ',' {
            buf.push(if c == ',' { '.' } else { c });
        } else {
            if buf.is_empty() {
                return None;
            }
            let n: f64 = buf.parse().ok()?;
            buf.clear();
            let secs = match (seen_t, c) {
                (false, 'Y') => n * 365.0 * 86400.0,
                (false, 'M') => n * 30.0 * 86400.0,
                (false, 'D') => n * 86400.0,
                (true, 'H') => n * 3600.0,
                (true, 'M') => n * 60.0,
                (true, 'S') => n,
                _ => return None,
            };
            total += secs;
            had_any = true;
        }
    }

    if !buf.is_empty() || !had_any {
        return None;
    }
    Some((sign * total) as i64)
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
            Verb::Point,
            Some(serde_json::json!(2)),
            None,
            Some("cups".to_string()),
            Some(t),
            None,
            "device-test",
        );
        assert_eq!(e.field, "coffee");
        assert_eq!(e.verb, Verb::Point);
        assert_eq!(e.hid.len(), HID_LEN);
        assert!(e.hid.chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
        assert!(e.slug.starts_with("coffee:"));
    }

    #[test]
    fn test_verb_roundtrip() {
        for v in [
            "point",
            "switch",
            "block",
            "block-switch",
            "point-switch",
            "occur",
            "start",
            "stop",
            "add",
        ] {
            let parsed: Verb = v.parse().unwrap();
            let displayed = format!("{parsed}");
            assert!(!displayed.is_empty());
        }
        assert_eq!("note".parse::<Verb>().unwrap(), Verb::Add);
        assert_eq!("end".parse::<Verb>().unwrap(), Verb::Stop);
    }

    #[test]
    fn test_dedup_via_content_hash() {
        let t = DatumTime::now();
        let e1 = Entry::new(
            "mood",
            Verb::Switch,
            Some(serde_json::json!("happy")),
            None,
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
        let e = Entry::new("coffee", Verb::Point, None, None, None, Some(t), None, "d");
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
