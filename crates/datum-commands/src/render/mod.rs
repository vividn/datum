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
/// Prefix marker for the first write to a previously-unseen field.
/// Deliberately a printable glyph (not an ANSI escape) so it survives
/// `strip_ansi` and is visible in pipes — the user sees they just
/// introduced a new field.
const FIRST_TIME_MARKER: &str = "✨ ";
/// Mid-run separator between the prior and new state blocks.
const TRANSITION_ARROW: &str = "▶";

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

/// Extra, caller-supplied context for rendering a single write.
///
/// The default renderer works purely from [`Entry`] + [`SpecRegistry`], which
/// is enough for table views and backfill tools. For interactive writes the
/// command layer can enrich the output with information that isn't on the
/// entry itself:
///
/// * `last_state` — the field's prior active state, so the state block can
///   render a `[prior ▶ next]` transition indicator matching original
///   datum's `formatState` three-fragment motif.
/// * `is_first_occurrence` — true when daydumb has never seen this field
///   before; the renderer prepends [`FIRST_TIME_MARKER`] to make new-field
///   writes visually distinctive.
#[derive(Clone, Debug, Default)]
pub struct RenderContext {
    pub last_state: Option<JsonValue>,
    pub is_first_occurrence: bool,
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

// ─── Truecolor palette ────────────────────────────────────────────────────────
//
// Original datum renders field and state colors primarily as **backgrounds**
// (`chalk.bgHex(...)`) with a contrast-aware foreground. Porting that look
// requires RGB triples, not the ANSI-256 fallback we used previously. The
// helpers below resolve spec colors (hex) or derive a stable fallback from
// `sha256(name)`, then emit truecolor 48/38;2;R;G;B escape pairs.

/// RGB color triple.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

impl Rgb {
    const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };
    const WHITE: Rgb = Rgb { r: 255, g: 255, b: 255 };

    fn fg(self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.r, self.g, self.b)
    }
    fn bg(self) -> String {
        format!("\x1b[48;2;{};{};{}m", self.r, self.g, self.b)
    }
}

/// YIQ perceived brightness, matching datum's `getColorBrightness`.
fn brightness(rgb: Rgb) -> u32 {
    (rgb.r as u32 * 299 + rgb.g as u32 * 587 + rgb.b as u32 * 114) / 1000
}

/// Pick black or white to contrast against `bg`, matching datum's
/// `getContrastTextColor` default path.
fn contrast_fg(bg: Rgb) -> Rgb {
    if brightness(bg) > 128 { Rgb::BLACK } else { Rgb::WHITE }
}

fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    Some(Rgb {
        r: u8::from_str_radix(&s[0..2], 16).ok()?,
        g: u8::from_str_radix(&s[2..4], 16).ok()?,
        b: u8::from_str_radix(&s[4..6], 16).ok()?,
    })
}

/// ANSI-256 cube index → approximate RGB. Used only when a spec's `color`
/// was stored as a legacy ANSI-256 number instead of a hex string.
fn ansi256_rgb(n: u8) -> Rgb {
    // Named colors 0..15 aren't well-defined in terminal standards; fall back
    // to a mid-grey to keep contrast checks sensible.
    if n < 16 {
        return Rgb { r: 128, g: 128, b: 128 };
    }
    if n >= 232 {
        let v = 8 + (n - 232) * 10;
        return Rgb { r: v, g: v, b: v };
    }
    let idx = n - 16;
    let r = idx / 36;
    let g = (idx % 36) / 6;
    let b = idx % 6;
    let q = |c: u8| -> u8 { if c == 0 { 0 } else { 55 + 40 * c } };
    Rgb { r: q(r), g: q(g), b: q(b) }
}

/// Derive a stable RGB color from `sha256(name)`. Mirrors datum's
/// `md5Color` intent with a more widely-available hash. Low-luminosity
/// results get nudged brighter so the color remains visible on dark terms.
fn hash_to_rgb(name: &str) -> Rgb {
    let hash = Sha256::digest(name.as_bytes());
    let mut rgb = Rgb { r: hash[0], g: hash[1], b: hash[2] };
    if brightness(rgb) < 40 {
        // One channel gets boosted high; which one is stable per name.
        match hash[3] % 3 {
            0 => rgb.r = rgb.r.max(180),
            1 => rgb.g = rgb.g.max(180),
            _ => rgb.b = rgb.b.max(180),
        }
    }
    rgb
}

