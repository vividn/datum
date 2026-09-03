//! The JSON contract between the browser and this module.
//!
//! One request shape, three response shapes. The response never carries an
//! exception across the ABI boundary — a malformed request, an unparseable
//! argv and a command this module does not build an entry for all come back
//! as JSON, so the caller renders a message instead of hitting a trap.

use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::log::{self, Host};

/// What the browser sends for one `run(argv)`.
#[derive(Debug, Deserialize)]
pub struct ParseRequest {
    /// The command words, *without* the program name — matching what the
    /// seam passes around (`web/src/lib/run.ts`).
    pub argv: Vec<String>,
    /// Wall clock as epoch milliseconds (`Date.now()`).
    pub now_ms: i64,
    /// The viewer's IANA zone name, when the browser can resolve one
    /// (`Intl.DateTimeFormat().resolvedOptions().timeZone`).
    #[serde(default)]
    pub timezone: Option<String>,
    /// The viewer's current UTC offset in minutes, east-positive — i.e.
    /// `-new Date().getTimezoneOffset()`. Used when `timezone` is absent or
    /// is not a name the tz database knows.
    #[serde(default)]
    pub offset_minutes: i32,
    /// This browser's device id.
    #[serde(default)]
    pub device_id: String,
    /// Random bytes for the hid, from `crypto.getRandomValues`.
    #[serde(default)]
    pub hid_bytes: Vec<u8>,
}

/// Parse one JSON request into one JSON response.
///
/// Responses:
/// * `{"entry": {…}}` — the row to insert.
/// * `{"unsupported": "<reason>"}` — a well-formed command this module does
///   not build an entry for. The seam turns this into its
///   "not available yet in the browser" exit.
/// * `{"error": "<message>"}` — the argv did not parse. The message is the
///   CLI's own, so the browser reports what `datum` would have reported.
pub fn parse_json(body: &str) -> String {
    let request: ParseRequest = match serde_json::from_str(body) {
        Ok(r) => r,
        Err(e) => return json!({ "error": format!("bad parse request: {e}") }).to_string(),
    };
    response(&request).to_string()
}

