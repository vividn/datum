// What the CLI-render panes run, and how wide they claim to be — plus
// the day strip's bands, which are the same thing one row down
// (`bandArgv` at the foot of this file).
//
// Each pane is one datum command rendered as text (da-1x72). Deliberately
// CLI renders for now: the moment a pane becomes an interactive table it
// stops being free, and that is a later, separate trade.

/** One pane: a title, the command behind it, and what to say when it's quiet. */
export interface PaneSpec {
	title: string;
	argv: string[];
	/** Shown when the command succeeds with no output at all. */
	empty?: string;
	/**
	 * Run the command against the pane's own box, in pixels.
	 *
	 * Both drawn panes. `datum dayview` authors a 2000px canvas because
	 * that is a sensible page to write a .svg to; a pane 345px wide showed
	 * it scaled to 0.17, which put the hour axis on screen at 1.5px and the
	 * date stamp at 3.4 — illegible on a laptop too, at 0.70 (da-m0cj). The
	 * band is drawn at the size it is displayed at instead.
	 */
	sized?: boolean;
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
 * A sized pane waits for its box: the geometry goes across as `--width`
 * and `--height`, the CLI's own flags in the CLI's own units, so the pane
 * asks datum for a band of a given size rather than resizing datum's
 * answer. Null is "not measured yet", which lasts until the first
 * observation — the alternative is one render at the 2000px default that
 * the measurement immediately replaces.
 */
export function paneArgv(spec: PaneSpec, box: PaneBox | null): string[] | null {
	if (!spec.sized) return spec.argv;
	if (!box || box.width <= 0 || box.height <= 0) return null;
	return [...spec.argv, '--width', String(box.width), '--height', String(box.height)];
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
 * The panes say this as `sized` above, because a pane also knows its
 * height; a line typed at the prompt scrolls, so it only names a width.
 */
const SIZED_COMMANDS = new Set(['dayview', 'nowview']);

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
 * widget bar room on a laptop, and the strip caps its own height against
 * the viewport as well (`maxStack` in DayStrip.svelte).
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

/** The box a stacked band occupies at `dayHeight`. It has no hour axis. */
export const bandBox = (dayHeight: number) => dayHeight + 2 * MARGIN;

/** The box today's band occupies at `dayHeight`. It carries the axis. */
export const todayBox = (dayHeight: number) => dayHeight + 2 * MARGIN + TIME_AXIS;

/**
 * One band of the day strip, as argv.
 *
 * `daysAgo` counts back from today, so 0 is today's band and 1 is
 * yesterday's. Today's carries the hour axis and the others suppress
 * theirs with `--time-axis-height 0`: they share one x-axis, and a strip
 * repeating it under every band would be five copies of the same ruler.
 * The gridlines are not the axis and are drawn on every band, which is
 * how a reader carries an hour up the stack.
 */
export function bandArgv(width: number, daysAgo: number, dayHeight: number): string[] {
	const argv = ['dayview', '--width', String(width), '--day-height', String(dayHeight)];
	if (daysAgo === 0) return argv;
	return [...argv, '--time-axis-height', '0', '--end-date', String(-daysAgo)];
}

// `nowview -o -`, not `now` or bare `nowview`: both of those paint a band
// for a terminal, and a browser pane is a better place for the picture the
// CLI writes to a .svg file — which `-o -` puts on stdout.
export const NOWVIEW: PaneSpec = {
	title: 'nowview',
	argv: ['nowview', '-o', '-'],
	empty: 'no field has a state yet',
	sized: true
};

export const TAIL: PaneSpec = {
	title: 'tail',
	argv: ['tail'],
	empty: 'nothing logged yet — try `switch mood happy`',
	view: 'tail-table'
};
