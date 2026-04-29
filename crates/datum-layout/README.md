# datum-layout

Pure layout primitives for daydumb's dayview and nowview visualizations.
The crate takes a slice of timed entries plus a config and returns a
struct with pixel-positioned blocks and axis ticks; the actual pixels
are drawn by an SVG/Canvas renderer that consumes that struct.

The split exists so the same layout math runs unchanged in:

- the CLI (`datum dayview`, `datum nowview`) writing SVG to stdout,
- the SvelteKit PWA via WASM rendering with D3,
- the future Flutter mobile app calling into the WASM build.

No I/O. No `datum-core`. No SQLite. No allocator-specific code.

## Shape

| Module          | What it owns                                                |
|-----------------|-------------------------------------------------------------|
| crate root      | [`layout_day`] — per-field rows of [`Block`]s within a fixed day window. Plus the POD types ([`LayoutEntry`], [`LayoutConfig`], [`Layout`], [`FieldRow`], [`AxisTick`]). |
| [`dayview`]     | Richer dayview pipeline ([`compute_dayview`](dayview::compute_dayview)) used by the SvelteKit dayview view. |
| [`nowview`]     | [`compute_nowview`](nowview::compute_nowview) — current state strip used by `datum nowview`. |
| [`dayview_svg`] | SVG renderer for the dayview output. Convenience for the CLI. |
| [`nowview_svg`] | SVG renderer for the nowview output. |
| [`svg`]         | Lower-level SVG primitives shared by the two renderers. |

The crate-root [`LayoutEntry`] and [`LayoutConfig`] are the only types
external callers need to construct; everything else is derived.

## Renderer contract

A renderer consumes a `Layout` and never re-does math:

- pixel positions are already computed,
- per-field row order is already decided,
- hourly axis ticks are already placed,
- block fills/labels come from the entry's `state`.

If you find yourself recomputing time-to-pixel mappings inside a
renderer, add the helper here instead — that is the rule that keeps
the SVG/D3/Flutter views consistent.

## WASM constraints

- No `std::time` (callers pass epoch-millis).
- No `chrono` in the public surface (the crate is intentionally
  timezone-agnostic; consumers convert before calling).
- No file I/O, no panics on user-controlled input — return empty
  layouts on degenerate windows.
- The `wasm` feature is reserved for future `wasm-bindgen` exports.
  The pure functions already build under `wasm32-unknown-unknown`.

## Where to look next

- `docs/visual-parity-analysis.md` for the visual targets the layout
  reproduces from the original datum.
- `docs/parity-audit/views.md` for the per-view compatibility map.
- [`datum-commands`](../datum-commands/) (`commands::dayview`,
  `commands::nowview`) for the CLI integration.
