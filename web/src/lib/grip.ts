// THE BAR'S GRIP (da-tj6m): the one gesture that moves the bar, wherever it
// is taken hold of.
//
// vividn, 2026-09-21: "you should be able to grab from anywhere in the repl
// and it behaves as one unit, so it will just pull the input section up
// more." So the bar and the log under it are not two surfaces with two
// gestures — they are one handle, and this action is what makes them one:
// `use:grip`, one drag, one number ($lib/log.ts). It is on the bar alone
// since the log moved above it and took a terminal's scroll (da-mwp4m).
//
// The state below is at MODULE scope and not per node, which is the point:
// a drag that starts on the log and wanders onto the bar is the same drag,
// and only one finger is ever holding the bar.

import { at, moveTo, moving, notch, resume, settle, stop } from './log.js';

/**
 * How far a pointer travels before it is a drag and not a tap.
 *
 * This and `SWALLOW_MS` are asserted with SYNTHETIC pointer events
 * (barlog.test.mjs), which is the right instrument for them: a synthetic
 * event carries the exact coordinates and the exact order a threshold is
 * about.
 *
 * It carries two things it cannot: TIME, because every move dispatched in a
 * loop shares one stamp, and a BROWSER'S OWN REFUSAL, because nothing the
 * page constructed is ever cancelled by the browser. So the two claims that
 * need one of those — the momentum, which is a speed, and `touch-action`,
 * whose whole effect is the `pointercancel` a real finger would get — are
 * driven by a real touch through CDP instead, in the same file. Neither
 * instrument replaces the other.
 */
const DRAG_START = 8;

/** A wheel this soon after one elsewhere is that gesture's inertia. */
const INERTIA_MS = 150;

/**
 * How long after a drag a click is still that drag's tail.
 *
 * A browser dispatches the click straight after the pointerup it came
 * from, so this only has to outlast one task; it is wide because being
 * wide costs nothing (no click is expected in that window anyway) and
 * being narrow would let a drag press the caret it ended on.
 *
 * The pointerup's stamp and the click's are comparable: both are
 * `DOMHighResTimeStamp` against the document's one time origin, which is
 * what `event.timeStamp` is for every event a page receives.
 */
const SWALLOW_MS = 300;

/** How far back a release looks for its speed. */
const WINDOW_MS = 80;

/**
 * And the least it will measure over.
 *
 * A real finger reports a move a frame at a time, so any gesture a person
 * made spans tens of milliseconds. Synthetic pointer events dispatched in
 * a loop all carry the SAME stamp, which over 200px would read as
 * hundreds of px/ms — a throw nobody made. Under this floor there is no
 * reading, so a script that sends a drag without time in it settles at
 * rest, which is what it is describing.
 */
const MIN_MS = 8;

/** Faster than this is a slip of the sensor, not a thumb. */
const MAX_SPEED = 4;

/** Every surface the grip is on — the bar, and the log under it. */
const gripped = new Set<HTMLElement>();

/** When a wheel last turned somewhere that is not one of them. */
let foreignAt = -Infinity;

/** The pointer that is down, where the bar was under it, and on which surface. */
let from: { y: number; open: number; id: number; node: HTMLElement } | null = null;

/**
 * The moves of the last `WINDOW_MS`, so a release knows how fast the finger
 * was going at the END of the gesture.
 *
 * The end and not the whole drag: a finger that raced and then stopped to
 * place the bar means the place, not the race. The stamps are
 * `DOMHighResTimeStamp` against the document's one time origin, so the
 * difference is milliseconds.
 */
let track: { y: number; t: number }[] = [];

let dragging = false;
/** Whether the press interrupted a settle that has to be finished for it. */
let settling = false;
/** A click this instant or sooner is the tail of a drag, and is not a tap. */
let swallowUntil = -Infinity;
/**
 * And on WHICH surface it would be.
 *
 * The bar and the log are one handle but they are two boxes, and a click on
 * one is never the tail of a drag on the other: a drag on the bar followed
 * inside 300ms by a press of `close history` down in the log is two acts,
 * and swallowing the second one left the log open with its own way out
 * pressed and ignored.
 */
