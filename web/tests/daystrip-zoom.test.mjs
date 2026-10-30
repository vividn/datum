// The day strip's zoom moves marks and stretches none of them (da-dqbso).
//
// vividn, 2026-10-22: «occur dots (and lettering, icons, etc) shouldn't
// stretch out and get weird when zooming in. It should be unnoticeable that
// nothing has redrawn yet. Maybe we don't even need to redraw at all.»
//
// So a gesture maps the picture it has; the map stretches time, and the strip
// scales every mark that is not a duration back to its own size
// (DayStrip.svelte). The release lays the picture out again at the window,
// from the day's data it holds, so at rest no mark is scaled (da-syaz8). Read
// off the screen: each mark's box as the reader sees it, at rest, under the
// fingers and after the release — and a block, which IS a duration, as the
// control that the zoom happened at all.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, seedLines, todayAt } from './support/app.mjs';
import { settledRoom, say, settledAt, pinch, wheelOverPast, PHONE } from './support/daystrip.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

// A picture held past a lift waits a second here, not a task: so a test
// reads inside the wait on every runner, and a release that held the old
// picture at the new window's map fails (da-mdfkf, da-49ssb).
const holdLift = (context) => context.addInitScript(() => localStorage.setItem('datum.liftHoldMs', '1000'));
const phone = sharedApp({ ...PHONE, clock: todayAt(12), prepare: holdLift });

const TODAY = '.band[data-days-ago="0"]';

/**
 * Today's marks as drawn: a dot, a glyph stretched twice as wide as it is
 * tall, the integrity finding's label, and a block — each
 * one's middle across the screen and its size, in px.
 */
const marks = ({ page }) =>
	page.locator(TODAY).evaluate((band) => {
		const box = (selector) => {
			const r = band.querySelector(selector).getBoundingClientRect();
			return { mid: r.x + r.width / 2, w: r.width, h: r.height };
		};
		return {
			dot: box('[data-field="tea"] .point-fg'),
			glyph: box('[data-field="coffee"] .point-fg'),
			label: box('.error-label'),
			block: box('.block[data-field="work"]')
		};
	});

/**
 * How many of the strip's marks carry a scale of their own: a mark scaled
 * back costs Chrome's compositing update every frame (da-syaz8).
 */
const scaled = ({ page }) =>
	page.evaluate(
		() => [...document.querySelectorAll('.band svg.svg :is(circle, text, use, rect, line)')].filter((m) => getComputedStyle(m).scale !== 'none').length
	);

/** Where the strip's own gridline for `hour` is across the screen. */
const hourAt = ({ page }, hour) =>
	page.evaluate((hour) => {
		const r = document.querySelectorAll('.gridlines .gridline')[hour].getBoundingClientRect();
		return r.x + r.width / 2;
	}, hour);

/** Every `dayview` asked for, by its argv. */
const dayviews = async (seen) => (await seen()).filter((argv) => argv[0] === 'dayview' && !argv.includes('--data-layer'));

/** `a` and `b` the same size, to a fraction of a pixel. */
function sameSize(a, b, what) {
	assert.ok(
		Math.abs(a.w - b.w) < 0.3 && Math.abs(a.h - b.h) < 0.3,
		`${what}: ${JSON.stringify(a)} is not ${JSON.stringify(b)}`
	);
}

async function seed(app) {
	const setup = await app.page.evaluate(() =>
		window.datum.runQuiet(['setup', 'coffee', '--verb', 'occur', '--symbol', '☕', '--symbol-aspect', '2'])
	);
	assert.equal(setup.code, 0, setup.stderr);
	await seedLines(app, [
		'occur tea -t 9:00',
		'occur coffee -t 10:00',
		'start work -t 8:00',
		'end work -t 11:30',
		// Started twice: `check integrity` draws a finding at 11:00.
		'start rd -t 10:30',
		'start rd -t 11:00'
	]);
}

