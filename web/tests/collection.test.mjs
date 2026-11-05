// The collect tab is «collection», the widget «collect» (da-jzica). vividn,
// 2026-11-03: «let's name the layout collection, and the widget collect».
// The layout's name stays `collect`, its identity, as the widget kind stayed
// `widgetbar` (da-w6437): what it is CALLED is `layoutTitle` (defaults.ts),
// so nothing saved or open migrates, and a layout saved under another name
// is called that.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, chooser, exactly, titles } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** The root bar's tab labels, in order, the modified dot stripped. */
const rootTabs = (page) =>
	page.locator('.root-bar [role="tab"]').evaluateAll((all) => all.map((b) => b.textContent.replace('•', '').trim()));

/** The layouts the root tabs hold: their identities. */
const rootIds = (page) =>
	page.evaluate(() => window.datum.shell.rootTabs(window.datum.layout.current()).map((t) => t.layout));

/** What one group of the open `+` lists. */
const group = (page, label) => chooser(page).locator(`[role="group"][aria-label="${label}"] button`).allInnerTexts();

/** Open the `+` and pick `name` from the group `label`. */
async function pick(page, label, name) {
	await page.locator('.root-bar button[aria-label="open a layout"]').click();
	await chooser(page).locator(`[role="group"][aria-label="${label}"] button`).filter({ hasText: exactly(name) }).click();
	await chooser(page).waitFor({ state: 'detached' });
}

test('a first visit’s third tab reads «collection», holding the layout `collect` and its collect widget', async () => {
	await withApp(async ({ page }) => {
		await until(() => rootTabs(page), (t) => t.join() === 'welcome,premade,setup,collection,dayview');
		assert.deepEqual(await rootIds(page), ['welcome', 'premade', 'setup', 'collect', 'dayview']);
		assert.ok((await titles(page)).includes('collect'), 'the widget is still titled collect');
	}, { ...DESKTOP, home: ON_COLLECT, desk: null });
});

test('the root + lists «collection» under layouts and «collect» under widgets, each opening its own', async () => {
	await withApp(async ({ page }) => {
		// `collect` on a desktop is `main`'s tree (defaults.ts), here the suite's pinned one.
		const main = await titles(page);
		await page.locator('.root-bar button[aria-label="open a layout"]').click();
		await chooser(page).waitFor();
		const layouts = (await group(page, 'Layouts')).map((t) => t.trim());
		const widgets = (await group(page, 'Widgets')).map((t) => t.trim());
		await page.keyboard.press('Escape');
		await chooser(page).waitFor({ state: 'detached' });
		assert.ok(layouts.includes('collection') && !layouts.includes('collect'), `layouts: ${layouts}`);
		assert.ok(widgets.includes('collect') && !widgets.includes('collection'), `widgets: ${widgets}`);

		await pick(page, 'Layouts', 'collection');
		await until(() => rootIds(page), (ids) => ids.includes('collect'));
		assert.deepEqual(await rootTabs(page), ['main', 'collection']);
		assert.deepEqual(await titles(page), main);

		await pick(page, 'Widgets', 'collect');
		await until(() => rootIds(page), (ids) => ids.length === 3);
		assert.deepEqual(await rootIds(page), ['main', 'collect', 'fields']);
		assert.deepEqual(await titles(page), ['collect']);
	}, DESKTOP);
});

test('a saved `collect` from before opens as it was saved, reading «collection»; one saved as another name keeps it', async () => {
	await withApp(async (app) => {
		const { page } = app;
		const mine = { version: 1, name: 'collect', root: { id: 'c', tabs: [{ id: 'c-tab', content: { id: 'c-w', kind: 'nowview', settings: {} } }], active: 0, bar: { edge: 'top', align: 'start' } } };
		await page.evaluate((l) => window.datum.layouts.saveLayout(l), mine);
		await app.reload();
		await until(() => rootTabs(page), (t) => t.join() === 'main,collection');
		await page.evaluate(() => window.datum.shell.activate(1));
		await until(() => titles(page), (t) => t.join() === 'nowview');

		const id = await page.evaluate(() => window.datum.shell.rootTabs(window.datum.layout.current())[1].id);
		await page.evaluate((id) => window.datum.shell.saveAs(id, 'my log'), id);
		await until(() => rootTabs(page), (t) => t.join() === 'main,my log');
	}, { ...DESKTOP, home: { names: ['main', 'collect'], active: 0 } });
});