let swallowOn: HTMLElement | null = null;

/**
 * px/ms over the window before `t`, positive when the bar is opening.
 *
 * Measured back from `t` and not from the last move (da-5rr8f): a pointer
 * at rest sends none, so a finger that stopped and held would otherwise
 * let go at the speed it had stopped from.
 */
function speed(y: number, t: number): number {
	const first = track.find((sample) => t - sample.t <= WINDOW_MS);
	if (!first) return 0;
	const ms = t - first.t;
	if (ms < MIN_MS) return 0;
	const px = (first.y - y) / ms;
	return Math.max(-MAX_SPEED, Math.min(MAX_SPEED, px));
}

/** Put the bar back on its way, after a press that turned out to be a tap. */
function unpause(): void {
	if (!settling) return;
	settling = false;
	// Where the glide WAS GOING, not "shut" ($lib/log.ts's `aim`): a tap
	// that lands inside a settle is not a request to undo what started it.
	resume();
}

function onPointerDown(event: PointerEvent): void {
	if (event.pointerType === 'mouse' && event.button !== 0) return;
	// A FINGER ALWAYS WINS OVER AN ANIMATION, so a press stops whatever
	// glide is in flight — and then it owes the bar a resting place, which
	// is what `settling` is for: a press that turns out to be a TAP puts
	// the bar back where the glide was taking it. Without that, a tap on
	// the box or the mark during a settle left the bar frozen part-way,
	// and a thrown glide is up to `GLIDE_MAX_MS` long, so the window is
	// wide enough to hit (canary on PR #46).
	settling = moving();
	stop();
	from = { y: event.clientY, open: at(), id: event.pointerId, node: event.currentTarget as HTMLElement };
	track = [{ y: event.clientY, t: event.timeStamp }];
	dragging = false;
}

