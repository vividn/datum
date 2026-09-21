// THE BAR'S FEEL, AS A LIVE PAIR OF NUMBERS (da-o2wm8).
//
// `TAU` and `DETENT` decide how the bottom bar feels, and `$lib/detent.ts`
// says in its own words why nobody here can judge them: "vremya's
// instrument adds latency to a synthetic touch, so a velocity measured in
// a drive is approximate, and where a real thumb lands relative to a
// boundary is a thing only a real thumb says." A synthetic drag cannot
// answer the question. A person with a phone can, in about thirty seconds,
// IF CHANGING THE NUMBER COSTS THEM NOTHING — which until now meant an
// edit, a build, a deploy and a reload, and is why neither number has
// moved since it was written.
//
// So this is the override: one store, read at the moment a release is
// settled (`settle` in $lib/log.ts), written by the debug widget's `bar`
// tool ($lib/DebugBar.svelte). Nothing here is ambient state the RULE
// reads — `rest()` stays pure and takes its tuning as an argument, which
// is what keeps `tests/detent.test.mjs` answerable in node.
//
// PER DEVICE, in localStorage, the way `OUTPUT_KEY` already keeps the
// fold: "never synced: it is a screen's", and a feel is a screen's too.
// UNSET BY DEFAULT: an untouched install has nothing stored and uses the
// exported constants exactly. And a blocked or throwing store costs the
// visit and nothing more.
//
// This reaches the deployed site, and that is correct: datum.day on a real
// phone is the only place the question can be answered. The safety is not
// a build flag — that would put the instrument out of reach of the one
// place it is needed — it is that the override is OFF until set and one
// tap from off again.

import { get, writable } from 'svelte/store';
import { DEFAULT_TUNING, type Tuning } from './detent.js';

/** Where the feel is remembered, per device (never synced: it is a screen's). */
export const TUNING_KEY = 'datum.bar.tuning.v1';

/**
 * How far each knob goes.
 *
 * Wide enough that the ends are boring, so the good value is never at an
 * edge — and `detent: 0` reachable, because "no detents at all" is a feel
 * that has to be tryable.
 *
 * ⚑ HERE, beside the clamp, and not in the widget's markup (canary on
 * PR #69). Every way in goes through `clamped` below — a slider, a typed
 * number, a pair restored from a store another build wrote — so a stored
 * `{ tau: 99999 }` cannot come back and sit in a number box outside that
 * box's own min and max. The widget DRAWS its inputs from these, so what
 * is offered and what is enforced cannot drift apart.
 */
export const RANGE: Record<keyof Tuning, { min: number; max: number }> = {
	tau: { min: 60, max: 1200 },
	detent: { min: 0, max: 80 }
};

/** One number, inside its range — or `fallback` when it is not a number at all. */
const num = (value: unknown, fallback: number, { min, max }: { min: number; max: number }): number =>
	typeof value === 'number' && Number.isFinite(value) ? Math.max(min, Math.min(max, value)) : fallback;

/** A whole pair, inside both ranges; anything unusable falls back to `from`. */
const clamped = (pair: Partial<Tuning>, from: Tuning = DEFAULT_TUNING): Tuning => ({
	tau: num(pair.tau, from.tau, RANGE.tau),
	detent: num(pair.detent, from.detent, RANGE.detent)
});

/** What this device last chose, or the constants where nothing was chosen. */
function kept(): Tuning {
	try {
		const raw = localStorage.getItem(TUNING_KEY);
		if (!raw) return DEFAULT_TUNING;
		return clamped(JSON.parse(raw) as Partial<Tuning>);
	} catch {
		// Blocked, absent (the prerender), or half-written: the defaults.
		return DEFAULT_TUNING;
	}
}

/** The live pair. Read at every release, so a change lands on the next throw. */
export const tuning = writable<Tuning>(kept());

/** The pair as a plain value, for the imperative side of $lib/log.ts. */
export const tuningNow = (): Tuning => get(tuning);

/** Move one knob, or both, and remember it. Out-of-range is brought in. */
export function setTuning(next: Partial<Tuning>): void {
	tuning.update((was) => {
		const now = clamped({ ...was, ...next }, was);
		try {
			localStorage.setItem(TUNING_KEY, JSON.stringify(now));
		} catch {
			// Unremembered is not unusable: it lasts the visit.
		}
		return now;
	});
}

/** Back to the constants, and back to storing nothing at all. */
export function resetTuning(): void {
	try {
		localStorage.removeItem(TUNING_KEY);
	} catch {
		// Nothing was stored anyway.
	}
	tuning.set(DEFAULT_TUNING);
}
