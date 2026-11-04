// Collect at rest (da-lw7ue). vividn, 2026-11-04: «on the far left (maybe
// 20%) of the collect widget, let's draw the current state (so like the now
// section of the nowview), then have this active section with a dotted line.
// Freeze this section. So then you pan to the right to see more and more
// distantly used fields, but can always see the active ones», «no vertical
// panning. Everything should be locked to it's proper y positioning», and of
// a field ending: «Since it is the most recent it should always get the first
// slot». At 16:05: «scrolling should have some momentum», and 16:11: «clicking
// on a piece of data in the now section of the collect widget should select
// its field, not the entry».
//
// The arrangement is pure (`atRest` in fields-rest-pure.test.mjs); this is
// the built app, read off where each field stands.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, withApp, todayAt } from './support/app.mjs';
import { PHONE, pickOption, swipe } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const ROOM = 'section.pane[data-title="collect"] .bar .collect .room';

/** A phone on collect, its main tab, tall enough for rows. */
const AT_REST = { ...PHONE, home: ON_COLLECT, clock: todayAt(15) };

/** A field with its band set, the way a spec sets it. */
const at = (field, verb, y, more = {}) => `setup ${field} --from-json '${JSON.stringify({ field, verb, y, height: 0.05, ...more })}'`;

/**
 * Run `lines`, a minute apart on the held clock, so each is used after the
 * one before it; then load, which sorts by recency.
 */
async function seeded(app, lines) {
	for (const line of lines) {
		assert.equal((await app.runLine(line)).code, 0, line);
		await app.clock.forward(60_000);
	}
	await app.reload();
	await app.page.locator(`${ROOM} .spot.shelf`).first().waitFor();
	await app.idle();
}

/** Log `line` as another tab would, and let the page hear it. */
async function logs(app, line) {
	assert.equal((await app.runLine(line)).code, 0, line);
	await app.redraw();
}

/** Every field at rest: its row, its section, and where it is drawn. */
const shelf = (page) =>
	page.locator(`${ROOM} .spot.shelf`).evaluateAll((spots) =>
		spots.map((s) => {
			const r = s.getBoundingClientRect();
			return { field: s.querySelector('[data-field]').dataset.field, row: Number(s.dataset.row), section: s.dataset.section, left: r.left, right: r.right };
		})
	);

/** Each row's fields of `section`, left to right. */
const rowsOf = (spots, section) => {
	const rows = new Map();
	for (const s of [...spots].filter((s) => s.section === section).sort((a, b) => a.left - b.left))
		rows.set(s.row, [...(rows.get(s.row) ?? []), s.field]);
	return Object.fromEntries(rows);
};

/** Wait until the fields stand still: no glide in flight. */
const settled = async (page) => {
	let last = null;
	await until(
		async () => JSON.stringify(await shelf(page)),
		(now) => {
			const same = now === last;
			last = now;
			return same;
		}
	);
	return JSON.parse(last);
};

// High: `mood` (a switch) and `tea`, `coffee`, used in that order; low:
// `work` (a block) and `walk`, `read`. So coffee and read are the most recent
// of their rows, and mood and work, once on, are active.
const SEED = [
	at('mood', 'switch', 0.05, { allowed_states: ['good', 'bad'] }),
	at('tea', 'occur', 0.05),
	at('coffee', 'occur', 0.05),
	at('work', 'start', 0.9),
	at('walk', 'occur', 0.9),
	at('read', 'occur', 0.9),
	'occur tea',
	'occur coffee',
	'occur walk',
	'occur read'
];

