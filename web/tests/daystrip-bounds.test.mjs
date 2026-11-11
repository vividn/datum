// How far a day may be zoomed (da-4pak5, vividn, 2026-11-11: «allow zooming
// both in and out considerably more»): from 16px a day, where the date stamp
// has given up a line and stamps each week on its Monday,
// to 1000px, where a point's dot is still the size a 240px day draws it.
// The pan at the shortest day is panfree.test.mjs's.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, ON_COLLECT } from './support/app.mjs';
import { until } from './support/until.mjs';
import { MIN_DAY, MAX_DAY, PHONE, pinch, say, settledAt, valueOf } from './support/daystrip.mjs';

before(startApp);
after(stopApp);

/** The dayview tab, whose strip is the phone's height less the bar. */
const ON_DAYVIEW = { ...PHONE, viewport: { width: 390, height: 844 }, home: { ...ON_COLLECT, active: 3 } };
const WEEKDAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

/** Every stamp with text: its lines, and the box its text takes on screen. */
const drawnStamps = (page) =>
	page.locator('.strip .stamp').evaluateAll((stamps) =>
		stamps
			.map((stamp) => {
				const spans = [...stamp.querySelectorAll('span')];
				const boxes = spans.map((s) => s.getBoundingClientRect());
				return {
					lines: spans.map((s) => s.textContent),
					top: Math.min(...boxes.map((b) => b.top)),
					bottom: Math.max(...boxes.map((b) => b.bottom))
				};
			})
			.filter((stamp) => stamp.lines.length > 0)
	);

/** No two stamps' text meets, top to bottom. */
function apart(stamps, at) {
	const sorted = [...stamps].sort((a, b) => a.top - b.top);
	for (let i = 1; i < sorted.length; i++)
		assert.ok(
			sorted[i].top >= sorted[i - 1].bottom - 0.5,
			`at ${at}px a day «${sorted[i - 1].lines}» runs into «${sorted[i].lines}»`
		);
}

test('the stamp gives up a line as the day shrinks, then stamps each week on its Monday, never meeting another', async () => {
	await withApp(async (app) => {
		for (const [day, lines] of [[40, 3], [20, 2]]) {
			await say(app, { dayHeight: day });
			await settledAt(app, day);
			const stamps = await drawnStamps(app.page);
			assert.ok(stamps.length > 3, `at ${day}px a day ${stamps.length} days are stamped`);
			assert.deepEqual(new Set(stamps.map((s) => s.lines.length)), new Set([lines]), `at ${day}px a day`);
			assert.ok(stamps.every((s) => WEEKDAYS.includes(s.lines[0])), `at ${day}px a stamp does not lead with its weekday`);
			apart(stamps, day);
		}
		for (const day of [17, MIN_DAY]) {
			await say(app, { dayHeight: day });
			await settledAt(app, day);
			const stamps = await drawnStamps(app.page);
			// The stack holds a fortnight at least: two Mondays.
			assert.ok(stamps.length >= 2, `at ${day}px a day ${stamps.length} weeks are stamped`);
			// Date and year, but the newest week's may have days enough below it for one line.
			const year = await app.page.evaluate(() => new Date().getUTCFullYear());
			for (const [i, { lines }] of stamps.entries()) {
				assert.equal(lines.length, i < stamps.length - 1 ? 2 : lines.length, `at ${day}px a day the week's stamp is «${lines}»`);
				const monday = new Date(`${lines[0]}, ${lines[1] ?? year} 12:00 UTC`);
				assert.equal(monday.getUTCDay(), 1, `at ${day}px a day «${lines}» is no Monday`);
			}
			apart(stamps, day);
		}
		assert.deepEqual(app.errors, []);
	}, ON_DAYVIEW);
});

test('a pinch down the strip goes on past 40 and 240 px a day, to the bounds', async () => {
	await withApp(async (app) => {
		const stack = await app.page.locator('.stack').boundingBox();
		const box = { ...(await app.page.locator('.band[data-days-ago="0"]').boundingBox()), y: stack.y, height: stack.height };
		// Each a pinch of the set height, which the next `say` sets again.
		for (const [set, from, to, want] of [
			[60, 80, 40, 30],
			[240, 50, 100, 480],
			[20, 80, 40, MIN_DAY],
			[600, 60, 120, MAX_DAY]
		]) {
			await say(app, { dayHeight: set });
			await settledAt(app, set);
			await app.idle();
			await pinch(app.page, box, from, to, undefined, 'y');
			await until(() => valueOf(app), (v) => v === want);
			// The pinch's height is the moment's, over the set one, until the ⊖.
			await app.page.locator('.unzoom').click();
		}
		assert.deepEqual(app.errors, []);
	}, ON_DAYVIEW);
});

test("a point's dot is drawn at least as a 20px day draws it and at most as a 240px day does", async () => {
	await withApp(async (app) => {
		// Today's band, the one drawn at every height: a taller yesterday is off screen.
		assert.equal((await app.run(['occur', 'tick', '-t', '-60'])).code, 0);
		await app.reload();
		await app.page.locator('.band[data-days-ago="0"] [data-field="tick"]').first().waitFor({ state: 'attached' });
		const dot = async (day) => {
			await say(app, { dayHeight: day });
			await settledAt(app, day);
			const svg = app.page.locator('.band[data-days-ago="0"] svg.svg');
			await app.page.waitForFunction((day) => document.querySelector('.band[data-days-ago="0"] svg.svg')?.getAttribute('height') === String(day), day);
			return Number(await svg.locator('circle.point-fg').first().getAttribute('r'));
		};
		const [least, most] = [await dot(20), await dot(240)];
		assert.ok(least > 0 && most > least, `a dot is ${least}px at 20 and ${most}px at 240`);
		assert.equal(await dot(MIN_DAY), least, 'the shortest day drew its dot smaller than a 20px day');
		assert.equal(await dot(MAX_DAY), most, 'the tallest day drew its dot bigger than a 240px day');
		assert.ok(Math.abs((await dot(100)) - (least * 100) / 20) < 1e-6, 'a day between drew its dot at another scale');
		assert.deepEqual(app.errors, []);
	}, ON_DAYVIEW);
});
