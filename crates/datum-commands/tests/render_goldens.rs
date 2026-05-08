//! Render-output goldens for canonical fixtures (da-8qwy).
//!
//! Three surfaces are pinned:
//!
//! 1. **Terminal** — color-on output of [`format_info_line`] for the 20
//!    canonical fixtures from `docs/visual-parity-analysis.md` §11. The
//!    existing [`render_fixtures`] tests assert on individual ANSI-stripped
//!    fragments; these goldens lock the *full* colored ANSI string so a
//!    silent layout drift (escape sequence change, glyph swap, padding
//!    regression) shows up as a snapshot diff. da-glb / da-zxs / da-7r4 /
//!    da-25l / da-yiy were all visual fixes caught by eye — pinning the
//!    output prevents recurrence.
//!
//! 2. **SVG** — full-document strings produced by the dayview / nowview
//!    render pipeline for a small set of deterministic scenes. Byte-equal
//!    comparison; the layout pipeline is pure so identical inputs yield
//!    identical SVG forever.
//!
//! 3. **PNG** — the same SVG scenes rasterized via resvg + tiny-skia. PNG
//!    bytes are not byte-compared (anti-aliasing rounding varies with
//!    rounding mode and the available font set); instead, the comparator
//!    decodes both pixmaps and computes the average L1 distance per RGBA
//!    channel. A small tolerance covers anti-aliasing variance without
//!    masking actual visual regressions.
//!
//! ## Update mode
//!
//! Run with `DATUM_UPDATE_GOLDENS=1` to regenerate every snapshot. The
//! comparator writes the actual output to the golden path and *passes* —
//! commit the diff after a sanity-check.
//!
//! ```bash
//! DATUM_UPDATE_GOLDENS=1 cargo test -p datum-commands --test render_goldens
//! ```

use chrono::{DateTime, TimeZone, Utc};
use datum_commands::render::{
    action::{format_info_line_at, RenderContext},
    colors::strip_ansi,
};
use datum_core::{
    entry::{Entry, Verb},
    time::DatumTime,
};
use datum_layout::{
    dayview::{
        compute_dayview, BlockFill, DayviewBlock, DayviewConfig, DayviewDayInput,
        DayviewFieldInput, DayviewPoint, Orientation,
    },
    dayview_svg::render_dayview_svg,
    nowview::{nowview_axis_ticks, NowviewPanelEntry},
    nowview_svg::render_nowview_svg,
};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// ─── Env scope (shared with render_fixtures.rs by convention) ────────────────

static ENV_LOCK: Mutex<()> = Mutex::new(());

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

fn color_scope() -> (std::sync::MutexGuard<'static, ()>, ScopedEnv, ScopedEnv) {
    let g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let d = ScopedEnv::set("DATUM_COLOR", Some("1"));
    let n = ScopedEnv::set("NO_COLOR", None);
    (g, d, n)
}

// ─── Time + entry construction ───────────────────────────────────────────────

/// Pinned wall-clock for the terminal fixtures — matches the timestamps in
/// `render_fixtures.rs`. Fixed so the relative-day prefix is deterministic.
fn fixed_time(h: u32, m: u32, s: u32) -> DatumTime {
    let utc: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 4, 22, h, m, s).unwrap();
    DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() }
}

/// Pinned future timestamp (always 2099) — drives the "future-time
/// underline" branch of the timestamp formatter without depending on
/// `Utc::now()`.
fn future_time(h: u32, m: u32) -> DatumTime {
    let utc: DateTime<Utc> = Utc.with_ymd_and_hms(2099, 1, 1, h, m, 0).unwrap();
    DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() }
}

/// Pinned "now" reference for the terminal fixtures. Same calendar day as
/// `fixed_time`, so the relative-day prefix collapses to None for all
/// past-day fixtures and `future_time` reliably triggers the future branch.
fn fixed_now() -> DatumTime {
    let utc: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 4, 22, 23, 0, 0).unwrap();
    DatumTime { utc, offset_minutes: 0, timezone: "UTC".to_string() }
}

fn entry(
    field: &str,
    verb: Verb,
    value: Option<serde_json::Value>,
    start: Option<DatumTime>,
    end: Option<DatumTime>,
) -> Entry {
    Entry::new_legacy(
        field.to_string(),
        verb,
        value,
        None,
        None,
        start,
        end,
        "test-device".to_string(),
    )
}

