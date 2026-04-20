//! Output formatting for datum entries.
//!
//! This module ports datum's "action"-based output format: every write command
//! emits a two-line record showing the action (CREATE/UPDATE/DELETE/...), a
//! short humanId, the full id, and an info line with time/field/state/dur.
//! List commands emit an aligned table with `time | field | state | hid` columns.

use chrono::Local;
use datum_core::{
    db::TrackedField,
    entry::{Entry, Verb},
    field_spec::{FieldSpec, SpecRegistry},
};
use sha2::{Digest, Sha256};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::io::IsTerminal;

// ─── ANSI primitives ─────────────────────────────────────────────────────────

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const INVERSE: &str = "\x1b[7m";
const GRAY_FG: &str = "\x1b[90m";

// ─── Glyphs (datum vocabulary) ───────────────────────────────────────────────

const DOT: &str = "●";
const NON_OCCUR: &str = "¢";
const NULL_SET: &str = "∅";

// ─── Public API: Show / OutputArgs / Action ──────────────────────────────────

/// Show levels, mirroring datum's `Show` enum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Show {
    /// Header + info + non-redundant extra_data.
    #[default]
    Default,
    /// Suppress all output. Useful for scripts.
    None,
    /// Only the header line (action + hid + id).
    Minimal,
    /// Same as Default, but always includes extra_data even with a format string.
    Standard,
    /// Render with `format_string` only (no stock output).
    Format,
    /// Default + full pretty-printed doc.
    All,
}

impl std::str::FromStr for Show {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "default" => Ok(Show::Default),
            "none" => Ok(Show::None),
            "minimal" => Ok(Show::Minimal),
            "standard" => Ok(Show::Standard),
            "format" => Ok(Show::Format),
            "all" => Ok(Show::All),
            other => Err(format!("unknown --show value: {other}")),
        }
    }
}

/// Shared output configuration passed to every write/read command.
#[derive(Clone, Debug, Default)]
pub struct OutputArgs {
    pub show: Show,
    /// `-o` / `--format-string`: `%field%`-templated override.
    pub format_string: Option<String>,
}

impl OutputArgs {
    /// Convenience: `--show none`.
    pub fn silent(&self) -> bool {
        self.show == Show::None
    }
}

/// Semantic outcome of a write. Maps to datum's `ACTIONS` vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Create,
    Update,
    Delete,
    Exists,
    OWrite,
    Rename,
    NoDiff,
    Failed,
}

impl Action {
    fn name(self) -> &'static str {
        match self {
            Action::Create => "CREATE",
            Action::Update => "UPDATE",
            Action::Delete => "DELETE",
            Action::Exists => "EXISTS",
            Action::OWrite => "OWRITE",
            Action::Rename => "RENAME",
            Action::NoDiff => "NODIFF",
            Action::Failed => "FAILED",
        }
    }

    /// Foreground ANSI color for the action tag.
    fn ansi_color(self) -> &'static str {
        match self {
            Action::Create => "\x1b[32m", // green
            Action::Delete | Action::Failed => "\x1b[31m", // red
            Action::Exists => "\x1b[33m", // yellow
            Action::Update | Action::Rename => "\x1b[36m", // cyan
            Action::OWrite => "\x1b[34m", // blue
            Action::NoDiff => "\x1b[38;2;255;165;0m", // orange
        }
    }
}

// ─── TTY / color gating ──────────────────────────────────────────────────────

/// ANSI colors are suppressed when stdout is not a TTY. Testable knob:
/// honoured globally via [`force_color_override`] which the test harness sets.
fn color_enabled() -> bool {
    if let Some(f) = force_color() {
        return f;
    }
    std::io::stdout().is_terminal()
}

// Test-only color force (set via DATUM_COLOR=1/0). Avoids a global static mutex.
fn force_color() -> Option<bool> {
    match std::env::var("DATUM_COLOR").ok().as_deref() {
        Some("1") | Some("always") => Some(true),
        Some("0") | Some("never") => Some(false),
        _ => None,
    }
}

fn paint(code: &str, body: &str) -> String {
    if !color_enabled() || code.is_empty() {
        body.to_string()
    } else {
        format!("{code}{body}{RESET}")
    }
}

// ─── Extraction (datum's `extractFormatted`) ─────────────────────────────────

