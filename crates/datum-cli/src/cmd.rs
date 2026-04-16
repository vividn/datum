//! Command dispatch and argument → Entry conversion.

use anyhow::{bail, Context, Result};
use datum_core::{
    db::Db,
    entry::{Entry, Verb},
    field_spec::Alias,
    time::{parse_date_arg, parse_time_arg, DatumTime, TimeBuilder},
};
use serde_json::Value as JsonValue;

use crate::{
    args::{AliasCommand, Cli, Command, LogArgs, MigrateArgs, SpecCommand, TailArgs},
    config::device_id,
    output::{print_entries, print_entry, print_logged},
};

pub fn dispatch(cli: Cli, db: &Db) -> Result<()> {
    match cli.command {
        Command::Occur(args) => log_entry(Verb::Occur, args, db),
        Command::Start(args) => log_entry(Verb::Start, args, db),
        Command::Stop(args) => log_entry(Verb::Stop, args, db),
        Command::Switch(args) => log_entry(Verb::Switch, args, db),
        Command::Note(args) => log_entry(Verb::Note, args, db),
        Command::Tail(args) => cmd_tail(args, db),
        Command::Now => cmd_now(db),
        Command::Spec(sub) => cmd_spec(sub, db),
        Command::Alias(sub) => cmd_alias(sub, db),
        Command::Migrate(args) => cmd_migrate(args, db),
    }
}

// ─── Logging ─────────────────────────────────────────────────────────────────

