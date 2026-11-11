// The text widget (da-12308). vividn, 2026-11-11: «Let's make a "text"
// widget, it's normally static text, but during modify mode, you can enter
// in markdown text that gets formatted when not in modify mode».
//
// So out of modify mode its markdown is drawn and nothing is editable; in
// the mode a textarea holds the source, written into the layout, so ✓ keeps
// it over a reload and the built-in's Factory default brings back its text.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, chooseRoot, leaveWidget, modifyWidget, saveOpenTab, titledPane } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const SOURCE = '# Morning\n\nA *slow* start, **then** work.\n\n- coffee\n- walk\n\n[datum](https://datum.day)';

/** `/text` alone, and its pane's id. */
async function opened(app) {
	await app.goto('/text');
	const pane = titledPane(app.page, 'text');
	await pane.waitFor();
	return { pane, id: await pane.getAttribute('data-pane') };
}

/** What the text widget's rendered markdown holds, by element. */
const drawn = (pane) =>
	pane.locator('.text').evaluate((el) => ({
		h1: el.querySelector('h1')?.textContent,
		em: el.querySelector('em')?.textContent,
		strong: el.querySelector('strong')?.textContent,
		items: [...el.querySelectorAll('ul > li')].map((li) => li.textContent),
		link: [el.querySelector('a')?.textContent, el.querySelector('a')?.getAttribute('href')]
	}));

const WANT = { h1: 'Morning', em: 'slow', strong: 'then', items: ['coffee', 'walk'], link: ['datum', 'https://datum.day'] };

describe('the text widget', () => {
	test('empty it says what it is; its markdown, written in modify mode, is drawn after ✓, kept over a reload, and Factory default brings back the built-in', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const { pane, id } = await opened(app);
			await pane.locator('p.quiet').waitFor();
			assert.equal(await pane.locator('textarea').count(), 0, 'editable out of modify mode');

			await modifyWidget(page, id);
			await pane.locator('textarea').fill(SOURCE);
			await leaveWidget(page, id);
			assert.equal(await pane.locator('textarea').count(), 0, 'still editable after ✓');
			assert.deepEqual(await drawn(pane), WANT);

			await app.reload();
			await pane.locator('.text').waitFor();
			assert.deepEqual(await drawn(pane), WANT, 'a reload lost the text');

			// Saved, so the reset has a save to go back from.
			await saveOpenTab(page);
			await chooseRoot(page, 'Factory default');
			await pane.locator('p.quiet').waitFor();
			assert.equal(await pane.locator('.text').count(), 0, 'the reset kept the text');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('↺ takes back what modify mode wrote', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const { pane, id } = await opened(app);
			await modifyWidget(page, id);
			await pane.locator('textarea').fill('kept');
			await leaveWidget(page, id);
			await until(() => pane.locator('.text').textContent(), (t) => t?.trim() === 'kept');

			await modifyWidget(page, id);
			assert.equal(await pane.locator('textarea').inputValue(), 'kept', 'modify mode lost the source');
			await pane.locator('textarea').fill('thrown away');
			await leaveWidget(page, id, 'reset');
			await until(() => pane.locator('.text').textContent(), (t) => t?.trim() === 'kept');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});
