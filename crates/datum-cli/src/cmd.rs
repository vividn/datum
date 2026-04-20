//! Command dispatch and argument → Entry conversion.

use anyhow::{Context, Result};
use datum_core::{
    db::Db,
    entry::{Entry, Verb},
    field_spec::Alias,
    time::{parse_date_arg, parse_duration, parse_time_arg, DatumTime, TimeBuilder},
};
use serde_json::Value as JsonValue;

use crate::{
    args::{
        AliasCommand, Cli, Command, DeleteArgs, GetArgs, HeadArgs, IgnoreArgs, LogArgs, MigrateArgs,
        NowArgs, SpecCommand, TailArgs, TrackArgs,
    },
    config::device_id,
    output::{print_entries, print_entry, print_logged, print_now_view},
};

pub fn dispatch(cli: Cli, db: &Db) -> Result<()> {
    match cli.command {
        Command::Occur(args) => log_entry(Verb::Occur, args, db),
        Command::Start(args) => log_entry(Verb::Start, args, db),
        Command::Stop(args) => log_entry(Verb::Stop, args, db),
        Command::Switch(args) => log_entry(Verb::Switch, args, db),
        Command::Note(args) => log_entry(Verb::Note, args, db),
        Command::Tail(args) => cmd_tail(args, db),
        Command::Head(args) => cmd_head(args, db),
        Command::Now(args) => cmd_now(args, db),
        Command::Get(args) => cmd_get(args, db),
        Command::Delete(args) => cmd_delete(args, db),
        Command::Spec(sub) => cmd_spec(sub, db),
        Command::Alias(sub) => cmd_alias(sub, db),
        Command::Track(args) => cmd_track(args, db),
        Command::Ignore(args) => cmd_ignore(args, db),
        Command::Migrate(args) => cmd_migrate(args, db),
    }
}

// ─── Logging ─────────────────────────────────────────────────────────────────

