// Collect's pinch (da-l1z6h). vividn, 2026-11-04: «let's add pinch to zoom
// which changes font size and height of the rows to allow you to fit more or
// less rows in the collect widget».
//
// The rows are `atRest`'s (fields-rest-pure.test.mjs), counted from the
// buttons' height as drawn; this is the built app, read off what it draws.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, withApp, todayAt } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';
import { pinch } from './support/daystrip.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const ROOM = 'section.pane[data-title="collect"] .bar .collect .room';
/** A tall phone on collect, so its rows are many to begin with. */
const AT_REST = { ...PHONE, viewport: { width: 390, height: 844 }, home: ON_COLLECT, clock: todayAt(15) };

/** Thirty fields down the pictures' height, so every row the room holds has one. */
const SEED = Array.from({ length: 30 }, (_, i) => {
	const field = `f${String(i).padStart(2, '0')}`;
	return `setup ${field} --from-json '${JSON.stringify({ field, verb: 'occur', y: (i + 0.5) / 30, height: 0.01 })}'`;
});

/** How many rows stand, and a button's height and font size, once nothing glides. */
async function rows(page) {
	const read = () =>
		page.locator(`${ROOM} .spot.shelf`).evaluateAll((spots) => {
			const button = spots[0].querySelector('button');
			return JSON.stringify({
				rows: new Set(spots.map((s) => s.dataset.row)).size,
				height: Math.round(button.getBoundingClientRect().height),
				font: Math.round(parseFloat(getComputedStyle(button).fontSize) * Number(getComputedStyle(button).zoom || 1))
			});
		});
	let last = null;
	await until(read, (now) => {
		const same = now === last;
		last = now;
		return same;
	});
	return JSON.parse(last);
}

const held = (page) => page.evaluate(() => Object.values(window.datum.layout.session()).map((s) => s.zoom));

describe('collect zoom', () => {
	test('a pinch out fits fewer, larger rows and a pinch in more, smaller ones; the session keeps it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of SEED) assert.equal((await app.runLine(line)).code, 0, line);
			await app.reload();
			await page.locator(`${ROOM} .spot.shelf`).first().waitFor();
			await app.idle();
			const room = await page.locator(ROOM).boundingBox();

			const start = await rows(page);
			assert.ok(start.rows > 4, `only ${start.rows} rows to begin with`);

			await pinch(page, room, 60, 180);
			const large = await rows(page);
			assert.ok(large.height > start.height * 1.5, `a button ${start.height}px tall became ${large.height}px`);
			assert.ok(large.font > start.font * 1.5, `a font of ${start.font}px became ${large.font}px`);
			assert.ok(large.rows < start.rows, `${start.rows} rows became ${large.rows}`);

			await pinch(page, room, 200, 40);
			const small = await rows(page);
			assert.ok(small.height < start.height, `a button ${start.height}px tall became ${small.height}px`);
			assert.ok(small.font >= 8, `a font of ${small.font}px is past legible`);
			assert.ok(small.rows > start.rows, `${start.rows} rows became ${small.rows}`);
			assert.deepEqual(await held(page), [0.65], 'the smallest zoom, held in the session');

			await app.reload();
			await page.locator(`${ROOM} .spot.shelf`).first().waitFor();
			assert.deepEqual(await rows(page), small, 'a reload keeps the zoom');
		}, AT_REST);
	});

	test('ctrl+wheel, a trackpad pinch, zooms the rows too', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of SEED) assert.equal((await app.runLine(line)).code, 0, line);
			await app.reload();
			await page.locator(`${ROOM} .spot.shelf`).first().waitFor();
			await app.idle();
			const room = await page.locator(ROOM).boundingBox();
			const start = await rows(page);
			await page.mouse.move(room.x + room.width / 2, room.y + room.height / 2);
			await page.keyboard.down('Control');
			await page.mouse.wheel(0, -100);
			await page.keyboard.up('Control');
			const large = await rows(page);
			assert.ok(large.height > start.height, `a button ${start.height}px tall became ${large.height}px`);
			assert.ok(large.rows < start.rows, `${start.rows} rows became ${large.rows}`);
		}, AT_REST);
	});
});
