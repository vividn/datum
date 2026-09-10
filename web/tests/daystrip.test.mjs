// The day strip: today pinned at the bottom, the past pulled down over it.
//
// What a reader is doing with this is a comparison — is today like
// yesterday, like last Tuesday — so the assertions are about the two
// things a comparison needs and nothing else: that the bands above today
// are the DAYS they claim to be, and that they are drawn against the same
// x-axis today is drawn against. A test that counted `<img>` elements
// before and after would pass on a stack of five copies of today
// (da-cgyh), so every band here is read by its date stamp, and the
// alignment is read off the gridlines rather than eyeballed.
//
// The jagged line is the third thing: it is a claim about what is
// currently ON SCREEN — the band above today is not the day before it —
// so it is asserted against the stack's scroll position, driven both ways.
//
// Since da-07ij the two things a reader sets are HEIGHTS, not counts: the
// grab bar along the bottom edge says how much room the strip has, and
// how much room it has is how much past it holds; the notch in the left
// margin says how tall one day is drawn. So the assertions about them are
// about pixels dragged and pixels rendered — the band's own SVG `height`,
// not a class name that claims to mean it — and about what comes back
// after a reload, which is the whole point of remembering them.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { startApp, stopApp, withApp, workerClock } from './support/app.mjs';

before(startApp);
after(stopApp);

/** Where the strip keeps what the reader asked for (DayStrip.svelte). */
const STORE_KEY = 'datum-daystrip';

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

/** How tall datum actually drew the band, off the document it wrote. */
const svgHeight = (svg) => Number(svg.match(/<svg[^>]*\sheight="([\d.]+)"/)[1]);

/**
 * The strip's own arithmetic, read off the element it styles.
 *
 * One band's box and the margin below it are what a day COSTS in strip
 * height — so a drag of `box + gap` is a drag of exactly one day, and the
 * test never has to restate a constant that lives in $lib/panes.ts.
 */
const metrics = ({ page }) =>
	page.locator('.strip').evaluate((el) => {
		const style = getComputedStyle(el);
		const px = (name) => parseFloat(style.getPropertyValue(name));
		return { box: px('--band'), gap: px('--gap'), unit: px('--band') + px('--gap') };
	});

/** What a handle currently says it is set to. */
const valueOf = async ({ page }, selector) =>
	Number(await page.locator(selector).getAttribute('aria-valuenow'));

/**
 * Wait until each named band has been re-drawn at the height it is due.
 *
 * `want` is keyed by `data-days-ago`, because the heights are NOT all the
 * same number: today's band carries the hour axis and is that much taller
 * than the stacked ones. A single expected height for all of them is a
 * wait that never ends.
 */
const waitForHeights = ({ page }, want) =>
	page.waitForFunction(
		(w) =>
			Object.entries(w).every(([daysAgo, height]) => {
				const img = document.querySelector(`.band[data-days-ago="${daysAgo}"] img.svg`);
				return img !== null && decodeURIComponent(img.src).includes(`height="${height}"`);
			}),
		want
	);

/** Wait until exactly `n` bands have a picture in them. */
const waitForBands = ({ page }, n) =>
	page.waitForFunction((want) => document.querySelectorAll('.band img.svg').length === want, n);

/** Drag a handle by `dy` pixels, as a pointer does: down, move, up. */
async function dragBy({ page }, selector, dy) {
	const box = await page.locator(selector).boundingBox();
	const x = box.x + box.width / 2;
	const y = box.y + box.height / 2;
	await page.mouse.move(x, y);
	await page.mouse.down();
	await page.mouse.move(x, y + dy, { steps: 8 });
	await page.mouse.up();
}

/**
 * Reach back `days` further, by the keyboard.
 *
 * The keyboard rather than a drag wherever the test is about something
 * else: one press is exactly one day's worth of room, so a test about the
 * jagged line is not also a test about how well a mouse lands on a 12px
 * bar.
 */