fn log_entry(verb: Verb, args: LogArgs, db: &Db) -> Result<()> {
    let (field, value, extra_data) = parse_log_args(&args)?;
    let time_start = build_time(&args)?;
    let unit = args.unit.clone();
    let device = device_id()?;

    let entry = Entry::new(field, verb, value, extra_data, unit, Some(time_start), None, device);

    match db.insert_entry(&entry) {
        Ok(()) => {
            print_logged(&entry);
            Ok(())
        }
        Err(datum_core::error::DatumError::Duplicate(msg)) => {
            eprintln!("⚠ Duplicate entry skipped: {msg}");
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// Parse LogArgs into (field, primary_value, extra_data).
///
/// The first positional may be the field (when verb is the subcommand) or a
/// field-like token. Remaining positionals become the primary value or key=value pairs.
fn parse_log_args(args: &LogArgs) -> Result<(String, Option<JsonValue>, Option<JsonValue>)> {
    let field = args.field_or_verb.clone();

    // Collect positional rest: could be "value key=val key=val ..."
    let mut value: Option<JsonValue> = None;
    let mut extra = serde_json::Map::new();

    for token in &args.rest {
        if let Some((k, v)) = token.split_once('=') {
            extra.insert(k.to_string(), parse_value_token(v));
        } else if value.is_none() {
            value = Some(parse_value_token(token));
        } else {
            // Extra positional after value — append to an array if value is already set
            match value.as_mut().unwrap() {
                JsonValue::Array(arr) => arr.push(parse_value_token(token)),
                existing => {
                    let prev = existing.clone();
                    *existing = JsonValue::Array(vec![prev, parse_value_token(token)]);
                }
            }
        }
    }

    // Add -k KEY=VALUE pairs to extra
    for kv in &args.keys {
        if let Some((k, v)) = kv.split_once('=') {
            extra.insert(k.to_string(), parse_value_token(v));
        }
    }

    if let Some(comment) = &args.comment {
        extra.insert("comment".to_string(), JsonValue::String(comment.clone()));
    }

    let extra_data = if extra.is_empty() {
        None
    } else {
        Some(JsonValue::Object(extra))
    };

    Ok((field, value, extra_data))
}

/// Parse a single value token. Tries number, bool, then falls back to string.
fn parse_value_token(s: &str) -> JsonValue {
    // Unit-annotated number: "75.2kg", "500ml"
    if let Some((num, unit)) = split_numeric_unit(s) {
        if let Ok(n) = num.parse::<f64>() {
            return serde_json::json!({"v": n, "u": unit});
        }
    }
    if let Ok(n) = s.parse::<i64>() {
        return JsonValue::Number(n.into());
    }
    if let Ok(n) = s.parse::<f64>() {
        return serde_json::json!(n);
    }
    if s == "true" { return JsonValue::Bool(true); }
    if s == "false" { return JsonValue::Bool(false); }
    JsonValue::String(s.to_string())
}

/// Split "75.2kg" → Some(("75.2", "kg")), "hello" → None.
fn split_numeric_unit(s: &str) -> Option<(&str, &str)> {
    let idx = s
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit() || *c == '.' || *c == '-')
        .last()
        .map(|(i, _)| i + 1)?;
    if idx == 0 || idx == s.len() {
        return None;
    }
    let (num, unit) = s.split_at(idx);
    if unit.chars().all(|c| c.is_alphabetic()) {
        Some((num, unit))
    } else {
        None
    }
}

fn build_time(args: &LogArgs) -> Result<DatumTime> {
    let mut builder = TimeBuilder::now()
        .with_yesterday_count(args.yesterday as i64)
        .with_quick_count(args.quick as i64);

    if let Some(d) = &args.date {
        let da = parse_date_arg(d)?;
        builder = builder.with_date_arg(da);
    }
    if let Some(t) = &args.time {
        let ta = parse_time_arg(t)?;
        builder = builder.with_time_arg(ta);
    }

    Ok(builder.build())
}

// ─── Tail / Now ──────────────────────────────────────────────────────────────

fn cmd_tail(args: TailArgs, db: &Db) -> Result<()> {
    let entries = if let Some(field) = &args.field {
        db.entries_for_field(field, Some(args.count))?
    } else {
        // All fields, most recent N
        let now = chrono::Utc::now();
        let far_past = chrono::DateTime::UNIX_EPOCH;
        let mut all = db.entries_in_range(far_past, now)?;
        all.sort_by(|a, b| {
            let ta = a.time_start.as_ref().map(|t| t.utc).unwrap_or(a.created_at);
            let tb = b.time_start.as_ref().map(|t| t.utc).unwrap_or(b.created_at);
            tb.cmp(&ta)
        });
        all.truncate(args.count);
        all
    };
    print_entries(&entries);
    Ok(())
}

fn cmd_now(db: &Db) -> Result<()> {
    // Show the most recent entry per field where verb is switch or start (open block).
    // Simple approach: last entry per field.
    let now = chrono::Utc::now();
    let far_past = chrono::DateTime::UNIX_EPOCH;
    let all = db.entries_in_range(far_past, now)?;

    let mut latest: std::collections::HashMap<String, &Entry> = std::collections::HashMap::new();
    // all is sorted ascending — iterate to keep the latest
    for entry in &all {
        latest.insert(entry.field.clone(), entry);
    }

    let mut fields: Vec<_> = latest.values().collect();
    fields.sort_by_key(|e| &e.field);

    if fields.is_empty() {
        println!("No entries yet.");
    } else {
        for e in fields {
            print_entry(e);
        }
    }
    Ok(())
}

// ─── Spec management ─────────────────────────────────────────────────────────

fn cmd_spec(sub: SpecCommand, db: &Db) -> Result<()> {
    match sub {
        SpecCommand::Show { field } => {
            let reg = db.load_registry()?;
            match reg.field_spec(&field) {
                Some(spec) => println!("{}", serde_json::to_string_pretty(spec)?),
                None => println!("No spec defined for field '{field}'"),
            }
        }
        SpecCommand::Set { field, json } => {
            let mut spec: datum_core::field_spec::FieldSpec = serde_json::from_str(&json)
                .with_context(|| "Invalid field spec JSON")?;
            spec.field = field;
            db.save_field_spec(&spec)?;
            println!("✓ Spec saved for '{}'", spec.field);
        }
        SpecCommand::List => {
            let reg = db.load_registry()?;
            if reg.fields.is_empty() {
                println!("No field specs defined.");
            } else {
                for (name, _spec) in &reg.fields {
                    println!("  {name}");
                }
            }
        }
    }
    Ok(())
}

// ─── Alias management ────────────────────────────────────────────────────────

fn cmd_alias(sub: AliasCommand, db: &Db) -> Result<()> {
    match sub {
        AliasCommand::Show { name } => {
            let reg = db.load_registry()?;
            match reg.aliases.get(&name) {
                Some(alias) => println!("{name} → datum {}", alias.expands_to),
                None => println!("No alias '{name}'"),
            }
        }
        AliasCommand::Set { name, expansion } => {
            let alias = Alias { name: name.clone(), expands_to: expansion, description: None };
            db.save_alias(&alias)?;
            println!("✓ Alias '{name}' saved.");
        }
        AliasCommand::List => {
            let reg = db.load_registry()?;
            if reg.aliases.is_empty() {
                println!("No aliases defined.");
            } else {
                for (name, alias) in &reg.aliases {
                    println!("  {name} → datum {}", alias.expands_to);
                }
            }
        }
        AliasCommand::Remove { name } => {
            // TODO: implement delete in db
            eprintln!("alias remove not yet implemented for '{name}'");
        }
    }
    Ok(())
}

// ─── Migration ───────────────────────────────────────────────────────────────

fn cmd_migrate(args: MigrateArgs, db: &Db) -> Result<()> {
    let content = std::fs::read_to_string(&args.backup_file)
        .with_context(|| format!("Reading backup file: {}", args.backup_file))?;
    let backup: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| "Parsing backup JSON")?;

    let docs = backup["docs"].as_array()
        .context("Backup JSON missing 'docs' array")?;

    let device = device_id()?;
    let mut imported = 0usize;
    let mut skipped = 0usize;

    for doc in docs {
        let data = &doc["data"];
        let meta = &doc["meta"];

        let field = match data["field"].as_str() {
            Some(f) => f.to_string(),
            None => { skipped += 1; continue; }
        };

        // Map datum verbs from state type
        let verb = if data["state"].is_boolean() { Verb::Switch }
            else if data["occurTime"].is_object() { Verb::Occur }
            else { Verb::Note };

        let value = if data["state"].is_null() {
            None
        } else {
            Some(data["state"].clone())
        };

        let time_start = parse_datum_time(&data["occurTime"]);

        // extra_data: everything in data except known fields
        let mut extra = serde_json::Map::new();
        if let Some(obj) = data.as_object() {
            for (k, v) in obj {
                if !matches!(k.as_str(), "field" | "state" | "occurTime" | "dur" | "lastState") {
                    extra.insert(k.clone(), v.clone());
                }
            }
        }
        let extra_data = if extra.is_empty() { None } else { Some(JsonValue::Object(extra)) };

        let created_at = parse_datum_time(&meta["createTime"])
            .map(|t| t.utc)
            .unwrap_or_else(chrono::Utc::now);

        let mut entry = Entry::new(
            field, verb, value, extra_data, None, time_start, None, device.clone(),
        );
        entry.created_at = created_at;

        if args.dry_run {
            println!("[dry-run] would import: {}", entry.slug);
            imported += 1;
            continue;
        }

        match db.insert_entry(&entry) {
            Ok(()) => imported += 1,
            Err(datum_core::error::DatumError::Duplicate(_)) => skipped += 1,
            Err(e) => return Err(e.into()),
        }
    }

    println!("Migration complete: {imported} imported, {skipped} skipped.");
    Ok(())
}

fn parse_datum_time(v: &JsonValue) -> Option<DatumTime> {
    let utc_str = v["utc"].as_str()?;
    let utc: chrono::DateTime<chrono::Utc> = utc_str.parse().ok()?;
    let tz = v["tz"].as_str().unwrap_or("UTC").to_string();
    let offset_minutes = v["o"].as_f64().map(|o| (o * 60.0) as i32).unwrap_or(0);
    Some(DatumTime { utc, offset_minutes, timezone: tz })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_value_token_number() {
        assert_eq!(parse_value_token("42"), JsonValue::Number(42.into()));
        assert_eq!(parse_value_token("true"), JsonValue::Bool(true));
    }

    #[test]
    fn test_parse_value_token_unit_annotated() {
        let v = parse_value_token("75.2kg");
        assert_eq!(v["v"], serde_json::json!(75.2));
        assert_eq!(v["u"], serde_json::json!("kg"));
    }

    #[test]
    fn test_parse_value_token_string() {
        assert_eq!(parse_value_token("happy"), JsonValue::String("happy".to_string()));
    }

    #[test]
    fn test_split_numeric_unit() {
        assert_eq!(split_numeric_unit("75.2kg"), Some(("75.2", "kg")));
        assert_eq!(split_numeric_unit("500ml"), Some(("500", "ml")));
        assert_eq!(split_numeric_unit("hello"), None);
        assert_eq!(split_numeric_unit("42"), None);
    }
}