test('a zoom moves every mark to its time, stretches none, and asks datum for nothing', async () => {
	await settledRoom(phone, 'marks', async (app) => {
		const { page } = app;
		await say(app, { dayHeight: 60 });
		await settledAt(app, 60);
		await seed(app);
		const rest = await marks(app);
		assert.equal(await scaled(app), 0, 'a mark at rest is scaled');

		// Under the fingers nothing in a band may change but its map: no
		// picture swapped in, no mark moved by a redraw.
		await page.evaluate(() => {
			window.__redrawn = 0;
			window.__watch = new MutationObserver(() => (window.__redrawn += 1));
			for (const band of document.querySelectorAll('.band svg'))
				window.__watch.observe(band, { subtree: true, childList: true, attributes: true });
		});
		const seen = await app.spyOnRunner();
		const box = await page.locator(TODAY).boundingBox();
		let during;
		// Fingers drawn three times as far apart: a third of the day.
		await pinch(page, box, 100, 300, async () => {
			during = await marks(app);
			assert.deepEqual(await dayviews(seen), [], 'the gesture asked datum for a picture');
			assert.equal(await page.evaluate(() => (window.__watch.disconnect(), window.__redrawn)), 0, 'a band’s picture changed under the fingers');
			assert.ok((await scaled(app)) > 0, 'under the fingers no mark is scaled back');
		});
		await app.idle();
		// The release lays today out again at the window: then nothing is scaled.
		await until(() => scaled(app), (n) => n === 0);
		const after = await marks(app);

		// The control: the zoom happened, and a duration is as long as its hours.
		assert.ok(
			Math.abs(during.block.w / rest.block.w - 3) < 0.05,
			`the block is ${during.block.w}px, was ${rest.block.w}`
		);
		for (const [at, name] of [
			[during, 'under the fingers'],
			[after, 'after the release']
		]) {
			for (const mark of ['dot', 'glyph', 'label'])
				sameSize(at[mark], rest[mark], `the ${mark} ${name}`);
			// And where its time is: the dot on the strip's 09:00.
			const nine = await hourAt(app, 9);
			assert.ok(Math.abs(at.dot.mid - nine) < 0.5, `the dot ${name} is at ${at.dot.mid}, 09:00 at ${nine}`);
		}

		assert.deepEqual(await dayviews(seen), [], 'the release asked datum for a picture');
		assert.deepEqual(app.errors, []);
	});
});

test('a pinch down the strip grows a dot as the day drawn at that height has it', async () => {
	await settledRoom(phone, 'tall marks', async (app) => {
		const { page } = app;
		await say(app, { dayHeight: 60 });
		await settledAt(app, 60);
		await seed(app);
		const rest = await marks(app);
		const box = await page.locator(TODAY).boundingBox();
		let during;
		/** How much wider than the picture's own a mark is drawn, across the screen. */
		const across = (selector) =>
			page.locator(`${TODAY} ${selector}`).evaluate((el) => parseFloat(getComputedStyle(el).scale));
		// Twice as far apart down the strip: a day twice as tall.
		await pinch(page, box, 30, 60, async () => {
			during = await marks(app);
			// A line keeps its stroke, where a dot grows with the day.
			assert.equal(await across('.error-line'), 1, 'the line’s stroke grew with the day');
			assert.equal(await across('[data-field="tea"] .point-fg'), 2);
		}, 'y');
		await settledAt(app, 120);
		await app.idle();
		const drawn = await marks(app);

		// datum sizes a dot by the day's height: the preview's is the drawn
		// one's, round, where the stretched picture's was twice as tall.
		assert.ok(
			Math.abs(drawn.dot.w / rest.dot.w - 2) < 0.05,
			`the drawn dot is ${drawn.dot.w}px, was ${rest.dot.w}`
		);
		sameSize(during.dot, drawn.dot, 'the previewed dot');
		sameSize(during.glyph, drawn.glyph, 'the previewed glyph');
		assert.deepEqual(app.errors, []);
	});
});

// da-hmtoq: vividn, 2026-10-25: «The one thing that doesn't seem to zoom in
// well are the striped multi-state region, which expand with the zoom … I
// would keep the same stripe pattern as you are zooming in so that visually
// it looks the same rather than stretching it like currently». A pattern
// tiles in its block's user space, which the map stretches; so read where
// the tile lands on the screen — the block's own screen CTM times the
// pattern's transform — for the stripes and for the hatch of the hour a
// clock change skipped (Berlin, 2026-03-29).
const dst = sharedApp({ ...PHONE, timezone: 'Europe/Berlin', clock: Date.UTC(2026, 2, 29, 12), prepare: holdLift });

