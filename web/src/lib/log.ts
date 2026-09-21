// How far the bar is OPEN (da-ckw3), as one number of pixels.
//
// vividn, 2026-09-18, on the first build: "when output section is closed,
// swiping or dragging up a bit should snap to the single output bar open,
// similarly swiping or dragging down when output section is open should
// close it again, similar to hitting ^ toggle. You should be able to open
// the REPL section as far as you want, not to full screen, for example just
// to see a few rows without it filling the whole screen. Swiping up and down
// should change the height of this open REPL until it fills the whole screen
// and then it start scrolling. […] Make sure there is smooth acceleration.
// It is jarring if it suddenly changes speed at various, though it should be
// snappy at the one output section open, like a nice notch."
//
// So the bar is not two states with a scroll between them any more. It is
// ONE CONTINUOUS NUMBER — `reveal`, in px — and everything is a slice of it:
//
//   reveal          0 ─────────── OUT ──────────────────── OUT + MAXLOG
//   the output      opening 0→OUT  fully open  ─────────────────────────
//   the log                        0 ────────────── growing ──── MAXLOG
//   the dashboard   the rest of the page, shrinking as those two grow
//
// `OUT` is the output section's own height, so the point where the output
// has just finished opening is a place on the line — the NOTCH. A release
// below it goes to whichever end is nearer, which is the same thing the `^`
// does; a release above it stays where the finger left it, because "as far
// as you want" is the whole point. Past MAXLOG the dashboard is gone and
// there is nothing left to give, so the log's own pane scrolls instead
// (Pane.svelte's body, which it has always done).
//
// THE PAGE STILL NEVER SCROLLS. `routes/+layout.svelte` sizes three boxes
// from this one number; nothing here moves a scrollport, so a gesture that
// does not start on the bar still cannot move the page. That was the bead's
// rule and it is unchanged.

import { derived, get, writable } from 'svelte/store';
import { PHONE_WIDTH } from './layout/defaults.js';

/** Where the fold is remembered, per device (never synced: it is a screen's). */
export const OUTPUT_KEY = 'datum.bar.output.v1';

/**
 * How near the notch a release still snaps to it, in px.
 *
 * vividn: "snappy at the one output section open, like a nice notch". Above
 * the notch the height is the reader's to choose, so this is the only place
 * a release is taken away from them, and it is small enough that choosing a
 * height just above the output is still possible.
 */
const DETENT = 24;

/** How long a settle takes when nothing threw it — see `glide`. */
const SETTLE_MS = 220;

/**
 * The speed, in px/ms, at which a release stops being a let-go and becomes a
 * FLING — a gesture that says where it was going rather than where it ended.
 *
 * MEASURED, not chosen. It was 0.5 px/ms, and vremya drove three real
 * gestures through it on a Pixel-shaped phone: a slow pull reads 0.33, a
 * deliberate 200px pull over 240ms reads 0.83, and a flick reads 1.67. At
 * 0.5 that middle one — a brisk "open it a bit" — went to the ceiling, which
 * is the opposite of the instruction it has to share the bar with: "You
 * should be able to open the REPL section as far as you want, not to full
 * screen, for example just to see a few rows" (vividn, 2026-09-18). A floor
 * that swallows a deliberate pull makes "as far as you want" unreachable for
 * anyone whose thumb is not slow.
 *
 * 1.2 px/ms sits between the pull and the flick with room either side, so a
 * gesture that MEANS a height keeps it and one that means "away" is obeyed.
 * If a real thumb still finds the bar leaping, this is the number to move.
 */
const FLING = 1.2;

/** A flung glide is never quicker than this, nor slower than that. */
const FLING_MIN_MS = 140;
const FLING_MAX_MS = 520;

const reduced = (): boolean =>
	typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

/**
 * The two MEASURED numbers: the output section's height and how far the bar
 * opens before the log's own pane takes over.
 *
 * Stores and not plain `let`s, because the three heights below are derived
 * from them as well as from `reveal` — and a Svelte `derived` recomputes
 * only when one of its own inputs changes. Read off a closed-over `let`,
 * `out` could change (a rotation, a `--output-height` a stylesheet moved)
 * and `shown` would go on publishing the height it had until the next time
 * a finger moved the bar (canary on PR #46). A number equal to the one
 * before it does not notify, so re-measuring could not have poked them
 * either.
 */
const outH = writable(0);
const ceilingH = writable(0);

/** The same two as plain numbers, for the imperative side of this file. */
const out = (): number => get(outH);
const maxReveal = (): number => get(ceilingH);

/** How far open the bar is, in px: 0 shut, `OUT` the output alone, more is log. */
export const reveal = writable(0);

/** The output section's height right now — it opens as the first `OUT` px. */
export const shown = derived([reveal, outH], ([r, o]) => Math.min(Math.max(r, 0), o));

