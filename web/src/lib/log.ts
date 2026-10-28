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
// release comes to rest on the line is #lib/detent.ts.
//
// THE BAR OPENS UPWARD (da-mwp4m). vividn, 2026-10-27: «keep the input bar
// on the bottom, and put the output/return bar above it again». So the input
// row stays at the foot of the page and what the number opens is ABOVE it:
// the output section first, then the log over that, growing up into the
// dashboard until the dashboard is gone at the ceiling. The line stops
// there. The log scrolls on its own, like a terminal's history, newest at
// the bottom next to the input.
//
// AND THE BAR RESTS IN THREE STATES (da-0b2vy, #lib/detent.ts): CLOSED at 0,
// the SINGLE ROW at the notch, and FULLSCREEN at the ceiling. Full, the strip
// gives way: the output section is 0 and the log takes its room, so the
// newest command is the log's last line, next to the input, and the commands
// that were side by side in the strip FLOW to their lines (`flow`). vividn,
// 2026-10-27: «Entries animate and flow to be vertically chronological, like
// it looks like in a cli … each command get's it's own line».
//
// THE PAGE STILL NEVER SCROLLS. `routes/+layout.svelte` sizes three boxes
// from this one number; nothing here moves a scrollport, so a gesture that
// does not start on the bar still cannot move the page. That was the bead's
// rule and it is unchanged.

import { tick } from 'svelte';
import { derived, get, writable } from 'svelte/store';
import { rest } from './detent.js';
import { tuningNow } from './tuning.js';

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
 *
 * ⛔ A FUNCTION AND NOT A CONSTANT (da-o2wm8). It used to be `const
 * GLIDE_MAX_MS = 3 * TAU`, computed once at import from the compile-time
 * `TAU` — so a reader who raised `TAU` to 900 with the debug widget still
 * had every glide clamped at 960ms by a ceiling derived from 320, and the
 * bar did not do what the number said. A wrong answer that looks like a
 * measurement is worse than no instrument.
 */
const GLIDE_MIN_MS = 140;
const glideMax = (): number => 3 * tuningNow().tau;

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

/** How far open the bar is, in px: 0 shut, `OUT` the output alone, more is log. */
export const reveal = writable(0);

/**
 * Whether this page draws no output section at all: a root tab that is the
 * CLI widget alone (`/cli`, `/repl`). vividn, 2026-10-27: «the output bar is
 * then completely gone on this full screen widget, since it basically
 * becomes the output section». Every height below is 0 while it is.
 */
export const outputless = writable(false);

/** How far the bar itself is open, as far as the ceiling and no further. */
const opened = derived([reveal, ceilingH, outputless], ([r, c, none]) => (none ? 0 : Math.min(r, c)));

/** FULLSCREEN: the bar at the ceiling, the history to the top of the page. */
export const full = derived([opened, outH, ceilingH], ([r, o, c]) => c > o && r >= c - 0.5);

/** The output section's height right now — it opens as the first `OUT` px, and is gone full. */
export const shown = derived([opened, outH, full], ([r, o, f]) => (f ? 0 : Math.min(r, o)));

/** The log's height right now — everything past the notch, and the whole bar full. */
export const logHeight = derived([opened, outH, full], ([r, o, f]) => (f ? r : Math.max(0, r - o)));

/** The output section is open (the `^`'s state, and what is remembered). */
export const output = derived([opened, outH], ([r, o]) => r >= o - 0.5 && o > 0);

/** The log is showing at all: `close history` is up and Escape closes. */
export const inLog = derived(logHeight, (h) => h > 0.5);

/**
 * Whether the log has been asked for yet.
 *
 * The scrollback over the bar costs a render per command, and until the
 * bar has opened PAST THE NOTCH nobody has ever seen it. So the CLI up
 * there mounts the first time it does (`put`) — a drag, a wheel or a
 * key — and stays.
 */
export const armed = writable(false);

const arm = (): void => {
	// A page with no output section has no log to mount: the widget is the
	// output there, and a gesture on its bar must not pay for a second
	// scrollback nobody can open (canary-qwen on #1256).
	if (!get(outputless)) armed.set(true);
};

/** The notch, in px: where the output has just finished opening. */
export const notch = (): number => out();

/** As far as the bar opens: the dashboard gone, the log up to the top. */
const maxReveal = (): number => get(ceilingH);

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
		// The notch MOVES with the page (`--output-height` is a share of it,
		// da-mwp4m), and so does the ceiling: the bar keeps its STATE, rather
		// than being left part-way at the old numbers.
		const state = get(full) ? 'full' : get(output) ? 'row' : null;
		outH.set(height);
		const bar = el.querySelector('.prompt-bar');
		const rule = bar ? parseFloat(getComputedStyle(bar).borderTopWidth) || 0 : 0;
		// The input row's own box, read from the DOM. It used to be the bar's
		// height less what is open, which is the same number by arithmetic and
		// one more thing that has to be true at the moment it is read (canary
		// on PR #46). The box knows its own height.
		const input = bar?.querySelector('.section.input')?.getBoundingClientRect().height ?? 0;
		ceilingH.set(Math.max(height, el.clientHeight - input - rule));
		if (state) reveal.set(state === 'full' ? maxReveal() : height);
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