fn ctx_with(
    lead: Option<serde_json::Value>,
    trail: Option<serde_json::Value>,
    is_first: bool,
) -> RenderContext {
    RenderContext {
        lead_state: lead,
        trail_state: trail,
        is_first_occurrence: is_first,
        ongoing_state: None,
    }
}

// ─── Canonical fixture set (the 20 from §11) ─────────────────────────────────

struct TerminalFixture {
    name: &'static str,
    entry: Entry,
    ctx: RenderContext,
}

fn all_fixtures() -> Vec<TerminalFixture> {
    let mut v = Vec::new();
    let mut push = |name: &'static str, entry: Entry, ctx: RenderContext| {
        v.push(TerminalFixture { name, entry, ctx });
    };

    push(
        "fx01_add_new_occur",
        entry("new_occur", Verb::Point, None, Some(fixed_time(16, 58, 47)), None),
        ctx_with(None, None, true),
    );
    push(
        "fx02_occur_first",
        entry("new_occur", Verb::Point, None, Some(fixed_time(16, 58, 56)), None),
        ctx_with(None, None, true),
    );
    push(
        "fx03_occur_again",
        entry("new_occur", Verb::Point, None, Some(fixed_time(16, 58, 59)), None),
        ctx_with(None, None, false),
    );
    push(
        "fx04_start_first",
        entry(
            "new_start",
            Verb::Switch,
            Some(json!(true)),
            Some(fixed_time(16, 59, 40)),
            None,
        ),
        ctx_with(None, Some(json!(true)), true),
    );
    push(
        "fx05_end",
        entry(
            "new_start",
            Verb::Switch,
            Some(json!(false)),
            Some(fixed_time(16, 59, 46)),
            None,
        ),
        ctx_with(Some(json!(true)), Some(json!(false)), false),
    );
    push(
        "fx06_start_after_end",
        entry(
            "new_start",
            Verb::Switch,
            Some(json!(true)),
            Some(fixed_time(16, 59, 52)),
            None,
        ),
        ctx_with(Some(json!(false)), Some(json!(true)), false),
    );
    push(
        "fx07_switch_first",
        entry(
            "new_switch",
            Verb::Switch,
            Some(json!("state1")),
            Some(fixed_time(17, 0, 21)),
            None,
        ),
        ctx_with(None, Some(json!("state1")), true),
    );
    push(
        "fx08_switch_prior_scalar",
        entry(
            "new_switch",
            Verb::Switch,
            Some(json!("state2")),
            Some(fixed_time(17, 0, 30)),
            None,
        ),
        ctx_with(Some(json!("state1")), Some(json!("state2")), false),
    );
    push(
        "fx09_switch_chain",
        entry(
            "new_switch",
            Verb::Switch,
            Some(json!("state3")),
            Some(fixed_time(17, 0, 38)),
            None,
        ),
        ctx_with(Some(json!("state2")), Some(json!("state3")), false),
    );
    push(
        "fx10_switch_to_repeat_label",
        entry(
            "new_switch",
            Verb::Switch,
            Some(json!("repeat")),
            Some(fixed_time(17, 0, 52)),
            None,
        ),
        ctx_with(Some(json!("state3")), Some(json!("repeat")), false),
    );
    push(
        "fx11_switch_same_state_repeat",
        entry(
            "new_switch",
            Verb::Switch,
            Some(json!("repeat")),
            Some(fixed_time(17, 0, 55)),
            None,
        ),
        ctx_with(Some(json!("repeat")), Some(json!("repeat")), false),
    );
    push(
        "fx12_end_switch",
        entry(
            "new_switch",
            Verb::Switch,
            Some(json!(false)),
            Some(fixed_time(17, 1, 13)),
            None,
        ),
        ctx_with(Some(json!("repeat")), Some(json!(false)), false),
    );
    push(
        "fx13_start_already_active",
        entry(
            "repeat_start",
            Verb::Switch,
            None,
            Some(fixed_time(17, 2, 0)),
            None,
        ),
        ctx_with(Some(json!(true)), Some(json!(true)), false),
    );
    push(
        "fx14_switch_with_dur_first",
        entry(
            "dur_blocks",
            Verb::Switch,
            Some(json!("state1")),
            Some(fixed_time(17, 2, 26)),
            None,
        ),
        ctx_with(None, Some(json!("state1")), true),
    );
    push(
        "fx15_switch_with_duration_block",
        entry(
            "dur_blocks",
            Verb::Switch,
            Some(json!("state2")),
            Some(fixed_time(17, 10, 0)),
            Some(fixed_time(17, 14, 0)),
        ),
        ctx_with(Some(json!("state1")), Some(json!("state1")), false),
    );
    push(
        "fx16_switch_array_state",
        entry(
            "field",
            Verb::Switch,
            Some(json!(["multi1", "multi2"])),
            Some(fixed_time(17, 12, 13)),
            None,
        ),
        ctx_with(
            Some(json!("prior")),
            Some(json!(["multi1", "multi2"])),
            false,
        ),
    );
    push(
        "fx17_switch_scalar_with_array_prior_and_dur",
        entry(
            "field",
            Verb::Switch,
            Some(json!("momentary_single")),
            Some(future_time(17, 15)),
            Some(future_time(17, 16)),
        ),
        ctx_with(
            Some(json!(["multi1", "multi2"])),
            Some(json!(["multi1", "multi2"])),
            false,
        ),
    );
    push(
        "fx18_ignore_renders_grey",
        entry(
            "foo",
            Verb::Switch,
            Some(json!("IGNORED")),
            Some(fixed_time(17, 30, 0)),
            None,
        ),
        ctx_with(None, Some(json!("IGNORED")), true),
    );
    push(
        "fx19_track_false_state",
        entry(
            "foo",
            Verb::Switch,
            Some(json!(false)),
            Some(fixed_time(17, 30, 5)),
            None,
        ),
        ctx_with(None, Some(json!(false)), true),
    );
    let mut e_thing = entry("thing", Verb::Point, None, Some(fixed_time(17, 40, 0)), None);
    e_thing.data = Some(json!({ "key": "value" }));
    push(
        "fx20_add_with_keys",
        e_thing,
        ctx_with(None, None, false),
    );

    v
}