/** The log's height right now — everything past the notch. */
export const logHeight = derived([reveal, outH], ([r, o]) => Math.max(0, r - o));

/** The output section is open (the `^`'s state, and what is remembered). */
export const output = derived([reveal, outH], ([r, o]) => r >= o - 0.5 && o > 0);

/** The log is showing at all: `close history` is up and Escape closes. */
export const inLog = derived(logHeight, (h) => h > 0.5);

/**
 * Whether the log has been asked for yet.
 *
 * The scrollback under the bar costs a render per command, and until a
 * finger or a wheel has actually PULLED the bar nobody has ever seen it. So
 * the REPL down there mounts on the first real gesture — a wheel, or a drag
 * past its threshold, never a tap — and stays.
 */
export const armed = writable(false);

export const arm = (): void => armed.set(true);

/** The notch, in px: where the output has just finished opening. */
export const notch = (): number => out();

/** As far as the bar opens before the log's own pane takes over. */
export const ceiling = (): number => maxReveal();

/** Where the bar is now, in px. */
export const at = (): number => get(reveal);

/**
 * `+layout.svelte` hands over the page's box.
 *
 * What is measured here is the output section's height and the page's, both
 * of which are CSS's to decide (`--output-height` on the shell) and neither
 * of which this file should hard-code. Re-measured whenever the box changes,
 * because a rotation changes what "as far as it opens" means.
 */
export function attach(el: HTMLElement): () => void {
	const probe = document.createElement('div');
	probe.style.cssText = 'position:absolute;visibility:hidden;height:var(--output-height)';
	const remeasure = (): void => {
		el.append(probe);
		const height = probe.getBoundingClientRect().height;
		probe.remove();
		outH.set(height);
		const bar = el.querySelector('.prompt-bar');
		const rule = bar ? parseFloat(getComputedStyle(bar).borderTopWidth) || 0 : 0;
		// The input row's own box, read from the DOM. It used to be the bar's
		// height less what is open, which is the same number by arithmetic and
		// one more thing that has to be true at the moment it is read (canary
		// on PR #46). The box knows its own height.
		const input = bar?.querySelector('.section.input')?.getBoundingClientRect().height ?? 0;
		ceilingH.set(Math.max(height, el.clientHeight - input - rule));
		// A shorter page cannot hold what a taller one did.
		reveal.update((r) => Math.min(r, maxReveal()));
	};
	remeasure();
	// The remembered fold, now that `out` is known. Before `observe`, which
	// is safe because a ResizeObserver delivers a first observation of its
	// own accord: a box that changed between these two lines is measured
	// again by that callback, and `remeasure` clamps `reveal` to it.
	reveal.set(remembered() ? out() : 0);
	const observer = new ResizeObserver(remeasure);
	observer.observe(el);
	// Every observer this makes is disconnected by the teardown of the same
	// call. There is no module-level handle to the element on purpose —
	// nothing here reads one, and a second `attach` before the first's
	// teardown would then be the only way to leak an observer (canary on
	// PR #46, twice; the variable it named was written and never read).
	return () => observer.disconnect();
}

/** The fold this device last chose; a phone starts shut, anything wider open. */
function remembered(): boolean {
	try {
		const kept = localStorage.getItem(OUTPUT_KEY);
		if (kept === 'open' || kept === 'folded') return kept === 'open';
	} catch {
		// A browser with storage off gets the device's default, every visit.
	}
	return typeof innerWidth === 'number' ? innerWidth > PHONE_WIDTH : true;
}

/** Remember the fold, but only as the two states the `^` speaks of. */
function remember(open: boolean): void {
	try {
		localStorage.setItem(OUTPUT_KEY, open ? 'open' : 'folded');
	} catch {
		// Unremembered is not unusable.
	}
}

/** Follow a finger: no animation, no snapping — the bar is under it. */
export function moveTo(px: number): void {
	stop();
	// The finger is placing the bar itself, so wherever a glide was aiming is
	// no longer where this gesture is going.
	aim = null;
	reveal.set(Math.max(0, Math.min(maxReveal(), px)));
}

