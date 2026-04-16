//! Output formatting for datum entries.

use chrono::Local;
use datum_core::{
    db::TrackedField,
    entry::{Entry, Verb},
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";

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

// ─── Color utilities ──────────────────────────────────────────────────────────

/// Map a name to an ANSI-256 color index using its SHA-256 hash.
/// Always returns a visible color on dark terminals (at least one channel ≥ 3).
pub fn hash_to_ansi256(name: &str) -> u8 {
    let hash = Sha256::digest(name.as_bytes());
    let r = hash[0] % 6;
    let g = hash[1] % 6;
    let b = hash[2] % 6;
    let (r, g, b) = if r.max(g).max(b) < 3 {
        match hash[3] % 3 {
            0 => (5, g, b),
            1 => (r, 5, b),
            _ => (r, g, 5),
        }
    } else {
        (r, g, b)
    };
    16 + 36 * r + 6 * g + b
}

/// Convert a stored color string to an ANSI escape sequence.
/// Accepts: ANSI-256 number ("196"), hex ("ff6600" or "#ff6600").
/// Falls back to hash-derived ANSI-256 for `fallback_name` if parsing fails.
fn ansi_from_stored(stored: &str, fallback_name: &str) -> String {
    let s = stored.trim_start_matches('#');
    if let Ok(n) = s.parse::<u8>() {
        return format!("\x1b[38;5;{n}m");
    }
    if s.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&s[0..2], 16),
            u8::from_str_radix(&s[2..4], 16),
            u8::from_str_radix(&s[4..6], 16),
        ) {
            return format!("\x1b[38;2;{r};{g};{b}m");
        }
    }
    // Unrecognised format: hash the fallback name
    let n = hash_to_ansi256(fallback_name);
    format!("\x1b[38;5;{n}m")
}

/// Compute the ANSI escape for a field's color (stored override or hash-derived).
fn field_ansi(tf: &TrackedField) -> String {
    match &tf.color {
        Some(c) => ansi_from_stored(c, &tf.field),
        None => {
            let n = hash_to_ansi256(&tf.field);
            format!("\x1b[38;5;{n}m")
        }
    }
}

/// Compute the ANSI escape for a state value's color.
/// Priority: state override → stored field color → hash(state_name).
fn state_ansi(
    field: &str,
    state: &str,
    field_stored_color: &Option<String>,
    state_colors: &HashMap<(String, String), String>,
) -> String {
    if let Some(c) = state_colors.get(&(field.to_string(), state.to_string())) {
        return ansi_from_stored(c, state);
    }
    if let Some(c) = field_stored_color {
        return ansi_from_stored(c, field);
    }
    let n = hash_to_ansi256(state);
    format!("\x1b[38;5;{n}m")
}

// ─── Now view ─────────────────────────────────────────────────────────────────

struct NowRow {
    bullet: &'static str,
    field: String,
    state_str: String,
    time_str: String,
    field_ansi: String,
    state_ansi: String,
    is_dim: bool,
}

/// Print the redesigned `datum now` view.
pub fn print_now_view(
    tracked: &[TrackedField],
    latest_per_field: &HashMap<String, Entry>,
    state_colors: &HashMap<(String, String), String>,
    show_ignored: bool,
) {
    let now = chrono::Utc::now();

    let mut active: Vec<NowRow> = Vec::new();
    let mut stopped: Vec<NowRow> = Vec::new();
    let mut ignored_rows: Vec<NowRow> = Vec::new();

    for tf in tracked {
        let fansi = field_ansi(tf);

        if tf.ignored {
            if show_ignored {
                ignored_rows.push(NowRow {
                    bullet: " ",
                    field: tf.field.clone(),
                    state_str: String::new(),
                    time_str: String::new(),
                    field_ansi: fansi,
                    state_ansi: String::new(),
                    is_dim: true,
                });
            }
            continue;
        }

        match latest_per_field.get(&tf.field) {
            None => {
                stopped.push(NowRow {
                    bullet: " ",
                    field: tf.field.clone(),
                    state_str: String::new(),
                    time_str: "(no entries)".to_string(),
                    field_ansi: fansi,
                    state_ansi: String::new(),
                    is_dim: false,
                });
            }
            Some(entry) => {
                let is_active = match &entry.verb {
                    Verb::Start => true,
                    Verb::Switch => {
                        entry.value.as_ref().map(|v| v != &serde_json::Value::Bool(false)).unwrap_or(true)
                    }
                    _ => false,
                };

                let state_str = entry
                    .value
                    .as_ref()
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default();

                let sansi = state_ansi(&tf.field, &state_str, &tf.color, state_colors);

                let entry_time = entry.time_start.as_ref().or(entry.time_end.as_ref());
                let time_str = match entry_time {
                    None => String::new(),
                    Some(t) => {
                        if !is_active && entry.verb == Verb::Occur {
                            t.utc.with_timezone(&Local).format("@%H:%M").to_string()
                        } else {
                            format_ago((now - t.utc).num_seconds())
                        }
                    }
                };

                let row = NowRow {
                    bullet: if is_active { "●" } else { " " },
                    field: tf.field.clone(),
                    state_str,
                    time_str,
                    field_ansi: fansi,
                    state_ansi: sansi,
                    is_dim: false,
                };

                if is_active { active.push(row); } else { stopped.push(row); }
            }
        }
    }

    // Sort sections alphabetically
    active.sort_by(|a, b| a.field.cmp(&b.field));
    stopped.sort_by(|a, b| a.field.cmp(&b.field));
    ignored_rows.sort_by(|a, b| a.field.cmp(&b.field));

    // Compute column widths across all visible non-ignored rows
    let all_visible = active.iter().chain(stopped.iter());
    let field_w = all_visible.clone().map(|r| r.field.len()).max().unwrap_or(0).max(1);
    let state_w = all_visible.map(|r| r.state_str.len()).max().unwrap_or(0);

    // Separator: 1 (bullet) + 1 (space) + field_w + 2 (gap) + state_w + 2 (gap) + ~10 for time
    let sep_len = 1 + 1 + field_w + 2 + state_w.max(1) + 2 + 10;
    let sep: String = "─".repeat(sep_len);

    let print_row = |row: &NowRow| {
        let fc = &row.field_ansi;
        let sc = &row.state_ansi;
        if row.is_dim {
            print!("{DIM}{fc}  {field}{RESET}", field = row.field);
        } else {
            print!(
                "{fc}{bullet}{RESET} {fc}{field:<field_w$}{RESET}  {sc}{state:<state_w$}{RESET}  {time}",
                bullet = row.bullet,
                field = row.field,
                state = row.state_str,
                time = row.time_str,
            );
        }
        println!();
    };

    for row in &active { print_row(row); }
    if !active.is_empty() && !stopped.is_empty() { println!("{sep}"); }
    for row in &stopped { print_row(row); }
    if show_ignored && !ignored_rows.is_empty() {
        if !active.is_empty() || !stopped.is_empty() { println!("{sep}"); }
        for row in &ignored_rows { print_row(row); }
    }

    if active.is_empty() && stopped.is_empty() && !(show_ignored && !ignored_rows.is_empty()) {
        println!("No tracked (non-ignored) fields. Use `datum track <field>`.");
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

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

fn format_ago(secs: i64) -> String {
    if secs < 0 {
        return "just now".to_string();
    }
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 { format!("{h}h ago") } else { format!("{h}h{m}m ago") }
    }
}
