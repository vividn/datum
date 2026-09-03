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
pub(crate) mod blocks;
pub mod characters;
pub mod colors;
pub mod show_all;
pub(crate) mod state_truncate;

// Holds `datum-layout-wasm`'s colour derivation to this module's (da-6h6p).
pub mod term_layout;
#[cfg(test)]
mod wasm_parity;

pub use self::action::{
    format_info_line, format_info_line_at, format_relative_day_prefix, format_state_group,
    render_entry_line, RenderContext, Surface,
};
pub use self::colors::{strip_ansi, visible_width};

use datum_core::{
    entry::{Entry, Verb},
    field_spec::SpecRegistry,
    time::DatumTime,
};
use datum_parse::time::{format_human_duration, DurationPrecision};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

// ─── ANSI primitives ─────────────────────────────────────────────────────────

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const GRAY_FG: &str = "\x1b[90m";

// ─── Public API: Show / OutputArgs / Outcome ─────────────────────────────────

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
pub enum Outcome {
    Create,
    Update,
    Delete,
    Exists,
    OWrite,
    Rename,
    Retime,
    NoDiff,
    Failed,
}

impl Outcome {
    fn name(self) -> &'static str {
        match self {
            Outcome::Create => "CREATE",
            Outcome::Update => "UPDATE",
            Outcome::Delete => "DELETE",
            Outcome::Exists => "EXISTS",
            Outcome::OWrite => "OWRITE",
            Outcome::Rename => "RENAME",
            Outcome::Retime => "RETIME",
            Outcome::NoDiff => "NODIFF",
            Outcome::Failed => "FAILED",
        }
    }

    /// Foreground ANSI color for the action tag.
    fn ansi_color(self) -> &'static str {
        match self {
            Outcome::Create => "\x1b[32m",                   // green
            Outcome::Delete | Outcome::Failed => "\x1b[31m", // red
            Outcome::Exists => "\x1b[33m",                   // yellow
            Outcome::Update | Outcome::Rename | Outcome::Retime => "\x1b[36m", // cyan
            Outcome::OWrite => "\x1b[34m",                   // blue
            Outcome::NoDiff => "\x1b[38;2;255;165;0m",       // orange
        }
    }
}

// ─── TTY / color gating ──────────────────────────────────────────────────────

// The color gate (color_enabled / paint) and the truecolor palette (Rgb /
// paint_bg / resolve_rgb / md5_rgb / parse_hex / …) live in `colors.rs` —
// single source of truth. The mod-local palette duplicates were removed
// when `format_field` was unified into `action::format_field_cell`
// (da-c11); the mod-local gate duplicates followed in da-tpny.
use self::colors::{color_enabled, paint};

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
    action: Option<Outcome>,
    registry: Option<&SpecRegistry>,
) -> Extracted {
    extract_with_context(entry, action, registry, None)
}

/// Context-aware variant. `ctx` supplies caller-side information (prior
/// state, first-time-tracked marker) that the entry alone can't carry.
pub(crate) fn extract_with_context(
    entry: &Entry,
    action: Option<Outcome>,
    registry: Option<&SpecRegistry>,
    ctx: Option<&RenderContext>,
) -> Extracted {
    extract_with_context_at(entry, action, registry, ctx, &DatumTime::now(), None)
}

