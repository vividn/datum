// A switch field's states stand RADIALLY around the tap (da-7q3ka). vividn:
// «better to distribute them radially around the field», `< Back` «just
// … in this radial cluster, on the left», and the `false` state, `end`,
// «in the same place as where the field button was». `+ Add state` is
// past the states on `end`'s line (da-cwieh): «to the far right of all
// the states, rather than taking up a premium spot close to the thumb».
//
// The spec's order is a rank by distance (`ringOf` in fields.ts);
// near an edge the ring is capped, not shifted, so `end` stays on the tap
// (da-8mk9p); the setup widget's ↑ / ↓ beside each state is what reorders it, and logging
// never does. The list's other behaviour — Back,
// `+`, Escape, the colours — is `fields-states.test.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ringOf } from '../src/lib/fields.ts';
import { ON_COLLECT, startApp, stopApp, withApp, seedLines, todayAt } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';
import { atRows } from './support/rows.mjs';
import { FIELD_FORM, leaveField, openField } from './support/field.mjs';

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

	/** The right edge of the rightmost button but `+ Add state`. */
	const rightmost = (ring, w) => Math.max(...boxes(ring, w).filter((b) => b.name !== 'add').map((b) => b.x + b.w / 2));

	test('`end` on the tap, `< Back` beside it on its line, `+ Add state` on its line past every state', () => {
		const w = sizes(5);
		const ring = ringOf(w, H, room(72), GAP);
		assert.deepEqual(ring.end, { x: 0, y: 0 });
		assert.deepEqual(ring.back, { x: -(25 + GAP + 40), y: 0 });
		assert.deepEqual(ring.add, { x: rightmost(ring, w) + GAP + 55, y: 0 });
		// da-cwieh: the spot right of `end` is a state's now.
		assert.ok(ring.states.some((s) => s.y === 0 && s.x > 0 && s.x < ring.add.x), 'no state on the line right of `end`');
	});

	test('ranked by distance: the first state nearest, ties clockwise from twelve o’clock', () => {
		const ring = ringOf(sizes(8, 50, () => 40), H, room(72), GAP);
		const d = ring.states.map(far);
		assert.ok(d.every((x, i) => i === 0 || x >= d[i - 1]), `further for a higher rank: ${d}`);
		const [a, b] = ring.states;
		assert.ok(a.x === 0 && a.y < 0, 'the first is not straight up');
		assert.ok(b.x === 0 && b.y > 0, 'the second is not straight down');
	});

	// da-cwieh: a never-false field's GAP is `end`'s width at the centre,
	// so the ring is the one it would be with `end` drawn.
	test('the centre is `end`’s, drawn or a gap: no state takes it, the first state nearest it', () => {
		const ring = ringOf(sizes(4), H, room(72), GAP);
		assert.deepEqual(ring.end, { x: 0, y: 0 });
		assert.ok(ring.states.every((s) => s.x !== 0 || s.y !== 0), 'a state took the centre');
		assert.deepEqual(ring.states[0], { x: 0, y: -(H + GAP) });
	});

	test('no two buttons touch, for none to a dozen states', () => {
		for (const end of [50, 30])
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
		const r = room(10, { left: 25 + GAP + 80 + 4 });
		const ring = ringOf(w, H, r, GAP);
		assert.deepEqual(ring.end, { x: 0, y: 0 });
		assert.ok(inside(ring, w, r), 'a button is past an edge');
		assert.deepEqual(overlaps(boxes(ring, w)), []);
	});

	// da-cwieh — vividn: «move "Add state" to the far right of all the states».
	test('capped right: the states stay inside, and `+ Add state` goes past the edge on the line', () => {
		const w = sizes(6);
		const r = room(72, { right: 25 + GAP + 60 });
		const ring = ringOf(w, H, r, GAP);
		assert.deepEqual(ring.end, { x: 0, y: 0 });
		assert.ok(
			boxes(ring, w).every((b) => b.name === 'add' || b.x + b.w / 2 <= r.right + 1e-9),
			'a button but `+ Add state` is past the edge'
		);
		assert.deepEqual(ring.add, { x: rightmost(ring, w) + GAP + 55, y: 0 });
		assert.ok(ring.add.x + 55 > r.right, '`+ Add state` is inside: the case is not tested');
		assert.deepEqual(overlaps(boxes(ring, w)), []);
	});

	// da-u9nfv — vividn: «yes shift those into another row if needed».
	for (const [edge, name, sign] of [['left', 'back', -1]])
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
				assert.deepEqual(ring.add.y, 0, '`+ Add state` left the line');
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
		assert.deepEqual(ring.add, { x: rightmost(ring, w) + GAP + 55, y: 0 });
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
	...OTHERS.map((f) => `setup ${f} --verb occur`),
	`setup mood --verb switch --states false,${STATES.join(',')}`
];

