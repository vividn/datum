// The ALLDAY column (da-bz9pb). vividn, 2026-11-05: «for the dayview, lets put
// a special band between the date label and the start of the day strip that
// shows the field's symbol at its y position, this section can grow or shrink
// as necessary to show all the symbols so they don't overlap. If there is no
// ALLDAY data visible, then this strip disappears».
//
// The CLI places each mark (`g.allday` before a day's picture) and the strip
// gives the column room, so this reads the screen: where the column is, where
// the band starts after it, and each mark against its field's own lane.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, seedLines, todayAt } from './support/app.mjs';
import { heldFor, metrics, say, scrollToDay, setBox, sizeWhole, valueOf, waitForDays } from './support/daystrip.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const clocked = sharedApp({ clock: todayAt(12) });

/** Today's row on screen: its boxes, each ALLDAY mark's by field, and each field's mark on its band. */
const today = ({ page }) =>
	page.evaluate(() => {
		const row = document.querySelector('.row:has(> .band[data-days-ago="0"])');
		const box = (el) => {
			if (!el) return null;
			const { left, right, top, bottom } = el.getBoundingClientRect();
			return { left, right, top, bottom, y: (top + bottom) / 2 };
		};
		const marks = Object.fromEntries(
			[...row.querySelectorAll(':scope > div.allday g.field')].map((g) => [g.dataset.field, box(g.querySelector('.point-fg'))])
		);
		const lane = (field) =>
			box(row.querySelector(`.band svg.svg .day-data :is(circle.point-fg, rect.block)[data-field="${field}"]`) ??
				row.querySelector(`.band svg.svg .day-data g.field[data-field="${field}"] circle.point-fg`));
		return {
			stamp: box(row.querySelector('.stamp')),
			allday: box(row.querySelector(':scope > div.allday')),
			band: box(row.querySelector('.band')),
			grid: box(document.querySelector('.grid-clip')),
			marks,
			lanes: { coffee: lane('coffee'), mood: lane('mood') }
		};
	});

const near = (a, b, what) => assert.ok(Math.abs(a - b) <= 1.5, `${what}: ${a} vs ${b}`);

test('ALLDAY entries are their fields’ symbols in a column between the date and the band, gone with none', async () => {
	await clocked.solo(async (app) => {
		await app.submit('moment coffee -t 10:00');
		await app.idle();
		const none = await today(app);
		assert.equal(none.allday, null, 'a column with no ALLDAY entry');
		near(none.band.left, none.stamp.right, 'the band starts at the date');

		await app.submit('moment coffee -D');
		await app.submit('change mood happy -D');
		const two = await until(() => today(app), (t) => Object.keys(t.marks).length === 2 && t.lanes.mood);
		near(two.allday.left, two.stamp.right, 'the column starts at the date');
		near(two.band.left, two.allday.right, 'the band starts after the column');
		near(two.grid.left, two.band.left, 'the gridlines start with the band');
		// Each at its field's height: the field's dot and its block on the band.
		near(two.marks.coffee.y, two.lanes.coffee.y, 'coffee is not at its lane');
		near(two.marks.mood.y, two.lanes.mood.y, 'mood is not at its lane');
		for (const mark of Object.values(two.marks))
			assert.ok(mark.left >= two.allday.left - 0.5 && mark.right <= two.allday.right + 0.5, 'a mark outside the column');

		// Two at one height take a column each, and the band grows to hold them.
		await app.submit('setup a --kind moment --y 0.3 --height 0.1');
		await app.submit('setup b --kind moment --y 0.3 --height 0.1');
		await app.submit('moment a -D');
		await app.submit('moment b -D');
		const four = await until(() => today(app), (t) => Object.keys(t.marks).length === 4);
		const [a, b] = [four.marks.a, four.marks.b];
		near(a.y, b.y, 'a and b at one height');
		assert.ok(a.right <= b.left || b.right <= a.left, `a and b overlap: ${JSON.stringify([a, b])}`);
		const wide = (t) => t.allday.right - t.allday.left;
		assert.ok(wide(four) > wide(two), `the column did not grow: ${wide(two)} → ${wide(four)}`);
		near(four.band.left, four.allday.right, 'the band starts after the grown column');
	});
});

