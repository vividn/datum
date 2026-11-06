// The nowview follows the bar's pills (da-2v3mr, vividn, 2026-11-06: «when
// you are entering in time or duration pills, the nowview should
// automatically pan and zoom (smoothly animated, but quick) so that you can
// see the time or the full extent of the duration. if the pills are
// dismissed, then return the previous zoom level and pan position», and
// «likewise if you add a quickid pill … pan to where that entry was logged.
// If the entry has a duration also zoom out (if needed)»).
//
// What is asserted is the window the picture on screen draws, read off its
// own attributes, never the widget's state.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, sharedApp } from './support/app.mjs';
import { drag } from './support/press.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const NOW = Date.UTC(2026, 8, 10, 15, 0, 0);
const MINUTE = 60_000;
const NOWVIEW = 'section.pane[data-title="nowview"]';
const desktop = sharedApp({ clock: NOW });

/** The instants the nowview's window shows, from its picture's left edge to its now-line or right edge, and its band's middle. */
const shown = (page) =>
	page.evaluate((sel) => {
		const svg = document.querySelector(`${sel} svg.svg:not(.behind svg)`);
		const box = svg.getBoundingClientRect();
		const view = svg.viewBox.baseVal;
		const px = (u) => box.left + ((u - view.x) * box.width) / view.width;
		const s = svg.querySelector('[data-lo]');
		const [lo, hi] = ['data-lo', 'data-hi'].map((a) => px(Number(s.getAttribute(a))));
		const [from, to] = ['data-from', 'data-to'].map((a) => Date.parse(s.getAttribute(a)));
		const at = (x) => from + ((x - lo) / (hi - lo)) * (to - from);
		const frame = svg.closest('.picture').getBoundingClientRect();
		const ahead = svg.querySelector('.flow') && svg.querySelector('.current-time-line')?.getAttribute('data-now');
		const end = Math.min(frame.right, ahead ? px(Number(ahead)) : Infinity);
		return { from: at(frame.left), to: at(end), y: box.top + box.height / 2, x: (frame.left + end) / 2, pxPerMin: (hi - lo) / ((to - from) / 60_000) };
	}, NOWVIEW);

/** Wait for the window to show `a` … `b`, a second either side to spare: what it shows. */
const showing = (page, a, b) =>
	until(
		() => shown(page),
		(w) => w.from <= a + 1000 && w.to >= b - 1000
	);

/** Wait for the window to be `w` again, to a thirtieth of it at each end. */
const back = (page, w) =>
	until(
		() => shown(page),
		(now) => Math.abs(now.from - w.from) < (w.to - w.from) / 30 && Math.abs(now.to - w.to) < (w.to - w.from) / 30
	);

async function ready(app) {
	await app.page.locator(`${NOWVIEW} svg.svg .gridline`).first().waitFor({ state: 'attached' });
	await app.idle();
}

describe('the nowview follows the bar’s pills', () => {
	test('a −1h time pill brings now−1h into view, a 2h length from it the whole span, and dismissing them returns the window', async () => {
		await desktop.fresh(async (app) => {
			const { page } = app;
			await ready(app);
			const was = await shown(page);
			assert.ok(was.from > NOW - 60 * MINUTE, 'the window showed now−1h before the pill');

			await page.evaluate(() => window.datum.cursor.point({ rel: 60 }));
			await showing(page, NOW - 60 * MINUTE, NOW - 60 * MINUTE);

			// It flies there: the band is scaled part of the way on a frame between.
			const scales = page.evaluate(
				(sel) =>
					new Promise((done) => {
						const seen = [];
						const band = document.querySelector(`${sel} .nowview-band`);
						const end = performance.now() + 600;
						const frame = () => {
							seen.push(Number(band.style.getPropertyValue('--zoom') || 1));
							if (performance.now() < end) requestAnimationFrame(frame);
							else done(seen);
						};
						requestAnimationFrame(frame);
					}),
				NOWVIEW
			);
			await page.evaluate(() => window.datum.cursor.point({ rel: 60, span: 120 }));
			const seen = await scales;
			assert.ok(seen.some((s) => s < 0.95 && s > 0.15), `no frame scaled part way: ${seen.join(' ')}`);
			await showing(page, NOW - 180 * MINUTE, NOW - 60 * MINUTE);
			await app.idle();

			await page.evaluate(() => window.datum.bar.removePill('cursor'));
			await back(page, was);
			assert.equal(await page.evaluate(() => window.datum.cursor.of()), null);
		});
	});

	test('a second pill dropped while the first still flies: their going returns the window from before both', async () => {
		await desktop.fresh(async (app) => {
			const { page } = app;
			await ready(app);
			const was = await shown(page);
			// The first changes the window's length, and the second comes a frame into its fly.
			await page.evaluate(
				() =>
					new Promise((done) => {
						window.datum.cursor.point({ rel: 60, span: 120 });
						requestAnimationFrame(() => requestAnimationFrame(() => (window.datum.cursor.point({ rel: 300 }), done())));
					})
			);
			await showing(page, NOW - 300 * MINUTE, NOW - 300 * MINUTE);
			await app.idle();

			await page.evaluate(() => window.datum.bar.removePill('cursor'));
			await back(page, was);
		});
	});

	test('a tail row’s id pill shows the whole block of its entry', async () => {
		await desktop.fresh(async (app) => {
			const { page } = app;
			const logged = await app.run(['start', 'walk', '2h', '-t', '2026-09-10T11:00:00+00:00']);
			assert.equal(logged.code, 0, logged.stderr);
			await app.redraw();
			await ready(app);
			const was = await shown(page);
			assert.ok(was.from > Date.UTC(2026, 8, 10, 9, 0), 'the block was in view before the tap');

			const row = page.locator('section.pane[data-title="tail"] tbody tr', { hasText: 'walk' }).first();
			await row.click();
			await until(
				() => page.evaluate(() => window.datum.bar.pills().filter((p) => p.kind === 'hid').length),
				(n) => n === 1
			);
			await showing(page, Date.UTC(2026, 8, 10, 9, 0), Date.UTC(2026, 8, 10, 11, 0));
			await app.idle();

			await page.evaluate(() => window.datum.bar.removePill('hid'));
			await back(page, was);
		});
	});

	test('a pan of the reader’s own while a pill stands is kept when the pill goes', async () => {
		await desktop.fresh(async (app) => {
			const { page } = app;
			await ready(app);
			await page.evaluate(() => window.datum.cursor.point({ rel: 60 }));
			const flown = await showing(page, NOW - 60 * MINUTE, NOW - 60 * MINUTE);
			await app.idle();

			// Ten minutes back, by hand.
			await drag(page, { x: flown.x, y: flown.y }, { x: flown.x + 10 * flown.pxPerMin, y: flown.y });
			const panned = await until(
				() => shown(page),
				(w) => w.from < flown.from - 8 * MINUTE
			);
			await app.idle();

			await page.evaluate(() => window.datum.bar.removePill('cursor'));
			await app.idle();
			await page.waitForTimeout(400);
			const after = await shown(page);
			assert.ok(Math.abs(after.from - panned.from) < MINUTE, `the window moved ${(after.from - panned.from) / MINUTE}m when the pill went`);
		});
	});
});
