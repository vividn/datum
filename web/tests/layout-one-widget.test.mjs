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
// and gets `Reset widget` and its OPTIONS, one item each, like any other. A
// layout of several widgets arrives under a split and gets neither; its
// panes have their own.
//
// Since da-vhwq an option is THIS SESSION'S: setting one from this menu
// changes what the tab shows and leaves the layout alone, and «Save options
// with layout» — offered only unlocked, and only with something held — is
// what makes it the layout's. A lock therefore leaves the option rows and
// takes that one act away, which is also why `Reset widget`, a tree write,
// is not in a locked menu. Since da-gshh every one of these pages opens
// LOCKED, so the options are what a first visit finds.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	chooseRoot,
	menuLines,
	choiceLines,
	pickOption,
	rootManageButton,
	rootMenuButton,
	toggleOption,
	toggleRootLock
} from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const NEWEST = 'newest first';
const HISTORY = 'history';
const SAVE = 'Save options with layout';
const RESET_WIDGET = 'Reset widget';
const RESET_LAYOUT = 'Reset layout';
const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true } };

/** The layout acts, in the order `layoutActs` builds them for a shipped layout. */
const ACTS = ['Save layout', 'Restore layout', RESET_LAYOUT, 'Save layout as…'];

/** Open the root menu, read its lines, close it again. */
const rootLines = (page) => linesFrom(page, () => rootMenuButton(page).click());

/** The same, for the `▾` beside the root's `+` (da-bzhq). */
const rootManageLines = (page) => linesFrom(page, () => rootManageButton(page).click());

async function linesFrom(page, open) {
	await open();
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
	test('/repl opens locked with its option; unlocked it has Change Widget and Reset widget too', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await openPage(app, 'repl');

			// The lock is a button beside the gear (da-bzhq), so the locked
			// menu is the settings door and the acts, and no lock item.
			assert.deepEqual(await rootLines(page), [NEWEST, ...ACTS], 'the locked root menu');
			assert.equal(await rootManageButton(page).count(), 0, 'a locked root still offers the ▾');

			await toggleRootLock(page);
			const lines = await rootLines(page);
			assert.deepEqual(lines, ['Change Widget', RESET_WIDGET, NEWEST, ...ACTS, 'export backup…']);
			assert.deepEqual(await rootManageLines(page), ['Split ← ↓ ↑ →', '× Close tab']);
			// The two ways back are told apart by their words and by where
			// they sit: `Reset widget` is up with Change Widget and the
			// options, about the widget in front; `Reset layout` is down in the
			// layout's own block, between Restore and Save as.
			assert.ok(lines.indexOf(RESET_WIDGET) < lines.indexOf('Save layout'));
			assert.ok(lines.indexOf(RESET_LAYOUT) > lines.indexOf('Restore layout'));
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// On a phone this menu is the only door to the widget, so the whole
	// round trip is driven here: the switch, the session, the ask that
	// makes it the layout's, and the reload that proves which is which.
	test('/repl on a phone: the switch flips the order this session, and the save makes it the layout’s', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await openPage(app, 'repl');
			assert.ok((await rootLines(page)).includes(NEWEST), 'no option on the phone’s root menu');
			// Locked, and the option still works: it is not the layout's.
			await toggleOption(page, null, NEWEST);
			assert.equal(await dotted(page, 'repl'), false, 'a session setting dirtied a locked layout');
			assert.equal((await rootSettings(page)).reversed, undefined, 'a session setting reached the tree');
			// The menu whole on a 390px screen, wherever it is placed.
			await rootMenuButton(page).click();
			const box = await page.locator('[role="menu"]').boundingBox();
			assert.ok(box.x >= 0 && box.x + box.width <= 390, `the menu runs off the phone: ${JSON.stringify(box)}`);
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });

			// A reload starts back at what the layout says.
			await app.reload();
			await until(() => rootSettings(page), (s) => s.reversed !== true);

			// Asked for — which wants the lock off — it is a draft edit of the
			// layout the tab names, so the dot lights and it survives a reload.
			await toggleRootLock(page);
			await toggleOption(page, null, NEWEST);
			await chooseRoot(page, SAVE);
			await until(() => rootSettings(page), (s) => s.reversed === true);
			assert.equal(await dotted(page, 'repl'), true, 'the save left the layout unmodified');

			await app.reload();
			await until(() => rootSettings(page), (s) => s.reversed === true);

			await chooseRoot(page, 'Restore layout');
			await until(() => rootSettings(page), (s) => s.reversed !== true);
			assert.equal(await dotted(page, 'repl'), false);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('/nowview: Reset widget from the root menu puts history back, saved or held', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await openPage(app, 'nowview');
			await toggleRootLock(page);

			await pickOption(page, null, HISTORY, '45m');
			await chooseRoot(page, SAVE);
			await until(() => rootSettings(page), (s) => s.history === '45m');

			// Held again over the save: «defaults» goes back past both.
			await pickOption(page, null, HISTORY, '90m');
			await chooseRoot(page, RESET_WIDGET);
			await until(() => rootSettings(page), (s) => s.history === '30m');
			assert.deepEqual(await choiceLines(page, null, HISTORY).then((l) => l.filter((v) => v.startsWith('✓'))), [
				'✓ 30m · default'
			]);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// `custom` declares neither options nor defaults (registry.ts), so it has
	// nothing to offer and the items are left out — the same arithmetic a
	// pane's menu does. It is the example here because `tail` stopped being
	// one: da-hkjp gave that kind a declared `type`, and a kind that declares
	// something has both items.
	test('/custom: neither item, because its kind declares neither', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await openPage(app, 'custom');
			await toggleRootLock(page);
			const lines = await rootLines(page);
			// `custom` declares neither defaults nor options, so unlocked its
			// menu is the whole list less both widget items. (`tail` was the
			// example until da-hkjp gave it a `type`.)
			assert.deepEqual(lines, ['Change Widget', ...ACTS, 'export backup…'], `custom reads ${lines.join(' | ')}`);
			assert.ok(!lines.includes(RESET_WIDGET), `custom reads ${lines.join(' | ')}`);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// More than one widget: the root tab holds a split, so there is no one
	// widget to mean and the menu is what it always was.
	test('day: the root menu is unchanged, and its panes keep their own', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const lines = await rootLines(page);
			assert.ok(!lines.includes(NEWEST), `day reads ${lines.join(' | ')}`);
			assert.ok(!lines.includes(RESET_WIDGET), `day reads ${lines.join(' | ')}`);
			assert.equal(lines[0], 'Change Widget');
			assert.equal(lines.at(-1), 'export backup…');
			assert.deepEqual(await rootManageLines(page), ['Split ← ↓ ↑ →', '× Close tab']);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// A layout the reader saved themselves is the same shape and gets the
	// same menu — the rule is never the layout's NAME (da-ze1x's trap).
	test('a one-widget layout a person saved gets the items too, and no Reset layout', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await openPage(app, 'repl');
			await toggleRootLock(page);
			page.once('dialog', (dialog) => dialog.accept('mine'));
			await chooseRoot(page, 'Save layout as…');
			await rootMenuButton(page).filter({ hasText: 'mine' }).waitFor();

			const lines = await rootLines(page);
			assert.ok(lines.includes(RESET_WIDGET), `mine reads ${lines.join(' | ')}`);
			assert.ok(lines.includes(NEWEST), `mine reads ${lines.join(' | ')}`);
			// Nothing shipped under this name, so there is no shipped tree to
			// go back to (da-luwe) — which is the one act the two differ on.
			assert.ok(!lines.includes(RESET_LAYOUT), `mine reads ${lines.join(' | ')}`);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});