/// As [`extract_with_context`] but with an explicit "now" reference instant.
/// Batch consumers (e.g. tail) supply a single `now` so every row in the
/// batch agrees on what "today" means. `reference_offset` is the table's
/// header UTC offset (da-lkji): `Some(r)` suppresses per-stamp offsets that
/// equal `r` and turns differing ones into a gray `(±off)` prefix; `None`
/// keeps the standalone trailing-offset form.
pub(crate) fn extract_with_context_at(
    entry: &Entry,
    action: Option<Outcome>,
    registry: Option<&SpecRegistry>,
    ctx: Option<&RenderContext>,
    now: &DatumTime,
    reference_offset: Option<i32>,
) -> Extracted {
    let spec = registry.and_then(|r| r.fields.get(&entry.field));
    let action_text = action.map(paint_action);
    let slug_text = Some(paint(GRAY_FG, &entry.slug));
    let hid_text = Some(paint(
        action.map(Outcome::ansi_color).unwrap_or(""),
        &short_hid(&entry.hid),
    ));

    let occur_uncertainty = entry.occur_uncertainty();
    let occur = entry
        .occur_time
        .as_ref()
        .map(|t| self::action::format_timestamp_ref(t, now, occur_uncertainty, reference_offset));
    let create = Some(self::action::format_timestamp_ref(
        &self::action::synthesize_local_datumtime(entry.create_time),
        now,
        None,
        reference_offset,
    ));
    let modify = Some(self::action::format_timestamp_ref(
        &self::action::synthesize_local_datumtime(entry.modify_time),
        now,
        None,
        reference_offset,
    ));
    let hybrid = occur.clone().or_else(|| {
        Some(format_gray_prefix(
            "c",
            self::action::format_timestamp_ref(
                &self::action::synthesize_local_datumtime(entry.create_time),
                now,
                None,
                reference_offset,
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
        dur: entry
            .duration_seconds()
            .map(|s| format_human_duration(s, DurationPrecision::Seconds)),
    }
}

// The `hid` column is a stored 12-char base36 random. The displayed short
// form is the first 5 chars (already lowercase); shorter-than-5 inputs
// (possible only for hand-crafted fixtures) are returned as-is.
fn short_hid(hid: &str) -> String {
    hid.chars().take(5).collect()
}

fn paint_action(action: Outcome) -> String {
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

// ─── Outcome output: headers + info lines ────────────────────────────────────

/// Print the `CREATE`-style two-line header for a single entry.
pub fn show_action(
    entry: &Entry,
    action: Outcome,
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
    action: Outcome,
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

    // The detailed data view (structured-state doc line + columnar data
    // block). Per datumv2's `showSingle` (da-jose): a supplied format
    // string *replaces* this view at the default show — the header + info
    // lines still print, then the format string stands in for the data
    // block. `Standard` keeps the data even alongside a format string;
    // `All` keeps its full doc dump regardless.
    let show_data_view = match args.show {
        Show::Standard | Show::All => true,
        Show::Default => args.format_string.is_none(),
        _ => false,
    };

    // State second line (da-gvj1 rule 3): doc-style renderers expose
    // structured-state internals on a line below the chip. Tail never
    // takes this path (see `print_table_with`).
    if show_data_view {
        if let Some(v) = entry.state.as_ref() {
            if let Some(line) = self::blocks::format_state_doc_line(v, &entry.field, spec) {
                println!("  {line}");
            }
        }
    }

    // User-data block
    if show_data_view {
        if let Some(extra) = self::blocks::format_data_block(entry) {
            println!("{extra}");
        }
    }
    if args.show == Show::All {
        // da-l41v: schema-grouped engineering view — one section per
        // schema.sql comment divider, every column visible (NULL when
        // unset) so the output doubles as a column-by-column reference.
        println!("{}", self::show_all::format_show_all(entry));
    }

    if let Some(fmt) = &args.format_string {
        if args.show != Show::Format {
            println!("{}", interpolate(fmt, entry));
        }
    }
}

pub fn show_create(entry: &Entry, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    show_action(entry, Outcome::Create, registry, args);
}
pub fn show_update(entry: &Entry, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    show_action(entry, Outcome::Update, registry, args);
}
pub fn show_delete(entry: &Entry, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    show_action(entry, Outcome::Delete, registry, args);
}
pub fn show_exists(entry: &Entry, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    show_action(entry, Outcome::Exists, registry, args);
}
pub fn show_nodiff(entry: &Entry, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    show_action(entry, Outcome::NoDiff, registry, args);
}
pub fn show_failed(entry: &Entry, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    show_action(entry, Outcome::Failed, registry, args);
}

/// Context-aware write renderer. Convenience entry point for command-layer
/// callers that have prior-state and/or first-time-tracked info to hand.
pub fn show_create_with_context(
    entry: &Entry,
    registry: Option<&SpecRegistry>,
    ctx: &RenderContext,
    args: &OutputArgs,
) {
    show_action_with_context(entry, Outcome::Create, registry, Some(ctx), args);
}
pub fn show_update_with_context(
    entry: &Entry,
    registry: Option<&SpecRegistry>,
    ctx: &RenderContext,
    args: &OutputArgs,
) {
    show_action_with_context(entry, Outcome::Update, registry, Some(ctx), args);
}

/// Datum's RENAME output: before id ⟶ after id.
pub fn show_rename(before_id: &str, after_id: &str, args: &OutputArgs) {
    if args.silent() || args.show == Show::Format {
        return;
    }
    let code = Outcome::Rename.ansi_color();
    let tag = if color_enabled() {
        format!("{code}{}{RESET}", Outcome::Rename.name())
    } else {
        Outcome::Rename.name().to_string()
    };
    let before_colored = paint(code, before_id);
    let after_colored = paint("\x1b[32m", after_id); // green for new id
    println!("{tag} {before_colored} ⟶ {after_colored}");
}

/// Datum's RETIME output: the entry's stable hid, then the time it moved
/// from ⟶ the time it now carries. Mirrors [`show_rename`] — retime moves a
/// time the way rename moves an id, so it reports the same before/after
/// shape rather than a bare count. One line per entry, so a batch retime
/// reads like a batch delete (one action line per row).
pub fn show_retime(
    entry: &Entry,
    before: &DatumTime,
    registry: Option<&SpecRegistry>,
    args: &OutputArgs,
) {
    if args.silent() || args.show == Show::Format {
        return;
    }
    println!(
        "{}",
        format_retime_line(entry, before, registry, &self::action::now_local())
    );
}

/// The [`show_retime`] line, as a string (`now` injected so tests are
/// deterministic).
fn format_retime_line(
    entry: &Entry,
    before: &DatumTime,
    registry: Option<&SpecRegistry>,
    now: &DatumTime,
) -> String {
    let code = Outcome::Retime.ansi_color();
    let spec = registry.and_then(|r| r.fields.get(&entry.field));
    let after = entry
        .occur_time
        .as_ref()
        .map(|t| self::action::format_timestamp(t, now, entry.occur_uncertainty()))
        .unwrap_or_default();
    format!(
        "{} {} {} {} ⟶ {}",
        paint_action(Outcome::Retime),
        paint(code, &short_hid(&entry.hid)),
        self::action::format_field_cell(&entry.field, spec),
        paint(
            code,
            &self::action::format_timestamp(before, now, before.uncertainty.as_deref(),)
        ),
        paint("\x1b[32m", &after), // green for the new time
    )
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
    /// da-5025: total visible columns the rendered table must fit inside.
    /// When `Some(n)` and the natural column widths would exceed `n`, the
    /// state column is truncated with `…` so no row wraps. `None` keeps
    /// the legacy unbounded layout (used by piped / non-tty callers and
    /// every non-tail surface).
    pub max_total_width: Option<usize>,
}

/// Render entries as a table with `time | field | state | dur | _n | hid` columns
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
    if let Some(text) = format_table_with(entries, contexts, now, registry, args) {
        print!("{text}");
    }
}

/// As [`print_table_with`] but returns the rendered text instead of writing
/// it to stdout. `None` is returned when the table is silenced (`--show
/// none`); every other path returns `Some` even when the body is empty
/// (`[No data]` and `Show::Format` still produce a string). Used by
/// `datum check`'s human renderer to embed the real tail formatter under
/// each flagged-issue block.
pub fn format_table_with(
    entries: &[Entry],
    contexts: Option<&[RenderContext]>,
    now: Option<&DatumTime>,
    registry: Option<&SpecRegistry>,
    args: &TableArgs,
) -> Option<String> {
    if args.output.silent() {
        return None;
    }
    let mut out = String::new();
    if entries.is_empty() && args.output.show != Show::Format {
        out.push_str("[No data]\n");
        return Some(out);
    }

    // Custom format short-circuits the table. Per datumv2's `tableOutput`
    // (da-jose) the presence of a format string alone drives format-only
    // rows here — unlike the single-doc `show_action` path, a table never
    // interleaves the format string with stock columns.
    if let Some(fmt) = &args.output.format_string {
        for e in entries {
            out.push_str(&interpolate(fmt, e));
            out.push('\n');
        }
        return Some(out);
    }

    // Build per-row cells.
    struct Row {
        time: String,
        time_metric_prefix: &'static str,
        field: String,
        /// Raw field name (no ANSI). Lets the table apply the field-cap
        /// truncation (da-ftw3) without re-decoding the painted cell.
        field_name: String,
        state: String,
        dur: String,
        /// da-8sx8: the `_N` quickId shorthand for this row, or empty when
        /// the row is past `QUICK_ORDINAL_CAP` / the caller supplied no
        /// ordinals. Empty for every row → the column drops out.
        quick: String,
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

    // da-lkji: the time column's UTC offset moves into the header. The
    // reference is the most common offset among the displayed stamps (tie →
    // the offset of the newest such row); rows that match print no offset,
    // rows that differ get a gray `(±off)` prefix. Only computed when the
    // header is actually printed — headerless embeds (`datum check`) keep
    // the legacy per-row trailing offsets so the offset stays visible.
    let reference_offset: Option<i32> = if args.suppress_header {
        None
    } else {
        let mut votes: HashMap<i32, (usize, chrono::DateTime<chrono::Utc>)> = HashMap::new();
        for e in entries {
            // P1D rows render a bare date (no clock, no offset) and
            // entries without the metric's stamp render nothing — neither
            // votes for the reference.
            let occur_vote = e
                .occur_time
                .as_ref()
                .filter(|_| e.occur_uncertainty() != Some("P1D"))
                .map(|t| (t.offset_minutes, t.utc));
            let host_vote = |utc: chrono::DateTime<chrono::Utc>| {
                let t = self::action::synthesize_local_datumtime(utc);
                (t.offset_minutes, t.utc)
            };
            let vote = match args.metric {
                TimeMetricCol::Occur => occur_vote,
                TimeMetricCol::Hybrid => {
                    if e.occur_time.is_some() {
                        occur_vote
                    } else {
                        Some(host_vote(e.create_time))
                    }
                }
                TimeMetricCol::Create => Some(host_vote(e.create_time)),
                TimeMetricCol::Modify => Some(host_vote(e.modify_time)),
                TimeMetricCol::None => None,
            };
            if let Some((off, ts)) = vote {
                let slot = votes.entry(off).or_insert((0, ts));
                slot.0 += 1;
                slot.1 = slot.1.max(ts);
            }
        }
        votes
            .into_iter()
            .max_by_key(|&(_, (count, newest))| (count, newest))
            .map(|(off, _)| off)
    };

    let mut rows: Vec<Row> = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        let ctx = contexts.and_then(|c| c.get(i));
        let ex = extract_with_context_at(e, None, registry, ctx, now_ref, reference_offset);
        let (time, prefix) = match args.metric {
            TimeMetricCol::Hybrid => (ex.time_hybrid.clone().unwrap_or_default(), ""),
            TimeMetricCol::Occur => (ex.time_occur.clone().unwrap_or_default(), ""),
            TimeMetricCol::Create => (
                ex.time_create.clone().unwrap_or_default(),
                metric_prefix(TimeMetricCol::Create),
            ),
            TimeMetricCol::Modify => (
                ex.time_modify.clone().unwrap_or_default(),
                metric_prefix(TimeMetricCol::Modify),
            ),
            TimeMetricCol::None => (String::new(), ""),
        };
        let extras = args
            .columns
            .iter()
            .map(|col| extract_column(e, col))
            .collect();
        let quick = ctx
            .and_then(|c| c.quick_ordinal)
            .map(|n| paint(GRAY_FG, &format!("_{n}")))
            .unwrap_or_default();
        rows.push(Row {
            time,
            time_metric_prefix: prefix,
            field: ex.field.unwrap_or_default(),
            field_name: e.field.clone(),
            state: ex.state.unwrap_or_default(),
            dur: ex.dur.unwrap_or_default(),
            quick,
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
    let show_quick = any(|r| &r.quick);
    let show_hid = any(|r| &r.hid);

    // Column widths (visual, ANSI-aware).
    let vw = colors::visible_width;
    // da-lkji: the header carries the reference offset once — `time (+2)`
    // — so same-offset rows can drop theirs entirely.
    let time_header = match reference_offset {
        Some(r) => format!(
            "time {}",
            paint(
                GRAY_FG,
                &format!("({})", self::action::compress_offset_minutes(r))
            )
        ),
        None => "time".to_string(),
    };
    let field_header = if rows.iter().any(|r| vw(&r.field) >= 5) {
        "field"
    } else {
        "f"
    };
    let state_header = if rows.iter().any(|r| vw(&r.state) >= 5) {
        "state"
    } else {
        "s"
    };
    let dur_header = "dur";
    let quick_header = "_n";
    let hid_header = "hid";

    // da-lkji: the time column right-aligns on the `HH:MM:SS` core; the
    // gray `±N<unit>` uncertainty suffix hangs into a shared right margin
    // (`time_suffix_w`) so uncertain rows don't shift the clock.
    let time_suffix_w = rows
        .iter()
        .map(|r| uncertainty_suffix_width(&r.time))
        .max()
        .unwrap_or(0);
    let time_core_w = std::iter::once(vw(&time_header))
        .chain(rows.iter().map(|r| {
            vw(&r.time) - uncertainty_suffix_width(&r.time)
                + if r.time_metric_prefix.is_empty() {
                    0
                } else {
                    1
                }
        }))
        .max()
        .unwrap_or(0);
    let time_w = time_core_w + time_suffix_w;
    // da-ftw3: when a width budget is in force, the field column is capped
    // at `FIELD_CAP` (default 10). Long field names like `environment`
    // otherwise compress the state column unhelpfully. Without a budget
    // (piped output / non-tail callers) the column expands to natural
    // width — scripts that scrape the table see the full label.
    let field_cap = if args.max_total_width.is_some() {
        state_truncate::FIELD_CAP
    } else {
        usize::MAX
    };
    let field_w = std::iter::once(field_header.len())
        .chain(rows.iter().map(|r| vw(&r.field).min(field_cap)))
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
    let quick_w = std::iter::once(quick_header.len())
        .chain(rows.iter().map(|r| vw(&r.quick)))
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

    // da-ftw3: column-width allocation under a width budget.
    //
    // 1. Field column is capped at `FIELD_CAP` (applied above).
    // 2. State column gets the remainder after fixed columns + gaps.
    // 3. If state falls below `STATE_FLOOR`, shrink the field column toward
    //    `FIELD_FLOOR` to recover room. Beyond that the terminal is too
    //    narrow — we still squeeze state down to 0, but the row will be
    //    legibly cramped rather than wrap.
    let (state_w, field_w) = if let Some(budget) = args.max_total_width {
        let gap = 2usize;
        let nonstate_fixed = |fw: usize| -> usize {
            [
                show_time.then_some(time_w),
                show_field.then_some(fw),
                show_dur.then_some(dur_w),
                show_quick.then_some(quick_w),
                show_hid.then_some(hid_w),
            ]
            .into_iter()
            .flatten()
            .map(|w| w + gap)
            .sum::<usize>()
                + extras_w.iter().map(|w| w + gap).sum::<usize>()
        };
        let mut fw = field_w;
        let natural_state = state_w;
        let mut sw = if show_state {
            let remaining = budget.saturating_sub(nonstate_fixed(fw) + gap);
            natural_state.min(remaining.max(state_header.len()))
        } else {
            natural_state
        };
        // Shrink the field column toward FIELD_FLOOR to recover state room.
        while show_state && sw < state_truncate::STATE_FLOOR && fw > state_truncate::FIELD_FLOOR {
            fw -= 1;
            let remaining = budget.saturating_sub(nonstate_fixed(fw) + gap);
            sw = natural_state.min(remaining.max(state_header.len()));
        }
        (sw, fw)
    } else {
        (state_w, field_w)
    };

    // Header row.
    if !args.suppress_header {
        let mut header = String::new();
        if show_time {
            // Right-align on the core (matching the rows' HH:MM:SS edge),
            // then pad across the ± suffix margin.
            header.push_str(&" ".repeat(time_core_w.saturating_sub(vw(&time_header))));
            header.push_str(&time_header);
            header.push_str(&" ".repeat(time_suffix_w));
            header.push_str("  ");
        }
        if show_field {
            push_cell(&mut header, field_header, field_w, false);
        }
        if show_state {
            push_cell(&mut header, state_header, state_w, false);
        }
        if show_dur {
            push_cell(&mut header, dur_header, dur_w, false);
        }
        if show_quick {
            push_cell(&mut header, quick_header, quick_w, false);
        }
        if show_hid {
            push_cell(&mut header, hid_header, hid_w, false);
        }
        for (i, col) in args.columns.iter().enumerate() {
            push_cell(&mut header, col, extras_w[i], false);
        }
        out.push_str(header.trim_end());
        out.push('\n');
    }

    // Data rows.
    for (i, r) in rows.iter().enumerate() {
        let mut line = String::new();
        if show_time {
            let body = if r.time_metric_prefix.is_empty() {
                r.time.clone()
            } else {
                format!("{}{}", paint(GRAY_FG, r.time_metric_prefix), r.time)
            };
            let suffix_w = uncertainty_suffix_width(&r.time);
            let core_w = vw(&r.time) - suffix_w
                + if r.time_metric_prefix.is_empty() {
                    0
                } else {
                    1
                };
            line.push_str(&" ".repeat(time_core_w.saturating_sub(core_w)));
            line.push_str(&body);
            line.push_str(&" ".repeat(time_suffix_w.saturating_sub(suffix_w)));
            line.push_str("  ");
        }
        if show_field {
            // da-ftw3: when the field name overflows the (possibly capped)
            // column width, right-truncate with `…` and re-paint with the
            // field bg so the chip still reads as the field's colour.
            let field_cell = if vw(&r.field_name) > field_w {
                let spec = registry.and_then(|reg| reg.fields.get(&r.field_name));
                state_truncate::truncate_field_cell(&r.field_name, spec, field_w)
            } else {
                r.field.clone()
            };
            push_cell(&mut line, &field_cell, field_w, false);
        }
        if show_state {
            // da-ftw3: per-state truncation. When the natural width fits,
            // use the pre-rendered cell. Otherwise re-render via the
            // truncating path so each parallel state gets its own share
            // of the budget and drop-mode emits a `+k` marker styled with
            // the field's bg.
            let state_cell = if vw(&r.state) > state_w {
                let e = &entries[i];
                let spec = registry.and_then(|reg| reg.fields.get(&e.field));
                let default_ctx = self::action::RenderContext::default();
                let ctx_ref = contexts.and_then(|c| c.get(i)).unwrap_or(&default_ctx);
                state_truncate::truncate_state_cell(e, spec, ctx_ref, state_w)
            } else {
                r.state.clone()
            };
            // Belt-and-braces: if the truncating path overshoots (e.g.
            // a structured chip whose `{id}` body is itself longer than
            // the budget), fall back to ANSI-aware visible truncation
            // so we still never wrap.
            let state_cell = if vw(&state_cell) > state_w {
                term_layout::truncate_visible(&state_cell, state_w)
            } else {
                state_cell
            };
            push_cell(&mut line, &state_cell, state_w, false);
        }
        if show_dur {
            push_cell(&mut line, &r.dur, dur_w, false);
        }
        if show_quick {
            push_cell(&mut line, &r.quick, quick_w, false);
        }
        if show_hid {
            push_cell(&mut line, &r.hid, hid_w, false);
        }
        for (i, extra) in r.extras.iter().enumerate() {
            push_cell(&mut line, extra, extras_w[i], false);
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    Some(out)
}

/// Visible width of the trailing gray `±N<unit>` uncertainty suffix
/// (da-lkji) in a rendered time cell; 0 when absent. `±` only ever
/// appears in that suffix within a time cell, so a plain scan suffices.
fn uncertainty_suffix_width(cell: &str) -> usize {
    let stripped = strip_ansi(cell);
    stripped
        .find('±')
        .map(|i| colors::visible_width(&stripped[i..]))
        .unwrap_or(0)
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
        max_total_width: None,
    };
    let ctx_slice = ctx.map(std::slice::from_ref);
    print_table_with(std::slice::from_ref(entry), ctx_slice, now, registry, &args);
}

/// Back-compat shim: old callers used `print_logged(&entry)`. Maps to
/// CREATE action with default OutputArgs.
pub fn print_logged(entry: &Entry) {
    show_create(entry, None, &OutputArgs::default());
}

// ─── Extra data rendering ────────────────────────────────────────────────────

/// Render a value for non-block consumers — format-string interpolation
/// (`%amount%`) and the `tail --columns` extractor. These callers want a
/// bare, copy-paste-friendly form (no quotes around strings, unit-annotated
/// `{v,u}` concatenated as `<v><u>`), distinct from the user-data block
/// renderer in [`blocks::format_data_block`] which follows the rule-11
/// quoting convention.
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
        "state" => entry
            .state
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
    let Some(v) = value else {
        return (String::new(), 0);
    };
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

// da-adoi: `render_now_view` / `print_now_view` retired. Their last
// caller was the test corpus in `now_cmd.rs` + `integrity_cmd.rs`;
// both migrated to `render_snapshot` against a `NowSnapshot` built
// the same way `Db::current_now` does. Production already used
// `render_snapshot` since da-mpod.

// ─── Snapshot-driven Now view (da-mpod) ──────────────────────────────────────

/// Render a [`NowSnapshot`] as the terminal `datum now` view. Single
/// source of truth with the JSON path: `now.rs` builds the snapshot
/// from `db.current_now(&filter)` and the snapshot's contents alone
/// determine which sections appear:
///
/// * `snapshot.active`        — always rendered (currently-truthy
///   tracked fields).
/// * `snapshot.tracked_false` — surfaces tracked-but-currently-false
///   fields in a dim block when the caller's `NowFilter` opted in
///   (`--tracked` / `--all`). Per da-mpod's "tracked-but-false: dim
///   dot + (off) + last-event time" treatment.
/// * `snapshot.ignored`       — surfaces ignored fields in a dim
///   block when the caller's filter opted in (`--all`).
///
/// Layered disclosure happens entirely in the snapshot construction
/// (now.rs), so the renderer is a straight dump of what's there.
///
/// `state_colors` and `overlap_counts` carry the same semantics as
/// the legacy [`render_now_view`] — the snapshot doesn't (yet) own
/// per-state colour overrides or the overlap-count map, so callers
/// supply them.
pub fn render_snapshot(
    snapshot: &datum_core::db::NowSnapshot,
    state_colors: &HashMap<(String, String), String>,
    overlap_counts: &HashMap<String, u32>,
) -> String {
    use std::fmt::Write;
    let now = chrono::Utc::now();
    let now_dt = DatumTime::now();

    let mut active: Vec<NowRow> = Vec::new();
    let mut tracked_false: Vec<NowRow> = Vec::new();
    let mut ignored_rows: Vec<NowRow> = Vec::new();

    for a in &snapshot.active {
        let tf_color = a.color.clone();
        let fansi = colors::resolve_rgb(tf_color.as_deref(), &a.field).fg();
        let (state_rendered, state_vw) =
            render_now_state(Some(&a.state), &a.field, &tf_color, state_colors);
        let entry_time = a.entry.occur_time.as_ref();
        let time_str = match entry_time {
            None => String::new(),
            Some(t) => format_now_time(
                t,
                &now_dt,
                now,
                true,
                &a.entry.verb,
                a.entry.occur_uncertainty(),
            ),
        };
        active.push(NowRow {
            bullet: "●",
            field: a.field.clone(),
            state_rendered,
            state_vw,
            time_str,
            field_ansi: fansi,
            is_dim: false,
        });
    }

    for f in &snapshot.tracked_false {
        // da-mpod: tracked-but-false rows render dim with a small dot
        // and the `(off)` label, plus the field's last-event time so
        // the user can tell at a glance when the state was last
        // toggled. The `since` and `last_event` fields are equal for
        // this snapshot variant — pick `last_event` for the wall
        // clock and fall back to `since`.
        let fansi = colors::resolve_rgb(f.color.as_deref(), &f.field).fg();
        let state_rendered = "(off)".to_string();
        let state_vw = "(off)".len();
        let time_str = match f.last_event.or(f.since) {
            None => "(no entries)".to_string(),
            Some(t) => match f.entry.as_ref() {
                // Reuse the entry-aware formatter so P1D rows surface
                // their bare date and Point rows get the `@HH:MM`
                // treatment — same time-column rules legacy
                // render_now_view applied to its `stopped` bucket.
                Some(e) => {
                    let local_t = DatumTime {
                        utc: t,
                        offset_minutes: now_dt.offset_minutes,
                        timezone: now_dt.timezone.clone(),
                        uncertainty: None,
                    };
                    format_now_time(
                        &local_t,
                        &now_dt,
                        now,
                        false,
                        &e.verb,
                        e.occur_uncertainty(),
                    )
                }
                None => format_now_time_utc(t, &now_dt, now),
            },
        };
        tracked_false.push(NowRow {
            bullet: "·",
            field: f.field.clone(),
            state_rendered,
            state_vw,
            time_str,
            field_ansi: fansi,
            is_dim: true,
        });
    }

    for i in &snapshot.ignored {
        let fansi = colors::resolve_rgb(i.color.as_deref(), &i.field).fg();
        ignored_rows.push(NowRow {
            bullet: " ",
            field: i.field.clone(),
            state_rendered: String::new(),
            state_vw: 0,
            time_str: String::new(),
            field_ansi: fansi,
            is_dim: true,
        });
    }

    let all_visible = active
        .iter()
        .chain(tracked_false.iter())
        .chain(ignored_rows.iter());
    let field_w = all_visible
        .clone()
        .map(|r| r.field.len())
        .max()
        .unwrap_or(0)
        .max(1);
    let state_w = all_visible.map(|r| r.state_vw).max().unwrap_or(0);

    let sep_len = 1 + 1 + field_w + 2 + state_w.max(1) + 2 + 10;
    let sep: String = "─".repeat(sep_len);

    let mut out = String::new();
    let write_row = |out: &mut String, row: &NowRow| {
        if let Some(&n) = overlap_counts.get(&row.field) {
            if n >= 2 {
                writeln!(
                    out,
                    "⚠ {n} overlapping opens on '{field}' — datum check",
                    field = row.field,
                )
                .unwrap();
            }
        }
        let fc = &row.field_ansi;
        if row.is_dim && row.state_rendered.is_empty() {
            // Ignored row — no state column, just the field name dim.
            write!(out, "{DIM}{fc}  {field}{RESET}", field = row.field).unwrap();
        } else if row.is_dim {
            // Tracked-but-false row — dim throughout, but keep field
            // colour visible so visually it still belongs to the
            // tracked-set.
            let pad = state_w.saturating_sub(row.state_vw);
            let padding: String = " ".repeat(pad);
            write!(
                out,
                "{DIM}{bullet} {field:<field_w$}  {state}{padding}  {time}{RESET}",
                bullet = row.bullet,
                field = row.field,
                state = row.state_rendered,
                time = row.time_str,
            )
            .unwrap();
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

    for row in &active {
        write_row(&mut out, row);
    }
    if !active.is_empty() && !tracked_false.is_empty() {
        writeln!(out, "{sep}").unwrap();
    }
    for row in &tracked_false {
        write_row(&mut out, row);
    }
    if !ignored_rows.is_empty() && (!active.is_empty() || !tracked_false.is_empty()) {
        writeln!(out, "{sep}").unwrap();
    }
    for row in &ignored_rows {
        write_row(&mut out, row);
    }

    if active.is_empty() && tracked_false.is_empty() && ignored_rows.is_empty() {
        writeln!(
            out,
            "No tracked (non-ignored) fields. Use `datum track <field>`."
        )
        .unwrap();
    }
    out
}

/// Format an absolute UTC instant for the now-view time column using
/// the same rules as [`format_now_time`]. Used by the
/// `tracked_false` / `ignored` snapshot variants where we only have
/// the bare timestamp, not the originating `Entry`.
fn format_now_time_utc(
    utc: chrono::DateTime<chrono::Utc>,
    now_dt: &DatumTime,
    now_utc: chrono::DateTime<chrono::Utc>,
) -> String {
    let t = DatumTime {
        utc,
        offset_minutes: now_dt.offset_minutes,
        timezone: now_dt.timezone.clone(),
        uncertainty: None,
    };
    if let Some(prefix) = self::action::format_relative_day_prefix(&t, now_dt) {
        let local = t.local_time();
        return format!("{prefix} {}", local.format("%H:%M"));
    }
    format_ago((now_utc - utc).num_seconds())
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn format_ago(secs: i64) -> String {
    // A negative age means the entry is stamped ahead of now. Sub-minute
    // negatives are clock skew and still read as the present, but anything
    // further out is a genuinely future-stamped entry and has to say so:
    // a mis-typed `-t` (a wrong tz offset, say) otherwise renders as
    // "just now" and a state that has not started reads as the current one.
    if secs < 0 {
        let ahead = -secs;
        return if ahead < 60 {
            "just now".to_string()
        } else {
            format!("in {}", format_span(ahead))
        };
    }
    format!("{} ago", format_span(secs))
}

/// Coarse minute/hour span shared by the past (`… ago`) and future
/// (`in …`) renderings of the now-view time column. A relative age is
/// read at a glance and re-read a minute later, so it names minutes
/// rather than a seconds field that churns on every redraw — but a
/// sub-minute span still says `40s`, never `0m`.
fn format_span(secs: i64) -> String {
    format_human_duration(secs, DurationPrecision::Minutes)
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
    uncertainty: Option<&str>,
) -> String {
    // da-5uph: P1D entries have no meaningful time-of-day. Show the bare
    // local date — `@HH:MM` or `Nm ago` makes no sense when the user
    // logged a date-only event.
    if uncertainty == Some("P1D") {
        return t.local_time().format("%Y-%m-%d").to_string();
    }
    if let Some(prefix) = self::action::format_relative_day_prefix(t, now_dt) {
        let local = t.local_time();
        return format!("{prefix} {}", local.format("%H:%M"));
    }
    // da-8y2q.2: legacy `Verb::Point` rows are now `Verb::Point` with
    // an `occur_time` set (timeless legacy `add` rows have no time and
    // never reach here). Same display rule.
    if !is_active && *verb == Verb::Point {
        t.local_time().format("@%H:%M").to_string()
    } else {
        format_ago((now_utc - t.utc).num_seconds())
    }
}

/// One-line banner to print at the top of `datum now` when there are
/// unresolved integrity warnings. Empty string for `n == 0` so callers
/// can append unconditionally.
pub fn render_pending_integrity_warning(n: u32) -> String {
    if n == 0 {
        return String::new();
    }
    let noun = if n == 1 { "warning" } else { "warnings" };
    format!("⚠ {n} unresolved integrity {noun} — run 'datum check'\n")
}

#[cfg(test)]
mod tests {
    use super::colors::strip_ansi;
    use super::*;
    use datum_core::time::DatumTime;
    // da-jfw6 / da-46nj: tests in this mod toggle the process-global
    // `DATUM_COLOR` / `NO_COLOR` env vars via `ScopedEnv` to drive
    // deterministic color-on/off rendering. Under parallel `cargo test
    // --lib`, one test's mid-flight set_var/remove_var races with
    // another's read inside `color_enabled` / `paint`, producing
    // intermittent failures like "expected ≥2 bg escapes for two
    // chips" with empty backgrounds.
    //
    // `env_lock()` is acquired ONCE per color-toggling test and held
    // for the lifetime of the test body; multiple `ScopedEnv` calls
    // inside the test reuse the same guard, so the second call does
    // NOT re-acquire the lock (and so doesn't deadlock). The mutex
    // is poison-tolerant and process-wide (da-3ywd).
    use datum_test_util::{env_lock, ScopedEnv};

    #[test]
    fn show_none_propagates_as_silent() {
        let args = OutputArgs {
            show: Show::None,
            format_string: None,
        };
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
        let entry = sample_entry("coffee", Verb::Point, Some(JsonValue::from(2)));
        let out = interpolate("%field%=%state%", &entry);
        assert_eq!(out, "coffee=2");
    }

    // da-igwt regression guard: entry "value" vocab is permanently gone.
    // `%state%` is the only state template key; `%value%` must NOT alias it.
    #[test]
    fn value_is_not_a_state_template_alias() {
        let entry = sample_entry("mood", Verb::Switch, Some(serde_json::json!("happy")));
        assert_eq!(resolve_template_key("state", &entry), "happy");
        assert_ne!(
            resolve_template_key("value", &entry),
            "happy",
            "the `value` template key must not alias state (da-igwt: value is gone)"
        );
    }

    #[test]
    fn interpolate_resolves_extra_data_keys() {
        let mut entry = sample_entry("coffee", Verb::Point, None);
        entry.data = Some(serde_json::json!({"amount": 200}));
        let out = interpolate("%amount%ml", &entry);
        assert_eq!(out, "200ml");
    }

    #[test]
    fn interpolate_renders_hid() {
        let entry = sample_entry("coffee", Verb::Point, None);
        let hid = interpolate("%hid%", &entry);
        assert_eq!(hid.len(), 5);
        assert_eq!(hid, hid.to_lowercase());
    }

    #[test]
    fn extract_populates_all_primary_fields() {
        let entry = sample_entry("coffee", Verb::Point, Some(JsonValue::from(2)));
        let ex = extract(&entry, Some(Outcome::Create), None);
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
        let _env_lock = env_lock();
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
        let entry = sample_entry("coffee", Verb::Point, None);
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
        assert_ne!(Outcome::Create.ansi_color(), Outcome::Delete.ansi_color());
        assert_ne!(Outcome::Update.ansi_color(), Outcome::Failed.ansi_color());
        assert_ne!(Outcome::NoDiff.ansi_color(), Outcome::Create.ansi_color());
    }

    #[test]
    fn table_with_no_entries_is_noop_when_silent() {
        let args = TableArgs {
            output: OutputArgs {
                show: Show::None,
                format_string: None,
            },
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
        Entry::new(field, verb, value, None, Some(t), None, "d1")
    }

    /// da-bgq2: a retime reports the move itself — `RETIME <hid> <field>
    /// <before> ⟶ <after>` — the same before/after shape RENAME uses, not
    /// a count of touched rows.
    #[test]
    fn retime_line_shows_before_and_after() {
        let now = DatumTime::now();
        let before = DatumTime {
            utc: "2024-01-01T08:00:00Z".parse().unwrap(),
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
            uncertainty: None,
        };
        let mut entry = sample_entry("coffee", Verb::Point, None);
        entry.occur_time = Some(DatumTime {
            utc: "2024-01-01T09:00:00Z".parse().unwrap(),
            ..before.clone()
        });

        let line = strip_ansi(&format_retime_line(&entry, &before, None, &now));
        assert_eq!(
            line,
            format!(
                "RETIME {} coffee 2024-01-01 08:00:00+0 ⟶ 2024-01-01 09:00:00+0",
                short_hid(&entry.hid)
            ),
        );
    }

    // ─── S4 visual parity tests ──────────────────────────────────────────────

    /// With colors forced on, a string state emits a truecolor BG escape
    /// sequence (`48;2;…`). This is the core of the "background-primary"
    /// port; previously only `38;5;…` foreground escapes were emitted.
    #[test]
    fn string_state_uses_truecolor_background() {
        let _env_lock = env_lock();
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
        let _env_lock = env_lock();
        let _datum = ScopedEnv::remove("DATUM_COLOR");
        let _no = ScopedEnv::set("NO_COLOR", "1");
        assert!(!color_enabled());
        assert_eq!(paint("\x1b[38;2;255;0;0m", "hi"), "hi");
    }

    /// `DATUM_COLOR` overrides `NO_COLOR` — useful for the internal test
    /// matrix that wants to force colour on regardless of env.
    #[test]
    fn datum_color_overrides_no_color() {
        let _env_lock = env_lock();
        let _no = ScopedEnv::set("NO_COLOR", "1");
        let _dc = ScopedEnv::set("DATUM_COLOR", "1");
        assert!(color_enabled());
    }

    // ─── da-gvj1 corpus: state-cell rendering ─────────────────────────────
    //
    // The bead's locked corpus sections A, B, and F live here — these
    // assertions exercise the **main-line chip** path (`format_state_group`
    // / `render_switch_badge`). The state-second-line corpus (C–N) lives
    // in `blocks::tests`.

    // A — structured renders `{id}` chip in main line
    #[test]
    fn structured_state_renders_id_in_curly_braces_in_main_line() {
        let _env_lock = env_lock();
        let _no = ScopedEnv::set("NO_COLOR", "1");
        let entry = sample_entry(
            "call",
            Verb::Switch,
            Some(serde_json::json!({"id": "joe", "title": "Meeting"})),
        );
        let state = strip_ansi(&extract(&entry, None, None).state.unwrap());
        assert!(
            state.contains("{joe}"),
            "expected `{{joe}}` chip: {state:?}"
        );
        assert!(
            !state.contains("Meeting"),
            "main line must not include sub-fields: {state:?}"
        );
    }

    // A — simple-string state unchanged
    #[test]
    fn simple_string_state_unchanged_in_main_line() {
        let _env_lock = env_lock();
        let _no = ScopedEnv::set("NO_COLOR", "1");
        let entry = sample_entry("mood", Verb::Switch, Some(JsonValue::from("happy")));
        let state = strip_ansi(&extract(&entry, None, None).state.unwrap());
        assert!(state.contains("happy"), "{state:?}");
        assert!(
            !state.contains("{"),
            "no curly braces for simple state: {state:?}"
        );
    }

    // B — same id → same chip background color (color stability)
    #[test]
    fn same_structured_state_id_gets_same_color_across_entries() {
        let _env_lock = env_lock();
        let _dc = ScopedEnv::set("DATUM_COLOR", "1");
        let _no = ScopedEnv::remove("NO_COLOR");
        let a = sample_entry(
            "call",
            Verb::Switch,
            Some(serde_json::json!({"id": "bob", "format": "audio"})),
        );
        let b = sample_entry(
            "call",
            Verb::Switch,
            Some(serde_json::json!({"id": "bob", "format": "video"})),
        );
        let sa = extract(&a, None, None).state.unwrap();
        let sb = extract(&b, None, None).state.unwrap();
        let bg = |s: &str| -> String {
            let start = s.find("\x1b[48").unwrap_or(0);
            let end = s[start..]
                .find('m')
                .map(|e| start + e + 1)
                .unwrap_or(s.len());
            s[start..end].to_string()
        };
        assert_eq!(
            bg(&sa),
            bg(&sb),
            "same .id must hash to same chip bg regardless of other fields: a={sa:?} b={sb:?}"
        );
    }

    // B — different id → different chip background color
    #[test]
    fn different_id_gets_different_color() {
        let _env_lock = env_lock();
        let _dc = ScopedEnv::set("DATUM_COLOR", "1");
        let _no = ScopedEnv::remove("NO_COLOR");
        let a = sample_entry("call", Verb::Switch, Some(serde_json::json!({"id": "joe"})));
        let b = sample_entry("call", Verb::Switch, Some(serde_json::json!({"id": "bob"})));
        let sa = extract(&a, None, None).state.unwrap();
        let sb = extract(&b, None, None).state.unwrap();
        assert_ne!(sa, sb, "different ids must paint with different bg");
    }

    // F — tail parallel states use `,` with NO space (rule 6)
    #[test]
    fn tail_parallel_states_separated_by_comma_no_space() {
        let _env_lock = env_lock();
        let _no = ScopedEnv::set("NO_COLOR", "1");
        let entry = sample_entry(
            "body",
            Verb::Switch,
            Some(serde_json::json!(["vertical", "sit"])),
        );
        let state = strip_ansi(&extract(&entry, None, None).state.unwrap());
        // Match the chip-line shape: `vertical,sit` (no space around comma).
        assert!(
            state.contains("vertical,sit"),
            "expected no-space comma: {state:?}"
        );
        assert!(
            !state.contains(", "),
            "tail line must not use `, `: {state:?}"
        );
    }

    // A — parallel of simple states: each member chipped separately
    #[test]
    fn parallel_simple_states_chipped_separately_in_main_line() {
        let _env_lock = env_lock();
        let _dc = ScopedEnv::set("DATUM_COLOR", "1");
        let _no = ScopedEnv::remove("NO_COLOR");
        let entry = sample_entry(
            "body",
            Verb::Switch,
            Some(serde_json::json!(["vertical", "sit"])),
        );
        let state = extract(&entry, None, None).state.unwrap();
        // Two distinct bg-painted regions → two `\x1b[48;2;…m` escape
        // openers in the rendered cell.
        let bg_count = state.matches("\x1b[48;2;").count();
        assert!(
            bg_count >= 2,
            "expected ≥2 bg escapes for two chips: {state:?}"
        );
    }

    /// The now-view time column must not read a future stamp as the present.
    /// Sub-minute negatives stay "just now" (clock skew); past renderings are
    /// unchanged; anything genuinely ahead says how far.
    #[test]
    fn format_ago_names_the_future_instead_of_calling_it_just_now() {
        // Past side — unchanged.
        assert_eq!(format_ago(0), "0s ago");
        assert_eq!(format_ago(59), "59s ago");
        assert_eq!(format_ago(60), "1m ago");
        assert_eq!(format_ago(3599), "59m ago");
        assert_eq!(format_ago(3600), "1h ago");
        assert_eq!(format_ago(3660), "1h1m ago");
        // Clock skew — still the present.
        assert_eq!(format_ago(-1), "just now");
        assert_eq!(format_ago(-59), "just now");
        // Genuinely ahead — must say so, not "just now".
        assert_eq!(format_ago(-60), "in 1m");
        assert_eq!(format_ago(-3600), "in 1h");
        assert_eq!(format_ago(-10080), "in 2h48m");
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
                _datum: ScopedEnv::set("DATUM_COLOR", "1"),
                _no: ScopedEnv::remove("NO_COLOR"),
            }
        }
    }
}