/// Resolve a stored color string (hex / ANSI-256 number) or fall back to a
/// hash derived from `fallback_name`. Returns an RGB triple suitable for
/// both foreground and background escape emission.
fn resolve_rgb(stored: Option<&str>, fallback_name: &str) -> Rgb {
    if let Some(s) = stored {
        if let Some(rgb) = parse_hex(s) {
            return rgb;
        }
        if let Ok(n) = s.trim_start_matches('#').parse::<u8>() {
            return ansi256_rgb(n);
        }
    }
    hash_to_rgb(fallback_name)
}

/// Paint `body` with `bg` color + contrast-derived fg. The "background is
/// primary" style datum uses for field/state blocks.
fn paint_bg(bg: Rgb, body: &str) -> String {
    if !color_enabled() {
        return body.to_string();
    }
    let fg = contrast_fg(bg);
    format!("{}{}{}{}", bg.bg(), fg.fg(), body, RESET)
}

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
    let spec = registry.and_then(|r| r.fields.get(&entry.field));
    let action_text = action.map(paint_action);
    let slug_text = Some(paint(GRAY_FG, &entry.slug));
    let hid_text = Some(paint(
        action.map(Action::ansi_color).unwrap_or(""),
        &short_hid(&entry.hid),
    ));

    let occur = entry.start_time.as_ref().map(|t| format_human_time(t.utc, false));
    let create = Some(format_human_time(entry.create_time, true));
    let modify = Some(format_human_time(entry.modify_time, true));
    let hybrid = occur
        .clone()
        .or_else(|| Some(format_gray_prefix("c", format_human_time(entry.create_time, false))));

    let first = ctx.map(|c| c.is_first_occurrence).unwrap_or(false);
    Extracted {
        action: action_text,
        slug: slug_text,
        hid: hid_text,
        time_occur: occur,
        time_create: create,
        time_modify: modify,
        time_hybrid: hybrid,
        field: Some(format_field(&entry.field, spec, first)),
        state: format_state(entry, spec, ctx),
        dur: entry.duration_seconds().map(format_duration),
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
        return format!(" {} ", action.name());
    }
    // Inverse + color: ` CREATE ` with green-inverse background.
    let code = action.ansi_color();
    format!("{INVERSE}{code} {} {RESET}", action.name())
}

// ─── Field / state colors ────────────────────────────────────────────────────

fn format_field(field: &str, spec: Option<&FieldSpec>, first_time: bool) -> String {
    let rgb = field_rgb(field, spec);
    let painted = paint_bg(rgb, field);
    if first_time {
        format!("{FIRST_TIME_MARKER}{painted}")
    } else {
        painted
    }
}

/// Resolve field color as RGB: stored spec color → hash-derived fallback.
fn field_rgb(field: &str, spec: Option<&FieldSpec>) -> Rgb {
    resolve_rgb(spec.and_then(|s| s.color.as_deref()), field)
}

/// Resolve state color as RGB: per-state override → field color → hash.
fn state_rgb(field: &str, state: &str, spec: Option<&FieldSpec>) -> Rgb {
    if let Some(s) = spec {
        if let Some(c) = s.state_colors.get(state) {
            return resolve_rgb(Some(c.as_str()), state);
        }
        if let Some(c) = s.color.as_deref() {
            return resolve_rgb(Some(c), field);
        }
    }
    hash_to_rgb(state)
}

/// A discrete "state piece" the renderer can emit — either a point glyph,
/// a plain word (no color), or a padded bg-color block matching datum's
/// `stateChalk`.
enum StatePiece {
    /// Foreground-only glyph in the given color. Used for `●` (point occur)
    /// and `¢` (non-occur note) — matches datum's `chalk.hex(stateColor)(DOT)`.
    Glyph { text: String, color: Rgb },
    /// Padded background block: ` text ` on `color`, with contrast fg.
    /// The visual workhorse — fields, start, strings, null, etc.
    Block { text: String, color: Rgb },
    /// Plain text, no ANSI at all (for `end` / false state).
    Plain(String),
    /// S12: concurrent states rendered as adjacent blocks separated by a
    /// comma colored like the second element. Composed of 2+ pieces.
    Multi { pieces: Vec<StatePiece>, separators: Vec<Rgb> },
}

