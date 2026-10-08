// The fields widget's «aligned to the picture» layout, as pure geometry
// (da-omw4n): where a button stands (`aligned`), and the zoom a pinch
// moves (`zoomed`, `panned`, `yIn`, `yOut`). The built app is
// fields-aligned.test.mjs.

import test from 'node:test';
import assert from 'node:assert/strict';
import { FULL, aligned, panned, yIn, yOut, zoomed } from '../src/lib/fields.ts';

const near = (a, b) => Math.abs(a - b) < 1e-9;

test('a button stands in the middle of its band, a band\'s own height from its top, and inside the box', () => {
	const [mid, tall, bottom] = aligned(
		[
			{ y: 0.45, height: 0.1, w: 50, h: 20 },
			{ y: 0.2, height: 0.4, w: 50, h: 80 },
			{ y: 0.95, height: 0.1, w: 50, h: 20 }
		],
		200,
		FULL,
		6
	);
	// Its centre on the band's: y 0.45–0.55 is the box's middle.
	assert.deepEqual(mid, { x: 0, top: 90 });
	// As tall as its band, it starts where the band does — beside the first,
	// whose height it shares.
	assert.deepEqual(tall, { x: 56, top: 40 });
	// Past the pictures' bottom edge it sits on it, not under it.
	assert.deepEqual(bottom, { x: 0, top: 180 });
});

test('buttons whose heights overlap stand side by side, in the order given; apart, both at the left', () => {
	const stands = aligned(
		[
			{ y: 0.1, height: 0, w: 50, h: 20 },
			{ y: 0.12, height: 0, w: 40, h: 20 },
			{ y: 0.14, height: 0, w: 30, h: 20 },
			{ y: 0.6, height: 0, w: 70, h: 20 }
		],
		200,
		FULL,
		6
	);
	assert.deepEqual(
		stands.map((s) => s.x),
		[0, 56, 102, 0]
	);
});

test('a tall band’s button holds its column down the whole of its height', () => {
	// The tall band spans 0–0.5; a short button beside it at the top, and
	// one lower down that only the tall band is beside, in the same column.
	const [tall, top, low] = aligned(
		[
			{ y: 0, height: 0.5, w: 60, h: 100 },
			{ y: 0, height: 0, w: 40, h: 20 },
			{ y: 0.3, height: 0, w: 40, h: 20 }
		],
		200,
		FULL,
		6
	);
	assert.deepEqual(tall, { x: 0, top: 0 });
	assert.deepEqual(top, { x: 66, top: 0 });
	assert.deepEqual(low, { x: 66, top: 50 });
});

test('a crowded band runs past the box; a zoom that pulls it apart brings them back left', () => {
	const crowd = Array.from({ length: 10 }, (_, i) => ({ y: 0.4 + i * 0.01, height: 0, w: 80, h: 24 }));
	const whole = aligned(crowd, 200, FULL, 6);
	assert.ok(Math.max(...whole.map((s) => s.x + 80)) > 390, 'ten crowded buttons fit a phone');
	const view = zoomed(FULL, 0.45, 8);
	const apart = aligned(crowd, 200, view, 6);
	assert.ok(
		Math.max(...apart.map((s) => s.x)) < Math.max(...whole.map((s) => s.x)),
		'zooming in did not spread the band into more rows'
	);
	// The sizes stay what they were: only the space between them grew.
	assert.ok(apart[1].top - apart[0].top > whole[1].top - whole[0].top);
});

test('a y maps into the box and back out, at any zoom', () => {
	for (const view of [FULL, { from: 0.25, to: 0.5 }, zoomed(FULL, 0.9, 3)]) {
		for (const y of [0, 0.3, 0.5, 0.75, 1]) assert.ok(near(yOut(view, yIn(view, y)), y));
	}
	assert.equal(yIn({ from: 0.25, to: 0.5 }, 0.375), 0.5);
	assert.equal(yOut({ from: 0.25, to: 0.5 }, 0.5), 0.375);
});

test('a zoom keeps the y under the pinch under it, inside the pictures and no deeper than an eighth', () => {
	const view = zoomed(FULL, 0.5, 4);
	assert.ok(near(view.from, 0.375) && near(view.to, 0.625));
	assert.ok(near(yOut(view, 0.5), 0.5));
	// At an edge it is pinned to the pictures rather than past them.
	assert.deepEqual(zoomed(FULL, 0, 2), { from: 0, to: 0.5 });
	assert.deepEqual(zoomed(FULL, 1, 2), { from: 0.5, to: 1 });
	// The deepest it goes, and back out to the whole again.
	const deep = zoomed(view, 0.5, 100);
	assert.ok(near(deep.to - deep.from, 1 / 8));
	assert.deepEqual(zoomed(deep, 0.3, 1 / 100), FULL);
});

test('a pan moves the view by a share of the box, and stops at the edges', () => {
	const view = { from: 0.25, to: 0.5 };
	assert.deepEqual(panned(view, 1), { from: 0.5, to: 0.75 });
	assert.deepEqual(panned(view, -5), { from: 0, to: 0.25 });
	assert.deepEqual(panned(view, 5), { from: 0.75, to: 1 });
	assert.deepEqual(panned(FULL, 1), FULL);
});
