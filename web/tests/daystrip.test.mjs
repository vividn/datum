// The day strip: today pinned at the bottom, the past pulled down over it.
//
// What a reader is doing with this is a comparison — is today like
// yesterday, like last Tuesday — so the assertions are about the two
// things a comparison needs and nothing else: that the bands above today
// are the DAYS they claim to be, and that they are drawn against the same
// x-axis today is drawn against. A test that counted `<img>` elements
// before and after a pull would pass on a stack of five copies of today
// (da-cgyh), so every band here is read by its date stamp, and the
// alignment is read off the gridlines rather than eyeballed.
//
// The jagged line is the third thing: it is a claim about what is
// currently ON SCREEN — the band above today is not the day before it —
// so it is asserted against the stack's scroll position, driven both ways.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/** datum's 3-line day label, as $lib/runners/views.ts builds one. */
const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
const MONTHS = [
	'Jan',
	'Feb',
	'Mar',
	'Apr',
	'May',
	'Jun',
	'Jul',
	'Aug',
	'Sep',
	'Oct',
	'Nov',
	'Dec'
];

/**
 * The label a band `daysAgo` before `nowMs` carries.
 *
 * Read in UTC because the browser context is pinned to UTC
 * (support/app.mjs), and off the PAGE's clock rather than node's: the two
 * straddle a midnight for one instant a day, and an expectation built
 * from the wrong one is a test that fails at 00:00 and nowhere else.
 */
function labelFor(nowMs, daysAgo) {
	const day = new Date(nowMs);
	day.setUTCHours(0, 0, 0, 0);
	day.setUTCDate(day.getUTCDate() - daysAgo);
	return [
		DAYS[day.getUTCDay()],
		`${MONTHS[day.getUTCMonth()]} ${String(day.getUTCDate()).padStart(2, '0')}`,
		String(day.getUTCFullYear())
	];
}

/** The colour datum gives a name: the first three bytes of its md5. */
const colorOf = (name) => '#' + createHash('md5').update(name).digest('hex').slice(0, 6);

/** One band's SVG, by how many days back it is. 0 is today's. */
async function band({ page }, daysAgo) {
	const src = await page
		.locator(`.band[data-days-ago="${daysAgo}"] img.svg`)
		.getAttribute('src');
	return decodeURIComponent(src.replace(/^data:image\/svg\+xml,/, ''));
}

/** The three lines of a band's date stamp. */
const dateLines = (svg) =>
	[...svg.matchAll(/<text class="date-label"[^>]*>([^<]*)</g)].map((m) => m[1]);

/** Where every hourly gridline sits along the band's x-axis. */
const gridlines = (svg) =>
	[...svg.matchAll(/<line class="gridline"[^>]*\sx1="([\d.]+)"/g)].map((m) => Number(m[1]));

/** Pull `times` more days down, and wait for them to be drawn. */
async function pull({ page }, times) {
	const have = await page.locator('.band img.svg').count();
	for (let i = 0; i < times; i++) await page.locator('button.pull').click();
	await page.waitForFunction(
		(n) => document.querySelectorAll('.band img.svg').length === n,
		have + times
	);
}

/** The stack's scroll box, as the strip reads it. */
const stackScroll = ({ page }) =>
	page.locator('.stack').evaluate((el) => ({
		top: el.scrollTop,
		client: el.clientHeight,
		total: el.scrollHeight
	}));

/** Whether the jagged line is on screen. */
const jagged = ({ page }) => page.locator('button.jagged').count();

