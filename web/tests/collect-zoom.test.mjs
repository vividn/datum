// Collect's pinch (da-l1z6h). vividn, 2026-11-04: «let's add pinch to zoom
// which changes font size and height of the rows to allow you to fit more or
// less rows in the collect widget».
//
// The rows are `atRest`'s (fields-rest-pure.test.mjs), counted from the
// buttons' height as drawn; this is the built app, read off what it draws.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, withApp, todayAt } from './support/app.mjs';
import { PHONE, choose } from './support/layout.mjs';
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

/** Thirty fields with long names, so the rows run past the room's width and a finger pans them. */
const WIDE = Array.from({ length: 30 }, (_, i) => {
	const field = `a_rather_long_field_${String(i).padStart(2, '0')}`;
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

/**
 * Two fingers on two field buttons, lifted one and then the other, as a hand
 * does (da-gh1ep): the first still, the second drawn `by` px away from it
 * over 700 ms, past a hold's 450. Then, with `tap`, a third finger tapped on
 * the first's spot at once, while the rows would still be gliding. Through
 * CDP, as `pinch` is, and in one session, so nothing waits between them.
 */
async function pinchOver(page, [a, b], by, tap = false) {
	const cdp = await page.context().newCDPSession(page);
	const at = (type, touchPoints) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
	const spread = (k) => [
		{ id: 1, x: a.x, y: a.y },
		{ id: 2, x: b.x + k * by, y: b.y + k * by }
	];
	try {
		await at('touchStart', [spread(0)[0]]);
		await at('touchStart', spread(0));
		for (let i = 1; i <= 7; i++) {
			await page.waitForTimeout(100);
			await at('touchMove', spread(i / 7));
		}
		await at('touchEnd', [spread(1)[0]]);
		await page.waitForTimeout(80);
		await at('touchEnd', []);
		if (tap) {
			await at('touchStart', [{ id: 3, x: a.x, y: a.y }]);
			await at('touchEnd', []);
		}
	} finally {
		await cdp.detach();
	}
}

/** The middle of a field's button as drawn. */
const middle = async (page, field) => {
	const box = await page.locator(`${ROOM} [data-field="${field}"]`).boundingBox();
	return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
};

/** A finger's touches through CDP, each `[type, points, ms to wait after]`. */
async function touches(page, steps) {
	const cdp = await page.context().newCDPSession(page);
	try {
		for (const [type, touchPoints, wait] of steps) {
			await cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
			if (wait) await page.waitForTimeout(wait);
		}
	} finally {
		await cdp.detach();
	}
}

/** The rows' zoom as drawn. */
const scaleOf = (page) => page.locator(ROOM).evaluate((el) => Number(getComputedStyle(el).getPropertyValue('--zoom')));
/** Where the first row's first field stands across the room. */
const firstX = (page) => page.locator(`${ROOM} .spot.shelf`).first().evaluate((el) => el.getBoundingClientRect().x);

/** One finger drawn `by` px across the room's middle, then the rows let settle: how far the first field moved. */
async function swipe(page, by) {
	const room = await page.locator(ROOM).boundingBox();
	const [x, y] = [room.x + room.width / 2 - by / 2, room.y + room.height / 2];
	const at = (k) => [{ id: 9, x: x + k * by, y }];
	const was = await firstX(page);
	await touches(page, [['touchStart', at(0), 30], ...[1, 2, 3, 4, 5].map((i) => ['touchMove', at(i / 5), 30]), ['touchEnd', [], 600]]);
	return (await firstX(page)) - was;
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

	test('two fingers on two fields, lifted one by one, log and open nothing, nor does a tap as the rows settle; the zoom outlives a reload, and a tap then logs', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of SEED) assert.equal((await app.runLine(line)).code, 0, line);
			await app.reload();
			await page.locator(`${ROOM} .spot.shelf`).first().waitFor();
			await app.idle();
			const [a, b] = await page.locator(`${ROOM} .spot.shelf button[data-field]`).evaluateAll((all) => all.slice(0, 2).map((el) => el.dataset.field));
			const start = await rows(page);
			const opened = async (what) => {
				await page.waitForTimeout(600);
				await app.idle();
				assert.deepEqual(await app.entries(), [], `${what} logged nothing`);
				assert.equal(await page.locator('.collect [data-back]').count(), 0, `${what} opened no field`);
				assert.equal(await page.locator(`${ROOM} [data-pilled]`).count(), 0, `${what} put no field pill on the bar`);
			};

			await pinchOver(page, [await middle(page, a), await middle(page, b)], 60);
			const large = await rows(page);
			assert.ok(large.height > start.height, `a button ${start.height}px tall became ${large.height}px`);
			await opened('the pinch');
			const [zoom] = await held(page);
			assert.ok(zoom > 1, `the pinch held a zoom of ${zoom}`);

			// Still, so the zoom stays and the tap lands on `a` (it logs there unguarded).
			await pinchOver(page, [await middle(page, a), await middle(page, b)], 0, true);
			await opened('a tap as the rows settle');
			assert.deepEqual(await held(page), [zoom], 'two still fingers kept the zoom');

			await app.reload();
			await page.locator(`${ROOM} .spot.shelf`).first().waitFor();
			assert.deepEqual(await held(page), [zoom], 'a reload keeps the zoom held');
			assert.deepEqual(await rows(page), large, 'and draws it');

			const tapped = await middle(page, a);
			await page.touchscreen.tap(tapped.x, tapped.y);
			await until(
				() => app.entries(),
				(rows) => rows.length === 1
			);
			assert.deepEqual((await app.entries()).map((row) => row.field), [a], 'a tap afterwards logs');
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

	// vividn, 2026-11-06: «it doesnt seem like i can scroll or zoom on the
	// collect widget anymore even after [resetting] the widget». A hold opens
	// its field under the finger, so the finger lifts on the field's view and
	// not on the rows; the rows kept it as down, and every touch after was a
	// pinch against it (da-yn6my).
	test('a hold that opens a field, then Back: a finger pans the rows and a pinch zooms them, and so they do after «Reset widget»', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of WIDE) assert.equal((await app.runLine(line)).code, 0, line);
			await app.reload();
			await page.locator(`${ROOM} .spot.shelf`).first().waitFor();
			await app.idle();
			assert.equal(await page.locator(ROOM).getAttribute('data-pans'), 'x', 'the rows run past the room, so a finger pans them');

			const holdOpen = async () => {
				const at = await middle(page, await page.locator(`${ROOM} .spot.shelf button[data-field]`).first().getAttribute('data-field'));
				await touches(page, [['touchStart', [{ id: 8, ...at }], 800], ['touchEnd', [], 300]]);
				assert.equal(await page.locator('.collect [data-back]').count(), 1, 'the hold opened its field');
				await page.keyboard.press('Escape');
				await page.locator(`${ROOM} .spot.shelf`).first().waitFor();
				await page.waitForTimeout(300);
			};
			const works = async (when) => {
				const scale = await scaleOf(page);
				const moved = [await swipe(page, -150), await swipe(page, 150)];
				assert.ok(moved[0] <= 0 && moved[1] >= 0 && moved[1] - moved[0] > 100, `${when}: a finger's swipes moved the rows ${moved.map(Math.round)} px`);
				assert.equal(await scaleOf(page), scale, `${when}: a finger's swipes left the zoom alone`);
				const room = await page.locator(ROOM).boundingBox();
				await pinch(page, room, 60, 180);
				await page.waitForTimeout(300);
				assert.ok((await scaleOf(page)) > scale * 1.5, `${when}: a pinch out took the zoom from ${scale} to ${await scaleOf(page)}`);
				await pinch(page, room, 180, 60);
				await page.waitForTimeout(300);
			};

			await holdOpen();
			await works('after a hold');
			await holdOpen();
			await choose(page, await page.locator('section.pane[data-title="collect"]').getAttribute('data-pane'), 'Reset widget');
			await page.locator(`${ROOM} .spot.shelf`).first().waitFor();
			await works('after a hold and «Reset widget»');
		}, AT_REST);
	});
});