// ─── Golden infrastructure ───────────────────────────────────────────────────

fn goldens_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
}

fn update_mode() -> bool {
    matches!(
        std::env::var("DATUM_UPDATE_GOLDENS").ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

fn write_golden(path: &Path, body: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create golden parent dir");
    }
    std::fs::write(path, body).expect("write golden");
}

/// Assert that `actual` matches the byte contents of the file at `path`.
/// Under `DATUM_UPDATE_GOLDENS=1`, the file is (re)written and the assertion
/// passes — review the diff before committing.
fn assert_text_golden(path: &Path, actual: &str) {
    if update_mode() {
        write_golden(path, actual.as_bytes());
        return;
    }
    let expected = std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "missing golden {path:?}: {e}. Run with DATUM_UPDATE_GOLDENS=1 to regenerate."
        )
    });
    if expected != actual {
        // Show a brief diff context so failures are actionable.
        panic!(
            "golden mismatch at {path:?}\n--- expected ({} bytes) ---\n{}\n--- actual ({} bytes) ---\n{}\n\nRun with DATUM_UPDATE_GOLDENS=1 to regenerate.",
            expected.len(),
            expected,
            actual.len(),
            actual,
        );
    }
}

/// Same as [`assert_text_golden`] but for raw bytes (PNG, etc.) with a
/// caller-supplied comparator.
#[cfg(feature = "png")]
fn assert_bytes_golden(
    path: &Path,
    actual: &[u8],
    compare: impl FnOnce(&[u8], &[u8]) -> Result<(), String>,
) {
    if update_mode() {
        write_golden(path, actual);
        return;
    }
    let expected = std::fs::read(path).unwrap_or_else(|e| {
        panic!(
            "missing golden {path:?}: {e}. Run with DATUM_UPDATE_GOLDENS=1 to regenerate."
        )
    });
    if let Err(msg) = compare(&expected, actual) {
        panic!("golden mismatch at {path:?}: {msg}\n\nRun with DATUM_UPDATE_GOLDENS=1 to regenerate.");
    }
}

/// Decode a PNG into a tiny-skia [`Pixmap`] (RGBA premul). Used by the PNG
/// comparator.
#[cfg(feature = "png")]
fn decode_png(bytes: &[u8]) -> Result<resvg::tiny_skia::Pixmap, String> {
    resvg::tiny_skia::Pixmap::decode_png(bytes).map_err(|e| format!("decode PNG: {e}"))
}

