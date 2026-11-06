// The nowview's zoom and its picture: a pinch and ctrl+wheel, what is drawn
// past its box, a stretch, the crop of the day's data under its overlay, and
// one band of time across a clock change.
// One of 3 files split from nowview.test.mjs by topic, so each stays well
// inside test.mjs's 300 s file cap (da-5rpcr); the helpers they share are
// support/nowview.mjs.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { DESKTOP, draft } from './support/layout.mjs';
import { until } from './support/until.mjs';
import { PANE, handle, notch, asked, handled, beside, hover, farIs, soloNowview, pinch, TOUCH } from './support/nowview.mjs';

before(startApp);
after(stopApp);

// ── the zoom (da-5goc) ───────────────────────────────────────────────
//
// Two fingers over the picture scale the window, and the release asks for
// the window they left, to the second (da-ygpto); ctrl+wheel lands on the
// ladder the handles' wheels step along.
//
// A pinch scales about the FINGERS (da-3zpdx, vividn, 2026-10-27: «if my
// fingers are on the time of two different occur dots moving the two
// fingers should track with the occur dots regardless if panning or
// zooming, letting go should then keep those two dots exactly where they
// are»): the instant under each finger stays under it, spread, squeezed or
// slid, and the window the release asks for keeps them there. That the
// frame after the release is the frame under the fingers is
// nowrelease.test.mjs.

/**
 * Each coffee's centre on screen, by its instant: the copy drawn, inside its
 * stretch. A picture the now strip cuts in two lays the day's data into each
 * stretch, clipped to it (`compose`), so the copy past its stretch is hidden.
 */
const coffees = (page) =>
	page.locator(`${PANE} svg.svg [data-field="coffee"][data-at]`).evaluateAll((ms) =>
		Object.fromEntries(
			ms.flatMap((m) => {
				const r = m.getBoundingClientRect();
				const [x, y] = [r.left + r.width / 2, r.top + r.height / 2];
				const g = m.closest('[data-lo]');
				const [lo, hi] = ['lo', 'hi'].map((k) => new DOMPoint(Number(g.dataset[k]), 0).matrixTransform(g.getScreenCTM()).x);
				return x >= Math.min(lo, hi) - 1 && x <= Math.max(lo, hi) + 1 ? [[m.dataset.at, { x, y }]] : [];
			})
		)
	);

/** Two fingers through CDP, landing one then the other at `from`, moved in six steps to `to`; `midway` with them still down. */
async function fingers(page, from, to, midway) {
	const cdp = await page.context().newCDPSession(page);
	const at = (type, touchPoints) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
	const step = (t) => from.map((f, i) => ({ id: i + 1, x: f.x + (to[i].x - f.x) * t, y: f.y + (to[i].y - f.y) * t }));
	try {
		await at('touchStart', [step(0)[0]]);
		await at('touchStart', step(0));
		for (let i = 1; i <= 6; i++) await at('touchMove', step(i / 6));
		await midway?.();
		await at('touchEnd', []);
	} finally {
		await cdp.detach();
	}
}

/** The picture's own box. */
const pictureBox = (page) => page.locator(`${PANE} svg.svg`).boundingBox();