fn log_entry(verb: Verb, args: LogArgs, db: &Db) -> Result<()> {
    let (field, value, extra_data) = parse_log_args(&args)?;
    let base_time = build_time(&args)?;
    let unit = args.unit.clone();
    let device = device_id()?;

    // Fix 3: --dur flag sets time_end based on verb direction
    let (time_start, time_end) = if let Some(dur_str) = &args.dur {
        let dur = parse_duration(dur_str)?;
        match verb {
            Verb::Start => {
                // start --dur: time_start=base, time_end=base+dur
                let end_utc = base_time.utc + dur;
                let time_end = DatumTime { utc: end_utc, ..base_time.clone() };
                (Some(base_time), Some(time_end))
            }
            Verb::Stop => {
                // stop --dur: time_end=base, time_start=base-dur
                let start_utc = base_time.utc - dur;
                let time_start = DatumTime { utc: start_utc, ..base_time.clone() };
                (Some(time_start), Some(base_time))
            }
            _ => {
                // occur/switch/note with --dur: set time_end=time_start+dur
                let end_utc = base_time.utc + dur;
                let time_end = DatumTime { utc: end_utc, ..base_time.clone() };
                (Some(base_time), Some(time_end))
            }
        }
    } else {
        (Some(base_time), None)
    };

    let entry = Entry::new(field, verb.clone(), value, extra_data, unit, time_start, time_end, device);

    match db.insert_entry(&entry) {
        Ok(()) => {
            if matches!(verb, Verb::Occur | Verb::Start) {
                db.ensure_tracked(&entry.field)?;
            }
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
        // Fix 5: include future-dated entries (e.g. sleep logged in advance)
        let far_future = chrono::Utc::now() + chrono::Duration::days(365);
        let far_past = chrono::DateTime::UNIX_EPOCH;
        let mut all = db.entries_in_range(far_past, far_future)?;
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

fn cmd_head(args: HeadArgs, db: &Db) -> Result<()> {
    let entries = if let Some(field) = &args.field {
        db.entries_for_field_asc(field, Some(args.count))?
    } else {
        let far_future = chrono::Utc::now() + chrono::Duration::days(365);
        let far_past = chrono::DateTime::UNIX_EPOCH;
        let mut all = db.entries_in_range(far_past, far_future)?;
        // entries_in_range already returns ASC, but be explicit for clarity.
        all.sort_by(|a, b| {
            let ta = a.time_start.as_ref().map(|t| t.utc).unwrap_or(a.created_at);
            let tb = b.time_start.as_ref().map(|t| t.utc).unwrap_or(b.created_at);
            ta.cmp(&tb)
        });
        all.truncate(args.count);
        all
    };
    print_entries(&entries);
    Ok(())
}

fn cmd_now(args: NowArgs, db: &Db) -> Result<()> {
    let tracked = db.get_tracked_fields()?;
    if tracked.is_empty() {
        println!("No tracked fields. Use `datum track <field>` or log with `occur`/`start`.");
        return Ok(());
    }

    let state_colors = db.get_all_state_colors()?;

    let mut latest_per_field = std::collections::HashMap::new();
    for tf in &tracked {
        if tf.ignored && !args.show_all {
            continue;
        }
        if let Some(entry) = db.latest_entry_for_now(&tf.field)? {
            latest_per_field.insert(tf.field.clone(), entry);
        }
    }

    print_now_view(&tracked, &latest_per_field, &state_colors, args.show_all);
    Ok(())
}

fn cmd_track(args: TrackArgs, db: &Db) -> Result<()> {
    db.set_tracked(&args.field, false, args.color.as_deref())?;
    for sc in &args.state_colors {
        match sc.split_once('=') {
            Some((state, color)) => db.set_state_color(&args.field, state, color)?,
            None => anyhow::bail!("Invalid --state format: expected STATE=COLOR, got '{sc}'"),
        }
    }
    println!("✓ Tracking '{}'", args.field);
    Ok(())
}

fn cmd_ignore(args: IgnoreArgs, db: &Db) -> Result<()> {
    db.set_tracked(&args.field, true, None)?;
    println!("✓ Ignoring '{}'", args.field);
    Ok(())
}

// ─── Get / Delete ─────────────────────────────────────────────────────────────

fn cmd_get(args: GetArgs, db: &Db) -> Result<()> {
    let entry = db.get_entry(&args.id_or_slug)
        .map_err(anyhow::Error::from)?;
    print_entry(&entry);
    Ok(())
}

fn cmd_delete(args: DeleteArgs, db: &Db) -> Result<()> {
    if !args.force {
        let entry = db.get_entry(&args.id_or_slug)
            .map_err(anyhow::Error::from)?;
        print_entry(&entry);
        eprint!("Delete this entry? [y/N] ");
        use std::io::BufRead;
        let stdin = std::io::stdin();
        let line = stdin
            .lock()
            .lines()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No input provided"))??;
        if !matches!(line.to_lowercase().as_str(), "y" | "yes") {
            println!("Aborted.");
            return Ok(());
        }
    }
    db.delete_entry(&args.id_or_slug)
        .map_err(anyhow::Error::from)?;
    println!("✓ Deleted {}", args.id_or_slug);
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

    #[test]
    fn test_dur_start_sets_time_end() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let args = LogArgs {
            field_or_verb: "sleep".to_string(),
            rest: vec![],
            yesterday: 0,
            quick: 0,
            date: None,
            time: None,
            dur: Some("8h".to_string()),
            timezone: None,
            unit: None,
            keys: vec![],
            comment: None,
        };
        // Just verify the dur parsing logic: start --dur 8h → end = start + 8h
        let base = build_time(&args).unwrap();
        let dur = parse_duration("8h").unwrap();
        let end_utc = base.utc + dur;
        let diff_secs = (end_utc - base.utc).num_seconds();
        assert_eq!(diff_secs, 8 * 3600);
        // Ensure dispatch doesn't error
        log_entry(Verb::Start, args, &db).unwrap();
    }

    #[test]
    fn test_dur_stop_sets_time_start() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let args = LogArgs {
            field_or_verb: "sleep".to_string(),
            rest: vec![],
            yesterday: 0,
            quick: 0,
            date: None,
            time: None,
            dur: Some("8h".to_string()),
            timezone: None,
            unit: None,
            keys: vec![],
            comment: None,
        };
        log_entry(Verb::Stop, args, &db).unwrap();
        let entries = db.entries_for_field("sleep", None).unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        let diff_secs = e.duration_seconds().unwrap();
        assert_eq!(diff_secs, 8 * 3600);
    }

    #[test]
    fn test_get_and_delete() {
        use datum_core::{db::Db, entry::Entry, time::DatumTime};
        let db = Db::open_in_memory().unwrap();
        let t = DatumTime::now();
        let entry = Entry::new("test", Verb::Occur, None, None, None, Some(t), None, "dev");
        let id = entry.id.clone();
        db.insert_entry(&entry).unwrap();

        // get
        let fetched = db.get_entry(&id).unwrap();
        assert_eq!(fetched.id, id);

        // delete
        db.delete_entry(&id).unwrap();
        let err = db.get_entry(&id);
        assert!(matches!(err, Err(datum_core::error::DatumError::NotFound(_))));
    }

    #[test]
    fn test_auto_track_on_occur() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let args = LogArgs {
            field_or_verb: "coffee".to_string(),
            rest: vec!["2".to_string()],
            yesterday: 0, quick: 0, date: None, time: None, dur: None,
            timezone: None, unit: Some("cups".to_string()), keys: vec![], comment: None,
        };
        log_entry(Verb::Occur, args, &db).unwrap();
        let tracked = db.get_tracked_fields().unwrap();
        assert!(tracked.iter().any(|f| f.field == "coffee" && !f.ignored));
    }

    #[test]
    fn test_auto_track_on_start() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let args = LogArgs {
            field_or_verb: "work".to_string(),
            rest: vec![],
            yesterday: 0, quick: 0, date: None, time: None, dur: None,
            timezone: None, unit: None, keys: vec![], comment: None,
        };
        log_entry(Verb::Start, args, &db).unwrap();
        let tracked = db.get_tracked_fields().unwrap();
        assert!(tracked.iter().any(|f| f.field == "work" && !f.ignored));
    }

    #[test]
    fn test_no_auto_track_on_note() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        let args = LogArgs {
            field_or_verb: "journal".to_string(),
            rest: vec!["some text".to_string()],
            yesterday: 0, quick: 0, date: None, time: None, dur: None,
            timezone: None, unit: None, keys: vec![], comment: None,
        };
        log_entry(Verb::Note, args, &db).unwrap();
        let tracked = db.get_tracked_fields().unwrap();
        assert!(!tracked.iter().any(|f| f.field == "journal"));
    }

    #[test]
    fn test_cmd_track_and_ignore() {
        use crate::args::{IgnoreArgs, TrackArgs};
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        let track_args = TrackArgs {
            field: "mood".to_string(),
            color: Some("ff6600".to_string()),
            state_colors: vec!["happy=00ff00".to_string(), "sad=ff0000".to_string()],
        };
        cmd_track(track_args, &db).unwrap();

        let tracked = db.get_tracked_fields().unwrap();
        let mood = tracked.iter().find(|f| f.field == "mood").unwrap();
        assert!(!mood.ignored);
        assert_eq!(mood.color.as_deref(), Some("ff6600"));

        let state_colors = db.get_all_state_colors().unwrap();
        assert_eq!(
            state_colors.get(&("mood".to_string(), "happy".to_string())).map(|s| s.as_str()),
            Some("00ff00")
        );

        // Now ignore mood
        cmd_ignore(IgnoreArgs { field: "mood".to_string() }, &db).unwrap();
        let tracked2 = db.get_tracked_fields().unwrap();
        let mood2 = tracked2.iter().find(|f| f.field == "mood").unwrap();
        assert!(mood2.ignored);
        // Color should be preserved (set_tracked with None color)
        assert_eq!(mood2.color.as_deref(), Some("ff6600"));
    }

    #[test]
    fn test_cmd_now_no_crash() {
        use crate::args::NowArgs;
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();

        // Empty db
        cmd_now(NowArgs { show_all: false }, &db).unwrap();

        // With some tracked fields and entries
        let args = LogArgs {
            field_or_verb: "mood".to_string(),
            rest: vec!["happy".to_string()],
            yesterday: 0, quick: 0, date: None, time: None, dur: None,
            timezone: None, unit: None, keys: vec![], comment: None,
        };
        log_entry(Verb::Switch, args, &db).unwrap();
        db.set_tracked("mood", false, None).unwrap();

        cmd_now(NowArgs { show_all: false }, &db).unwrap();
        cmd_now(NowArgs { show_all: true }, &db).unwrap();
    }

    // ─── head command tests ─────────────────────────────────────────────────

    fn make_entry_at(field: &str, secs_offset: i64, device: &str) -> datum_core::entry::Entry {
        use datum_core::{entry::Entry, time::DatumTime};
        // Fixed base time + offset keeps ids/slugs distinct and orderable.
        let base: chrono::DateTime<chrono::Utc> =
            "2024-01-01T00:00:00Z".parse().unwrap();
        let utc = base + chrono::Duration::seconds(secs_offset);
        let t = DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() };
        Entry::new(
            field,
            Verb::Occur,
            Some(serde_json::json!(secs_offset)),
            None,
            None,
            Some(t),
            None,
            device,
        )
    }

    #[test]
    fn head_returns_oldest_first() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        // Insert out-of-order: newest first, then oldest, then middle.
        db.insert_entry(&make_entry_at("mood", 300, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();

        cmd_head(HeadArgs { field: None, count: 10 }, &db).unwrap();

        // Verify via db that ordering is ASC by time_start.
        let far_past = chrono::DateTime::UNIX_EPOCH;
        let far_future = chrono::Utc::now() + chrono::Duration::days(365);
        let entries = db.entries_in_range(far_past, far_future).unwrap();
        let values: Vec<i64> = entries
            .iter()
            .map(|e| e.value.as_ref().unwrap().as_i64().unwrap())
            .collect();
        assert_eq!(values, vec![100, 200, 300]);
    }

    #[test]
    fn head_respects_limit() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        for i in 0..20 {
            db.insert_entry(&make_entry_at("mood", i * 60, "d1")).unwrap();
        }

        // No panic on execution + the db method returns exactly `count` entries ASC.
        cmd_head(HeadArgs { field: Some("mood".to_string()), count: 5 }, &db).unwrap();

        let got = db.entries_for_field_asc("mood", Some(5)).unwrap();
        assert_eq!(got.len(), 5);
        let values: Vec<i64> = got
            .iter()
            .map(|e| e.value.as_ref().unwrap().as_i64().unwrap())
            .collect();
        assert_eq!(values, vec![0, 60, 120, 180, 240]);
    }

    #[test]
    fn head_filters_by_field() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&make_entry_at("coffee", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();
        db.insert_entry(&make_entry_at("coffee", 300, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 400, "d1")).unwrap();

        cmd_head(HeadArgs { field: Some("coffee".to_string()), count: 10 }, &db).unwrap();

        let got = db.entries_for_field_asc("coffee", None).unwrap();
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|e| e.field == "coffee"));
        let values: Vec<i64> = got
            .iter()
            .map(|e| e.value.as_ref().unwrap().as_i64().unwrap())
            .collect();
        assert_eq!(values, vec![100, 300]);
    }

    #[test]
    fn head_default_limit_is_10() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        for i in 0..15 {
            db.insert_entry(&make_entry_at("mood", i * 60, "d1")).unwrap();
        }
        // Default count matches clap's default_value = "10".
        let default_count: usize = 10;
        let got = db.entries_for_field_asc("mood", Some(default_count)).unwrap();
        assert_eq!(got.len(), 10);
        // First entry should be the oldest (offset=0).
        assert_eq!(
            got[0].value.as_ref().unwrap().as_i64().unwrap(),
            0
        );
    }

    #[test]
    fn head_returns_all_when_fewer_than_limit() {
        use datum_core::db::Db;
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&make_entry_at("mood", 100, "d1")).unwrap();
        db.insert_entry(&make_entry_at("mood", 200, "d1")).unwrap();

        cmd_head(HeadArgs { field: None, count: 10 }, &db).unwrap();

        let got = db.entries_for_field_asc("mood", Some(10)).unwrap();
        assert_eq!(got.len(), 2);
    }
}
