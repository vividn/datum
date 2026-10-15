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
pub mod json_echo;
pub mod ordinals;
pub mod show_all;
pub(crate) mod state_truncate;
pub mod tail_rows;

pub mod term_layout;

pub use self::action::{
    format_info_line, format_info_line_at, format_relative_day_prefix, format_state_group,
    render_entry_line, RenderContext, Surface,
};
pub use self::colors::{strip_ansi, visible_width};
pub use self::ordinals::{attach_quick_ordinals, quick_ordinal_map};

use datum_core::entry::Verb;
use datum_core::{entry::Entry, field_spec::SpecRegistry, time::DatumTime};
use datum_parse::time::{format_human_duration, DurationPrecision};
use serde_json::Value as JsonValue;
use std::borrow::Cow;
use std::collections::HashMap;

// ─── ANSI primitives ─────────────────────────────────────────────────────────

use self::colors::RESET;
const DIM: &str = "\x1b[2m";
/// da-uxgv: the sync-conflict marker's cast — bright yellow, so a `*` on a
/// hid reads as "look at this" without competing with a state or field
/// colour. Drops to plain text under `--no-color` / `NO_COLOR` like every
/// other escape here, since it routes through [`paint`].
const CONFLICT_FG: &str = "\x1b[93m";

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
    /// The action echo as one JSON document on stdout, nothing else
    /// (da-yc9w). See [`json_echo`].
    Json,
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
            "json" => Ok(Show::Json),
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
    /// `--if-changed` found the field already in that state (da-hxmtg).
    IsLast,
    Failed,
}

thread_local! {
    static QUEUED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// While held, every action word is QUEUED (da-9q3fr): the command runs on
/// a snapshot of a database another holds the lock of, and what it prints
/// lands when its spool drains.
pub struct Queued;

impl Queued {
    pub fn hold() -> Self {
        QUEUED.set(true);
        Queued
    }
}

impl Drop for Queued {
    fn drop(&mut self) {
        QUEUED.set(false);
    }
}

impl Outcome {
    fn name(self) -> &'static str {
        if QUEUED.get() {
            return "QUEUED";
        }
        match self {
            Outcome::Create => "CREATE",
            Outcome::Update => "UPDATE",
            Outcome::Delete => "DELETE",
            Outcome::Exists => "EXISTS",
            Outcome::OWrite => "OWRITE",
            Outcome::Rename => "RENAME",
            Outcome::Retime => "RETIME",
            Outcome::NoDiff => "NODIFF",
            Outcome::IsLast => "ISLAST",
            Outcome::Failed => "FAILED",
        }
    }

    /// `#rrggbb` for the action tag — the same six roles [`Self::ansi_color`]
    /// paints, as a colour a surface that is not a terminal can use.
    ///
    /// A REPRESENTATIVE hex for each basic ANSI colour, the way
    /// [`colors::Rgb::GREY`] is `#888888` for bright black: a terminal's
    /// own theme decides what its green looks like, and nothing outside one
    /// has a green until somebody names it. These are legible on a dark
    /// ground, which is every surface that asks for them. `NoDiff` is the one
    /// that is not a choice — it is already truecolor orange in the escape.
    ///
    /// Published as `action_color` by `--show json` (da-ckw3, vividn:
    /// "Make sure these ACTIONs are colored appropriately"), beside
    /// `field_color` and `state_color`, so the web paints datum's own colours
    /// rather than keeping a second palette of its own.
    pub fn hex(self) -> &'static str {
        if QUEUED.get() {
            return "#c061cb"; // magenta
        }
        match self {
            Outcome::Create => "#26a269",                   // green
            Outcome::Delete | Outcome::Failed => "#e01b24", // red
            Outcome::Exists => "#e5a50a",                   // yellow
            Outcome::Update | Outcome::Rename | Outcome::Retime => "#2aa1b3", // cyan
            Outcome::OWrite => "#3584e4",                   // blue
            Outcome::NoDiff => "#ffa500",                   // orange, as emitted
            Outcome::IsLast => "#b8860b",                   // dark amber, as emitted
        }
    }

    /// Foreground ANSI color for the action tag.
    fn ansi_color(self) -> &'static str {
        if QUEUED.get() {
            return "\x1b[35m"; // magenta
        }
        match self {
            Outcome::Create => "\x1b[32m",                   // green
            Outcome::Delete | Outcome::Failed => "\x1b[31m", // red
            Outcome::Exists => "\x1b[33m",                   // yellow
            Outcome::Update | Outcome::Rename | Outcome::Retime => "\x1b[36m", // cyan
            Outcome::OWrite => "\x1b[34m",                   // blue
            Outcome::NoDiff => "\x1b[38;2;255;165;0m",       // orange
            Outcome::IsLast => "\x1b[38;2;184;134;11m",      // dark amber
        }
    }
}

// ─── TTY / color gating ──────────────────────────────────────────────────────

