//! One JSON request in, the CLI's lines out.
//!
//! The request carries the entry as the browser holds it — the shape
//! `datum-parse-wasm` handed back — plus what this crate would otherwise
//! read from a host it does not have: the wall clock and the viewer's zone.
//! `datum-commands` is built here with `--no-default-features`, so the
//! code that would reach for either is not in the module.

use chrono::{DateTime, TimeZone, Utc};
use datum_commands::render::action::{format_info_line_at, RenderContext};
use datum_commands::render::{action_lines_at, Outcome};
use datum_core::entry::Entry;
use datum_core::ids::{Hid, Slug};
use datum_parse::{DatumTime, Dur, Verb};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

/// What the module needs from the browser for one render.
#[derive(Debug, Deserialize)]
pub struct RenderRequest {
    /// The entry, exactly as `datum-parse-wasm` produced it: camelCase keys,
    /// `occurTime: {utc, offsetMinutes, timezone}`, RFC 3339 stamps.
    pub entry: JsonValue,
    /// The action to head the output with — `create`, `update`, `delete`,
    /// `exists`, `nodiff`, `failed` — or absent for the info line alone, the
    /// way `tail` shows a row.
    #[serde(default)]
    pub action: Option<String>,
    pub now_ms: i64,
    /// The viewer's IANA zone, when the browser can name one.
    #[serde(default)]
    pub timezone: Option<String>,
    /// The viewer's UTC offset in minutes, east-positive.
    pub offset_minutes: i32,
}

/// `{"lines": [...]}` on success, `{"error": "..."}` otherwise. Never panics
/// on bad input: the browser reports the message and carries on.
pub fn render_json(body: &str) -> String {
    match render(body) {
        Ok(lines) => json!({ "lines": lines }).to_string(),
        Err(e) => json!({ "error": e }).to_string(),
    }
}

fn render(body: &str) -> Result<Vec<String>, String> {
    let req: RenderRequest = serde_json::from_str(body).map_err(|e| format!("bad request: {e}"))?;
    let entry = entry_from_web(&req.entry)?;
    let now = DatumTime {
        utc: Utc
            .timestamp_millis_opt(req.now_ms)
            .single()
            .ok_or("now_ms out of range")?,
        offset_minutes: req.offset_minutes,
        timezone: req.timezone.clone(),
        uncertainty: None,
    };
    let outcome = match req.action.as_deref() {
        None => None,
        Some("create") => Some(Outcome::Create),
        Some("update") => Some(Outcome::Update),
        Some("delete") => Some(Outcome::Delete),
        Some("exists") => Some(Outcome::Exists),
        Some("nodiff") => Some(Outcome::NoDiff),
        Some("failed") => Some(Outcome::Failed),
        Some(other) => return Err(format!("unknown action `{other}`")),
    };
    // No neighbouring rows are known here, so no lead/trail context: the
    // same default the CLI uses when it prints a lone action.
    let ctx = RenderContext::default();
    let mut out = Vec::new();
    match outcome {
        Some(action) => {
            let lines = action_lines_at(&entry, action, None, Some(&ctx), &now);
            out.push(lines.header);
            if !lines.info.is_empty() {
                out.push(lines.info);
            }
            out.extend(lines.data);
        }
        None => {
            let info = format_info_line_at(&entry, None, &ctx, &now);
            if !info.is_empty() {
                out.push(info);
            }
        }
    }
    Ok(out)
}

fn str_field(v: &JsonValue, key: &str) -> Result<String, String> {
    v.get(key)
        .and_then(JsonValue::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("entry.{key} missing or not a string"))
}

fn opt_str(v: &JsonValue, key: &str) -> Option<String> {
    v.get(key).and_then(JsonValue::as_str).map(str::to_string)
}

fn rfc3339(s: &str, what: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| format!("{what}: {e}"))
}

