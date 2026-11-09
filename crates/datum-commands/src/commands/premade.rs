//! `datum setup --premade` — the premade examples (da-rky8h): fields ready
//! to set up, by theme, from `premade.toml`, compiled in. `--json` prints
//! the file whole, each example with the spec it sets up, as `setup F
//! --json` prints one; datum.day's catalog reads that.

use anyhow::{ensure, Context, Result};
use datum_core::{action::Action, field_spec::FieldSpec};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use unicode_width::UnicodeWidthStr;

use datum_parse::input::Show;

const FILE: &str = include_str!("../../premade.toml");

/// The kinds an example may be, as a spec stores them.
const KINDS: [&str; 5] = ["occur", "start", "switch", "add", "allday"];

#[derive(Serialize, Deserialize)]
pub struct Premade {
    pub themes: Vec<String>,
    #[serde(rename(deserialize = "example"))]
    pub examples: Vec<Example>,
}

#[derive(Serialize, Deserialize)]
pub struct Example {
    pub field: String,
    pub theme: String,
    pub kind: String,
    pub symbol: String,
    pub color: String,
    pub shows: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub choices: IndexMap<String, Choice>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub states: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub never_false: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<Alias>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol_by: Option<Value>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub place: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_above: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_below: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub day: Value,
    #[serde(default, rename = "as", skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<Variant>,
    /// The spec it sets up, as `setup F --json` prints one: filled by [`load`].
    #[serde(default, skip_deserializing)]
    pub spec: Value,
    /// Any other key: the spec's own, given to `setup --from-json` as it is.
    #[serde(flatten)]
    pub more: Map<String, Value>,
}

/// One way to log an example that gives its own keys: weight's lbs or kg.
#[derive(Serialize, Deserialize)]
pub struct Choice {
    pub keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_above: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_below: Option<String>,
    #[serde(default, skip_deserializing)]
    pub spec: Value,
}

