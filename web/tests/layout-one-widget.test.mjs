// A root tab whose layout is ONE WIDGET offers that widget's acts (da-ze1x).
//
// `datum.day/repl`, `/nowview`, `/dayview`, `/tail` … each open a root tab
// holding the one-widget built-in (`only()` in `defaults.ts`). Until now
// the root bar's menu for such a tab was the layout's acts alone — the rule
// asked whether the tab NAMED a layout — so someone who opened
// `datum.day/repl` could not reverse it and someone at `/nowview` could not
// set its window, and on a phone that tab is the whole page and the root
// bar's menu the only door to it.
//
// vividn, 2026-09-17 17:18, on the question: «yes to your question about
// single widget root tab».
//
// The rule is now the tab's SHAPE (`widgetOptions` in `options.ts`,
// `resetWidget` in `shell.ts`): a saved layout of one widget is LIFTED into
// the tab that holds it (`inline`, shell.ts), so that tab shows one widget
// and gets `Reset widget` and `Options…` like any other. A layout of
// several widgets arrives under a split and gets neither; its panes have
// their own. Nothing new is stored: the change is a draft edit of the
// layout the tab names, so its dot lights and `Restore layout` is the way
// back.
//
// The lock is da-gvdc's rule, unchanged: a lock stops the layout being
// split, tabbed or resized, and a widget's settings change under one — so
// the locked menu keeps `Options…` (and «defaults» inside it is the reset)
// and not `Reset widget`. Since da-gshh every one of these pages opens
// LOCKED, so `Options…` is what a first visit finds.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, chooseRoot, menuLines, rootMenuButton } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const OPTIONS = 'Options…';
const RESET_WIDGET = 'Reset widget';
const RESET_LAYOUT = 'Reset layout';
const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true } };

/** The layout acts, in the order `layoutActs` builds them for a shipped layout. */
const ACTS = ['Save layout', 'Restore layout', RESET_LAYOUT, 'Save layout as…'];

/** Open the root menu, read its lines, close it again. */
async function rootLines(page) {
	await rootMenuButton(page).click();
	const lines = await menuLines(page);
	await page.keyboard.press('Escape');
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
	return lines;
}

/** Open `/<name>` and wait for the root tab to be it. */
async function openPage(app, name) {
	await app.goto(`/${name}`);
	await rootMenuButton(app.page).filter({ hasText: name }).waitFor();
}

/** The settings of the widget the root tab in front holds. */
const rootSettings = (page) =>
	page.evaluate(() => {
		const root = window.datum.layout.current().root;
		return root.tabs[root.active].content.settings;
	});

const dotted = (page, name) =>
	page.locator('.root-bar [role="tab"].modified').filter({ hasText: name }).count().then((n) => n > 0);

describe('a root tab whose layout is one widget', () => {
	// Locked is what a first visit finds (da-gshh), and da-gvdc's rule
	// decides what a lock leaves: the settings door, not the structure.
	test('/repl opens locked with Options…; unlocked it has Reset widget too', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await openPage(app, 'repl');

			assert.deepEqual(await rootLines(page), [OPTIONS, ...ACTS, 'Unlock layout'], 'the locked root menu');

			await chooseRoot(page, 'Unlock layout');
			const lines = await rootLines(page);
			assert.deepEqual(lines, [
				'Choose…',
				RESET_WIDGET,
				OPTIONS,
				'Split ← → ↑ ↓',
				...ACTS,
				'Lock layout',
				'export backup…',
				'× Close tab'
			]);
			// The two ways back are told apart by their words and by where
			// they sit: `Reset widget` is up with Choose… and Options…, about
			// the widget in front; `Reset layout` is down in the layout's own
			// block, between Restore and Save as.
			assert.ok(lines.indexOf(RESET_WIDGET) < lines.indexOf('Save layout'));
			assert.ok(lines.indexOf(RESET_LAYOUT) > lines.indexOf('Restore layout'));
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('/repl on a phone: Options… flips the order, the tab is modified, it survives a reload, Restore takes it back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await openPage(app, 'repl');
			assert.ok((await rootLines(page)).includes(OPTIONS), 'no Options… on the phone’s root menu');

			await chooseRoot(page, OPTIONS);
			const modal = page.locator('dialog.options');
			await modal.waitFor();
			const box = await modal.boundingBox();
			assert.ok(box.x >= 0 && box.x + box.width <= 390, `the modal runs off the phone: ${JSON.stringify(box)}`);
			await modal.locator('[data-option="reversed"] input').setChecked(true);
			await page.keyboard.press('Escape');
			await modal.waitFor({ state: 'detached' });

			await until(() => rootSettings(page), (s) => s.reversed === true);
			assert.equal(await dotted(page, 'repl'), true, 'the option left the layout unmodified');

			await app.reload();
			await until(() => rootSettings(page), (s) => s.reversed === true);

			await chooseRoot(page, 'Restore layout');
			await until(() => rootSettings(page), (s) => s.reversed !== true);
			assert.equal(await dotted(page, 'repl'), false);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('/nowview: Reset widget from the root menu puts history back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await openPage(app, 'nowview');
			await chooseRoot(page, 'Unlock layout');

			await chooseRoot(page, OPTIONS);
			const modal = page.locator('dialog.options');
			await modal.waitFor();
			await modal.locator('[data-option="history"] select').selectOption('45m');
			await page.keyboard.press('Escape');
			await modal.waitFor({ state: 'detached' });
			await until(() => rootSettings(page), (s) => s.history === '45m');

			await chooseRoot(page, RESET_WIDGET);
			await until(() => rootSettings(page), (s) => s.history === '30m');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// `tail` declares neither options nor defaults (registry.ts), so it has
	// nothing to offer and the items are left out — the same arithmetic a
	// pane's menu does.
	test('/tail: neither item, because its kind declares neither', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await openPage(app, 'tail');
			await chooseRoot(page, 'Unlock layout');
			const lines = await rootLines(page);
			assert.ok(!lines.includes(OPTIONS), `tail reads ${lines.join(' | ')}`);
			assert.ok(!lines.includes(RESET_WIDGET), `tail reads ${lines.join(' | ')}`);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// More than one widget: the root tab holds a split, so there is no one
	// widget to mean and the menu is what it always was.
	test('day: the root menu is unchanged, and its panes keep their own', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const lines = await rootLines(page);
			assert.ok(!lines.includes(OPTIONS), `day reads ${lines.join(' | ')}`);
			assert.ok(!lines.includes(RESET_WIDGET), `day reads ${lines.join(' | ')}`);
			assert.equal(lines[0], 'Choose…');
			assert.deepEqual(lines.slice(-2), ['export backup…', '× Close tab']);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// A layout the reader saved themselves is the same shape and gets the
	// same menu — the rule is never the layout's NAME (da-ze1x's trap).
	test('a one-widget layout a person saved gets the items too, and no Reset layout', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await openPage(app, 'repl');
			await chooseRoot(page, 'Unlock layout');
			page.once('dialog', (dialog) => dialog.accept('mine'));
			await chooseRoot(page, 'Save layout as…');
			await rootMenuButton(page).filter({ hasText: 'mine' }).waitFor();

			const lines = await rootLines(page);
			assert.ok(lines.includes(RESET_WIDGET), `mine reads ${lines.join(' | ')}`);
			assert.ok(lines.includes(OPTIONS), `mine reads ${lines.join(' | ')}`);
			// Nothing shipped under this name, so there is no shipped tree to
			// go back to (da-luwe) — which is the one act the two differ on.
			assert.ok(!lines.includes(RESET_LAYOUT), `mine reads ${lines.join(' | ')}`);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});