describe('collect at rest', () => {
	test('nothing active: no outline, no strip, each row by recency and locked to y', async () => {
		await withApp(async (app) => {
			await seeded(app, SEED);
			const { page } = app;
			const spots = await settled(page);
			assert.equal(await page.locator(`${ROOM} .dotted`).count(), 0, 'an outline with nothing active');
			assert.equal(await page.locator(`${ROOM} [data-now-field]`).count(), 0);
			const rows = rowsOf(spots, 'recent');
			const keys = Object.keys(rows).map(Number).sort((a, b) => a - b);
			assert.equal(keys.length, 2, `two rows, by y: ${JSON.stringify(rows)}`);
			// Most recent first; never used last, then by name.
			assert.deepEqual(rows[keys[0]], ['coffee', 'tea', 'mood']);
			assert.deepEqual(rows[keys[1]], ['read', 'walk', 'work']);
			// The rows start past the strip, a fifth of the room.
			const room = await page.locator(ROOM).boundingBox();
			assert.ok(Math.min(...spots.map((s) => s.left)) >= room.x + room.width / 5, 'a field over the strip');
		}, AT_REST);
	});

	test('active fields stand in the outlined section past the strip, ahead of every other, and their states are on the strip', async () => {
		await withApp(async (app) => {
			await seeded(app, [...SEED, 'switch mood good', 'start work']);
			const { page } = app;
			const spots = await settled(page);
			assert.deepEqual(rowsOf(spots, 'active'), { [spots.find((s) => s.field === 'mood').row]: ['mood'], [spots.find((s) => s.field === 'work').row]: ['work'] });
			const section = Math.max(...spots.filter((s) => s.section === 'active').map((s) => s.right));
			for (const s of spots.filter((s) => s.section === 'recent')) assert.ok(s.left > section, `${s.field} stands in the active section`);
			await page.locator(`${ROOM} .dotted path`).waitFor();
			const strip = await page.locator(`${ROOM} [data-now-field]`).evaluateAll((bs) => bs.map((b) => [b.dataset.nowField, getComputedStyle(b).backgroundColor]));
			assert.deepEqual(strip.map(([f]) => f).sort(), ['mood', 'work']);
			// Each in the colour its button is filled with.
			for (const [field, color] of strip) {
				const fill = await page.locator(`${ROOM} [data-field="${field}"]`).evaluate((b) => getComputedStyle(b).backgroundColor);
				assert.equal(color, fill, `${field}'s strip`);
			}
		}, AT_REST);
	});

	test('a field that ends takes the first slot of its row; one that starts goes into the section', async () => {
		await withApp(async (app) => {
			// work started before walk and read were used: by the last sort it is the oldest of its row.
			await seeded(app, [...SEED.slice(0, 6), 'start work', ...SEED.slice(6)]);
			const { page } = app;
			assert.equal((await settled(page)).find((s) => s.field === 'work').section, 'active');
			await logs(app, 'end work');
			await until(async () => (await shelf(page)).find((s) => s.field === 'work').section, (s) => s === 'recent');
			let spots = await settled(page);
			const row = spots.find((s) => s.field === 'work').row;
			assert.deepEqual(rowsOf(spots, 'recent')[row], ['work', 'read', 'walk']);
			assert.equal(await page.locator(`${ROOM} .dotted`).count(), 0);

			await logs(app, 'switch mood bad');
			await until(async () => (await shelf(page)).find((s) => s.field === 'mood').section, (s) => s === 'active');
			spots = await settled(page);
			assert.ok(spots.find((s) => s.field === 'mood').right < Math.min(...spots.filter((s) => s.section === 'recent').map((s) => s.left)));
		}, AT_REST);
	});

	// vividn, 16:41: «a "side: left | right" row in the widget menu».
	test('side: right is the mirror: the strip and the section at the right, each row last used first leftward', async () => {
		await withApp(async (app) => {
			await seeded(app, [...SEED, 'start work']);
			const { page } = app;
			await pickOption(page, 'bar', 'side', 'right');
			const room = await page.locator(ROOM).boundingBox();
			const strip = await page.locator(`${ROOM} [data-now-field="work"]`).boundingBox();
			assert.ok(strip.x >= room.x + room.width * 0.75, `the strip at ${strip.x} of ${room.x}–${room.x + room.width}`);
			await until(async () => {
				const spots = await shelf(page);
				const work = spots.find((s) => s.field === 'work');
				const row = spots.filter((s) => s.row === work.row && s.section === 'recent').sort((a, b) => b.right - a.right);
				return work.right <= strip.x && row.map((s) => s.field).join() === 'read,walk' && row[0].right <= work.left;
			}, Boolean);
			// The dotted outline mirrored with it: around work, at the strip's edge.
			const [line, work] = await Promise.all([page.locator(`${ROOM} .dotted path`).boundingBox(), page.locator(`${ROOM} [data-field="work"]`).boundingBox()]);
			assert.ok(line.x <= work.x && line.x + line.width >= work.x + work.width && line.x + line.width <= strip.x + 1, `the outline at ${line.x}–${line.x + line.width}, work at ${work.x}, the strip at ${strip.x}`);
		}, AT_REST);
	});

	test('a tap on the strip puts its field on the bar, not an entry', async () => {
		await withApp(async (app) => {
			await seeded(app, [...SEED, 'start work']);
			const { page } = app;
			await page.locator(`${ROOM} [data-now-field="work"]`).click();
			await until(
				() => page.locator('.prompt-bar .pill').evaluateAll((ps) => ps.map((p) => [[...p.classList].filter((c) => c !== 'pill').join(' '), p.innerText.replace(/[×\s]+$/, '').trim()])),
				(pills) => pills.length === 1 && pills[0][1] === 'work' && /field/.test(pills[0][0])
			);
		}, AT_REST);
	});
});