describe('the day strip', () => {
	test('starts as today alone, and a pull brings the days before it', async () => {
		await withApp(async (app) => {
			// At rest the strip is what the frame has always shown: one
			// band, today's, with no stack above it at all.
			assert.equal(await app.page.locator('.band').count(), 1);
			assert.equal(await app.page.locator('.stack').count(), 0);

			const nowMs = await app.page.evaluate(() => Date.now());
			await pull(app, 2);

			// Two bands above today, and each is the day it claims: the two
			// days immediately before today, oldest at the top.
			for (const daysAgo of [2, 1, 0]) {
				assert.deepEqual(
					dateLines(await band(app, daysAgo)),
					labelFor(nowMs, daysAgo),
					`the band ${daysAgo} day(s) back is stamped with the wrong date`
				);
			}

			// And today's band is still the live one: an entry logged now
			// lands in it on the epoch tick, and in none of the others.
			await app.submit('switch mood happy -t -5');
			const happy = colorOf('happy');
			await app.page.waitForFunction(
				(c) =>
					document
						.querySelector('.band[data-days-ago="0"] img.svg')
						.src.includes(encodeURIComponent(c)),
				happy
			);
			for (const daysAgo of [1, 2]) {
				assert.ok(
					!(await band(app, daysAgo)).includes(happy),
					`the band ${daysAgo} day(s) back drew an entry logged today`
				);
			}
		});
	});

	// The pull the feature is named for. The button in the corner is the
	// keyboard's route and the one a reader can see; these two are the
	// gestures — a wheel up on a pointer, a drag down on a finger. Each
	// gets its own app because one flick is one day: the strip rate-limits
	// gesture pulls, and two gestures in the same millisecond would be
	// testing that limit rather than the gestures.
	test('a wheel up at the top of the stack reaches for one more day', async () => {
		await withApp(async (app) => {
			const box = await app.page.locator('.strip').boundingBox();
			await app.page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
			await app.page.mouse.wheel(0, -120);
			await app.page.waitForFunction(
				() => document.querySelectorAll('.band img.svg').length === 2
			);
		});
	});

	test('a finger dragged down the strip reaches for one more day', async () => {
		await withApp(async (app) => {
			await app.page.locator('.strip').evaluate((el) => {
				const at = (clientY) => {
					const touch = new Touch({ identifier: 1, target: el, clientX: 0, clientY });
					return { touches: [touch], changedTouches: [touch], bubbles: true };
				};
				el.dispatchEvent(new TouchEvent('touchstart', at(0)));
				el.dispatchEvent(new TouchEvent('touchmove', at(80)));
			});
			await app.page.waitForFunction(
				() => document.querySelectorAll('.band img.svg').length === 2
			);
		});
	});

	test('the jagged line says the band above today is not yesterday', async () => {
		await withApp(async (app) => {
			await pull(app, 3);

			// Three bands is more than the stack shows at once, so there is
			// something to scroll. Nothing to tear yet: yesterday is at the
			// bottom, against today.
			const box = await stackScroll(app);
			assert.ok(box.total > box.client, `the stack does not scroll: ${JSON.stringify(box)}`);
			assert.equal(await jagged(app), 0, 'the jagged line is up with yesterday in place');

			// Scroll up into the older days: now what sits above today is
			// not the day before it, and the strip says so.
			await app.page.locator('.stack').evaluate((el) => (el.scrollTop = 0));
			await app.page.locator('button.jagged').waitFor();

			// It is a button, it takes focus, and the keyboard works it.
			await app.page.locator('button.jagged').focus();
			assert.ok(
				await app.page.evaluate(() => document.activeElement.classList.contains('jagged')),
				'the jagged line does not take focus'
			);
			await app.page.locator('button.jagged').press('Enter');
			await app.page.locator('button.jagged').waitFor({ state: 'detached' });

			const back = await stackScroll(app);
			assert.ok(
				back.top + back.client >= back.total - 1,
				`enter did not bring yesterday back: ${JSON.stringify(back)}`
			);
			// K is unchanged — the line puts the stack back, it does not
			// throw away the days that were pulled.
			assert.equal(await app.page.locator('.band').count(), 4);

			// And the same by pointer, which is the other half of "reachable
			// by keyboard and by touch".
			await app.page.locator('.stack').evaluate((el) => (el.scrollTop = 0));
			await app.page.locator('button.jagged').click();
			await app.page.locator('button.jagged').waitFor({ state: 'detached' });
		});
	});

	test('a midnight moves the whole strip on by a day', async () => {
		// 10:00 UTC, so the fast-forward below lands after midnight and
		// inside the next day rather than on its boundary.
		const noon = Date.UTC(2026, 8, 9, 10, 0, 0);
		await withApp(
			async (app) => {
				// Three, so the stack is deeper than it shows and the jagged
				// line has something to say at the end.
				await pull(app, 3);
				assert.deepEqual(dateLines(await band(app, 0)), labelFor(noon, 0));
				assert.deepEqual(dateLines(await band(app, 1)), labelFor(noon, 1));

				// Over the boundary. The strip redraws on the frame's epoch,
				// which is the same beat today's band has always refreshed
				// on — so a command is what makes it look at the clock again.
				const after = noon + 16 * 60 * 60 * 1000;
				await app.page.clock.setFixedTime(new Date(after));
				await app.submit('occur coffee');

				await app.page.waitForFunction(
					(want) =>
						document
							.querySelector('.band[data-days-ago="0"] img.svg')
							.src.includes(encodeURIComponent(`>${want}<`)),
					labelFor(after, 0)[1]
				);

				// Today is the new day; yesterday is the day that was today;
				// the stack is stamped with the days before that.
				for (const daysAgo of [0, 1, 2, 3]) {
					assert.deepEqual(
						dateLines(await band(app, daysAgo)),
						labelFor(after, daysAgo),
						`the band ${daysAgo} day(s) back did not follow the new today`
					);
				}
				// And the rule still reads off the scroll position, not off
				// the date it was first drawn for.
				assert.equal(await jagged(app), 0);
				await app.page.locator('.stack').evaluate((el) => (el.scrollTop = 0));
				await app.page.locator('button.jagged').waitFor();
			},
			{ prepare: (context) => context.clock.setFixedTime(new Date(noon)) }
		);
	});

	test('every band is drawn against the same x-axis as today', async () => {
		await withApp(async (app) => {
			await pull(app, 3);

			// The hour gridlines are the axis as the data area sees it, and
			// they are drawn on every band — the tick LABELS are today's
			// alone. Same x, band for band, or the stack is a comparison of
			// two different rulers.
			const today = gridlines(await band(app, 0));
			assert.ok(today.length >= 4, `today's band drew ${today.length} gridlines`);
			for (const daysAgo of [1, 2, 3]) {
				assert.deepEqual(
					gridlines(await band(app, daysAgo)),
					today,
					`the band ${daysAgo} day(s) back is on a different x-axis`
				);
			}

			// The axis strip itself is drawn once, under today.
			const ticks = (svg) => [...svg.matchAll(/class="tick-label"/g)].length;
			assert.ok(ticks(await band(app, 0)) >= 4, "today's band lost its hour axis");
			assert.equal(ticks(await band(app, 1)), 0, 'a stacked band repeated the hour axis');
		});
	});

	// The strip is full-width, and the fix for horizontal scrollbars over
	// it landed the day before this did (1ceada8) — so every depth the
	// stack can be in is swept, at every width the app is read at.
	for (const width of [360, 390, 768, 1280]) {
		test(`never scrolls the page sideways at ${width}px, at any depth`, async () => {
			await withApp(
				async (app) => {
					let depth = 0;
					for (const want of [0, 1, 5]) {
						if (want > depth) await pull(app, want - depth);
						depth = want;
						const seen = await app.page.evaluate(() => {
							const el = document.scrollingElement;
							return { scroll: el.scrollWidth, client: el.clientWidth };
						});
						assert.ok(
							seen.scroll <= seen.client,
							`${width}px at ${depth} days deep scrolls sideways: ${seen.scroll} in ${seen.client}`
						);
					}
				},
				{ viewport: { width, height: 800 } }
			);
		});
	}
});