/// Compare two PNGs at the pixel level, with tolerance for anti-aliasing
/// variance. Returns `Ok` when:
/// - dimensions match exactly, AND
/// - the average L1 distance per RGBA channel is ≤ `avg_channel_tolerance`,
///   AND no individual pixel exceeds `max_pixel_tolerance` in summed L1.
///
/// `avg_channel_tolerance = 1.0` means an average channel deviation of 1
/// (out of 255) — comfortably absorbs sub-pixel rasterizer rounding while
/// still flagging large color or layout shifts.
#[cfg(feature = "png")]
fn compare_png_with_tolerance(
    expected: &[u8],
    actual: &[u8],
    avg_channel_tolerance: f64,
    max_pixel_tolerance: u32,
) -> Result<(), String> {
    let exp = decode_png(expected)?;
    let act = decode_png(actual)?;
    if exp.width() != act.width() || exp.height() != act.height() {
        return Err(format!(
            "dimensions differ: expected {}x{}, actual {}x{}",
            exp.width(),
            exp.height(),
            act.width(),
            act.height(),
        ));
    }
    let exp_data = exp.data();
    let act_data = act.data();
    if exp_data.len() != act_data.len() {
        return Err(format!(
            "byte length mismatch: {} vs {}",
            exp_data.len(),
            act_data.len()
        ));
    }
    let mut total_abs_diff: u64 = 0;
    let mut worst_pixel: u32 = 0;
    let mut worst_idx: usize = 0;
    let pixels = (exp.width() as usize) * (exp.height() as usize);
    for px in 0..pixels {
        let i = px * 4;
        let mut pixel_diff: u32 = 0;
        for k in 0..4 {
            let d = (exp_data[i + k] as i32 - act_data[i + k] as i32).unsigned_abs();
            total_abs_diff += d as u64;
            pixel_diff += d;
        }
        if pixel_diff > worst_pixel {
            worst_pixel = pixel_diff;
            worst_idx = px;
        }
    }
    let avg_channel = total_abs_diff as f64 / (pixels as f64 * 4.0);
    if avg_channel > avg_channel_tolerance {
        return Err(format!(
            "average channel diff {avg_channel:.3} exceeds tolerance {avg_channel_tolerance:.3} ({} pixels)",
            pixels
        ));
    }
    if worst_pixel > max_pixel_tolerance {
        let x = worst_idx % exp.width() as usize;
        let y = worst_idx / exp.width() as usize;
        return Err(format!(
            "worst-pixel L1 sum {worst_pixel} at ({x}, {y}) exceeds max-pixel tolerance {max_pixel_tolerance} (avg {avg_channel:.3})"
        ));
    }
    Ok(())
}

// ─── Pure-Rust SVG → PNG (mirrors commands/{dayview,nowview}::rasterize_to_png) ───

/// Rasterize an SVG string to a deterministic PNG byte buffer.
///
/// Tests run in a single binary, so font fallback resolution happens once
/// and is stable across all PNG goldens in this file. Callers should keep
/// SVG inputs free of fancy text content (or rely on the tolerance) so
/// goldens survive cross-machine font variance.
#[cfg(feature = "png")]
fn rasterize_to_png(svg_xml: &str) -> Vec<u8> {
    let mut opts = resvg::usvg::Options::default();
    {
        let fontdb = opts.fontdb_mut();
        fontdb.load_system_fonts();
        let fallback = fontdb.faces().next().and_then(|face| {
            face.families
                .iter()
                .find(|(_, lang)| {
                    *lang == resvg::usvg::fontdb::Language::English_UnitedStates
                })
                .or_else(|| face.families.first())
                .map(|(name, _)| name.clone())
        });
        if let Some(name) = fallback {
            fontdb.set_serif_family(name.clone());
            fontdb.set_sans_serif_family(name.clone());
            fontdb.set_monospace_family(name.clone());
            fontdb.set_cursive_family(name.clone());
            fontdb.set_fantasy_family(name.clone());
            opts.font_family = name;
        }
    }
    let tree = resvg::usvg::Tree::from_str(svg_xml, &opts).expect("parse SVG");
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .expect("allocate pixmap");
    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap.encode_png().expect("encode PNG")
}

// ─── Terminal goldens ────────────────────────────────────────────────────────