/// Cached per-extraction rendered fragments. Unused fragments are `None` so
/// the table layer can decide column presence per-row.
#[derive(Clone, Debug, Default)]
pub(crate) struct Extracted {
    pub(crate) action: Option<String>,
    pub(crate) id: Option<String>,
    pub(crate) hid: Option<String>,
    pub(crate) time_occur: Option<String>,
    pub(crate) time_create: Option<String>,
    pub(crate) time_modify: Option<String>,
    pub(crate) time_hybrid: Option<String>,
    pub(crate) field: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) dur: Option<String>,
}

/// Extract per-entry rendered fragments. Colors, if enabled, are applied here.
pub(crate) fn extract(entry: &Entry, action: Option<Action>, registry: Option<&SpecRegistry>) -> Extracted {
    let spec = registry.and_then(|r| r.fields.get(&entry.field));
    let action_text = action.map(|a| paint_action(a));
    let id_text = Some(paint(GRAY_FG, &entry.id));
    let hid_text = Some(paint(action.map(Action::ansi_color).unwrap_or(""), &short_hid(&entry.id)));

    let occur = entry.time_start.as_ref().map(|t| format_human_time(t.utc, false));
    let create = Some(format_human_time(entry.created_at, true));
    let modify = Some(format_human_time(entry.modified_at, true));
    let hybrid = occur.clone().or_else(|| Some(format_gray_prefix("c", format_human_time(entry.created_at, false))));

    Extracted {
        action: action_text,
        id: id_text,
        hid: hid_text,
        time_occur: occur,
        time_create: create,
        time_modify: modify,
        time_hybrid: hybrid,
        field: Some(format_field(&entry.field, spec)),
        state: format_state(entry, spec),
        dur: entry.duration_seconds().map(format_duration),
    }
}

// Datum shortens the meta.humanId (base36 random) to 5 chars. We don't persist
// a separate humanId — derive one deterministically from the ULID so repeated
// lookups are stable. Using the last 5 chars of the lowercased ULID lines up
// with the existing `Entry::short_uid()`.
fn short_hid(id: &str) -> String {
    let n = id.chars().count();
    if n <= 5 {
        id.to_lowercase()
    } else {
        id.chars().skip(n - 5).collect::<String>().to_lowercase()
    }
}

fn paint_action(action: Action) -> String {
    if !color_enabled() {
        return format!(" {} ", action.name());
    }
    // Inverse + color: ` CREATE ` with green-inverse background.
    let code = action.ansi_color();
    format!("{INVERSE}{code} {} {RESET}", action.name())
}

// ─── Field / state colors ────────────────────────────────────────────────────

fn format_field(field: &str, spec: Option<&FieldSpec>) -> String {
    let stored = spec.and_then(|s| s.color.as_deref());
    let code = field_color_code(field, stored);
    paint(&code, field)
}

/// Resolve field color: stored spec color → hash-derived fallback.
fn field_color_code(field: &str, stored: Option<&str>) -> String {
    if let Some(c) = stored {
        ansi_from_stored(c, field)
    } else {
        let n = hash_to_ansi256(field);
        format!("\x1b[38;5;{n}m")
    }
}

