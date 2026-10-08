// What the CLI-render panes run, and how wide they claim to be — plus
// the day strip's pictures, which are the same thing one row down
// (`bandArgv` at the foot of this file).
//
// Each pane is one datum command rendered as text (da-1x72). Deliberately
// CLI renders for now: the moment a pane becomes an interactive table it
// stops being free, and that is a later, separate trade.

import { shellSplit } from './shellSplit.js';

/** One pane: a title, the command behind it, and what to say when it's quiet. */
export interface PaneSpec {
	title: string;
	argv: string[];
	/** Shown when the command succeeds with no output at all. */
	empty?: string;
	/**
	 * Which component shows the command's answer.
	 *
	 * `text` — the default — is CLI output rendered as a terminal renders
	 * it: monospace, whitespace kept, ANSI translated into spans, and an
	 * SVG document shown as a picture (`CliOutput.svelte`).
	 *
	 * `tail-table` is the one pane whose content is a grid rather than a
	 * block: `datum tail` IS a table, and a terminal draws it by padding
	 * cells to a column count nobody in a browser has measured. So that
	 * pane lays the CLI's own cells out in a real `<table>` instead
	 * (da-6ec2) — the cells, their order, their colours and the header
	 * words are all datum's; only the geometry is the browser's.
	 */
	view?: 'text' | 'tail-table';
}

/** A pane's content box, in whole pixels — but the tail table's, which is exact (da-zs3t9). */
export interface PaneBox {
	width: number;
	height: number;
}

/**
 * The argv a pane runs, or `null` when it cannot run yet.
 *
 * Whether the pane's box goes along is the argv's to say, not the pane's
 * (da-wb2n): a pane whose command the reader types can be anything. A
 * drawn view is asked for at the size it is displayed at. `datum dayview`
 * authors a 2000px canvas because that is a sensible page to write a .svg
 * to; a pane 345px wide showed it scaled to 0.17, which put the hour axis
 * on screen at 1.5px and the date stamp at 3.4 — illegible on a laptop
 * too, at 0.70 (da-m0cj).
 *
 * Such a pane waits for its box: the geometry goes across as `--width`
 * and `--height`, the CLI's own flags in the CLI's own units, so the pane
 * asks datum for a band of a given size rather than resizing datum's
 * answer. Null is "not measured yet", which lasts until the first
 * observation — the alternative is one render at the 2000px default that
 * the measurement immediately replaces.
 */
export function paneArgv(argv: string[], box: PaneBox | null): string[] | null {
	if (!wantsBox(argv)) return argv;
	if (!box || box.width <= 0 || box.height <= 0) return null;
	return [...argv, '--width', String(box.width), '--height', String(box.height)];
}

/**
 * The commands whose output must be asked for at the size it is shown at.
 *
 * Both drawn views. A command that authors its own canvas lands wherever
 * the canvas puts it: `dayview` writes 2000px, four times any pane or
 * prompt, so it arrived scaled and the text with it. nowview writes 520 —
 * near a wide pane's width, which is why it always READ correctly at
 * 1280, and why the size it was wrong at was its HEIGHT (da-45wt).
 *
 * A pane sends both, because a pane also knows its height; a line typed
 * at the prompt scrolls, so it only names a width.
 */
const SIZED_COMMANDS = new Set(['dayview', 'nowview']);

/** Whether a pane running `argv` must tell it the pane's box. */
export function wantsBox(argv: string[]): boolean {
	return SIZED_COMMANDS.has(argv[0]);
}

/**
 * The commands whose answer is a picture of NOW: they read the clock
 * inside the wasm process and draw what it says — nowview's elapsed
 * durations, dayview's now-line, `now`'s "for 25m". Run once, they show
 * the minute they were run in, so a pane running one of them re-runs on
 * the minute clock (da-r61i, `$lib/clock.ts`).
 *
 * Unless it is PINNED: `-t <instant>` centres nowview on an instant the
 * reader chose (the time cursor) and `--end-date` asks dayview for a day of
 * the past — the strip's own bands are asked for that way. `--end-date` by
 * the flag alone, not its value, so `--end-date 0` — today, spelled the long
 * way, which nothing in the app writes — is pinned too; nowview's `-t` only
 * when its value is not an offset (`-t -7` moves with the clock, da-5flt).
 */