impl StatePiece {
    fn render(&self) -> String {
        match self {
            StatePiece::Glyph { text, color } => paint(&color.fg(), text),
            StatePiece::Block { text, color } => paint_bg(*color, &format!(" {text} ")),
            StatePiece::Plain(s) => s.clone(),
            StatePiece::Multi { pieces, separators } => {
                let mut out = String::new();
                for (i, p) in pieces.iter().enumerate() {
                    if i > 0 {
                        let sep_color = separators.get(i - 1).copied().unwrap_or(Rgb::WHITE);
                        out.push_str(&paint(&sep_color.fg(), ","));
                    }
                    out.push_str(&p.render());
                }
                out
            }
        }
    }

    /// Whether this piece contributes a coloured block that pairs naturally
    /// with a transition arrow. Plain `end` and bare glyphs don't.
    fn is_block(&self) -> bool {
        matches!(self, StatePiece::Block { .. } | StatePiece::Multi { .. })
    }
}

/// Map `value` → a [`StatePiece`] according to datum's `formatState` rules.
/// Returns None when the value is absent (caller falls back to a verb-level
/// bullet).
fn piece_for_value(value: Option<&JsonValue>, field: &str, spec: Option<&FieldSpec>) -> Option<StatePiece> {
    match value? {
        JsonValue::Null => Some(StatePiece::Block {
            text: NULL_SET.to_string(),
            color: Rgb { r: 136, g: 136, b: 136 }, // #888, datum's null grey
        }),
        // `start` uses the field's colour; it's the "field is active" signal.
        JsonValue::Bool(true) => Some(StatePiece::Block {
            text: "start".to_string(),
            color: field_rgb(field, spec),
        }),
        // `end` renders plain — matches `stateChalk({state: false}) === chalk`.
        JsonValue::Bool(false) => Some(StatePiece::Plain("end".to_string())),
        JsonValue::String(s) => Some(StatePiece::Block {
            text: s.clone(),
            color: state_rgb(field, s, spec),
        }),
        // S12: multi-element arrays render as adjacent coloured blocks with
        // per-element palette, comma separators coloured like the following
        // element. Single-element arrays unfold (normalize_state already
        // flattens these in the state timeline; this mirrors that at display).
        JsonValue::Array(items) if items.len() >= 2 => {
            let pieces: Vec<StatePiece> = items
                .iter()
                .filter_map(|v| piece_for_value(Some(v), field, spec))
                .collect();
            if pieces.is_empty() {
                return None;
            }
            let separators: Vec<Rgb> = pieces
                .iter()
                .skip(1)
                .map(|p| match p {
                    StatePiece::Block { color, .. } => *color,
                    StatePiece::Glyph { color, .. } => *color,
                    _ => field_rgb(field, spec),
                })
                .collect();
            if pieces.len() == 1 {
                pieces.into_iter().next()
            } else {
                Some(StatePiece::Multi { pieces, separators })
            }
        }
        JsonValue::Array(items) if items.len() == 1 => {
            piece_for_value(Some(&items[0]), field, spec)
        }
        other => {
            let t = other.to_string();
            let c = state_rgb(field, &t, spec);
            Some(StatePiece::Block { text: t, color: c })
        }
    }
}

/// Verb-level glyph when the entry has no explicit value (e.g. bare `occur`
/// or `start`). Returns None for verbs without a default glyph.
fn piece_for_verb(entry: &Entry, spec: Option<&FieldSpec>) -> Option<StatePiece> {
    let fg = field_rgb(&entry.field, spec);
    match &entry.verb {
        // Point occur / start with no value → foreground-only bullet.
        Verb::Occur | Verb::Start => Some(StatePiece::Glyph { text: DOT.to_string(), color: fg }),
        Verb::Stop => Some(StatePiece::Plain("end".to_string())),
        // `add` with no start_time is a free-floating note.
        Verb::Add if entry.start_time.is_none() => {
            Some(StatePiece::Glyph { text: NON_OCCUR.to_string(), color: fg })
        }
        _ => None,
    }
}

