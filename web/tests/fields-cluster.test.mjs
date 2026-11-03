// A switch field's states, opened in the collect widget (da-vg8vr,
// da-w6437): the first of its spec in the middle of the room between its
// time and its keys, and the rest ring by ring, nearest first. vividn,
// 2026-11-03: «highest ranked states are in the middle, and then start
// building in circles, still pannable like the current infrastructure».
//
// The spec's order is a rank by distance (`ringOf` in fields.ts); the
// setup widget's ↑ / ↓ beside each state is what reorders them, and
// logging never does. The view as a whole is `collect.test.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ringIn, ringOf } from '../src/lib/fields.ts';
import { startApp, stopApp, withApp, seedLines, todayAt } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';
import { FIELD_FORM, leaveField, openField } from './support/field.mjs';

const H = 30;
const GAP = 6;

/** Every button of a ring as a box. */
function boxes(ring, w) {
	return [
		{ name: 'end', ...ring.end, w: w.end },
		...ring.states.map((s, i) => ({ name: i, ...s, w: w.states[i] }))
	];
}

/** Whether two boxes stand closer than `GAP`. */
const touch = (a, b) => Math.abs(a.x - b.x) < (a.w + b.w) / 2 + GAP - 1e-9 && Math.abs(a.y - b.y) < H + GAP - 1e-9;
/** The pairs that stand closer than `GAP`. */
const overlaps = (list) => list.flatMap((a, i) => list.slice(i + 1).filter((b) => touch(a, b)).map((b) => [a.name, b.name]));
/** How far the centre is from the nearest point of a box. */
const near = (p, wide) => Math.hypot(Math.max(0, Math.abs(p.x) - wide / 2), Math.max(0, Math.abs(p.y) - H / 2));

describe('ringOf', () => {
	const sizes = (n, end = 50, wide = (i) => 40 + 7 * (i % 3)) => ({
		end,
		states: Array.from({ length: n }, (_, i) => wide(i))
	});

	test('`end` the centre; the first state straight up, then down, right and left', () => {
		const ring = ringOf(sizes(4, 50, () => 40), H, GAP);
		assert.deepEqual(ring.end, { x: 0, y: 0 });
		const [up, down, right, left] = ring.states;
		assert.ok(up.x === 0 && up.y < 0, `the first is at ${up.x},${up.y}`);
		assert.ok(down.x === 0 && down.y > 0, `the second is at ${down.x},${down.y}`);
		assert.ok(right.y === 0 && right.x > 0, `the third is at ${right.x},${right.y}`);
		assert.ok(left.y === 0 && left.x < 0, `the fourth is at ${left.x},${left.y}`);
	});

	test('no two buttons touch, for none to forty states', () => {
		for (const end of [50, 30])
			for (let n = 0; n <= 40; n++) {
				const w = sizes(n, end);
				assert.deepEqual(overlaps(boxes(ringOf(w, H, GAP), w)), [], `${n} states, end ${end}`);
			}
	});

	// «a later-ranked state never nearer than an earlier one could have
	// stood»: no spot on the 2px grid that was free when a state was placed
	// is nearer the centre than the one it took.
	test('each state takes the nearest spot free when its turn came', () => {
		const w = sizes(16, 54.75, (i) => (i % 4 === 3 ? 90 : 46.6));
		const ring = ringOf(w, H, GAP);
		const all = boxes(ring, w);
		ring.states.forEach((s, i) => {
			const wide = w.states[i];
			const before = all.filter((b) => b.name === 'end' || (typeof b.name === 'number' && b.name < i));
			const took = near(s, wide);
			for (let x = -2 * Math.ceil(took + wide); x <= 2 * Math.ceil(took + wide); x += 2)
				for (let y = -2 * Math.ceil(took + H); y <= 2 * Math.ceil(took + H); y += 2) {
					const p = { x, y, w: wide };
					if (before.every((b) => !touch(b, p))) assert.ok(near(p, wide) >= took - 1e-9, `state ${i} took ${took}, ${x},${y} was free at ${near(p, wide)}`);
				}
			// And so no later state stands where it would have been nearer.
			for (const later of ring.states.slice(i + 1)) {
				const p = { ...later, w: wide };
				if (before.every((b) => !touch(b, p))) assert.ok(near(p, wide) >= took - 1e-9, `state ${i} at ${took}, a later one at ${near(p, wide)}`);
			}
		});
	});
});