describe('the nowview widget zooms', () => {
	test('a pinch keeps the instant under each finger under it: apart, together and sideways, and after the release', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				for (let i = 0; i < 40; i++) await app.runLine(`occur coffee -t -${1 + 3 * i}`);
				await app.redraw();
				await page.locator(`${PANE} svg.svg [data-field="coffee"][data-at]`).first().waitFor({ state: 'attached' });
				await app.idle();
				await app.spyOnRunner();
				const band = await page.locator(`${PANE} .nowview-band`).boundingBox();
				for (const [name, to] of [
					// Back first: together, about the fingers, would otherwise
					// bring the time past now in, and the window stops at now
					// (da-v8gzq).
					['back', (a, b) => [a.x + 150, b.x + 150]],
					['apart', (a, b) => [a.x - 30, b.x + 30]],
					['together', (a, b) => [a.x + (b.x - a.x) / 5, b.x - (b.x - a.x) / 5]],
					['sideways', (a, b) => [a.x + 40, b.x + 40]]
				]) {
					// Two coffees in the window, clear of the now strip and of the band's ends.
					const shown = Object.entries(await coffees(page)).filter(([, c]) => c.x > band.x + 40 && c.x < band.x + band.width * 0.6);
					const pick = (x) => shown.reduce((a, b) => (Math.abs(b[1].x - x) < Math.abs(a[1].x - x) ? b : a));
					const [a, b] = [pick(band.x + 40), pick(band.x + band.width * 0.55)];
					assert.ok(b[1].x - a[1].x > 60, `${name}: no two coffees apart in ${JSON.stringify(shown)}`);
					const xs = to(a[1], b[1]);
					const ends = xs.map((x) => ({ x, y: a[1].y }));
					const off = (when, at) =>
						[a, b].flatMap(([key], i) =>
							Math.abs(at[key]?.x - xs[i]) <= 1 ? [] : [`${name}, ${when}: the coffee at ${key} is at ${at[key]?.x}, its finger at ${xs[i]}`]
						);
					let during;
					await fingers(page, [a[1], b[1]], ends, async () => (during = await coffees(page)));
					assert.deepEqual(off('under the fingers', during), []);
					// Let go: datum's picture of the window replaces the one held.
					await page.waitForFunction((band) => document.querySelector(band).style.getPropertyValue('--zoom') === '1', `${PANE} .nowview-band`);
					await app.idle();
					assert.deepEqual(off('after the release', await coffees(page)), []);
				}
				const asked = (await page.evaluate(() => window.__datumSeen)).filter((a) => a[0] === 'nowview');
				assert.ok(asked.length >= 2, `the pinches asked for ${asked.length} windows`);
				assert.deepEqual(app.errors, []);
			},
			{ ...TOUCH, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
		);
	});

	// vividn, on #1445: «when you click on the handle, have the new times scrollable by mouse wheel».
	const inMinutes = (h) => parseFloat(h) * (h.endsWith('h') ? 60 : h.endsWith('s') ? 1 / 60 : 1);
	for (const kind of ['rel', 'abs'])
		test(`a click opens the ${kind} handle's numbers, a hover does not, and a mouse wheel scrolls them`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await beside(page, PANE);
					await app.spyOnRunner();
					const box = await page.locator(`${PANE} .handle[data-axis="${kind}"]`).boundingBox();
					await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
					assert.equal(await page.locator('.nowview-wheel').count(), 0, 'a hover opened the numbers');
					await page.mouse.down();
					await page.mouse.up();
					await page.locator('.nowview-wheel .notch.held').waitFor();
					const history = () =>
						page.evaluate(() => {
							const a = window.__datumSeen.filter((c) => c[0] === 'nowview').at(-1);
							return a?.[a.indexOf('--history') + 1];
						});
					// What a closed set of numbers was turned short of a step is gone with them.
					await page.mouse.wheel(0, -60);
					await page.mouse.down();
					await page.mouse.up();
					await page.locator('.nowview-wheel').waitFor({ state: 'detached' });
					await page.mouse.down();
					await page.mouse.up();
					await page.locator('.nowview-wheel .notch.held').waitFor();
					await page.mouse.wheel(0, -60);
					await page.waitForTimeout(300);
					assert.equal(await page.locator(`${PANE} .handle.dragged`).count(), 0);
					assert.ok(!(await history()) || inMinutes(await history()) === 30, 'two short turns made a step across a close');
					await page.mouse.wheel(0, -100);
					const longer = await until(history, (m) => m && inMinutes(m) > 30);
					await page.locator('.nowview-wheel .notch.held').waitFor();
					// Past what that turn left short of a step, the other way.
					await page.mouse.wheel(0, 160);
					await until(history, (m) => m && inMinutes(m) < inMinutes(longer));
					assert.equal(await page.locator('.nowview-wheel').count(), 1, 'the wheel closed the numbers');
					assert.deepEqual(app.errors, []);
				},
				{ ...DESKTOP, prepare: (context) => context.addInitScript(draft, soloNowview({ history: '30m' })) }
			);
		});

	test('ctrl+wheel steps the same ladder, and the handles pick up from it', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await app.spyOnRunner();
				const box = await pictureBox(page);
				await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
				await page.keyboard.down('Control');

				await page.mouse.wheel(0, -100); // up: a shorter window
				await asked(page, '20m');
				// Drawn, not only asked: under the wheel its rule is asked too (da-gy4xz).
				await farIs(page, '-20m');

				await page.mouse.wheel(0, 100); // down: a longer one
				await asked(page, '30m');
				await page.keyboard.up('Control');

				// A bare wheel is the pane's own scroll, not a zoom.
				await page.mouse.wheel(0, -100);
				await app.idle();
				await farIs(page, '-30m');

				// The handles pick up from where the wheel left the window.
				await hover(page, '-30m');
				await notch(page, '30m').click();
				await asked(page, '60m');
				assert.deepEqual(app.errors, []);
			},
			{ ...DESKTOP }
		);
	});

	// Off now too: the window keeps the instant under the fingers where it
	// is (da-3zpdx), where a handle scales about now (da-zccjs).
	test('a window centred elsewhere scales about the fingers', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await app.spyOnRunner();
				const box = await pictureBox(page);
				// Now is drawn, if at all, in the surroundings past the window (da-6fdaf).
				const lines = await page
					.locator(`${PANE} svg.svg .current-time-line`)
					.evaluateAll((ls) => ls.map((l) => l.getBoundingClientRect().left));
				assert.ok(lines.every((x) => x >= box.x + box.width), `the now line is in the window: ${lines} of ${JSON.stringify(box)}`);
				// Its ticks count from now, three hours on, and the handle stands in
				// the window's middle, at -3h: the words `farIs` halves.
				await farIs(page, '-6h');

				await pinch(page, box, 80, 120);
				await asked(page, '20m');
				await app.idle();
				// The fingers at the box's middle, a tenth of the box past the
				// window's (the box's last fifth is the strip's room): at 09:03:45.
				// 08:45–09:15 two thirds as long, that instant kept, is centred 09:01:15.
				const centreOf = async () => {
					const argv = (await page.evaluate(() => window.__datumSeen)).filter((a) => a[0] === 'nowview').at(-1);
					return { argv, centre: Date.parse(argv[argv.indexOf('--center') + 1]) };
				};
				const expected = todayAt(9, 1) + 15_000;
				const { argv, centre } = await until(centreOf, ({ centre }) => Math.abs(centre - expected) <= 5_000);
				assert.ok(Math.abs(centre - expected) <= 5_000, `asked for ${argv.join(' ')}`);
				assert.equal(argv.includes('-t'), false, 'a pinch made an instant');
				assert.deepEqual(app.errors, []);
			},
			{
				...TOUCH,
				clock: todayAt(12, 0),
				prepare: (context) =>
					context.addInitScript(draft, soloNowview({ history: '30m', at: '09:00' }))
			}
		);
	});
});

