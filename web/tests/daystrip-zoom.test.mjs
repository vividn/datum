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
			return { mid: r.x + r.width / 2, w: r.width, h: r.height, end: r.right };
		};
		return {
			dot: box('[data-field="tea"] .point-fg'),
			glyph: box('[data-field="coffee"] .point-fg'),
			label: box('.error-label'),
			line: box('.error-line'),
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
			// The finding's label ends just before its line (da-9sjt5): scaled
			// back about its right edge, it never runs over or away from it.
			for (const [it, when] of [[rest, 'at rest'], [at, name]])
				assert.ok(
					it.label.end <= it.line.mid + 1 && it.label.end > it.line.mid - 8,
					`the label ${when} ends at ${it.label.end}, its line at ${it.line.mid}`
				);
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

// da-o0r1o: vividn, 2026-11-07: «when zooming in/out on the dayview first the
// stripes seem to dance around quite a bit … after releasing the thumbs, the
// stripes change their visual frequency so when you're very zoomed in the
// stripes get super tiny and no longer resemble stripes. They should maintain
// the same visual frequency across zoom levels.» A block's stripes are a
// texture of the screen, laid from the block's own corner: so read where
// across its tile that corner falls, which a gesture must not move, and the
// tile on the screen at rest at two windows far apart, with the same stripes
// on days the window leaves off screen, whose patterns share today's id.
const desk = sharedApp({ clock: todayAt(12), prepare: holdLift });

/** Where across its tile today's striped block's corner lies, in the tile's units; the tile on the screen as a 2×2; the block's width. */
const stripes = ({ page }) =>
	page.locator(TODAY).evaluate((band) => {
		const block = band.querySelector('rect.multi-state');
		// The page's pattern of that id, which is the one drawn (da-8kryw).
		const pattern = document.getElementById(block.getAttribute('fill').slice('url(#'.length, -1));
		const m = block.getScreenCTM().multiply(pattern.patternTransform.baseVal.consolidate().matrix);
		const r = block.getBoundingClientRect();
		const at = new DOMPoint(r.x, r.y).matrixTransform(m.inverse());
		const [w, h] = ['width', 'height'].map((n) => Number(pattern.getAttribute(n)));
		return { phase: ((at.x % w) + w) % w, w, h, tile: [m.a, m.b, m.c, m.d], block: r.width };
	});

/** The corner where it was across the tile, to a tenth of a unit, either way round. */
function samePhase(at, rest, when) {
	const d = Math.abs(at.phase - rest.phase);
	assert.ok(Math.min(d, rest.w - d) < 0.1, `the stripes slid across the block ${when}: ${at.phase} of ${rest.w}, was ${rest.phase}`);
}

const STRIPED = ['switch posture vertical,sit -t 6:00', 'switch posture standing -t 11:00'];

for (const [name, on] of [['phone', phone], ['desktop', desk]]) {
	test(`${name}: a zoom in hand keeps the stripes still on their block`, async () => {
		await settledRoom(on, `still stripes ${name}`, async (app) => {
			const { page } = app;
			await say(app, { dayHeight: 60 });
			await settledAt(app, 60);
			await seedLines(app, STRIPED);
			const rest = await stripes(app);
			// Each block's own, once a picture: none made again as the zoom moves.
			const patterns = () => page.evaluate(() => document.querySelectorAll('.strip pattern').length);
			const made = await patterns();
			let during;
			if (on === phone) {
				const box = await page.locator(TODAY).boundingBox();
				await pinch(page, box, 100, 300, async () => (during = await stripes(app)));
			} else {
				// ctrl+wheel, read before its pause commits it: the picture is still the one at rest.
				during = await page.locator(TODAY).evaluate(async (band) => {
					const svg = band.querySelector('svg.svg');
					const r = band.getBoundingClientRect();
					band.dispatchEvent(new WheelEvent('wheel', { ctrlKey: true, deltaY: -400, clientX: r.x + r.width / 3, clientY: r.y + r.height / 2, bubbles: true, cancelable: true }));
					await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));
					return band.querySelector('svg.svg') === svg;
				});
				assert.ok(during, 'the wheel committed before the stripes were read');
				during = await stripes(app);
			}
			// The control: the zoom happened.
			assert.ok(during.block / rest.block > 2.5, `the block is ${during.block}px, was ${rest.block}`);
			samePhase(during, rest, 'under the zoom');
			assert.equal(await patterns(), made, 'the zoom made patterns');
			assert.deepEqual(app.errors, []);
		});
	});

	test(`${name}: the stripes stay as wide and as steep through a zoom and after it, under another day's alike`, async () => {
		await settledRoom(on, `rest stripes ${name}`, async (app) => {
			const { page } = app;
			await say(app, { dayHeight: 60 });
			await settledAt(app, 60);
			// Yesterday striped as today, above it on the page: one pattern id.
			await seedLines(app, [...STRIPED, ...STRIPED.map((line) => `${line} -d -1`)]);
			await app.idle();
			const whole = await stripes(app);
			// Today's tile on the screen at every change to the strip and every
			// frame, through the zoom and the redraws after it: the band itself
			// is laid out again first, the others as the page is idle.
			await page.locator(TODAY).evaluate((band) => {
				const edges = () => {
					const block = document.querySelector('.band[data-days-ago="0"] rect.multi-state');
					const pattern = block && document.getElementById(block.getAttribute('fill').slice('url(#'.length, -1));
					if (!pattern) return;
					const m = block.getScreenCTM().multiply(pattern.patternTransform.baseVal.consolidate().matrix);
					const [w, h] = ['width', 'height'].map((n) => Number(pattern.getAttribute(n)));
					window.__edges.push([Math.hypot(m.a, m.b) * w, Math.hypot(m.c, m.d) * h]);
				};
				window.__edges = [];
				window.__sampling = new MutationObserver(edges);
				window.__sampling.observe(band.closest('.strip'), { subtree: true, childList: true, attributes: true });
				const frame = () => (edges(), window.__sampling && requestAnimationFrame(frame));
				frame();
			});
			// Ten times and more, as a reader lets go: a pinch's fingers, diagonal as a thumb's are, or the wheel's notches.
			if (on === phone) await pinch(page, await page.locator(TODAY).boundingBox(), 30, 330, undefined, 'xy');
			else {
				const r = await page.locator(TODAY).boundingBox();
				await page.mouse.move(r.x + r.width / 3, r.y + r.height / 2);
				await page.keyboard.down('Control');
				await page.mouse.wheel(0, -900);
				await page.keyboard.up('Control');
			}
			await until(() => stripes(app), (s) => s.block / whole.block > 8);
			await until(() => scaled(app), (n) => n === 0);
			await app.idle();
			const seen = await page.evaluate(() => (window.__sampling.disconnect(), (window.__sampling = null), window.__edges));
			const zoomed = await stripes(app);
			const rest = [Math.hypot(whole.tile[0], whole.tile[1]) * whole.w, Math.hypot(whole.tile[2], whole.tile[3]) * whole.h];
			const off = seen.filter((e) => e.some((v, i) => Math.abs(v - rest[i]) >= 1));
			assert.deepEqual(off, [], `of ${seen.length} looks, these were not ${rest}`);
			// And at rest, as steep: the tile the same within a pixel along each edge.
			const at = (s) => [s.tile[0] * s.w, s.tile[1] * s.w, s.tile[2] * s.h, s.tile[3] * s.h];
			assert.ok(at(zoomed).every((v, i) => Math.abs(v - at(whole)[i]) < 1), `the tile zoomed: ${zoomed.tile}, at the whole day: ${whole.tile}`);
			assert.deepEqual(app.errors, []);
		});
	});
}