describe('ringIn', () => {
	// The tutorial's `room` on a phone (da-q0lcx): its fields widget's box at
	// 390px, its buttons as drawn, and its back icon's corner.
	const ROOM = { w: 351, h: 88.5, side: 41.6, lid: 36.8 };
	const SIZES = { end: 54.8, states: [87.5, 120.2, 87.5, 95.7, 87.5] };
	const TALL = 33.9;
	/** Whether a ring stands wholly in the room, none under a corner. */
	const held = (ring, room) =>
		[{ ...ring.end, w: SIZES.end }, ...ring.states.map((s, i) => ({ ...s, w: SIZES.states[i] }))].every((b) => {
			const [left, top] = [room.w / 2 + b.x - b.w / 2, room.h / 2 + b.y - TALL / 2];
			return left >= 0 && left + b.w <= room.w && top >= 0 && top + TALL <= room.h && (top >= room.lid || (left >= room.side && left + b.w <= room.w - room.side));
		});

	test('a box too short for the ring holds every state in its rows, between the corners on the top one', () => {
		assert.equal(held(ringOf(SIZES, TALL, GAP), ROOM), false, 'the ring fits: the case is not tested');
		const ring = ringIn(SIZES, TALL, GAP, ROOM);
		assert.ok(held(ring, ROOM), JSON.stringify(ring));
		assert.ok(Math.abs(ring.end.x) < 1, `end moved across, to ${ring.end.x}`);
		// The old corner, «+ Add state» a third of the row: no packing holds them.
		const old = { ...ROOM, side: 129.2, lid: 42.9 };
		assert.equal(held(ringIn(SIZES, TALL, GAP, old), old), false, 'they fit beside the old corner: this test says nothing of it');
	});

	test('a box the ring fits keeps the ring; one nothing fits keeps it too, and pans', () => {
		assert.deepEqual(ringIn(SIZES, TALL, GAP, { ...ROOM, h: 240 }), ringOf(SIZES, TALL, GAP));
		const narrow = { ...ROOM, w: 200 };
		assert.deepEqual(ringIn(SIZES, TALL, GAP, narrow), ringOf(SIZES, TALL, GAP));
	});
});

const PAGE = JSON.stringify(along('column', widgetPane('repl', 'cli'), widgetPane('bar', 'widgetbar')));
const BAR = 'section[data-pane="bar"] .bar';
const VIEW = `${BAR} .collect.open`;
const ROOM = `${VIEW} .room`;
const STATES = ['a', 'b', 'c', 'd'];
/** Every field a switch of the same states, so each opens the same picture: one to tap wherever a field is drawn. */
const FIELDS = 'alfa bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november oscar papa quebec romeo sierra mood'.split(' ');
const SEED = FIELDS.map((f) => `setup ${f} --verb switch --states false,${STATES.join(',')}`);

/** What the open states draw: the room, the box of states, and each state's box, by name. */
const seen = (page) =>
	page.locator(ROOM).evaluate((room) => {
		const box = (el) => {
			const r = el.getBoundingClientRect();
			return { left: r.left, right: r.right, top: r.top, bottom: r.bottom, x: r.left + r.width / 2, y: r.top + r.height / 2 };
		};
		return {
			pane: box(room),
			ring: box(room.querySelector('.ring')),
			buttons: [...room.querySelectorAll('[data-state]')].map((b) => ({ name: b.dataset.state, ...box(b) }))
		};
	});

/** Read the states once placed: the room takes its size as the view's rows
 * mount, and its observer places the states again, so wait out two frames. */
async function placed(page) {
	await page.locator(`${ROOM} [data-state="a"]`).waitFor();
	await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
	return seen(page);
}

