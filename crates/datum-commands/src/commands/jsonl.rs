//! `datum import` and `datum export --jsonl` (da-jouhm): an entry as one JSON
//! row, the stored row an op's `effect.create` holds less what the importer
//! derives. `docs/import.md` is the spec; [`SCHEMA`] its JSON Schema.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use datum_core::{
    action::Action,
    db::{is_meta_field, ConflictStrategy, Db, InsertOutcome, ListParams},
    derive_verb,
    dur::Dur,
    entry::{build_slug, off_the_earth, Entry, Verb, AT_RESERVED},
    ids::Hid,
    slug::validate_new_field_name,
    time::DatumTime,
    timezone_field::{self, TIMEZONE_FIELD},
};
use serde_json::{Map, Value};

use crate::args::{ExportArgs, ImportArgs};

/// What the importer derives, so `export` leaves it out and `import` ignores it.
const DERIVED: &[&str] = &[
    "hid",
    "slug",
    "id_structure",
    "content_hash",
    "client_id",
    "parser_version",
    "modify_time",
    "alias_chain",
];

/// The keys a row may give, each read below.
const KEYS: &[&str] = &[
    "field",
    "at",
    "state",
    "dur",
    "data",
    "following_state",
    "verb",
    "create_time",
    "lat",
    "lon",
    "location",
    "raw_input",
    "extra_metadata",
    "user_asserted_last_state",
    "field_structure",
    "deleted_at",
];

/// The rows' JSON Schema, which `datum import --schema` prints.
pub const SCHEMA: &str = include_str!("jsonl_schema.json");

/// An entry as `export` prints it: its stored row, less what `import`
/// derives, a tombstone's `deleted_at` and every key with no value.
pub fn row(entry: &Entry) -> Map<String, Value> {
    let Ok(Value::Object(mut row)) = serde_json::to_value(entry) else {
        return Map::new();
    };
    row.retain(|k, v| !v.is_null() && !DERIVED.contains(&k.as_str()) && k != "deleted_at");
    row
}

pub fn export(args: ExportArgs, db: &Db) -> Result<()> {
    for field in &args.fields {
        if is_meta_field(field) {
            bail!("{field} is datum's own field, and not exported");
        }
        if !db.field_exists(field)? {
            bail!("no field named {field}");
        }
    }
    let params = ListParams {
        fields: args.fields,
        include_notes: true,
        ..Default::default()
    };
    let mut out = std::io::stdout().lock();
    for entry in db.list_entries(&params)? {
        if !is_meta_field(&entry.field) {
            writeln!(out, "{}", Value::Object(row(&entry)))?;
        }
    }
    Ok(())
}

/// What the import knows of a field before it writes.
struct Field {
    new: bool,
    timed: bool,
    closed: bool,
}

