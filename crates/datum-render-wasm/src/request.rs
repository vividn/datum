//! One JSON request in, the CLI's lines out.
//!
//! The request carries the entry as the browser holds it — the shape
//! `datum-parse-wasm` handed back — plus what this crate would otherwise
//! read from a host it does not have: the wall clock and the viewer's zone.
//! `datum-commands` is built here with `--no-default-features`, so the
//! code that would reach for either is not in the module.

use chrono::{DateTime, TimeZone, Utc};
use datum_commands::render::action::{format_info_line_at, RenderContext};
use datum_commands::render::colors::ColorOverride;
use datum_commands::render::{action_lines_at, Outcome};
use datum_core::entry::Entry;
use datum_core::field_spec::{FieldSpec, SpecRegistry};
use datum_core::ids::{Hid, Slug};
use datum_core::state::FieldStateEvents;
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
    /// The field's other live entries, as the browser stores them, in any
    /// order. The renderer compiles the field's state from them the way the
    /// CLI does from its database — the `∅` slot (prior state absent or
    /// IGNORED), the LEAD/TRAIL colours of a transition, the dot's ambient
    /// state. The row itself may be among them; every probe excludes it by
    /// hid. Empty means a field with no history.
    #[serde(default)]
    pub history: Vec<JsonValue>,
    /// Paint the lines.
    ///
    /// The gate the CLI reads from `DATUM_COLOR` / `NO_COLOR` / a tty
    /// (`render::colors::color_enabled`) answers "no" here whatever the
    /// caller wants: the module has no environment and no terminal. So
    /// colour is a bit on the request instead, forced for the length of
    /// this call and released after it — see
    /// `datum_commands::render::colors::ColorOverride`.
    #[serde(default)]
    pub color: bool,
    /// The field specs the browser holds, as they are stored
    /// (`field_specs.spec_json`). A field's declared `--color` is its
    /// colour on the terminal and must be its colour here too. A spec that
    /// will not parse is skipped: one field loses its declared colour,
    /// where refusing the request would lose the whole render.
    #[serde(default)]
    pub specs: Vec<JsonValue>,
}

/// The instant the caller says it is, as datum's own time.
pub(crate) fn now_from(
    now_ms: i64,
    timezone: Option<String>,
    offset_minutes: i32,
) -> Result<DatumTime, String> {
    Ok(DatumTime {
        utc: Utc
            .timestamp_millis_opt(now_ms)
            .single()
            .ok_or("now_ms out of range")?,
        offset_minutes,
        timezone,
        uncertainty: None,
    })
}

/// The browser's stored field specs as a registry the renderer can read.
pub(crate) fn registry_from_web(specs: &[JsonValue]) -> SpecRegistry {
    let mut registry = SpecRegistry::new();
    for spec in specs {
        if let Ok(spec) = serde_json::from_value::<FieldSpec>(spec.clone()) {
            registry.register_field(spec);
        }
    }
    registry
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
    let now = now_from(req.now_ms, req.timezone.clone(), req.offset_minutes)?;
    let registry = registry_from_web(&req.specs);
    // Held for the length of the render and dropped with it: the module is
    // reused across calls, and a gate left open would paint the next one.
    let _color = ColorOverride::set(req.color);
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
    // The same context the CLI builds for an action, from the same events —
    // theirs loaded from SQLite, ours from the rows the browser sent.
    let history = req
        .history
        .iter()
        .map(entry_from_web)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("history: {e}"))?;
    let events = FieldStateEvents::from_entries(history.iter());
    let ctx = RenderContext::from_events(Some(&events), &entry);
    let mut out = Vec::new();
    match outcome {
        Some(action) => {
            let lines = action_lines_at(&entry, action, Some(&registry), Some(&ctx), &now);
            out.push(lines.header);
            if !lines.info.is_empty() {
                out.push(lines.info);
            }
            out.extend(lines.data);
        }
        None => {
            let info = format_info_line_at(&entry, registry.fields.get(&entry.field), &ctx, &now);
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

    fn rendered_with(entry: &JsonValue, history: &[JsonValue]) -> String {
        let req = json!({
            "entry": entry,
            "action": null,
            "now_ms": NOW_MS,
            "timezone": "Europe/Berlin",
            "offset_minutes": OFFSET,
            "history": history,
        });
        let res: JsonValue = serde_json::from_str(&render_json(&req.to_string())).unwrap();
        assert!(res.get("error").is_none(), "render error: {res}");
        res["lines"][0].as_str().unwrap().to_string()
    }

    /// The `∅` slot follows the CLI's rule: the field's prior compiled state
    /// is absent or IGNORED. A field with no history gets it; one with a
    /// live state does not; one that was ignored gets it back.
    #[test]
    fn the_empty_set_marks_an_absent_or_ignored_prior_state() {
        let coffee = parsed(&["occur", "coffee", "-t", "1200"]);
        let alone = rendered_with(&coffee, &[]);
        assert!(alone.ends_with("coffee ∅●"), "{alone}");
        // The parser fixture mints every hid from the same bytes; the probe
        // excludes the row's own hid, so history rows need their own.
        let mut switched_on = parsed(&["switch", "coffee", "brewing", "-t", "1100"]);
        switched_on["hid"] = json!("hist00000001");
        let after_state = rendered_with(&coffee, std::slice::from_ref(&switched_on));
        assert!(after_state.ends_with("coffee ●"), "{after_state}");
        let mut ignored = parsed(&["switch", "coffee", "IGNORED", "-t", "1130"]);
        ignored["hid"] = json!("hist00000002");
        ignored["state"] = json!("IGNORED");
        let after_ignore = rendered_with(&coffee, &[switched_on, ignored]);
        assert!(after_ignore.ends_with("coffee ∅●"), "{after_ignore}");
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