/**
 * Scroll the bar until mood's centre is at `fraction` of the bar's width
 * and `px` more (as near as the scroll reaches), press it there, and read where the ring
 * landed; then Back.
 *
 * The press is the element's own click, not the mouse's: a bar one row
 * high is a viewport so short the bottom bar lies over it, and a click
 * with no pointer (`detail` 0, as a key's) stands the ring on the BUTTON's
 * center (da-rgyrj).
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

	test('`end` on the tap, at the left edge too, `< Back` on another row there, reordered by the setup widget alone', async () => {
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
					// Tapped with `end` just inside the bar's left edge, where
					// `< Back` cannot stand beside it: `end` stays on the tap and
					// `< Back` steps onto another row. `+ Add state` is past the
					// states on `end`'s line (da-cwieh), so it never needs to.
					const margin = (end.right - end.left) / 2 + 4;
					for (const [side, fraction, px, name, other] of [['left', 0, margin, '< Back', '+ Add state']]) {
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

				// Logging does not reorder; the setup widget does.
				// Timed apart: the app's clock is held, and two switches of one
				// field at one minute collide.
				for (const [s, t] of [['d', '1400'], ['d', '1410'], ['c', '1420'], ['d', '1430']])
					await app.submit(`switch mood ${s} -t ${t}`);
				const logged = await tapAt(page, 0.5);
				assert.deepEqual(logged.buttons.map((b) => b.name), RING, 'logging reordered the states');

				const { saved: saved } = await openField(page, 'mood');
				await page.locator(`${FIELD_FORM} [aria-label="move d: drag, or the arrow keys"]`).focus();
				for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowUp');
				await page.locator(`${FIELD_FORM} .commit`).click();
				await saved;
				assert.deepEqual(JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout).allowed_states, ['false', 'd', 'a', 'b', 'c']);
				await leaveField(page);

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
				await app.submit('setup mood --states false,d,a,b,c,e');
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
					[...OTHERS, 'mood'].map((f) => `setup ${f} --verb switch --states false,${STATES.join(',')}`)
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
					// `+ Add state` past the states may run past the right edge (da-cwieh).
					for (const b of t.buttons.filter((b) => b.name !== '+ Add state'))
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

	// da-rgyrj — vividn: «It should appear beneath the tap location instead.
	// So you can consistently tap tap to end and the movement is the same to
	// change to a certain field regardless where the initial tap is.» A real
	// pointer's press, near each end of a long name; a key's has no spot, so
	// the button's center.
	test('a long field: `end` stands under the tap at either end of it, and a second tap there ends it; a key centers it', async () => {
		const long = 'the_tutorial_mood_with_a_long_name';
		await withApp(
			async (app) => {
				const { page } = app;
				await seedLines(app, [
					...OTHERS.slice(0, 2).map((f) => `setup ${f} --verb occur`),
					`setup ${long} --verb switch --states false,${STATES.join(',')}`,
					`switch ${long} a -t 1400`
				]);
				const button = page.locator(`${BAR} [data-field="${long}"]`);
				await button.waitFor();
				await atRows(page, 'bar', 3);
				const at = await button.boundingBox();
				assert.ok(at.width > 150, `${long} is ${at.width}px wide: not long enough to tell its center from its ends`);
				const end = page.locator(`${BAR} [data-state="false"]`);
				const center = async () => {
					const b = await end.boundingBox();
					return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
				};
				const y = at.y + at.height / 2;
				// In from each end by more than half of `end`, so a bar's edge
				// never caps it (da-8mk9p) and the tap alone decides.
				for (const [side, x] of [
					['left', at.x + 40],
					['right', at.x + at.width - 40]
				]) {
					await page.mouse.click(x, y);
					await end.waitFor();
					const c = await center();
					assert.ok(Math.abs(c.x - x) <= 2 && Math.abs(c.y - y) <= 2, `${side}: end at ${c.x},${c.y}, the tap at ${x},${y}`);
					const before = (await app.echoes()).length;
					await page.mouse.click(x, y);
					await button.waitFor();
					await app.idle();
					const echoes = await app.echoes();
					assert.equal(echoes.length, before + 1, `${side}: the second tap logged nothing`);
					assert.equal(echoes.at(-1), `end ${long}`, `${side}: the second tap did not end it`);
					// The clock is held: two logs of one field at one minute collide.
					await app.clock.forward(60_000);
					await app.submit(`switch ${long} a`);
					await app.clock.forward(60_000);
				}

				await button.focus();
				await page.keyboard.press('Enter');
				await end.waitFor();
				const c = await center();
				const mid = { x: at.x + at.width / 2, y };
				assert.ok(Math.abs(c.x - mid.x) <= 2 && Math.abs(c.y - mid.y) <= 2, `key: end at ${c.x},${c.y}, the button's center at ${mid.x},${mid.y}`);
			},
			{
				viewport: { width: 390, height: 640 },
				clock: todayAt(15),
				prepare: (context) => context.addInitScript(draft, PAGE)
			}
		);
	});

	// da-hygvc: the same on a phone's «collect», whose fields pane is half
	// the screen. A name longer than the pane is shortened inside it, and a
	// tap at its visible right end, where `end` cannot be centered on the
	// tap without leaving the box, still ends on a second tap there. What
	// lies under the point is asked as well as what the tap logged: the
	// browser's touch adjustment hands a tap that misses `end` by a pixel to
	// it, and a finger's may not.
	test('a phone: a field longer than its pane stays inside it, and tap-tap ends near the pane’s edge as well as its left end', async () => {
		const long = 'a_rather_long_mood_field_name';
		await withApp(
			async (app) => {
				const { page } = app;
				for (const line of [`setup ${long} --verb switch --states false,calm,busy`, `switch ${long} calm -t 1400`])
					assert.equal((await app.runLine(line)).code, 0, line);
				await app.reload();
				const pane = page.locator('section.pane[data-title="fields"]');
				const button = pane.locator(`.bar [data-field="${long}"]`);
				await button.waitFor();
				const box = await pane.boundingBox();
				const at = await button.boundingBox();
				assert.ok(at.x >= box.x - 0.5 && at.x + at.width <= box.x + box.width + 0.5, `${long} at ${at.x}–${at.x + at.width}, its pane ${box.x}–${box.x + box.width}`);
				// Shortened as it is drawn, not as it is named.
				assert.ok((await button.getAttribute('title')).includes(long), 'the title lost the full name');
				assert.ok((await button.textContent()).includes(long), 'the accessible name lost the full name');
				const end = pane.locator('[data-state="false"]');
				const y = at.y + at.height / 2;
				const under = (x, sel) => page.evaluate(([x, y, sel]) => !!document.elementFromPoint(x, y)?.closest(sel), [x, y, sel]);
				for (const [side, x, centered] of [
					['left', at.x + at.width * 0.12, true],
					['right', box.x + box.width - 12, false]
				]) {
					assert.ok(await under(x, `[data-field="${long}"]`), `${side}: the tap at ${x} is not on ${long}`);
					await page.touchscreen.tap(x, y);
					await end.waitFor();
					const b = await end.boundingBox();
					const c = b.x + b.width / 2;
					// On the right `end` cannot stand on the tap, which is the case.
					assert.equal(Math.abs(c - x) <= 2, centered, `${side}: end at ${c}, the tap at ${x}`);
					assert.ok(await under(x, '[data-state="false"]'), `${side}: end is not under the tap at ${x}`);
					// The REPL is folded on a phone: the log is read, not echoed.
					const before = (await app.entries()).length;
					await page.touchscreen.tap(x, y);
					await button.waitFor();
					await app.idle();
					const logged = await app.entries();
					assert.equal(logged.length, before + 1, `${side}: the second tap logged nothing`);
					assert.deepEqual([logged[0].field, logged[0].state], [long, false], `${side}: the second tap did not end it`);
					// The clock is held: two logs of one field at one minute collide.
					await app.clock.forward(60_000);
					assert.equal((await app.runLine(`switch ${long} calm`)).code, 0);
					await app.clock.forward(60_000);
				}
			},
			{ viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT, clock: todayAt(15) }
		);
	});

	// da-bstjx — vividn: «Prioritize using visible space over having to
	// scroll. Right now in a 8 row high fields widget, the available states
	// of a field only go into 4 rows». More states than five rows hold on a
	// phone, fewer than eight do (38 of these fit, measured at 390px): they
	// fill the bar's rows and none scrolls.
	test('in an 8-row bar on a phone, the states fill its rows before any scrolls', async () => {
		const many = Array.from({ length: 34 }, (_, i) => `s${i}`);
		// Enough fields more to overrun eight rows, so the bar scrolls and the
		// tap is at its middle: fewer stand centered (da-syu4f), where the tap
		// falls by chance on a spot `ringOf` packs one state short of the
		// rows (da-65210).
		const more = Array.from({ length: 20 }, (_, i) => `x${i}`);
		await withApp(
			async (app) => {
				const { page } = app;
				await seedLines(app, [
					...[...OTHERS, ...more].map((f) => `setup ${f} --verb occur`),
					`setup mood --verb switch --states false,${many.join(',')}`
				]);
				await page.locator(`${BAR} [data-field="mood"]`).waitFor();
				await atRows(page, 'bar', 8, Infinity);
				const t = await tapAt(page, 0.5);
				const box = await page.locator(BAR).boundingBox();
				assert.ok(Math.abs(t.tap.x - (box.x + box.width / 2)) < 2, `the tap is not at the bar's middle: ${t.tap.x}`);
				const end = t.find('false');
				const states = many.map(t.find);
				const row = end.bottom - end.top + GAP;
				const rows = (top, bottom) => Math.round((bottom - top + GAP) / row);
				const used = rows(Math.min(...states.map((s) => s.top)), Math.max(...states.map((s) => s.bottom)));
				assert.equal(used, rows(t.ring.top, t.ring.bottom), `the states use ${used} of the bar's rows`);
				for (const s of states) assert.ok(s.left >= t.ring.left - 1 && s.right <= t.ring.right + 1, `${s.name} is scrolled out of the bar`);
			},
			{
				viewport: { width: 390, height: 640 },
				clock: todayAt(15),
				prepare: (context) => context.addInitScript(draft, PAGE)
			}
		);
	});
});
