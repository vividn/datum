// Pointing INTO the nowview and the dayview (da-rskp, da-xg1vi): which
// gesture a press on a band has become.
//
// THE TABLE, vividn's, 2026-10-22 (da-xg1vi), for both views:
//
//   - single tap → select a piece of data (the mark under the finger), and
//     a second tap on it the time there instead (15:04);
//   - single tap where there is no data, or in the time axis → select a time;
//   - long tap / right click → select a time;
//   - simple swipe or drag → pan; or, if already at the edge, pass the
//     swipe to the container (so it switches tabs);
//   - long tap and drag → select a time period;
//   - swiping left/right must not select time;
//   - touch the time axis and drag → no pan: the time follows the finger,
//     to choose it precisely.
//
// «For quickly selecting a time, a user can touch in the time axis, so that
// won't slow them down.» Which is what retired the rule before it, that a
// plain tap always meant TIME.
//
// The machine below is the part of that table a MOVE decides:
//
//   down on the band
//     │
//     ├─ two fingers ─────────────────────────► PINCH   (the widget's own)
//     ├─ on the time axis ────────────────────► SCRUB   (the widget's own: decided at the press)
//     │
//     └─ one finger, mode `press`
//          ├─ shift held, |dx| > 6px ─────────────────────► RANGE  (the desk's, no long press)
//          ├─ down for LONG_MS, still ────────────────────► HOLD   (undecided: see below)
//          ├─ |dx| > 6px and |dx| ≥ |dy| ─────────────────► PAN    (the window moves, or the
//          │                                                        container's, at an edge)
//          └─ up, still `press` ──────────────────────────► TAP    (the mark under it, or the time)
//
//   and from HOLD, which is NOT yet committed:
//          ├─ in the NOW REGION, |dx| > 6px leftward ─────► NOW    (a duration ending now)
//          ├─ |dx| > 6px and |dx| ≥ |dy| ─────────────────► RANGE  (a long press that TRAVELS)
//          └─ up, still ──────────────────────────────────► the TIME where it fell
//
// What a lift MEANS — a mark's entry or a time — is the widget's, which has
// the picture; the machine only says which of the two kinds of lift it was.
//
// THE LONG PRESS DOES NOT COMMIT (da-pzzoz): a still long press stays
// undecided in `hold`, and the finger decides it by moving or lifting. The
// price is FEEDBACK — a mode that commits to nothing has none — so the two
// widgets draw a mark of their own for the wait (`.hold-mark`). It is owed,
// not optional.
//
// The other transitions are ONE-WAY: a drag that became a pan never
// becomes a range, and a range never becomes anything else.
// `|dx| ≥ |dy|` is what leaves the page its vertical scroll — `touch-action`
// is `pan-y`, so a mostly-vertical finger belongs to the browser and this
// never claims it.
//
// Pure, so the truth table above is `tests/nowpoint-pure.test.mjs` in node
// and not something only a browser can be asked.

/**
 * What a press has become.
 *
 * `press` and `hold` are both UNDECIDED — a lift there is a gesture of
 * its own (a tap, a time) and a move can still turn either into something
 * else. `pan`, `range`, `now` and `scrub` are committed and final; `scrub`
 * is the time axis's, which the widget decides at the press.
 */
export type Mode = 'press' | 'hold' | 'pan' | 'range' | 'now' | 'scrub';

/** Whether a mode is still open to becoming another one. */
export const undecided = (mode: Mode): boolean => mode === 'press' || mode === 'hold';

/** Where a press went down, and what it landed on. */
export interface Press {
	x: number;
	y: number;
	/** In the NOW region — the strip at and right of the now-line. */
	inNow: boolean;
	/**
	 * A drag here is a RANGE with no waiting: shift is held, because a long
	 * press with a mouse is nobody's habit. Never a finger's — «swiping
	 * left/right must not select time» (vividn, da-xg1vi).
	 */
	atOnce: boolean;
}

/**
 * How long a still press takes to become a range: the app's one hold,
 * `HOLD_MS` (#lib/hold.ts), so a press long on a band is long everywhere.
 */
export { HOLD_MS as LONG_MS } from './hold.js';

/**
 * How far a press may travel and still be still, in px.
 *
 * A pixel over `inspect.ts`'s CLICK_SLOP of 5, so a press that commits to a
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
 * A COMMITTED mode is returned unchanged: what a gesture became is decided
 * once. `hold` is not committed — it is «still down, and long enough» —
 * so it is asked again on every move, and the move is what decides it.
 *
 * Travel before the wait is a PAN, travel after it a RANGE — or, from the
 * now region and leftward, the duration ending now. A pan with nowhere to
 * go is the widget's to hand on (the table above): the machine does not
 * know where the edges are.
 */
export function advance(mode: Mode, p: Press, x: number, y: number, long: boolean): Mode {
	if (!undecided(mode)) return mode;
	const [dx, dy] = [x - p.x, y - p.y];
	const sideways = Math.abs(dx) > SLOP_PX && Math.abs(dx) >= Math.abs(dy);
	if (!sideways) return long ? 'hold' : 'press';
	if (long || p.atOnce) return p.inNow && dx < 0 ? 'now' : 'range';
	return 'pan';
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
 * The steps a tap's time can snap to, in seconds: a minute is the coarsest,
 * which is what every window from 30 minutes up has always said.
 */
const TAP_STEPS = [1, 5, 10, 15, 30, 60];

/**
 * How finely a tap on a window `minutes` long says its time, in seconds:
 * the coarsest step that is still a 24th of the window or less.
 *
 * vividn, on a two-minute window: «you should be able to do 5 seconds at
 * this scale». A 24th of two minutes is five seconds; of the 30-minute
 * default it is over a minute, so that window stays in whole minutes, and
 * a window under two minutes goes to the second.
 */
export function tapStep(minutes: number): number {
	const want = (minutes * 60) / 24;
	return TAP_STEPS.filter((s) => s <= want).pop() ?? TAP_STEPS[0];
}

/**
 * A relative selection's argv: the `-q` ladder on the five-minute marks it
 * reaches, the relative `-t -N` on any other whole minute, and the CLI's
 * seconds (`-t -45s`, `-t -1m30s`) when it is not a whole minute.
 *
 * All are read by the CLI as «that long before the moment this RUNS»
 * (`-q` is five minutes back and stacks; `-t -7` is seven minutes back),
 * which is exactly what a selection dropped on a relative axis means.
 * vividn: "If the -5 -10 -15 or -20 lines is clicked, use -q, -qq, -qqq,
 * -qqqq as the pill. for other times use relative time as the pill".
 *
 * `rel` is minutes back, to the second.
 */
export function relArg(rel: number): string {
	const s = Math.round(rel * 60);
	const [m, sec] = [Math.floor(s / 60), s % 60];
	if (sec) return `-t -${m ? `${m}m` : ''}${sec}s`;
	const q = m / 5;
	return Number.isInteger(q) && q >= 1 && q <= QUANTA ? `-${'q'.repeat(q)}` : `-t -${m}`;
}
