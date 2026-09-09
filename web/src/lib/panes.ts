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
	if (!SIZED_COMMANDS.has(argv[0]) || width === null || width <= 0) return argv;
	if (argv.some((word) => word === '--width' || word.startsWith('--width='))) return argv;
	return [...argv, '--width', String(width)];
}

/**
 * The data height of one band of the day strip, in pixels.
 *
 * A constant, and the same constant for every band — which is what makes
 * the stack a stack (da-cgyh). `label_width` in `compute_dayview` is
 * derived from the label font size, which is derived from `day_height`;
 * two bands drawn at different heights therefore start their data areas
 * at different x, and their hour gridlines would not line up. Pinning
 * `--day-height` pins all of it, so the same hour is the same column
 * from the oldest band down to today's.
 *
 * 85 is what the strip has always drawn at: a 9rem pane, less its title
 * bar and padding, less the axis strip and margins datum adds — the
 * height `datum dayview --height` derived for the box the pane had.
 */
export const BAND_HEIGHT = 85;

/**
 * The box one band occupies, `BAND_HEIGHT` plus datum's own outer margin
 * top and bottom (`--margin`, default 2 — `DayviewArgs::default`).
 *
 * Reserved in CSS so the stack's height is known before a band's picture
 * has decoded. Without it the stack grows under the reader a frame after
 * a pull, and the scroll position it was anchored to — yesterday against
 * today — is a scroll position it no longer has.
 */
export const BAND_BOX = BAND_HEIGHT + 2 * 2;

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
export function bandArgv(width: number, daysAgo: number): string[] {
	const argv = ['dayview', '--width', String(width), '--day-height', String(BAND_HEIGHT)];
	if (daysAgo === 0) return argv;
	return [...argv, '--time-axis-height', '0', '--end-date', String(-daysAgo)];
}

// `nowview`, not `now`: the CLI splits them by output device — `now`
// paints a band on the terminal, `nowview` writes the SVG — and a browser
// pane is the second of those. The pane shows what the CLI would write to
// a .svg file, which is a better picture than the terminal's.
export const NOWVIEW: PaneSpec = {
	title: 'nowview',
	argv: ['nowview'],
	empty: 'no field has a state yet',
	sized: true
};

export const TAIL: PaneSpec = {
	title: 'tail',
	argv: ['tail'],
	empty: 'nothing logged yet — try `switch mood happy`'
};
