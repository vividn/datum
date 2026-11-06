// No button draws the browser's default (da-ju446). vividn met it twice on
// 2026-11-06, collect's time options («lots of white», da-iunoo) and the docs'
// «← back» and «contents»: a button no rule names is the browser's light gray
// on the dark page.
//
// So on a phone (where both were found), on a first visit's tabs, every button
// drawn is held to the theme: its background never the browser's own, read
// off an unstyled button made in the same page (enabled and disabled), and
// never white. Transparent is the theme's too. The states walked: the shell,
// the docs at their contents and on a doc, collect at rest and with each of
// its time options out, the setup widget, and the menus.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { ready, startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE, exactly } from './support/layout.mjs';

before(startApp);
after(stopApp);

/** Each button on screen whose background is the browser's default, or white: where it is and what it says. */
const unthemed = (page) =>
	page.evaluate(() => {
		const probe = (disabled) => {
			const b = document.createElement('button');
			b.disabled = disabled;
			b.textContent = 'x';
			document.body.append(b);
			const bg = getComputedStyle(b).backgroundColor;
			b.remove();
			return bg;
		};
		const browsers = new Set([probe(false), probe(true)]);
		const white = (bg) => {
			const [r, g, b, a = 1] = bg.match(/[\d.]+/g).map(Number);
			return r >= 235 && g >= 235 && b >= 235 && a > 0.5;
		};
		return [...document.querySelectorAll('button')]
			.filter((b) => b.checkVisibility({ visibilityProperty: true }) && b.getBoundingClientRect().width > 0)
			.map((b) => ({ b, bg: getComputedStyle(b).backgroundColor }))
			.filter(({ bg }) => browsers.has(bg) || white(bg))
			.map(({ b, bg }) => `${b.closest('[data-title]')?.dataset.title ?? 'page'}: ${b.getAttribute('aria-label') ?? b.textContent.trim()} (${bg})`);
	});

const rootTab = (page, name) => page.locator('.root-bar [role="tab"]').filter({ hasText: exactly(name) });

test('no button draws the browser default background, or white, in the shell, the docs, collect, setup and the menus', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			// A field, so collect draws a field's button in its own color too.
			assert.equal((await app.run(['setup', 'mood', '--verb', 'switch', '--states', 'good,bad'])).code, 0);
			await app.redraw();
			await ready(page);
			const found = [];
			const look = async (state) => found.push(...(await unthemed(page)).map((b) => `${state} › ${b}`));

			await look('the shell');

			// The docs, from the welcome: at the contents «← back» is disabled; on a doc «contents» is drawn too.
			await page.locator('button[data-open="docs"]', { hasText: 'Read the docs' }).click();
			const docs = page.locator('article.doc[data-doc="README.md"] h1');
			await docs.waitFor();
			await look('the docs, at the contents');
			await page.locator('article.doc[data-doc="README.md"] a[data-doc="basics.md"]').click();
			await page.locator('article.doc[data-doc="basics.md"] h1').waitFor();
			await page.locator('button.contents').waitFor();
			await look('the docs, on basics.md');

			// Collect at rest, its time section open, and each of its options out.
			await rootTab(page, 'collection').click();
			await page.locator('article.doc').waitFor({ state: 'hidden' });
			const when = page.locator('section.pane[data-title="collect"] [data-when]');
			await page.locator('section.pane[data-title="collect"] [data-when-clock]').waitFor();
			await look('collect, at rest');
			await page.locator('section.pane[data-title="collect"] [data-when-clock]').click();
			for (const kind of ['time', 'dur']) {
				await when.locator(`[data-part="${kind}"]`).first().click();
				await when.locator(`[data-when-more="${kind}"]`).waitFor();
				await look(`collect, its ${kind} options out`);
				await when.locator(`[data-part="${kind}"]`).first().click();
				await when.locator(`[data-when-more="${kind}"]`).waitFor({ state: 'detached' });
			}

			// The menus: the open root tab's, and a pane's name.
			await page.locator('.root-bar [role="tab"][aria-selected="true"]').click();
			await page.locator('[role="menu"]').first().waitFor();
			await look('the page menu');
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });
			await page.locator('section.pane[data-title="collect"] .titlebar button.title').first().click();
			await page.locator('[role="menu"]').first().waitFor();
			await look('collect’s menu');
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });

			// The setup widget.
			await rootTab(page, 'setup').click();
			await page.locator('section.pane[data-title="setup"]').waitFor();
			await page.locator('.root-bar [role="tab"][aria-selected="true"]').filter({ hasText: exactly('setup') }).waitFor();
			await look('setup');

			assert.deepEqual(found, []);
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, home: null }
	);
});