/** Open `field` by a press where it is drawn, read the states, close them with Escape. */
async function open(page, field) {
	await page.locator(`${BAR} [data-field="${field}"]`).dispatchEvent('click');
	const t = await placed(page);
	await page.keyboard.press('Escape');
	await page.locator(VIEW).waitFor({ state: 'detached' });
	return { ...t, find: (name) => t.buttons.find((b) => b.name === name) };
}

/** Drag from the room's middle by dx, dy, in steps, and read what is drawn. */
async function dragBy(page, dx, dy) {
	const from = await page.locator(ROOM).boundingBox();
	const [x, y] = [from.x + from.width / 2, from.y + from.height / 2];
	await page.mouse.move(x, y);
	await page.mouse.down();
	for (let i = 1; i <= 5; i++) await page.mouse.move(x + (dx * i) / 5, y + (dy * i) / 5);
	await page.mouse.up();
	return seen(page);
}

/** Each state's center from the room's, to the pixel. */
const offsets = (t) => Object.fromEntries(t.buttons.map((b) => [b.name, [Math.round(b.x - t.pane.x), Math.round(b.y - t.pane.y)]]));

/** The pills on the input bar, by id. */
const pills = (page) => page.locator('.prompt-bar .pill').evaluateAll((ps) => ps.map((p) => p.dataset.pill));

const options = (width = 390) => ({ viewport: { width, height: 640 }, clock: todayAt(15), prepare: (context) => context.addInitScript(draft, PAGE) });

