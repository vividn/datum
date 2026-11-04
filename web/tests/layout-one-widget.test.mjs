// A root tab whose layout is ONE WIDGET draws it in a pane of its own
// (da-7nh4l), framed and titled as a pane of a split is, and that pane's
// menu has the widget's acts; the root tab's menu has the layout's.
//
// vividn, 2026-11-04: «single widget layouts should actually draw the
// widget box inside of it. Let's keep the root tabs seperate ideas from
// widget tabs in the layouts so if you select dayview as a tab, within the
// tab there is just a single dayview widget with it's own pane (and the
// dayview label on it's frame) the widget options then go inside that
// dayview's widget menu rather than in the root menu as before», and of
// the elastic drag past an end: «the elastic effect doesn't bring the tabs
// boundary line with it and it should».
//
// `inline` (shell.ts) keeps a saved layout's root pane whole under the root
// tab, and the save is unchanged. Since da-ze1x such a widget's acts were
// the root bar's, the widget filling the root with no frame of its own;
// they are the same acts, in the pane's menu now. An option is THIS
// SESSION'S (da-vhwq), and «Save options with layout» makes it the layout's.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import {
	DESKTOP,
	atRest,
	choiceLines,
	choose,
	chooseRoot,
	enterLayoutMode,
	leaveLayoutMode,
	menuLines,
	optionChecked,
	pickOption,
	rootMenuButton,
	titledMenuButton,
	titledPane,
	toggleOption
} from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const shared = sharedApp(DESKTOP);

const NEWEST = 'newest first';
const HISTORY = 'history';
const FROZEN = 'today frozen at the bottom';
const SAVE = 'Save options with layout';
const RESET_WIDGET = 'Reset widget';
const RESET_LAYOUT = 'Reset layout';
const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true } };

/** The layout acts, in the order `layoutActs` builds them for a shipped layout. */
const ACTS = ['Save layout', 'Restore layout', RESET_LAYOUT, 'Save layout as'];

/** The root's layout-mode menu: what is done to the page, and the ways out. */
const MANAGE = ['Change Widget', 'Split ← ↓ ↑ →', 'Accept', 'Save layout', 'Discard changes', '× Close tab'];

async function linesFrom(page, button) {
	await button.click();
	const lines = await menuLines(page);
	await page.keyboard.press('Escape');
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
	return lines;
}

const rootLines = (page) => linesFrom(page, rootMenuButton(page));
const paneLines = (page, title) => linesFrom(page, titledMenuButton(page, title));

/** Open `/<name>` and wait for the root tab to be it. */
async function openPage(app, name) {
	await app.goto(`/${name}`);
	await rootMenuButton(app.page).filter({ hasText: name }).waitFor();
}

/** The one pane the root tab in front holds: in the tree, and its id. */
const lonePane = (page) =>
	page.evaluate(() => {
		const root = window.datum.layout.current().root;
		const { content } = root.tabs[root.active];
		return content.children?.length === 1 ? content.children[0] : null;
	});

/** The settings of the widget that pane holds. */
const loneSettings = (page) => lonePane(page).then((p) => p.tabs[0].content.settings);

const dotted = (page, name) =>
	page.locator('.root-bar [role="tab"].modified').filter({ hasText: name }).count().then((n) => n > 0);

/** The root tab in front's frame, and its one pane's: both boxes, and the pane's bottom border. */
const frames = (page, title) =>
	page.evaluate((title) => {
		const root = document.querySelector('section.root');
		const front = root.querySelector(':scope > .content > .tab-body:not(.behind)');
		const pane = front.querySelector(`section.pane[data-title="${title}"]`);
		const box = (el) => el.getBoundingClientRect().toJSON();
		return {
			panes: front.querySelectorAll('section.pane').length,
			root: box(root),
			pane: pane && box(pane),
			bottom: pane && getComputedStyle(pane).borderBottomStyle,
			bottomWidth: pane && parseFloat(getComputedStyle(pane).borderBottomWidth)
		};
	}, title);

