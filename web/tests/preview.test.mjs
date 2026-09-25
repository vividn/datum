// A PREVIEW root tab (da-xamd): `/<name>?from=<layout>`.
//
// vividn, 2026-09-16 22:17: "we have this /layout_name functionality. Maybe
// we can somehow exploit that, so that each test gets its own layout for
// playing around in? maybe we can have a query string like ?from=other_layout
// so that it could load some predefined layout, that might also be useful
// for users but at least not actively harmful" — and at 22:26, on the trap
// of adding it to the root bar for good: "Maybe don't add it as a tab on
// the root layout until a user requests it, similar to vs code, it could be
// in italics until a user says keep tab after opening from a link, 'keep
// tab' could appear in the menu in this case."
//
// So a preview is a tab that is none of this device's business. It is drawn
// in italics, it is written NOWHERE — not the open list, not a draft, not
// the store — and a reload has never heard of it. `Keep tab` in its menu is
// the one door out, and it is a save under the name.
//
// Every assertion here is about a WRITE: what this device's layout storage
// holds, byte for byte, and what the layout store lists. The italics are
// the reader's half of the same fact.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { devices } from 'playwright';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';
import {
	cellExtents,
	chooseRoot,
	grab,
	layoutStorage,
	manageRoot,
	settled,
	takeRoom,
	titles
} from './support/layout.mjs';

before(startApp);
after(stopApp);

const DESKTOP = { viewport: { width: 1280, height: 900 } };

/** The day frame's panes, which is what a room seeded from `main` draws. */
const DAY = ['dayview', 'repl', 'nowview', 'tail', 'custom', 'fields'];

const rootTabs = (page) =>
	page
		.locator('.root-bar [role="tab"]')
		.evaluateAll((all) => all.map((b) => b.textContent.replace('•', '').trim()));

const activeRootTab = (page) =>
	page
		.locator('.root-bar [role="tab"][aria-selected="true"]')
		.evaluate((b) => b.textContent.replace('•', '').trim());

/** The root tabs drawn in ITALICS, as the browser resolves the style. */
const italic = (page) =>
	page
		.locator('.root-bar [role="tab"]')
		.evaluateAll((all) =>
			all
				.filter((b) => getComputedStyle(b).fontStyle === 'italic')
				.map((b) => b.textContent.replace('•', '').trim())
		);

const layouts = (app, fn, ...args) =>
	app.page.evaluate(([fn, args]) => window.datum.layouts[fn](...args), [fn, args]);

/** This device's open layouts, as the shell wrote them (`draft.ts`). */
const openList = (page) =>
	page.evaluate(() => JSON.parse(localStorage.getItem('datum.layout.root.v1') ?? 'null'));

const app = sharedApp(DESKTOP);