const NOW_COMMANDS = new Set(['now', 'nowview', 'dayview']);
const PINNED = /^--end-date(=|$)/;

export function dependsOnNow(argv: string[]): boolean {
	return (
		NOW_COMMANDS.has(argv[0]) &&
		!argv.some(
			(word, i) =>
				PINNED.test(word) ||
				(argv[0] === 'nowview' && word === '-t' && !/^[+-]/.test(argv[i + 1] ?? '-'))
		)
	);
}

/**
 * Whether `argv` draws a picture: `dayview` always does, and `nowview` when
 * it is asked for its SVG on stdout (`-o -`) rather than the terminal band
 * it prints bare (da-57dj).
 */
function draws(argv: string[]): boolean {
	if (!SIZED_COMMANDS.has(argv[0])) return false;
	if (argv[0] !== 'nowview') return true;
	return argv.some((word, i) => (word === '-o' || word === '--output-file') && argv[i + 1] === '-') ||
		argv.includes('--output-file=-');
}

/**
 * Fit a typed command to the pixels its output will land in.
 *
 * `paneArgv`'s counterpart for the REPL (da-wexl). Fixing the pane left
 * this path alone — they are different components — so `dayview` typed at
 * the prompt still went through the seam bare, arrived as the 2000px
 * canvas and was scaled to 0.46 on a laptop and 0.17 on a phone: the 9px
 * hour axis at 4.1px and 1.55px.
 *
 * Width only: a pane fills a box both ways, but scrollback grows
 * downward, so the band keeps the height datum derives for it. An argv
 * that already carries `--width` is left alone — someone who names a size
 * gets that size — and an unmeasured box (`null`) falls back to datum's
 * own default rather than guessing one.
 */
export function fitArgv(argv: string[], width: number | null): string[] {
	if (!draws(argv) || width === null || width <= 0) return argv;
	if (argv.some((word) => word === '--width' || word.startsWith('--width='))) return argv;
	return [...argv, '--width', String(width)];
}

/**
 * The day strip's day height at rest, in pixels — datum's `--day-height`.
 *
 * The same value for every band, which is what makes the stack a stack
 * (da-cgyh). `label_width` in `compute_dayview` is derived from the label
 * font size, which is derived from `day_height`; two bands drawn at
 * different heights therefore start their data areas at different x, and
 * their hour gridlines would not line up. Pinning `--day-height` pins all
 * of it, so the same hour is the same column from the oldest band down to
 * today's — and the day-height handle (da-07ij) may move it as far as it
 * likes so long as it moves EVERY band together.
 *
 * 85 is what the strip has always drawn at: a 9rem pane, less its title
 * bar and padding, less the axis strip and margins datum adds — the
 * height `datum dayview --height` derived for the box the pane had.
 */
export const DEFAULT_DAY_HEIGHT = 85;

/**
 * How short and how tall the day-height handle may draw a day.
 *
 * The floor is the legibility one (da-m0cj), read off the layout's own
 * arithmetic: `compute_dayview` picks the label font as
 * `day_height / lines - 6`, capped at 20 and floored at 7, taking the most
 * lines that clear the floor — so 39 is the shortest day that still fits
 * datum's three label lines (Wed / Sep 09 / 2026), and 40 is the first
 * round number above it. Below that the date stamp starts dropping lines,
 * and the stamp is how a reader tells one band from another.
 *
 * The ceiling is a screen: 240px of one day still leaves the REPL and the
 * widget bar room on a laptop, and the pane the strip sits in is what
 * caps the strip's own height (da-zb0d).
 */
export const MIN_DAY_HEIGHT = 40;
export const MAX_DAY_HEIGHT = 240;

/**
 * The date-label column the STRIP draws itself, in pixels (da-iu9i).
 *
 * datum used to write the date stamps into the picture, which is why the
 * bands could only be stitched at one label width — `label_width` is
 * derived from the label font, which is derived from the day height, so
 * moving either moved where the data area began. The strip asks for
 * `--label-width 0` now and draws the stamps in its own column, so the
 * column's width is a SETTING like the day height: the `+` handle on the
 * divider drags it sideways.
 *
 * The floor is a fingertip, not zero: the freeze-today toggle sits at the
 * far left of the divider, inside this column, and a column narrower than
 * the toggle is a toggle under the handle.
 */