// ── the surroundings (da-61nm8) ──────────────────────────────────────
//
// vividn: «predraw enough of the surrounding time so that I can click
// anywhere in the nowview and move my thumb mouse anywhere horizontally on
// the screen or be able to zoom out and be able to see the data without
// having to redraw». The picture is asked for a window's worth past its box
// either side (`--surround`), and a gesture moves what is drawn: a coffee
// forty minutes back, outside the thirty-minute window, is on screen
// mid-gesture with no nowview asked for since the press — and under a zoom,
// whose rule follows the window (da-gy4xz), none but the overlay of the
// window it is at, the picture under the fingers the one from before it.

describe('the nowview draws past its box', () => {
	const BAND = 'section[data-pane="nowview"] .nowview-band';

	/**
	 * Whether the coffee's mark is drawn inside the band, how many nowviews
	 * were asked for and how many of them were no overlay, and whether the
	 * picture is the one from before the gesture.
	 */
	const midGesture = (page) =>
		page.evaluate((band) => {
			const box = document.querySelector(band).getBoundingClientRect();
			const shown = [...document.querySelectorAll(`${band} svg.svg [data-field="coffee"]:not(g)`)].some((m) => {
				const r = m.getBoundingClientRect();
				return r.width > 0 && r.left >= box.left && r.right <= box.right;
			});
			const asked = (window.__datumSeen ?? []).filter((a) => a[0] === 'nowview');
			const same = document.querySelector(`${band} svg.svg`) === window.__before;
			return { shown, asked: asked.length, data: asked.filter((a) => !a.includes('--overlay')).length, same };
		}, BAND);

	const phone = { viewport: { width: 390, height: 664 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT };

	async function withCoffee(app) {
		await app.runLine('occur coffee -t -40');
		await app.redraw();
		await app.page.locator(`${BAND} svg.svg [data-field="coffee"]:not(g)`).first().waitFor({ state: 'attached' });
		await app.idle();
		const before = await midGesture(app.page);
		assert.equal(before.shown, false, 'the coffee is in the window before any gesture');
		await app.page.evaluate((band) => (window.__before = document.querySelector(`${band} svg.svg`)), BAND);
		await app.spyOnRunner();
		return app.page.locator(BAND).boundingBox();
	}

	test('a drag of a whole width shows what was before the window, and asks for nothing until the lift', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const box = await withCoffee(app);
			const y = box.y + box.height / 2;
			await page.mouse.move(box.x + 4, y);
			await page.mouse.down();
			await page.mouse.move(box.x + 4 + box.width, y, { steps: 8 });
			const during = await midGesture(page);
			await page.mouse.up();
			assert.deepEqual(during, { shown: true, asked: 0, data: 0, same: true });
			assert.deepEqual(app.errors, []);
		}, phone);
	});

	test('a pinch out a step shows it too, with nothing asked for under the fingers but its window’s overlay', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const box = await withCoffee(app);
			let during;
			await pinch(page, box, 160, 80, async () => (during = await midGesture(page)));
			assert.deepEqual({ ...during, asked: during.asked > 0 }, { shown: true, asked: true, data: 0, same: true });
			assert.deepEqual(app.errors, []);
		}, phone);
	});

	test('the clock stands over the now section, «now» under it, and the clock keeps the time', async () => {
		await withApp(
			async ({ page, clock }) => {
				const clockAt = () =>
					page.locator(BAND).evaluate((band) => {
						const box = (selector) => band.querySelector(`svg.svg ${selector}`)?.getBoundingClientRect();
						const [time, strip, word] = [box('.now-clock'), box('.now-strip'), box('.now-label')];
						return {
							text: band.querySelector('svg.svg .now-clock')?.textContent,
							order: time && strip && word && time.bottom <= strip.top + 0.5 && strip.bottom <= word.top + 0.5,
							within: time && strip && time.left >= strip.left - 0.5 && time.right <= strip.right + 0.5
						};
					});
				const at = await until(clockAt, (c) => c.text === '14:30');
				assert.ok(at.order, `clock, strip, «now» from the top: ${JSON.stringify(at)}`);
				assert.ok(at.within, `the clock lies in the strip's width: ${JSON.stringify(at)}`);
				// Well short of what the picture drew ahead being half spent.
				await clock.forward(60_000);
				await until(clockAt, (c) => c.text === '14:31');
			},
			{ ...phone, clock: todayAt(14, 30) }
		);
	});
});

