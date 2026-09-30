// A switch field's states stand RADIALLY around the tap (da-7q3ka). vividn:
// «better to distribute them radially around the field», `< Back` and
// `+ Add state` «just two more buttons … in this radial cluster, on the
// left and right sides», and the `false` state, `end`, «in the same place
// as where the field button was».
//
// The spec's order is a rank by distance (`ringOf` in fields.ts);
// near an edge the ring is capped, not shifted, so `end` stays on the tap
// (da-8mk9p); the States tab's ↑ / ↓ is what reorders it, and logging
// never does. The list's other behaviour — Back,
// `+`, Escape, the colours — is `fields-states.test.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ringOf } from '../src/lib/fields.ts';
import { startApp, stopApp, withApp, seedLines, todayAt } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';
import { atRows } from './support/rows.mjs';
import { SPEC, openSpec } from './support/spec.mjs';

const H = 30;
const GAP = 6;

/** Every button of a ring as a box, `< Back` and `+ Add state` included. */
function boxes(ring, w) {
	return [
		{ name: 'back', ...ring.back, w: w.back },
		{ name: 'add', ...ring.add, w: w.add },
		...(ring.end ? [{ name: 'end', ...ring.end, w: w.end }] : []),
		...ring.states.map((s, i) => ({ name: i, ...s, w: w.states[i] }))
	];
}

/** The pairs that stand closer than `GAP`. */
const overlaps = (list) =>
	list.flatMap((a, i) =>
		list
			.slice(i + 1)
			.filter((b) => Math.abs(a.x - b.x) < (a.w + b.w) / 2 + GAP - 1e-9 && Math.abs(a.y - b.y) < H + GAP - 1e-9)
			.map((b) => [a.name, b.name])
	);

