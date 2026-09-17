// Which fields a press on a picture brings to the front, in node (da-qu16).
//
// vividn, 2026-09-17: «fields that have a span over that y position and
// occur dots that are close to that position should be prioritized in the
// fields menu.» `nearest` is that sentence as a ranking, over boxes and
// instants a picture drew; `promoted` is what the widget does with it.
//
// Nothing here is removed: every case below checks that the rule ORDERS.
// The same rules as a reader meets them are `fieldsorder.test.mjs`.

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

const { REACH_PX, ROW_PX, nearest } = await import('../src/lib/lanes.ts');
const { promoted } = await import('../src/lib/commands.ts');

const MINUTE = 60_000;
const T = Date.UTC(2026, 8, 17, 14, 0);

/** A block of `field` drawn over rows `top`–`bottom`, covering `from`→`to`. */
const block = (field, top, bottom, from, to) => ({
	field,
	box: { top, bottom, left: 0, right: 400 },
	from: T + from * MINUTE,
	to: T + to * MINUTE
});

/** A dot of `field` drawn centred on (x, y), four px across. */
const dot = (field, x, y) => ({
	field,
	dot: true,
	box: { top: y - 2, bottom: y + 2, left: x - 2, right: x + 2 }
});

/** The press: on the band at (x, y), having selected `t` minutes past T. */
const at = (x, y, t = null) => ({ x, y, t: t === null ? null : T + t * MINUTE });

/** `nearest` as a plain list, best rank first — what the bar's front becomes. */
const front = (drawn, press) => {
	const rank = nearest(drawn, press);
	return rank === null ? null : [...rank].sort((a, b) => a[1] - b[1]).map(([f]) => f);
};

describe('a press ranks the fields the picture drew under it', () => {
	// The three lanes of the picture every case below presses on: `sleep`
	// runs the whole window across rows 10–30, `work` only the first half
	// across rows 40–60, and `coffee` is a dot in the third lane.
	const PICTURE = [block('sleep', 10, 30, -60, 60), block('work', 40, 60, -60, 0), dot('coffee', 200, 80)];

	test('a block over the height AND the time beats a block over the height alone', () => {
		// 20 minutes past T is inside sleep's span and past the end of work's.
		const rank = nearest([...PICTURE, block('work', 10, 30, -60, 0)], at(200, 20, 20));
		assert.equal(rank.get('sleep'), 0, 'sleep spans both the height and the time');
		assert.equal(rank.get('work'), 1, 'work spans the height but the time is past its end');
		assert.deepEqual(front([...PICTURE, block('work', 10, 30, -60, 0)], at(200, 20, 20)), ['sleep', 'work']);
	});

	test('a press with no instant at all still ranks by height — the now region, the snapshot', () => {
		// A chip in the now strip carries no span of its own: it is the lane
		// running on in whatever state it is in (vividn, 17:01).
		const chip = { field: 'mood', box: { top: 31, bottom: 50, left: 380, right: 400 } };
		assert.deepEqual(front([...PICTURE, chip], at(390, 45)), ['work', 'mood']);
		assert.equal(nearest([chip], at(390, 45)).get('mood'), 1);
	});

	test('a dot within a finger of the press is reached; one further off is not', () => {
		assert.deepEqual(front(PICTURE, at(200 + REACH_PX - 1, 80)), ['coffee']);
		assert.equal(nearest(PICTURE, at(200, 80)).get('coffee'), 2, 'a dot ranks after every block');
		// Beside it in TIME is no more on it than beside it in height: the
		// distance is the finger's, in both directions at once.
		assert.equal(front(PICTURE, at(200 + REACH_PX + 2, 80)), null);
		assert.equal(front(PICTURE, at(200, 80 + REACH_PX + 2)), null);
	});

	test('a row hit squarely is that row alone, though its neighbour abuts it', () => {
		// datum stacks the lanes by hashing each field's name, so they abut
		// and overlap: a tolerance that always applied would pull the
		// neighbour in with every press.
		const stacked = [block('a', 254, 261, -60, 60), block('b', 261, 268, -60, 60)];
		assert.deepEqual(front(stacked, at(200, 257, 0)), ['a']);
		assert.deepEqual(front(stacked, at(200, 265, 0)), ['b']);
	});

	test('a press that fell in NO row reaches a hairline for one — and those tie', () => {
		// The tiny nowview in `mobile`: lanes a few px tall with a gap.
		const tiny = [block('a', 0, 3, -60, 60), block('b', 6, 9, -60, 60), block('c', 30, 33, -60, 60)];
		const rank = nearest(tiny, at(200, 4.5, 0));
		assert.deepEqual([...rank.keys()].sort(), ['a', 'b'], `${ROW_PX}px reaches either row from the gap`);
		assert.deepEqual([...new Set(rank.values())], [0], 'and they tie rather than being ranked');
		assert.ok(!rank.has('c'), 'a lane 25px away is not under the finger');
	});

	test('no height at all changes nothing: a typed -t, a tail row', () => {
		assert.equal(nearest(PICTURE, { x: 200, y: null, t: T }), null);
		assert.equal(nearest([], at(200, 20, 0)), null, 'and so does a press on an empty picture');
	});
});

describe('the ranking orders the bar without hiding anything', () => {
	const fields = ['coffee', 'mood', 'run', 'sleep'].map((field) => ({ field, shape: { kind: 'point' } }));
	const names = (list) => list.map((b) => b.field);

	test('the ranked fields come first, in rank order, and the rest keep theirs', () => {
		const rank = new Map([
			['sleep', 0],
			['coffee', 2]
		]);
		assert.deepEqual(names(promoted(fields, rank)), ['sleep', 'coffee', 'mood', 'run']);
	});

	test('a tie keeps the usual order — a tie is not a ranking', () => {
		const rank = new Map([
			['run', 1],
			['mood', 1]
		]);
		assert.deepEqual(names(promoted(fields, rank)), ['mood', 'run', 'coffee', 'sleep']);
	});

	test('no ranking is the list itself, unmoved and whole', () => {
		assert.deepEqual(names(promoted(fields, null)), names(fields));
		assert.deepEqual(names(promoted(fields, new Map())), names(fields));
		assert.equal(promoted(fields, new Map([['sleep', 0]])).length, fields.length, 'nothing is removed');
	});
});
