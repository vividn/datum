// A day the clock jumped in, on the day strip (da-1v9gp).
//
// vividn, 2026-10-09: "always keep dayview in local time, so that if you see
// 10am somewhere it actually meant it was 10am when it was experienced
// regardless if your timezone, location, or dst has changed meanwhile."
//
// The picture is the CLI's, so what it draws is `dayview_cmd.rs`'s and the
// layout's to hold. What is the page's is the POINTER: a day the clock
// jumped in is a group per stretch, each with an axis of its own, and a
// press has to be read against the stretch it lands on — the lane of the
// hour that happened twice, the nearest end of the hour that never did.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';
import { devices } from 'playwright';

before(startApp);
after(stopApp);

const TODAY = '.band[data-days-ago="0"]';
const BERLIN = { timezone: 'Europe/Berlin', viewport: { width: 1280, height: 900 } };

/** Today's band with `dots` logged on it, each a `-t` the CLI reads. */
async function withDay(clock, dots, fn, screen = BERLIN) {
	await withApp(
		async (app) => {
			for (const at of dots) assert.equal((await app.run(['occur', 'water', '-t', at])).code, 0);
			await app.redraw();
			await app.page.locator(`${TODAY} svg.svg .day-data circle`).first().waitFor({ state: 'attached' });
			await app.idle();
			await fn(app);
		},
		{ clock, ...screen }
	);
}

/** What today's band draws: its stretches, each with its lane and its dots, and the jump's marks. */
const drawn = (page) =>
	page.locator(TODAY).evaluate((band) => {
		const svg = band.querySelector('svg.svg');
		const box = (el) => {
			const r = el.getBoundingClientRect();
			return { left: r.left, right: r.right, top: r.top, bottom: r.bottom };
		};
		const ctm = svg.getScreenCTM();
		const px = (g, name) => new DOMPoint(Number(g.getAttribute(name)), 0).matrixTransform(ctm).x;
		return {
			box: box(svg),
			stretches: [...svg.querySelectorAll('.day-data')].map((g) => ({
				span: [g.getAttribute('data-from'), g.getAttribute('data-to')],
				lo: px(g, 'data-lo'),
				hi: px(g, 'data-hi'),
				lane: g.querySelector(':scope > .lane') ? box(g.querySelector(':scope > .lane')) : null,
				dots: [...g.querySelectorAll('circle[data-at]')].map((c) => ({ at: c.getAttribute('data-at'), ...box(c) }))
			})),
			labels: [...svg.querySelectorAll('.zone-label')].map((t) => t.textContent),
			labelBoxes: [...svg.querySelectorAll('.zone-label')].map(box),
			dead: [...svg.querySelectorAll('.dead-zone')].map(box)
		};
	});

/** The pixel a wall-clock hour is at across the band: its axis is 00:00 to 24:00, edge to edge. */
const xOf = (d, hour) => Math.round(d.box.left + (hour / 24) * (d.box.right - d.box.left));
/**
 * The instant the pixel `x` IS on a stretch's own axis, to the minute a
 * selection is kept to — so a press is expected to read what the pixel
 * pressed is worth, and the test's claim is WHICH stretch it was read on.
 */
const instantOn = (stretch, x) => {
	const [from, to] = stretch.span.map(Date.parse);
	const f = Math.min(1, Math.max(0, (x - stretch.lo) / (stretch.hi - stretch.lo)));
	return new Date(Math.round((from + f * (to - from)) / 60_000) * 60_000).toISOString();
};
/** A height on the band, as a fraction of it from the top. */
const yOf = (d, f) => d.box.top + f * (d.box.bottom - d.box.top);

/** Click the band and wait for the instant the page is then pointed at. */
async function pointedAt(page, x, y, want) {
	await page.mouse.click(x, y);
	return until(
		() => page.evaluate(() => window.datum.cursor.at()?.toISOString() ?? null),
		(at) => at === want
	);
}

/** Where the selection's arrow is drawn across the screen. */
const headX = (page) =>
	page.locator(`${TODAY} svg.svg .cursor-head`).evaluate((el) => {
		const r = el.getBoundingClientRect();
		return r.left + r.width / 2;
	});