// da-w6437: a field opened in collect stands its states in the room between
// its time and its keys, the first of the spec in the middle — vividn:
// «highest ranked states are in the middle, and then start building in
// circles, still pannable like the current infrastructure».
describe('a switch field’s states in collect, the first in the middle', () => {
	before(startApp);
	after(stopApp);

	for (const width of [390, 1280])
		test(`${width}px: the same picture whichever field is pressed and wherever it stands, the first state in the middle`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await seedLines(app, SEED);
				await page.locator(`${BAR} [data-field="mood"]`).waitFor();
				const fields = await page.locator(BAR).evaluate((bar) => [...bar.querySelectorAll('[data-field]')].map((b) => b.dataset.field));
				const picked = [fields[0], fields[Math.floor(fields.length / 2)], fields.at(-1)];
				const first = await open(page, picked[0]);
				assert.deepEqual(first.buttons.map((b) => b.name), STATES, '`end` is the commands’, not a state');
				const a = first.find('a');
				assert.ok(Math.abs(a.x - first.pane.x) <= 1 && Math.abs(a.y - first.pane.y) <= 1, `a at ${a.x},${a.y}, the room's center at ${first.pane.x},${first.pane.y}`);
				// The rank: b no further from a than c, c than d, to the nearest point of each.
				const reach = (b) => Math.hypot(Math.max(0, b.left - a.x, a.x - b.right), Math.max(0, b.top - a.y, a.y - b.bottom));
				const d = STATES.slice(1).map((s) => reach(first.find(s)));
				assert.ok(d.every((x, i) => i === 0 || x >= d[i - 1] - 1), `a higher rank stands further: ${d}`);
				for (const field of picked.slice(1)) assert.deepEqual(offsets(await open(page, field)), offsets(first), `${field}: not ${picked[0]}'s picture`);
				// A key's press opens the same.
				await page.locator(`${BAR} [data-field="mood"]`).focus();
				await page.keyboard.press('Enter');
				assert.deepEqual(offsets(await placed(page)), offsets(first), 'by key: not the same picture');
			}, options(width));
		});

	test('22 states at 390px: a drag pans them and presses nothing, and a tap on a state logs it', async () => {
		// Long names, so they run past a phone's width as well as the room's height.
		const many = Array.from({ length: 22 }, (_, i) => `a_rather_long_state_${String(i).padStart(2, '0')}`);
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, [...FIELDS.slice(0, 4).map((f) => `setup ${f} --verb occur`), `setup mood --verb switch --states false,${many.join(',')}`]);
			const field = page.locator(`${BAR} [data-field="mood"]`);
			await field.waitFor();
			await field.click();
			await page.locator(`${ROOM} [data-state="${many[0]}"]`).waitFor();
			const was = await seen(page);
			assert.ok(was.ring.left < was.pane.left && was.ring.right > was.pane.right, 'the states fit across: the case is not tested');
			const before = (await app.echoes()).length;
			// Started ON the middle state, well past the slop: to the left.
			const moved = await dragBy(page, -30, 0);
			const got = moved.ring.left - was.ring.left;
			assert.ok(Math.abs(got + Math.min(30, was.ring.right - was.pane.right)) <= 1, `the states moved ${got}`);
			await app.idle();
			assert.equal((await app.echoes()).length, before, 'a drag pressed the state it started on');
			assert.deepEqual(await pills(page), ['field'], 'a drag chose a state');

			// Past their extent either way, their far edge stops at the room's,
			// like a scroll; down they fit, so a drag there moves nothing.
			const strokes = async (dx, dy) => {
				const n = Math.max(1, Math.ceil(Math.max(Math.abs(dx), Math.abs(dy)) / 150));
				let t;
				for (let i = 0; i < n; i++) t = await dragBy(page, dx / n, dy / n);
				return t;
			};
			const span = was.ring.right - was.ring.left;
			const left = await strokes(-span, 0);
			assert.ok(Math.abs(left.ring.right - left.pane.right) <= 1, `panned past the states: their right ${left.ring.right}, the room's ${left.pane.right}`);
			const right = await strokes(2 * span, 0);
			assert.ok(Math.abs(right.ring.left - right.pane.left) <= 1, `panned past the states the other way: their left ${right.ring.left}, the room's ${right.pane.left}`);
			assert.ok(right.ring.bottom - right.ring.top <= right.pane.bottom - right.pane.top, 'the states do not fit down: the case below is not tested');
			const down = await strokes(0, 80);
			assert.equal(down.ring.top, right.ring.top, 'states that fit down were panned down');
			await strokes(was.ring.left - right.ring.left, 0);
			await app.idle();
			assert.equal((await app.echoes()).length, before, 'a pan pressed something');

			// A press that does not travel is a press: a state logs at once
			// (vividn, 2026-11-03 16:07), and collect is back at rest.
			await page.locator(`${ROOM} [data-state="${many[0]}"]`).click();
			await page.locator(VIEW).waitFor({ state: 'detached' });
			await app.idle();
			assert.equal((await app.echoes()).length, before + 1, 'the state did not run');
			assert.equal(JSON.parse((await app.run(['tail', 'mood', '-n', '1', '--json'])).stdout).rows[0].state, many[0]);
		}, options());
	});

	// da-71p3u: the ring overruns its room, which clips it; a reveal of a
	// state it clips — a focus, a click's scroll into view — is a pan, so
	// neither the room nor anything around the widget is left scrolled.
	test('22 states at 390px: focusing a state the room clips pans it toward sight, and the room does not scroll', async () => {
		const many = Array.from({ length: 22 }, (_, i) => `a_rather_long_state_${String(i).padStart(2, '0')}`);
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, [...FIELDS.slice(0, 4).map((f) => `setup ${f} --verb occur`), `setup mood --verb switch --states false,${many.join(',')}`]);
			const field = page.locator(`${BAR} [data-field="mood"]`);
			await field.waitFor();
			await field.click();
			await page.locator(`${ROOM} [data-state="${many[0]}"]`).waitFor();
			const outside = (b, pane) =>
				Math.max(0, pane.left - b.left) + Math.max(0, b.right - pane.right) + Math.max(0, pane.top - b.top) + Math.max(0, b.bottom - pane.bottom);
			const t = await seen(page);
			// Wholly past the room's right edge: a focus reveals only what is
			// out of sight, and a reveal scrolls a box toward its end — nothing
			// stands before its start for one to scroll back to.
			const clipped = t.buttons.find((b) => b.left >= t.pane.right && b.top >= t.pane.top && b.bottom <= t.pane.bottom);
			assert.ok(clipped, 'no state the room clips: the case is not tested');
			await page.evaluate(() => document.activeElement?.blur());
			await page.locator(`${ROOM} [data-state="${clipped.name}"]`).focus();
			await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
			const scrolled = await page.locator(ROOM).evaluate((el) => {
				const out = [];
				for (let e = el; e; e = e.parentElement) if (e.scrollTop || e.scrollLeft) out.push(`${e.className}: ${e.scrollLeft},${e.scrollTop}`);
				return out;
			});
			assert.deepEqual(scrolled, [], 'focusing scrolled');
			const now = await seen(page);
			const after = outside(now.buttons.find((b) => b.name === clipped.name), now.pane);
			assert.ok(after < outside(clipped, t.pane) - 1, `${clipped.name} was not panned toward sight`);
		}, options());
	});

	test('Escape and back put the fields back as they were, logging nothing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const field = page.locator(`${BAR} [data-field="mood"]`);
			await field.waitFor();
			const before = (await app.echoes()).length;
			for (const how of ['Escape', 'back']) {
				await field.dispatchEvent('click');
				await page.locator(`${ROOM} [data-state="a"]`).waitFor();
				assert.equal(await field.count(), 0, `${how}: the fields are still drawn under the states`);
				if (how === 'Escape') await page.keyboard.press('Escape');
				else await page.locator(`${VIEW} [data-back]`).click();
				await page.locator(VIEW).waitFor({ state: 'detached' });
				await app.idle();
				assert.equal((await app.echoes()).length, before, `${how} logged something`);
				assert.ok(await field.isVisible(), `${how}: the fields are not back`);
				assert.deepEqual(await pills(page), [], `${how}: a pill stayed`);
			}
		}, options());
	});

	test('reordered by the setup widget alone, and a state learned while they are open placed where a fresh open puts it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seedLines(app, SEED);
			const field = page.locator(`${BAR} [data-field="mood"]`);
			await field.waitFor();
			// Logging does not reorder. Timed apart: the app's clock is held,
			// and two switches of one field at one minute collide.
			const first = await open(page, 'mood');
			for (const [s, t] of [['d', '1400'], ['d', '1410'], ['c', '1420'], ['d', '1430']]) await app.submit(`switch mood ${s} -t ${t}`);
			await app.idle();
			const logged = await open(page, 'mood');
			assert.deepEqual(offsets(logged), offsets(first), 'logging moved a state');

			const { saved } = await openField(page, 'mood');
			await page.locator(`${FIELD_FORM} [aria-label="move d: drag, or the arrow keys"]`).focus();
			for (let i = 0; i < 3; i++) await page.keyboard.press('ArrowUp');
			await page.locator(`${FIELD_FORM} .commit`).click();
			await saved;
			assert.deepEqual(JSON.parse((await app.run(['setup', 'mood', '--json'])).stdout).allowed_states, ['false', 'd', 'a', 'b', 'c']);
			await leaveField(page);
			await field.waitFor();
			await app.idle();
			// d took the place a had, and so on round.
			const [now, was] = [offsets(await open(page, 'mood')), offsets(logged)];
			assert.deepEqual(['d', 'a', 'b', 'c'].map((s) => now[s]), STATES.map((s) => was[s]));

			const offset = () =>
				page.locator(`${ROOM} .ring`).evaluate((ring) => {
					const at = (sel) => ring.querySelector(sel).getBoundingClientRect();
					const [e, d] = [at('[data-state="e"]'), at('[data-state="d"]')];
					return [Math.round(e.x - d.x), Math.round(e.y - d.y)];
				});
			await field.dispatchEvent('click');
			await page.locator(`${ROOM} [data-state="d"]`).waitFor();
			await app.submit('setup mood --states false,d,a,b,c,e');
			await page.locator(`${ROOM} [data-state="e"]`).waitFor();
			const live = await offset();
			await page.keyboard.press('Escape');
			await page.locator(VIEW).waitFor({ state: 'detached' });
			await field.dispatchEvent('click');
			await page.locator(`${ROOM} [data-state="e"]`).waitFor();
			assert.deepEqual(live, await offset(), 'the learned state was not placed');
		}, options());
	});
});
