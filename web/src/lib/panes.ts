// What the three CLI-render panes run, and how wide they claim to be.
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
	 * For the drawn panes. `datum dayview` authors a 2000px canvas because
	 * that is a sensible page to write a .svg to; a pane 345px wide showed
	 * it scaled to 0.17, which put the hour axis on screen at 1.5px and the
	 * date stamp at 3.4 — illegible on a laptop too, at 0.70 (da-m0cj). The
	 * band is drawn at the size it is displayed at instead, which is the
	 * same reason nowview always read correctly.
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
 * `dayview` alone. It authors a 2000px canvas — the right page to write a
 * .svg to, and four times the width of any pane or prompt — so wherever it
 * lands it was scaled, and the text with it. nowview is authored at 520px,
 * near its display size, which is why it always read correctly.
 *
 * The panes say this as `sized` above, because a pane also knows its
 * height; a line typed at the prompt scrolls, so it only names a width.
 */
const SIZED_COMMANDS = new Set(['dayview']);

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
 * The column count the panes render CLI output at.
 *
 * datum sizes its tables to the terminal (`render/term_layout.rs`: ioctl,
 * then `$COLUMNS`, then 80). A browser pane has no terminal, so it claims
 * a fixed width instead of tracking its own pixels — a table that
 * re-columnised on every window drag would be unreadable. The panes are
 * laid out at least this wide and scroll sideways past it; the wasm
 * runner (da-1vat) exports the same number as `$COLUMNS` so the CLI and
 * the pane agree.
 */
export const PANE_COLS = 80;

export const DAYVIEW: PaneSpec = {
	title: 'dayview',
	argv: ['dayview'],
	empty: 'nothing to draw yet',
	sized: true
};

// `nowview`, not `now`: the CLI splits them by output device — `now`
// paints a band on the terminal, `nowview` writes the SVG — and a browser
// pane is the second of those. The pane shows what the CLI would write to
// a .svg file, which is a better picture than the terminal's.
export const NOWVIEW: PaneSpec = {
	title: 'nowview',
	argv: ['nowview'],
	empty: 'no field has a state yet'
};

export const TAIL: PaneSpec = {
	title: 'tail',
	argv: ['tail'],
	empty: 'nothing logged yet — try `switch mood happy`'
};
