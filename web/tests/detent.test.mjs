// WHERE A RELEASE COMES TO REST, in node, with no browser (da-tj6m).
//
// `$lib/detent.ts` is the bar's whole momentum rule and it is pure, so the
// table at its head can be asked directly rather than thrown at a phone.
// What it CANNOT settle is the numbers: vremya's instrument adds latency to
// a synthetic touch, so a velocity measured in a drive is approximate, and
// where a real thumb lands relative to a boundary is a thing only a real
// thumb says. What is asserted here is the SHAPE — that momentum applies at
// every speed, that no threshold divides a small push from a big one, that
// the three detents catch, and that a coast snags at an edge it came from
// above while a finger may be dragged through it.
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

describe('momentum at any speed', () => {
	// THE COMPLAINT ITSELF. vividn, 2026-09-21: "right now there is only
	// acceleration if it is enough to get it all the way up or down, it
	// should allow little bits of acceleration sliding the section up and
	// down". Under the old `FLING` floor every release below 1.2 px/ms
	// snapped and every release above it went to the end, so these three
	// pushes had two answers between them. Now they have three, and they
	// are in the order the pushes were.
	test('a small push slides a little, a bigger one further, with no cliff between', () => {
		const small = at(200, 0.3);
		const middling = at(200, 0.6);
		const big = at(200, 1.2);
		assert.ok(small > 200, `a push of 0.3 px/ms moved the bar to ${small}`);
		assert.ok(middling > small, `0.6 px/ms (${middling}) did not beat 0.3 (${small})`);
		assert.ok(big > middling, `1.2 px/ms (${big}) did not beat 0.6 (${middling})`);
		// And PROPORTIONALLY: twice the speed is twice the glide, which is
		// what "no cliff" means as an equation.
		assert.equal(middling - 200, 2 * (small - 200));
	});

	// Swept, because a cliff is a thing that hides between two samples: the
	// old floor was invisible to any test that only asked 0.3 and 1.7. A
	// step of one hundredth of a px/ms may move the rest by its own glide
	// and by at most one detent's pull — never by the hundreds of px that a
	// threshold to "all the way up" costs.
	test('across every speed a thumb can make, the rest never leaps to an end', () => {
		const step = 0.02;
		const most = DETENT + step * TAU;
		let last = at(300, 0);
		for (let v = step; v <= 4; v += step) {
			const now = at(300, v);
			assert.ok(now >= last, `${v.toFixed(2)} px/ms went backwards: ${now} after ${last}`);
			assert.ok(now - last <= most + 1e-9, `a cliff at ${v.toFixed(2)} px/ms: ${last} → ${now}`);
			last = now;
		}
	});

	test('a release at rest stays where the finger left it, above the notch', () => {
		assert.equal(at(300, 0), 300, 'a height the reader chose was taken from them');
	});
});

describe('the detents catch', () => {
	test('a glide that would stop near a detent is taken by it', () => {
		assert.equal(at(NOTCH + DETENT, 0), NOTCH, 'arriving just above the notch did not catch');
		assert.equal(at(CEILING - DETENT, 0), CEILING, 'arriving just under the ceiling did not catch');
		assert.equal(at(NOTCH + DETENT + 1, 0), NOTCH + DETENT + 1, 'the detent reached too far');
	});

	// "it scrolls smoothly but catches at the right place" — a glide that
	// PASSES a detent slowly is a glide that stops just beyond it, so the
	// same window is both halves of the rule.
	test('a glide that passes a detent slowly is caught; one with real speed left is not', () => {
		const creeping = DETENT / TAU / 2;
		assert.equal(at(NOTCH - 4, creeping, true), NOTCH, 'a slow pass through the notch did not catch');
		assert.equal(at(NOTCH + 4, 1.2, true), NOTCH + 4 + 1.2 * TAU, 'a fast pass was caught');
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

	test('but a real push from down there carries past it into the log', () => {
		const thrown = at(10, 1.5, true);
		assert.ok(thrown > NOTCH + DETENT, `a push from shut stopped at ${thrown}`);
	});
});

describe('a coast snags, a finger does not', () => {
	// vividn, 2026-09-18: "When closing snag at the notch so that the output
	// section stays open, but you can drag over this notch if you like."
	test('a throw down from a tall log stops at the notch, not at shut', () => {
		assert.equal(at(500, -3, false), NOTCH, 'the throw closed the output as well');
	});

	test('and from the notch the next one shuts it', () => {
		// The finger itself has already carried the bar to 0 by the time it
		// lets go, which is what "you can drag over this notch" looks like.
		assert.equal(at(0, -3, false), 0);
	});

	// "When I get back to the top, then trying to scroll up further starts
	// closing the input section" — so the top of the list is a place a coast
	// STOPS at, and closing is the next thing the reader does.
	test('a throw down from inside the list stops at the top of it', () => {
		assert.equal(at(CEILING + 400, -3, false), CEILING, 'the throw tore the bar down as well');
	});

	test('a drag that ENDS below an edge is obeyed, however fast it was going', () => {
		// The finger is at 20px when it lets go: it dragged THROUGH the
		// notch, so there is no edge above it left to snag on.
		assert.equal(at(20, -3, false), 0);
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

	// `speed * TAU` is the whole of what a release carries, so twice the
	// time constant is twice the glide — at a speed whose landing is near
	// no detent, which is the arithmetic on its own.
	test('a longer TAU carries the same push proportionally further', () => {
		const normal = tuned(TAU, DETENT)(300, 0.3) - 300;
		const twice = tuned(2 * TAU, DETENT)(300, 0.3) - 300;
		assert.equal(normal, 0.3 * TAU);
		assert.equal(twice, 2 * normal);
	});

	// "DETENT = 0 must be reachable — that is no detents at all, and
	// feeling it is how he finds out whether he wants them."
	test('DETENT = 0 is no detents at all, and the snag is not one of them', () => {
		const none = tuned(TAU, 0);
		assert.equal(none(NOTCH + 1, 0), NOTCH + 1, 'a rest one px above the notch was taken by it');
		assert.equal(none(CEILING - 1, 0), CEILING - 1, 'a rest one px under the ceiling was taken by it');
		// The SNAG is a separate rule and is untouched by the knob: a coast
		// down from a tall log still stops at the notch it came from above.
		assert.equal(none(500, -3, false), NOTCH);
	});

	test('a wider DETENT catches from further away than the default reaches', () => {
		assert.equal(tuned(TAU, 60)(NOTCH + 40, 0), NOTCH, 'a 60px detent did not reach 40px');
		assert.equal(at(NOTCH + 40, 0), NOTCH + 40, 'the default caught it anyway');
	});
});