/// Another kind for the same field: menstruation each day, not on and off.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variant {
    pub kind: String,
    pub label: String,
    pub shows: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub day: Value,
    #[serde(default, skip_deserializing)]
    pub spec: Value,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alias {
    pub name: String,
    pub expands_to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// The examples, each checked and given its specs; an error names the one at fault.
pub fn load() -> Result<Premade> {
    let mut premade: Premade = toml::from_str(FILE).context("premade.toml")?;
    let mut seen = std::collections::HashSet::new();
    for e in &mut premade.examples {
        let at = format!("premade.toml: {}", e.field);
        ensure!(seen.insert(e.field.clone()), "{at}: listed twice");
        ensure!(
            premade.themes.contains(&e.theme),
            "{at}: no theme '{}'",
            e.theme
        );
        check(e, &e.kind, &e.day).context(at.clone())?;
        ensure!(
            e.variants.len() <= 1 && (e.variants.is_empty() || e.label.is_some()),
            "{at}: one variant at most, and a label beside it"
        );
        for v in &e.variants {
            ensure!(v.kind != e.kind, "{at}: a variant of its own kind");
            check(e, &v.kind, &v.day).with_context(|| format!("{at}, as {}", v.kind))?;
        }
        e.spec = spec(e, &e.kind, &e.keys).context(at.clone())?;
        let choices = e.choices.values().map(|c| spec(e, &e.kind, &c.keys));
        let variants = e.variants.iter().map(|v| spec(e, &v.kind, &e.keys));
        let specs = choices
            .chain(variants)
            .collect::<Result<Vec<_>>>()
            .context(at)?;
        let places = e.choices.values_mut().map(|c| &mut c.spec);
        for (place, spec) in places
            .chain(e.variants.iter_mut().map(|v| &mut v.spec))
            .zip(specs)
        {
            *place = spec;
        }
    }
    for theme in &premade.themes {
        ensure!(
            premade.examples.iter().any(|e| &e.theme == theme),
            "premade.toml: theme '{theme}' has no example"
        );
    }
    Ok(premade)
}

/// `kind` is one of the five, and `day` is a day of it: every clock in it
/// in the day, each entry the shape the kind logs, a change's states its own.
fn check(e: &Example, kind: &str, day: &Value) -> Result<()> {
    ensure!(KINDS.contains(&kind), "no kind '{kind}': one of {KINDS:?}");
    let clock = |entry: &Value, key: &str, end: bool| -> Result<()> {
        let t = entry[key]
            .as_str()
            .with_context(|| format!("each needs `{key}`"))?;
        let ok = t.len() == 5
            && (chrono::NaiveTime::parse_from_str(t, "%H:%M").is_ok() || (end && t == "24:00"));
        ensure!(ok, "`{key} = \"{t}\"` is no clock of the day (HH:MM)");
        Ok(())
    };
    let entries = || day.as_array().context("its day is a list of entries");
    match kind {
        "add" => ensure!(day.is_null(), "a record is untimed: it has no day"),
        "allday" => ensure!(
            !day.is_null() && !day.is_array() && !day.is_object(),
            "an all-day day is one value"
        ),
        "occur" => {
            for entry in entries()? {
                clock(entry, "at", false)?;
                ensure!(entry.get("state").is_none(), "a moment has no state");
            }
        }
        "start" => {
            for entry in entries()? {
                clock(entry, "from", false)?;
                clock(entry, "to", true)?;
            }
        }
        _ => {
            for entry in entries()? {
                clock(entry, "at", false)?;
                let state = &entry["state"];
                let listed = state
                    .as_str()
                    .is_some_and(|s| e.states.is_empty() || e.states.contains_key(s));
                ensure!(
                    state == &json!(false) || listed,
                    "no state {state} of its own"
                );
            }
        }
    }
    Ok(())
}

/// The spec `e` sets up as `kind` with `keys`, as `setup F --json` prints
/// it: built as `setup --from-json` reads one, its aliases made as a save
/// makes them.
fn spec(e: &Example, kind: &str, keys: &[String]) -> Result<Value> {
    let mut m = e.more.clone();
    m.extend(e.place.clone());
    m.insert("field".into(), json!(e.field));
    m.insert("verb".into(), json!(kind));
    m.insert("color".into(), json!(e.color));
    m.insert("symbol".into(), json!(e.symbol));
    m.insert("data_keys".into(), json!(keys));
    if kind == "switch" && !e.states.is_empty() {
        m.insert(
            "allowed_states".into(),
            json!(e.states.keys().collect::<Vec<_>>()),
        );
        m.insert("state_colors".into(), json!(e.states));
    }
    if e.never_false {
        m.insert("never_false".into(), json!(true));
    }
    if let Some(by) = &e.symbol_by {
        m.insert("symbol_by".into(), by.clone());
    }
    if !e.aliases.is_empty() {
        let aliases: Vec<Value> = e
            .aliases
            .iter()
            .map(|a| json!({"state": a.name, "expands_to": a.expands_to, "kind": a.kind.as_deref().unwrap_or("bare_field")}))
            .collect();
        m.insert("no_autoalias".into(), json!(true));
        m.insert("derived_aliases".into(), json!(aliases));
    }
    let mut spec: FieldSpec = serde_json::from_value(Value::Object(m))?;
    let unknown: Vec<&String> = spec.extra.keys().collect();
    ensure!(unknown.is_empty(), "no example or spec key {unknown:?}");
    spec.validate_verb().map_err(|e| anyhow::anyhow!(e))?;
    spec.regenerate_derived_aliases();
    Ok(serde_json::to_value(super::spec::spec_json(&spec))?)
}

/// `setup --premade`: by theme, each example's symbol, field, kind and line;
/// `--json` the file whole, with the specs.
pub fn premade(show: Show) -> Result<()> {
    let premade = load()?;
    if show == Show::Json {
        println!("{}", serde_json::to_string(&premade)?);
        return Ok(());
    }
    for theme in &premade.themes {
        println!("{theme}");
        for e in premade.examples.iter().filter(|e| &e.theme == theme) {
            // The kinds in the words a person reads: moment, on/off, change.
            let kinds: Vec<String> = std::iter::once(&e.spec)
                .chain(e.variants.iter().map(|v| &v.spec))
                .map(|s| serde_json::from_value::<Action>(s["verb"].clone()).map(Action::kind))
                .collect::<Result<_, _>>()?;
            let pad = " ".repeat(2usize.saturating_sub(e.symbol.width()));
            println!(
                "  {}{pad} {:<13} {:<16} {}",
                e.symbol,
                e.field,
                kinds.join(" or "),
                e.shows
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_parses_and_every_example_holds() {
        let premade = load().unwrap();
        assert!(premade.examples.len() > 40);
        // Menstruation is kept two ways, and the second is all day.
        let m = premade
            .examples
            .iter()
            .find(|e| e.field == "menstruation")
            .unwrap();
        assert_eq!(m.variants[0].kind, "allday");
        assert_eq!(m.variants[0].spec["verb"], "allday");
    }

    /// Each refusal [`check`] makes, so a broken day cannot pass for the wrong reason.
    #[test]
    fn a_day_that_does_not_fit_its_kind_is_refused() {
        let base = load()
            .unwrap()
            .examples
            .into_iter()
            .find(|e| e.field == "room")
            .unwrap();
        for (kind, day, says) in [
            ("occur", json!([{"at": "25:00"}]), "no clock"),
            ("occur", json!([{"at": "7:00"}]), "no clock"),
            ("occur", json!([{"at": "07:00", "state": "x"}]), "no state"),
            ("start", json!([{"from": "07:00"}]), "needs `to`"),
            (
                "start",
                json!([{"from": "24:00", "to": "07:00"}]),
                "no clock",
            ),
            (
                "switch",
                json!([{"at": "07:00", "state": "attic"}]),
                "no state",
            ),
            ("allday", json!([1]), "one value"),
            ("add", json!(1), "untimed"),
            ("block", json!(null), "no kind"),
        ] {
            let err = check(&base, kind, &day).unwrap_err().to_string();
            assert!(err.contains(says), "{kind} {day}: {err}");
        }
        assert!(check(&base, "start", &json!([{"from": "23:00", "to": "24:00"}])).is_ok());
        assert!(check(&base, "switch", &json!([{"at": "07:00", "state": false}])).is_ok());
    }
}