/// Pin the colored ANSI output of every canonical fixture. Failure means
/// either intentional renderer change (regenerate with `DATUM_UPDATE_GOLDENS=1`
/// after eyeballing the diff) or a regression like da-glb / da-25l / da-yiy.
#[test]
fn terminal_goldens_per_fixture() {
    let _g = color_scope();
    let now = fixed_now();
    let dir = goldens_root().join("terminal");
    for fx in all_fixtures() {
        let rendered = format_info_line_at(&fx.entry, None, &fx.ctx, &now);
        // Sanity: the renderer must always emit at least one ANSI escape
        // when colors are forced on. If the colored byte slice is empty,
        // the goldens are testing nothing useful.
        assert!(
            rendered.contains('\x1b'),
            "fixture {} rendered without ANSI escapes — color_scope is broken",
            fx.name,
        );
        let path = dir.join(format!("{}.ansi", fx.name));
        assert_text_golden(&path, &rendered);

        // Also pin the ANSI-stripped form for human-readable diffs.
        let stripped = strip_ansi(&rendered);
        let path = dir.join(format!("{}.txt", fx.name));
        assert_text_golden(&path, &stripped);
    }
}

// ─── Canonical SVG / PNG scenes ──────────────────────────────────────────────

/// Pinned epoch-ms anchors so SVG/PNG goldens are reproducible. Midnight
/// 2026-04-22 UTC is `1745280000000`. Hour-aligned for convenient pixel
/// math in any future debug output.
const DAY_START_MS: i64 = 1_745_280_000_000;
const HOUR_MS: i64 = 60 * 60 * 1000;
const MIN_MS: i64 = 60 * 1000;

fn dayview_scene_empty() -> DayviewLayoutResult {
    let cfg = DayviewConfig {
        days: vec![DayviewDayInput {
            label_lines: vec!["Wed".into(), "Apr 22".into(), "2026".into()],
            start_ms: DAY_START_MS,
            end_ms: DAY_START_MS + 24 * HOUR_MS,
            has_error: false,
            fields: vec![],
        }],
        now_ms: DAY_START_MS + 24 * HOUR_MS, // past today → no now-line
        width: 800.0,
        day_height: 80.0,
        time_axis_height: 12.0,
        margin: 2.0,
        ..DayviewConfig::default()
    };
    let layout = compute_dayview(&cfg);
    DayviewLayoutResult { svg: render_dayview_svg(&layout) }
}

fn dayview_scene_single_block() -> DayviewLayoutResult {
    // One field, one block 09:00-11:00, plus a point at 14:30. No now-line
    // (now is set past today's window).
    let day = DayviewDayInput {
        label_lines: vec!["Wed".into(), "Apr 22".into(), "2026".into()],
        start_ms: DAY_START_MS,
        end_ms: DAY_START_MS + 24 * HOUR_MS,
        has_error: false,
        fields: vec![DayviewFieldInput {
            field: "mood".into(),
            y_frac: 0.0,
            height_frac: 1.0,
            z_index: 0.0,
            blocks: vec![DayviewBlock {
                start_ms: DAY_START_MS + 9 * HOUR_MS,
                end_ms: DAY_START_MS + 11 * HOUR_MS,
                fill: BlockFill::Single("#4488cc".into()),
                state_label: Some("focused".into()),
            }],
            points: vec![DayviewPoint {
                time_ms: DAY_START_MS + 14 * HOUR_MS + 30 * MIN_MS,
                fg_color: "#cc4488".into(),
                bg_color: "#000000".into(),
                state_label: Some("coffee".into()),
            }],
        }],
    };
    let cfg = DayviewConfig {
        days: vec![day],
        now_ms: DAY_START_MS + 25 * HOUR_MS,
        width: 800.0,
        day_height: 80.0,
        time_axis_height: 12.0,
        margin: 2.0,
        ..DayviewConfig::default()
    };
    let layout = compute_dayview(&cfg);
    DayviewLayoutResult { svg: render_dayview_svg(&layout) }
}