describe('a preview tab', () => {
	test('/t-1?from=main is main’s tree in italics, and this device is not told about it', async () => {
		const a = await app();
		await a.solo();
		const { page } = a;

		// The URL itself, a real navigation: the door a reader comes in by.
		await a.goto('/t-1?from=main');
		assert.deepEqual(await rootTabs(page), ['main', 't-1']);
		assert.equal(await activeRootTab(page), 't-1');
		assert.deepEqual(await italic(page), ['t-1'], 'the preview is not the one italic tab');
		// Seeded from `main`: what it draws is the day frame, under its own name.
		assert.deepEqual(await titles(page), DAY);

		// And nothing about it is written down.
		assert.deepEqual(await openList(page), { names: ['main'], active: 0 });
		assert.deepEqual(
			(await layouts(a, 'listLayouts')).map((l) => l.name),
			[],
			'the preview was saved'
		);
		assert.ok(
			!JSON.stringify(await layoutStorage(page)).includes('t-1'),
			'the preview is written on this device somewhere'
		);

		// A reload has never heard of it.
		await a.reload();
		assert.deepEqual(await rootTabs(page), ['main']);
		assert.deepEqual(await italic(page), []);
		assert.deepEqual(a.errors, []);
	});

	test('an existing name wins, a second link brings the preview forward, and a from that is nothing is the notice', async () => {
		const a = await app();
		await a.solo();
		const { page } = a;

		// `main` is a name something answers to, so `?from=` is ignored and it
		// opens exactly as `/day` does — no second tab, nothing italic, and
		// the repl tree it was pointed at nowhere in sight.
		await a.goto('/main?from=repl');
		assert.deepEqual(await rootTabs(page), ['main']);
		assert.deepEqual(await italic(page), []);
		assert.deepEqual(await titles(page), DAY);

		// A link to a name nothing answers to, twice: the second brings the
		// preview to the front rather than opening a second copy of it. The
		// second one is followed WITHOUT a navigation, which is the only way
		// to ask the question — a real one boots the app afresh, and a fresh
		// app has no preview to find.
		await a.goto('/t-2?from=main');
		assert.deepEqual(await rootTabs(page), ['main', 't-2']);
		await page.locator('.root-bar [role="tab"]', { hasText: 'main' }).click();
		await until(() => activeRootTab(page), (t) => t === 'main');
		assert.equal(await page.evaluate(() => window.datum.shell.openPreview('t-2', 'main')), null);
		assert.deepEqual(await rootTabs(page), ['main', 't-2']);
		assert.equal(await activeRootTab(page), 't-2');

		// An EMPTY `?from=` is no `from`: the name opens plainly, so a name
		// that means something opens as itself and nothing is seeded.
		await a.goto('/main?from=');
		assert.deepEqual(await rootTabs(page), ['main']);
		assert.deepEqual(await italic(page), []);
		assert.deepEqual(await titles(page), DAY);

		// A `from` that is nothing is the notice every other missing name
		// gets, naming the one that is missing — not the tab's own name.
		const before = await layoutStorage(page);
		await a.goto('/t-3?from=no-such-layout');
		const notice = page.locator('main .notice');
		await notice.waitFor();
		assert.match(await notice.innerText(), /no-such-layout/);
		assert.doesNotMatch(await notice.innerText(), /t-3/);

		// And an empty one on a name that is nothing is that name's own
		// notice — never «nothing is called “”».
		await a.goto('/t-3?from=');
		await notice.waitFor();
		assert.match(await notice.innerText(), /t-3/);
		// A navigation is a fresh app, and a fresh app has never heard of a
		// preview: `t-2` is gone with the page that held it, which is the
		// whole of what «written nowhere» buys.
		assert.deepEqual(await rootTabs(page), ['main']);
		assert.deepEqual(await layoutStorage(page), before, 'a refused link wrote to this device');
		assert.deepEqual(a.errors, []);
	});

	test('an edit inside the preview is the preview’s alone: main, and main’s save, are untouched', async () => {
		const a = await app();
		await a.solo();
		const { page } = a;
		const day = await cellExtents(page, 'column');

		const clean = await takeRoom(a, 't-4');
		// Drag the border under the room's day strip, a good way down.
		const at = await grab(page, page.locator('.split.column > [role="separator"]').first());
		await page.mouse.move(at.x, at.y + 120);
		await page.mouse.up();
		await settled(page);
		const moved = await cellExtents(page, 'column');
		assert.notDeepEqual(moved, day, 'the drag moved nothing');

		// `main` itself, behind the other tab, is the tree it always was.
		await page.locator('.root-bar [role="tab"]', { hasText: 'main' }).click();
		await until(() => activeRootTab(page), (t) => t === 'main');
		assert.deepEqual(await titles(page), DAY);
		assert.deepEqual(await cellExtents(page, 'column'), day, 'the drag moved main’s own border');
		// Nor is there a save of `main` to have moved: the room's edit is the
		// room's, and nobody's layout was written.
		assert.deepEqual(
			(await layouts(a, 'listLayouts')).map((l) => l.name),
			[]
		);

		// The preview still holds the edit — it lives as long as the page does.
		await page.locator('.root-bar [role="tab"]', { hasText: 't-4' }).click();
		await until(() => activeRootTab(page), (t) => t === 't-4');
		assert.deepEqual(await cellExtents(page, 'column'), moved);
		await clean();
	});

	// kimi-k3, reviewing #23: `saveAs` renames the copy and the chooser
	// replaces it, and neither took the old name out of the preview set — so
	// a layout that later came to exist under that name would have been
	// drawn italic, kept out of the open list and passed over by
	// `followSaves`. A preview is a name a TAB CARRIES; `settle` prunes.
	test('a preview saved under another name gives its own name back', async () => {
		const a = await app();
		const { page } = a;
		await a.room('t-9');

		page.once('dialog', (dialog) => dialog.accept('mine'));
		await chooseRoot(page, 'Save layout as…');
		await until(() => rootTabs(page), (t) => t.includes('mine'));
		// The copy moved to the new name, and it is real: italic nowhere, and
		// in this device's open list.
		assert.deepEqual(await rootTabs(page), ['main', 'mine']);
		assert.deepEqual(await italic(page), []);
		assert.deepEqual(await openList(page), { names: ['main', 'mine'], active: 1 });

		// And `t-9` is a name like any other again: a layout saved under it
		// opens as itself, not as the preview it used to be.
		await page.evaluate(() =>
			window.datum.layouts.saveLayout({
				version: 1,
				name: 't-9',
				root: {
					id: 't9',
					tabs: [{ id: 't9-tab', content: { id: 't9-w', kind: 'nowview', settings: {} } }],
					active: 0,
					bar: { edge: 'top', align: 'start' }
				}
			})
		);
		await page.evaluate(() => window.datum.shell.open('t-9'));
		await until(() => rootTabs(page), (t) => t.includes('t-9'));
		assert.deepEqual(await italic(page), [], 'a real layout was drawn as a preview');
		assert.deepEqual(await openList(page), { names: ['main', 'mine', 't-9'], active: 2 });
		assert.deepEqual(await titles(page), ['nowview']);
		await a.solo();
	});

	test('closing a preview writes nothing at all', async () => {
		const a = await app();
		await a.solo();
		const { page } = a;
		const before = await layoutStorage(page);

		await a.room('t-5');
		assert.deepEqual(await rootTabs(page), ['main', 't-5']);
		// An edit first, so what is thrown away is a tree somebody changed.
		await manageRoot(page, 'split: a new pane to the right');
		await until(() => titles(page), (t) => t.includes('empty'));

		await manageRoot(page, 'Close tab');
		await until(() => rootTabs(page), (t) => !t.includes('t-5'));
		assert.deepEqual(await layoutStorage(page), before, 'closing a preview wrote to this device');
		assert.deepEqual(
			(await layouts(a, 'listLayouts')).map((l) => l.name),
			[]
		);
		assert.deepEqual(a.errors, []);
	});
});

