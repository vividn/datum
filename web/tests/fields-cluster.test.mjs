// A switch field's states stand RADIALLY around the tap (da-7q3ka). vividn:
// «better to distribute them radially around the field», `< Back` and
// `+ Add state` «just two more buttons … in this radial cluster, on the
// left and right sides», and the `false` state, `end`, «in the same place
// as where the field button was».
//
// The spec's order is a rank by distance (`ringOf` in fields.ts);
// near an edge the ring shifts whole; the States tab's ↑ / ↓ is what
// reorders it, and logging never does. The list's other behaviour — Back,
// `+`, Escape, the colours — is `fields-states.test.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ringOf } from '../src/lib/fields.ts';
import { startApp, stopApp, withApp, seedLines, todayAt } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';
import { atRows } from './support/rows.mjs';

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

	test('`end` on the tap, `< Back` and `+ Add state` either side of it on its line', () => {
		const ring = ringOf(sizes(5), H, 72, GAP);
		assert.deepEqual(ring.end, { x: 0, y: 0 });
		assert.deepEqual(ring.back, { x: -(25 + GAP + 40), y: 0 });
		assert.deepEqual(ring.add, { x: 25 + GAP + 55, y: 0 });
	});

	test('ranked by distance: the first state nearest, ties clockwise from twelve o’clock', () => {
		const ring = ringOf(sizes(8, 50, () => 40), H, 72, GAP);
		const d = ring.states.map(far);
		assert.ok(d.every((x, i) => i === 0 || x >= d[i - 1]), `further for a higher rank: ${d}`);
		const [a, b] = ring.states;
		assert.ok(a.x === 0 && a.y < 0, 'the first is not straight up');
		assert.ok(b.x === 0 && b.y > 0, 'the second is not straight down');
	});

	test('without `end`, the first state takes the tap', () => {
		const ring = ringOf(sizes(4, null), H, 72, GAP);
		assert.equal(ring.end, null);
		assert.deepEqual(ring.states[0], { x: 0, y: 0 });
	});

	test('no two buttons touch, with `end` or without, for none to a dozen states', () => {
		for (const end of [50, null])
			for (let n = 0; n <= 12; n++) {
				const w = sizes(n, end);
				assert.deepEqual(overlaps(boxes(ringOf(w, H, 72, GAP), w)), [], `${n} states, end ${end}`);
			}
	});

	test('a bar too short for a row above lays them all on the tap’s line', () => {
		const w = sizes(4);
		const ring = ringOf(w, H, 10, GAP);
		assert.ok([ring.back, ring.add, ...ring.states].every((s) => s.y === 0), 'off the line');
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
 * (as near as the scroll reaches), press it there, and read where the ring
 * landed; then Back.
 *
 * The press is the element's own click, not the mouse's: a bar one row
 * high is a viewport so short the bottom bar lies over it, and the widget
 * reads where the BUTTON is, not where the pointer was.
 */
async function tapAt(page, fraction) {
	const tap = await page.locator(BAR).evaluate((bar, fraction) => {
		const box = bar.getBoundingClientRect();
		const button = () => bar.querySelector('[data-field="mood"]');
		const m = button().getBoundingClientRect();
		bar.scrollLeft += m.left + m.width / 2 - (box.left + fraction * box.width);
		const at = button().getBoundingClientRect();
		button().click();
		return { x: at.left + at.width / 2, y: at.top + at.height / 2 };
	}, fraction);
	await page.locator(`${BAR} [data-add-state]`).waitFor();
	const seen = await page.locator(BAR).evaluate((bar) => {
		const ring = bar.querySelector('.ring').getBoundingClientRect();
		return {
			ring: { left: ring.left, right: ring.right },
			buttons: [...bar.querySelectorAll('.ring button')].map((b) => {
				const r = b.getBoundingClientRect();
				return {
					name: b.dataset.state ?? b.textContent.trim(),
					x: r.left + r.width / 2,
					y: r.top + r.height / 2,
					left: r.left,
					right: r.right,
					fill: getComputedStyle(b).backgroundColor
				};
			})
		};
	});
	await page.locator(`${BAR} [data-back]`).dispatchEvent('click');
	await page.locator(`${BAR} [data-field="mood"]`).waitFor();
	const find = (name) => seen.buttons.find((b) => b.name === name);
	return { tap, ...seen, find, states: STATES.map(find) };
}

/** `name`'s centre relative to `end`'s, to the pixel. */
const from = (t, name) => [Math.round(t.find(name).x - t.find('false').x), Math.round(t.find(name).y - t.find('false').y)];
const shape = (t) => t.buttons.map((b) => from(t, b.name));

describe('a switch field’s states around the tap', () => {
	before(startApp);
	after(stopApp);

	test('`end` on the tap, one shape from anywhere, shifted whole at an edge, reordered by the States tab alone', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seedLines(app, SEED);
				await page.locator(`${BAR} [data-field="mood"]`).waitFor();

				for (const want of [1, 3]) {
					await atRows(page, 'bar', want);
					const [middle, left, right] = [await tapAt(page, 0.5), await tapAt(page, 0.02), await tapAt(page, 0.98)];
					assert.deepEqual(middle.buttons.map((b) => b.name), RING, `${want} rows: not in the spec's order`);
					// The same shape from every tap.
					for (const t of [left, right]) assert.deepEqual(shape(t), shape(middle), `${want} rows: the shape moved`);
					// `< Back` and `+ Add state` flank `end` on its own line.
					const [back, end, add] = [middle.find('< Back'), middle.find('false'), middle.find('+ Add state')];
					assert.ok(back.right <= end.left && end.right <= add.left, `${want} rows: < Back, end, + Add state are not left to right`);
					assert.ok(Math.abs(back.y - end.y) < 1 && Math.abs(add.y - end.y) < 1, `${want} rows: not on one line`);
					if (want === 1) continue;

					// `end` on the tap, and a ring: states above AND below its line.
					assert.ok(Math.abs(end.x - middle.tap.x) <= 1, `end at ${end.x}, the tap at ${middle.tap.x}`);
					assert.ok(middle.states.some((s) => s.y < end.y - 1) && middle.states.some((s) => s.y > end.y + 1), 'not around the tap');
					// The spec's order is a rank: a no further than b, b than c…
					const d = STATES.map((name) => Math.hypot(...from(middle, name)));
					assert.ok(d.every((x, i) => i === 0 || x >= d[i - 1] - 1), `a higher rank stands further: ${d}`);
					// At an edge the ring is shifted inside, whole — so the taps really
					// were at edges, or this proves nothing.
					for (const [t, side] of [
						[left, 'left'],
						[right, 'right']
					]) {
						const lo = Math.min(...t.buttons.map((b) => b.left));
						const hi = Math.max(...t.buttons.map((b) => b.right));
						assert.ok(lo >= t.ring.left - 1 && hi <= t.ring.right + 1, `${side}: a button is outside the bar`);
						assert.ok(Math.abs(t.find('false').x - t.tap.x) > 1, `${side}: the tap was not near enough the edge to shift`);
					}
					assert.ok(left.find('false').x > left.tap.x && right.find('false').x < right.tap.x, 'shifted the wrong way');

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

				const saved = page.evaluate(() => window.datum.spec('mood'));
				await page.locator('dialog.spec [data-tab="states"]').click();
				for (let i = 0; i < 3; i++) await page.locator('dialog.spec [aria-label="move d up"]').click();
				await page.locator('dialog.spec .commit').click();
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
});