pub fn import(args: ImportArgs, db: &Db) -> Result<()> {
    if args.schema {
        print!("{SCHEMA}");
        return Ok(());
    }
    let _tz = args.tz.as_deref().map(Tz::set).transpose()?;
    let text = match args.file.as_deref() {
        None if std::io::IsTerminal::is_terminal(&std::io::stdin()) => {
            bail!("import takes a file: datum import <FILE>, or `-` to type the rows")
        }
        None | Some("-") => std::io::read_to_string(std::io::stdin())?,
        Some(f) => std::fs::read_to_string(f).with_context(|| format!("cannot read {f}"))?,
    };
    let client = crate::config::client_id()?;
    let now = Utc::now();
    let mut fields: HashMap<String, Field> = HashMap::new();
    let (mut rows, mut lines, mut refused) = (Vec::new(), Vec::new(), Vec::new());
    let mut zoneless = false;
    // A writer's copy judges each row as a log would be: a row its chain
    // refuses is refused by its line, not kept here alone (da-vspq5).
    let writer = db.sync_writer_bounds()?;
    let refuses = |entry: &Entry| -> Result<Option<String>> {
        Ok(writer.as_ref().map(|w| w(entry)).transpose()?.flatten())
    };
    let (read, untyped): (Rows, Vec<String>) = if args.csv {
        csv_rows(&text, args.field.as_deref(), db)?
    } else {
        let rows = text
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(i, line)| (i + 1, serde_json::from_str(line).context("not JSON")))
            .collect();
        (rows, Vec::new())
    };
    for (n, row) in read {
        // A row with no create_time is the import's, a millisecond apart, so
        // two untimed rows of one field keep two slugs.
        let created = now + Duration::milliseconds(n as i64);
        let read =
            row.and_then(|row| read_row(row, created, &client, db, &mut fields, &mut zoneless));
        match read.and_then(|e| match refuses(&e)? {
            Some(why) => bail!("{why}"),
            None => Ok(e),
        }) {
            Ok(entry) => {
                rows.push(entry);
                lines.push(n);
            }
            Err(e) => refused.push((n, format!("{e:#}"))),
        }
    }
    let total = rows.len() + refused.len();
    for (n, e) in &refused {
        eprintln!("error: line {n}: {e}");
    }
    if !refused.is_empty() && !args.allow_partial {
        bail!(
            "{} of {total} rows refused: nothing imported (--allow-partial imports the rest)",
            refused.len()
        );
    }

    // Per field: imported, already there; and the rows there with other content.
    let mut tally: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let mut differ = Vec::new();
    let mut count = |outcomes: &[InsertOutcome]| -> Result<Vec<Hid>> {
        let mut hids = Vec::new();
        for ((entry, outcome), n) in rows.iter().zip(outcomes).zip(&lines) {
            let t = tally.entry(entry.field.as_str()).or_default();
            match outcome {
                InsertOutcome::Inserted { hid, .. } => {
                    t.0 += 1;
                    hids.push(hid.clone());
                }
                InsertOutcome::Ignored { slug } => {
                    let held = db.get_entry(slug.as_str()).ok();
                    if held.is_some_and(|h| h.content_hash == entry.content_hash) {
                        t.1 += 1;
                    } else {
                        differ.push(format!("line {n}: {slug}"));
                    }
                }
                _ => {}
            }
        }
        Ok(hids)
    };
    let imported = if args.dry_run {
        db.rehearse_entries_bulk(&rows, &mut |_| {}, &mut count)?
            .len()
    } else {
        let mark = datum_core::last_docs::history_mark(db)?;
        let outcomes = db.insert_entries_bulk(&rows, ConflictStrategy::Ignore, &mut |_| {})?;
        let hids = count(&outcomes)?;
        write_specs(db, &rows)?;
        // One action, so one `undo` takes the import back, as v2import's.
        datum_core::last_docs::set_last_to_batch(db, mark, &hids)?;
        // A bulk row leaves no history, so `undo` is told its hid.
        datum_core::undo_log::add_untracked(db.conn(), &hids)?;
        hids.len()
    };

    let would = if args.dry_run {
        "would import"
    } else {
        "imported"
    };
    let w = tally.keys().map(|f| f.len()).max().unwrap_or(0);
    for (field, (new, there)) in &tally {
        let mut said = format!("{field:<w$}  {new} {would}");
        if *there > 0 {
            said += &format!(", {there} already there");
        }
        if fields.get(*field).is_some_and(|f| f.new) {
            said += " (new field)";
        }
        println!("{said}");
    }
    if !differ.is_empty() {
        println!(
            "{} already there with other content, kept as it is:",
            differ.len()
        );
        for d in &differ {
            println!("  {d}");
        }
    }
    if zoneless {
        let zone = datum_parse::time::clock::host_iana_tz_name();
        let zone = zone.as_deref().unwrap_or("UTC");
        println!("A time naming no zone was read in {zone}.");
    }
    for line in &untyped {
        println!("{line}");
    }
    let s = if imported == 1 { "y" } else { "ies" };
    let dry = if args.dry_run {
        " (dry run: nothing written)"
    } else {
        ""
    };
    println!(
        "{} {imported} entr{s} of {total} rows{dry}.",
        if args.dry_run {
            "Would import"
        } else {
            "Imported"
        }
    );
    if !refused.is_empty() {
        bail!("{} of {total} rows refused", refused.len());
    }
    Ok(())
}

/// A spec for each field the rows made: a record where none of its rows has
/// a time, as `note` makes one, else the kind its states say.
fn write_specs(db: &Db, rows: &[Entry]) -> Result<()> {
    let mut untimed: BTreeMap<&str, bool> = BTreeMap::new();
    for e in rows {
        *untimed.entry(&e.field).or_insert(true) &= e.at.is_none();
    }
    for (field, untimed) in untimed {
        if db.load_spec(field)?.is_some() {
            continue;
        }
        let kind = match untimed {
            true => Some(Action::Add),
            false => db.verb_from_field_log(field)?,
        };
        if let Some(kind) = kind {
            super::helpers::save_track_spec(db, field, kind)?;
        }
    }
    Ok(())
}