describe('a root tab whose layout is one widget', () => {
	for (const [what, screen] of [
		['a desktop', DESKTOP],
		['a phone', PHONE]
	])
		test(`${what}: dayview is one pane framed and titled «dayview», its options in its menu and not the root's`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await titledPane(page, 'dayview').waitFor();
					const at = await frames(page, 'dayview');
					assert.equal(at.panes, 1, 'not one pane');
					// The frame is drawn whole, its bottom edge inside the root's box.
					assert.equal(at.bottom, 'solid', 'no bottom edge');
					assert.ok(at.bottomWidth >= 1, 'no bottom edge');
					assert.ok(at.pane.bottom <= at.root.bottom, `the bottom edge is cut off: ${at.pane.bottom} > ${at.root.bottom}`);
					assert.equal((await titledPane(page, 'dayview').locator('> .titlebar h2').innerText()).trim(), 'dayview');

					const pane = await paneLines(page, 'dayview');
					assert.ok(pane.includes(RESET_WIDGET), `the pane reads ${pane.join(' | ')}`);
					assert.ok(pane.some((l) => l.includes(FROZEN)), `the pane reads ${pane.join(' | ')}`);
					assert.deepEqual(await rootLines(page), ACTS);
					assert.deepEqual(app.errors, []);
				},
				{ ...screen, home: { names: ['main', 'dayview'], active: 1 } }
			);
		});

	test('dragged past the last root tab, the pane’s frame moves with its content, and goes back', async () => {
		await withApp(
			async ({ page }) => {
				await titledPane(page, 'dayview').waitFor();
				const before = (await frames(page, 'dayview')).pane;
				const strip = await page.locator('.swipe-strip').boundingBox();
				const y = strip.y + strip.height / 2;
				const cdp = await page.context().newCDPSession(page);
				await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: 290, y }] });
				for (let i = 1; i <= 6; i++)
					await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: 290 - (160 * i) / 6, y }] });
				const held = (await frames(page, 'dayview')).pane;
				const moved = await page
					.locator('section.root > .content > .tab-body:not(.behind)')
					.evaluate((el) => Number(/translateX\((-?[\d.]+)px\)/.exec(el.style.transform)?.[1] ?? 0));
				await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
				await cdp.detach();
				assert.ok(moved < 0, `the drag past the end gave nothing: ${moved}`);
				assert.ok(Math.abs(held.left - before.left - moved) < 1, `the frame stayed: ${before.left} → ${held.left}, the content ${moved}`);
				assert.ok(Math.abs(held.right - before.right - moved) < 1, `the frame stayed: ${before.right} → ${held.right}, the content ${moved}`);
				await atRest(page.locator('section.root > .content'));
				assert.ok(Math.abs((await frames(page, 'dayview')).pane.left - before.left) < 1, 'it did not go back');
			},
			{ ...PHONE, home: { names: ['main', 'dayview'], active: 1 } }
		);
	});

	test('/cli: Reset widget and its option are the pane’s, the root’s are the acts, and the save is unchanged', async () => {
		await shared.wipe(async (app) => {
			const { page } = app;
			await openPage(app, 'cli');
			assert.deepEqual(await paneLines(page, 'cli'), [RESET_WIDGET, NEWEST]);
			assert.deepEqual(await rootLines(page), ACTS);
			await enterLayoutMode(page);
			assert.deepEqual(await rootLines(page), MANAGE);
			await leaveLayoutMode(page);
			// Drawn in a pane, held as it shipped: the pane is the layout's own
			// root pane, and no dot.
			const lone = await lonePane(page);
			assert.deepEqual([lone.id, lone.tabs.length, lone.tabs[0].content.kind], ['cli-only', 1, 'cli']);
			assert.equal(await dotted(page, 'cli'), false, 'drawing it in a pane modified the layout');
			// The page's settings by the root tab's id are its pane's widget's.
			const [byRoot, byPane] = await page.evaluate(() => {
				const root = window.datum.layout.current().root;
				const tab = root.tabs[root.active];
				return [tab.id, tab.content.children[0].tabs[0].id].map((id) => window.datum.layout.settings(id));
			});
			assert.notEqual(byRoot, undefined, 'the root tab’s id finds no widget');
			assert.deepEqual(byRoot, byPane);
			assert.deepEqual(app.errors, []);
		});
	});

	// On a phone the round trip: the switch, the session, the ask that makes
	// it the layout's, and the reload that proves which is which.
	test('/cli on a phone: the pane’s switch flips the order this session, and the save makes it the layout’s', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await openPage(app, 'cli');
			const id = (await lonePane(page)).id;
			await toggleOption(page, id, NEWEST);
			assert.equal(await dotted(page, 'cli'), false, 'a session setting dirtied the layout');
			assert.equal((await loneSettings(page)).reversed, undefined, 'a session setting reached the tree');

			await app.reload();
			assert.equal(await optionChecked(page, id, NEWEST), true, 'a reload lost a session setting');
			assert.equal((await loneSettings(page)).reversed, undefined, 'a kept session setting reached the tree');

			await choose(page, id, SAVE);
			await until(() => loneSettings(page), (s) => s.reversed === true);
			assert.equal(await dotted(page, 'cli'), true, 'the save left the layout unmodified');

			await app.reload();
			await until(() => loneSettings(page), (s) => s.reversed === true);

			await chooseRoot(page, 'Restore layout');
			await until(() => loneSettings(page), (s) => s.reversed !== true);
			assert.equal(await dotted(page, 'cli'), false);
			assert.deepEqual(app.errors, []);
		}, PHONE);
	});

	test('/nowview: Reset widget from the pane’s menu puts history back, saved or held', async () => {
		await shared.wipe(async (app) => {
			const { page } = app;
			await openPage(app, 'nowview');
			const id = (await lonePane(page)).id;

			await pickOption(page, id, HISTORY, '45m');
			await choose(page, id, SAVE);
			await until(() => loneSettings(page), (s) => s.history === '45m');

			await pickOption(page, id, HISTORY, '90m');
			await choose(page, id, RESET_WIDGET);
			await until(() => loneSettings(page), (s) => s.history === '30m');
			assert.deepEqual(await choiceLines(page, id, HISTORY).then((l) => l.filter((v) => v.startsWith('✓'))), [
				'✓ 30m · default'
			]);
			assert.deepEqual(app.errors, []);
		});
	});

	// A layout the reader saved themselves is the same shape and is drawn the
	// same way — the rule is never the layout's NAME (da-ze1x's trap).
	test('a one-widget layout a person saved is a pane too, with the items, and its root has no Reset layout', async () => {
		await shared.wipe(async (app) => {
			const { page } = app;
			await openPage(app, 'cli');
			page.once('dialog', (dialog) => dialog.accept('mine'));
			await chooseRoot(page, 'Save layout as');
			await rootMenuButton(page).filter({ hasText: 'mine' }).waitFor();

			assert.deepEqual(await paneLines(page, 'cli'), [RESET_WIDGET, NEWEST]);
			// Nothing shipped under this name, so there is no shipped tree to
			// go back to (da-luwe).
			assert.deepEqual(await rootLines(page), ACTS.filter((a) => a !== RESET_LAYOUT));
			assert.deepEqual(app.errors, []);
		});
	});
});
