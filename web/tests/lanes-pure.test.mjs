// Which fields a press on a picture brings to the front, in node (da-qu16).
//
// vividn, 2026-09-17: «fields that have a span over that y position and
// occur dots that are close to that position should be prioritized in the
// fields menu.» `nearest` is that sentence as a ranking, over boxes and
// instants a picture drew; `promoted` is what the widget does with it.
//
// And on 2026-09-21, from his phone: «it seems occur dots aren't well
// filtered when clicking at a certain y in the now view. Let's filter. The
// fields with the closest y should appear first in the fields list.» So a
// dot is reached by HEIGHT the way a block is, and the coarse rank is only
// the first key — the distances order within it (da-q5jq).
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

const { REACH_PX, ROW_PX, closer, nearest, promoted, rowsUnder } = await import('../src/lib/lanes.ts');

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

/** `nearest` as a plain list, nearest first — what the bar's front becomes. */
const front = (drawn, press) => {
	const rank = nearest(drawn, press);
	return rank === null ? null : [...rank].sort((a, b) => closer(a[1], b[1])).map(([f]) => f);
};

describe('a press ranks the fields the picture drew under it', () => {
	// The three lanes of the picture every case below presses on: `sleep`
	// runs the whole window across rows 10–30, `work` only the first half
	// across rows 40–60, and `coffee` is a dot in the third lane.
	const PICTURE = [block('sleep', 10, 30, -60, 60), block('work', 40, 60, -60, 0), dot('coffee', 200, 80)];

	test('a block over the height AND the time beats a block over the height alone', () => {
		// 20 minutes past T is inside sleep's span and past the end of work's.
		const rank = nearest([...PICTURE, block('work', 10, 30, -60, 0)], at(200, 20, 20));
		assert.equal(rank.get('sleep').rank, 0, 'sleep spans both the height and the time');
		assert.equal(rank.get('work').rank, 1, 'work spans the height but the time is past its end');
		assert.deepEqual(front([...PICTURE, block('work', 10, 30, -60, 0)], at(200, 20, 20)), ['sleep', 'work']);
	});

	test('a press with no instant at all still ranks by height — the now region, the snapshot', () => {
		// A chip in the now strip carries no span of its own: it is the lane
		// running on in whatever state it is in (vividn, 17:01).
		const chip = { field: 'mood', box: { top: 31, bottom: 50, left: 380, right: 400 } };
		assert.deepEqual(front([...PICTURE, chip], at(390, 45)), ['work', 'mood']);
		assert.equal(nearest([chip], at(390, 45)).get('mood').rank, 1);
	});

	test('a dot is reached by HEIGHT, the way a block is — however far off in time it fell', () => {
		// vividn, 2026-09-21 13:37, on his phone: «it seems occur dots aren't
		// well filtered when clicking at a certain y in the now view. Let's
		// filter.» A block spans every x and so is reached by height alone.
		// A dot was reached by a RADIUS, so the same press 25px along its
		// own row reached it no more than one in another lane.
		assert.deepEqual(front(PICTURE, at(200 + REACH_PX - 1, 80)), ['coffee']);
		assert.deepEqual(front(PICTURE, at(200 + REACH_PX + 2, 80)), ['coffee'], 'and 26px away in TIME');
		assert.deepEqual(front(PICTURE, at(0, 80)), ['coffee'], 'and clear across the picture');
		assert.equal(nearest(PICTURE, at(200, 80)).get('coffee').rank, 2, 'a dot still ranks after a block');

		// Height still gates it, and the distance is from the mark's own
		// box — the two px a dot is drawn across (`dot` above).
		assert.deepEqual(front(PICTURE, at(200, 80 + 2 + REACH_PX)), ['coffee'], 'a finger clear of its edge');
		assert.equal(front(PICTURE, at(200, 80 + 3 + REACH_PX)), null, 'and a pixel further is another lane');
	});

	test('the closest y comes first, and time only separates what height cannot', () => {
		// «The fields with the closest y should appear first in the fields
		// list.» Two dots in reach, 1px and 18px off the press's height.
		const stacked = [dot('far', 200, 98), dot('near', 200, 81)];
		assert.deepEqual(front(stacked, at(200, 80)), ['near', 'far']);
		const rank = nearest(stacked, at(200, 80));
		assert.ok(rank.get('near').dy < rank.get('far').dy, 'and it is the height that separates them');

		// The same height: then the one nearer in TIME leads, rather than
		// the two being equal or the far one absent.
		const sameRow = [dot('later', 380, 80), dot('here', 204, 80)];
		assert.deepEqual(front(sameRow, at(200, 80)), ['here', 'later']);
		assert.deepEqual([...nearest(sameRow, at(200, 80))].map(([f, n]) => [f, n.dy]), [
			['later', 0],
			['here', 0]
		]);
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
		assert.equal(closer(rank.get('a'), rank.get('b')), 0, 'equidistant from either, so they tie');
		assert.ok(!rank.has('c'), 'a lane 25px away is not under the finger');

		// And a press that is not in the middle of the gap is nearer one.
		assert.deepEqual(front(tiny, at(200, 3.5, 0)), ['a', 'b']);
		assert.deepEqual(front(tiny, at(200, 5.5, 0)), ['b', 'a']);
	});

	test('no height at all changes nothing: a typed -t, a tail row', () => {
		assert.equal(nearest(PICTURE, { x: 200, y: null, t: T }), null);
		assert.equal(nearest([], at(200, 20, 0)), null, 'and so does a press on an empty picture');
	});
});