/// One row's entry, or why it is refused.
fn read_row(
    row: Value,
    created: DateTime<Utc>,
    client: &str,
    db: &Db,
    fields: &mut HashMap<String, Field>,
    zoneless: &mut bool,
) -> Result<Entry> {
    let Value::Object(row) = row else {
        bail!("not a JSON object");
    };
    if let Some(k) = row
        .keys()
        .find(|k| !KEYS.contains(&k.as_str()) && !DERIVED.contains(&k.as_str()))
    {
        bail!("unknown key `{k}`: `datum import --schema` lists the keys");
    }
    let get = |k: &str| row.get(k).filter(|v| !v.is_null());
    let text = |k: &str| -> Result<Option<String>> {
        match get(k) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(v) => bail!("`{k}` is a string, not {v}"),
        }
    };
    let number = |k: &str| -> Result<Option<f64>> {
        get(k)
            .map(|v| {
                v.as_f64()
                    .with_context(|| format!("`{k}` is a number, not {v}"))
            })
            .transpose()
    };
    let Some(field) = text("field")?.filter(|f| !f.is_empty()) else {
        bail!("`field` is required");
    };
    if is_meta_field(&field) {
        bail!("{field} is datum's own field: `setup`, `alias` and `kit` write it");
    }
    validate_new_field_name(&field)?;
    if get("deleted_at").is_some() {
        bail!("a deleted entry is not imported");
    }
    if !fields.contains_key(&field) {
        let new = !db.field_exists(&field)?;
        let spec = db.load_spec(&field)?;
        fields.insert(
            field.clone(),
            Field {
                new,
                timed: spec.is_some_and(|s| !matches!(s.verb, Action::Add | Action::Note)),
                closed: new && db.creation_closer(&field)?.is_some(),
            },
        );
    }
    let known = &fields[&field];
    if known.closed {
        bail!("{field} is a new field, and this database takes none unless set up: `datum setup {field}` first");
    }

    let state = get("state").cloned();
    let following_state = get("following_state").cloned();
    let dur = match get("dur") {
        None => None,
        Some(Value::String(s)) => {
            Some(Dur::parse_input(s).with_context(|| format!("dur {s:?} is not a duration"))?)
        }
        Some(v) => match v.as_i64() {
            Some(secs) => Some(Dur::of(Duration::seconds(secs))),
            None => bail!("dur {v} is not a duration"),
        },
    }
    .map(|mut d| {
        d.span = d.span.clone().zero_as_moment();
        d
    });
    let at = get("at").map(|v| read_time(v, zoneless)).transpose()?;
    if at.is_none()
        && (state.is_some() || dur.is_some() || following_state.is_some() || known.timed)
    {
        bail!("`at` is required: {field}'s entry is timed, and an import never puts one at now");
    }
    let (verb, _) = derive_verb(state.clone(), dur.as_ref(), following_state.clone())?;
    if let Some(given) = get("verb") {
        let given: Verb = serde_json::from_value(given.clone())
            .with_context(|| format!("verb {given} is none of datum's"))?;
        if given != verb {
            bail!("verb {given} is not what its state and dur make, {verb}");
        }
    }
    let data = match get("data") {
        None => None,
        Some(Value::Object(d)) if d.contains_key("at") => bail!("{AT_RESERVED}"),
        Some(d @ Value::Object(_)) => Some(d.clone()),
        Some(v) => bail!("`data` is an object, not {v}"),
    };
    if field == TIMEZONE_FIELD
        && [&state, &following_state]
            .into_iter()
            .flatten()
            .any(|s| !timezone_field::holds(s))
    {
        bail!("a timezone state is an IANA zone or IGNORED");
    }

    let mut entry = Entry::new(field, verb, state, data, at, dur, client);
    entry.following_state = following_state;
    entry.create_time = match get("create_time") {
        Some(v) => read_time(v, &mut false)?.utc,
        None => created,
    };
    entry.modify_time = entry.create_time;
    entry.slug = build_slug(&entry.field, entry.at.as_ref(), &entry.create_time);
    // A place on Earth (da-qpuh8). A number a float cannot hold exactly is
    // past 2^53, far outside both, so none is stored rounded either.
    let degrees = |k: &str| -> Result<Option<f64>> {
        let n = number(k)?;
        if let Some(why) = n.zip(get(k)).and_then(|(n, v)| off_the_earth(k, n, v)) {
            bail!("{why}");
        }
        Ok(n)
    };
    entry.lat = degrees("lat")?;
    entry.lon = degrees("lon")?;
    entry.location = text("location")?;
    entry.raw_input = text("raw_input")?;
    entry.field_structure = text("field_structure")?;
    entry.user_asserted_last_state = get("user_asserted_last_state").cloned();
    entry.extra_metadata = match get("extra_metadata") {
        None => None,
        Some(m @ Value::Object(_)) => Some(m.clone()),
        Some(v) => bail!("`extra_metadata` is an object, not {v}"),
    };
    entry.alias_chain = None;
    // Not produced by a parse, as v2import's rows are not.
    entry.parser_version = 0;
    timezone_field::keep_in_named_zone(&mut entry)?;
    entry.content_hash = entry.expected_content_hash();
    datum_core::db::check_bulk_entry(&entry)?;
    Ok(entry)
}