// ── a stretch (da-6fdaf) ─────────────────────────────────────────────
//
// vividn, 2026-10-26: «Rescaling nowview should behave similarly to
// rescaling dayview, so occur dots and multi state stripes don't stretch
// with it … it would be good to load in more data into nowview, so that it
// can handle at least a x5 stretch». A pinch in to a fifth: the dot and the
// stripes' tile lie on the screen as they did at rest, and a coffee 90
// minutes back, three windows out, is drawn under the fingers with no
// nowview asked for since the press but the overlay of the window it is at
// (da-gy4xz).

describe('a nowview stretch moves marks, never sizes them', () => {
	const BAND = 'section[data-pane="nowview"] .nowview-band';
	const phone = { viewport: { width: 390, height: 664 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT };

	/** The tea dot's box, the stripes' tile on the screen as a 2×2, whether the coffee is in the band, the nowviews asked for that are no overlay, and the marks with a scale of their own. */
	const marks = (page) =>
		page.evaluate((band) => {
			const box = document.querySelector(band).getBoundingClientRect();
			const dot = document.querySelector(`${band} svg.svg [data-field="tea"] .point-fg`).getBoundingClientRect();
			const block = document.querySelector(`${band} svg.svg rect.block.multi-state`);
			const pattern = document.getElementById(block.getAttribute('fill').slice('url(#'.length, -1));
			const m = block.getScreenCTM().multiply(pattern.patternTransform.baseVal.consolidate().matrix);
			const coffee = [...document.querySelectorAll(`${band} svg.svg [data-field="coffee"]:not(g)`)].some((c) => {
				const r = c.getBoundingClientRect();
				return r.width > 0 && r.left >= box.left && r.right <= box.right;
			});
			return {
				dot: [dot.width, dot.height],
				tile: [m.a, m.b, m.c, m.d],
				block: block.getBoundingClientRect().width,
				coffee,
				asked: (window.__datumSeen ?? []).filter((a) => a[0] === 'nowview' && !a.includes('--overlay')).length,
				scaled: [...document.querySelectorAll(`${band} svg.svg :is(circle, text, use, rect, line)`)].filter((m) => getComputedStyle(m).scale !== 'none').length
			};
		}, BAND);

	const near = (a, b) => a.every((v, i) => Math.abs(v - b[i]) < 1e-3 * Math.max(1, Math.abs(b[i])));

	test('a pinch in to a fifth keeps the dot and the stripes, and shows the coffee three windows back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of ['occur coffee -t -90', 'switch posture vertical,sit -t -20', 'occur tea -t -10'])
				await app.runLine(line);
			await app.redraw();
			await page.locator(`${BAND} svg.svg rect.block.multi-state`).waitFor({ state: 'attached' });
			await app.idle();
			const rest = await marks(page);
			assert.equal(rest.coffee, false, 'the coffee is in the window before any gesture');
			// At rest the crop lays each mark out at its size: a mark scaled
			// back costs Chrome's compositing update every frame (da-syaz8).
			assert.equal(rest.scaled, 0, 'a mark at rest is scaled');
			await app.spyOnRunner();
			const box = await page.locator(BAND).boundingBox();
			let during;
			// Fingers inside the band, a fifth as far apart at the end.
			const apart = box.width - 20;
			await pinch(page, box, apart, apart / 5, async () => {
				during = await marks(page);
				// The dash under the now strip is the strip's: scaled back with it, about now, never on its own.
				assert.equal(
					await page.locator(`${BAND} svg.svg line.current-time-line-far`).evaluate((l) => getComputedStyle(l).scale),
					'none'
				);
			});
			// The control: the stretch happened, and a duration is as long as its minutes.
			assert.ok(Math.abs(during.block / rest.block - 0.2) < 0.01, `the block is ${during.block}px, was ${rest.block}`);
			assert.ok(near(during.dot, rest.dot), `the dot is ${during.dot}, was ${rest.dot}`);
			assert.ok(near(during.tile, rest.tile), `the stripes' tile is ${during.tile}, was ${rest.tile}`);
			assert.deepEqual([during.coffee, during.asked], [true, 0]);
			assert.ok(during.scaled > 0, 'under the fingers no mark is scaled back');
			await app.idle();
			assert.equal((await marks(page)).scaled, 0, 'a mark is still scaled after the release');
			assert.deepEqual(app.errors, []);
		}, phone);
	});
});