export const DEFAULT_LABEL_WIDTH = 54;
export const MIN_LABEL_WIDTH = 24;
export const MAX_LABEL_WIDTH = 200;

/**
 * The line between two days: one pixel (vividn, 2026-09-15 — "make it
 * thinner (1 px)"). It is the whole distance between them now; datum's
 * 15px interday margin belonged to a picture holding several days, and
 * the strip's picture holds one.
 */
export const DIVIDER = 1;

/**
 * The hour-axis row the strip draws under today, in pixels — datum's own
 * `--time-axis-height` default, which the bands are now asked to suppress
 * so the ruler is drawn once, by the web, from the JSON's `hours[]`.
 */
export const AXIS_HEIGHT = 15;

// ── the hour window (da-ukcf) ────────────────────────────────────────
//
// The strip draws an HOUR RANGE, and the whole day is just its widest
// setting. Every band draws the same range — they share the x-axis, and a
// stack whose days were windowed one at a time would not be a stack.
//
// `dayview` has no window flag, so the window is a picture WIDER than the
// band, shown through it: the day is asked for at `pictureWidth`, which is
// as many pixels as the band would need to hold all 24 hours at the
// window's scale, and `windowMap` is the offset and scale that puts the
// window's own hours in the band. The gridlines and the ruler are the
// strip's own, drawn from the same map, so they follow the picture without
// a second question. (The flag would be better — one day's data at one
// width, whatever the zoom — and it is a CLI follow-up, not a reason to
// keep the strip at 24 hours meanwhile.)

/** The whole day: the window at rest, and what the strip's `⊖` returns to. */
export const HOUR_FROM = 0;
export const HOUR_TO = 24;
const DAY_HOURS = 24;

/**
 * The shortest window, in hours. An hour of one day across a whole band
 * is a minute to the millimetre — finer than anything a person logs.
 */
const MIN_SPAN = 1;

/**
 * The widest picture a band is asked for, in pixels.
 *
 * The zoom costs width: a 2-hour window is a day drawn twelve times as
 * wide as the band. Past this the picture stops growing and the map
 * stretches it instead — the deepest zooms are a little soft rather than
 * a `dayview` of tens of thousands of pixels per band per day.
 */
const MAX_PICTURE = 6000;

/** An hour, rounded to the minute — as fine as a window is ever stated. */
const toMinute = (hour: number) => Math.round(hour * 60) / 60;

/**
 * `[from, to]` as a window the strip can draw: inside the day, at least
 * `MIN_SPAN` long, and stated to the minute.
 */
export function clampWindow(from: number, to: number): [number, number] {
	const a = Number.isFinite(from) ? from : HOUR_FROM;
	const b = Number.isFinite(to) ? to : HOUR_TO;
	const span = Math.min(DAY_HOURS, Math.max(MIN_SPAN, b - a || DAY_HOURS));
	const start = Math.min(Math.max(a, 0), DAY_HOURS - span);
	return [toMinute(start), toMinute(start + span)];
}

/** Whether `view` is the whole day — the one window that needs no map. */
export const wholeDay = (view: [number, number]) =>
	view[0] <= HOUR_FROM && view[1] >= HOUR_TO;

/**
 * How wide a day must be DRAWN for `span` of its hours to fill `bandWidth`.
 *
 * At the whole day it is the band itself, which is what the strip has
 * always asked for.
 */
export const pictureWidth = (bandWidth: number, span: number) =>
	Math.min(MAX_PICTURE, Math.round((bandWidth * DAY_HOURS) / span));

/**
 * The picture's x → the band's x: `x * scale + offset`.
 *
 * One map for the pictures (a CSS transform) and for the strip's own
 * gridlines and ruler (arithmetic), so the three cannot disagree. At the
 * whole day it is the identity, to the pixel the rounding of
 * `pictureWidth` leaves.
 */