fn dayview_scene_multi_state() -> DayviewLayoutResult {
    // Multi-state stripes: an array of two states renders as diagonal
    // stripes per da-7r4. Pin the SVG so the stripe geometry doesn't drift.
    let day = DayviewDayInput {
        label_lines: vec!["Wed".into(), "Apr 22".into(), "2026".into()],
        start_ms: DAY_START_MS,
        end_ms: DAY_START_MS + 24 * HOUR_MS,
        has_error: false,
        fields: vec![DayviewFieldInput {
            field: "tasks".into(),
            y_frac: 0.0,
            height_frac: 1.0,
            z_index: 0.0,
            blocks: vec![
                DayviewBlock {
                    start_ms: DAY_START_MS + 8 * HOUR_MS,
                    end_ms: DAY_START_MS + 10 * HOUR_MS,
                    fill: BlockFill::Single("#cc8844".into()),
                    state_label: Some("solo".into()),
                },
                DayviewBlock {
                    start_ms: DAY_START_MS + 13 * HOUR_MS,
                    end_ms: DAY_START_MS + 15 * HOUR_MS,
                    fill: BlockFill::MultiState(vec!["#aa3333".into(), "#33aa33".into()]),
                    state_label: Some("multi".into()),
                },
            ],
            points: vec![],
        }],
    };
    let cfg = DayviewConfig {
        days: vec![day],
        now_ms: DAY_START_MS + 25 * HOUR_MS,
        width: 800.0,
        day_height: 80.0,
        time_axis_height: 12.0,
        margin: 2.0,
        ..DayviewConfig::default()
    };
    let layout = compute_dayview(&cfg);
    DayviewLayoutResult { svg: render_dayview_svg(&layout) }
}

fn dayview_scene_today_now_line() -> DayviewLayoutResult {
    // da-zxs: the state band on today's row should stop at the now-line.
    // We model that by clipping the block's end_ms to now_ms in the input;
    // the renderer paints up to that point and then drops the yellow
    // dashed now-line on top.
    let now_ms = DAY_START_MS + 14 * HOUR_MS + 30 * MIN_MS;
    let day = DayviewDayInput {
        label_lines: vec!["Wed".into(), "Apr 22".into(), "2026".into()],
        start_ms: DAY_START_MS,
        end_ms: DAY_START_MS + 24 * HOUR_MS,
        has_error: false,
        fields: vec![DayviewFieldInput {
            field: "mood".into(),
            y_frac: 0.0,
            height_frac: 1.0,
            z_index: 0.0,
            blocks: vec![DayviewBlock {
                start_ms: DAY_START_MS + 9 * HOUR_MS,
                // Active block: ends at "now", not 24h.
                end_ms: now_ms,
                fill: BlockFill::Single("#4488cc".into()),
                state_label: Some("focused".into()),
            }],
            points: vec![],
        }],
    };
    let cfg = DayviewConfig {
        days: vec![day],
        now_ms,
        width: 800.0,
        day_height: 80.0,
        time_axis_height: 12.0,
        margin: 2.0,
        ..DayviewConfig::default()
    };
    let layout = compute_dayview(&cfg);
    DayviewLayoutResult { svg: render_dayview_svg(&layout) }
}

fn nowview_scene_30min() -> DayviewLayoutResult {
    // 30-minute history window with a single ongoing switch and one point
    // at -10m. Rendered through nowview's overlay (HH:MM clock + state
    // panel).
    let history_minutes = 30.0;
    let end_ms = DAY_START_MS + 14 * HOUR_MS + 30 * MIN_MS;
    let start_ms = end_ms - (history_minutes * 60_000.0) as i64;

    let panel_width: f64 = 120.0;
    let width: f64 = 520.0;
    let plot_width: f64 = (width - 4.0).max(0.0);
    let time_axis_extent_for_ticks = (plot_width - panel_width).max(0.0);
    let axis_ticks_override =
        Some(nowview_axis_ticks(time_axis_extent_for_ticks, history_minutes));

    let day = DayviewDayInput {
        label_lines: vec![],
        start_ms,
        end_ms,
        has_error: false,
        fields: vec![DayviewFieldInput {
            field: "mood".into(),
            y_frac: 0.0,
            height_frac: 1.0,
            z_index: 0.0,
            blocks: vec![DayviewBlock {
                start_ms: end_ms - 25 * MIN_MS,
                end_ms,
                fill: BlockFill::Single("#4488cc".into()),
                state_label: Some("focused".into()),
            }],
            points: vec![DayviewPoint {
                time_ms: end_ms - 10 * MIN_MS,
                fg_color: "#cc4488".into(),
                bg_color: "#000000".into(),
                state_label: Some("coffee".into()),
            }],
        }],
    };
    let cfg = DayviewConfig {
        days: vec![day],
        now_ms: end_ms,
        width,
        height: None,
        day_height: 250.0,
        interday_margin: 0.0,
        time_axis_height: 12.0,
        margin: 2.0,
        label_width: Some(0.0),
        orientation: Orientation::Horizontal,
        axis_ticks_override,
        right_panel_width: panel_width,
    };
    let layout = compute_dayview(&cfg);
    let panel_entries = vec![NowviewPanelEntry {
        field: "mood".into(),
        state_label: Some("focused".into()),
        colors: vec!["#4488cc".into()],
    }];
    let svg = render_nowview_svg(&layout, "14:30", &panel_entries);
    DayviewLayoutResult { svg }
}

