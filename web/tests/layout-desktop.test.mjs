// The desktop default (da-ncsyn): `main`'s tree, which a desktop that has
// never opened the site finds under its `collect` tab (da-u7vz8).
//
// vividn, 2026-09-23: «change the default desktop interface to be more
// similar to the mobile interface, so commands along the bottom, next row
// up can just be fields. In the second row, let's have tail, nowview, and
// stats. Then dayview along the top». So: dayview / tail | nowview | stats
// / fields / commands, over the app's own bottom bar and with no `input`
// pane drawing that bar a second time.
//
// The rest of the suite opens `main` on a tree it PINS (support/desk.mjs),
// so this file is the one that asks for the shipped tree (`desk: null`).
// The weights are `legibly`'s over the panes' legible floors, and a weight
// is a claim about every screen, so the claim is measured at five.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, atEachScreen, sharedApp, startApp, stopApp } from './support/app.mjs';
import { CHROME, DESKTOP, pane, titles } from './support/layout.mjs';

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
	bar: { width: 160, height: 49 },
	command: { width: 160, height: 49 }
};

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

/** The commands pane's buttons drawn whole, inside the bar's box. */
const wholeCommands = (page) =>
	pane(page, 'command')
		.locator('.bar.commands')
		.evaluate((el) => {
			const box = el.getBoundingClientRect();
			return [...el.querySelectorAll('button')]
				.filter((b) => {
					const r = b.getBoundingClientRect();
					return r.width > 0 && r.left >= box.left - 0.5 && r.right <= box.right + 0.5 && r.top >= box.top - 0.5 && r.bottom <= box.bottom + 0.5;
				})
				.map((b) => b.textContent.trim());
		});

describe('a fresh desktop', () => {
	test('`collect` is `main`: dayview / tail | nowview | stats / fields / command', async () => {
		const { page, errors } = await fresh();
		assert.deepEqual(
			await page.evaluate(() => window.datum.shell.rootTabs(window.datum.layout.current()).map((t) => t.layout)),
			['welcome', 'track', 'collect']
		);
		assert.deepEqual(await titles(page), ['dayview', 'tail', 'nowview', 'stats', 'fields', 'command']);
		// No `input`: the bottom bar is the command line, and one is enough.
		assert.equal(await page.locator('main section.pane[data-title="input"]').count(), 0);
		// Splits, not tabs: all three of row two are on the screen at once.
		assert.equal(await page.locator('main [role="tab"]').count(), 0, 'something in `main` is tabbed');

		const { panes: p } = await screen(page);
		const near = (a, b) => Math.abs(a - b) <= 1;
		assert.ok(p.strip.bottom <= p.tail.top + 1, 'the strip is not the top row');
		for (const id of ['nowview', 'stats'])
			assert.ok(near(p[id].top, p.tail.top) && near(p[id].bottom, p.tail.bottom), `${id} is not on the tail's row`);
		assert.ok(p.tail.right <= p.nowview.left + 1 && p.nowview.right <= p.stats.left + 1, 'row two is not tail, nowview, stats');
		assert.ok(p.tail.bottom <= p.bar.top + 1, 'fields is not under row two');
		assert.ok(p.bar.bottom <= p.command.top + 1, 'command is not under fields');
		assert.deepEqual(errors, []);
	});

	test('every pane is legible, and the commands whole above the bottom bar, at every size', async (t) => {
		const { page } = await fresh();
		const seen = [];
		await atEachScreen(t, page, SIZES, async ({ width, height }) => {
			const { prompt, panes } = await screen(page);
			for (const [id, floor] of Object.entries(LEGIBLE)) {
				const box = panes[id];
				assert.ok(
					box.height >= floor.height + CHROME.height && box.width >= floor.width + CHROME.width,
					`${width}×${height}: ${id} is ${box.width.toFixed(1)}×${box.height.toFixed(1)}, under its ${floor.width}×${floor.height} and its frame`
				);
			}
			// The pane a reader reaches for, and the one that loses when
			// anything above it grows: on the screen, over the bar, whole.
			assert.ok(panes.command.bottom <= prompt + 0.5, `${width}×${height}: the commands run under the bottom bar`);
			const commands = await wholeCommands(page);
			for (const name of ['Undo', '↷', 'occur', 'start', 'end'])
				assert.ok(commands.includes(name), `${width}×${height}: ${name} is not whole: ${commands.join(' · ')}`);
			seen.push(`${width}×${height} ${Object.entries(panes).map(([id, b]) => `${id} ${Math.round(b.width)}×${Math.round(b.height)}`).join(' ')}`);
		});
		t.diagnostic(seen.join('\n'));
	});
});