/// The browser's entry back into datum's own row type — the inverse of
/// `datum_parse_wasm::log::entry_to_json`, which is what produced it.
///
/// Strict where rendering depends on it (times, verb, hid, slug, field) and
/// lenient where it does not (device id, raw input, the bookkeeping columns
/// the browser never had): a missing `deviceId` is not a reason to print
/// nothing.
pub fn entry_from_web(v: &JsonValue) -> Result<Entry, String> {
    let occur = v.get("occurTime").ok_or("entry.occurTime missing")?;
    let occur_time = DatumTime {
        utc: rfc3339(&str_field(occur, "utc")?, "occurTime.utc")?,
        offset_minutes: occur
            .get("offsetMinutes")
            .and_then(JsonValue::as_i64)
            .ok_or("occurTime.offsetMinutes missing")? as i32,
        timezone: opt_str(occur, "timezone"),
        uncertainty: opt_str(v, "occurTimeUncertainty"),
    };
    let verb: Verb = serde_json::from_value(json!(str_field(v, "verb")?))
        .map_err(|e| format!("entry.verb: {e}"))?;
    let dur = match opt_str(v, "dur") {
        Some(d) => Some(Dur::parse(&d).ok_or_else(|| format!("entry.dur `{d}` did not parse"))?),
        None => None,
    };
    let create_time = rfc3339(&str_field(v, "createTime")?, "createTime")?;
    let modify_time = match opt_str(v, "modifyTime") {
        Some(m) => rfc3339(&m, "modifyTime")?,
        None => create_time,
    };
    Ok(Entry {
        hid: Hid(str_field(v, "hid")?),
        slug: Slug(str_field(v, "slug")?),
        content_hash: opt_str(v, "contentHash").unwrap_or_default(),
        field: str_field(v, "field")?,
        verb,
        state: v.get("state").cloned(),
        dur,
        following_state: v.get("followingState").cloned(),
        data: v.get("data").cloned(),
        occur_time: Some(occur_time),
        lat: None,
        lon: None,
        location: None,
        create_time,
        modify_time,
        device_id: opt_str(v, "deviceId").unwrap_or_default(),
        id_structure: None,
        field_structure: None,
        deleted_at: None,
        superseded_by: None,
        raw_input: opt_str(v, "rawInput"),
        alias_chain: None,
        parser_version: 0,
        user_asserted_last_state: None,
        extra_metadata: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-09-08T14:17:25Z, in a zone two hours east — the example in the
    /// crate docs, which is also the one vividn measured.
    const NOW_MS: i64 = 1_788_877_045_000;
    const OFFSET: i32 = 120;

    /// Exactly what the browser does: parse the argv with datum's parser,
    /// hand the row it built to this crate.
    fn parsed(argv: &[&str]) -> JsonValue {
        let req = json!({
            "argv": argv,
            "now_ms": NOW_MS,
            "timezone": "Europe/Berlin",
            "offset_minutes": OFFSET,
            "device_id": "test",
            "hid_bytes": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        });
        let res: JsonValue =
            serde_json::from_str(&datum_parse_wasm::parse_json(&req.to_string())).unwrap();
        res.get("entry")
            .cloned()
            .expect("the parser built an entry")
    }

    fn rendered(entry: &JsonValue, action: Option<&str>) -> Vec<String> {
        let req = json!({
            "entry": entry,
            "action": action,
            "now_ms": NOW_MS,
            "timezone": "Europe/Berlin",
            "offset_minutes": OFFSET,
        });
        let res: JsonValue = serde_json::from_str(&render_json(&req.to_string())).unwrap();
        assert!(res.get("error").is_none(), "render error: {res}");
        res["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn a_switch_renders_the_cli_s_two_lines() {
        let entry = parsed(&["switch", "mood", "happy"]);
        let lines = rendered(&entry, Some("create"));
        assert_eq!(lines.len(), 2, "{lines:?}");
        // The header carries the SHORT hid — five characters, `CREATE 65nfi …`
        // — because that is what the CLI prints and what a person types back.
        let hid = entry["hid"].as_str().unwrap();
        let slug = entry["slug"].as_str().unwrap();
        assert_eq!(lines[0], format!("CREATE {} {slug}", &hid[..5]));
        // 16:17:25 in a +2 zone: clock with seconds and offset, then field, then state.
        assert!(lines[1].starts_with("16:17:25+2"), "{}", lines[1]);
        assert!(lines[1].contains("mood"), "{}", lines[1]);
        assert!(lines[1].contains("happy"), "{}", lines[1]);
    }

    #[test]
    fn a_point_with_no_state_renders_time_and_field_only() {
        let entry = parsed(&["occur", "coffee"]);
        let lines = rendered(&entry, Some("create"));
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].starts_with("CREATE "));
        assert!(lines[1].starts_with("16:17:25+2"), "{}", lines[1]);
        assert!(lines[1].contains("coffee"), "{}", lines[1]);
    }

    #[test]
    fn no_action_renders_the_info_line_alone() {
        let entry = parsed(&["switch", "mood", "happy"]);
        let lines = rendered(&entry, None);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with("16:17:25+2"), "{}", lines[0]);
    }

    #[test]
    fn a_day_precise_row_shows_its_date_not_a_clock() {
        // `-y` stores midnight as an anchor with `P1D` uncertainty; the CLI
        // prints the date rather than a 00:00 it disclaims (da-kb8p).
        let entry = parsed(&["occur", "coffee", "-y"]);
        assert_eq!(entry["occurTimeUncertainty"], "P1D");
        let lines = rendered(&entry, None);
        assert!(!lines[0].contains("00:00"), "{}", lines[0]);
    }

    #[test]
    fn the_web_entry_round_trips_into_the_row() {
        let mut entry = parsed(&["switch", "mood", "happy"]);
        // A duration in the ISO form `entry_to_json` emits, so the inverse is
        // exercised on it without depending on which flag the parser takes.
        entry["dur"] = json!("PT5M");
        let row = entry_from_web(&entry).unwrap();
        assert_eq!(row.hid.0, entry["hid"]);
        assert_eq!(row.slug.0, entry["slug"]);
        assert_eq!(row.field, "mood");
        assert_eq!(row.state, Some(json!("happy")));
        assert!(row.dur.is_some(), "dur carried across");
        let t = row.occur_time.unwrap();
        assert_eq!(t.offset_minutes, OFFSET);
        assert_eq!(t.timezone.as_deref(), Some("Europe/Berlin"));
    }

    #[test]
    fn an_unknown_action_is_an_error_not_a_panic() {
        let entry = parsed(&["occur", "coffee"]);
        let req = json!({ "entry": entry, "action": "explode", "now_ms": NOW_MS, "offset_minutes": OFFSET });
        let res: JsonValue = serde_json::from_str(&render_json(&req.to_string())).unwrap();
        assert!(res["error"].as_str().unwrap().contains("explode"));
    }

    #[test]
    fn garbage_in_is_an_error_not_a_panic() {
        let res: JsonValue = serde_json::from_str(&render_json("not json")).unwrap();
        assert!(res.get("error").is_some());
    }
}