// The color gate (color_enabled / paint) and the truecolor palette (Rgb /
// paint_bg / resolve_rgb / md5_rgb / parse_hex / …) live in `colors.rs` —
// single source of truth. The mod-local palette duplicates were removed
// when `format_field` was unified into `action::format_field_cell`
// (da-c11); the mod-local gate duplicates followed in da-tpny.
use self::colors::{color_enabled, paint, GREY_FG};

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
    extract_with_context_at(
        entry,
        action,
        registry,
        ctx,
        &self::action::now_local(),
        None,
    )
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
    // da-c8mh: a TRACK document says what a field IS, so it names itself
    // in the action slot — `TRACK●`/`█`/`▞`/`¢`, six columns like
    // `CREATE` — on the single-document surfaces that have one: the write
    // echo and `get`. Only where the word would otherwise say "a write
    // landed" or "this row is already here" — a DELETE of a TRACK row is
    // still a DELETE, and saying so is the whole value of that slot.
    // da-ftg8p: NOT in a table. A table reads one row against the others,
    // and an action slot no other row fills is a column one row conjures
    // into being, shifting every other row's cells sideways to pay for it.
    // The `∅` the following row already wears says the field was IGNORED
    // before, which is the information the word carried.
    let names_the_row =
        !QUEUED.get() && matches!(action, None | Some(Outcome::Create) | Some(Outcome::Exists));
    let action_text = match datum_core::track::track_verb(entry).filter(|_| names_the_row) {
        Some(verb) => Some(paint_track(verb)),
        None => action.map(paint_action),
    };
    let slug_text = Some(paint(GREY_FG, &entry.slug));
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
        Some(format_grey_prefix(
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
        dur: self::action::format_dur(entry),
    }
}

// The `hid` column is a stored 12-char base36 random. The displayed short
// form is the first 5 chars (already lowercase); shorter-than-5 inputs
// (possible only for hand-crafted fixtures) are returned as-is.
fn short_hid(hid: &str) -> String {
    hid.chars().take(5).collect()
}

/// The TRACK action word, painted the green a fresh write gets.
fn paint_track(verb: datum_core::action::Action) -> String {
    let word = datum_core::track::track_action_label(verb);
    if !color_enabled() {
        return word.to_string();
    }
    format!("{}{word}{RESET}", Outcome::Create.ansi_color())
}

fn paint_action(action: Outcome) -> String {
    if !color_enabled() {
        return action.name().to_string();
    }
    let code = action.ansi_color();
    format!("{code}{}{RESET}", action.name())
}

// ─── Time rendering ──────────────────────────────────────────────────────────