test('an hour that happened twice is a lane each, and a press reads the lane it lands in', async () => {
	// Berlin, 2026-10-25: 03:00 summer time is 02:00 winter time.
	const dots = ['2026-10-25T02:20:00+02:00', '2026-10-25T02:15:00+01:00'];
	await withDay(Date.UTC(2026, 9, 25, 12), dots, async ({ page, run }) => {
		const d = await drawn(page);
		assert.deepEqual(
			d.stretches.map((s) => s.span),
			[
				['2026-10-24T22:00:00.000Z', '2026-10-25T01:00:00.000Z'],
				['2026-10-25T01:00:00.000Z', '2026-10-25T23:00:00.000Z']
			],
			'a 25 hour day: 00:00–03:00 at +2, then 02:00–24:00 at +1'
		);
		assert.deepEqual(d.labels, ['UTC+2', 'UTC+1']);
		assert.deepEqual(d.dead, []);

		// Each entry in the lane of the offset it was logged at: the first
		// 02:20 above, the second 02:15 below it, neither over the other.
		const [first, second] = d.stretches;
		assert.deepEqual(first.dots.map((dot) => dot.at), ['2026-10-25T00:20:00.000Z']);
		assert.deepEqual(second.dots.map((dot) => dot.at), ['2026-10-25T01:15:00.000Z']);
		const mid = yOf(d, 0.5);
		assert.ok(first.dots[0].bottom <= mid + 0.5, `the first 02:20 is in the top lane: ${JSON.stringify(first.dots[0])}`);
		assert.ok(second.dots[0].top >= mid - 0.5, `the second 02:15 is in the bottom lane: ${JSON.stringify(second.dots[0])}`);
		assert.ok(first.lane && second.lane, 'each stretch has its lane\'s path');

		// 02:30 is one place across the band and two instants an hour
		// apart: which one a press means is which lane it is in.
		const x = xOf(d, 2.5);
		const [early, late] = [instantOn(first, x), instantOn(second, x)];
		assert.equal(Date.parse(late) - Date.parse(early), 3_600_000, 'the same clock time, an hour apart');
		assert.ok(Math.abs(Date.parse(early) - Date.parse('2026-10-25T00:30:00Z')) <= 120_000, `about 02:30 the first time: ${early}`);
		await pointedAt(page, x, yOf(d, 0.25), early);
		const above = await headX(page);
		await pointedAt(page, x, yOf(d, 0.75), late);
		const below = await headX(page);
		assert.ok(Math.abs(above - x) <= 1.5 && Math.abs(below - x) <= 1.5, `both are drawn at 02:30: ${above}, ${below}, ${x}`);
		// Past the fork a press is the one hour there is, whatever its height.
		await pointedAt(page, xOf(d, 12), yOf(d, 0.25), instantOn(second, xOf(d, 12)));

		// The strip's band and a bare `dayview` are one drawing.
		const svg = (await run(['dayview', '--width', '1200'])).stdout;
		const spans = [...svg.matchAll(/<g class="day-data"[^>]* data-from="([^"]*)" data-to="([^"]*)"/g)];
		assert.deepEqual(
			spans.map((m) => [m[1], m[2]]),
			d.stretches.map((s) => s.span)
		);
		assert.equal(svg.match(/class="zone-label"[^>]*>([^<]*)</g).length, 2);
	});
});

test('an hour that never happened is hatched, and a press on it is the instant the clock jumped', async () => {
	// Berlin, 2026-03-29: 02:00 winter time is 03:00 summer time.
	const dots = ['2026-03-29T01:40:00+01:00', '2026-03-29T03:20:00+02:00'];
	await withDay(Date.UTC(2026, 2, 29, 12), dots, async ({ page }) => {
		const d = await drawn(page);
		assert.deepEqual(
			d.stretches.map((s) => s.span),
			[
				['2026-03-28T23:00:00.000Z', '2026-03-29T01:00:00.000Z'],
				['2026-03-29T01:00:00.000Z', '2026-03-29T22:00:00.000Z']
			],
			'a 23 hour day: 00:00–02:00 at +1, then 03:00–24:00 at +2'
		);
		assert.deepEqual(d.labels, ['UTC+1', 'UTC+2']);
		assert.equal(d.dead.length, 1);
		const [zone] = d.dead;
		assert.ok(
			Math.abs(zone.left - xOf(d, 2)) <= 1 && Math.abs(zone.right - xOf(d, 3)) <= 1,
			`02:00 to 03:00 is the dead zone: ${JSON.stringify(zone)}`
		);
		// Each dot at the hour its own clock read, either side of the zone.
		const at = (dot) => (dot.left + dot.right) / 2;
		assert.ok(Math.abs(at(d.stretches[0].dots[0]) - xOf(d, 1 + 40 / 60)) <= 1, '01:40 is at 01:40');
		assert.ok(Math.abs(at(d.stretches[1].dots[0]) - xOf(d, 3 + 20 / 60)) <= 1, '03:20 is at 03:20');

		// Either side of the zone a press reads its own stretch's clock; in
		// it, the end of whichever is nearer — the one instant 02:00 and
		// 03:00 both are.
		const y = yOf(d, 0.5);
		const [before, since] = d.stretches;
		const jump = '2026-03-29T01:00:00.000Z';
		assert.ok(Math.abs(Date.parse(instantOn(since, xOf(d, 3.5))) - Date.parse('2026-03-29T01:30:00Z')) <= 120_000);
		await pointedAt(page, xOf(d, 1.5), y, instantOn(before, xOf(d, 1.5)));
		await pointedAt(page, xOf(d, 2.75), y, jump);
		await pointedAt(page, xOf(d, 3.5), y, instantOn(since, xOf(d, 3.5)));
		await pointedAt(page, xOf(d, 2.25), y, jump);
	});
});