/// The columns a CSV row gives the row's own keys, as text; `lat` and `lon`
/// are numbers, `state` and `following_state` as [`StateKind`] reads them,
/// and any other column is a `data` key.
const CSV_KEYS: &[&str] = &["field", "at", "dur", "create_time", "location"];

/// Each row read, by the line it starts on.
type Rows = Vec<(usize, Result<Value>)>;

/// What a field's spec says of its CSV cells: the types its keys declare, and
/// whether it is on/off, so a `true` or `false` state is one.
struct CellTypes {
    keys: BTreeMap<String, datum_parse::input::KeyType>,
    on_off: bool,
}

/// `--csv` (da-tkppz): each row after the header as the JSON row it maps to,
/// with the line it starts on, and the summary's lines on the columns kept as
/// text that read as numbers or booleans. A cell is the text it is, unless
/// the field's spec types its key or the key's name does (`…_time`, `…_date`),
/// as on a command line, and a state is `true` or `false` only on an on/off
/// field (vividn, 2026-11-15: an import does not change what the file says).
/// An empty cell gives no key.
fn csv_rows(text: &str, field: Option<&str>, db: &Db) -> Result<(Rows, Vec<String>)> {
    let mut records = csv_records(text.strip_prefix('\u{feff}').unwrap_or(text))?.into_iter();
    let Some((_, header)) = records.next() else {
        bail!("a CSV import takes a header row naming its columns");
    };
    let header: Vec<&str> = header.iter().map(|h| h.trim()).collect();
    if let Some(i) = header.iter().position(|h| h.is_empty()) {
        bail!("header column {} has no name", i + 1);
    }
    if let Some(h) = header
        .iter()
        .enumerate()
        .find_map(|(i, h)| header[..i].contains(h).then_some(h))
    {
        bail!("the header names `{h}` twice");
    }
    match (field, header.contains(&"field")) {
        (Some(_), true) => {
            bail!("--field names every row's field, and the header has a `field` column too")
        }
        (None, false) => bail!(
            "the header has no `field` column: name one, or give --field <F> for a file of one field"
        ),
        _ => {}
    }
    let mut specs: HashMap<String, CellTypes> = HashMap::new();
    // Per field and column kept as text: whether every cell read as a number,
    // and whether every one as a boolean.
    let mut untyped: BTreeMap<(String, String), (bool, bool)> = BTreeMap::new();
    let mut rows = Vec::new();
    for (n, cells) in records {
        if cells.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        if cells.len() != header.len() {
            let why = format!(
                "{} cells, where the header names {} columns",
                cells.len(),
                header.len()
            );
            rows.push((n, Err(anyhow::anyhow!(why))));
            continue;
        }
        let cell = |key: &str| {
            header
                .iter()
                .position(|h| *h == key)
                .map(|i| cells[i].trim())
                .filter(|c| !c.is_empty())
        };
        let name = field
            .or_else(|| cell("field"))
            .unwrap_or_default()
            .to_string();
        if !specs.contains_key(&name) {
            let spec = db.load_spec(&name).ok().flatten();
            specs.insert(
                name.clone(),
                CellTypes {
                    keys: spec.as_ref().map(|s| s.key_types()).unwrap_or_default(),
                    on_off: spec.is_some_and(|s| matches!(s.verb, Action::Start | Action::Stop)),
                },
            );
        }
        let types = &specs[&name];
        let mut kept = |key: &str, cell: &str| {
            let read = datum_parse::input::infer_type(cell);
            let t = untyped
                .entry((name.clone(), key.to_string()))
                .or_insert((true, true));
            t.0 &= read.as_ref().is_some_and(Value::is_number);
            t.1 &= read.as_ref().is_some_and(Value::is_boolean);
            Value::String(cell.into())
        };
        let read = (|| -> Result<Value> {
            let (mut row, mut data) = (Map::new(), Map::new());
            if field.is_some() {
                row.insert("field".into(), name.clone().into());
            }
            for (key, cell) in header.iter().zip(&cells) {
                let cell = cell.trim();
                if cell.is_empty() {
                    continue;
                }
                let value = match *key {
                    k if CSV_KEYS.contains(&k) => Some(cell.into()),
                    "lat" | "lon" => Some(
                        datum_parse::input::infer_type(cell)
                            .filter(Value::is_number)
                            .unwrap_or(cell.into()),
                    ),
                    "state" | "following_state" => {
                        let flag = ["true", "false"]
                            .iter()
                            .find(|b| types.on_off && cell.eq_ignore_ascii_case(b));
                        Some(match flag {
                            Some(b) => Value::Bool(*b == "true"),
                            None => kept(key, cell),
                        })
                    }
                    k => {
                        let value = match datum_parse::input::typed_value(k, cell, &types.keys) {
                            Some(typed) => typed?,
                            None => Some(kept(k, cell)),
                        };
                        data.extend(value.map(|v| (k.to_string(), v)));
                        None
                    }
                };
                if let Some(v) = value {
                    row.insert((*key).into(), v);
                }
            }
            if !data.is_empty() {
                row.insert("data".into(), Value::Object(data));
            }
            Ok(Value::Object(row))
        })();
        rows.push((n, read));
    }
    let said = untyped
        .into_iter()
        .filter_map(|((field, key), (numbers, bools))| {
            let reads = match (numbers, bools) {
                (true, _) => "a number",
                (_, true) => "true or false",
                _ => return None,
            };
            // No flag types a key: a spec's subfield does, which `setup
            // --from-json` writes whole and a kit's `[[field]]` installs.
            let how = match (key.as_str(), numbers) {
                ("state" | "following_state", false) => format!("{field} is not on/off: `datum setup {field} --kind on` first"),
                ("state" | "following_state", true) => "a CSV state has no type; a JSONL row's `\"state\": 2` is a number".into(),
                _ => {
                    let ty = if numbers { "number" } else { "bool" };
                    format!(
                        "to type it, give {field}'s spec `subfields.{key} = {{ value_type = \"{ty}\" }}` first \
                         (in a kit's [[field]]; `setup --from-json` replaces the whole spec)"
                    )
                }
            };
            Some(format!(
                "{field}: every `{key}` cell reads as {reads}, and is kept as text: {how}"
            ))
        })
        .collect();
    Ok((rows, said))
}