export function windowMap(
	view: [number, number],
	full: number,
	bandWidth: number
): { offset: number; scale: number } {
	const span = view[1] - view[0];
	return {
		offset: -(view[0] * bandWidth) / span,
		scale: (DAY_HOURS * bandWidth) / (full * span)
	};
}

/**
 * The window a gesture asks for: `view` scaled by `by` about the band
 * fraction `was`, with that point carried to the fraction `now`.
 *
 * `by` is the gesture's own — fingers apart is greater than one, and a
 * picture magnified shows LESS time, so the span is divided by it. The
 * hour under the fingers when they landed is the hour under them now,
 * which is what makes the gesture a grip on the picture rather than a
 * dial beside it; a pan is the same thing with `by` of one.
 */
export function zoomWindow(
	view: [number, number],
	by: number,
	was: number,
	now = was
): [number, number] {
	const span = (view[1] - view[0]) / Math.max(by, 1e-3);
	const hour = view[0] + was * (view[1] - view[0]);
	const from = hour - now * span;
	return clampWindow(from, from + span);
}

/**
 * The box one day of the past occupies: the picture and nothing else.
 *
 * A band is asked for with `--margin 0`, so the SVG is its data area edge
 * to edge — which is what lets two bands touch across a 1px divider and
 * still read as one picture with the gridlines running through.
 */
export const bandBox = (dayHeight: number) => dayHeight;

/** What each further day costs the stack: a day and the divider above it. */
export const dayPitch = (dayHeight: number) => dayHeight + DIVIDER;

/** The box today's band occupies. The hour axis under it is the web's row. */
export const todayBox = (dayHeight: number) => dayHeight + AXIS_HEIGHT;

/**
 * The day height at which ONE DAY exactly fills a strip box `height` tall
 * — `todayBox` read backwards (da-r0ydu).
 *
 * vividn, 2026-09-21: «let's make the dayview widget internal options
 * default to a day height that is equal to the height of the widget». It
 * dissolves a problem rather than tuning it: a day was a FIXED height
 * inside a pane that is a PROPORTION, so the strip showed the same number
 * of days at every screen size and «exactly one day» was a ~3px window a
 * ratio had to hold across a 1.59x spread of phone heights (da-hkjp's
 * `STRIP_WEIGHT`, now gone). Derived, one day fits at every size by
 * construction and there is no number to measure at one of them.
 *
 * Clamped as the strip clamps anything it draws: below the floor the box
 * is shorter than the shortest legible day and today is clipped from the
 * top (the strip's own notch); above the ceiling the past comes back into
 * the scroller, which is what a very tall strip is for.
 */
export const dayFilling = (height: number): number =>
	Math.min(MAX_DAY_HEIGHT, Math.max(MIN_DAY_HEIGHT, Math.round(height - AXIS_HEIGHT)));

/**
 * One picture of the day strip, as argv: the single day `end` days back.
 *
 * One `dayview` per DAY (da-iu9i), and each one bands only — no date
 * column (`--label-width 0`), no ruler (`--time-axis-height 0`), no outer
 * margin (`--margin 0`). What comes back is a picture exactly `width` by
 * `dayHeight`, all of it data area, so the days tile the column and the
 * strip draws the stamps, the ruler and the gridlines that run across
 * them itself.
 *
 * Every band is asked at the same width and day height, which is what
 * pins the x of every hour: `time_axis_extent` is the width, and the
 * gridlines fall at the same fractions of it in each.
 */
export function bandArgv(width: number, dayHeight: number, end = 0): string[] {
	const argv = [
		'dayview',
		'--width',
		String(width),
		'--day-height',
		String(dayHeight),
		'--margin',
		'0',
		'--label-width',
		'0',
		'--time-axis-height',
		'0',
		'--days',
		'1'
	];
	return end === 0 ? argv : [...argv, '--end-date', String(-end)];
}

/**
 * The geometry every band shares, as argv: today's band asked for its
 * layout rather than its picture (da-55ru's `--show json`).
 *
 * `hours[]` is where each gridline sits along the picture's x, which is
 * where the strip draws its own continuous ones and its ruler's labels;
 * `days[0].date` is the day the CLI cut, which the stamps count back
 * from — so the strip never does civic-day arithmetic of its own.
 */
