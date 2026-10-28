// WHERE A RELEASE COMES TO REST (da-tj6m): the bar's momentum, as a pure
// function of where the finger let go and how fast it was going.
//
// vividn, 2026-09-21, of the build before this one: "the momentum and
// acceleration of the input section still needs work, right now there is
// only acceleration if it is enough to get it all the way up or down, it
// should allow little bits of acceleration sliding the section up and down,
// and just have a nice notch at closed or open."
//
// Asked which of two he meant — momentum at any speed with detents that
// catch, or always settling to the nearer of two places — he chose the
// first: "More the first one, so it scrolls smoothly but catches at the
// right place."
//
// THREE STATES AND NOTHING BETWEEN (da-0b2vy). vividn, 2026-10-27, of
// the CLI redesign: «the top of the output bar should show a very clear
// handle toggle. Clicking or swiping up on this makes it full screen …
// when the cli is fullscreen … clicking or swiping there closes the cli
// again». That supersedes the height «as far as you want» of 2026-09-18:
// the bar rests in one of three places, each a state the bar MEANS:
//
//     0         CLOSED — the input row alone
//     notch     the SINGLE ROW — the output strip over the input
//     ceiling   FULLSCREEN — the history, one command a line, to the top
//
// A finger still carries the bar continuously, and a release still glides
// by what it CARRIES — `speed * TAU` — with no fling threshold. Where the
// glide would end decides nothing by itself any more, except near a state:
//
// THE NOTCH AND THE CEILING CATCH: a glide that would come to rest within
// `DETENT` of one is taken by it, so a drag too short to mean anything puts
// the bar back where it was. Anywhere else, the gesture's own DIRECTION
// picks between the two states either side — below the notch, up to the
// notch or down to shut; above it, up to the full screen or down to the
// row. vividn, 2026-09-18: "dragging up a bit should snap to the single
// output bar open… similar to hitting ^ toggle" — a toggle does not ask
// how far you pushed it.
//
// AND A COAST NEVER CROSSES A STATE IT CAME FROM — the SNAG, either way.
// vividn, 2026-09-18: "When closing snag at the notch so that the output
// section stays open, but you can drag over this notch if you like." So a
// fling down from the full screen stops at the row and a second one shuts
// it; and from shut, a fling up opens the row (the bead: «From closed, a tap
// or swipe up on the handle reopens the single row»). A finger still drags
// through either, at any speed: the snag is about what momentum does alone.
//
// Pure, so the table above is `tests/detent.test.mjs` in node rather than
// something only a phone can be asked. What it cannot settle is the NUMBERS
// — vremya's instrument adds latency to a synthetic touch, so a velocity
// measured in a drive is approximate, and where a real thumb lands relative
// to a boundary is a thing only a real thumb says.

/**
 * How long a release keeps gliding, in ms.
 *
 * The time constant of an exponential decay, so a release at `v` px/ms
 * travels `v * TAU` px before it stops. 320ms is the decay a browser's own
 * momentum scrolling uses (per-frame friction 0.95 at 60fps), which is the
 * feel a thumb already knows from every other scrolling surface.
 */
export const TAU = 320;

/**
 * How near a detent a glide is taken by it, in px.
 *
 * Wide enough that nobody has to aim, and that a drag shorter than it puts
 * the bar back in the state it started in.
 */
export const DETENT = 24;

/**
 * The two numbers the rule is made of, as one value a caller can vary.
 *
 * ONE OBJECT AND NOT TWO MORE POSITIONAL ARGUMENTS (da-o2wm8): `rest`
 * already takes five, and `rest(now, speed, up, notch, ceiling, 900, 0)`
 * is a line nobody can read. The pair also travels together everywhere —
 * the debug widget moves them as a pair, `#lib/tuning.ts` keeps them as
 * a pair — so it is one thing, not two.
 */
export interface Tuning {
	/** `TAU`, in ms. */
	tau: number;
	/** `DETENT`, in px. */
	detent: number;
}

/** What an untouched install feels: the two constants above, as a pair. */
export const DEFAULT_TUNING: Tuning = { tau: TAU, detent: DETENT };

/**
 * Where a release at `now` px, travelling at `speed` px/ms (positive is
 * opening), comes to rest: 0, the notch or the ceiling.
 *
 * `up` is the gesture's own direction, and it picks between the two states
 * either side of where the glide would end.
 *
 * `tuning` is the two numbers, PASSED IN and never read off ambient state
 * (da-o2wm8). The debug widget moves them at runtime so that the one
 * person who can judge them can, and the rule stays a pure function of
 * them — which is what keeps the table above answerable in node.
 */
export function rest(
	now: number,
	speed: number,
	up: boolean,
	notch: number,
	ceiling: number,
	{ tau, detent }: Tuning = DEFAULT_TUNING
): number {
	let to = now + speed * tau;
	// THE SNAG, at the first threshold this glide would cross, either way.
	//
	// `now > edge + 0.5` and not `now >= edge - 0.5`: a release AT an edge
	// has not come from beyond it, and must not be snagged back onto it. That
	// asymmetry is what lets a second throw down SHUT the bar from the notch
	// — vividn's "you can drag over this notch if you like" — where the
	// symmetric test would pin it there for ever (canary on PR #59).
	const down = to < now;
	for (const edge of down ? [ceiling, notch] : [notch, ceiling]) {
		if (down ? now > edge + 0.5 && to < edge : now < edge - 0.5 && to > edge) {
			to = edge;
			break;
		}
	}
	if (Math.abs(to - notch) <= detent) return notch;
	if (Math.abs(to - ceiling) <= detent) return ceiling;
	// Between two states, the gesture's own direction picks which.
	if (to < notch) return up ? notch : 0;
	return up ? ceiling : notch;
}
