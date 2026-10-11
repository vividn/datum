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
// SO THERE IS NO FLING THRESHOLD. There used to be one (`FLING`, 1.2 px/ms)
// and it was the complaint itself: under it a release snapped to one of two
// places, over it the bar went all the way, and nothing in between existed.
// A release now glides by what it CARRIES — `speed * TAU` — and may come to
// rest anywhere. What takes a rest away from the reader is only a detent,
// and there are three of them, each a place the bar MEANS something:
//
//     0         shut
//     notch     the output section open and nothing else
//     ceiling   the bar at the top of the screen, the list at its newest
//
// THE NOTCH AND THE CEILING CATCH, two ways which are one rule: a glide that
// would come to rest within `DETENT` of one is taken by it — that is
// "arrives at rest near it" — and a glide that would pass it is measured the
// same way, because a glide passing slowly is a glide that stops just beyond.
//
// SHUT HAS NO CATCH. It is reached by the one rule left: BELOW THE NOTCH
// there is nowhere to rest, so a glide ending short of the notch's catch goes
// where the gesture was heading, whatever its speed — up to the notch, down
// to shut. One that ends 10px from shut after a gesture heading up opens to
// the notch.
//
// AND A COAST NEVER CROSSES A DETENT IT CAME FROM ABOVE — the SNAG. vividn,
// 2026-09-18: "When closing snag at the notch so that the output section
// stays open, but you can drag over this notch if you like." So a fling
// down from a tall log stops at the notch and a second one shuts it, which
// is the agreement `toggleOutput` keeps from the other side. The ceiling
// snags for the same reason and it is the same sentence: "When I get back
// to the top, then trying to scroll up further starts closing the input
// section" — getting back to the top is a place a coast stops at, and
// closing from there is the NEXT thing the reader does, not something the
// coast does for them. A finger still drags through either, at any speed:
// the snag is about what momentum does alone.
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
 * Small enough that a height just above the output section is still a
 * height the reader can choose — "You should be able to open the REPL
 * section as far as you want" (vividn, 2026-09-18) — and wide enough that
 * nobody has to aim.
 */
export const DETENT = 24;

/**
 * The two numbers the rule is made of, as one value a caller can vary.
 *
 * ONE OBJECT AND NOT TWO MORE POSITIONAL ARGUMENTS (da-o2wm8): `rest`
 * already takes five, and `rest(now, speed, up, notch, ceiling, 900, 0)`
 * is a line nobody can read. The pair also travels together everywhere —
 * the debug widget moves them as a pair, `$lib/tuning.ts` keeps them as
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
 * opening), comes to rest — given the notch and the ceiling.
 *
 * `up` is the gesture's own direction and decides only one thing: BELOW THE
 * NOTCH there is nowhere to rest, because a half-drawn output row is not a
 * state, so a glide that ends down there goes to whichever end the gesture
 * was heading for. vividn, 2026-09-18: "dragging up a bit should snap to the
 * single output bar open… similar to hitting ^ toggle" — and a toggle does
 * not ask how far you pushed it.
 *
 * The answer is unclamped: how far the bar may actually go is the page's
 * and the list's business ($lib/log.ts), not this table's.
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
	// THE SNAG, at the first threshold this glide would cross downward.
	//
	// `now > edge + 0.5` and not `now >= edge - 0.5`: a release AT an edge
	// has not come from above it, and must not be snagged back onto it. That
	// asymmetry is what lets a second throw down SHUT the bar from the notch
	// — vividn's "you can drag over this notch if you like" — where the
	// symmetric test would pin it there for ever. Nothing is lost at the
	// boundary itself: a glide that ends within `DETENT` of an edge is taken
	// by it two lines below, snag or no snag (canary on PR #59).
	for (const edge of [ceiling, notch]) {
		if (now > edge + 0.5 && to < edge) {
			to = edge;
			break;
		}
	}
	if (Math.abs(to - notch) <= detent) return notch;
	if (Math.abs(to - ceiling) <= detent) return ceiling;
	return to < notch ? (up ? notch : 0) : to;
}