describe('collect at rest on a phone', () => {
	/** Twenty fields in one row, more than the room holds, and one active. */
	const MANY = Array.from({ length: 20 }, (_, i) => `field_${String(i).padStart(2, '0')}`);
	const CROWD = [at('mood', 'switch', 0.5, { allowed_states: ['good'] }), ...MANY.map((f) => at(f, 'occur', 0.5)), 'switch mood good'];

	/** How far the rows are panned: a recent field's shift. */
	const panned = (page) =>
		page.locator(`${ROOM} .spot.shelf[data-section="recent"]`).first().evaluate((s) => -new DOMMatrix(getComputedStyle(s).transform).m41);
	/** Wait until the pan stands still, and read it. */
	const rests = async (page) => {
		let last = null;
		await until(
			() => panned(page),
			(now) => {
				const same = now === last;
				last = now;
				return same;
			}
		);
		return last;
	};

	test('a fast swipe pans on past the finger and a slow one does not; the strip and the section stay', async () => {
		await withApp(async (app) => {
			await seeded(app, CROWD);
			const { page } = app;
			assert.equal(await page.locator(ROOM).getAttribute('data-pans'), 'x', 'the rows fit: nothing to pan');
			const mood = () => page.locator(`${ROOM} [data-field="mood"]`).boundingBox();
			const before = await mood();
			const room = await page.locator(ROOM).boundingBox();
			const [x, y] = [room.x + room.width * 0.85, room.y + room.height / 2];

			// Slow, and held still before it lifts: no speed, so the pan is the finger's.
			await swipe(page, x, y, -80, 0, 8, 60, 200);
			const slow = await rests(page);
			assert.ok(Math.abs(slow - 80) <= 2, `a slow swipe of 80px panned ${slow}px`);

			// Fast: 80px in under 50ms carries on well past the finger.
			await swipe(page, x, y, -80, 0, 6, 8);
			const fast = (await rests(page)) - slow;
			assert.ok(fast > 160, `a fast swipe of 80px panned ${fast}px`);

			assert.deepEqual(await mood(), before, 'the active section moved');

			// The mirror pans the other way: rows run leftward, so a finger moving right brings older fields in.
			await pickOption(page, 'bar', 'side', 'right');
			await until(() => panned(page), (p) => p === 0);
			const [mx, my] = [room.x + room.width * 0.15, y];
			await swipe(page, mx, my, 80, 0, 8, 60, 200);
			const mirrored = await rests(page);
			assert.ok(Math.abs(mirrored + 80) <= 2, `a slow swipe right of 80px on the mirror panned ${-mirrored}px`);
		}, AT_REST);
	});
});