fn format_state(entry: &Entry, spec: Option<&FieldSpec>) -> Option<String> {
    // Rendered text for the current value, if any.
    let state_str: Option<String> = match &entry.value {
        None => None,
        Some(JsonValue::Null) => Some(NULL_SET.to_string()),
        Some(JsonValue::Bool(true)) => Some("start".to_string()),
        Some(JsonValue::Bool(false)) => Some("end".to_string()),
        Some(JsonValue::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    };

    // Decorators:
    //  - Occur/start with no state → bullet "●"
    //  - Stop with no state → "end"
    //  - Note with no occur time → "¢"
    let field_stored = spec.and_then(|s| s.color.as_deref());
    let state_colors_map = spec
        .map(|s| s.state_colors.clone())
        .unwrap_or_default();

    let bullet = match (&entry.verb, &state_str) {
        (Verb::Occur, None) | (Verb::Start, None) => {
            let code = field_color_code(&entry.field, field_stored);
            Some(paint(&code, DOT))
        }
        (Verb::Stop, None) => Some("end".to_string()),
        (Verb::Note, _) if entry.time_start.is_none() => {
            let code = field_color_code(&entry.field, field_stored);
            Some(paint(&code, NON_OCCUR))
        }
        _ => None,
    };

    match (bullet, state_str) {
        (Some(b), None) => Some(b),
        (None, Some(s)) => {
            let code = state_color_code(&entry.field, &s, field_stored, &state_colors_map);
            Some(paint(&code, &s))
        }
        (Some(b), Some(s)) => {
            let code = state_color_code(&entry.field, &s, field_stored, &state_colors_map);
            Some(format!("{b} {}", paint(&code, &s)))
        }
        (None, None) => None,
    }
}

fn state_color_code(
    field: &str,
    state: &str,
    field_stored: Option<&str>,
    state_colors: &HashMap<String, String>,
) -> String {
    if let Some(c) = state_colors.get(state) {
        return ansi_from_stored(c, state);
    }
    if let Some(c) = field_stored {
        return ansi_from_stored(c, field);
    }
    let n = hash_to_ansi256(state);
    format!("\x1b[38;5;{n}m")
}

// ─── Time rendering ──────────────────────────────────────────────────────────

fn format_human_time(utc: chrono::DateTime<chrono::Utc>, include_offset: bool) -> String {
    let local = utc.with_timezone(&Local);
    let today = Local::now().date_naive();
    let date = local.date_naive();
    let date_prefix = if date == today {
        String::new()
    } else {
        let delta = (date - today).num_days();
        if delta.abs() <= 3 {
            if delta < 0 { format!("-{}d ", -delta) } else { format!("+{delta}d ") }
        } else {
            format!("{} ", date.format("%Y-%m-%d"))
        }
    };
    let time_str = local.format("%H:%M:%S").to_string();
    let offset = if include_offset || !date_prefix.is_empty() {
        paint(GRAY_FG, &local.format("%z").to_string())
    } else {
        paint(GRAY_FG, &local.format("%z").to_string())
    };
    format!("{date_prefix}{time_str}{offset}")
}

fn format_gray_prefix(prefix: &str, body: String) -> String {
    format!("{}{}", paint(GRAY_FG, prefix), body)
}

// ─── Action output: headers + info lines ─────────────────────────────────────

/// Print the `CREATE`-style two-line header for a single entry.
pub fn show_action(entry: &Entry, action: Action, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    if args.silent() {
        return;
    }
    if args.show == Show::Format {
        if let Some(fmt) = &args.format_string {
            println!("{}", interpolate(fmt, entry));
        }
        return;
    }

    let ex = extract(entry, Some(action), registry);
    // Header line: action + hid + id
    let header = [ex.action.as_deref(), ex.hid.as_deref(), ex.id.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    println!("{header}");

    if args.show == Show::Minimal {
        return;
    }

    // Info line: occur_time + field + state + dur
    let info_parts = [
        ex.time_occur.as_deref(),
        ex.field.as_deref(),
        ex.state.as_deref(),
        ex.dur.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if !info_parts.is_empty() {
        println!("{}", info_parts.join(" "));
    }

    // Extra data
    if matches!(args.show, Show::Default | Show::Standard | Show::All) {
        if let Some(extra) = format_extra_data(entry) {
            println!("  {extra}");
        }
    }
    if args.show == Show::All {
        if let Ok(j) = serde_json::to_string_pretty(entry) {
            println!("{j}");
        }
    }

    if let Some(fmt) = &args.format_string {
        if args.show != Show::Format {
            println!("{}", interpolate(fmt, entry));
        }
    }
}

pub fn show_create(entry: &Entry, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    show_action(entry, Action::Create, registry, args);
}
pub fn show_update(entry: &Entry, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    show_action(entry, Action::Update, registry, args);
}
pub fn show_delete(entry: &Entry, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    show_action(entry, Action::Delete, registry, args);
}
pub fn show_exists(entry: &Entry, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    show_action(entry, Action::Exists, registry, args);
}
pub fn show_nodiff(entry: &Entry, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    show_action(entry, Action::NoDiff, registry, args);
}
pub fn show_failed(entry: &Entry, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    show_action(entry, Action::Failed, registry, args);
}

/// Datum's RENAME output: before id ⟶ after id.
pub fn show_rename(before_id: &str, after_id: &str, args: &OutputArgs) {
    if args.silent() || args.show == Show::Format {
        return;
    }
    let code = Action::Rename.ansi_color();
    let tag = if color_enabled() {
        format!("{INVERSE}{code} {} {RESET}", Action::Rename.name())
    } else {
        format!(" {} ", Action::Rename.name())
    };
    let before_colored = paint(code, before_id);
    let after_colored = paint("\x1b[32m", after_id); // green for new id
    println!("{tag} {before_colored} ⟶ {after_colored}");
}

// ─── Table output (tail / head / grep) ───────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeMetricCol {
    #[default]
    Hybrid,
    Occur,
    Create,
    Modify,
    None,
}

impl std::str::FromStr for TimeMetricCol {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "hybrid" => Ok(TimeMetricCol::Hybrid),
            "occur" => Ok(TimeMetricCol::Occur),
            "create" => Ok(TimeMetricCol::Create),
            "modify" => Ok(TimeMetricCol::Modify),
            "none" => Ok(TimeMetricCol::None),
            other => Err(format!("unknown time metric: {other}")),
        }
    }
}