/// Render the state column. When a prior state is available via
/// [`RenderContext`] or `entry.data.last_state` AND both prior and next
/// are real coloured blocks, emit datum's transition motif:
/// `<prior> ▶ <next>`. Otherwise render the single current piece.
fn format_state(
    entry: &Entry,
    spec: Option<&FieldSpec>,
    ctx: Option<&RenderContext>,
) -> Option<String> {
    let current = match piece_for_value(entry.value.as_ref(), &entry.field, spec) {
        Some(p) => p,
        None => return piece_for_verb(entry, spec).map(|p| p.render()),
    };

    // Prior state: explicit ctx override wins; otherwise peek at
    // `extra_data.last_state` (written by `stop --last-state`).
    let prior_value = ctx
        .and_then(|c| c.last_state.as_ref())
        .or_else(|| {
            entry
                .data
                .as_ref()
                .and_then(|d| d.get("last_state"))
        });

    let transition = prior_value
        .and_then(|pv| piece_for_value(Some(pv), &entry.field, spec))
        .filter(|prior| {
            // Only emit a transition when both sides are real blocks AND the
            // colors differ. Two identical coloured blocks or a plain prior
            // would be visual noise.
            prior.is_block()
                && current.is_block()
                && piece_colors_differ(prior, &current)
        });

    Some(match transition {
        Some(prior) => format!(
            "{}{TRANSITION_ARROW}{}",
            prior.render(),
            current.render()
        ),
        None => current.render(),
    })
}

