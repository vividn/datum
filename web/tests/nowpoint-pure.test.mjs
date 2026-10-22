// The nowview's gestures and the argv a selection is, in node (da-rskp).
//
// `advance` is the truth table at the head of $lib/nowpoint.ts — what a
// press becomes after how many ms and how many px, and what cannot become
// what — and `snapTo` is the finger's width around a gridline. `relArg`
// is the other half: what a relative selection PUTS on the command line,
// which is the whole of what it means to the CLI.
//
// The same rules as a reader meets them are `nowpoint.test.mjs`.

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

const { LONG_MS, SLOP_PX, SNAP_PX, advance, relArg, snapTo, tapStep, undecided } = await import(
	'../src/lib/nowpoint.ts'
);
const { HOLD_MS } = await import('../src/lib/hold.ts');

/** A press at (100, 50), with whatever it landed on. */
const press = (extra = {}) => ({ x: 100, y: 50, inNow: false, atOnce: false, ...extra });

describe('a press on the band becomes one gesture', () => {
	test('a still press is undecided, and the wait does not decide it either', () => {
		const p = press();
		assert.equal(advance('press', p, 100, 50, false), 'press');
		assert.equal(advance('press', p, 100 + SLOP_PX, 50, false), 'press', 'inside the slop is still still');
		assert.ok(LONG_MS >= 300, `${LONG_MS}ms is what the timer waits, and a tap must not reach it`);
		assert.equal(LONG_MS, HOLD_MS, "the band's long press is the app's one hold (da-0gxd7)");
	});

	// da-pzzoz. The 400ms used to return `range` outright, which is why
	// «long press to select, as long as it's not a long press drag» could
	// not be said at all: the range had already taken the press. Now the
	// wait says only that the press is OLD ENOUGH, and the finger decides.
	test('the long press makes a press HELD, not a range: the finger decides which', () => {
		const p = press();
		assert.equal(advance('press', p, 100, 50, true), 'hold', 'the wait alone commits to nothing');
		assert.ok(undecided('hold'), 'a hold is still open to becoming something else');
		assert.equal(advance('hold', p, 100 + SLOP_PX + 1, 50, true), 'range', 'a hold that TRAVELS is the range');
		assert.equal(advance('hold', p, 100 - SLOP_PX - 1, 50, true), 'range', 'either way along the band');
		assert.equal(advance('hold', p, 100, 50, true), 'hold', 'and one that does not travel stays held');
		assert.equal(
			advance('hold', p, 100 + 10, 50 + 40, true),
			'hold',
			'a mostly-vertical finger is the browser’s, held or not'
		);
	});

	// The order of the wait and the movement is the WHOLE difference
	// between the two gestures: move first and it is a pan, wait first and
	// it is a range.
	test('travel BEFORE the wait is a pan; travel AFTER it is a range', () => {
		const p = press();
		assert.equal(advance('press', p, 100 + SLOP_PX + 1, 50, false), 'pan');
		assert.equal(advance('hold', p, 100 + SLOP_PX + 1, 50, true), 'range');
	});

	test('a drag sideways pans; a drag mostly downward is the page’s scroll, not ours', () => {
		const p = press();
		assert.equal(advance('press', p, 100 + SLOP_PX + 1, 50, false), 'pan');
		assert.equal(advance('press', p, 100 - SLOP_PX - 1, 50, false), 'pan', 'either way along the band');
		assert.equal(advance('press', p, 100 + 10, 50 + 40, false), 'press', 'mostly vertical belongs to the browser');
		assert.equal(advance('press', p, 100 + 10, 50 + 10, false), 'pan', 'exactly diagonal is ours');
	});

	// `atOnce` is shift on a desk, and only that: «swiping left/right must
	// not select time» (vividn, da-xg1vi), so no view says it for a finger.
	test('atOnce makes a drag a range with no waiting, and a still press still nothing', () => {
		const p = press({ atOnce: true });
		assert.equal(advance('press', p, 100 + SLOP_PX + 1, 50, false), 'range');
		assert.equal(advance('press', p, 100, 50, false), 'press', 'holding alone selects nothing');
		assert.equal(advance('press', p, 100, 50, true), 'hold', 'and the wait only makes it HELD');
		assert.equal(advance('hold', p, 100 + SLOP_PX + 1, 50, true), 'range', 'a held finger that travels');
		assert.equal(
			advance('press', p, 100 + 10, 50 + 40, false),
			'press',
			'mostly vertical is still the browser’s, however little waiting there is'
		);
	});

	// da-xg1vi moved the duration ending now behind the long press, as every
	// range is: a plain swipe there pans like a swipe anywhere.
	test('in the now region a HELD drag left is a duration ending now; a swipe is a pan', () => {
		const p = press({ inNow: true });
		assert.equal(advance('press', p, 100 - SLOP_PX - 1, 50, false), 'pan', 'a swipe never selects time');
		assert.equal(advance('press', p, 100 + SLOP_PX + 1, 50, false), 'pan');
		assert.equal(advance('press', p, 100, 50, true), 'hold');
		assert.equal(advance('hold', p, 100 - SLOP_PX - 1, 50, true), 'now', 'held, then left: the duration');
		assert.equal(advance('hold', p, 100 + SLOP_PX + 1, 50, true), 'range', 'held, then right: a range');
	});

	test('what a COMMITTED gesture became is decided ONCE', () => {
		const p = press();
		// A pan that outlives the long press is still a pan.
		assert.equal(advance('pan', p, 100, 50, true), 'pan');
		assert.equal(undecided('pan'), false);
		assert.equal(undecided('range'), false);
		assert.equal(undecided('now'), false);
		assert.equal(undecided('scrub'), false, 'the time axis decides at the press');
		assert.equal(undecided('press'), true);
		// A range whose finger then runs sideways is still a range.
		assert.equal(advance('range', p, 400, 50, true), 'range');
		assert.equal(advance('now', p, 400, 50, true), 'now');
		assert.equal(advance('scrub', p, 400, 50, true), 'scrub', 'a scrub never pans');
	});
});

