// The docs widget (da-0d8l6): datum's docs read inside the page. vividn,
// 2026-11-05: «you should be able to explore the docs without leaving the app,
// so make "read the docs" a button that opens some documentation widget that
// let's you explore».
//
// So the welcome's «Read the docs» opens the docs place on the contents (a
// modal on a desktop), a link to another doc opens it there and «← back»
// returns, a link out of the docs opens in a new tab, and every doc the
// contents names opens.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { DESKTOP, widgetPane } from './support/layout.mjs';

before(startApp);
after(stopApp);

const desktop = sharedApp({ ...DESKTOP, desk: null });
const onWelcome = (fn) => desktop.solo(fn, JSON.stringify({ version: 1, name: 'default', root: widgetPane('welcome', 'welcome') }));

const modal = (page) => page.locator('dialog.sheet[open][data-place="docs"]');
const doc = (page, name) => modal(page).locator(`article.doc[data-doc="${name}"]`);

/** Open the docs from the welcome, on their contents. */
async function readTheDocs(page) {
	await page.locator('button[data-open="docs"]', { hasText: 'Read the docs' }).click();
	await doc(page, 'README.md').locator('h1').waitFor();
}

describe('the docs place', () => {
	test('«Read the docs» opens it on the contents; a link to basics.md opens it there, and back returns', async () => {
		await onWelcome(async ({ page, errors }) => {
			await readTheDocs(page);
			assert.equal(await doc(page, 'README.md').locator('h1').innerText(), "datum's docs");
			await doc(page, 'README.md').locator('a[data-doc="basics.md"]').click();
			assert.equal(await doc(page, 'basics.md').locator('h1').innerText(), 'The basics');
			await modal(page).locator('button.back').click();
			await doc(page, 'README.md').locator('h1').waitFor();
			assert.ok(await modal(page).locator('button.back').isDisabled(), 'back is spent at the contents');
			assert.deepEqual(errors, []);
		});
	});

	test('a link out of the docs opens in a new tab', async () => {
		await onWelcome(async ({ page }) => {
			await readTheDocs(page);
			await doc(page, 'README.md').locator('a[data-doc="sync.md"]').click();
			const iroh = doc(page, 'sync.md').locator('a[href="https://iroh.computer"]');
			assert.equal(await iroh.getAttribute('target'), '_blank');
			await modal(page).locator('button.back').click();
			await doc(page, 'README.md').locator('a[data-doc="agents.md"]').click();
			// A file of the repo beside the docs is the repo's, on GitHub.
			const contributing = doc(page, 'agents.md').locator('a[href$="/blob/main/CONTRIBUTING.md"]');
			assert.equal(await contributing.getAttribute('target'), '_blank');
			assert.equal(await contributing.getAttribute('data-doc'), null);
		});
	});

	test('every doc the contents names opens', async () => {
		await onWelcome(async ({ page, errors }) => {
			await readTheDocs(page);
			const named = await doc(page, 'README.md').locator('a[data-doc]').evaluateAll((all) => all.map((a) => a.dataset.doc));
			assert.ok(named.includes('basics.md') && named.length >= 10, `the contents name ${named}`);
			for (const name of new Set(named)) {
				await doc(page, 'README.md').locator(`a[data-doc="${name}"]`).first().click();
				await doc(page, name).locator('h1').waitFor();
				assert.equal(await modal(page).locator('.quiet', { hasText: 'could not be read' }).count(), 0, name);
				await modal(page).locator('button.back').click();
				await doc(page, 'README.md').waitFor();
			}
			assert.deepEqual(errors, []);
		});
	});
});