describe('ringOf', () => {
	const sizes = (n, end = 50, wide = (i) => 40 + 7 * (i % 3)) => ({
		back: 80,
		add: 110,
		end,
		states: Array.from({ length: n }, (_, i) => wide(i))
	});
	const far = (p) => Math.hypot(p.x, p.y);
	const inside = (ring, w, r) => boxes(ring, w).every((b) => b.x - b.w / 2 >= -r.left - 1e-9 && b.x + b.w / 2 <= r.right + 1e-9);
	/** Room for `ry` of centres above and below, and plenty sideways. */
	const room = (ry, more = {}) => ({ up: ry + H / 2, down: ry + H / 2, left: 1000, right: 1000, ...more });

	test('`end` on the tap, `< Back` and `+ Add state` either side of it on its line', () => {
		const ring = ringOf(sizes(5), H, room(72), GAP);
		assert.deepEqual(ring.end, { x: 0, y: 0 });
		assert.deepEqual(ring.back, { x: -(25 + GAP + 40), y: 0 });
		assert.deepEqual(ring.add, { x: 25 + GAP + 55, y: 0 });
	});

	test('ranked by distance: the first state nearest, ties clockwise from twelve o’clock', () => {
		const ring = ringOf(sizes(8, 50, () => 40), H, room(72), GAP);
		const d = ring.states.map(far);
		assert.ok(d.every((x, i) => i === 0 || x >= d[i - 1]), `further for a higher rank: ${d}`);
		const [a, b] = ring.states;
		assert.ok(a.x === 0 && a.y < 0, 'the first is not straight up');
		assert.ok(b.x === 0 && b.y > 0, 'the second is not straight down');
	});

	test('without `end`, the first state takes the tap', () => {
		const ring = ringOf(sizes(4, null), H, room(72), GAP);
		assert.equal(ring.end, null);
		assert.deepEqual(ring.states[0], { x: 0, y: 0 });
	});

	test('no two buttons touch, with `end` or without, for none to a dozen states', () => {
		for (const end of [50, null])
			for (let n = 0; n <= 12; n++) {
				const w = sizes(n, end);
				assert.deepEqual(overlaps(boxes(ringOf(w, H, room(72), GAP), w)), [], `${n} states, end ${end}`);
			}
	});

	test('a bar too short for a row above lays them all on the tap’s line', () => {
		const w = sizes(4);
		const ring = ringOf(w, H, room(10), GAP);
		assert.ok([ring.back, ring.add, ...ring.states].every((s) => s.y === 0), 'off the line');
		assert.deepEqual(overlaps(boxes(ring, w)), []);
	});

	// da-8mk9p: the field on the bar's first or last row.
	for (const [edge, sign] of [
		['up', -1],
		['down', 1]
	])
		test(`capped ${edge}: \`end\` still on the tap, nothing past the edge, the ranks nearest-first`, () => {
			const w = sizes(8, 50, () => 40);
			const ring = ringOf(w, H, room(72, { [edge]: H / 2 }), GAP);
			assert.deepEqual(ring.end, { x: 0, y: 0 });
			assert.ok([ring.back, ring.add].every((s) => s.y === 0), '< Back or + Add state left the line');
			assert.ok(ring.states.every((s) => sign * s.y <= 0), `a state is past the ${edge} edge`);
			assert.ok(ring.states.some((s) => sign * s.y < 0), 'nothing on the free side');
			const d = ring.states.map(far);
			assert.ok(d.every((x, i) => i === 0 || x >= d[i - 1]), `further for a higher rank: ${d}`);
			assert.deepEqual(overlaps(boxes(ring, w)), []);
		});

	test('capped sideways: the states stay inside, and the line where it fits', () => {
		// On one line, so the states would stand either side of it.
		const w = sizes(6);
		// Room for the line and a little more: `end` stays, the states fit.
		for (const r of [room(10, { left: 25 + GAP + 80 + 4 }), room(10, { right: 25 + GAP + 110 + 4 })]) {
			const ring = ringOf(w, H, r, GAP);
			assert.deepEqual(ring.end, { x: 0, y: 0 });
			assert.ok(inside(ring, w, r), `a button is past an edge: ${JSON.stringify(r)}`);
			assert.deepEqual(overlaps(boxes(ring, w)), []);
		}
	});

	// da-u9nfv — vividn: «yes shift those into another row if needed».
	for (const [edge, name, sign] of [
		['left', 'back', -1],
		['right', 'add', 1]
	])
		test(`no room ${edge} of \`end\`: it stays on the tap, and \`${name}\` steps onto another row inside`, () => {
			const w = sizes(6);
			// Just short of the line, so its own side has room on another row;
			// then room for `end` alone, so it has none.
			for (const [reach, own] of [
				[25 + GAP + w[name] - 1, true],
				[25 + 5, false]
			]) {
				const r = room(72, { [edge]: reach });
				const ring = ringOf(w, H, r, GAP);
				assert.deepEqual(ring.end, { x: 0, y: 0 });
				assert.notEqual(ring[name].y, 0, `${reach}: \`${name}\` stayed on the line`);
				assert.ok(inside(ring, w, r), `${reach}: a button is past an edge`);
				if (own) assert.ok(sign * ring[name].x > 0, `${reach}: \`${name}\` crossed \`end\``);
				assert.deepEqual(ring[name === 'back' ? 'add' : 'back'].y, 0, 'the other left the line');
				assert.deepEqual(overlaps(boxes(ring, w)), []);
			}
		});

	test('a state no spot inside holds goes past the right edge, never the left', () => {
		const w = sizes(12);
		const r = room(10, { left: 120, right: 150 });
		const ring = ringOf(w, H, r, GAP);
		assert.deepEqual(ring.end, { x: 0, y: 0 });
		assert.ok(ring.states.every((s, i) => s.x - w.states[i] / 2 >= -r.left), 'a state is past the left edge');
		assert.ok(ring.states.some((s, i) => s.x + w.states[i] / 2 > r.right), 'nothing overflowed: the case is not tested');
		assert.deepEqual(overlaps(boxes(ring, w)), []);
	});
});

