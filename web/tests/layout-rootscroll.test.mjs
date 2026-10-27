// The root tab bar keeps the tab in front in view (da-i0hbs).
//
// vividn, 2026-10-27: «as you're swiping through the tabs, the tab bar at
// the top should scroll with the active tab, so that it is visible. and
// also possible to see what the next and previous tabs are». On a phone
// the strip of root tabs scrolls sideways (RootBar.svelte), and nothing
// scrolled it: a swipe could leave the tab in front off screen.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE, swipeOver } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** This device's open list, written once over the harness's `main`. */
const opened = (names, active) => ({
	...PHONE,
	reducedMotion: 'reduce',
	prepare: (context) =>
		context.addInitScript((json) => {
			if (sessionStorage.getItem('rootscroll.seeded')) return;
			sessionStorage.setItem('rootscroll.seeded', '1');
			localStorage.setItem('datum.layout.root.v1', json);
		}, JSON.stringify({ names, active }))
});

const MANY = ['main', 'tail', 'cli', 'aliases', 'stats', 'sync', 'setup', 'welcome'];

/**
 * Which tabs lie wholly inside the strip's visible box, by name, with the
 * one in front and the strip's scroll.
 */
const inView = (page) =>
	page.locator('.root-bar .tabs').evaluate((strip) => {
		const s = strip.getBoundingClientRect();
		const all = [...strip.querySelectorAll('[role="tab"]')];
		return {
			shown: all
				.filter((t) => {
					const r = t.getBoundingClientRect();
					return r.left >= s.left - 0.5 && r.right <= s.right + 0.5;
				})
				.map((t) => t.textContent.replace('•', '').trim()),
			front: all.findIndex((t) => t.getAttribute('aria-selected') === 'true'),
			names: all.map((t) => t.textContent.replace('•', '').trim()),
			scroll: strip.scrollLeft,
			overflows: strip.scrollWidth > strip.clientWidth
		};
	});

/** Waits until the tab at `index` is in front with its neighbours in view. */
const framed = (page, index) =>
	until(
		() => inView(page),
		({ front, names, shown }) =>
			front === index && [names[index - 1], names[index], names[index + 1]].filter(Boolean).every((n) => shown.includes(n))
	);

describe('the root bar scrolls the tab in front into view', () => {
	test('a click on a far tab, a swipe past the end, and back to the first', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const start = await inView(page);
			assert.deepEqual(start.names, MANY);
			assert.ok(start.overflows, 'the tabs fit the strip — this case needs more of them');
			assert.ok(!start.shown.includes('welcome'), 'the last tab is already in view');

			// A click on the last tab that shows: the strip moves on to show
			// the one after it.
			const edge = start.shown.at(-1);
			await page.locator('.root-bar [role="tab"]', { hasText: edge }).click();
			const clicked = await framed(page, MANY.indexOf(edge));
			assert.ok(clicked.scroll > 0, 'the strip did not scroll');

			// Swipes onward to the last tab, each one framed on its way.
			for (let i = MANY.indexOf(edge) + 1; i < MANY.length; i++) {
				await swipeOver(page, page.locator('section.root > .content'), -120, 0, { x: 0.5, y: 0.97 });
				await framed(page, i);
			}

			// The arrow keys walk back, and the first tab comes into view.
			for (let i = MANY.length - 1; i > 0; i--) {
				await page.locator('.root-bar [role="tab"][aria-selected="true"]').press('ArrowLeft');
				await framed(page, i - 1);
			}
			assert.equal((await inView(page)).scroll, 0, 'the strip did not go back to its start');
		}, opened(MANY, 0));
	});

	test('a strip whose tabs all fit never scrolls', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.ok(!(await inView(page)).overflows, 'two tabs overflow the strip');
			await page.locator('.root-bar [role="tab"]', { hasText: 'tail' }).click();
			await framed(page, 1);
			assert.equal((await inView(page)).scroll, 0);
		}, opened(['main', 'tail'], 0));
	});
});