describe('the ranking orders the bar without hiding anything', () => {
	/** The four buttons the bar draws, in the order it had before a press. */
	const bar = (active = []) =>
		['coffee', 'mood', 'run', 'sleep'].map((field) => ({
			field,
			shape: { kind: 'point' },
			active: active.includes(field)
		}));
	const fields = bar();
	const names = (list) => list.map((b) => b.field);
	/** One mark, as `nearest` reports it. */
	const near = (rank, dy = 0, dx = 0) => ({ rank, dy, dx });

	test('the ranked fields come first, in rank order, and the rest keep theirs', () => {
		const rank = new Map([
			['sleep', near(0)],
			['coffee', near(2)]
		]);
		assert.deepEqual(names(promoted(bar(['sleep']), rank)), ['sleep', 'coffee', 'mood', 'run']);
	});

	test('a tie keeps the usual order — a tie is not a ranking', () => {
		const rank = new Map([
			['run', near(1)],
			['mood', near(1)]
		]);
		assert.deepEqual(names(promoted(bar(['run', 'mood']), rank)), ['mood', 'run', 'coffee', 'sleep']);
	});

	test('inside one rank the nearer in HEIGHT leads, then the nearer in time', () => {
		// «The fields with the closest y should appear first in the fields
		// list» (vividn, 2026-09-21) — and `run` is drawn 18px off the press,
		// so alphabetical order is not what comes back.
		const byHeight = new Map([
			['run', near(2, 18)],
			['sleep', near(2, 1)]
		]);
		assert.deepEqual(names(promoted(fields, byHeight)), ['sleep', 'run', 'coffee', 'mood']);

		// Two at the same height: then how far off in TIME separates them,
		// rather than leaving them equal.
		const byTime = new Map([
			['coffee', near(2, 0, 300)],
			['run', near(2, 0, 4)]
		]);
		assert.deepEqual(names(promoted(fields, byTime)), ['run', 'coffee', 'mood', 'sleep']);
	});

	test('a block outranks a dot while its field is ON, and falls in among them when it is OFF', () => {
		// vividn, 2026-09-21 13:46: «a block can outrank an occur, particularly
		// if it is in an active state. If the block is false, then I would
		// rank by order to center.» `run` is a block the press is nearly on;
		// `coffee` a dot it is nearer still.
		const rank = new Map([
			['run', near(1, 6)],
			['coffee', near(2, 1)]
		]);
		assert.deepEqual(names(promoted(bar(['run']), rank)), ['run', 'coffee', 'mood', 'sleep'], 'on: the block leads');
		assert.deepEqual(names(promoted(bar(), rank)), ['coffee', 'run', 'mood', 'sleep'], 'off: the nearer mark leads');
		// It is a DEMOTION to the dots' rank, never a removal: the same
		// inactive block still leads a dot further off than it.
		const further = new Map([
			['run', near(1, 6)],
			['coffee', near(2, 20)]
		]);
		assert.deepEqual(names(promoted(bar(), further)), ['run', 'coffee', 'mood', 'sleep']);
	});

	test('no ranking is the list itself, unmoved and whole', () => {
		assert.deepEqual(names(promoted(fields, null)), names(fields));
		assert.deepEqual(names(promoted(fields, new Map())), names(fields));
		assert.equal(promoted(fields, new Map([['sleep', near(0)]])).length, fields.length, 'nothing is removed');
	});
});

describe('under a press, the fields it reached get the rows to themselves (da-xtkcp)', () => {
	// vividn, 2026-09-24: «when clicking at a y in the nowview, the fields are
	// filtered to those close by to the click, so they can spread out a
	// little more vertically in this case to better utilize the space.»
	/** Six fields a sixth apart in y, none used more than another. */
	const six = ['a', 'b', 'c', 'd', 'e', 'f'].map((field, i) => ({ field, y: i / 6, active: false }));
	const standings = new Map(six.map(({ field, y }) => [field, { y, frecency: 0 }]));
	const near = (dy) => ({ rank: 2, dy, dx: 0 });
	const read = (rows) => rows.map(({ lead, rest }) => [lead.map((b) => b.field), rest.map((b) => b.field)]);

	test('with no press, the rows are the bar\'s own', () => {
		assert.deepEqual(read(rowsUnder(six, 3, standings, null)), [
			[[], ['a', 'b']],
			[[], ['c', 'd']],
			[[], ['e', 'f']]
		]);
	});

	test('three fields near one height spread over three rows, top to bottom; the rest stay in theirs', () => {
		// `c`, `d`, `e` share two bands of the bar; the press gives them one
		// row each, in the order of y, and moves no other field's row.
		const rank = new Map([
			['d', near(0)],
			['c', near(4)],
			['e', near(8)]
		]);
		assert.deepEqual(read(rowsUnder(six, 3, standings, rank)), [
			[['c'], ['a', 'b']],
			[['d'], []],
			[['e'], ['f']]
		]);
	});

	test('more reached than rows: bands of y, nearest first along each', () => {
		const rank = new Map([
			['a', near(9)],
			['b', near(1)],
			['c', near(5)],
			['d', near(0)]
		]);
		assert.deepEqual(read(rowsUnder(six, 2, standings, rank)), [
			[['b', 'a'], []],
			[['d', 'c'], ['e', 'f']]
		]);
	});
});