const PAGE = JSON.stringify(along('column', widgetPane('repl', 'repl'), widgetPane('bar', 'widgetbar')));
const BAR = 'section[data-pane="bar"] .bar';
/** Nineteen fields around `mood` alphabetically, so it stands mid-row and can be scrolled anywhere. */
const OTHERS = 'alfa bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november oscar papa quebec romeo sierra'.split(' ');
const STATES = ['a', 'b', 'c', 'd'];
const RING = ['< Back', 'false', ...STATES, '+ Add state'];
const SEED = [
	...OTHERS.map((f) => `spec set ${f} --verb occur`),
	`spec set mood --verb switch --state false ${STATES.map((s) => `--state ${s}`).join(' ')}`
];

/**
 * Scroll the bar until mood's centre is at `fraction` of the bar's width
 * and `px` more (as near as the scroll reaches), press it there, and read where the ring
 * landed; then Back.
 *
 * The press is the element's own click, not the mouse's: a bar one row
 * high is a viewport so short the bottom bar lies over it, and the widget
 * reads where the BUTTON is, not where the pointer was.
 */
async function tapAt(page, fraction, field = 'mood', px = 0) {
	const tap = await page.locator(BAR).evaluate(
		(bar, [fraction, field, px]) => {
			const box = bar.getBoundingClientRect();
			const button = () => bar.querySelector(`[data-field="${field}"]`);
			const m = button().getBoundingClientRect();
			bar.scrollLeft += m.left + m.width / 2 - (box.left + fraction * box.width + px);
			const at = button().getBoundingClientRect();
			button().click();
			return { x: at.left + at.width / 2, y: at.top + at.height / 2 };
		},
		[fraction, field, px]
	);
	await page.locator(`${BAR} [data-add-state]`).waitFor();
	const seen = await page.locator(BAR).evaluate((bar) => {
		const ring = bar.querySelector('.ring').getBoundingClientRect();
		return {
			ring: { left: ring.left, right: ring.right, top: ring.top, bottom: ring.bottom },
			buttons: [...bar.querySelectorAll('.ring button')].map((b) => {
				const r = b.getBoundingClientRect();
				return {
					name: b.dataset.state ?? b.textContent.trim(),
					x: r.left + r.width / 2,
					y: r.top + r.height / 2,
					left: r.left,
					right: r.right,
					top: r.top,
					bottom: r.bottom,
					fill: getComputedStyle(b).backgroundColor
				};
			})
		};
	});
	await page.locator(`${BAR} [data-back]`).dispatchEvent('click');
	await page.locator(`${BAR} [data-field="${field}"]`).waitFor();
	const find = (name) => seen.buttons.find((b) => b.name === name);
	return { tap, ...seen, find, states: STATES.map(find) };
}

/** `name`'s centre relative to `end`'s, to the pixel. */
const from = (t, name) => [Math.round(t.find(name).x - t.find('false').x), Math.round(t.find(name).y - t.find('false').y)];