/** How the stripes' and the hatch's tiles lie on the screen, as a 2×2, and the block's width. */
const tiles = ({ page }) =>
	page.locator(TODAY).evaluate((band) => {
		const on = (filled) => {
			// The page's pattern of that id, which is the one drawn (da-8kryw).
			const pattern = document.getElementById(filled.getAttribute('fill').slice('url(#'.length, -1));
			const m = filled.getScreenCTM().multiply(pattern.patternTransform.baseVal.consolidate().matrix);
			return [m.a, m.b, m.c, m.d];
		};
		const block = band.querySelector('rect.multi-state');
		return {
			stripes: on(block),
			hatch: on(band.querySelector('.dead-zone')),
			block: block.getBoundingClientRect().width
		};
	});

/** Each tile laid on the screen as it was at rest. */
function sameTiles(at, rest, when) {
	for (const tile of ['stripes', 'hatch'])
		assert.ok(
			at[tile].every((v, i) => Math.abs(v - rest[tile][i]) < 1e-3),
			`the ${tile} ${when}: ${at[tile]} is not ${rest[tile]}`
		);
}

test('a zoom keeps the stripes and the hatch as wide and as steep as at rest', async () => {
	await settledRoom(dst, 'stripes', async (app) => {
		const { page } = app;
		await say(app, { dayHeight: 60 });
		await settledAt(app, 60);
		await seedLines(app, ['switch posture vertical,sit -t 6:00', 'switch posture standing -t 11:00']);
		const rest = await tiles(app);
		const box = await page.locator(TODAY).boundingBox();
		let during;
		await pinch(page, box, 100, 300, async () => (during = await tiles(app)));
		// The control: the block is as long as its hours.
		assert.ok(Math.abs(during.block / rest.block - 3) < 0.05, `the block is ${during.block}px, was ${rest.block}`);
		sameTiles(during, rest, 'under the fingers');
		await app.idle();
		sameTiles(await tiles(app), rest, 'after the release');
		// And a day twice as tall, previewed: the tile is not stretched down.
		await pinch(page, box, 30, 60, async () => sameTiles(await tiles(app), rest, 'under a pinch down the strip'), 'y');
		assert.deepEqual(app.errors, []);
	});
});

// da-332xu: vividn, 2026-10-30: «zooming in on the dayview while data is
// selected causes a strange bug. after letting go, the day with the data (and
// only that day) offsets itself considerably, showing a different part of the
// day misaligned with the time axis». So read each day against the axis after
// the release, the selected one and the one beside it alike: its box unscrolled,
// and its block's edge on the strip's 02:00.
const offAxis = (app) =>
	app.page.evaluate(() => {
		const hours = document.querySelectorAll('.gridlines .gridline');
		const two = hours[2].getBoundingClientRect();
		return [...document.querySelectorAll('.band')]
			.filter((band) => band.querySelector('.block[data-field="work"]'))
			.map((band) => {
				const block = band.querySelector('.block[data-field="work"]').getBoundingClientRect();
				const box = band.getBoundingClientRect();
				return {
					day: band.dataset.daysAgo,
					scrolled: [band.scrollLeft, band.scrollTop],
					across: block.x - (two.x + two.width / 2),
					wide: block.width,
					down: (block.y - box.y) / box.height
				};
			});
	});

/**
 * The two days' blocks, and today's tea at 11:00, tapped: selected, and focused
 * as a tap focuses. Past a window on the morning, where a box can scroll to it:
 * a mark before the window lies at a negative scroll, which none reaches.
 */