/**
 * Come to rest after a gesture that went `up` (or did not), at `speed`.
 *
 * A FLUNG DRAWER KEEPS GOING. vividn, 2026-09-18: "There should be
 * acceleration on the REPL drawer you're pulling out. So if you let go with
 * velocity, it should keep moving. So a big swipe will fully close or open
 * the bar. When closing snag at the notch so that the output section stays
 * open, but you can drag over this notch if you like."
 *
 * So a release is read two ways and the speed decides which:
 *
 * - FLUNG (`speed` past `FLING`, in px/ms): it goes all the way. Up is the
 *   whole page. Down SNAGS AT THE NOTCH — a fling from a tall log leaves the
 *   output open, and it takes a second fling from there to shut it. That is
 *   the snag, and it is why a fling can still close the bar: notch, then 0.
 * - LET GO (slow, or at rest): the old rules, which are what "you can drag
 *   over this notch if you like" means — a deliberate drag that ENDS below
 *   the notch closes, however it got there. Below the notch the DIRECTION
 *   decides and not the midpoint ("dragging up a bit should snap to the
 *   single output bar open… similar to hitting ^ toggle" — and a toggle does
 *   not ask how far you pushed it). Above it the height is the reader's,
 *   unless the release is inside `DETENT` of the notch.
 *
 * Landing ON the notch stays on it either way: one wheel up from a log
 * exactly one notch tall lands the number on `out`, and the direction rule
 * would read that as "shut it", closing an output the gesture never touched
 * (vremya measured it: shown 58 log 58 → 0, 0).
 */
export function settle(up: boolean, speed = 0): void {
	const now = get(reveal);
	const o = out();
	if (Math.abs(now - o) <= 0.5) return glide(o, speed);
	if (Math.abs(speed) >= FLING) {
		if (up) return glide(maxReveal(), speed);
		return glide(now > o ? o : 0, speed);
	}
	if (now < o) return glide(up ? o : 0);
	if (now - o <= DETENT) return glide(o);
	remember(true);
}

/**
 * THE ARROW: open the output, or shut it.
 *
 * vividn, 2026-09-21: "tapping the arrow should open/close the output section".
 * It was a button, then a mark, and it is a button again — what he asked for
 * on 2026-09-18 was that pressing it must not FOCUS the input ("That is
 * jarring"), and removing the press took more than that away. The focus rule
 * stands and lives in PromptBar's handler; this is only where it goes.
 *
 * From inside the LOG it collapses to the notch rather than shutting, so the
 * two gestures agree: a fling down snags at the notch too, and a second press
 * closes from there.
 */
export function toggleOutput(): void {
	glide(get(output) && get(logHeight) === 0 ? 0 : out());
}

/** Open at least as far as the output — a tap on a failure's `ERROR`. */
export const openOutput = (): void => {
	if (get(reveal) < out()) glide(out());
};

/** Shut the log but leave the output where the `^` last put it. */
export const closeLog = (): void => glide(get(reveal) > out() ? out() : 0);

let frame = 0;
/**
 * Where the glide in flight was going, or null when none is.
 *
 * A press stops a glide — a finger wins over an animation — and then owes the
 * bar an ending. That ending is THIS, not "shut": an interrupted glide was
 * already on its way somewhere the reader asked for, and a tap that lands
 * during it is not a request to go back. `resume` used to settle as a release
 * with no speed, which below the notch means 0 — so a tap on the box inside
 * the 220ms after tapping ERROR closed the output that tap had just opened
 * (canary on PR #46).
 */
let aim: number | null = null;

/** Whether a settle is in flight — a press that stops one owes it an end. */
export const moving = (): boolean => frame !== 0;

/** Send a stopped glide on to where it was going. Nothing, if none was. */
export function resume(): void {
	if (aim !== null) glide(aim);
}

/** Stop any settle in flight, so a finger always wins over an animation. */
export function stop(): void {
	if (frame) cancelAnimationFrame(frame);
	frame = 0;
}

/**
 * Animate to `target` on one curve, start to finish.
 *
 * vividn: "It is jarring if it suddenly changes speed". So every settle is one
 * ease-out — the speed falls away smoothly and never steps — and a new one
 * cancels the old rather than fighting it.
 *
 * `speed` is how fast the finger was going when it let go, in px/ms, and it
 * sets the DURATION so that the curve leaves at that same speed: an ease-out
 * cubic starts at 3·distance/duration, so duration = 3·distance/speed is the
 * one that carries on from where the finger stopped instead of restarting
 * the motion. Clamped, because a barely-moving fling should not take a
 * second and a violent one should still be watchable.
 */
function glide(target: number, speed = 0): void {
	stop();
	const from = get(reveal);
	const to = Math.max(0, Math.min(maxReveal(), target));
	remember(to >= out() - 0.5);
	if (reduced() || from === to) {
		aim = null;
		return reveal.set(to);
	}
	const ms = speed
		? Math.min(FLING_MAX_MS, Math.max(FLING_MIN_MS, (3 * Math.abs(to - from)) / Math.abs(speed)))
		: SETTLE_MS;
	aim = to;
	const started = performance.now();
	const step = (now: number): void => {
		const t = Math.min(1, (now - started) / ms);
		// ease-out cubic: fastest at the start, arriving without a stop.
		reveal.set(from + (to - from) * (1 - (1 - t) ** 3));
		if (t < 1) {
			frame = requestAnimationFrame(step);
		} else {
			frame = 0;
			aim = null;
		}
	};
	frame = requestAnimationFrame(step);
}