fn piece_colors_differ(a: &StatePiece, b: &StatePiece) -> bool {
    match (a, b) {
        (StatePiece::Block { color: ca, .. }, StatePiece::Block { color: cb, .. }) => ca != cb,
        // Multi-state blocks always carry at least one color that differs
        // from any single block — emit the transition arrow so the user
        // sees the scalar ↔ array shift.
        (StatePiece::Multi { .. }, _) | (_, StatePiece::Multi { .. }) => true,
        _ => false,
    }
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
    /// Skip the header row. Used by `tail --watch` so tick-appended batches
    /// don't reprint headers.
    pub suppress_header: bool,
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
        suppress_header: false,
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
        "id" | "slug" => entry.slug.clone(),
        "hid" => short_hid(&entry.hid),
        "full_hid" => entry.hid.clone(),
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
    /// Pre-rendered state column — may include inline ANSI escapes, so
    /// alignment math must go through [`visible_width`] rather than `len()`.
    state_rendered: String,
    state_vw: usize,
    time_str: String,
    field_ansi: String,
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

/// S12: render the state column for `datum now`. Scalar values get the
/// original single-color treatment; arrays become a checkerboard-flavored
/// sequence of per-element blocks so concurrent states are immediately
/// distinguishable. Returns `(rendered, visible_width)` so the caller can
/// align columns with [`visible_width`].
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

    #[test]
    fn state_glyph_for_point_occur() {
        // An `occur` with no value → foreground-only bullet glyph.
        let entry = sample_entry("coffee", Verb::Occur, None);
        let state = format_state(&entry, None, None).unwrap();
        assert_eq!(strip_ansi(&state), DOT);
    }

    #[test]
    fn state_glyph_for_switch_true_false() {
        // `true` renders as a padded "start" block ( start ); `false` is
        // plain "end" (no bg). strip_ansi preserves the padding so we
        // trim the result for portable assertions.
        let entry_on = sample_entry("mood", Verb::Switch, Some(JsonValue::Bool(true)));
        let entry_off = sample_entry("mood", Verb::Switch, Some(JsonValue::Bool(false)));
        assert_eq!(strip_ansi(&format_state(&entry_on, None, None).unwrap()).trim(), "start");
        assert_eq!(strip_ansi(&format_state(&entry_off, None, None).unwrap()), "end");
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
        // Scope the env var to just this test.
        let _guard = ColorGuard::force_on();
        let entry = sample_entry("mood", Verb::Switch, Some(JsonValue::from("happy")));
        let out = format_state(&entry, None, None).unwrap();
        assert!(
            out.contains("\x1b[48;2;"),
            "expected truecolor BG escape, got: {out:?}"
        );
    }

    /// Transition motif: prior-state block + arrow + current-state block,
    /// both wrapping the state word with single-space padding per datum.
    #[test]
    fn transition_indicator_drawn_when_prior_state_differs() {
        let _guard = ColorGuard::force_on();
        let entry = sample_entry("mood", Verb::Switch, Some(JsonValue::from("happy")));
        let ctx = RenderContext {
            last_state: Some(JsonValue::from("tired")),
            is_first_occurrence: false,
        };
        let out = format_state(&entry, None, Some(&ctx)).unwrap();
        let stripped = strip_ansi(&out);
        assert!(
            stripped.contains(TRANSITION_ARROW),
            "expected transition arrow in {stripped:?}"
        );
        assert!(stripped.contains("tired"), "prior state missing: {stripped:?}");
        assert!(stripped.contains("happy"), "current state missing: {stripped:?}");
        // Prior appears before current in the composed string.
        assert!(
            stripped.find("tired").unwrap() < stripped.find("happy").unwrap(),
            "prior must render first: {stripped:?}"
        );
    }

    /// No arrow when the prior state matches the current one — the user
    /// would just see a doubled block.
    #[test]
    fn transition_suppressed_when_prior_equals_current() {
        let entry = sample_entry("mood", Verb::Switch, Some(JsonValue::from("happy")));
        let ctx = RenderContext {
            last_state: Some(JsonValue::from("happy")),
            is_first_occurrence: false,
        };
        let out = format_state(&entry, None, Some(&ctx)).unwrap();
        assert!(!strip_ansi(&out).contains(TRANSITION_ARROW));
    }

    /// `extra_data.last_state` (written by `stop --last-state`) feeds the
    /// transition when no explicit ctx override is present.
    #[test]
    fn transition_reads_last_state_from_extra_data() {
        let mut entry = sample_entry("mood", Verb::Switch, Some(JsonValue::from("happy")));
        entry.data = Some(serde_json::json!({ "last_state": "tired" }));
        let out = format_state(&entry, None, None).unwrap();
        assert!(strip_ansi(&out).contains(TRANSITION_ARROW));
    }

    /// First-time marker prepends `✨ ` to the rendered field name. The
    /// sparkle is a printable glyph (not an ANSI escape) so pipes and
    /// `strip_ansi` preserve it.
    #[test]
    fn first_time_marker_prepends_field() {
        let out_plain = format_field("newthing", None, false);
        let out_marked = format_field("newthing", None, true);
        assert!(
            !strip_ansi(&out_plain).starts_with(FIRST_TIME_MARKER),
            "plain first-time=false must NOT have sparkle: {out_plain:?}"
        );
        assert!(
            strip_ansi(&out_marked).starts_with(FIRST_TIME_MARKER),
            "first-time=true must start with sparkle: {out_marked:?}"
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
        let rgb = Rgb { r: 255, g: 0, b: 0 };
        assert_eq!(paint_bg(rgb, "hi"), "hi");
        assert_eq!(paint(&rgb.fg(), "hi"), "hi");
    }

    /// `DATUM_COLOR` overrides `NO_COLOR` — useful for the internal test
    /// matrix that wants to force colour on regardless of env.
    #[test]
    fn datum_color_overrides_no_color() {
        let _no = ScopedEnv::set("NO_COLOR", Some("1"));
        let _dc = ScopedEnv::set("DATUM_COLOR", Some("1"));
        assert!(color_enabled());
    }

    /// Contrast-fg picks black for bright backgrounds and white for dark —
    /// matches datum's `getColorBrightness` / YIQ threshold.
    #[test]
    fn contrast_fg_flips_on_brightness_boundary() {
        assert_eq!(contrast_fg(Rgb { r: 255, g: 255, b: 255 }), Rgb::BLACK);
        assert_eq!(contrast_fg(Rgb { r: 0, g: 0, b: 0 }), Rgb::WHITE);
    }

    /// Hex parser accepts both `#rrggbb` and `rrggbb`, rejects bad lengths.
    #[test]
    fn parse_hex_accepts_hash_prefix_and_rejects_garbage() {
        assert_eq!(parse_hex("#ff6600"), Some(Rgb { r: 0xff, g: 0x66, b: 0x00 }));
        assert_eq!(parse_hex("ff6600"), Some(Rgb { r: 0xff, g: 0x66, b: 0x00 }));
        assert_eq!(parse_hex("xxx"), None);
        assert_eq!(parse_hex("f60"), None);
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