describe('a press snaps to a gridline it lands near', () => {
	const marks = [100, 200, 300];

	test('within a finger’s width of a mark it IS the mark; past it, it is where it fell', () => {
		assert.equal(snapTo(100 + SNAP_PX - 1, marks), 100);
		assert.equal(snapTo(200 - SNAP_PX, marks), 200, 'the edge of the width counts');
		assert.equal(snapTo(250, marks), 250, 'between the marks is between the marks');
		assert.equal(snapTo(200 + SNAP_PX + 1, marks), 200 + SNAP_PX + 1);
	});

	test('the NEAREST mark, the first of a tie, and nothing at all with no marks', () => {
		assert.equal(snapTo(208, [200, 210]), 210);
		assert.equal(snapTo(202, [200, 210]), 200);
		assert.equal(snapTo(205, [200, 210]), 200, 'a tie lands on the first, twice running');
		assert.equal(snapTo(205, [210, 200]), 210);
		assert.equal(snapTo(150, []), 150);
	});
});

describe('what a selection puts on the command line', () => {
	test('the -q ladder on the four five-minute marks, the relative -t everywhere else', () => {
		assert.deepEqual(
			[5, 10, 15, 20].map(relArg),
			['-q', '-qq', '-qqq', '-qqqq'],
			'five minutes a letter (vividn: "use -q, -qq, -qqq, -qqqq as the pill")'
		);
		assert.equal(relArg(7), '-t -7', 'the CLI reads -t -7 as seven minutes back');
		assert.equal(relArg(25), '-t -25', 'past -qqqq there is no letter for it');
		assert.equal(relArg(1), '-t -1');
	});

	test('a time between whole minutes says its seconds, as the CLI reads them', () => {
		assert.equal(relArg(20 / 60), '-t -20s');
		assert.equal(relArg(1.5), '-t -1m30s');
		assert.equal(relArg(5 + 5 / 60), '-t -5m5s', 'off the five-minute mark, no -q');
		assert.equal(relArg(119.6 / 60), '-t -2', 'to the second: 119.6 s is two minutes');
	});
});

describe('how finely a tap says its time follows the window', () => {
	test('five seconds at two minutes, whole minutes from the 30-minute default up', () => {
		assert.equal(tapStep(2), 5, 'vividn: «you should be able to do 5 seconds at this scale»');
		assert.deepEqual([0.5, 1, 5, 10, 15, 20].map(tapStep), [1, 1, 10, 15, 30, 30]);
		assert.deepEqual([30, 45, 60, 720].map(tapStep), [60, 60, 60, 60], 'a minute, as it always was');
	});

	// What an ABSOLUTE selection says, and the range that says no time at
	// all because its entry's time is already now, are read off the live
	// pills in `nowpoint.test.mjs`: `cursorArg` composes this with the
	// `-t HH:MM` the bar has always drawn, which is the bar's own module.
});
