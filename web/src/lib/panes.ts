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

/** A pane's content box, in whole pixels. */
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
 * datum's own outer margin, top and bottom (`--margin`, default 2 —
 * `DayviewArgs::default`), and the hour-axis strip under today's band
 * (`--time-axis-height`, default 15).
 *
 * Named here because the strip reserves each band's box in CSS before its
 * picture has decoded. Without that the stack grows under the reader a
 * frame after a drag, and the scroll position it was anchored to —
 * yesterday against today — is a position it no longer has.
 */
const MARGIN = 2;
const TIME_AXIS = 15;
/** The space datum leaves between two days of one picture (`DEFAULT_INTERDAY_MARGIN`). */
const INTERDAY = 15;

/** The box one day of the past occupies at `dayHeight`. It has no hour axis. */
export const bandBox = (dayHeight: number) => dayHeight + 2 * MARGIN;

/** What each further day adds to a picture of the past: a day and the space above it. */
export const dayPitch = (dayHeight: number) => dayHeight + INTERDAY;

/** The box today's band occupies at `dayHeight`. It carries the axis. */
export const todayBox = (dayHeight: number) => dayHeight + 2 * MARGIN + TIME_AXIS;

/**
 * One picture of the day strip, as argv: `days` days whose newest is
 * `end` days back from today.
 *
 * `end` 0 is today's band, which carries the hour axis. The past is drawn
 * as ONE dayview of several days (da-hnyb), so the hour gridlines run
 * through it and its ground is one; it suppresses the axis with
 * `--time-axis-height 0` — it shares today's x-axis, and the ruler is
 * drawn once, under today.
 */
export function bandArgv(width: number, dayHeight: number, end = 0, days = 1): string[] {
	const argv = ['dayview', '--width', String(width), '--day-height', String(dayHeight)];
	if (end === 0) return argv;
	return [...argv, '--time-axis-height', '0', '--n-days', String(days), '--end-date', String(-end)];
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

/** NOWVIEW at a window of `history`, centred on `at` when there is one (da-pcis). */
export const nowviewSpec = (history: string, at?: string): PaneSpec => ({
	...NOWVIEW,
	argv: [...NOWVIEW.argv, '--history', history, ...(at ? ['--at', at] : [])]
});

export const TAIL: PaneSpec = {
	title: 'tail',
	argv: ['tail'],
	empty: 'nothing logged yet — try `switch mood happy`',
	view: 'tail-table'
};

/**
 * The argv of a command typed into the command pane (da-wb2n).
 *
 * Split as the REPL splits a line, so `note "hello world"` means the same in
 * both. Bare `nowview` is asked for its picture (`-o -`, as NOWVIEW is) —
 * the terminal band it prints otherwise is a worse thing to put in a pane.
 */
export function commandArgv(line: string): string[] {
	const argv = shellSplit(line);
	return argv[0] === 'nowview' && !draws(argv) ? [...argv, '-o', '-'] : argv;
}