/// RFC 4180 records, each with the line it starts on: a quoted cell may hold
/// commas, line breaks and `""` for a quote.
fn csv_records(text: &str) -> Result<Vec<(usize, Vec<String>)>> {
    let (mut records, mut record, mut cell) = (Vec::new(), Vec::new(), String::new());
    // `start` is the line the record being read began on, set again after
    // each record from `line`, which counts every line break, quoted or not.
    let (mut line, mut start, mut quoted) = (1, 1, false);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                cell.push('"');
            }
            '"' if quoted => quoted = false,
            '"' if cell.is_empty() => quoted = true,
            ',' if !quoted => record.push(std::mem::take(&mut cell)),
            '\r' if !quoted && chars.peek() == Some(&'\n') => {}
            '\n' if !quoted => {
                record.push(std::mem::take(&mut cell));
                records.push((start, std::mem::take(&mut record)));
                line += 1;
                start = line;
            }
            c => {
                line += usize::from(c == '\n');
                cell.push(c);
            }
        }
    }
    if quoted {
        bail!("line {start}: a quoted cell is never closed");
    }
    if !cell.is_empty() || !record.is_empty() {
        record.push(cell);
        records.push((start, record));
    }
    Ok(records)
}

/// A time as `-t` reads it, or the object `export` prints. Sets `zoneless`
/// where it names no zone and takes this machine's (or `--tz`'s).
fn read_time(v: &Value, zoneless: &mut bool) -> Result<DatumTime> {
    match v {
        Value::String(s) => {
            *zoneless |= !names_offset(s);
            let time = datum_parse::input::TimeArgs {
                time: Some(s.clone()),
                ..Default::default()
            };
            time.build(&DatumTime::now())?
                .time
                .with_context(|| format!("{s:?} names no time"))
        }
        Value::Object(o) if o.contains_key("offset_minutes") => {
            serde_json::from_value(v.clone()).with_context(|| format!("{v} is not a time"))
        }
        Value::Object(o) => {
            let utc = o
                .get("utc")
                .and_then(Value::as_str)
                .context("a time object has `utc`")?;
            let utc: DateTime<Utc> = utc
                .parse()
                .with_context(|| format!("{utc:?} is not a time"))?;
            let zone = match o.get("timezone").and_then(Value::as_str) {
                Some(z) => Some(z.to_string()),
                None => {
                    *zoneless = true;
                    datum_parse::time::clock::host_iana_tz_name()
                }
            };
            let mut t = match zone {
                Some(z) => DatumTime::from_utc_and_tz(utc, &z)?,
                None => DatumTime {
                    utc,
                    offset_minutes: 0,
                    timezone: None,
                    uncertainty: None,
                },
            };
            t.uncertainty = o
                .get("uncertainty")
                .and_then(Value::as_str)
                .map(str::to_string);
            Ok(t)
        }
        v => bail!("{v} is not a time"),
    }
}