function onPointerMove(event: PointerEvent): void {
	if (!from || event.pointerId !== from.id) return;
	const moved = from.y - event.clientY;

	// STILL A TAP — nothing below this line runs, and in particular
	// nothing is prevented: the box keeps its caret, its selection and
	// its keyboard until the finger has said it means a drag.
	//
	// A guard at the top rather than an early `return` nested inside the
	// block below, which is the same thing and read as the opposite by
	// two reviewers in a row (PR #46). The shape was the problem, not the
	// order.
	if (!dragging && Math.abs(moved) < DRAG_START) return;

	// The first move past it, once.
	if (!dragging) {
		dragging = true;
		// So a finger that wanders off this surface mid-pull keeps pulling.
		// It can refuse — a synthetic pointer has no capture to give —
		// and the drag is fine without it.
		try {
			(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
		} catch {
			// no capture; the listeners on the surface are enough
		}
	}

	// And EVERY move from here on, which is why it is not inside the
	// block above: the browser would otherwise select text under the
	// finger for the whole rest of the pull.
	event.preventDefault();
	track = [...track, { y: event.clientY, t: event.timeStamp }].filter(
		(sample) => event.timeStamp - sample.t <= WINDOW_MS
	);
	// ONE NUMBER, THREE PHASES. Nothing here knows which phase it is in:
	// past the ceiling the same pull becomes the list scrolling, and back
	// at the list's top the same pull becomes the bar closing ($lib/log.ts).
	moveTo(from.open + moved);
}

function onPointerUp(event: PointerEvent): void {
	if (!from || event.pointerId !== from.id) return;
	const from0 = from;
	from = null;
	if (!dragging) return unpause();
	dragging = false;
	settling = false;
	swallowUntil = event.timeStamp + SWALLOW_MS;
	swallowOn = from0.node;
	// Where it comes to rest is `settle`'s to say, and how fast the finger
	// was going when it let go is half of that answer ($lib/detent.ts).
	settle(from0.y - event.clientY > 0, speed(event.clientY, event.timeStamp));
}

function onPointerCancel(): void {
	if (!from) return;
	from = null;
	if (!dragging) return unpause();
	dragging = false;
	settling = false;
	// The swallow is deliberately untouched here. A cancel produces NO click
	// — that is what cancelling is — so there is nothing for this path to arm
	// against; and `swallowUntil` may still belong to a drag that really did
	// end a moment ago, whose own click has not arrived yet. Clearing it
	// would hand that drag's click to whatever it ended on, which is the bug
	// the swallow exists to prevent (canary on PR #59 asked for the opposite;
	// this is the reason it is the opposite).
	//
	// A gesture the system took away (a call, a notification) still has to
	// leave the bar somewhere it is ALLOWED to be, and it decided nothing —
	// so it settles as a release with no speed in it, heading down: back to
	// the row from above the notch, and shut from below it — unless it is
	// within a detent of the notch or the ceiling (da-0b2vy, $lib/detent.ts).
	settle(false);
}

/**
 * A wheel over the bar or the log opens it further, or less.
 *
 * The same one number a drag moves, so a trackpad and a finger agree —
 * and settled the same way, which is what puts the notch under a wheel
 * too. A notch's worth per event, so one flick of a mouse's wheel opens
 * the output exactly as the `^` does, and deep in the list it is a
 * comfortable line of history at a time.
 *
 * A DIAGONAL wheel is vertical: only `deltaY` is read, so a trackpad
 * gesture carrying both moves the bar by its vertical part and a
 * horizontal-only one is left alone — which is the prompt's own sideways
 * scroll, and not the bar's business (canary on PR #46).
 */
function onWheel(event: WheelEvent): void {
	if (event.timeStamp - foreignAt < INERTIA_MS || !event.deltaY) return;
	event.preventDefault();
	moveTo(at() + (event.deltaY > 0 ? notch() : -notch()));
	settle(event.deltaY > 0);
}

/** A drag declines to be a click; it is not an act of its own. */
function swallow(event: MouseEvent): void {
	if (event.timeStamp > swallowUntil || event.currentTarget !== swallowOn) return;
	swallowUntil = -Infinity;
	event.stopPropagation();
	event.preventDefault();
}

/** A wheel anywhere else is somebody else's, and its inertia is too. */
function elsewhere(event: WheelEvent): void {
	for (const node of gripped) if (node.contains(event.target as Node)) return;
	foreignAt = event.timeStamp;
}

/**
 * A handle on the bar: `use:grip` on the bar itself, and on its log.
 *
 * `gripped` is the reference count for the one window-level listener:
 * added as the set goes empty→1 and removed as it comes back to 0, so
 * destroying ONE of two surfaces correctly leaves it attached for the other
 * (canary on PR #59 read the pair as a leak; the set's own size is what
 * counts them).
 */
export function grip(node: HTMLElement) {
	if (gripped.size === 0)
		addEventListener('wheel', elsewhere, { capture: true, passive: true });
	gripped.add(node);
	node.addEventListener('wheel', onWheel, { passive: false });
	node.addEventListener('pointerdown', onPointerDown);
	node.addEventListener('pointermove', onPointerMove, { passive: false });
	node.addEventListener('pointerup', onPointerUp);
	node.addEventListener('pointercancel', onPointerCancel);
	node.addEventListener('click', swallow, { capture: true });
	return {
		destroy() {
			node.removeEventListener('wheel', onWheel);
			node.removeEventListener('pointerdown', onPointerDown);
			node.removeEventListener('pointermove', onPointerMove);
			node.removeEventListener('pointerup', onPointerUp);
			node.removeEventListener('pointercancel', onPointerCancel);
			node.removeEventListener('click', swallow, { capture: true });
			gripped.delete(node);
			if (gripped.size === 0) removeEventListener('wheel', elsewhere, { capture: true });
		}
	};
}