fn response(request: &ParseRequest) -> JsonValue {
    let now = match log::utc_from_millis(request.now_ms) {
        Ok(t) => t,
        Err(e) => return json!({ "error": e.to_string() }),
    };
    let host = Host {
        now,
        timezone: request.timezone.as_deref(),
        offset_minutes: request.offset_minutes,
        device_id: &request.device_id,
        hid_bytes: &request.hid_bytes,
    };
    match log::build(&request.argv, &host) {
        Ok(log::Parsed::Entry(entry)) => json!({ "entry": log::entry_to_json(&entry) }),
        Ok(log::Parsed::Unsupported(reason)) => json!({ "unsupported": reason }),
        Err(e) => json!({ "error": e.to_string() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-03-16T14:00:00Z.
    const NOW_MS: i64 = 1_773_669_600_000;

    fn run(argv: &[&str]) -> JsonValue {
        let request = ParseRequest {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            now_ms: NOW_MS,
            timezone: Some("UTC".to_string()),
            offset_minutes: 0,
            device_id: "web-test".to_string(),
            hid_bytes: vec![7; 12],
        };
        response(&request)
    }

    fn entry(argv: &[&str]) -> JsonValue {
        let out = run(argv);
        assert!(out.get("entry").is_some(), "expected an entry, got {out}");
        out["entry"].clone()
    }

    #[test]
    fn occur_is_a_point_with_no_state() {
        let e = entry(&["occur", "coffee", "-t", "0915"]);
        assert_eq!(e["field"], "coffee");
        assert_eq!(e["verb"], "point");
        assert!(e.get("state").is_none());
        assert_eq!(e["occurTime"]["utc"], "2026-03-16T09:15:00+00:00");
    }

    /// The bug that motivated the swap: `-q` is 5 minutes back, not 15.
    #[test]
    fn quick_flags_step_five_minutes_each() {
        let one = entry(&["occur", "coffee", "-q"]);
        assert_eq!(one["occurTime"]["utc"], "2026-03-16T13:55:00+00:00");
        let two = entry(&["occur", "coffee", "-qq"]);
        assert_eq!(two["occurTime"]["utc"], "2026-03-16T13:50:00+00:00");
    }

    /// And `-y` is a whole day back at midnight, not sixty minutes — and
    /// certainly not a state called "-y".
    #[test]
    fn yesterday_flags_step_whole_days_to_midnight() {
        let one = entry(&["occur", "coffee", "-y"]);
        assert_eq!(one["occurTime"]["utc"], "2026-03-15T00:00:00+00:00");
        assert_eq!(one["occurTimeUncertainty"], "P1D");
        assert!(one.get("state").is_none(), "-y leaked into state: {one}");
        let two = entry(&["occur", "coffee", "-yy"]);
        assert_eq!(two["occurTime"]["utc"], "2026-03-14T00:00:00+00:00");
    }

    /// da-4fyb: a date-only row slugs on the bare local date.
    #[test]
    fn a_date_only_row_slugs_on_the_date() {
        let e = entry(&["occur", "coffee", "-y"]);
        assert_eq!(e["slug"], "coffee:2026-03-15");
    }

    #[test]
    fn start_and_stop_canonicalize_state() {
        assert_eq!(entry(&["start", "work"])["state"], true);
        assert_eq!(entry(&["start", "work"])["verb"], "switch");
        assert_eq!(entry(&["stop", "work"])["state"], false);
    }

    #[test]
    fn switch_takes_its_state_from_the_first_bare_positional() {
        let e = entry(&["switch", "mood", "happy"]);
        assert_eq!(e["state"], "happy");
        assert_eq!(e["verb"], "switch");
    }

    /// Field-first argv is the CLI's own pre-pass, not a browser special
    /// case: `mood switch sad` swaps before the parser sees it.
    #[test]
    fn field_first_argv_swaps() {
        let e = entry(&["mood", "switch", "sad"]);
        assert_eq!(e["field"], "mood");
        assert_eq!(e["state"], "sad");
    }

    #[test]
    fn every_duration_spelling_normalizes_to_iso() {
        for (spelling, iso) in [
            ("dur=30m", "PT30M"),
            ("dur=1:30", "PT1H30M"),
            ("dur=PT30M", "PT30M"),
        ] {
            let e = entry(&["occur", "nap", spelling]);
            assert_eq!(e["dur"], iso, "for {spelling}");
            assert_eq!(e["verb"], "block", "for {spelling}");
        }
    }

    #[test]
    fn key_value_tokens_land_in_data() {
        let e = entry(&["occur", "coffee", "strength=strong"]);
        assert_eq!(e["data"]["strength"], "strong");
    }

    /// A unit rides inside the value, `{"v": 75.2, "u": "kg"}` — there is
    /// no `unit` column.
    #[test]
    fn a_unit_annotates_the_value_it_belongs_to() {
        let e = entry(&["occur", "weight", "amount=75.2kg"]);
        assert_eq!(e["data"]["amount"]["v"], 75.2);
        assert_eq!(e["data"]["amount"]["u"], "kg");
    }

    #[test]
    fn the_hid_is_base36_and_the_stamps_are_rfc3339() {
        let e = entry(&["occur", "coffee"]);
        assert_eq!(e["hid"].as_str().unwrap().len(), 12);
        assert_eq!(e["createTime"], "2026-03-16T14:00:00+00:00");
        assert_eq!(e["modifyTime"], e["createTime"]);
        assert_eq!(e["deviceId"], "web-test");
        assert_eq!(e["rawInput"], "occur coffee");
    }

    #[test]
    fn a_command_this_module_does_not_build_reports_unsupported() {
        for argv in [
            vec!["tail"],
            vec!["note", "word", "correr"],
            vec!["occur", "coffee", "--dry-run"],
            vec!["switch", "a", "x", ",", "y"],
        ] {
            let out = run(&argv);
            assert!(
                out.get("unsupported").is_some(),
                "expected unsupported for {argv:?}, got {out}"
            );
        }
    }

    #[test]
    fn an_unparseable_argv_reports_the_cli_s_own_message() {
        let out = run(&["occur"]);
        assert!(
            out["error"]
                .as_str()
                .unwrap()
                .contains("missing required positional"),
            "unhelpful error: {out}"
        );
    }

    /// The viewer's zone decides what a wall-clock `-t` means, and the row
    /// records the zone it was written in.
    #[test]
    fn a_clock_time_lands_in_the_viewer_s_zone() {
        let request = ParseRequest {
            argv: ["occur", "coffee", "-t", "0915"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            now_ms: NOW_MS,
            timezone: Some("America/Chicago".to_string()),
            offset_minutes: -300,
            device_id: "web-test".to_string(),
            hid_bytes: vec![7; 12],
        };
        let out = response(&request);
        let e = &out["entry"];
        assert_eq!(e["occurTime"]["utc"], "2026-03-16T14:15:00+00:00");
        assert_eq!(e["occurTime"]["timezone"], "America/Chicago");
        assert_eq!(e["occurTime"]["offsetMinutes"], -300);
    }

    /// A viewer whose zone we cannot name still has an offset, and that
    /// offset is what the row records — never a fabricated "UTC" (da-2pgd).
    #[test]
    fn an_unnameable_zone_keeps_the_offset_and_names_nothing() {
        let request = ParseRequest {
            argv: ["occur", "coffee", "-t", "0915"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            now_ms: NOW_MS,
            timezone: None,
            offset_minutes: 120,
            device_id: "web-test".to_string(),
            hid_bytes: vec![7; 12],
        };
        let e = response(&request)["entry"].clone();
        assert_eq!(e["occurTime"]["utc"], "2026-03-16T07:15:00+00:00");
        assert_eq!(e["occurTime"]["offsetMinutes"], 120);
        assert!(e["occurTime"].get("timezone").is_none());
    }
}