describe('a switch field’s states around the tap', () => {
	before(startApp);
	after(stopApp);

	test('`end` on the tap, at a side edge too, `< Back` or `+ Add state` on another row there, reordered by the States tab alone', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seedLines(app, SEED);
				await page.locator(`${BAR} [data-field="mood"]`).waitFor();

				for (const want of [1, 3]) {
					await atRows(page, 'bar', want);
					const middle = await tapAt(page, 0.5);
					assert.deepEqual(middle.buttons.map((b) => b.name), RING, `${want} rows: not in the spec's order`);
					// `< Back` and `+ Add state` flank `end` on its own line.
					const [back, end, add] = [middle.find('< Back'), middle.find('false'), middle.find('+ Add state')];
					assert.ok(back.right <= end.left && end.right <= add.left, `${want} rows: < Back, end, + Add state are not left to right`);
					assert.ok(Math.abs(back.y - end.y) < 1 && Math.abs(add.y - end.y) < 1, `${want} rows: not on one line`);
					if (want === 1) continue;

					// `end` on the tap, and a ring: states off its line — above AND
					// below it where the bar has a row either side (da-8mk9p).
					assert.ok(Math.abs(end.x - middle.tap.x) <= 1, `end at ${end.x}, the tap at ${middle.tap.x}`);
					const row = end.bottom - end.top + GAP;
					const [above, below] = [middle.states.some((s) => s.y < end.y - 1), middle.states.some((s) => s.y > end.y + 1)];
					assert.ok(above || below, 'not around the tap');
					if (end.top - middle.ring.top > row) assert.ok(above, 'a row above, and nothing in it');
					if (middle.ring.bottom - end.bottom > row) assert.ok(below, 'a row below, and nothing in it');
					// The spec's order is a rank: a no further than b, b than c…
					const d = STATES.map((name) => Math.hypot(...from(middle, name)));
					assert.ok(d.every((x, i) => i === 0 || x >= d[i - 1] - 1), `a higher rank stands further: ${d}`);

					// da-u9nfv — vividn: «yes shift those into another row if needed».
					// Tapped with `end` just inside the bar's left or right edge,
					// where `< Back` or `+ Add state` cannot stand beside it: `end`
					// stays on the tap and that one steps onto another row.
					const margin = (end.right - end.left) / 2 + 4;
					for (const [side, fraction, px, name, other] of [
						['left', 0, margin, '< Back', '+ Add state'],
						['right', 1, -margin, '+ Add state', '< Back']
					]) {
						const t = await tapAt(page, fraction, 'mood', px);
						const [e, moved] = [t.find('false'), t.find(name)];
						assert.ok(Math.abs(e.x - t.tap.x) <= 1 && Math.abs(e.y - t.tap.y) <= 1, `${side}: end at ${e.x},${e.y}, the tap at ${t.tap.x},${t.tap.y}`);
						// Too near the edge for the line, or this proves nothing.
						const room = side === 'left' ? t.tap.x - t.ring.left : t.ring.right - t.tap.x;
						assert.ok(room < margin + GAP + moved.right - moved.left, `${side}: the tap was not near enough the edge`);
						assert.ok(Math.abs(moved.y - e.y) > 1, `${side}: ${name} stayed on end's line`);
						assert.ok(Math.abs(t.find(other).y - e.y) < 1, `${side}: ${other} left end's line`);
						for (const b of t.buttons)
							assert.ok(b.left >= t.ring.left - 1 && b.right <= t.ring.right + 1, `${side}: ${b.name} is outside the bar`);
					}

					// Tinted states; `end`, `< Back` and `+ Add state` are hollow.
					const clear = 'rgba(0, 0, 0, 0)';
					assert.ok(middle.states.every((s) => s.fill !== clear), 'a state is not filled');
					assert.equal(end.fill, clear, '`end` is filled');
					for (const b of [back, add]) assert.ok(middle.states.every((s) => s.fill !== b.fill), `${b.name} is filled like a state`);
				}

				// Logging does not reorder; the States tab does.
				// Timed apart: the app's clock is held, and two switches of one
				// field at one minute collide.
				for (const [s, t] of [['d', '1400'], ['d', '1410'], ['c', '1420'], ['d', '1430']])
					await app.submit(`switch mood ${s} -t ${t}`);
				const logged = await tapAt(page, 0.5);
				assert.deepEqual(logged.buttons.map((b) => b.name), RING, 'logging reordered the states');

				const { saved: saved } = await openSpec(page, 'mood');
				await page.locator(`${SPEC} [data-tab="states"]`).click();
				for (let i = 0; i < 3; i++) await page.locator(`${SPEC} [aria-label="move d up"]`).click();
				await page.locator(`${SPEC} .commit`).click();
				await saved;
				assert.deepEqual(JSON.parse((await app.run(['spec', 'show', 'mood'])).stdout).allowed_states, ['false', 'd', 'a', 'b', 'c']);

				await page.locator(`${BAR} [data-field="mood"]`).waitFor();
				await app.idle();
				const reordered = await tapAt(page, 0.5);
				// d took the place a had, and so on round the ring; `end` stays on the tap.
				assert.deepEqual(
					['d', 'a', 'b', 'c'].map((s) => from(reordered, s)),
					STATES.map((s) => from(logged, s))
				);

				// A state learned while the ring is open is placed too — where a
				// ring opened afresh puts it, not left in a corner until something
				// happens to resize.
				const offset = () =>
					page.locator(`${BAR} .ring`).evaluate((ring) => {
						const at = (sel) => ring.querySelector(sel).getBoundingClientRect();
						const [e, end] = [at('[data-state="e"]'), at('[data-state="false"]')];
						return [Math.round(e.x - end.x), Math.round(e.y - end.y)];
					});
				await page.locator(`${BAR} [data-field="mood"]`).click();
				await page.locator(`${BAR} [data-add-state]`).waitFor();
				await app.submit('spec add-state mood e');
				await page.locator(`${BAR} [data-state="e"]`).waitFor();
				const live = await offset();
				await page.locator(`${BAR} [data-back]`).click();
				await page.locator(`${BAR} [data-field="mood"]`).click();
				await page.locator(`${BAR} [data-state="e"]`).waitFor();
				assert.deepEqual(live, await offset(), 'the learned state was not placed');
			},
			{
				viewport: { width: 390, height: 640 },
				clock: todayAt(15),
				prepare: (context) => context.addInitScript(draft, PAGE)
			}
		);
	});

	// da-8mk9p — vividn: «still radial, just capped on one side and spreading
	// out further in the non capped direction, but the center should still
	// be end». Every field a switch field, so whichever stands on the first
	// row and on the last can be tapped.
	test('on the bar’s first and last rows, `end` stays on the tap and every state is inside the bar', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seedLines(
					app,
					[...OTHERS, 'mood'].map((f) => `spec set ${f} --verb switch --state false ${STATES.map((s) => `--state ${s}`).join(' ')}`)
				);
				await page.locator(`${BAR} [data-field="mood"]`).waitFor();
				await atRows(page, 'bar', 3);
				const rows = await page.locator(BAR).evaluate((bar) => {
					const box = bar.getBoundingClientRect();
					const fields = [...bar.querySelectorAll('[data-field]')].map((b) => {
						const r = b.getBoundingClientRect();
						const x = r.left + r.width / 2 - box.left + bar.scrollLeft;
						// How far short of the bar's middle a scroll leaves it.
						const miss = Math.abs(x - Math.min(Math.max(x, box.width / 2), bar.scrollWidth - box.width / 2));
						return { field: b.dataset.field, top: r.top, miss };
					});
					const tops = fields.map((f) => f.top);
					// The row's field the bar scrolls nearest its middle, so the
					// line has room either side and only the rows are capped.
					const at = (top) => fields.filter((f) => Math.abs(f.top - top) < 1).sort((a, b) => a.miss - b.miss)[0].field;
					return { first: at(Math.min(...tops)), last: at(Math.max(...tops)) };
				});
				for (const [row, field] of Object.entries(rows)) {
					const t = await tapAt(page, 0.5, field);
					const end = t.find('false');
					assert.ok(Math.abs(end.x - t.tap.x) <= 1 && Math.abs(end.y - t.tap.y) <= 1, `${row} row: end at ${end.x},${end.y}, the tap at ${t.tap.x},${t.tap.y}`);
					for (const b of t.buttons)
						assert.ok(
							b.top >= t.ring.top - 1 && b.bottom <= t.ring.bottom + 1 && b.left >= t.ring.left - 1 && b.right <= t.ring.right + 1,
							`${row} row: ${b.name} is outside the bar`
						);
					// Capped: no state on the edge's side of the line, and some beyond it.
					const free = row === 'first' ? 1 : -1;
					assert.ok(t.states.every((s) => free * (s.y - end.y) > -1), `${row} row: a state is on the capped side`);
					assert.ok(t.states.some((s) => free * (s.y - end.y) > 1), `${row} row: nothing on the free side`);
					// The rank still decides who is nearest.
					const d = STATES.map((name) => Math.hypot(...from(t, name)));
					assert.ok(d.every((x, i) => i === 0 || x >= d[i - 1] - 1), `${row} row: a higher rank stands further: ${d}`);
				}
			},
			{
				viewport: { width: 390, height: 640 },
				clock: todayAt(15),
				prepare: (context) => context.addInitScript(draft, PAGE)
			}
		);
	});
});
