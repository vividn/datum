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