struct DayviewLayoutResult {
    svg: String,
}

fn svg_scenes() -> Vec<(&'static str, DayviewLayoutResult)> {
    vec![
        ("dayview_empty", dayview_scene_empty()),
        ("dayview_single_block", dayview_scene_single_block()),
        ("dayview_multi_state", dayview_scene_multi_state()),
        ("dayview_today_now_line", dayview_scene_today_now_line()),
        ("nowview_30min", nowview_scene_30min()),
    ]
}

#[test]
fn svg_goldens_per_scene() {
    let dir = goldens_root().join("svg");
    for (name, res) in svg_scenes() {
        // Sanity: every scene must produce a non-trivial SVG document.
        assert!(res.svg.starts_with("<svg "), "{name}: missing <svg> root");
        assert!(res.svg.ends_with("</svg>"), "{name}: not <svg>-terminated");
        let path = dir.join(format!("{name}.svg"));
        assert_text_golden(&path, &res.svg);
    }
}

#[cfg(feature = "png")]
#[test]
fn png_goldens_per_scene() {
    let dir = goldens_root().join("png");
    // Anti-aliasing tolerance: average channel difference per RGBA channel.
    // 1.0/255 = ~0.4% per channel — generous enough for resvg's sub-pixel
    // rounding, tight enough to flag actual color shifts.
    let avg_tol = 1.0;
    // Worst-case single-pixel: 4 channels × 32 = 128. Catches isolated
    // off-by-one rasterizer pixels at edges without flagging them as
    // regressions.
    let max_pixel_tol = 128;
    for (name, res) in svg_scenes() {
        let png = rasterize_to_png(&res.svg);
        let path = dir.join(format!("{name}.png"));
        assert_bytes_golden(&path, &png, |expected, actual| {
            compare_png_with_tolerance(expected, actual, avg_tol, max_pixel_tol)
        });
    }
}

// ─── Sanity: the comparator itself ───────────────────────────────────────────

#[cfg(feature = "png")]
#[test]
fn png_comparator_accepts_identical_input() {
    let scene = dayview_scene_single_block();
    let a = rasterize_to_png(&scene.svg);
    let b = rasterize_to_png(&scene.svg);
    assert!(compare_png_with_tolerance(&a, &b, 0.0, 0).is_ok(),
        "identical PNGs must compare equal under any tolerance");
}

#[cfg(feature = "png")]
#[test]
fn png_comparator_rejects_size_mismatch() {
    use resvg::tiny_skia::Pixmap;
    let mut a = Pixmap::new(10, 10).unwrap();
    a.fill(resvg::tiny_skia::Color::WHITE);
    let mut b = Pixmap::new(20, 10).unwrap();
    b.fill(resvg::tiny_skia::Color::WHITE);
    let result = compare_png_with_tolerance(
        &a.encode_png().unwrap(),
        &b.encode_png().unwrap(),
        100.0,
        u32::MAX,
    );
    assert!(matches!(result, Err(ref msg) if msg.contains("dimensions")));
}

#[cfg(feature = "png")]
#[test]
fn png_comparator_rejects_color_shift_above_tolerance() {
    use resvg::tiny_skia::{Color, Pixmap};
    let mut a = Pixmap::new(8, 8).unwrap();
    a.fill(Color::from_rgba8(0, 0, 0, 255));
    let mut b = Pixmap::new(8, 8).unwrap();
    // Solid 100/100/100 vs 0/0/0 → average channel diff = 75 (only RGB; A
    // matches) — well above tolerance 1.0.
    b.fill(Color::from_rgba8(100, 100, 100, 255));
    let result = compare_png_with_tolerance(
        &a.encode_png().unwrap(),
        &b.encode_png().unwrap(),
        1.0,
        128,
    );
    assert!(result.is_err(), "100-unit color shift must fail tolerance");
}
