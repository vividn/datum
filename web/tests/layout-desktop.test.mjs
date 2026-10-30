// The desktop default (da-ncsyn): `main`'s tree, which a desktop that has
// never opened the site finds under its `collect` tab (da-u7vz8).
//
// vividn, 2026-10-30 (da-uzi5x): dayview at 1/3 over a row at 2/3 of tail |
// fields | nowview over stats, each column 1/3, and «a separate dayview tab»,
// over the app's own bottom bar with no `input` pane drawing that bar a
// second time.
//
// The commands pane at the foot went in da-mwp4m. vividn, 2026-10-27:
// «remove the command widget on the collect tab (it can remain a widget in
// general)»: its room is the bar's output section's, open over the input.
//
// The rest of the suite opens `main` on a tree it PINS (support/desk.mjs),
// so this file is the one that asks for the shipped tree (`desk: null`).
// The shares are his, and a share is a claim about every screen, so what they
// leave each pane is measured at six.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, atEachScreen, sharedApp, startApp, stopApp } from './support/app.mjs';
import { CHROME, DESKTOP, titles } from './support/layout.mjs';

before(startApp);
after(stopApp);

/** A desktop on its first visit, `collect` in front (da-u7vz8), drawn from the shipped tree. */
const fresh = sharedApp({ ...DESKTOP, home: ON_COLLECT, desk: null });

/**
 * Desktop screens, tallest to shortest: 1080p, a 1440×900 laptop, the
 * suite's own 1280×900, a 1366×768 laptop, Playwright's 1280×720, and a
 * 1024×600 netbook — the shortest a desktop gets, where the column sits
 * nearest its floors.
 */
const SIZES = [
	{ width: 1920, height: 1080 },
	{ width: 1440, height: 900 },
	{ width: 1280, height: 900 },
	{ width: 1366, height: 768 },
	{ width: 1280, height: 720 },
	{ width: 1024, height: 600 }
];

/**
 * Each pane's legible box — registry.ts's `legible`, the content box below
 * which the kind stops being legible — by the id the tree gives the pane.
 * The strip's is one day at the shortest the strip may draw (da-r0ydu).
 * A pane is legible when its box holds this and its frame (`CHROME`), which
 * is how the root's own floor adds them up (Layout.svelte).
 */
const LEGIBLE = {
	strip: { width: 240, height: 71 },
	tail: { width: 200, height: 102 },
	nowview: { width: 160, height: 108 },
	stats: { width: 240, height: 140 },
	bar: { width: 160, height: 49 }
};

/**
 * What his shares leave under its legible box, by screen, said in the PR and
 * not bent away (da-uzi5x): half of two thirds is 127px at a netbook's height.
 */
const SHORT = { '1024×600': ['stats'] };

/** Every dashboard pane's box, and the top of the bottom bar. */
const screen = (page) =>
	page.evaluate(() => ({
		prompt: document.querySelector('.prompt-bar').getBoundingClientRect().top,
		panes: Object.fromEntries(
			[...document.querySelectorAll('main section.pane')].map((p) => {
				const { left, right, top, bottom, width, height } = p.getBoundingClientRect();
				return [p.dataset.pane, { left, right, top, bottom, width, height }];
			})
		)
	}));

describe('a fresh desktop', () => {
	test('`collect` is `main`: dayview / tail | fields | nowview over stats, and no command pane', async () => {
		const { page, errors } = await fresh();
		assert.deepEqual(
			await page.evaluate(() => window.datum.shell.rootTabs(window.datum.layout.current()).map((t) => t.layout)),
			['welcome', 'setup', 'collect', 'dayview']
		);
		assert.deepEqual(await titles(page), ['dayview', 'tail', 'fields', 'nowview', 'stats']);
		// No `input`: the bottom bar is the command line, and one is enough.
		assert.equal(await page.locator('main section.pane[data-title="input"]').count(), 0);
		// Splits, not tabs: every pane is on the screen at once.
		assert.equal(await page.locator('main [role="tab"]').count(), 0, 'something in `main` is tabbed');

		const { panes: p } = await screen(page);
		const near = (a, b, by = 2) => Math.abs(a - b) <= by;
		// The splits' shares are of what the borders between their panes leave.
		const gap = p.tail.top - p.strip.bottom;
		const main = { width: p.nowview.right - p.tail.left - 2 * gap, height: p.tail.bottom - p.strip.top - gap };
		assert.ok(near(p.strip.height, main.height / 3), `the strip is ${p.strip.height} of ${main.height}`);
		assert.ok(near(p.tail.height, (main.height * 2) / 3), `the row is ${p.tail.height} of ${main.height}`);
		assert.ok(near(p.bar.height, p.tail.height) && near(p.bar.top, p.tail.top), 'fields is not as tall as the tail');
		for (const id of ['tail', 'bar', 'nowview']) assert.ok(near(p[id].width, main.width / 3), `${id} is ${p[id].width} of ${main.width}`);
		assert.ok(near(p.stats.width, main.width / 3), `stats is ${p.stats.width} of ${main.width}`);
		assert.ok(p.tail.right <= p.bar.left + 1 && p.bar.right <= p.nowview.left + 1, 'the row is not tail, fields, nowview');
		assert.ok(near(p.nowview.top, p.tail.top) && near(p.stats.bottom, p.tail.bottom), 'nowview over stats is not the row');
		assert.ok(near(p.nowview.height, p.stats.height, 1) && p.nowview.bottom <= p.stats.top + 1, 'nowview is not half over stats');
		// Its room is the bar's output, open over the input.
		assert.equal(await page.locator('.prompt-bar .section.output').count(), 1, 'the output section is not open');
		assert.deepEqual(errors, []);
	});

	test('every pane is legible, and the fields above the bottom bar, at every size', async (t) => {
		const { page } = await fresh();
		const seen = [];
		await atEachScreen(t, page, SIZES, async ({ width, height }) => {
			const { prompt, panes } = await screen(page);
			for (const [id, floor] of Object.entries(LEGIBLE)) {
				if (SHORT[`${width}×${height}`]?.includes(id)) continue;
				const box = panes[id];
				assert.ok(
					box.height >= floor.height + CHROME.height && box.width >= floor.width + CHROME.width,
					`${width}×${height}: ${id} is ${box.width.toFixed(1)}×${box.height.toFixed(1)}, under its ${floor.width}×${floor.height} and its frame`
				);
			}
			// The last row: on the screen, over the bar.
			assert.ok(panes.bar.bottom <= prompt + 0.5, `${width}×${height}: the fields run under the bottom bar`);
			seen.push(`${width}×${height} ${Object.entries(panes).map(([id, b]) => `${id} ${Math.round(b.width)}×${Math.round(b.height)}`).join(' ')}`);
		});
		t.diagnostic(seen.join('\n'));
	});
});