export const geometryArgv = (width: number, dayHeight: number): string[] => [
	...bandArgv(width, dayHeight),
	'--show',
	'json'
];

/** `geometryArgv`'s answer, as much of it as the strip draws with. */
export interface DayviewJson {
	hours?: { hour: number; x: number }[];
	axis_ticks?: { is_major?: boolean; label?: string }[];
	days?: { date: string }[];
}

// `nowview -o -`, not `now` or bare `nowview`: both of those paint a band
// for a terminal, and a browser pane is a better place for the picture the
// CLI writes to a .svg file — which `-o -` puts on stdout.
export const NOWVIEW: PaneSpec = {
	title: 'nowview',
	argv: ['nowview', '-o', '-'],
	empty: 'no field has a state yet'
};

/**
 * How far back the nowview widget reaches, as its − / + step it (vividn,
 * 2026-09-11): `0` is the snapshot, each field's state now and no time.
 */
export const NOWVIEW_STEPS = ['0', '5m', '10m', '15m', '20m', '30m', '45m', '60m', '90m', '2h', '3h', '6h', '12h'];
export const NOWVIEW_HISTORY = '30m';

/** A step of the ladder as minutes. It reads the ladder's own spellings, no more. */
function stepMinutes(step: string): number {
	const m = /^(\d+(?:\.\d+)?)\s*([mh])?$/.exec(step);
	if (!m) return NaN;
	return Number(m[1]) * (m[2] === 'h' ? 60 : 1);
}

/**
 * The window a zoom of `scale` asks for: the step nearest the scaled one
 * (da-5goc).
 *
 * `scale` is the gesture's own — fingers apart is greater than 1, and a
 * picture magnified shows LESS time, so the window is divided by it.
 *
 * Nearest by RATIO, not by minutes: the ladder is geometric, and 6h's
 * neighbour is 12h at a distance nothing near the short end of the ladder
 * can rival in minutes. Which also means a small pinch lands back on the
 * step it started from — a zoom is not a ratchet, and the re-ask a
 * quarter-turn of the fingers would cost is not made.
 *
 * The snapshot has no span to divide, so it stands in at half the shortest
 * window: one pinch out off `0` reaches 5m, and one pinch in off 5m
 * reaches `0`.
 */
export function zoomHistory(history: string, scale: number): string {
	const minutes = NOWVIEW_STEPS.map(stepMinutes);
	const from = stepMinutes(NOWVIEW_STEPS.includes(history) ? history : NOWVIEW_HISTORY);
	const target = (from || minutes[1] / 2) / Math.max(scale, 0.001);
	if (target < minutes[1] / 2) return NOWVIEW_STEPS[0];
	let best = 1;
	const off = (i: number) => Math.abs(Math.log(minutes[i] / target));
	for (let i = 2; i < minutes.length; i++) if (off(i) < off(best)) best = i;
	return NOWVIEW_STEPS[best];
}

/**
 * NOWVIEW at a window of `history`, centred on `at` when there is one
 * (da-pcis), and drawn with time running down the page when `vertical`
 * (da-78evn).
 */
export const nowviewSpec = (history: string, at?: string, vertical = false): PaneSpec => ({
	...NOWVIEW,
	argv: [
		...NOWVIEW.argv,
		'--history',
		history,
		...(at ? ['-t', at] : []),
		...(vertical ? ['--orientation', 'vertical'] : [])
	]
});

export const TAIL: PaneSpec = {
	title: 'tail',
	argv: ['tail'],
	empty: 'nothing logged yet — try `switch mood happy`',
	view: 'tail-table'
};

/**
 * The argv of a command typed into the custom pane (da-wb2n).
 *
 * Split as the REPL splits a line, so `note "hello world"` means the same in
 * both. Bare `nowview` is asked for its picture (`-o -`, as NOWVIEW is) —
 * the terminal band it prints otherwise is a worse thing to put in a pane.
 */
export function commandArgv(line: string): string[] {
	const argv = shellSplit(line);
	return argv[0] === 'nowview' && !draws(argv) ? [...argv, '-o', '-'] : argv;
}