fn format_grey_prefix(prefix: &str, body: String) -> String {
    format!("{}{}", paint(GREY_FG, prefix), body)
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
/// What [`show_action_with_context`] prints for one entry, as strings.
///
/// The CLI prints these, and `--show json` carries them as its echo.
pub struct ActionLines {
    /// `CREATE 65nfi mood:2026-09-08T14:17:25.061Z` — action, hid, slug.
    pub header: String,
    /// The §2 info line, `16:17:25+2  mood  sad`; empty when there is nothing to say.
    pub info: String,
    /// The default show's data view: the structured-state doc line, then the
    /// user-data block. Empty for a plain string state with no extra data.
    pub data: Vec<String>,
    /// The three pieces [`Self::header`] is built from, ANSI-free, for a
    /// caller that must address them separately — the web bar puts the full
    /// id behind the hid (da-yc9w). `header` stripped of escapes is always
    /// these three joined by a space.
    pub action: String,
    /// The displayed (5-char) hid, ANSI-free.
    pub hid: String,
    /// The full `field:time` slug, ANSI-free.
    pub id: String,
    /// The data view as one string per key — the structured-state doc line,
    /// then one `key: value` per user-data key, unpadded and ANSI-free.
    /// [`Self::data`] is the same content laid out for a terminal.
    pub data_items: Vec<String>,
    /// [`Self::data_items`] under the CLI's own inline-block join, so a
    /// caller never composes one.
    pub data_inline: String,
    /// The entry's field name.
    pub field: String,
    /// The entry's state as logged — a JSON string / bool / null / object,
    /// or `None` where the entry carries none.
    pub state: Option<JsonValue>,
    /// `#rrggbb` for [`Self::action`] — the colour the terminal paints the
    /// action tag, as data (da-ckw3). See [`Outcome::hex`].
    pub action_color: String,
    /// `#rrggbb` for [`Self::field`], from `colors::field_hex` — the very
    /// derivation the terminal badge and `fields --json` paint with.
    pub field_color: String,
    /// `#rrggbb` for [`Self::state`], from `colors::state_hex`; `None` where
    /// there is no state, or where the state paints none (`Bool(false)`).
    pub state_color: Option<String>,
    /// [`Self::info`] split into its coloured runs (da-yu9s), so a surface
    /// that is not a terminal paints the line without ever looking for the
    /// field inside it — `switch mood mood` names it twice. The texts
    /// concatenated equal [`Self::info`] byte for byte.
    pub info_segments: Vec<colors::AnsiSegment>,
}

/// Build [`ActionLines`] against an explicit `now`, so a caller that renders
/// many rows, or a test, gets deterministic "today" semantics.
pub fn action_lines_at(
    entry: &Entry,
    action: Outcome,
    registry: Option<&SpecRegistry>,
    ctx: Option<&RenderContext>,
    now: &DatumTime,
) -> ActionLines {
    let ex = extract_with_context_at(entry, Some(action), registry, ctx, now, None);
    // Header line: action + hid + slug (datum's user-facing id)
    let header = [ex.action.as_deref(), ex.hid.as_deref(), ex.slug.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");

    // Info line via the §2 parity renderer: <time> <field-badge>
    // <left-char><state-badge><right-char> [dur]. Replaces the older
    // column-style `<time> <field> <prior ▶ current> <dur>` path.
    let spec = registry.and_then(|r| r.fields.get(&entry.field));
    let default_ctx = self::action::RenderContext::default();
    let render_ctx = ctx.unwrap_or(&default_ctx);
    let info = self::action::format_info_line_at(entry, spec, render_ctx, now);
    // The same line again with the colour gate forced open, split at its
    // own escapes (da-yu9s). Re-rendering rather than threading a second
    // return type through the renderer is what makes the two forms
    // unable to disagree: `format_info_line_at` is a function of
    // (entry, spec, ctx, now), and every painter here leaves the plain
    // text alone — so these segments' texts are exactly `info` stripped.
    let info_segments = {
        let _painted = self::colors::ColorOverride::set(true);
        self::colors::ansi_segments(&self::action::format_info_line_at(
            entry, spec, render_ctx, now,
        ))
    };

    // State second line (da-gvj1 rule 3): doc-style renderers expose
    // structured-state internals on a line below the chip. Tail never
    // takes this path (see `print_table_with`). Then the user-data block.
    let mut data = Vec::new();
    let mut data_items = Vec::new();
    if let Some(v) = entry.state.as_ref() {
        if let Some(line) = self::blocks::format_state_doc_line(v, &entry.field, spec) {
            data.push(format!("  {line}"));
            data_items.push(strip_ansi(&line));
        }
    }
    if let Some(extra) = self::blocks::format_data_block(entry) {
        data.push(extra);
    }
    data_items.extend(
        self::blocks::data_entry_items(entry)
            .iter()
            .map(|s| strip_ansi(s)),
    );
    let data_inline = self::blocks::data_inline(&data_items);

    ActionLines {
        header,
        info,
        data,
        // Stripped from the very fragments `header` was joined from, so the
        // two can only ever say the same thing.
        action: strip_ansi(ex.action.as_deref().unwrap_or_default()),
        hid: strip_ansi(ex.hid.as_deref().unwrap_or_default()),
        id: strip_ansi(ex.slug.as_deref().unwrap_or_default()),
        data_items,
        data_inline,
        field: entry.field.clone(),
        state: entry.state.clone(),
        action_color: action.hex().to_string(),
        field_color: self::colors::field_hex(&entry.field, spec),
        state_color: entry
            .state
            .as_ref()
            .and_then(|v| self::colors::state_hex(v, &entry.field, spec)),
        info_segments,
    }
}

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

    // `now_local()` is `DatumTime::now()`; the info line always took its
    // "today" from it and the header never reads `now`, so sourcing both
    // from the one builder changes nothing about what is printed.
    let lines = action_lines_at(entry, action, registry, ctx, &self::action::now_local());

    // `--show json`: collect, print nothing. The outermost dispatch scope
    // emits the one document (da-yc9w). `Show::None` already returned above,
    // so silence still wins over the echo.
    if args.show == Show::Json {
        self::json_echo::push(&lines);
        return;
    }

    println!("{}", lines.header);

    if args.show == Show::Minimal {
        return;
    }

    if !lines.info.is_empty() {
        println!("{}", lines.info);
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

    if show_data_view {
        for line in &lines.data {
            println!("{line}");
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
pub fn show_islast(entry: &Entry, registry: Option<&SpecRegistry>, args: &OutputArgs) {
    show_action(entry, Outcome::IsLast, registry, args);
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
///
/// Silent under [`Show::Json`]: RENAME is a half-line about two slugs, not an
/// [`ActionLines`] echo, and giving it a JSON form would mean a second action
/// formatter. Every call site follows it with the real CREATE/UPDATE echo,
/// whose `id` is the new slug — so the document loses only the old one.
pub fn show_rename(before_id: &str, after_id: &str, args: &OutputArgs) {
    if args.silent() || args.show == Show::Format || args.show == Show::Json {
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
    // No `--show json` branch: `retime` carries no output flags at all
    // (`RetimeArgs` has no `OutputArgs`), so this renderer is only ever
    // reached at the default show.
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
        .unwrap_or_else(|| "timeless".into()); // `retime -T`
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

/// A header row over plain cells, for the listings that have no [`Entry`]
/// to lay out — `fields` (da-1lnf). Columns are separated by the entry
/// table's two-space gutter and sized with the same ANSI- and
/// unicode-aware [`visible_width`], so a field named `日本語` sits in its
/// column rather than pushing it.
pub fn format_grid(headers: &[&str], rows: &[Vec<String>]) -> String {
    let widths: Vec<usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            std::iter::once(colors::visible_width(h))
                .chain(
                    rows.iter()
                        .filter_map(|r| r.get(i))
                        .map(|c| colors::visible_width(c)),
                )
                .max()
                .unwrap_or(0)
        })
        .collect();
    let header: Vec<String> = headers.iter().map(|h| (*h).to_string()).collect();
    let mut out = String::new();
    for cells in std::iter::once(&header).chain(rows) {
        let mut line = String::new();
        for (i, cell) in cells.iter().enumerate() {
            push_cell(&mut line, cell, widths[i], false);
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// Render entries as a table with `_n | time | field | state | dur | hid` columns
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

/// One rendered table row: every cell painted, none padded.
///
/// Pulled out of [`format_table_with`] by da-6ec2 so the browser's tail
/// table can be the same cells in `<td>`s rather than a second derivation
/// of them — see [`table_columns_with`].
struct Row {
    time: String,
    time_metric_prefix: &'static str,
    field: String,
    /// Raw field name (no ANSI). Lets the table apply the field-cap
    /// truncation (da-ftw3) without re-decoding the painted cell.
    field_name: String,
    state: String,
    /// da-gw0sh: an occur dot's declared keys' values, plain, as typed
    /// after the field (`25 15` for `lift 25 15`). The terminal table's
    /// alone: `table_columns_with` and `--json` never carry it.
    keys: String,
    dur: String,
    /// da-8sx8: the `_N` quickId shorthand for this row, or empty when
    /// the row is past `QUICK_ORDINAL_CAP` / the caller supplied no
    /// ordinals. Empty for every row → the column drops out.
    quick: String,
    hid: String,
    extras: Vec<String>,
}

/// Build every row's cells, and the header's reference UTC offset
/// (da-lkji). The rows come back in `entries` order.
fn build_rows(
    entries: &[Entry],
    contexts: Option<&[RenderContext]>,
    now: Option<&DatumTime>,
    registry: Option<&SpecRegistry>,
    args: &TableArgs,
) -> (Vec<Row>, Option<i32>) {
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
            now_owned = self::action::now_local();
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
            .map(|n| paint(GREY_FG, &format!("_{n}")))
            .unwrap_or_default();
        // da-uxgv: `*` after the hid means "another row lost an identity
        // contest to this one and is superseded behind it". vividn asked for
        // the conflict to be marked where the reader already is; `datum show
        // <hid> -A` is where the pair itself is inspected.
        let mut hid = ex.hid.unwrap_or_default();
        if ctx.is_some_and(|c| c.has_supersedees) {
            hid.push_str(&paint(CONFLICT_FG, "*"));
        }
        rows.push(Row {
            time,
            time_metric_prefix: prefix,
            field: ex.field.unwrap_or_default(),
            field_name: e.field.clone(),
            state: ex.state.unwrap_or_default(),
            keys: declared_key_values(e, registry),
            dur: ex.dur.unwrap_or_default(),
            quick,
            hid,
            extras,
        });
    }
    (rows, reference_offset)
}

/// An occur dot's values for its spec's declared keys, in the order a
/// person types them (the `-K` slots, then the `-k`), space-separated
/// (da-gw0sh, vividn: «show the first few defined keys in the spec so that
/// it is similar to what a user would type in to log it»). A row with a
/// state of its own, or a key the entry lacks, shows nothing for it.
fn declared_key_values(e: &Entry, registry: Option<&SpecRegistry>) -> String {
    use datum_parse::spec_keys::KeyKind;
    let Some(spec) = registry.and_then(|r| r.fields.get(&e.field)) else {
        return String::new();
    };
    if e.occur_time.is_none() || datum_core::shape::has_state_badge(e.verb, e.dur.as_ref()) {
        return String::new();
    }
    let mut keys = spec.declared_keys(None);
    keys.retain(|k| !matches!(k.kind, KeyKind::Assign(_)));
    // The `-K` slots (`id`) first, as they bind; stable within each.
    keys.sort_by_key(|k| std::cmp::Reverse(k.id));
    keys.iter()
        .map(|k| extract_column(e, &k.name))
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The state cell the terminal table draws in `max` columns: the state,
/// then an occur dot's declared keys (da-gw0sh), cut with `…` where they
/// do not fit. The keys give way; the dot never does.
fn state_with_keys(r: &Row, max: usize) -> String {
    let room = max.saturating_sub(colors::visible_width(&r.state) + 1);
    if r.keys.is_empty() || room == 0 {
        r.state.clone()
    } else {
        format!(
            "{} {}",
            r.state,
            term_layout::truncate_visible(&r.keys, room)
        )
    }
}

/// Which columns the table shows: a column every row leaves empty drops
/// out entirely, so `datum tail` on point entries carries no `state` and
/// `dur` columns at all.
struct Shown {
    time: bool,
    field: bool,
    state: bool,
    dur: bool,
    quick: bool,
    hid: bool,
}

impl Shown {
    fn of(rows: &[Row]) -> Self {
        let any = |f: fn(&Row) -> &str| rows.iter().any(|r| !f(r).is_empty());
        Self {
            time: any(|r| &r.time),
            field: any(|r| &r.field),
            state: any(|r| &r.state),
            dur: any(|r| &r.dur),
            quick: any(|r| &r.quick),
            hid: any(|r| &r.hid),
        }
    }
}

/// The header words the table prints. `field` and `state` shrink to `f`
/// and `s` when no row needs the room, and the time header carries the
/// listing's reference UTC offset (da-lkji).
struct Headers {
    time: String,
    field: &'static str,
    state: &'static str,
}

impl Headers {
    const DUR: &'static str = "dur";
    const QUICK: &'static str = "_n";
    const HID: &'static str = "hid";

    fn of(rows: &[Row], reference_offset: Option<i32>) -> Self {
        let vw = colors::visible_width;
        Self {
            time: match reference_offset {
                Some(r) => format!(
                    "time {}",
                    paint(
                        GREY_FG,
                        &format!("({})", self::action::compress_offset_minutes(r))
                    )
                ),
                None => "time".to_string(),
            },
            field: if rows.iter().any(|r| vw(&r.field) >= 5) {
                "field"
            } else {
                "f"
            },
            state: if rows.iter().any(|r| vw(&r.state) >= 5) {
                "state"
            } else {
                "s"
            },
        }
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

    let args = &with_last_state_column(entries, args);
    let (rows, reference_offset) = build_rows(entries, contexts, now, registry, args);

    let Shown {
        time: show_time,
        field: show_field,
        state: show_state,
        dur: show_dur,
        quick: show_quick,
        hid: show_hid,
    } = Shown::of(&rows);

    // Column widths (visual, ANSI-aware).
    let vw = colors::visible_width;
    let Headers {
        time: time_header,
        field: field_header,
        state: state_header,
    } = Headers::of(&rows, reference_offset);
    let dur_header = Headers::DUR;
    let quick_header = Headers::QUICK;
    let hid_header = Headers::HID;

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
        // `usize::MAX`: no width to fit, so the keys' natural width.
        .chain(rows.iter().map(|r| vw(&state_with_keys(r, usize::MAX))))
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
        if show_quick {
            push_cell(&mut header, quick_header, quick_w, false);
        }
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
        if show_quick {
            push_cell(&mut line, &r.quick, quick_w, false);
        }
        if show_time {
            let body = if r.time_metric_prefix.is_empty() {
                r.time.clone()
            } else {
                format!("{}{}", paint(GREY_FG, r.time_metric_prefix), r.time)
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
                state_with_keys(r, state_w)
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

/// One column of the table: its header word and one cell per row.
///
/// The pieces [`format_table_with`] pads into a terminal grid, handed over
/// unpadded for a surface that has a grid of its own (da-6ec2: the tail
/// pane in the browser is an HTML `<table>`). Every string is painted the
/// way the table paints it — the colour is the CLI's, not the caller's.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TableColumn {
    /// `_n` | `time` | `field` | `state` | `dur` | `hid`, or the name of a
    /// `--column` extra. Stable across the header's own shrinking: the
    /// `field` column keys `field` even when its header word reads `f`.
    pub key: String,
    /// The header word this column prints, painted.
    pub header: String,
    /// One cell per entry, in `entries` order, painted and unpadded.
    pub cells: Vec<String>,
}

/// One of the table's stock columns, before the empty ones are dropped:
/// whether it is shown, its key, its header word, and how to read its cell
/// off a [`Row`].
type StockColumn = (bool, &'static str, String, fn(&Row) -> String);

/// The table's columns for `entries` — the same cells [`format_table_with`]
/// lays out, without the terminal grid.
///
/// Column choice is the table's: a column no row fills drops out, and the
/// `field`/`state` headers shrink to `f`/`s` on the same rule. What does
/// NOT carry across is `args.max_total_width`: the width budget exists to
/// stop a terminal row wrapping, and a caller with real columns wraps or
/// scrolls them itself, so cells come back at their natural width.
pub fn table_columns_with(
    entries: &[Entry],
    contexts: Option<&[RenderContext]>,
    now: Option<&DatumTime>,
    registry: Option<&SpecRegistry>,
    args: &TableArgs,
) -> Vec<TableColumn> {
    let args = &with_last_state_column(entries, args);
    let (rows, reference_offset) = build_rows(entries, contexts, now, registry, args);
    let shown = Shown::of(&rows);
    let headers = Headers::of(&rows, reference_offset);
    // The CLI's order, which is the reader's: what to type (da-431c, vividn:
    // «move that column to be the first column on both CLI and web»), when
    // it landed, what it is about, what it says, how long, what to keep.
    // The terminal table pads the same order. A column no row fills is
    // dropped here exactly as the table drops it.
    let stock: [StockColumn; 6] = [
        (shown.quick, "_n", Headers::QUICK.to_string(), |r| {
            r.quick.clone()
        }),
        (shown.time, "time", headers.time, |r| {
            if r.time_metric_prefix.is_empty() {
                r.time.clone()
            } else {
                format!("{}{}", paint(GREY_FG, r.time_metric_prefix), r.time)
            }
        }),
        (shown.field, "field", headers.field.to_string(), |r| {
            r.field.clone()
        }),
        (shown.state, "state", headers.state.to_string(), |r| {
            r.state.clone()
        }),
        (shown.dur, "dur", Headers::DUR.to_string(), |r| {
            r.dur.clone()
        }),
        (shown.hid, "hid", Headers::HID.to_string(), |r| {
            r.hid.clone()
        }),
    ];
    let mut out: Vec<TableColumn> = stock
        .into_iter()
        .filter(|&(shown, ..)| shown)
        .map(|(_, key, header, cell)| TableColumn {
            key: key.to_string(),
            header,
            cells: rows.iter().map(cell).collect(),
        })
        .collect();
    // The `--column NAME` extras, after the stock set, as the table puts them.
    for (i, col) in args.columns.iter().enumerate() {
        out.push(TableColumn {
            key: col.clone(),
            header: col.clone(),
            cells: rows.iter().map(|r| r.extras[i].clone()).collect(),
        });
    }
    out
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
/// bare, copy-paste-friendly form (no quotes around strings), distinct
/// from the user-data block
/// renderer in [`blocks::format_data_block`] which follows the rule-11
/// quoting convention.
fn render_extra_value(v: &JsonValue) -> String {
    match v {
        JsonValue::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn extract_column(entry: &Entry, col: &str) -> String {
    if let (LAST_STATE_COLUMN, Some(v)) = (col, &entry.user_asserted_last_state) {
        return render_extra_value(v);
    }
    entry
        .data
        .as_ref()
        .and_then(|v| v.get(col))
        .map(|v| {
            render_extra_value(&datum_core::doc_ref::shown(
                datum_core::entry::shown_data_value(col, v),
            ))
        })
        .unwrap_or_default()
}

/// The column `-L`'s assertion shows in, ahead of a `last_state` data key.
const LAST_STATE_COLUMN: &str = "last_state";

/// `-A` adds the `last_state` column when a row asserts one (da-n86g3).
fn with_last_state_column<'a>(entries: &[Entry], args: &'a TableArgs) -> Cow<'a, TableArgs> {
    if args.output.show != Show::All
        || args.columns.iter().any(|c| c == LAST_STATE_COLUMN)
        || entries.iter().all(|e| e.user_asserted_last_state.is_none())
    {
        return Cow::Borrowed(args);
    }
    let mut args = args.clone();
    args.columns.push(LAST_STATE_COLUMN.to_string());
    Cow::Owned(args)
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
            .map(|v| {
                render_extra_value(&datum_core::doc_ref::shown(
                    datum_core::entry::shown_data_value(other, v),
                ))
            })
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
            let vw = colors::visible_width(&rendered);
            (rendered, vw)
        }
        other => {
            let rendered = render_now_state_scalar(other, field, field_stored_color, state_colors);
            let vw = colors::visible_width(&rendered);
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

/// Render a [`NowSnapshot`](datum_core::db::NowSnapshot) as the terminal `datum now` view. Single
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
/// the legacy `render_now_view` — the snapshot doesn't (yet) own
/// per-state colour overrides or the overlap-count map, so callers
/// supply them.
///
/// `color` is the caller's colour gate (da-5v3pl): off, the view carries
/// no escape at all, the DIM of the tracked-false and ignored rows included.
///
/// `cols` is the width to fit (da-coi0m): the field column is capped at a
/// third of it, as the band's label is, the state column takes what the
/// time column leaves, each cut with `…`, and no line runs past it. `None`
/// (piped, no `$COLUMNS`) prints every column whole.
pub fn render_snapshot(
    snapshot: &datum_core::db::NowSnapshot,
    state_colors: &HashMap<(String, String), String>,
    overlap_counts: &HashMap<String, u32>,
    color: bool,
    cols: Option<usize>,
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
        // and the `(off)` label, plus the time the state was last
        // toggled so the user can tell at a glance how long it has been
        // off. That is `since`; it usually equals `last_event`, and
        // da-phoe is where they part — a block that has handed the
        // field back is the last event, but the state it handed back to
        // began at the block's trailing edge.
        let fansi = colors::resolve_rgb(f.color.as_deref(), &f.field).fg();
        let state_rendered = "(off)".to_string();
        let state_vw = "(off)".len();
        let time_str = match f.since.or(f.last_event) {
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
    let mut field_w = all_visible
        .clone()
        .map(|r| colors::visible_width(&r.field))
        .max()
        .unwrap_or(0)
        .max(1);
    let mut state_w = all_visible.clone().map(|r| r.state_vw).max().unwrap_or(0);
    let max_w = cols.unwrap_or(usize::MAX);
    if let Some(c) = cols {
        let time_w = all_visible
            .map(|r| colors::visible_width(&r.time_str))
            .max()
            .unwrap_or(0);
        field_w = field_w.min((c / 3).max(6));
        state_w = state_w.min(c.saturating_sub(2 + field_w + 2 + 2 + time_w).max(1));
    }
    // A cell cut to its column, then padded to it by display width —
    // `{:<w$}` pads by chars, which a wide glyph throws out.
    let fit = |s: &str, w: usize| {
        let s = term_layout::truncate_visible(s, w);
        let pad = w.saturating_sub(colors::visible_width(&s));
        format!("{s}{}", " ".repeat(pad))
    };

    let sep_len = 1 + 1 + field_w + 2 + state_w.max(1) + 2 + 10;
    let sep: String = "─".repeat(sep_len.min(max_w));

    let mut out = String::new();
    let write_row = |out: &mut String, row: &NowRow| {
        if let Some(&n) = overlap_counts.get(&row.field) {
            if n >= 2 {
                let warn = format!(
                    "⚠ {n} overlapping opens on '{field}' — datum check",
                    field = row.field,
                );
                writeln!(out, "{}", term_layout::truncate_visible(&warn, max_w)).unwrap();
            }
        }
        let fc = &row.field_ansi;
        let field = fit(&row.field, field_w);
        let state = fit(&row.state_rendered, state_w);
        let line = if row.is_dim && row.state_rendered.is_empty() {
            // Ignored row — no state column, just the field name dim.
            let field = term_layout::truncate_visible(&row.field, field_w);
            format!("{DIM}{fc}  {field}{RESET}")
        } else if row.is_dim {
            // Tracked-but-false row — dim throughout, but keep field
            // colour visible so visually it still belongs to the
            // tracked-set.
            format!(
                "{DIM}{bullet} {field}  {state}  {time}{RESET}",
                bullet = row.bullet,
                time = row.time_str,
            )
        } else {
            format!(
                "{fc}{bullet}{RESET} {fc}{field}{RESET}  {state}  {time}",
                bullet = row.bullet,
                time = row.time_str,
            )
        };
        // The last word on a width too narrow for even the time column.
        out.push_str(&term_layout::truncate_visible(&line, max_w));
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
    if color {
        out
    } else {
        colors::strip_ansi(&out)
    }
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
    // `FORCE_COLOR` / `NO_COLOR` env vars via `ScopedEnv` to drive
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

    /// Every outcome has a hex, and it says the same thing its escape does.
    ///
    /// The web paints the ACTION word from `action_color` (da-ckw3), so the
    /// two palettes have to agree about which outcomes share a colour — the
    /// day one of them gains a role the other has not, a DELETE and a FAILED
    /// stop matching on one surface and keep matching on the other.
    #[test]
    fn every_outcome_hex_matches_the_role_its_escape_paints() {
        use Outcome::*;
        let all = [
            Create, Update, Delete, Exists, OWrite, Rename, Retime, NoDiff, IsLast, Failed,
        ];
        for a in all {
            let hex = a.hex();
            assert_eq!(hex.len(), 7, "{} has no #rrggbb: {hex:?}", a.name());
            assert!(hex.starts_with('#'), "{} has no #rrggbb: {hex:?}", a.name());
            assert!(
                hex[1..].chars().all(|c| c.is_ascii_hexdigit()),
                "{} has no #rrggbb: {hex:?}",
                a.name()
            );
            for b in all {
                assert_eq!(
                    a.hex() == b.hex(),
                    a.ansi_color() == b.ansi_color(),
                    "{} and {} agree in one palette and not the other",
                    a.name(),
                    b.name()
                );
            }
        }
    }

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
            prior_state_ignored: true,
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
        // The guard restores NO_COLOR on drop.
        let _env_lock = env_lock();
        let _no = ScopedEnv::set("NO_COLOR", "1");
        assert!(!color_enabled());
        assert_eq!(paint("\x1b[38;2;255;0;0m", "hi"), "hi");
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
        let _dc = ScopedEnv::set("FORCE_COLOR", "1");
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
        let _dc = ScopedEnv::set("FORCE_COLOR", "1");
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
        let _dc = ScopedEnv::set("FORCE_COLOR", "1");
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

    /// da-n86g3: `-A` adds a `last_state` column holding what `-L` asserted,
    /// only when a row asserts one; the default view never does.
    #[test]
    fn show_all_adds_the_asserted_last_state_column() {
        let mut asserted = sample_entry("mood", Verb::Switch, Some(serde_json::json!("sad")));
        asserted.user_asserted_last_state = Some(serde_json::json!("happy"));
        let plain = sample_entry("mood", Verb::Switch, Some(serde_json::json!("ok")));
        let last_state = |show, entries: &[Entry]| {
            let args = TableArgs {
                output: OutputArgs {
                    show,
                    ..Default::default()
                },
                ..Default::default()
            };
            table_columns_with(entries, None, None, None, &args)
                .into_iter()
                .find(|c| c.key == "last_state")
                .map(|c| c.cells)
        };
        let both = [asserted, plain.clone()];
        assert_eq!(
            last_state(Show::All, &both),
            Some(vec!["happy".to_string(), String::new()])
        );
        assert_eq!(last_state(Show::Default, &both), None);
        assert_eq!(last_state(Show::All, &[plain]), None);
    }

    // ─── Test helpers ────────────────────────────────────────────────────────

    /// RAII guard that flips `FORCE_COLOR=1` for the test scope and restores
    /// the prior value on drop. Required because `cargo test` threads share
    /// a process, so one test changing env could bleed into another.
    struct ColorGuard {
        _force: ScopedEnv,
        _no: ScopedEnv,
    }
    impl ColorGuard {
        fn force_on() -> Self {
            Self {
                _force: ScopedEnv::set("FORCE_COLOR", "1"),
                _no: ScopedEnv::remove("NO_COLOR"),
            }
        }
    }

    /// da-w149: the tail pane's state cell for a timeless note naming a
    /// state is `¢` and the label in the state's colour as fg — the colour
    /// the pane reports as `state_color` — with no bg chip.
    #[test]
    fn a_timeless_state_notes_cell_is_its_state_colour_as_fg() {
        let _color = colors::ColorOverride::set(true);
        let state = serde_json::json!("outside");
        let mut entry = sample_entry("environment", Verb::Point, Some(state.clone()));
        entry.occur_time = None;
        entry.dur = Some(datum_core::dur::Dur::moment());
        let columns = table_columns_with(
            std::slice::from_ref(&entry),
            None,
            None,
            None,
            &TableArgs::default(),
        );
        let cell = &columns.iter().find(|c| c.key == "state").unwrap().cells[0];
        let fg = colors::state_value_rgb(&state, "environment", None)
            .unwrap()
            .fg();
        assert_eq!(
            *cell,
            format!(
                "{}{}",
                colors::paint(&fg, "¢"),
                colors::paint(&fg, "outside")
            )
        );
        assert!(!cell.contains("\x1b[48;2;"), "{cell:?}");
    }

    // ─── da-gw0sh: an occur dot's declared keys ───────────────────────────

    /// `lift` declares `weightKg` then `reps` — not alphabetical, so a
    /// row read in key order would say `15 25`.
    fn lift(data: JsonValue) -> (Entry, SpecRegistry) {
        let mut spec =
            datum_core::field_spec::FieldSpec::new("lift", datum_core::action::Action::Occur);
        spec.data_keys = vec!["weightKg".into(), "reps=".into()];
        spec.assign.insert("gym".into(), "home".into());
        let mut registry = SpecRegistry::new();
        registry.register_field(spec);
        let mut e = sample_entry("lift", Verb::Point, None);
        e.data = Some(data);
        (e, registry)
    }

    fn table(entries: &[Entry], registry: &SpecRegistry, width: Option<usize>) -> String {
        let args = TableArgs {
            max_total_width: width,
            ..Default::default()
        };
        strip_ansi(&format_table_with(entries, None, None, Some(registry), &args).unwrap())
    }

    /// vividn's B7: `lift 25 15` reads back `● 25 15`, in the order typed;
    /// an assigned key, which nobody types, is not shown.
    #[test]
    fn occur_row_shows_declared_keys_as_typed() {
        let _env_lock = env_lock();
        let _no = ScopedEnv::set("NO_COLOR", "1");
        let (e, registry) = lift(serde_json::json!({"weightKg": 25, "reps": 15, "gym": "home"}));
        let out = table(&[e], &registry, None);
        assert!(out.contains("● 25 15  "), "{out}");
        assert!(!out.contains("home"), "{out}");
    }

    /// A value too long for the terminal is cut with `…`, the dot kept,
    /// and no line passes the width.
    #[test]
    fn occur_row_keys_truncate_within_the_width() {
        let _env_lock = env_lock();
        let _no = ScopedEnv::set("NO_COLOR", "1");
        let long = "x".repeat(80);
        let (e, registry) = lift(serde_json::json!({"weightKg": 30, "reps": long}));
        for width in [40, 30, 26] {
            let out = table(std::slice::from_ref(&e), &registry, Some(width));
            for line in out.lines() {
                assert!(visible_width(line) <= width, "{width}: {line:?}");
            }
            assert!(out.contains('●'), "{width}: the dot gives way last: {out}");
        }
        let out = table(&[e], &registry, Some(40));
        assert!(out.contains("● 30 x"), "{out}");
        assert!(out.contains("x…"), "{out}");
    }

    /// The keys are the terminal table's: `--json`'s columns and rows carry
    /// the dot alone, as they did.
    #[test]
    fn occur_row_keys_stay_out_of_json() {
        let _env_lock = env_lock();
        let _no = ScopedEnv::set("NO_COLOR", "1");
        let (e, registry) = lift(serde_json::json!({"weightKg": 25, "reps": 15}));
        let entries = [e];
        let columns =
            table_columns_with(&entries, None, None, Some(&registry), &TableArgs::default());
        let state = columns.iter().find(|c| c.key == "state").unwrap();
        assert_eq!(state.cells, vec!["●".to_string()]);
        let rows = tail_rows::tail_rows(&entries, None, TimeMetricCol::Hybrid, Some(&registry));
        assert_eq!(rows[0].state_body, "●");
    }
}
