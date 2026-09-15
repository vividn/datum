//! The JSON contract between the browser and this module.
//!
//! One request shape, four response shapes. The response never carries an
//! exception across the ABI boundary — a malformed request, an unparseable
//! argv and a command this module does not build an entry for all come back
//! as JSON, so the caller renders a message instead of hitting a trap.

use datum_core::field_spec::{Alias, AliasEffect, FieldSpec};
use datum_parse::help;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::log::{self, Host};

/// One row of the caller's `aliases` table, as stored.
///
/// `alias_json` is the column's text and is deserialised here into the
/// same [`Alias`] that `datum-core`'s `db/alias.rs` serialises on the way
/// in — so the browser holds the row, never a second definition of its
/// shape (da-5kbj).
#[derive(Debug, Deserialize)]
pub struct AliasRow {
    pub name: String,
    pub alias_json: String,
}

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
    /// Every row of the caller's `aliases` table. A row whose
    /// `alias_json` does not deserialise is skipped, the way the browser
    /// already skips a `spec_json` it cannot read: one unusable alias
    /// costs its own name, not every command typed after it.
    #[serde(default)]
    pub aliases: Vec<AliasRow>,
    /// Every stored field spec, in `rowid` order — the order that decides
    /// a derived-alias collision, so it is the caller's job to preserve it.
    /// Carried as raw JSON for the same reason the aliases are: a spec this
    /// module cannot read costs its own derived names, not the command
    /// being typed.
    #[serde(default)]
    pub field_specs: Vec<JsonValue>,
}

impl ParseRequest {
    /// The alias rows this request carries, deserialised. A row that does
    /// not read is skipped rather than failing the invocation.
    fn aliases(&self) -> Vec<Alias> {
        self.aliases
            .iter()
            .filter_map(|row| serde_json::from_str::<Alias>(&row.alias_json).ok())
            .collect()
    }

    /// The field specs this request carries, deserialised — unreadable ones
    /// skipped, as above.
    fn specs(&self) -> Vec<FieldSpec> {
        self.field_specs
            .iter()
            .filter_map(|v| serde_json::from_value::<FieldSpec>(v.clone()).ok())
            .collect()
    }
}

/// Parse one JSON request into one JSON response.
///
/// Responses:
/// * `{"entry": {…}}` — the row to insert.
/// * `{"unsupported": "<reason>"}` — a well-formed command this module does
///   not build an entry for. The seam turns this into its
///   "not available yet in the browser" exit.
/// * `{"output": "<text>"}` — the command's own stdout, exit 0. `--help`,
///   `--version` and `--man` reach the caller as `ParseError` variants
///   whose documented contract is "render this and exit 0", so they are
///   rendered here rather than passed off as errors (da-e0m6).
/// * `{"alias": {…}}` — a `datum alias` command: `output` is the text to
///   print, `warning` (when present) the line for stderr, and `save` /
///   `delete` the write the caller must perform against its own `aliases`
///   table. `alias show` / `alias list` need no write and come back as
///   plain `output`.
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
    let aliases = request.aliases();
    let specs = request.specs();
    let host = Host {
        now,
        timezone: request.timezone.as_deref(),
        offset_minutes: request.offset_minutes,
        device_id: &request.device_id,
        hid_bytes: &request.hid_bytes,
        aliases: &aliases,
        specs: &specs,
    };
    match log::build(&request.argv, &host) {
        Ok(log::Parsed::Entry(entry)) => json!({ "entry": log::entry_to_json(&entry) }),
        Ok(log::Parsed::Alias(effect)) => alias_response(effect),
        Ok(log::Parsed::Unsupported(reason)) => json!({ "unsupported": reason }),
        // Discriminated on the variant by `special_output`, the same
        // function `datum-cli` prints through — never on the Display
        // string, which says three different things for these three
        // variants and changes whenever the `#[error(...)]` is reworded.
        Err(e) => match help::special_output(&e) {
            Some(text) => json!({ "output": text }),
            None => json!({ "error": e.to_string() }),
        },
    }
}

