// WHERE A RELEASE COMES TO REST, in node, with no browser (da-tj6m).
//
// `#lib/detent.ts` is the bar's whole momentum rule and it is pure, so the
// table at its head can be asked directly rather than thrown at a phone.
// What it CANNOT settle is the numbers: vremya's instrument adds latency to
// a synthetic touch, so a velocity measured in a drive is approximate, and
// where a real thumb lands relative to a boundary is a thing only a real
// thumb says. What is asserted here is the SHAPE — that every release rests
// closed, at the row or full (da-0b2vy), that the notch and the ceiling
// catch, that between two states the gesture's direction decides, and that
// a coast snags at an edge it came from while a finger may be dragged
// through it.
//
// The same rules as a reader meets them are `barlog.test.mjs`.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';

registerHooks({
	resolve(specifier, context, next) {
		if (/^\.\/.*\.js$/.test(specifier) && context.parentURL?.includes('/src/lib/'))
			return next(specifier.replace(/\.js$/, '.ts'), context);
		return next(specifier, context);
	}
});

const { DEFAULT_TUNING, DETENT, TAU, rest } = await import('../src/lib/detent.ts');

/** A phone's numbers: the output section is 58px and the page leaves 600. */
const NOTCH = 58;
const CEILING = 600;
const at = (now, speed = 0, up = speed >= 0) => rest(now, speed, up, NOTCH, CEILING);

describe('three states and nothing between (da-0b2vy)', () => {
	// vividn, 2026-10-27: «Clicking or swiping up on this makes it full
	// screen». So every release comes to rest closed, at the row or full.
	test('every release, at every place and speed, rests in one of the three', () => {
		for (let now = 0; now <= CEILING; now += 7)
			for (let v = -4; v <= 4; v += 0.25)
				for (const up of [true, false]) {
					const to = at(now, v, up);
					assert.ok([0, NOTCH, CEILING].includes(to), `${now}px at ${v} px/ms (${up ? 'up' : 'down'}) rested at ${to}`);
				}
	});

	test('above the notch the gesture’s direction picks: up is full, down is the row', () => {
		assert.equal(at(300, 0, true), CEILING, 'a swipe up from the row did not go full');
		assert.equal(at(300, 0, false), NOTCH, 'a swipe down from full did not come back to the row');
	});

	test('a drag shorter than a detent puts the bar back where it was', () => {
		assert.equal(at(NOTCH + DETENT, 0, true), NOTCH);
		assert.equal(at(CEILING - DETENT, 0, false), CEILING);
		assert.equal(at(NOTCH + DETENT + 1, 0, true), CEILING, 'the detent reached too far');
	});

	test('landing exactly ON the notch stays on it, whichever way the gesture went', () => {
		// vremya measured the one this is about: shown 58, log 58 → 0, 0.
		assert.equal(at(NOTCH, 0, true), NOTCH);
		assert.equal(at(NOTCH, 0, false), NOTCH);
	});
});

describe('below the notch there is nowhere to rest', () => {
	// vividn, 2026-09-18: "dragging up a bit should snap to the single output
	// bar open… similar to hitting ^ toggle" — and a toggle does not ask how
	// far you pushed it. A half-drawn output row is not a state.
	test('the gesture’s own direction decides, not the midpoint', () => {
		assert.equal(at(4, 0, true), NOTCH, 'a nudge up from shut did not open the output');
		assert.equal(at(NOTCH - 26, 0, false), 0, 'a nudge down from just under the notch did not shut it');
	});
});

describe('a coast snags, a finger does not', () => {
	// vividn, 2026-09-18: "When closing snag at the notch so that the output
	// section stays open, but you can drag over this notch if you like."
	test('a throw down from full stops at the row, not at shut', () => {
		assert.equal(at(500, -3, false), NOTCH, 'the throw closed the output as well');
	});

	test('and from the notch the next one shuts it', () => {
		// The finger itself has already carried the bar to 0 by the time it
		// lets go, which is what "you can drag over this notch" looks like.
		assert.equal(at(0, -3, false), 0);
	});

	// The bead: «From closed, a tap or swipe up on the handle reopens the
	// single row». A throw up from shut is a swipe up from shut.
	test('a throw up from shut stops at the row, not full', () => {
		assert.equal(at(10, 3, true), NOTCH, 'the throw went past the row');
	});

	test('a drag that ENDS past an edge is obeyed, however fast it was going', () => {
		// The finger is at 20px when it lets go: it dragged THROUGH the
		// notch, so there is no edge above it left to snag on.
		assert.equal(at(20, -3, false), 0);
		// And up: a finger past the notch goes full on a throw.
		assert.equal(at(NOTCH + 30, 3, true), CEILING);
	});
});

// A TUNING THAT IS NOT THE DEFAULT (da-o2wm8).
//
// `DEBUG bar` moves these two numbers at runtime, because nobody here can
// judge them and a thumb can. The rule stays PURE: the tuning is the last
// argument, defaulting to the exported constants — which is why every case
// above still reads those constants and still passes word for word. What
// is asserted here is that a different pair moves the resting place as the
// table says it should, so the instrument reports on a feel the bar really
// produces.
describe('a tuning that is not the default', () => {
	const tuned =
		(tau, detent) =>
		(now, speed = 0, up = speed >= 0) =>
			rest(now, speed, up, NOTCH, CEILING, { tau, detent });

	test('the default IS the exported pair, so an untouched install is unchanged', () => {
		assert.deepEqual(DEFAULT_TUNING, { tau: TAU, detent: DETENT });
		assert.equal(at(300, 0.7), tuned(TAU, DETENT)(300, 0.7));
	});

	// `speed * TAU` is what a release carries: a push that falls short of the
	// notch's catch at one TAU is carried into it at twice the TAU.
	test('a longer TAU carries the same push further', () => {
		// 20px a TAU: from 30px past the catch, one TAU ends 10px outside it.
		const push = -20 / TAU;
		assert.equal(tuned(TAU, DETENT)(NOTCH + DETENT + 30, push, true), CEILING);
		assert.equal(tuned(2 * TAU, DETENT)(NOTCH + DETENT + 30, push, true), NOTCH);
	});

	// "DETENT = 0 must be reachable — that is no detents at all, and
	// feeling it is how he finds out whether he wants them."
	test('DETENT = 0 is no detents at all, and the snag is not one of them', () => {
		const none = tuned(TAU, 0);
		assert.equal(none(NOTCH + 1, 0, true), CEILING, 'a rest one px above the notch was taken by it');
		assert.equal(none(CEILING - 1, 0, false), NOTCH, 'a rest one px under the ceiling was taken by it');
		// The SNAG is a separate rule and is untouched by the knob: a coast
		// down from a tall log still stops at the notch it came from above.
		assert.equal(none(500, -3, false), NOTCH);
	});

	test('a wider DETENT catches from further away than the default reaches', () => {
		assert.equal(tuned(TAU, 60)(NOTCH + 40, 0), NOTCH, 'a 60px detent did not reach 40px');
		assert.equal(at(NOTCH + 40, 0), CEILING, 'the default caught it anyway');
	});
});
