// The day strip's zoom moves marks and stretches none of them (da-dqbso).
//
// vividn, 2026-10-22: «occur dots (and lettering, icons, etc) shouldn't
// stretch out and get weird when zooming in. It should be unnoticeable that
// nothing has redrawn yet. Maybe we don't even need to redraw at all.»
//
// So a band is drawn once, at its own width, and a window is that picture
// mapped; the map stretches time, and the strip scales every mark that is not
// a duration back to its own size (DayStrip.svelte). Read off the screen: each
// mark's box as the reader sees it, at rest, under the fingers and after the
// release — and a block, which IS a duration, as the control that the zoom
// happened at all.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, seedLines, todayAt } from './support/app.mjs';
import { settledRoom, say, settledAt, pinch, PHONE } from './support/daystrip.mjs';

before(startApp);
after(stopApp);

const phone = sharedApp({ ...PHONE, clock: todayAt(12) });

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
		window.datum.runQuiet(['setup', 'coffee', '--verb', 'occur', '--glyph', '☕', '--glyph-aspect', '2'])
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

test('a zoom moves every mark to its time, stretches none, and redraws nothing', async () => {
	await settledRoom(phone, 'marks', async (app) => {
		const { page } = app;
		await say(app, { dayHeight: 60 });
		await settledAt(app, 60);
		await seed(app);
		const rest = await marks(app);

		// From the gesture on, nothing in a band may change but its map: no
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
		});
		await app.idle();
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
		assert.equal(
			await page.evaluate(() => (window.__watch.disconnect(), window.__redrawn)),
			0,
			'a band’s picture changed'
		);
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
const dst = sharedApp({ ...PHONE, timezone: 'Europe/Berlin', clock: Date.UTC(2026, 2, 29, 12) });

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