/// One [`AliasEffect`] as the caller's instruction.
///
/// A refusal is an error like any other parse error; the three that
/// succeed differ only in whether a row has to be written afterwards.
fn alias_response(effect: AliasEffect) -> JsonValue {
    match effect {
        AliasEffect::Refuse(message) => json!({ "error": message }),
        AliasEffect::Say(text) => json!({ "output": text }),
        AliasEffect::Nothing => json!({ "output": "" }),
        AliasEffect::Save {
            alias,
            text,
            warning,
        } => json!({ "alias": {
            "save": {
                "name": alias.name,
                // The column's text, produced by the type that reads it
                // back — never assembled in the browser.
                "aliasJson": serde_json::to_string(&alias).unwrap_or_default(),
            },
            "output": text,
            "warning": warning,
        }}),
        AliasEffect::Delete { name, text } => json!({ "alias": {
            "delete": name,
            "output": text,
        }}),
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
            aliases: Vec::new(),
            field_specs: Vec::new(),
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

    /// `--help` / `--version` are not errors: their contract is "render
    /// this and exit 0", and the browser gets the text, not `error:` glued
    /// to a Display string (da-e0m6). Both spellings of each, because they
    /// are separate variants with the same bug.
    #[test]
    fn help_and_version_come_back_as_output_not_error() {
        for argv in [vec!["help"], vec!["-h"], vec!["--help"]] {
            let out = run(&argv);
            assert!(out.get("error").is_none(), "{argv:?} still errors: {out}");
            let text = out["output"].as_str().unwrap_or_default();
            assert!(text.contains("USAGE:"), "no help text for {argv:?}: {out}");
        }
        for argv in [vec!["--version"], vec!["-V"]] {
            let out = run(&argv);
            assert!(out.get("error").is_none(), "{argv:?} still errors: {out}");
            let text = out["output"].as_str().unwrap_or_default();
            assert!(text.starts_with("datum "), "no banner for {argv:?}: {out}");
        }
    }

    /// The per-command spellings carry the subcommand, so the text is that
    /// command's page rather than the top-level one.
    #[test]
    fn per_command_help_renders_that_command() {
        for argv in [vec!["help", "occur"], vec!["occur", "--help"]] {
            let text = run(&argv)["output"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            assert!(text.contains("datum occur"), "not occur's page: {text}");
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

    /// One `aliases` row, as the browser holds it: the name plus the
    /// column text `db/alias.rs` writes.
    fn alias_row(name: &str, expands_to: &str) -> AliasRow {
        AliasRow {
            name: name.to_string(),
            alias_json: serde_json::to_string(&Alias {
                name: name.to_string(),
                expands_to: expands_to.to_string(),
                description: None,
            })
            .unwrap(),
        }
    }

    fn with_aliases(argv: &[&str], aliases: Vec<AliasRow>) -> JsonValue {
        with_world(argv, aliases, Vec::new())
    }

    fn with_world(argv: &[&str], aliases: Vec<AliasRow>, specs: Vec<FieldSpec>) -> JsonValue {
        let field_specs = specs
            .iter()
            .map(|s| serde_json::to_value(s).unwrap())
            .collect();
        response(&ParseRequest {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            now_ms: NOW_MS,
            timezone: Some("UTC".to_string()),
            offset_minutes: 0,
            device_id: "web-test".to_string(),
            hid_bytes: vec![7; 12],
            aliases,
            field_specs,
        })
    }

    /// The whole point: a name the caller's `aliases` table knows expands
    /// before the parser sees it, and the row records both what was typed
    /// and what it became.
    #[test]
    fn an_alias_expands_and_the_row_carries_its_provenance() {
        let out = with_aliases(
            &["cof", "-t", "0915"],
            vec![alias_row("cof", "occur coffee strength=strong")],
        );
        let e = &out["entry"];
        assert_eq!(e["field"], "coffee");
        assert_eq!(e["data"]["strength"], "strong");
        assert_eq!(e["rawInput"], "cof -t 0915", "raw_input is the typed line");
        assert_eq!(e["aliasChain"][0]["name"], "cof");
        assert_eq!(
            e["aliasChain"][0]["expansion"],
            "occur coffee strength=strong"
        );
    }

    /// vividn's case (da-5kbj): a v2 alias carrying `--id %comment%`, so the
    /// row is slugged by what the comment said rather than by the minute.
    #[test]
    fn an_id_region_inside_an_alias_slugs_on_the_data() {
        let out = with_aliases(
            &["tx", "amount=40", "comment=electricity", "-t", "0915"],
            vec![alias_row("tx", "occur tx --id %comment%")],
        );
        let e = &out["entry"];
        assert_eq!(e["slug"], "tx:electricity");
        assert_eq!(e["idStructure"], "%comment%");
        assert_eq!(e["data"]["amount"], 40);
        assert_eq!(e["data"]["comment"], "electricity");
    }

    /// A spec's derived aliases make its bare name a command, the same as
    /// in the CLI — `sleep` starts sleep, `happy` switches mood.
    #[test]
    fn a_field_spec_makes_its_own_names_commands() {
        let mut mood = FieldSpec::with_verb("mood", datum_core::action::Action::Switch);
        mood.allowed_states = Some(vec!["happy".to_string()]);
        let specs = vec![mood];

        let bare = with_world(&["mood", "-t", "0915"], Vec::new(), specs.clone());
        assert_eq!(bare["entry"]["field"], "mood");
        assert_eq!(bare["entry"]["state"], true);

        let state = with_world(&["happy", "-t", "0916"], Vec::new(), specs.clone());
        assert_eq!(state["entry"]["field"], "mood");
        assert_eq!(state["entry"]["state"], "happy");

        // A user row of the same name shadows the derived one.
        let shadowed = with_world(
            &["happy", "-t", "0917"],
            vec![alias_row("happy", "occur joy")],
            specs,
        );
        assert_eq!(shadowed["entry"]["field"], "joy");
    }

    /// Two hops both land on the chain, in the order they fired.
    #[test]
    fn a_two_hop_chain_records_both_hops() {
        let out = with_aliases(
            &["a", "-t", "0915"],
            vec![
                alias_row("a", "b strength=strong"),
                alias_row("b", "occur coffee"),
            ],
        );
        let chain = out["entry"]["aliasChain"].as_array().unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0]["name"], "a");
        assert_eq!(chain[1]["name"], "b");
        assert_eq!(out["entry"]["field"], "coffee");
    }

    /// A cycle is the CLI's own refusal, not a hang.
    #[test]
    fn a_cycle_is_refused_with_the_cli_s_message() {
        let out = with_aliases(&["a"], vec![alias_row("a", "b"), alias_row("b", "a")]);
        assert_eq!(out["error"], "Alias cycle: a → b → a");
    }

    /// `alias set` / `alias remove` come back as the write to perform plus
    /// the CLI's own sentence; `list` and `show` need no write at all.
    #[test]
    fn the_alias_command_answers_with_the_write_and_the_text() {
        let saved = with_aliases(&["alias", "set", "cof", "occur coffee"], Vec::new());
        assert_eq!(saved["alias"]["save"]["name"], "cof");
        assert_eq!(saved["alias"]["output"], "✓ Alias 'cof' saved.");
        let stored: Alias =
            serde_json::from_str(saved["alias"]["save"]["aliasJson"].as_str().unwrap()).unwrap();
        assert_eq!(stored.expands_to, "occur coffee");

        let rows = || vec![alias_row("cof", "occur coffee")];
        assert_eq!(
            with_aliases(&["alias", "list"], rows())["output"],
            "  cof → datum occur coffee"
        );
        assert_eq!(
            with_aliases(&["alias", "show", "cof"], rows())["output"],
            "cof → datum occur coffee"
        );
        assert_eq!(
            with_aliases(&["alias", "remove", "cof"], rows())["alias"]["delete"],
            "cof"
        );
        assert_eq!(
            with_aliases(&["alias", "remove", "nope"], rows())["error"],
            "no user alias 'nope'"
        );
    }

    /// A derived alias has no row to remove, so `alias remove` says where
    /// the name actually comes from instead of reporting a missing row.
    #[test]
    fn removing_a_derived_alias_names_the_spec_that_owns_it() {
        let spec = FieldSpec::with_verb("sleep", datum_core::action::Action::Start);
        let out = with_world(&["alias", "remove", "sleep"], Vec::new(), vec![spec]);
        assert_eq!(
            out["error"],
            "'sleep' is a derived alias from the 'sleep' field spec; edit that spec to change it"
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
            aliases: Vec::new(),
            field_specs: Vec::new(),
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
            aliases: Vec::new(),
            field_specs: Vec::new(),
        };
        let e = response(&request)["entry"].clone();
        assert_eq!(e["occurTime"]["utc"], "2026-03-16T07:15:00+00:00");
        assert_eq!(e["occurTime"]["offsetMinutes"], 120);
        assert!(e["occurTime"].get("timezone").is_none());
    }
}