/// Arguments for table rendering. Wraps `OutputArgs` with table-only knobs.
#[derive(Clone, Debug, Default)]
pub struct TableArgs {
    pub output: OutputArgs,
    pub metric: TimeMetricCol,
    pub columns: Vec<String>,
}

/// Render entries as a table with `time | field | state | dur | hid` columns
/// (plus any extra `columns`). Empty result → `[No data]`. Silenced by
/// `--show none`.
pub fn print_table(entries: &[Entry], registry: Option<&SpecRegistry>, args: &TableArgs) {
    if args.output.silent() {
        return;
    }
    if entries.is_empty() && args.output.show != Show::Format {
        println!("[No data]");
        return;
    }

    // Custom format short-circuits the table.
    if args.output.show == Show::Format {
        if let Some(fmt) = &args.output.format_string {
            for e in entries {
                println!("{}", interpolate(fmt, e));
            }
        }
        return;
    }

    // Build per-row cells.
    struct Row {
        time: String,
        time_metric_prefix: &'static str,
        field: String,
        state: String,
        dur: String,
        hid: String,
        extras: Vec<String>,
    }

    let metric_prefix = |metric: TimeMetricCol| -> &'static str {
        match metric {
            TimeMetricCol::Modify => "m",
            TimeMetricCol::Create => "c",
            _ => "",
        }
    };

    let mut rows: Vec<Row> = Vec::with_capacity(entries.len());
    for e in entries {
        let ex = extract(e, None, registry);
        let (time, prefix) = match args.metric {
            TimeMetricCol::Hybrid => (ex.time_hybrid.clone().unwrap_or_default(), ""),
            TimeMetricCol::Occur => (ex.time_occur.clone().unwrap_or_default(), ""),
            TimeMetricCol::Create => (ex.time_create.clone().unwrap_or_default(), metric_prefix(TimeMetricCol::Create)),
            TimeMetricCol::Modify => (ex.time_modify.clone().unwrap_or_default(), metric_prefix(TimeMetricCol::Modify)),
            TimeMetricCol::None => (String::new(), ""),
        };
        let extras = args
            .columns
            .iter()
            .map(|col| extract_column(e, col))
            .collect();
        rows.push(Row {
            time,
            time_metric_prefix: prefix,
            field: ex.field.unwrap_or_default(),
            state: ex.state.unwrap_or_default(),
            dur: ex.dur.unwrap_or_default(),
            hid: ex.hid.unwrap_or_default(),
            extras,
        });
    }

    // Decide columns: drop empty ones.
    let any = |f: fn(&Row) -> &str| rows.iter().any(|r| !f(r).is_empty());
    let show_time = any(|r| &r.time);
    let show_field = any(|r| &r.field);
    let show_state = any(|r| &r.state);
    let show_dur = any(|r| &r.dur);
    let show_hid = any(|r| &r.hid);

    // Column widths (visual, ANSI-aware).
    let vw = visible_width;
    let time_header = "time";
    let field_header = if rows.iter().any(|r| vw(&r.field) >= 5) { "field" } else { "f" };
    let state_header = if rows.iter().any(|r| vw(&r.state) >= 5) { "state" } else { "s" };
    let dur_header = "dur";
    let hid_header = "hid";

    let time_w = std::iter::once(time_header.len())
        .chain(rows.iter().map(|r| vw(&r.time) + if r.time_metric_prefix.is_empty() { 0 } else { 1 }))
        .max()
        .unwrap_or(0);
    let field_w = std::iter::once(field_header.len())
        .chain(rows.iter().map(|r| vw(&r.field)))
        .max()
        .unwrap_or(0);
    let state_w = std::iter::once(state_header.len())
        .chain(rows.iter().map(|r| vw(&r.state)))
        .max()
        .unwrap_or(0);
    let dur_w = std::iter::once(dur_header.len())
        .chain(rows.iter().map(|r| vw(&r.dur)))
        .max()
        .unwrap_or(0);
    let hid_w = std::iter::once(hid_header.len())
        .chain(rows.iter().map(|r| vw(&r.hid)))
        .max()
        .unwrap_or(0);
    let extras_w: Vec<usize> = args
        .columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            std::iter::once(col.len())
                .chain(rows.iter().map(|r| vw(&r.extras[i])))
                .max()
                .unwrap_or(0)
        })
        .collect();

    // Header row.
    let mut header = String::new();
    if show_time { push_cell(&mut header, time_header, time_w, true); }
    if show_field { push_cell(&mut header, field_header, field_w, false); }
    if show_state { push_cell(&mut header, state_header, state_w, false); }
    if show_dur { push_cell(&mut header, dur_header, dur_w, false); }
    if show_hid { push_cell(&mut header, hid_header, hid_w, false); }
    for (i, col) in args.columns.iter().enumerate() {
        push_cell(&mut header, col, extras_w[i], false);
    }
    println!("{}", header.trim_end());

    // Data rows.
    for r in &rows {
        let mut line = String::new();
        if show_time {
            let body = if r.time_metric_prefix.is_empty() {
                r.time.clone()
            } else {
                format!("{}{}", paint(GRAY_FG, r.time_metric_prefix), r.time)
            };
            let extra_pad = if r.time_metric_prefix.is_empty() { 0 } else { 1 };
            let pad = time_w.saturating_sub(vw(&r.time) + extra_pad);
            line.push_str(&" ".repeat(pad));
            line.push_str(&body);
            line.push_str("  ");
        }
        if show_field { push_cell(&mut line, &r.field, field_w, false); }
        if show_state { push_cell(&mut line, &r.state, state_w, false); }
        if show_dur { push_cell(&mut line, &r.dur, dur_w, false); }
        if show_hid { push_cell(&mut line, &r.hid, hid_w, false); }
        for (i, extra) in r.extras.iter().enumerate() {
            push_cell(&mut line, extra, extras_w[i], false);
        }
        println!("{}", line.trim_end());
    }
}