/// Whether a time as typed names its own offset: a trailing `Z`, or a `+` or
/// `-` and digits after its clock (`…T08:30:00+01:00`), not a date's `-`.
fn names_offset(s: &str) -> bool {
    if s.ends_with(['Z', 'z']) {
        return true;
    }
    s.rfind(['+', '-']).is_some_and(|i| {
        let (clock, offset) = (&s[..i], &s[i + 1..]);
        clock.contains(':')
            && !offset.is_empty()
            && offset.chars().all(|c| c.is_ascii_digit() || c == ':')
    })
}

/// `--tz`: `TZ` set for the import, and put back after.
struct Tz(Option<std::ffi::OsString>);

impl Tz {
    fn set(zone: &str) -> Result<Self> {
        let held = Tz(std::env::var_os("TZ"));
        std::env::set_var("TZ", datum_parse::time::iana_zone(zone)?);
        Ok(held)
    }
}

impl Drop for Tz {
    fn drop(&mut self) {
        match self.0.take() {
            Some(tz) => std::env::set_var("TZ", tz),
            None => std::env::remove_var("TZ"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_names_its_offset_only_after_its_clock() {
        for zoned in [
            "2026-11-10T08:30:00+01:00",
            "2026-11-10T08:30:00Z",
            "2026-11-10 08:30-0500",
        ] {
            assert!(names_offset(zoned), "{zoned}");
        }
        for bare in [
            "2026-11-10 08:30",
            "2026-11-10T08:30",
            "2026-11-10",
            "yesterday 10am",
            "-20",
        ] {
            assert!(!names_offset(bare), "{bare}");
        }
    }

    #[test]
    fn a_quoted_cell_holds_commas_quotes_and_line_breaks() {
        let records = csv_records("a,b\r\n\"x, \"\"y\"\"\",\"two\nlines\"\n\nlast,\n").unwrap();
        let want = |n: usize, cells: &[&str]| (n, cells.iter().map(|c| c.to_string()).collect());
        assert_eq!(
            records,
            vec![
                want(1, &["a", "b"]),
                want(2, &["x, \"y\"", "two\nlines"]),
                want(4, &[""]),
                want(5, &["last", ""]),
            ]
        );
        let first = csv_records("\"a\nb\",c\nd,e").unwrap();
        assert_eq!(first, vec![want(1, &["a\nb", "c"]), want(3, &["d", "e"])]);
    }

    /// The schema names exactly the keys a row may give, read or derived.
    #[test]
    fn the_schema_names_every_key_the_import_reads() {
        let schema: Value = serde_json::from_str(SCHEMA).unwrap();
        let mut named: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let mut keys: Vec<&str> = KEYS.iter().chain(DERIVED).copied().collect();
        named.sort_unstable();
        keys.sort_unstable();
        assert_eq!(named, keys);
    }
}
