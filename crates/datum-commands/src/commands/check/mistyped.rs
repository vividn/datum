//! The `mistyped_data_value` check: a value that does not fit the type its
//! key's spec declares (da-b2f8o). Logging refuses one; an entry another
//! device wrote is merged as it is, and lands here instead. So does a
//! `*_time` value logged before it was an instant (da-x15of), typed by its
//! name or a spec: its line still holds its day.

use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::field_spec::SpecRegistry;
use datum_parse::input::KeyType;
use serde_json::Value as JsonValue;

use super::{render_flagged_block, FlaggedEntry, Issue, IssueExample, Severity};
use crate::commands::reparse;

pub(super) fn collect_mistyped(
    db: &Db,
    entries: &[Entry],
    registry: &SpecRegistry,
) -> Option<Issue> {
    let mut mistyped: Vec<(Entry, String)> = Vec::new();
    for e in entries {
        let Some(JsonValue::Object(map)) = e.data.as_ref() else {
            continue;
        };
        let mut types = registry
            .field_spec(&e.field)
            .map(|spec| spec.key_types())
            .unwrap_or_default();
        for key in map.keys() {
            if !types.contains_key(key) && KeyType::of_suffix(key) == Some(KeyType::Time) {
                types.insert(key.clone(), KeyType::Time);
            }
        }
        // Replayed once an entry, and only for one with a misfit time.
        let mut replayed: Option<Option<JsonValue>> = None;
        for (key, ty) in types {
            if let Some(v) = map.get(&key).filter(|v| !ty.fits(v)) {
                // The command is named only where it mends this value: a row
                // with no line to replay (merged, imported) gets none.
                let mends = ty == KeyType::Time
                    && replayed
                        .get_or_insert_with(|| reparse::replayed_data(e))
                        .as_ref()
                        .and_then(|data| data.get(&key))
                        .is_some_and(|new| ty.fits(new));
                let hint = match mends {
                    true => format!(": `datum reparse {} --fix`", e.short_uid()),
                    false => String::new(),
                };
                mistyped.push((
                    e.clone(),
                    format!("{key}: {v} is not a {}{hint}", ty.name()),
                ));
            }
        }
    }
    let (first, detail) = mistyped.first()?;
    let count = mistyped.len();
    // One row per value in `entries`, one line per entry in the block.
    let mut seen = std::collections::HashSet::new();
    let flagged: Vec<Entry> = mistyped
        .iter()
        .filter(|(e, _)| seen.insert(e.hid.clone()))
        .map(|(e, _)| e.clone())
        .collect();
    // Under each row, what is wrong with it and the command that mends
    // it: `--json` was the only place that said (da-q3r7v).
    let rows = render_flagged_block(db, &flagged);
    let mut block = String::new();
    if rows.lines().count() == flagged.len() {
        for (row, e) in rows.lines().zip(&flagged) {
            block += &format!("{row}\n");
            for (_, detail) in mistyped.iter().filter(|(m, _)| m.hid == e.hid) {
                block += &format!("  {detail}\n");
            }
        }
    } else {
        // A row a newline in its state spread over lines cannot be told
        // from its neighbour by position: the details follow the rows,
        // each naming its own.
        block = rows;
        for (e, detail) in &mistyped {
            block += &format!("  {} {detail}\n", e.short_uid());
        }
    }
    Some(Issue {
        severity: Severity::Warning,
        category: "mistyped_data_value",
        message: format!(
            "{count} data value{s} not of the type its key declares",
            s = if count == 1 { "" } else { "s" },
        ),
        count,
        example: Some(IssueExample {
            id: Some(first.slug.to_string()),
            field: Some(first.field.clone()),
            detail: Some(detail.clone()),
        }),
        entries: mistyped
            .iter()
            .map(|(e, detail)| FlaggedEntry {
                hid: e.short_uid(),
                field: e.field.clone(),
                state: detail.clone(),
            })
            .collect(),
        rendered_block: block,
    })
}
