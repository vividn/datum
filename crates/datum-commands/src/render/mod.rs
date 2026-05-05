//! Output formatting for datum entries.
//!
//! This module ports datum's "action"-based output format: every write command
//! emits a two-line record showing the action (CREATE/UPDATE/DELETE/...), a
//! short humanId, the full id, and an info line with time/field/state/dur.
//! List commands emit an aligned table with `time | field | state | hid` columns.
//!
//! The visual-parity renderer (per `docs/visual-parity-analysis.md`) lives
//! in the [`action`] submodule; color derivation in [`colors`]; character
//! constants in [`characters`]. This root module owns the table / now-view
//! / extract plumbing and glues the write-action path to `action::`.

pub mod action;
pub mod characters;
pub mod colors;

pub use self::action::{
    format_info_line, format_info_line_at, format_relative_day_prefix, format_state_group,
    render_entry_line, RenderContext,
};
pub use self::colors::{strip_ansi, visible_width};

use datum_core::{
    db::TrackedField,
    entry::{Entry, Verb},
    field_spec::SpecRegistry,
    time::DatumTime,
};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::io::IsTerminal;

// ─── ANSI primitives ─────────────────────────────────────────────────────────

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const GRAY_FG: &str = "\x1b[90m";

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

/// ANSI colors are suppressed when stdout is not a TTY, when the standard
/// `NO_COLOR` env var is set (<https://no-color.org/>), or when the
/// test-only `DATUM_COLOR=0`/`never` override is in force.
///
/// Precedence: `DATUM_COLOR` (explicit override, highest) → `NO_COLOR`
/// (standard opt-out) → TTY detection.
fn color_enabled() -> bool {
    if let Some(f) = force_color() {
        return f;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
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

// Truecolor palette (Rgb / paint_bg / resolve_rgb / md5_rgb / parse_hex / …)
// lives in `colors.rs` — single source of truth. The mod-local duplicates
// were removed when `format_field` was unified into `action::format_field_cell`
// (da-c11); colors.rs is the only palette implementation.

// ─── Extraction (datum's `extractFormatted`) ─────────────────────────────────

/// Cached per-extraction rendered fragments. Unused fragments are `None` so
/// the table layer can decide column presence per-row.
#[derive(Clone, Debug, Default)]
pub(crate) struct Extracted {
    pub(crate) action: Option<String>,
    /// User-facing entry reference (datum-compatible `field:time` slug).
    pub(crate) slug: Option<String>,
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
#[allow(dead_code)]
pub(crate) fn extract(
    entry: &Entry,
    action: Option<Action>,
    registry: Option<&SpecRegistry>,
) -> Extracted {
    extract_with_context(entry, action, registry, None)
}

/// Context-aware variant. `ctx` supplies caller-side information (prior
/// state, first-time-tracked marker) that the entry alone can't carry.
pub(crate) fn extract_with_context(
    entry: &Entry,
    action: Option<Action>,
    registry: Option<&SpecRegistry>,
    ctx: Option<&RenderContext>,
) -> Extracted {
    extract_with_context_at(entry, action, registry, ctx, &DatumTime::now())
}

/// As [`extract_with_context`] but with an explicit "now" reference instant.
/// Batch consumers (e.g. tail) supply a single `now` so every row in the
/// batch agrees on what "today" means.
pub(crate) fn extract_with_context_at(
    entry: &Entry,
    action: Option<Action>,
    registry: Option<&SpecRegistry>,
    ctx: Option<&RenderContext>,
    now: &DatumTime,
) -> Extracted {
    let spec = registry.and_then(|r| r.fields.get(&entry.field));
    let action_text = action.map(paint_action);
    let slug_text = Some(paint(GRAY_FG, &entry.slug));
    let hid_text = Some(paint(
        action.map(Action::ansi_color).unwrap_or(""),
        &short_hid(&entry.hid),
    ));

    let occur = entry
        .start_time
        .as_ref()
        .map(|t| self::action::format_timestamp(t, now));
    let create = Some(self::action::format_timestamp(
        &self::action::synthesize_local_datumtime(entry.create_time),
        now,
    ));
    let modify = Some(self::action::format_timestamp(
        &self::action::synthesize_local_datumtime(entry.modify_time),
        now,
    ));
    let hybrid = occur.clone().or_else(|| {
        Some(format_gray_prefix(
            "c",
            self::action::format_timestamp(
                &self::action::synthesize_local_datumtime(entry.create_time),
                now,
            ),
        ))
    });

    let default_ctx = self::action::RenderContext::default();
    let state_ctx = ctx.unwrap_or(&default_ctx);
    Extracted {
        action: action_text,
        slug: slug_text,
        hid: hid_text,
        time_occur: occur,
        time_create: create,
        time_modify: modify,
        time_hybrid: hybrid,
        field: Some(self::action::format_field_cell(&entry.field, spec)),
        state: self::action::format_state_group(entry, spec, state_ctx),
        dur: entry.duration_seconds().map(self::action::format_duration),
    }
}

// The `hid` column is a stored 12-char base36 random. The displayed short
// form is the first 5 chars (already lowercase); shorter-than-5 inputs
// (possible only for hand-crafted fixtures) are returned as-is.
fn short_hid(hid: &str) -> String {
    hid.chars().take(5).collect()
}

fn paint_action(action: Action) -> String {
    if !color_enabled() {
        return action.name().to_string();
    }
    let code = action.ansi_color();
    format!("{code}{}{RESET}", action.name())
}


// ─── Time rendering ──────────────────────────────────────────────────────────

fn format_gray_prefix(prefix: &str, body: String) -> String {
    format!("{}{}", paint(GRAY_FG, prefix), body)
}

// ─── Action output: headers + info lines ─────────────────────────────────────

/// Print the `CREATE`-style two-line header for a single entry.
pub fn show_action(
    entry: &Entry,
    action: Action,
    registry: Option<&SpecRegistry>,
    args: &OutputArgs,
) {
    show_action_with_context(entry, action, registry, None, args)
}

/// Context-aware variant. Passing a [`RenderContext`] enables the transition
/// indicator and first-time-tracked marker; `None` yields identical output to
/// [`show_action`].
pub fn show_action_with_context(
    entry: &Entry,
    action: Action,
    registry: Option<&SpecRegistry>,
    ctx: Option<&RenderContext>,
    args: &OutputArgs,
) {
    if args.silent() {
        return;
    }
    if args.show == Show::Format {
        if let Some(fmt) = &args.format_string {
            println!("{}", interpolate(fmt, entry));
        }
        return;
    }

    let ex = extract_with_context(entry, Some(action), registry, ctx);
    // Header line: action + hid + slug (datum's user-facing id)
    let header = [ex.action.as_deref(), ex.hid.as_deref(), ex.slug.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    println!("{header}");

    if args.show == Show::Minimal {
        return;
    }

    // Info line via the §2 parity renderer: <time> <field-badge>
    // <left-char><state-badge><right-char> [dur]. Replaces the older
    // column-style `<time> <field> <prior ▶ current> <dur>` path.
    let spec = registry.and_then(|r| r.fields.get(&entry.field));
    let default_ctx = self::action::RenderContext::default();
    let render_ctx = ctx.unwrap_or(&default_ctx);
    let info = self::action::format_info_line(entry, spec, render_ctx);
    if !info.is_empty() {
        println!("{info}");
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

/// Context-aware write renderer. Convenience entry point for command-layer
/// callers that have prior-state and/or first-time-tracked info to hand.
pub fn show_create_with_context(
    entry: &Entry,
    registry: Option<&SpecRegistry>,
    ctx: &RenderContext,
    args: &OutputArgs,
) {
    show_action_with_context(entry, Action::Create, registry, Some(ctx), args);
}
pub fn show_update_with_context(
    entry: &Entry,
    registry: Option<&SpecRegistry>,
    ctx: &RenderContext,
    args: &OutputArgs,
) {
    show_action_with_context(entry, Action::Update, registry, Some(ctx), args);
}

/// Datum's RENAME output: before id ⟶ after id.
pub fn show_rename(before_id: &str, after_id: &str, args: &OutputArgs) {
    if args.silent() || args.show == Show::Format {
        return;
    }
    let code = Action::Rename.ansi_color();
    let tag = if color_enabled() {
        format!("{code}{}{RESET}", Action::Rename.name())
    } else {
        Action::Rename.name().to_string()
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
    /// Skip the header row. Used by `tail --watch` so tick-appended batches
    /// don't reprint headers.
    pub suppress_header: bool,
}

/// Render entries as a table with `time | field | state | dur | hid` columns
/// (plus any extra `columns`). Empty result → `[No data]`. Silenced by
/// `--show none`.
pub fn print_table(entries: &[Entry], registry: Option<&SpecRegistry>, args: &TableArgs) {
    print_table_with(entries, None, None, registry, args)
}

/// As [`print_table`] but with optional per-row [`RenderContext`]s and a
/// shared `now` reference instant. The contexts populate the state-cell
/// `<left><state><right>` group with prior-state arrows; the `now`
/// snapshot drives the relative-day prefix on the time column. Both
/// default-fall-through when omitted.
pub fn print_table_with(
    entries: &[Entry],
    contexts: Option<&[RenderContext]>,
    now: Option<&DatumTime>,
    registry: Option<&SpecRegistry>,
    args: &TableArgs,
) {
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

    let now_owned;
    let now_ref: &DatumTime = match now {
        Some(n) => n,
        None => {
            now_owned = DatumTime::now();
            &now_owned
        }
    };

    let mut rows: Vec<Row> = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        let ctx = contexts.and_then(|c| c.get(i));
        let ex = extract_with_context_at(e, None, registry, ctx, now_ref);
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
    let vw = colors::visible_width;
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
    if !args.suppress_header {
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
    }

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
    let w = colors::visible_width(body);
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

// ─── Single-entry helpers (for `get` / `delete` preview) ─────────────────────

/// Pretty JSON output for `datum get --pretty`.
pub fn print_entry_pretty(entry: &Entry) -> serde_json::Result<()> {
    println!("{}", serde_json::to_string_pretty(entry)?);
    Ok(())
}

/// Print a single entry's table row (no action header). Useful as a preview.
///
/// `ctx` and `now` mirror [`print_table_with`]: when both are supplied, the
/// state cell renders the same ambient/transition decoration the live `tail`
/// view uses, so dry-run / confirm previews match what the row will look like
/// after the mutation. Pass `None` for both to fall back to the
/// context-free rendering.
pub fn print_entry(
    entry: &Entry,
    ctx: Option<&RenderContext>,
    now: Option<&DatumTime>,
    registry: Option<&SpecRegistry>,
) {
    let args = TableArgs {
        output: OutputArgs::default(),
        metric: TimeMetricCol::Hybrid,
        columns: vec![],
        suppress_header: false,
    };
    let ctx_slice = ctx.map(std::slice::from_ref);
    print_table_with(
        std::slice::from_ref(entry),
        ctx_slice,
        now,
        registry,
        &args,
    );
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
    let extra = entry.data.as_ref()?;
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
        .data
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
        // `%id%` was the old ULID template key; kept as an alias for slug
        // (the new primary identifier) so any stored format strings still work.
        "id" | "slug" => entry.slug.to_string(),
        "hid" => short_hid(&entry.hid),
        "full_hid" => entry.hid.to_string(),
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
            .data
            .as_ref()
            .and_then(|v| v.get(other))
            .map(render_extra_value)
            .unwrap_or_default(),
    }
}

/// Convenience: apply [`short_hid`] to an `hid` string for external callers.
pub fn short_hid_of(hid: &str) -> String {
    short_hid(hid)
}

// ─── Now view (preserved) ────────────────────────────────────────────────────

struct NowRow {
    bullet: &'static str,
    field: String,
    /// Pre-rendered state column — may include inline ANSI escapes, so
    /// alignment math must go through [`colors::visible_width`] rather than `len()`.
    state_rendered: String,
    state_vw: usize,
    time_str: String,
    field_ansi: String,
    is_dim: bool,
}

fn field_ansi(tf: &TrackedField) -> String {
    colors::resolve_rgb(tf.color.as_deref(), &tf.field).fg()
}

fn state_ansi(
    field: &str,
    state: &str,
    field_stored_color: &Option<String>,
    state_colors: &HashMap<(String, String), String>,
) -> String {
    if let Some(c) = state_colors.get(&(field.to_string(), state.to_string())) {
        return colors::resolve_rgb(Some(c.as_str()), state).fg();
    }
    if let Some(c) = field_stored_color {
        return colors::resolve_rgb(Some(c.as_str()), field).fg();
    }
    colors::md5_rgb(state).fg()
}

/// S12: render the state column for `datum now`. Scalar values get the
/// original single-color treatment; arrays become a checkerboard-flavored
/// sequence of per-element blocks so concurrent states are immediately
/// distinguishable. Returns `(rendered, visible_width)` so the caller can
/// align columns with [`colors::visible_width`].
fn render_now_state(
    value: Option<&JsonValue>,
    field: &str,
    field_stored_color: &Option<String>,
    state_colors: &HashMap<(String, String), String>,
) -> (String, usize) {
    let Some(v) = value else { return (String::new(), 0); };
    match v {
        JsonValue::Array(items) if items.len() > 1 => {
            let parts: Vec<String> = items
                .iter()
                .map(|item| render_now_state_scalar(item, field, field_stored_color, state_colors))
                .collect();
            let rendered = parts.join(",");
            let vw: usize = items.iter().map(|it| scalar_text(it).len()).sum::<usize>()
                + items.len().saturating_sub(1); // commas
            (rendered, vw)
        }
        other => {
            let text = scalar_text(other);
            let rendered = render_now_state_scalar(other, field, field_stored_color, state_colors);
            let vw = text.len();
            (rendered, vw)
        }
    }
}

fn scalar_text(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn render_now_state_scalar(
    v: &JsonValue,
    field: &str,
    field_stored_color: &Option<String>,
    state_colors: &HashMap<(String, String), String>,
) -> String {
    let text = scalar_text(v);
    let ansi = state_ansi(field, &text, field_stored_color, state_colors);
    format!("{ansi}{text}{RESET}")
}

/// Print the redesigned `datum now` view.
///
/// `overlap_counts` maps field → number of live ongoing entries. Fields
/// with a count ≥ 2 get an overlap-warning line prepended by
/// [`render_now_view`]; fields missing from the map are treated as 0.
pub fn print_now_view(
    tracked: &[TrackedField],
    latest_per_field: &HashMap<String, Entry>,
    state_colors: &HashMap<(String, String), String>,
    overlap_counts: &HashMap<String, u32>,
    show_ignored: bool,
) {
    print!(
        "{}",
        render_now_view(
            tracked,
            latest_per_field,
            state_colors,
            overlap_counts,
            show_ignored,
        )
    );
}

/// Render the `datum now` view as a string. Thin sibling of [`print_now_view`]
/// used by integration tests that need to inspect output.
///
/// `overlap_counts` maps field → number of live ongoing entries. A count
/// ≥ 2 prepends an `⚠ N overlapping opens — datum conflicts list` line
/// above the field's row.
pub fn render_now_view(
    tracked: &[TrackedField],
    latest_per_field: &HashMap<String, Entry>,
    state_colors: &HashMap<(String, String), String>,
    overlap_counts: &HashMap<String, u32>,
    show_ignored: bool,
) -> String {
    use std::fmt::Write;
    let now = chrono::Utc::now();
    let now_dt = DatumTime::now();

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
                    state_rendered: String::new(),
                    state_vw: 0,
                    time_str: String::new(),
                    field_ansi: fansi,
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
                    state_rendered: String::new(),
                    state_vw: 0,
                    time_str: "(no entries)".to_string(),
                    field_ansi: fansi,
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

                let (state_rendered, state_vw) = render_now_state(
                    entry.value.as_ref(),
                    &tf.field,
                    &tf.color,
                    state_colors,
                );

                let entry_time = entry.start_time.as_ref().or(entry.end_time.as_ref());
                let time_str = match entry_time {
                    None => String::new(),
                    Some(t) => format_now_time(t, &now_dt, now, is_active, &entry.verb),
                };

                let row = NowRow {
                    bullet: if is_active { "●" } else { " " },
                    field: tf.field.clone(),
                    state_rendered,
                    state_vw,
                    time_str,
                    field_ansi: fansi,
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
    let state_w = all_visible.map(|r| r.state_vw).max().unwrap_or(0);

    let sep_len = 1 + 1 + field_w + 2 + state_w.max(1) + 2 + 10;
    let sep: String = "─".repeat(sep_len);

    let mut out = String::new();
    let write_row = |out: &mut String, row: &NowRow| {
        if let Some(&n) = overlap_counts.get(&row.field) {
            if n >= 2 {
                writeln!(
                    out,
                    "⚠ {n} overlapping opens on '{field}' — datum conflicts list",
                    field = row.field,
                )
                .unwrap();
            }
        }
        let fc = &row.field_ansi;
        if row.is_dim {
            write!(out, "{DIM}{fc}  {field}{RESET}", field = row.field).unwrap();
        } else {
            let pad = state_w.saturating_sub(row.state_vw);
            let padding: String = " ".repeat(pad);
            write!(
                out,
                "{fc}{bullet}{RESET} {fc}{field:<field_w$}{RESET}  {state}{padding}  {time}",
                bullet = row.bullet,
                field = row.field,
                state = row.state_rendered,
                time = row.time_str,
            )
            .unwrap();
        }
        out.push('\n');
    };

    for row in &active { write_row(&mut out, row); }
    if !active.is_empty() && !stopped.is_empty() {
        writeln!(out, "{sep}").unwrap();
    }
    for row in &stopped { write_row(&mut out, row); }
    if show_ignored && !ignored_rows.is_empty() {
        if !active.is_empty() || !stopped.is_empty() {
            writeln!(out, "{sep}").unwrap();
        }
        for row in &ignored_rows { write_row(&mut out, row); }
    }

    if active.is_empty() && stopped.is_empty() && !(show_ignored && !ignored_rows.is_empty()) {
        writeln!(out, "No tracked (non-ignored) fields. Use `datum track <field>`.").unwrap();
    }
    out
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

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

/// Pick the right "last seen" stamp for a `datum now` row. Today entries
/// keep the existing micro-format (`@HH:MM` for stopped occurs, `Nm ago`
/// otherwise); rows from a different calendar day show the relative-day
/// prefix instead so an inactive field doesn't render as `11304h ago`.
fn format_now_time(
    t: &DatumTime,
    now_dt: &DatumTime,
    now_utc: chrono::DateTime<chrono::Utc>,
    is_active: bool,
    verb: &Verb,
) -> String {
    if let Some(prefix) = self::action::format_relative_day_prefix(t, now_dt) {
        let local = t.local_time();
        return format!("{prefix} {}", local.format("%H:%M"));
    }
    if !is_active && *verb == Verb::Occur {
        t.local_time().format("@%H:%M").to_string()
    } else {
        format_ago((now_utc - t.utc).num_seconds())
    }
}

/// Strip all ANSI escape sequences from a string. Handy for snapshot tests.
/// One-line warning to print at the top of `datum now` when there are
/// unresolved conflicts. Empty string for `n == 0` so callers can append
/// unconditionally.
pub fn render_pending_conflict_warning(n: u32) -> String {
    if n == 0 {
        return String::new();
    }
    let noun = if n == 1 { "conflict" } else { "conflicts" };
    format!("⚠ {n} unresolved {noun} — run 'datum conflicts list'\n")
}

/// `datum conflicts show <hid>` rendering: header + side-by-side compare
/// of the local snapshot vs the remote snapshot. The display is plain
/// text so it composes cleanly with `less`/pipes; we deliberately don't
/// reuse the colorised entry renderer (which assumes a live entry).
pub fn render_conflict_show(c: &datum_core::conflicts::Conflict) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let status = match &c.resolution {
        None => "pending".to_string(),
        Some(r) => format!("resolved:{r}"),
    };
    let _ = writeln!(
        out,
        "conflict {hid}  kind={kind}  detected_by={by}  [{status}]",
        hid = c.hid,
        kind = c.conflict_kind.as_str(),
        by = c.detected_by.as_str(),
    );
    let _ = writeln!(out, "  slug:        {}", c.entry_slug);
    let _ = writeln!(out, "  detected_at: {}", c.detected_at.to_rfc3339());
    if let Some(t) = c.resolved_at {
        let _ = writeln!(out, "  resolved_at: {}", t.to_rfc3339());
    }
    let _ = writeln!(out);

    let local_lines = entry_summary_lines(&c.local_entry);
    let remote_lines = match &c.remote_entry {
        Some(e) => entry_summary_lines(e),
        None => vec!["(no counterpart)".to_string()],
    };

    let lwidth = local_lines.iter().map(|s| s.len()).max().unwrap_or(0).max(5);
    let _ = writeln!(out, "  {:<lw$}    {}", "LOCAL", "REMOTE", lw = lwidth);
    let _ = writeln!(
        out,
        "  {:<lw$}    {}",
        "-".repeat(lwidth.min(40)),
        "-".repeat(40),
        lw = lwidth
    );
    let n = local_lines.len().max(remote_lines.len());
    for i in 0..n {
        let l = local_lines.get(i).map(String::as_str).unwrap_or("");
        let r = remote_lines.get(i).map(String::as_str).unwrap_or("");
        let _ = writeln!(out, "  {:<lw$}    {}", l, r, lw = lwidth);
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  resolve with: datum conflicts resolve {} --take local|remote | --edit | --skip",
        c.hid,
    );
    out
}

/// One summary line per non-trivial Entry field. Stable order so the
/// side-by-side display lines up across the local/remote columns.
fn entry_summary_lines(e: &Entry) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("hid={}", e.hid));
    lines.push(format!("slug={}", e.slug));
    lines.push(format!("field={}", e.field));
    lines.push(format!("verb={:?}", e.verb));
    lines.push(format!("shape={:?}", e.entry_shape));
    if let Some(v) = &e.value {
        lines.push(format!("value={v}"));
    }
    if let Some(d) = &e.data {
        lines.push(format!("data={d}"));
    }
    if let Some(u) = &e.unit {
        lines.push(format!("unit={u}"));
    }
    if let Some(t) = &e.start_time {
        lines.push(format!("start={}", t.utc.to_rfc3339()));
    }
    if let Some(t) = &e.end_time {
        lines.push(format!("end={}", t.utc.to_rfc3339()));
    }
    lines.push(format!("device={}", e.device_id));
    lines.push(format!("modified={}", e.modify_time.to_rfc3339()));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::colors::strip_ansi;
    use datum_core::time::DatumTime;

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
        entry.data = Some(serde_json::json!({"amount": 200}));
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
        assert!(ex.slug.is_some());
        assert!(ex.time_occur.is_some());
        assert!(ex.field.is_some());
    }

    /// Per da-c11: table-path field cell and info-line field badge must
    /// share a single renderer. The badge wraps the cell with the §4 leading
    /// space; nothing else differs (no sparkle, no first-time adornment).
    /// This guards against the legacy split that emitted `✨ field` in the
    /// table while the info-line emitted plain `field`.
    #[test]
    fn field_cell_matches_info_line_badge() {
        let _guard = ColorGuard::force_on();
        let entry = sample_entry("newfield", Verb::Switch, Some(JsonValue::from("ok")));
        let ctx = self::action::RenderContext {
            is_first_occurrence: true,
            ..Default::default()
        };
        let table_cell = extract_with_context(&entry, None, None, Some(&ctx))
            .field
            .expect("extractor populates field");
        let badge = self::action::format_field_badge(&entry.field, None);
        assert_eq!(
            badge,
            format!(" {table_cell}"),
            "info-line badge must equal ' ' + table cell"
        );
        // And neither path adds the legacy sparkle prefix.
        assert!(!strip_ansi(&table_cell).contains('✨'));
        assert!(!strip_ansi(&badge).contains('✨'));
    }

    #[test]
    fn state_cell_for_point_occur_emits_dot() {
        // `Occur` with no explicit value → padded DOT block via the canonical
        // §6 state-group renderer.
        let entry = sample_entry("coffee", Verb::Occur, None);
        let ex = extract(&entry, None, None);
        let state = ex.state.expect("state cell");
        assert!(strip_ansi(&state).contains(super::characters::DOT));
    }

    #[test]
    fn state_cell_for_switch_true_false() {
        // `true` → padded "start" block + arrow; `false` renders as " end ".
        let entry_on = sample_entry("mood", Verb::Switch, Some(JsonValue::Bool(true)));
        let entry_off = sample_entry("mood", Verb::Switch, Some(JsonValue::Bool(false)));
        let on = strip_ansi(&extract(&entry_on, None, None).state.unwrap());
        let off = strip_ansi(&extract(&entry_off, None, None).state.unwrap());
        assert!(on.contains("start"), "switch true → start label: {on:?}");
        assert!(off.contains("end"), "switch false → end label: {off:?}");
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
    fn short_hid_returns_first_five_of_hid() {
        assert_eq!(short_hid("abcdefghijkl"), "abcde");
        // Inputs shorter than 5 chars (fixtures/tests) return as-is.
        assert_eq!(short_hid("abc"), "abc");
    }

    fn sample_entry(field: &str, verb: Verb, value: Option<JsonValue>) -> Entry {
        let t = DatumTime::now();
        Entry::new(field, verb, value, None, None, Some(t), None, "d1")
    }

    // ─── S4 visual parity tests ──────────────────────────────────────────────

    /// With colors forced on, a string state emits a truecolor BG escape
    /// sequence (`48;2;…`). This is the core of the "background-primary"
    /// port; previously only `38;5;…` foreground escapes were emitted.
    #[test]
    fn string_state_uses_truecolor_background() {
        let _guard = ColorGuard::force_on();
        let entry = sample_entry("mood", Verb::Switch, Some(JsonValue::from("happy")));
        let out = extract(&entry, None, None).state.unwrap();
        assert!(
            out.contains("\x1b[48;2;"),
            "expected truecolor BG escape, got: {out:?}"
        );
    }

    /// With the de-facto `NO_COLOR` env var set, the color gate should
    /// return false regardless of terminal state. This is the pipe-friendly
    /// behaviour the bead asks for.
    #[test]
    fn no_color_env_suppresses_all_ansi() {
        // Use the NO_COLOR path explicitly by clearing DATUM_COLOR and
        // setting NO_COLOR. The guard restores both on drop.
        let _datum = ScopedEnv::set("DATUM_COLOR", None);
        let _no = ScopedEnv::set("NO_COLOR", Some("1"));
        assert!(!color_enabled());
        assert_eq!(paint("\x1b[38;2;255;0;0m", "hi"), "hi");
    }

    /// `DATUM_COLOR` overrides `NO_COLOR` — useful for the internal test
    /// matrix that wants to force colour on regardless of env.
    #[test]
    fn datum_color_overrides_no_color() {
        let _no = ScopedEnv::set("NO_COLOR", Some("1"));
        let _dc = ScopedEnv::set("DATUM_COLOR", Some("1"));
        assert!(color_enabled());
    }

    // ─── Test helpers ────────────────────────────────────────────────────────

    /// RAII guard that flips `DATUM_COLOR=1` for the test scope and restores
    /// the prior value on drop. Required because `cargo test` threads share
    /// a process, so one test changing env could bleed into another.
    struct ColorGuard {
        _datum: ScopedEnv,
        _no: ScopedEnv,
    }
    impl ColorGuard {
        fn force_on() -> Self {
            Self {
                _datum: ScopedEnv::set("DATUM_COLOR", Some("1")),
                _no: ScopedEnv::set("NO_COLOR", None),
            }
        }
    }

    struct ScopedEnv {
        key: &'static str,
        prior: Option<std::ffi::OsString>,
    }
    impl ScopedEnv {
        fn set(key: &'static str, value: Option<&str>) -> Self {
            let prior = std::env::var_os(key);
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
            Self { key, prior }
        }
    }
    impl Drop for ScopedEnv {
        fn drop(&mut self) {
            match self.prior.take() {
                Some(v) => std::env::set_var(self.key, v),
                None => std::env::remove_var(self.key),
            }
        }
    }
}