async function deepen(app, days) {
	const have = await app.page.locator('.band img.svg').count();
	await app.page.locator('button.grab').focus();
	for (let i = 0; i < days; i++) await app.page.keyboard.press('ArrowDown');
	await waitForBands(app, have + days);
}

/** What the strip has written down, or null if it has written nothing. */
const stored = ({ page }, key = STORE_KEY) =>
	page.evaluate((k) => {
		const raw = localStorage.getItem(k);
		return raw === null ? null : JSON.parse(raw);
	}, key);

/** Wait out the write debounce: what is on screen is what is written. */
const waitForStored = ({ page }, want, key = STORE_KEY) =>
	page.waitForFunction(
		([k, w]) => {
			const raw = localStorage.getItem(k);
			return raw !== null && JSON.parse(raw).dayHeight === w;
		},
		[key, want]
	);

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
	test('the bottom handle drags the days before today into view', async () => {
		await withApp(async (app) => {
			// At rest the strip is what the frame has always shown: one
			// band, today's, with no stack above it at all.
			assert.equal(await app.page.locator('.band').count(), 1);
			assert.equal(await app.page.locator('.stack').count(), 0);

			const { unit } = await metrics(app);
			const nowMs = await app.page.evaluate(() => Date.now());

			// Two bands' worth of room, so two more days.
			await dragBy(app, 'button.grab', 2 * unit);
			await waitForBands(app, 3);
			assert.equal(await valueOf(app, 'button.grab'), 2);

			// And each is the day it claims: the two days immediately before
			// today, oldest at the top.
			for (const daysAgo of [2, 1, 0]) {
				assert.deepEqual(
					dateLines(await band(app, daysAgo)),
					labelFor(nowMs, daysAgo),
					`the band ${daysAgo} day(s) back is stamped with the wrong date`
				);
			}

			// Today's band is still the live one: an entry logged now lands
			// in it on the epoch tick, and in none of the others.
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

			// Dragged back up to today's own height, the past goes away
			// again — the handle is a height, not a ratchet.
			await dragBy(app, 'button.grab', -2 * unit);
			await waitForBands(app, 1);
			assert.equal(await valueOf(app, 'button.grab'), 0);
			assert.equal(await app.page.locator('.stack').count(), 0);
		});
	});

	test('the margin handle re-draws every day at the new height', async () => {
		await withApp(async (app) => {
			await deepen(app, 2);
			const before = await Promise.all([2, 1, 0].map((n) => band(app, n).then(svgHeight)));
			const was = await valueOf(app, 'button.day-grip');
			const grow = 30;

			await dragBy(app, 'button.day-grip', grow);
			assert.equal(await valueOf(app, 'button.day-grip'), was + grow);
			// Re-drawn, not stretched: the picture datum wrote is the new
			// size. Waited for, because the render happens on the drop.
			await waitForHeights(app, {
				2: before[0] + grow,
				1: before[1] + grow,
				0: before[2] + grow
			});

			const after = await Promise.all([2, 1, 0].map((n) => band(app, n)));
			// Every band grew, by the same amount, including today's — the
			// two stacked ones drawn to the same height as each other.
			after.forEach((svg, i) => {
				assert.equal(
					svgHeight(svg),
					before[i] + grow,
					`band ${[2, 1, 0][i]} did not follow the day height`
				);
			});
			assert.equal(svgHeight(after[0]), svgHeight(after[1]), 'the stacked bands disagree');

			// And the thing the stack is for still holds at the new height:
			// the same hour is the same column, band for band.
			const today = gridlines(after[2]);
			assert.ok(today.length >= 4, `today's band drew ${today.length} gridlines`);
			for (const i of [0, 1]) {
				assert.deepEqual(
					gridlines(after[i]),
					today,
					`band ${[2, 1][i]} is on a different x-axis after a resize`
				);
			}
		});
	});

	test('both handles come back after a reload, and the bands are drawn afresh', async () => {
		await withApp(async (app) => {
			await deepen(app, 2);
			const was = await valueOf(app, 'button.day-grip');
			await app.page.locator('button.day-grip').focus();
			await app.page.keyboard.press('ArrowDown');
			const taller = await valueOf(app, 'button.day-grip');
			assert.ok(taller > was, 'the arrow key did not make the day taller');

			// An entry for YESTERDAY, logged before the reload: after it, the
			// band a day back has to be a fresh render of the database and
			// not whatever picture was last drawn.
			await app.submit('switch mood happy -y');
			await waitForStored(app, taller);

			await app.reload();
			await waitForBands(app, 3);
			assert.equal(await valueOf(app, 'button.grab'), 2, 'the strip forgot how deep it was');
			assert.equal(
				await valueOf(app, 'button.day-grip'),
				taller,
				'the strip forgot how tall a day was'
			);
			assert.ok(
				(await band(app, 1)).includes(colorOf('happy')),
				"yesterday's band came back stale — it does not show what was logged into it"
			);
		});
	});

	test('the reset arrow puts the strip back and forgets it', async () => {
		await withApp(async (app) => {
			const fallback = await valueOf(app, 'button.day-grip');
			await deepen(app, 2);
			await dragBy(app, 'button.day-grip', 40);
			await waitForStored(app, fallback + 40);

			await app.page.locator('button.reset').click();
			await waitForBands(app, 1);
			assert.equal(await app.page.locator('.stack').count(), 0);
			assert.equal(await valueOf(app, 'button.day-grip'), fallback);
			// Cleared, not overwritten with the defaults — and the debounced
			// write from the drag must not land on top of the reset.
			await app.page.waitForFunction(
				(k) => localStorage.getItem(k) === null,
				STORE_KEY
			);

			// A second visit is the default one, not the one that was reset.
			await app.reload();
			assert.equal(await app.page.locator('.band').count(), 1);
			assert.equal(await valueOf(app, 'button.day-grip'), fallback);
			assert.equal(await stored(app), null);
		});
	});

	test('the keyboard works both handles and the reset', async () => {
		await withApp(async (app) => {
			const fallback = await valueOf(app, 'button.day-grip');

			// Down is more, on both handles, because down is the way a
			// pointer drags for more.
			await app.page.locator('button.grab').focus();
			assert.ok(
				await app.page.evaluate(() => document.activeElement.classList.contains('grab')),
				'the bottom handle does not take focus'
			);
			for (const want of [1, 2, 3]) {
				await app.page.keyboard.press('ArrowDown');
				assert.equal(await valueOf(app, 'button.grab'), want);
			}
			await waitForBands(app, 4);
			await app.page.keyboard.press('ArrowUp');
			await waitForBands(app, 3);

			await app.page.locator('button.day-grip').focus();
			await app.page.keyboard.press('ArrowUp');
			const shorter = await valueOf(app, 'button.day-grip');
			assert.ok(shorter < fallback, `up did not shorten the day: ${shorter}`);
			await app.page.keyboard.press('ArrowDown');
			await app.page.keyboard.press('ArrowDown');
			assert.equal(await valueOf(app, 'button.day-grip'), 2 * fallback - shorter);

			await app.page.locator('button.reset').press('Enter');
			await waitForBands(app, 1);
			assert.equal(await valueOf(app, 'button.day-grip'), fallback);
		});
	});

	test('a browser with no localStorage still draws the strip and works its handles', async () => {
		// What the defaults are is the strip's business, not this file's, so
		// they are read off an ordinary app rather than restated here.
		let fallback;
		await withApp(async (app) => {
			fallback = await valueOf(app, 'button.day-grip');
		});

		await withApp(
			async (app) => {
				assert.equal(await app.page.locator('.band').count(), 1);
				assert.equal(await app.page.locator('.stack').count(), 0);
				assert.equal(await valueOf(app, 'button.day-grip'), fallback);

				// The handles still work — the preference is lost at the end
				// of the visit, not during it.
				await deepen(app, 1);
				assert.equal(await valueOf(app, 'button.grab'), 1);
				await app.page.locator('button.day-grip').focus();
				await app.page.keyboard.press('ArrowDown');
				assert.ok(await valueOf(app, 'button.day-grip') > fallback);

				assert.deepEqual(app.errors, [], 'the page threw with storage blocked');
			},
			{
				// Throwing on the ACCESS, which is what a browser set to block
				// site data actually does — a `getItem` returning null is the
				// easy half and never caught anything.
				prepare: (context) =>
					context.addInitScript(() => {
						Object.defineProperty(window, 'localStorage', {
							configurable: true,
							get() {
								throw new DOMException('storage is blocked', 'SecurityError');
							}
						});
					})
			}
		);
	});

	test('the jagged line says the band above today is not yesterday', async () => {
		await withApp(async (app) => {
			// Deeper than the strip will show at once, so there is something
			// to scroll. Nothing to tear yet: yesterday is at the bottom,
			// against today.
			await deepen(app, 5);

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
			// The depth is unchanged — the line puts the stack back, it does
			// not throw away the days that were revealed.
			assert.equal(await app.page.locator('.band').count(), 6);

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
		// The CLI draws the bands, in the database worker: its clock too.
		const clock = workerClock(noon);
		await withApp(
			async (app) => {
				// Deep enough that the stack scrolls, so the jagged line has
				// something to say at the end.
				await deepen(app, 5);
				assert.deepEqual(dateLines(await band(app, 0)), labelFor(noon, 0));
				assert.deepEqual(dateLines(await band(app, 1)), labelFor(noon, 1));

				// Over the boundary. The strip redraws on the frame's epoch,
				// which is the same beat today's band has always refreshed
				// on — so a command is what makes it look at the clock again.
				const after = noon + 16 * 60 * 60 * 1000;
				await app.page.clock.setFixedTime(new Date(after));
				await clock.set(app.page, after);
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
				for (const daysAgo of [0, 1, 2, 3, 4, 5]) {
					assert.deepEqual(
						dateLines(await band(app, daysAgo)),
						labelFor(after, daysAgo),
						`the band ${daysAgo} day(s) back did not follow the new today`
					);
				}
				// The handles kept what they were set to across the rollover.
				assert.equal(await valueOf(app, 'button.grab'), 5);
				// And the rule still reads off the scroll position, not off
				// the date it was first drawn for.
				assert.equal(await jagged(app), 0);
				await app.page.locator('.stack').evaluate((el) => (el.scrollTop = 0));
				await app.page.locator('button.jagged').waitFor();
			},
			{
				prepare: async (context) => {
					await context.clock.setFixedTime(new Date(noon));
					await clock.prepare(context);
				}
			}
		);
	});

	test('every band is drawn against the same x-axis as today', async () => {
		await withApp(async (app) => {
			await deepen(app, 3);

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
						if (want > depth) await deepen(app, want - depth);
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

	// The handle can ask for more room than there is, and the strip's
	// height drives its grid track — so the thing a phone user logs with
	// is what a strip dragged to its limit would push off the bottom
	// (da-hkpd). The strip caps itself against the viewport instead.
	test('a strip asked past its limit leaves the widget bar on screen', async () => {
		const screen = { width: 360, height: 640 };
		await withApp(
			async (app) => {
				await app.submit('occur coffee');
				await app.page.locator('.bar button.widget').first().waitFor();

				// Six days is more strip than half this screen, which is
				// where the strip caps itself — so it is the CAP being
				// tested and not the reader's restraint.
				await deepen(app, 6);

				const bar = await app.page.locator('.bar button.widget').first().boundingBox();
				assert.ok(
					bar.y + bar.height <= screen.height,
					`the widget bar is off the bottom at full strip height: ${JSON.stringify(bar)}`
				);
			},
			{ viewport: screen }
		);
	});
});
