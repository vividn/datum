//! Output formatting for datum entries.

use datum_core::entry::{Entry, Verb};
use chrono::Local;

/// Print a single entry in a concise one-line format.
pub fn print_entry(entry: &Entry) {
    let time_str = entry
        .time_start
        .as_ref()
        .or(entry.time_end.as_ref())
        .map(|t| {
            t.utc
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "no-time".to_string());

    let value_str = entry
        .value
        .as_ref()
        .map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default();

    let unit_str = entry.unit.as_deref().unwrap_or("");
    let unit_display = if unit_str.is_empty() { String::new() } else { format!(" {unit_str}") };

    let dur_str = entry
        .duration_seconds()
        .map(|s| format!(" ({})", format_duration(s)))
        .unwrap_or_default();

    let extra_str = entry
        .extra_data
        .as_ref()
        .map(|e| format!("  {e}"))
        .unwrap_or_default();

    println!(
        "{time_str}  [{verb}] {field}{value_part}{unit_display}{dur_str}{extra_str}",
        verb = entry.verb,
        field = entry.field,
        value_part = if value_str.is_empty() { String::new() } else { format!(" = {value_str}") },
    );
}

/// Print multiple entries.
pub fn print_entries(entries: &[Entry]) {
    for e in entries {
        print_entry(e);
    }
}

/// Print confirmation after a successful log.
pub fn print_logged(entry: &Entry) {
    let time_str = entry
        .time_start
        .as_ref()
        .map(|t| t.utc.with_timezone(&Local).format("%H:%M").to_string())
        .unwrap_or_else(|| "now".to_string());

    let value_str = entry.value.as_ref()
        .map(|v| match v {
            serde_json::Value::String(s) => format!(" → {s}"),
            other => format!(" → {other}"),
        })
        .unwrap_or_default();

    println!("✓ {verb} {field}{value}{time}",
        verb = entry.verb,
        field = entry.field,
        value = value_str,
        time = format!("  @{time_str}"),
    );
}

fn format_duration(secs: i64) -> String {
    let secs = secs.abs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 { format!("{h}h") } else { format!("{h}h{m}m") }
    }
}