// ── a crop of the day's data (da-0e06p) ──────────────────────────────
//
// vividn, 2026-10-26: «the cached part could just be the data part, and then
// dayview and nowview draw their respective axes and ui elements overtop
// cheaply and quickly without needing any database calls». The picture is
// the CLI's overlay (`nowview --overlay`, no data) with the day's data layer
// (`dayview --data-layer`) laid under it: a stretch asks for the overlay of
// its window on the release, and for no data at all.

describe('the nowview crops the day’s data under its overlay', () => {
	const BAND = 'section[data-pane="nowview"] .nowview-band';
	const phone = { viewport: { width: 390, height: 664 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT };
	const seen = (page) => page.evaluate(() => (window.__datumSeen ?? []).map((a) => a.join(' ')));
	/** The dates a data layer run drew, by its `--end-date` and `--pictures` (#lib/dayLayers.ts), or checked too, by `--digests`. */
	const layerDates = (argv, flags = ['--pictures']) => {
		const words = argv.split(' ');
		const end = words[0] === 'dayview' && words.includes('--data-layer') && words[words.indexOf('--end-date') + 1];
		const flag = flags.find((f) => words.includes(f));
		const backs = end && flag ? words[words.indexOf(flag) + 1].split(',') : [];
		return backs.map((back) => new Date(Date.parse(end) - Number(back) * 86_400_000).toISOString().slice(0, 10));
	};

	test('a pinch in to a fifth asks for the new window’s overlay on the release, and for no data', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const line of ['occur coffee -t -90', 'switch mood calm -t -20', 'occur tea -t -10']) await app.runLine(line);
			await app.redraw();
			await page.locator(`${BAND} svg.svg rect.block[data-state="calm"]`).first().waitFor({ state: 'attached' });
			await app.idle();
			const rest = await handled(page, 'section[data-pane="nowview"]');
			await app.spyOnRunner();
			const box = await page.locator(BAND).boundingBox();
			const apart = box.width - 20;
			await pinch(page, box, apart, apart / 5);
			await app.idle();
			const asked = await seen(page);
			// Its surroundings reach further, and a day held there from before may be
			// checked (da-vnfr3); never a day the window it shows draws.
			const today = await page.evaluate(() => [new Date()].map((d) => [d.getFullYear(), d.getMonth() + 1, d.getDate()].map((n) => String(n).padStart(2, '0')).join('-'))[0]);
			assert.deepEqual(asked.filter((a) => layerDates(a, ['--pictures', '--digests']).includes(today)), [], 'the stretch asked for its window’s data');
			// Under the fingers too, as its rule follows the window (da-gy4xz): an overlay each.
			const pictures = asked.filter((a) => a.startsWith('nowview'));
			assert.ok(pictures.length >= 1, `asked: ${asked.join(' | ')}`);
			for (const a of pictures) assert.match(a, /--overlay .*--history (?!30m)/);
			// The ticks follow the window: its handles say the new one.
			await until(() => handled(page, 'section[data-pane="nowview"]'), (h) => h.join() !== rest.join());
			// The coffee, 90 minutes back, is drawn from the data on screen.
			assert.ok(
				await page.locator(`${BAND} svg.svg [data-field="coffee"]:not(g)`).first().evaluate((c, band) => {
					const [r, b] = [c.getBoundingClientRect(), document.querySelector(band).getBoundingClientRect()];
					return r.width > 0 && r.left >= b.left && r.right <= b.right;
				}, BAND),
				'the coffee is not in the window'
			);
			assert.deepEqual(app.errors, []);
		}, phone);
	});

	test('a window across midnight shows the end of yesterday stitched on', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await app.spyOnRunner();
				for (const line of ['switch mood late -t -20', 'switch mood early -t -15']) await app.runLine(line);
				await app.redraw();
				const late = page.locator(`${BAND} svg.svg rect.block[data-state="late"]`).first();
				await late.waitFor({ state: 'attached' });
				const [r, b] = await late.evaluate((l, band) => [l.getBoundingClientRect(), document.querySelector(band).getBoundingClientRect()].map((x) => x.toJSON()), BAND);
				assert.ok(r.width > 1 && r.left >= b.left && r.right <= b.right, `yesterday's block is at ${JSON.stringify(r)} in ${JSON.stringify(b)}`);
				const yesterday = await page.evaluate(() => {
					const d = new Date();
					d.setDate(d.getDate() - 1);
					return [d.getFullYear(), d.getMonth() + 1, d.getDate()].map((n) => String(n).padStart(2, '0')).join('-');
				});
				assert.ok((await seen(page)).some((a) => layerDates(a).includes(yesterday)), 'yesterday was not asked');
				assert.deepEqual(app.errors, []);
			},
			{ ...phone, clock: todayAt(0, 10) }
		);
	});
});

