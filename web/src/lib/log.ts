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
//   reveal          0 ─────────── OUT ──────────────────────── CEILING
//   the output      opening 0→OUT  fully open  ─────────────────────────
//   the log                        0 ────────────── growing ──────────
//   the dashboard   the rest of the page, shrinking as those two grow
//
// `OUT` is the output section's own height, so the point where the output
// has just finished opening is a place on the line — the NOTCH. Where a
// release comes to rest on the line is $lib/detent.ts.
//
// AND THE LINE DOES NOT STOP AT THE CEILING (da-tj6m). vividn, 2026-09-21:
// "you should be able to grab from anywhere in the repl and it behaves as
// one unit, so it will just pull the input section up more. When and only
// when the input bar reaches the top of the screen, does the REPL then
// scroll all together (including the most recent command). […] When I get
// back to the top, then trying to scroll up further starts closing the
// input section." Three phases, in sequence, inside ONE drag — so they are
// three slices of the ONE NUMBER and nothing hands over to anything:
//
//   reveal   0 ─── OUT ─── CEILING ─────────────────── CEILING+OVERHANG
//   phase    the bar opening │ the LIST scrolling under a pinned input row
//
// Past the ceiling the bar cannot open further, so the pull becomes the
// list moving: first the bar's OWN row rolls off the top (`rolled`, which
// is what makes "the most recent command" part of the scroll — it is drawn
// by the output section, not by the log), and then the log's box scrolls
// (`scrolled`). The log's box is scrolled BY THIS NUMBER and never by the
// browser: its own vertical scroll is inert, which is what stopped rows
// 2-N from sliding about under a finger that meant to open the bar.
//
// THE PAGE STILL NEVER SCROLLS. `routes/+layout.svelte` sizes three boxes
// from this one number; nothing here moves a scrollport, so a gesture that
// does not start on the bar still cannot move the page. That was the bead's
// rule and it is unchanged.

import { derived, get, writable } from 'svelte/store';
import { TAU, rest } from './detent.js';
import { PHONE_WIDTH } from './layout/defaults.js';

/** Where the fold is remembered, per device (never synced: it is a screen's). */
export const OUTPUT_KEY = 'datum.bar.output.v1';

/** How long a settle takes when nothing threw it — see `glide`. */
const SETTLE_MS = 220;

/**
 * A glide that was thrown is never quicker than this, nor slower than that.
 *
 * The slow end is `3 * TAU`, which is what a release carrying its own
 * momentum and meeting no detent asks for: `glide` sets the duration so the
 * curve leaves at the finger's speed, and for a free glide that arithmetic
 * lands exactly there. Clamping it any nearer would put a step back into
 * the one thing vividn asked never to step — "It is jarring if it suddenly
 * changes speed".
 */
const GLIDE_MIN_MS = 140;
const GLIDE_MAX_MS = 3 * TAU;

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

/**
 * The log's scrolling box, once the log has mounted.
 *
 * Held here and not in the component because the ONE NUMBER owns the
 * scroll: its extent is how far the number goes, and its `scrollTop` is a
 * slice of the number. A live read of `scrollHeight` rather than a store,
 * because the list grows with every command and a number that has to be
 * right at the moment a finger asks is a number to read then.
 *
 * A LIST THAT CHANGES UNDER A GESTURE is therefore read again on the next
 * move and on the release, never carried over from where the gesture began.
 * A list that GREW simply lets the number go further; one that SHRANK
 * clamps it back, which is the only honest answer — there is no longer a
 * list down there to be scrolled to. The command that changes it is the
 * reader's own, and `toNewest` has already brought the list back to its
 * newest row by then (canary on PR #59).
 */
let scroller: HTMLElement | null = null;

/** The log hands over its box; `null` when it goes away. */
export const trackLog = (el: HTMLElement | null): void => {
	scroller = el;
};

/** The rows BELOW the bar's own, in px — nothing until the log has mounted. */
const content = (): number => scroller?.scrollHeight ?? 0;

/**
 * As far as the number goes: the bar fully open, plus the list's OVERHANG.
 *
 * The list is the bar's own row and then the log's rows, and the window it
 * is read through is the page less the input row — the ceiling. Whatever
 * the list is taller than that window by is how far it can be scrolled, so
 * a history that fits has no phase 2 at all and the ceiling is the end.
 */
const maxReveal = (): number => {
	const c = get(ceilingH);
	return c + Math.max(0, content() + out() - c);
};

/** How far open the bar is, in px: 0 shut, `OUT` the output alone, more is log. */
export const reveal = writable(0);

/** How far the bar itself is open — the number, with the scroll taken off. */
const opened = derived([reveal, ceilingH], ([r, c]) => Math.min(r, c));

/**
 * How much of the bar's OWN row has rolled off the top of the screen, in px.
 *
 * The first `OUT` px of the scroll. "the REPL then scroll all together
 * (including the most recent command)" — the most recent command is the
 * output section's row, and the output section is in the bar, above the
 * log. So the two surfaces scroll as one list by this one number: the row
 * rides up under the input as the section it is in gives its height to the
 * log below it, and the seam between them simply moves.
 */
export const rolled = derived([reveal, ceilingH, outH], ([r, c, o]) =>
	Math.min(Math.max(0, r - c), o)
);

/** The output section's height right now — it opens as the first `OUT` px. */
export const shown = derived([opened, outH, rolled], ([r, o, k]) => Math.max(0, Math.min(r, o) - k));

/** The log's height right now — everything past the notch, and what the row gave. */
export const logHeight = derived([opened, outH, rolled], ([r, o, k]) => Math.max(0, r - o) + k);

/** The log box's own `scrollTop`: the scroll left once the bar's row is gone. */
export const scrolled = derived([reveal, ceilingH, outH], ([r, c, o]) => Math.max(0, r - c - o));

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
 * All of the rule is `$lib/detent.ts` — momentum at any speed, three
 * detents that catch, and a coast that snags at a threshold it came from
 * above. What is left here is the two numbers the table needs and the
 * curve that carries the bar there.
 */
export function settle(up: boolean, speed = 0): void {
	glide(rest(get(reveal), speed, up, out(), get(ceilingH)), speed);
}

/**
 * Back to the newest row, with no glide.
 *
 * A command that lands is row one of the list, so the list comes back to
 * it — the same pin `Repl.svelte` does after a run, said to the number
 * that owns the scroll. How far the bar is OPEN is untouched: a reader
 * with the log pulled up keeps it pulled up.
 */
export const toNewest = (): void => reveal.update((r) => Math.min(r, get(ceilingH)));

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
 * the motion. Clamped at both ends — see `GLIDE_MAX_MS`.
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
		? Math.min(GLIDE_MAX_MS, Math.max(GLIDE_MIN_MS, (3 * Math.abs(to - from)) / Math.abs(speed)))
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