// The column moves the bands' size, and a size held from before may show a
// day as it was before its ALLDAY entry: a column read off that picture took
// itself back, and the two sizes swapped every frame, the page busy for good.
test('scrolled to a day whose ALLDAY entry came while it was off screen, the column settles', async () => {
	await clocked.solo(async (app) => {
		await seedLines(app, [...[1, 2, 3, 4].map((back) => `moment coffee -d -${back}`), 'moment coffee']);
		await sizeWhole(app, 5);
		await app.idle();
		await say(app, { dayHeight: (await valueOf(app)) + 7 });
		await app.idle();
	});
	await clocked.solo(async (app) => {
		const m = await metrics(app);
		await setBox(app, m.band + m.seam + m.axis + m.pitch - 2 + m.chrome);
		await waitForDays(app, heldFor(1));
		await app.idle();
		await app.submit('moment coffee -d -10');
		await app.idle();
		await scrollToDay(app, 10);
		const column = () =>
			app.page.evaluate(() => document.querySelector('.row:has(> .band[data-days-ago="10"]) > div.allday')?.getBoundingClientRect().width ?? 0);
		await until(column, (w) => w > 0);
		await app.idle();
		const seen = await app.spyOnRunner();
		const width = await column();
		await new Promise((resolve) => setTimeout(resolve, 2000));
		assert.equal(await column(), width, 'the column moved with nothing written');
		assert.deepEqual((await seen()).filter((argv) => argv[0] === 'dayview'), [], 'the strip kept asking');
	});
});

// A day's pill (da-x3l9n). vividn, 2026-11-05: «if you tap the date label or
// this allday band in the dayview it should add a -D pill (or -y, -yy, -yyy,
// -d <date>) pill for other days, and then filter for ALLDAY kind entries in
// the collect widget», and «or actually show moment data there as well, but
// show the ALLDAY data first».
/** Tall enough that collect's pane has room for its buttons beside the strip. */
const tall = sharedApp({ clock: todayAt(12), viewport: { width: 1280, height: 1100 } });

test('a tap on a day’s date label or ALLDAY column is its pill, and collect shows its all-day fields, then its moments', async () => {
	await tall.solo(async (app) => {
		const { page } = app;
		// steps is used first, so the moments stand ahead of it until a day is on the bar.
		for (const line of [
			'setup steps --kind allday',
			'allday steps 8000 -y',
			'allday steps 7000',
			'moment coffee -t 10:00',
			'moment tea -t 10:30',
			'on work -t 09:00',
			'change mood happy -t 09:00'
		])
			await app.submit(line);
		await app.idle();
		const SPOTS = 'section.pane[data-title="collect"] .bar .room .spot.shelf';
		const collect = () =>
			page.$$eval(SPOTS, (spots) => spots.map((s) => `${s.querySelector('[data-field]').dataset.field}${s.hasAttribute('data-promoted') ? '*' : ''}`));
		const pills = () => page.evaluate(() => window.datum.bar.pills().map((p) => p.text));
		const tap = (back, what = '.stamp') => page.locator(`.row:has(> .band[data-days-ago="${back}"]) > ${what}`).click();
		const every = await until(collect, (c) => c.length === 5);
		assert.notEqual(every[0], 'steps', `steps already stands first: ${every}`);
		const day = ['steps*', ...every.filter((f) => ['coffee', 'tea'].includes(f))];

		await tap(0);
		assert.deepEqual(await until(pills, (p) => p.length === 1), ['-D']);
		await until(collect, (c) => c.join() === day.join());

		await tap(1, 'div.allday');
		assert.deepEqual(await until(pills, (p) => p[0] !== '-D'), ['-y'], 'yesterday’s ALLDAY column');
		await tap(1, 'div.allday');
		await until(pills, (p) => p.length === 0);
		await until(collect, (c) => c.join() === every.join());

		await tap(1);
		assert.deepEqual(await until(pills, (p) => p.length === 1), ['-y'], 'yesterday’s date label');
		await scrollToDay(app, 7);
		await tap(7);
		const week = new Date(todayAt(12) - 7 * 86_400_000).toISOString().slice(0, 10);
		assert.deepEqual(await until(pills, (p) => p[0] !== '-y'), [`-d ${week}`], 'a week back');
		await until(collect, (c) => c.join() === day.join());

		// The next tap logs that day's full-day entry, and the pill's × gives every field back.
		await page.locator(`${SPOTS} [data-field="coffee"]`).click();
		await until(
			async () => JSON.parse((await app.runLine('tail coffee --json')).stdout).rows.map((r) => r.time),
			(t) => t.some((time) => time?.startsWith(week))
		);
		await until(pills, (p) => p.length === 0);
		await tap(0);
		await until(collect, (c) => c.join() === day.join());
		await page.locator('.prompt-bar .pill[data-pill="date"] button.remove').click();
		await until(collect, (c) => c.join() === every.join());
	});
});