/// Push a left- or right-padded cell (ANSI-aware) with a 2-space gap.
fn push_cell(buf: &mut String, body: &str, width: usize, right_align: bool) {
    let w = visible_width(body);
    let pad = width.saturating_sub(w);
    if right_align {
        buf.push_str(&" ".repeat(pad));
        buf.push_str(body);
    } else {
        buf.push_str(body);
        buf.push_str(&" ".repeat(pad));
    }
    buf.push_str("  ");
}

/// Count visible characters in a string, ignoring ANSI escape sequences.
pub fn visible_width(s: &str) -> usize {
    let mut in_esc = false;
    let mut width = 0usize;
    for c in s.chars() {
        if in_esc {
            if c == 'm' {
                in_esc = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_esc = true;
            continue;
        }
        width += 1;
    }
    width
}

// ─── Single-entry helpers (for `get` / `delete` preview) ─────────────────────

/// Pretty JSON output for `datum get --pretty`.
pub fn print_entry_pretty(entry: &Entry) -> serde_json::Result<()> {
    println!("{}", serde_json::to_string_pretty(entry)?);
    Ok(())
}

/// Print a single entry's table row (no action header). Useful as a preview.
pub fn print_entry(entry: &Entry, registry: Option<&SpecRegistry>) {
    let args = TableArgs {
        output: OutputArgs::default(),
        metric: TimeMetricCol::Hybrid,
        columns: vec![],
    };
    print_table(std::slice::from_ref(entry), registry, &args);
}

/// Back-compat shim: old callers used `print_logged(&entry)`. Maps to
/// CREATE action with default OutputArgs.
pub fn print_logged(entry: &Entry) {
    show_create(entry, None, &OutputArgs::default());
}

// ─── Extra data rendering ────────────────────────────────────────────────────

/// Format extra_data, flattened with 1 space of leading padding per line.
/// Returns None if there's nothing to show.
fn format_extra_data(entry: &Entry) -> Option<String> {
    let extra = entry.extra_data.as_ref()?;
    let map = extra.as_object()?;
    if map.is_empty() {
        return None;
    }
    let parts: Vec<String> = map
        .iter()
        .map(|(k, v)| format!("{k}={}", render_extra_value(v)))
        .collect();
    Some(parts.join(" "))
}

fn render_extra_value(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        JsonValue::Object(m) => {
            // {"v": x, "u": "unit"} → "xu"
            if let (Some(val), Some(unit)) = (m.get("v"), m.get("u").and_then(|u| u.as_str())) {
                return format!("{}{unit}", render_extra_value(val));
            }
            serde_json::to_string(v).unwrap_or_default()
        }
        other => other.to_string(),
    }
}