async function selectTea(app) {
	await say(app, { dayHeight: 60 });
	await settledAt(app, 60);
	await seedLines(app, [
		'start work -d -1 -t 2:00',
		'end work -d -1 -t 4:00',
		'start work -t 2:00',
		'end work -t 4:00',
		'occur tea -t 11:00'
	]);
	const { page } = app;
	const at = await page.locator(`${TODAY} [data-field="tea"] .point-fg`).boundingBox();
	await page.touchscreen.tap(at.x + at.width / 2, at.y + at.height / 2);
	const chosen = page.locator(`${TODAY} svg.svg [data-selected]`);
	await until(() => chosen.count(), (n) => n > 0);
	assert.ok(await chosen.first().evaluate((m) => m === document.activeElement), 'the tap focused no mark');
	return page.locator(TODAY).boundingBox();
}

test('a zoom with an entry selected leaves its day on the axis, as the day beside it', async () => {
	await settledRoom(phone, 'selected zoom', async (app) => {
		const box = await selectTea(app);
		const rest = await offAxis(app);
		// About the band's first third, three times over: 11:00 falls off the window.
		await pinch(app.page, { ...box, width: box.width / 3 }, 40, 120);
		await app.idle();
		await until(() => scaled(app), (n) => n === 0);
		const days = await offAxis(app);
		assert.equal(days.length, 2, JSON.stringify(days));
		// The control: the zoom happened, which no scroll can fake, a block's width being its hours.
		const wide = days[0].wide / rest[0].wide;
		assert.ok(Math.abs(wide - 3) < 0.2, `the block grew ${wide} times`);
		for (const day of days) {
			assert.deepEqual(day.scrolled, [0, 0], `day ${day.day} is scrolled in its band: ${JSON.stringify(days)}`);
			assert.ok(Math.abs(day.across) < 1, `day ${day.day}'s 02:00 is ${day.across}px off the axis`);
		}
		// And a focus of the browser's own, as Tab is, which no call here can tell not to scroll.
		await app.page.locator(`${TODAY} svg.svg [data-selected]`).first().evaluate((m) => (m.blur(), m.focus()));
		assert.deepEqual((await offAxis(app)).map((day) => day.scrolled), [[0, 0], [0, 0]], 'a focus scrolled a band');
		assert.deepEqual(app.errors, []);
	});
});

test('a redraw of the day holding the focus leaves a stack scrolled back where it is', async () => {
	await settledRoom(phone, 'selected scrolled', async (app) => {
		const { page } = app;
		await selectTea(app);
		// The keyboard's focus, which a redraw gives back; a tap's it does not (da-rvbns).
		await page.keyboard.press('Tab');
		const keyed = await page.locator(`${TODAY} svg.svg [data-selected]`).first().evaluate((m) => (m.focus({ preventScroll: true }), m.matches(':focus-visible')));
		assert.ok(keyed, 'the mark does not have the keyboard’s focus');
		const top = () => page.locator('.stack').evaluate((stack) => stack.scrollTop);
		const pinned = await top();
		await wheelOverPast(app, -600);
		const back = await until(top, (at) => at < pinned);
		await app.runLine('occur mate -t 11:30');
		await app.redraw();
		await app.idle();
		// The control: today was drawn again, and the focus is on its new mark.
		await page.locator(`${TODAY} [data-field="mate"]`).first().waitFor({ state: 'attached' });
		assert.ok(await page.locator(`${TODAY} svg.svg [data-selected]`).first().evaluate((m) => m === document.activeElement), 'the redraw kept no focus');
		assert.equal(await top(), back, 'the redraw scrolled the stack to the focused mark');
		assert.deepEqual(app.errors, []);
	});
});

test('a pinch down the strip with an entry selected leaves its day where the day beside it is', async () => {
	await settledRoom(phone, 'selected tall', async (app) => {
		const box = await selectTea(app);
		await pinch(app.page, box, 30, 60, undefined, 'y');
		await settledAt(app, 120);
		await app.idle();
		const days = await offAxis(app);
		assert.equal(days.length, 2, JSON.stringify(days));
		for (const day of days) assert.deepEqual(day.scrolled, [0, 0], `day ${day.day} is scrolled in its band`);
		assert.ok(Math.abs(days[0].down - days[1].down) < 0.01, JSON.stringify(days));
		assert.deepEqual(app.errors, []);
	});
});