/**
 * The fold this device last chose; open, on every screen, until it chose.
 *
 * A phone started folded until vividn, 2026-10-27: «make the output section
 * open by default, always above the input bar».
 */
function remembered(): boolean {
	try {
		const kept = localStorage.getItem(OUTPUT_KEY);
		if (kept === 'open' || kept === 'folded') return kept === 'open';
	} catch {
		// A browser with storage off gets the default, every visit.
	}
	return true;
}

/** Remember the fold, but only as the two states the `^` speaks of. */
function remember(open: boolean): void {
	try {
		localStorage.setItem(OUTPUT_KEY, open ? 'open' : 'folded');
	} catch {
		// Unremembered is not unusable.
	}
}

/**
 * Set the bar, and when that moves it into or out of the full screen, let
 * the commands FLOW to where they are drawn now.
 *
 * Measured here because here is the one moment both places are known: the
 * store has changed and the page has not yet (Svelte draws on a microtask),
 * so what is on screen is still where each command WAS. After the draw, each
 * one drawn again — in the log or in the strip, by its `data-ran` — plays
 * the move back from there (FLIP). Under `prefers-reduced-motion` the bar
 * changes state with no flow.
 */
function put(px: number): void {
	if (px > out() + 0.5) arm();
	const was = get(full);
	reveal.set(px);
	if (get(full) !== was && !reduced()) flow(was);
}

/** How long the commands take to flow to their new places, in ms. */
const FLOW_MS = 300;

/** Each command on screen, by when it ran: the strip's over the log's when both draw it. */
function places(): Map<string, DOMRect> {
	const seen = new Map<string, DOMRect>();
	for (const box of document.querySelectorAll('.log, .prompt-bar .roll')) {
		const frame = box.getBoundingClientRect();
		for (const el of box.querySelectorAll<HTMLElement>('[data-ran]')) {
			const at = el.getBoundingClientRect();
			if (at.bottom > frame.top && at.top < frame.bottom && at.right > frame.left && at.left < frame.right)
				seen.set(el.dataset.ran!, at);
		}
	}
	return seen;
}

function flow(wasFull: boolean): void {
	const before = places();
	// Leaving the full screen, the log's copy of a command still in the strip
	// would otherwise claim it: the strip is where the eye follows it to.
	const into = wasFull ? '.prompt-bar .roll [data-ran], .log [data-ran]' : '.log [data-ran]';
	void tick().then(() => {
		for (const el of document.querySelectorAll<HTMLElement>(into)) {
			const was = before.get(el.dataset.ran!);
			if (!was) continue;
			const now = el.getBoundingClientRect();
			// Foot to foot: a command stands on its foot in both places.
			const dx = was.left - now.left;
			const dy = was.bottom - now.bottom;
			if (Math.abs(dx) + Math.abs(dy) < 1) continue;
			el.animate([{ transform: `translate(${dx}px, ${dy}px)` }, { transform: 'none' }], {
				duration: FLOW_MS,
				easing: 'ease-out'
			});
		}
	});
}

/** Follow a finger: no animation, no snapping — the bar is under it. */
export function moveTo(px: number): void {
	// A page with no output section has no bar to move, and a gesture there
	// must not write the fold every other page opens with.
	if (get(outputless)) return;
	stop();
	// The finger is placing the bar itself, so wherever a glide was aiming is
	// no longer where this gesture is going.
	aim = null;
	put(Math.max(0, Math.min(maxReveal(), px)));
}

/**
 * Come to rest after a gesture that went `up` (or did not), at `speed`.
 *
 * All of the rule is `#lib/detent.ts` — momentum at any speed, a notch and
 * a ceiling that catch, a direction that decides below the notch, and a
 * coast that snags at a threshold it came from above. What is left here is
 * the two numbers the table needs and the curve that carries the bar there.
 */
export function settle(up: boolean, speed = 0): void {
	glide(rest(get(reveal), speed, up, out(), get(ceilingH), tuningNow()), speed);
}

/** PageUp on the box (da-pnxug): closed opens the row, the row goes full; `closeLog` is PageDown. */
export const raise = (): void => glide(get(output) ? maxReveal() : out());

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
 * the motion. Clamped at both ends — see `glideMax`.
 */
function glide(target: number, speed = 0): void {
	if (get(outputless)) return;
	stop();
	const from = get(reveal);
	const to = Math.max(0, Math.min(maxReveal(), target));
	remember(to >= out() - 0.5);
	if (reduced() || from === to) {
		aim = null;
		return put(to);
	}
	const ms = speed
		? Math.min(glideMax(), Math.max(GLIDE_MIN_MS, (3 * Math.abs(to - from)) / Math.abs(speed)))
		: SETTLE_MS;
	aim = to;
	const started = performance.now();
	const step = (now: number): void => {
		const t = Math.min(1, (now - started) / ms);
		// ease-out cubic: fastest at the start, arriving without a stop.
		put(from + (to - from) * (1 - (1 - t) ** 3));
		if (t < 1) {
			frame = requestAnimationFrame(step);
		} else {
			frame = 0;
			aim = null;
		}
	};
	frame = requestAnimationFrame(step);
}