describe('Keep tab', () => {
	test('makes the preview real: a layout of the reader’s own, in the open list, past a reload, and the italics go', async () => {
		const a = await app();
		await a.solo();
		const { page } = a;
		await a.room('t-6');
		assert.deepEqual(await italic(page), ['t-6']);

		await chooseRoot(page, 'Keep tab');
		await until(() => italic(page), (i) => i.length === 0);

		const listed = (await layouts(a, 'listLayouts')).filter((l) => l.name === 't-6');
		assert.deepEqual(listed.map((l) => l.source), ['user']);
		assert.deepEqual(await openList(page), { names: ['main', 't-6'], active: 1 });
		// And what was saved is the tree that was on screen.
		assert.deepEqual(await titles(page), DAY);

		await a.reload();
		assert.deepEqual(await rootTabs(page), ['main', 't-6']);
		assert.equal(await activeRootTab(page), 't-6');
		assert.deepEqual(await italic(page), [], 'a kept tab is still italic after a reload');
		assert.deepEqual(await titles(page), DAY);

		await a.solo();
	});

	test('is offered to a preview and to nobody else, last in the name menu, with Close under the ▾', async () => {
		const a = await app();
		await a.solo();
		const { page } = a;
		const from = async (open) => {
			await open();
			const seen = await page
				.locator('[role="menu"]')
				.evaluate((menu) => [...menu.children].map((el) => el.textContent.trim()));
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });
			return seen;
		};
		const items = () => from(() => page.locator('.root-bar [role="tab"][aria-selected="true"]').click());
		const manage = () => from(() => page.locator('.root-bar button.manage').click());
		assert.ok(!(await items()).includes('Keep tab'), 'main is offered Keep tab');

		await a.room('t-7');
		const menu = await items();
		assert.ok(menu.includes('Keep tab'), `the preview is not offered Keep tab: ${menu}`);
		// `Keep tab` WRITES the tab down, so it stayed with the other acts
		// that write — last in the name's menu, after the layout's acts. The
		// Close that is a preview's other answer (da-xamd) is a management
		// act, so the slicing (da-bzhq) put it under the `▾`, last there.
		// The two answers are one press apart either way; what they are no
		// longer is adjacent.
		assert.equal(menu.indexOf('Keep tab'), menu.length - 1, `Keep tab is not last: ${menu}`);
		assert.equal(menu.at(-2), 'Save layout as…');
		assert.match((await manage()).at(-1), /Close tab$/);
		await a.solo();
	});
});

describe('on a phone', () => {
	// An italic tab is a TAB: `reachable.test.mjs`'s rules hold for it, and
	// the root bar's one scrolling row is still one row (da-0a5o).
	test('the preview tab is on screen, a fingertip tall, and in the one row', async () => {
		await withApp(
			async (a) => {
				const { page } = a;
				await a.goto('/t-8?from=main');
				assert.deepEqual(await rootTabs(page), ['main', 't-8']);
				assert.deepEqual(await italic(page), ['t-8']);
				const row = await page.locator('.root-bar').evaluate((el) => ({
					tabs: [...el.querySelectorAll('[role="tab"]')].map((t) => {
						const r = t.getBoundingClientRect();
						return { top: Math.round(r.top), height: r.height, right: r.right };
					}),
					screen: document.documentElement.clientWidth
				}));
				assert.equal(new Set(row.tabs.map((t) => t.top)).size, 1, 'the tabs wrapped');
				for (const tab of row.tabs) {
					assert.ok(tab.height >= 24, `a root tab is ${tab.height}px tall`);
					assert.ok(tab.right <= row.screen, `a root tab is off-screen at ${tab.right}`);
				}
			},
			{ viewport: { width: 390, height: 844 }, device: devices['iPhone 12'] }
		);
	});
});
