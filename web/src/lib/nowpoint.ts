// Pointing INTO the nowview (da-rskp): which of four gestures a press on
// the band has become.
//
// THE STATE MACHINE, written down before the code that runs it, because
// the band carries four gestures of its own — tap, drag, long-press+drag,
// two-finger pinch — over a pane that swipes between tabs and a page that
// scrolls:
//
//   down on the band
//     │
//     ├─ two fingers ─────────────────────────► PINCH   (da-5goc, the widget's own)
//     │
//     └─ one finger, mode `press`
//          ├─ in the NOW REGION and |dx| > 6px leftward ──► NOW    (a duration ending now)
//          ├─ `atOnce` and |dx| > 6px ────────────────────► RANGE  (no long press needed)
//          ├─ down for 400 ms ────────────────────────────► RANGE  (the bar drops at once)
//          ├─ |dx| > 6px and |dx| ≥ |dy| ─────────────────► PAN    (the window moves)
//          └─ up, still `press` ──────────────────────────► CLICK  (a selection where it fell)
//
// The transitions are ONE-WAY: a drag that became a pan never becomes a
// range, and a long press that has dropped its bar never becomes a pan.
// `|dx| ≥ |dy|` is what leaves the page its vertical scroll — `touch-action`
// is `pan-y`, so a mostly-vertical finger belongs to the browser and this
// never claims it.
//
// Pure, so the truth table above is `tests/nowpoint-pure.test.mjs` in node
// and not something only a browser can be asked.

/**
 * What a press has become.
 *
 * `press` is undecided — still a click if the finger lifts there. The
 * other three are committed and final.
 */
export type Mode = 'press' | 'pan' | 'range' | 'now';

/** Where a press went down, and what it landed on. */
export interface Press {
	x: number;
	y: number;
	/** In the NOW region — the strip at and right of the now-line. */
	inNow: boolean;
	/**
	 * A drag here is a RANGE with no waiting.
	 *
	 * Two views say it for two reasons, and the recogniser needs only the
	 * answer: shift is held, because a long press with a mouse is nobody's
	 * habit (the nowview), or the view is zoomed all the way out and has
	 * nowhere to pan to, so a drag can be nothing else (the day strip,
	 * da-ku7y).
	 */
	atOnce: boolean;
}

/** How long a still press takes to become a range. */
export const LONG_MS = 400;

/**
 * How far a press may travel and still be still, in px.
 *
 * A pixel over `hover.ts`'s CLICK_SLOP of 5, so a press that commits to a
 * gesture here has already stopped being a click there.
 */
export const SLOP_PX = 6;

/**
 * The mode a press is in with the pointer at (`x`, `y`), `long` once it has
 * lasted `LONG_MS`.
 *
 * Called on every move AND once on the timer that sets `long`, so the long
 * press lands without the finger doing anything. `long` is that TIMER's
 * word and not a reading of the clock: `Date.now()` is a thing a test may
 * hold still, and a gesture the app cannot make under a fixed clock is a
 * gesture nothing can assert.
 *
 * A mode past `press` is returned unchanged: what a gesture became is
 * decided once.
 */
export function advance(mode: Mode, p: Press, x: number, y: number, long: boolean): Mode {
	if (mode !== 'press') return mode;
	const [dx, dy] = [x - p.x, y - p.y];
	const sideways = Math.abs(dx) > SLOP_PX && Math.abs(dx) >= Math.abs(dy);
	if (p.inNow) return sideways && dx < 0 ? 'now' : 'press';
	if (p.atOnce) return sideways ? 'range' : 'press';
	if (long) return 'range';
	return sideways ? 'pan' : 'press';
}

/**
 * How near a gridline a press must land to be read as ON it, in px.
 *
 * vividn asked for the −5 / −10 / −15 / −20 minute gridlines to spell
 * themselves `-q` … `-qqqq`, «within a finger's width». A whole fingertip
 * is ~44px, which at nowview's 30-minute default is eleven minutes of
 * band: it would swallow the times BETWEEN the marks, and those are the
 * relative `-t -7` the same sentence asks for. 12px is the width of the
 * mark itself plus a dash either side — near enough to hit without aiming,
 * and 2½ minutes of a 30-minute window.
 */
export const SNAP_PX = 12;

/**
 * Where a press at `x` lands once it has snapped: the nearest of `marks`
 * within `SNAP_PX`, or `x` itself.
 *
 * `marks` are the gridlines' own screen positions, read off the picture —
 * never a multiple of five minutes computed here, because which minutes
 * the picture RULED is the picture's to say (a 12-hour window rules hours).
 * Two marks equally near is the FIRST of them, in the order the picture
 * drew them: a tie has to land somewhere, and «somewhere» has to be the
 * same answer twice.
 */
export function snapTo(x: number, marks: number[]): number {
	let best = x;
	let off = Infinity;
	for (const mark of marks) {
		const d = Math.abs(mark - x);
		if (d <= SNAP_PX && d < off) [best, off] = [mark, d];
	}
	return best;
}

/**
 * How far back `-q` reaches: five minutes a letter, four letters (`-qqqq`
 * is twenty). The gridlines at those four minutes are the ones a press
 * spells this way; past them there is no letter for it.
 */
const QUANTA = 4;

/**
 * A relative selection's argv: the `-q` ladder on the five-minute marks it
 * reaches, and the relative `-t -N` everywhere else.
 *
 * Both are read by the CLI as «that long before the moment this RUNS»
 * (`-q` is five minutes back and stacks; `-t -7` is seven minutes back),
 * which is exactly what a selection dropped on a relative axis means.
 * vividn: "If the -5 -10 -15 or -20 lines is clicked, use -q, -qq, -qqq,
 * -qqqq as the pill. for other times use relative time as the pill".
 */
export function relArg(rel: number): string {
	const q = rel / 5;
	return Number.isInteger(q) && q >= 1 && q <= QUANTA ? `-${'q'.repeat(q)}` : `-t -${rel}`;
}