// ── one band of time across a clock change (da-31hhd) ───────────────
//
// vividn, 2026-10-28: «don't show timezone changes in the nowview. Right now
// scrolling back I can see the DST change rendered in the nowview. It should
// just be one continuous band of time». Its axis is instants, so a window over
// 2026-10-25, when Berlin's clock went back at 01:00 UTC, draws the hour once.

describe('the nowview is one band of time across a clock change', () => {
	const BAND = `${PANE} .nowview-band`;

	test('panned back over Berlin’s fall-back: marks spaced by their instants, the state one block, no seam or zone', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				// A coffee every 20 minutes either side of the change, and a
				// state running through it and through Berlin's midnight (22:00Z).
				const lines = ['switch mood calm -t 2026-10-24T21:30:00+00:00', 'switch mood ok -t 2026-10-25T02:30:00+00:00'];
				for (let m = 21 * 60 + 10; m < 29 * 60; m += 20) {
					const t = new Date(Date.UTC(2026, 9, 24, 0, m)).toISOString().replace('.000Z', '+00:00');
					lines.push(`occur coffee -t ${t}`);
				}
				for (const line of lines) assert.equal((await app.runLine(line)).code, 0, line);
				await app.redraw();
				await page.locator(`${BAND} svg.svg rect.block[data-state="calm"]`).first().waitFor({ state: 'attached' });
				await app.idle();

				const band = await page.locator(BAND).evaluate((el) => {
					const svg = el.querySelector('svg.svg');
					const box = el.getBoundingClientRect();
					const mid = (r) => (r.left + r.right) / 2;
					const inBox = (r) => r.width > 0 && r.left >= box.left - 0.5 && r.right <= box.right + 0.5;
					const axis = [...svg.querySelectorAll('[data-lo]')].find((g) => !g.closest('.crop'));
					const x = (name) => new DOMPoint(Number(axis.getAttribute(name)), 0).matrixTransform(axis.getScreenCTM()).x;
					const pxPerMs = (x('data-hi') - x('data-lo')) / (Date.parse(axis.getAttribute('data-to')) - Date.parse(axis.getAttribute('data-from')));
					const coffees = {};
					for (const m of svg.querySelectorAll('[data-field="coffee"][data-at]')) {
						const r = m.getBoundingClientRect();
						if (inBox(r)) coffees[m.dataset.at] = mid(r);
					}
					const calm = [...svg.querySelectorAll('.block[data-state="calm"]')]
						.map((b) => b.getBoundingClientRect())
						.filter((r) => r.width > 0)
						.map((r) => [r.left, r.right, Math.round(r.height)]);
					return {
						pxPerMs,
						box: [box.left, box.right],
						coffees: Object.entries(coffees).map(([at, x]) => [Date.parse(at), x]).sort((a, b) => a[0] - b[0]),
						calm: calm.sort((a, b) => a[0] - b[0]),
						zones: svg.querySelectorAll('.zone-label, .dead-zone, .lane').length,
						ticks: [...svg.querySelectorAll('.tick-label')].map((t) => t.textContent)
					};
				});

				// Marks either side of 01:00Z, each the next 20 minutes on by the axis's own scale.
				const change = Date.UTC(2026, 9, 25, 1);
				assert.ok(band.coffees.some(([t]) => t < change) && band.coffees.some(([t]) => t > change), JSON.stringify(band));
				assert.ok(band.coffees.length >= 6, JSON.stringify(band));
				for (let i = 1; i < band.coffees.length; i++) {
					const [[t0, x0], [t1, x1]] = [band.coffees[i - 1], band.coffees[i]];
					const want = (t1 - t0) * band.pxPerMs;
					assert.ok(Math.abs(x1 - x0 - want) < 1, `${new Date(t0).toISOString()} → ${new Date(t1).toISOString()}: ${x1 - x0}px, not ${want}px`);
				}
				// The state is one band, a lane's height, unbroken where the days meet.
				assert.equal(new Set(band.calm.map((c) => c[2])).size, 1, `the state narrows: ${JSON.stringify(band.calm)}`);
				for (let i = 1; i < band.calm.length; i++)
					assert.ok(Math.abs(band.calm[i][0] - band.calm[i - 1][1]) < 1, `a seam in the state: ${JSON.stringify(band.calm)}`);
				// No offset label, dead zone or lane, and no tick said twice.
				assert.equal(band.zones, 0);
				assert.deepEqual(band.ticks, [...new Set(band.ticks)]);
				assert.deepEqual(app.errors, []);
			},
			{
				...DESKTOP,
				timezone: 'Europe/Berlin',
				clock: Date.UTC(2026, 9, 27, 12),
				prepare: (context) => context.addInitScript(draft, soloNowview({ history: '8h', at: '2026-10-25T01:00:00+00:00' }))
			}
		);
	});
});