// da-0b9z4: the strip counter-scales every text about its own center, which
// moved the origin of a label's `rotate(-90 x y)` and threw it to about
// (2x, 2y): off the band, but for «a random U floating around somewhere».
test('each offset label is drawn inside the band, beside the end it names, on a desktop and on a phone', async () => {
	const days = [
		[Date.UTC(2026, 9, 25, 12), ['2026-10-25T02:30:00+02:00', '2026-10-25T02:30:00+01:00'], { 'UTC+2': 3, 'UTC+1': 2 }],
		[Date.UTC(2026, 2, 29, 12), ['2026-03-29T01:40:00+01:00', '2026-03-29T03:20:00+02:00'], { 'UTC+1': 2, 'UTC+2': 3 }]
	];
	const screens = { desktop: BERLIN, phone: { timezone: 'Europe/Berlin', device: devices['iPhone 12'] } };
	for (const [name, screen] of Object.entries(screens)) {
		for (const [clock, dots, ends] of days) {
			await withDay(
				clock,
				dots,
				async ({ page }) => {
					const d = await drawn(page);
					assert.deepEqual([...d.labels].sort(), Object.keys(ends).sort());
					d.labels.forEach((text, n) => {
						const r = d.labelBoxes[n];
						const what = `${name}, ${text}: ${JSON.stringify(r)} in ${JSON.stringify(d.box)}`;
						assert.ok(r.right - r.left > 0 && r.bottom - r.top > 0, `drawn: ${what}`);
						assert.ok(
							r.left >= d.box.left - 0.5 && r.right <= d.box.right + 0.5 && r.top >= d.box.top - 0.5 && r.bottom <= d.box.bottom + 0.5,
							`inside the band: ${what}`
						);
						// Its width along the axis is the text's height, so its
						// middle is within a label of the end it names.
						const off = Math.abs((r.left + r.right) / 2 - xOf(d, ends[text]));
						assert.ok(off <= 12, `beside ${ends[text]}:00, ${off.toFixed(1)}px off: ${what}`);
					});
				},
				screen
			);
		}
	}
});

// da-3ctgr: the tip told a mark's time on the BROWSER's clock, so a reader
// now in Berlin was told 07:20 for a dot drawn at 09:20, the time it was
// in Moscow, where it was logged.
test("a mark's tip tells its time on the clock its day was lived by, as the axis under it does", async () => {
	await withApp(
		async (app) => {
			const logged = await app.run(['occur', 'water', '-t', '2026-01-11T06:20:00Z', '-z', 'Europe/Moscow']);
			assert.equal(logged.code, 0, logged.stderr);
			await app.redraw();
			const d = await until(
				() => drawn(app.page),
				(d) => d.stretches[0]?.dots.length === 1
			);
			assert.deepEqual(d.stretches[0].span, ['2026-01-10T21:00:00.000Z', '2026-01-11T21:00:00.000Z'], "Moscow's day");
			const [dot] = d.stretches[0].dots;
			assert.ok(Math.abs((dot.left + dot.right) / 2 - xOf(d, 9 + 20 / 60)) <= 1, `drawn at 09:20: ${JSON.stringify(dot)}`);
			await app.page.locator(`${TODAY} svg.svg .day-data circle[data-at]`).hover();
			assert.equal(await app.page.locator(`${TODAY} .tip`).innerText(), '09:20 water');
		},
		{ clock: Date.UTC(2026, 0, 11, 12), ...BERLIN }
	);
});