fn extract_column(entry: &Entry, col: &str) -> String {
    entry
        .extra_data
        .as_ref()
        .and_then(|v| v.get(col))
        .map(render_extra_value)
        .unwrap_or_default()
}

// ─── Format-string interpolation ────────────────────────────────────────────

/// Minimal `%field%`/`%value%`/`%id%`/`%hid%` template expansion.
pub fn interpolate(fmt: &str, entry: &Entry) -> String {
    let mut out = String::with_capacity(fmt.len());
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut key = String::new();
        let mut closed = false;
        while let Some(&next) = chars.peek() {
            chars.next();
            if next == '%' {
                closed = true;
                break;
            }
            key.push(next);
        }
        if !closed {
            out.push('%');
            out.push_str(&key);
            continue;
        }
        out.push_str(&resolve_template_key(&key, entry));
    }
    out
}

fn resolve_template_key(key: &str, entry: &Entry) -> String {
    match key {
        "field" => entry.field.clone(),
        "id" => entry.id.clone(),
        "hid" => short_hid(&entry.id),
        "slug" => entry.slug.clone(),
        "value" | "state" => entry
            .value
            .as_ref()
            .map(|v| match v {
                JsonValue::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default(),
        "verb" => entry.verb.to_string(),
        other => entry
            .extra_data
            .as_ref()
            .and_then(|v| v.get(other))
            .map(render_extra_value)
            .unwrap_or_default(),
    }
}

// ─── Color helpers (kept public for check/now-view compat) ───────────────────

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

/// Convert a stored color string to an ANSI foreground escape.
/// Accepts: ANSI-256 number ("196"), hex ("ff6600" or "#ff6600").
/// Falls back to hash-derived ANSI-256 for `fallback_name` on parse failure.
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
    let n = hash_to_ansi256(fallback_name);
    format!("\x1b[38;5;{n}m")
}

// ─── Now view (preserved) ────────────────────────────────────────────────────

struct NowRow {
    bullet: &'static str,
    field: String,
    state_str: String,
    time_str: String,
    field_ansi: String,
    state_ansi: String,
    is_dim: bool,
}

fn field_ansi(tf: &TrackedField) -> String {
    match &tf.color {
        Some(c) => ansi_from_stored(c, &tf.field),
        None => {
            let n = hash_to_ansi256(&tf.field);
            format!("\x1b[38;5;{n}m")
        }
    }
}

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
                    Verb::Switch => entry
                        .value
                        .as_ref()
                        .map(|v| v != &JsonValue::Bool(false))
                        .unwrap_or(true),
                    _ => false,
                };

                let state_str = entry
                    .value
                    .as_ref()
                    .map(|v| match v {
                        JsonValue::String(s) => s.clone(),
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

    active.sort_by(|a, b| a.field.cmp(&b.field));
    stopped.sort_by(|a, b| a.field.cmp(&b.field));
    ignored_rows.sort_by(|a, b| a.field.cmp(&b.field));

    let all_visible = active.iter().chain(stopped.iter());
    let field_w = all_visible.clone().map(|r| r.field.len()).max().unwrap_or(0).max(1);
    let state_w = all_visible.map(|r| r.state_str.len()).max().unwrap_or(0);

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

/// Strip all ANSI escape sequences from a string. Handy for snapshot tests.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_esc = false;
    for c in s.chars() {
        if in_esc {
            if c == 'm' {
                in_esc = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_esc = true;
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use datum_core::time::DatumTime;

    #[test]
    fn strip_ansi_removes_color_codes() {
        let s = "\x1b[32mhello\x1b[0m world";
        assert_eq!(strip_ansi(s), "hello world");
    }

    #[test]
    fn visible_width_ignores_escapes() {
        assert_eq!(visible_width("abc"), 3);
        assert_eq!(visible_width("\x1b[32mabc\x1b[0m"), 3);
        // Multibyte glyphs still count as 1 char each.
        assert_eq!(visible_width("●"), 1);
        assert_eq!(visible_width("\x1b[32m●\x1b[0m"), 1);
    }

    #[test]
    fn hash_to_ansi256_deterministic() {
        assert_eq!(hash_to_ansi256("coffee"), hash_to_ansi256("coffee"));
        assert_ne!(hash_to_ansi256("coffee"), hash_to_ansi256("tea"));
    }

    #[test]
    fn show_none_propagates_as_silent() {
        let args = OutputArgs { show: Show::None, format_string: None };
        assert!(args.silent());
    }

    #[test]
    fn show_from_str() {
        assert_eq!("none".parse::<Show>().unwrap(), Show::None);
        assert_eq!("minimal".parse::<Show>().unwrap(), Show::Minimal);
        assert_eq!("all".parse::<Show>().unwrap(), Show::All);
        assert!("bogus".parse::<Show>().is_err());
    }

    #[test]
    fn interpolate_expands_template_keys() {
        let entry = sample_entry("coffee", Verb::Occur, Some(JsonValue::from(2)));
        let out = interpolate("%field%=%value%", &entry);
        assert_eq!(out, "coffee=2");
    }

    #[test]
    fn interpolate_resolves_extra_data_keys() {
        let mut entry = sample_entry("coffee", Verb::Occur, None);
        entry.extra_data = Some(serde_json::json!({"amount": 200}));
        let out = interpolate("%amount%ml", &entry);
        assert_eq!(out, "200ml");
    }

    #[test]
    fn interpolate_renders_hid() {
        let entry = sample_entry("coffee", Verb::Occur, None);
        let hid = interpolate("%hid%", &entry);
        assert_eq!(hid.len(), 5);
        assert_eq!(hid, hid.to_lowercase());
    }

    #[test]
    fn extract_populates_all_primary_fields() {
        let entry = sample_entry("coffee", Verb::Occur, Some(JsonValue::from(2)));
        let ex = extract(&entry, Some(Action::Create), None);
        assert!(ex.action.as_deref().unwrap().contains("CREATE"));
        assert!(ex.hid.is_some());
        assert!(ex.id.is_some());
        assert!(ex.time_occur.is_some());
        assert!(ex.field.is_some());
    }

    #[test]
    fn state_glyph_for_point_occur() {
        // An `occur` with no value → bullet glyph in state col.
        let entry = sample_entry("coffee", Verb::Occur, None);
        let state = format_state(&entry, None).unwrap();
        assert_eq!(strip_ansi(&state), DOT);
    }

    #[test]
    fn state_glyph_for_switch_true_false() {
        let entry_on = sample_entry("mood", Verb::Switch, Some(JsonValue::Bool(true)));
        let entry_off = sample_entry("mood", Verb::Switch, Some(JsonValue::Bool(false)));
        assert_eq!(strip_ansi(&format_state(&entry_on, None).unwrap()), "start");
        assert_eq!(strip_ansi(&format_state(&entry_off, None).unwrap()), "end");
    }

    #[test]
    fn action_colors_distinct_per_action() {
        assert_ne!(Action::Create.ansi_color(), Action::Delete.ansi_color());
        assert_ne!(Action::Update.ansi_color(), Action::Failed.ansi_color());
        assert_ne!(Action::NoDiff.ansi_color(), Action::Create.ansi_color());
    }

    #[test]
    fn table_with_no_entries_is_noop_when_silent() {
        let args = TableArgs {
            output: OutputArgs { show: Show::None, format_string: None },
            ..Default::default()
        };
        // Smoke: does not panic, produces no output.
        print_table(&[], None, &args);
    }

    #[test]
    fn extra_data_unit_annotated_renders_compact() {
        let v = serde_json::json!({"v": 75.2, "u": "kg"});
        assert_eq!(render_extra_value(&v), "75.2kg");
    }

    #[test]
    fn short_hid_returns_last_five_lowercased() {
        assert_eq!(short_hid("01KPNHS91T1F0E0GT8AH0MSYSP"), "msysp");
        assert_eq!(short_hid("ABC"), "abc");
    }

    fn sample_entry(field: &str, verb: Verb, value: Option<JsonValue>) -> Entry {
        let t = DatumTime::now();
        Entry::new(field, verb, value, None, None, Some(t), None, "d1")
    }
}
