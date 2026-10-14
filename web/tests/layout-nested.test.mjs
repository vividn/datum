// A layout keeps its identity at any depth (da-crp5).
//
// A tab carrying `layout` is a COPY of that saved layout, wherever it sits
// — the root bar's own tabs, or a pane deep in a tree. What that means,
// and what this file holds the app to: the copy's subtree is compared to
// that layout's save, so it wears its own orange dot; save, restore and save
// as reach it through its pane's menu; its unsaved edits are kept
// in the same per-name draft a root tab's are, so they survive a reload.
//
// The bug this file was written for: an edit made INSIDE an embedded
// layout was silently lost on reload. It could not be saved into the outer
// layout (whose save keeps only a reference to the inner one) and could
// not be saved into the inner one (only a root tab had a name). The first
// test is that reproduction.
//
// Driven through the built app in a real browser, like the other
// `layout-*` files, and reading the same things a reader sees: the panes
// on screen, their dots, and what `window.datum.layouts` says is stored.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';
import {
	DESKTOP,
	MINE,
	addTab,
	choose,
	chooseRoot,
	chooser,
	enterLayoutMode,
	exactly,
	fill,
	leaveLayoutMode,
	manage,
	menuButton,
	manageRoot,
	menuLines,
	pane,
	pickFrom,
	rootMenuButton,
	settled,
	titles,
	widgetPane
} from './support/layout.mjs';

before(startApp);
after(stopApp);

// ONE APP for most of this file (da-6b3dj, sharedApp in support/app.mjs):
// what a test here leaves behind is layout — `mine` saved and embedded,
// `main` saved over, the page in layout mode — which `solo` puts back:
// every saved layout dropped, layout mode left, `main` restored to what it
// shipped as. What still opens an app of its own
// reloads, opens a second tab, or holds a widget option this session, which
// `solo` does not put back.
const shared = sharedApp(DESKTOP);

const layouts = (app, fn, ...args) =>
	app.page.evaluate(([fn, args]) => window.datum.layouts[fn](...args), [fn, args]);

/**
 * The pane drawing a copy of `mine` — named by an inner pane it holds,
 * since two copies wear the same title and sit in whatever order the
 * outer tree puts them. The first copy embedded keeps the save's own ids
 * (`m-a`); the second is re-minted `m-a@mine` (shell.ts).
 */
const copy = (page, innerId = 'm-a') =>
	page
		.locator('section.pane[data-title="mine"]')
		.filter({ has: page.locator(`section.pane[data-pane="${innerId}"]`) });

/** The titles of the panes drawn INSIDE a copy, in order. */
const inside = (holder) => holder.locator('section.pane').evaluateAll((all) => all.map((p) => p.dataset.title));

/** Whether a copy wears the modified dot. */
const dotted = async (holder) => (await holder.locator('> .titlebar .dot').count()) > 0;

/** Whether the root bar's tab for `name` wears it. */
const rootDotted = async (page, name) =>
	(await page.locator(`.root-bar [role="tab"].modified`).filter({ hasText: name }).count()) > 0;

/** Open a copy's own menu and choose an item by its words, in either mode. */
async function act(page, label, holder = copy(page)) {
	await pickFrom(page, holder.locator('> .titlebar button.title, > .titlebar [role="tab"][aria-selected="true"]').first(), label);
	await settled(page);
}

/** Save `mine`, then embed it in a pane split off the main layout's tail. */
async function embed(app) {
	const { page } = app;
	await layouts(app, 'saveLayout', MINE);
	await manage(page, 'tail', 'split: a new pane to the right');
	await fill(page, 'mine');
	await page.locator('section.pane[data-title="mine"]').first().waitFor();
	await settled(page);
}

describe('an edit inside an embedded layout', () => {
	test('survives a reload — the reproduction', async () => {
		await withApp(async (app) => { // own app: it reloads
			const { page } = app;
			await embed(app);
			assert.deepEqual(await inside(copy(page)), ['nowview', 'fields']);

			// An edit INSIDE `mine`: a pane split off its nowview.
			await manage(page, 'm-a', 'split: a new pane to the right');
			assert.deepEqual(await inside(copy(page)), ['nowview', 'empty', 'fields']);

			// It is kept the way a root tab's is — in `mine`'s own draft, by
			// name — so it is still there after a reload.
			await app.reload();
			assert.deepEqual(await inside(copy(page)), ['nowview', 'empty', 'fields']);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test("does not go into the outer layout's save: day keeps mine by reference", async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await embed(app);
			await manage(page, 'm-a', 'split: a new pane to the right');

			// Save `main` with the edit inside `mine` standing.
			await chooseRoot(page, 'Save layout');
			await until(() => rootDotted(page, 'main'), (dot) => dot === false);

			// What `main` stored is the REFERENCE: a tab naming `mine`, its
			// content a stub — never the panes of `mine` inlined into it.
			const day = await layouts(app, 'loadLayout', 'main');
			const found = [];
			const walk = (pane) => {
				for (const tab of pane.tabs) {
					if (tab.layout !== undefined) found.push(tab);
					if ('children' in tab.content) tab.content.children.forEach(walk);
				}
			};
			walk(day.root);
			assert.equal(found.length, 1, JSON.stringify(found));
			assert.equal(found[0].layout, 'mine');
			assert.equal(found[0].content.kind, 'empty');
			assert.equal(JSON.stringify(day).includes('nowview'), true, 'main has its own nowview');
			assert.equal(JSON.stringify(found[0]).includes('m-a'), false, "mine's panes were inlined into main's save");
		});
	});
});

describe('the dot and the acts on a nested layout', () => {
	test('the nested pane wears its own dot, and its menu saves it away', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await embed(app);
			// Embedding changed `main`, not `mine`.
			await chooseRoot(page, 'Save layout');
			await until(() => rootDotted(page, 'main'), (dot) => dot === false);
			assert.equal(await dotted(copy(page)), false);

			await manage(page, 'm-a', 'split: a new pane to the right');
			assert.equal(await dotted(copy(page)), true, 'the edit inside mine did not dot mine');
			assert.equal(await rootDotted(page, 'main'), false, "the edit inside mine dotted day, which holds it by reference");

			await act(page, 'Save layout');
			await until(() => dotted(copy(page)), (dot) => dot === false);

			// And the save is `mine`'s own: reopened as a root tab it has
			// the new pane.
			const saved = await layouts(app, 'loadLayout', 'mine');
			assert.equal(JSON.stringify(saved).includes('"empty"'), true, JSON.stringify(saved));
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('two copies of one layout', () => {
	/** `mine` embedded twice: in a pane beside the tail, and in one beside the repl. */
	async function embedTwice(app) {
		const { page } = app;
		await embed(app);
		await manage(page, 'repl', 'split: a new pane below');
		await fill(page, 'mine');
		await until(() => page.locator('section.pane[data-title="mine"]').count(), (n) => n === 2);
		await settled(page);
	}

	test('each wears its own dot; saving one moves the other’s', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await embedTwice(app);
			const one = copy(page, 'm-a');
			const two = copy(page, 'm-a@mine');
			assert.deepEqual(await inside(one), ['nowview', 'fields']);
			assert.deepEqual(await inside(two), ['nowview', 'fields']);
			assert.deepEqual([await dotted(one), await dotted(two)], [false, false]);

			// Edit the first copy: it differs from the save, the second does not.
			await manage(page, 'm-a', 'split: a new pane to the right');
			assert.deepEqual([await dotted(one), await dotted(two)], [true, false]);

			// Saving the first makes ITS tree the save — so the second, which
			// still holds the old one, is now the copy that differs.
			await act(page, 'Save layout', one);
			await until(() => dotted(two), (dot) => dot === true);
			assert.equal(await dotted(one), false);
			assert.deepEqual(await inside(two), ['nowview', 'fields'], 'the other copy is not rewritten by the save');
			assert.deepEqual(app.errors, []);
		});
	});

	test('the ids stay apart across a save and a reload', async () => {
		await withApp(async (app) => { // own app: it reloads
			const { page } = app;
			await embedTwice(app);
			await chooseRoot(page, 'Save layout');
			await until(() => rootDotted(page, 'main'), (dot) => dot === false);

			await app.reload();
			await until(() => page.locator('section.pane[data-title="mine"]').count(), (n) => n === 2);
			await settled(page);
			// Both copies drew, from the one save, with ids that did not
			// collide — and nothing drifted, so `main` is not modified by
			// having been loaded.
			assert.deepEqual(await inside(copy(page, 'm-a')), ['nowview', 'fields']);
			assert.deepEqual(await inside(copy(page, 'm-a@mine')), ['nowview', 'fields']);
			const ids = await page.locator('section.pane').evaluateAll((all) => all.map((p) => p.dataset.pane));
			assert.equal(new Set(ids).size, ids.length, `duplicate pane ids: ${ids.join(', ')}`);
			assert.equal(await rootDotted(page, 'main'), false, 'a reload reported day as modified');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

describe('a save in another tab', () => {
	test('reaches a nested copy of that layout', async () => {
		await withApp(async (app) => { // own app: it opens a second tab
			const { page } = app;
			await embed(app);
			assert.deepEqual(await inside(copy(page)), ['nowview', 'fields']);

			// A second tab on the same database saves `mine` with a third pane.
			const other = await page.context().newPage();
			await other.goto(page.url());
			await other.waitForFunction(() => typeof window.datum?.run === 'function');
			const grown = structuredClone(MINE);
			grown.root.tabs[0].content.children.push(widgetPane('m-c', 'tail'));
			grown.root.tabs[0].content.ratios = [1 / 3, 1 / 3, 1 / 3];
			await other.evaluate((l) => window.datum.layouts.saveLayout(l), grown);

			// The copy here follows it: nobody edited it, so it moves to the
			// new save.
			await until(() => inside(copy(page)), (t) => t.length === 3);
			assert.deepEqual(await inside(copy(page)), ['nowview', 'fields', 'tail']);
			assert.equal(await dotted(copy(page)), false);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

// Choosing a WIDGET for a tab that holds a layout (da-8o07).
//
// vividn, from the train: "Select a layout, for example default […] then
// click on default and hit choose, and select a widget, like tail. Instead
// of replacing the default layout with a tail widget, it changes the
// default layout to have a tail widget at the top level […] Instead, the
// default layout should disappear and it should just be a single pane,
// tail."
//
// So the widget is not put INSIDE the copy: it replaces it. The tab sheds
// the name, and with it the dot, the acts and the draft — and the SAVE is
// untouched, because the tab was only ever a copy of it.
describe('a widget chosen for a tab that holds a layout', () => {
	/** The active tab of a pane, as the live tree holds it — `shell` is the root. */
	const activeTabOf = (page, paneId) =>
		page.evaluate((paneId) => {
			const all = [];
			const walk = (p) => {
				all.push(p);
				for (const tab of p.tabs) if ('children' in tab.content) tab.content.children.forEach(walk);
			};
			walk(window.datum.layout.current().root);
			const found = all.find((p) => p.id === paneId);
			return found ? found.tabs[found.active] : null;
		}, paneId);

	/** The tail pane's tab strip, one label per tab, the dot kept. */
	const tabsOf = (page, id) => pane(page, id).locator('[role="tab"]').allInnerTexts();

	test('a pane tab sheds the layout: the tab IS the widget, and the save is untouched', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await layouts(app, 'saveLayout', MINE);
			const before = await layouts(app, 'loadLayout', 'mine');

			// The tail pane's `+`: a second tab holding `mine`, two panes in it.
			await addTab(page, 'tail', 'mine');
			await pane(page, 'tail').locator('[role="tab"]', { hasText: 'mine' }).waitFor();
			await settled(page);
			assert.deepEqual(await inside(copy(page)), ['nowview', 'fields']);

			// That tab’s layout-mode menu: Change Widget → the tail widget.
			await manage(page, 'tail', 'Change Widget');
			await chooser(page).waitFor();
			await chooser(page).locator('button').filter({ hasText: exactly('tail') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			await settled(page);

			// The tab is the widget: no name, no lifted title, no panes under it.
			const tab = await activeTabOf(page, 'tail');
			assert.equal(tab.layout, undefined, JSON.stringify(tab));
			assert.equal(tab.title, undefined, JSON.stringify(tab));
			assert.equal(tab.content.kind, 'tail');
			assert.deepEqual(await tabsOf(page, 'tail'), ['tail', 'tail'], 'the tab still reads mine');
			assert.equal(await page.locator('section.pane[data-title="mine"]').count(), 0);
			assert.equal(await pane(page, 'tail').locator('[role="tab"] .dot').count(), 0, 'a shed tab wears a dot');

			// The save is whole: the tab was a copy, and the copy is gone.
			assert.deepEqual(await layouts(app, 'loadLayout', 'mine'), before);
			await chooseRoot(page, 'Save layout');
			await until(() => rootDotted(page, 'main'), (dot) => dot === false);
			assert.deepEqual(await layouts(app, 'loadLayout', 'mine'), before, 'saving day wrote over mine');
			assert.deepEqual(app.errors, []);
		});
	});

	// At the ROOT the shed would cost the tab its name, and a root tab's
	// name is the whole of what this device writes down (`shell.ts`'s
	// `settle`): «day → Change Widget → tail», a reload, and the tail tab is
	// gone. So at the root the widget is its BUILT-IN one-widget layout
	// instead (da-y2f6) — the tab IS the widget and is called `tail`, with
	// the four acts a named tab has and a line in the open list.
	test('a ROOT tab becomes the built-in layout of that widget, and survives a reload', async () => {
		await withApp(async (app) => { // own app: it reloads
			const { page } = app;
			await layouts(app, 'saveLayout', MINE);
			const before = await layouts(app, 'loadLayout', 'mine');

			// The root bar's `+`: `mine` as a second root tab.
			await page.locator('.root-bar button[aria-label="open a layout"]').click();
			await chooser(page).locator('button').filter({ hasText: exactly('mine') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			await page.locator('.root-bar [role="tab"]', { hasText: 'mine' }).waitFor();
			await settled(page);

			// `mine` is the name open above the tab, and `tail` is not it:
			// nothing here is a layout holding itself.
			assert.deepEqual(
				await page.evaluate(() => window.datum.shell.ancestorsOf(window.datum.layout.current().root.tabs[1].id)),
				['mine']
			);

			await manageRoot(page, 'Change Widget');
			await chooser(page).waitFor();
			await chooser(page).locator('button').filter({ hasText: exactly('tail') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			// The built-in is READ before it lands (`resolve`), so the tab
			// changes a beat after the modal closes.
			await page.locator('.root-bar [role="tab"]', { hasText: 'tail' }).waitFor();
			await settled(page);

			const tab = await activeTabOf(page, 'shell');
			assert.equal(tab.layout, 'tail', JSON.stringify(tab));
			assert.equal(tab.content.kind, 'tail');
			assert.deepEqual(await titles(page), ['tail']);
			assert.deepEqual(await page.locator('.root-bar [role="tab"]').allInnerTexts(), ['main', 'tail']);
			assert.equal(await page.locator('.root-bar [role="tab"] .dot').count(), 0, 'the built-in came up modified');
			assert.equal(await page.locator('section.pane[data-title="mine"]').count(), 0);

			// A named tab keeps the acts.
			await leaveLayoutMode(page);
			await rootMenuButton(page).click();
			const lines = await menuLines(page);
			for (const act of ['Save layout', 'Restore layout', 'Save layout as'])
				assert.ok(lines.includes(act), `${act} is missing: ${lines.join(' | ')}`);
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });

			// The name is what the device writes down, so the tab comes back.
			await app.reload();
			await page.locator('.root-bar [role="tab"]', { hasText: 'tail' }).waitFor();
			await settled(page);
			assert.deepEqual(await page.locator('.root-bar [role="tab"]').allInnerTexts(), ['main', 'tail']);
			assert.equal((await activeTabOf(page, 'shell')).layout, 'tail');
			assert.deepEqual(await titles(page), ['tail']);

			// `mine` is as it was saved: the tab was only ever a copy of it.
			assert.deepEqual(await layouts(app, 'loadLayout', 'mine'), before);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

// The pane that HOLDS a sublayout is a pane of the layout ABOVE it
// (da-z8klv): its tabs, its Change Widget, its + and × are the parent's.
// There is one mode for the whole page now (da-vvcxc), so the holder and
// the panes inside the copy follow it alike.
//
// Panes are addressed by `data-pane` and never by `data-title`: a title is
// the ACTIVE TAB's name, and this file's own `Change Widget` test changes
// it. Every id is checked against the page's own list first, so a target
// that has gone missing fails by name rather than reading as a feature
// that is not there.
describe('a pane holding a sublayout', () => {
	/** Every pane drawn, by id. */
	const paneIds = (page) => page.locator('section[data-pane]').evaluateAll((all) => all.map((s) => s.dataset.pane));

	/** The id of the pane holding the copy of `mine`, asserted to be on the page. */
	async function holderId(page) {
		const id = await copy(page).getAttribute('data-pane');
		const ids = await paneIds(page);
		assert.ok(id && ids.includes(id), `no pane holds mine; the page draws ${ids.join(', ')}`);
		return id;
	}

	/** The badges a pane draws on its OWN bar, in order — never a child's. */
	const chrome = (page, id) =>
		pane(page, id)
			.locator('> .titlebar button.add, > .titlebar button.close')
			.evaluateAll((all) => all.map((b) => (b.classList.contains('add') ? '+' : '×')));

	/** The same, for every pane INSIDE the copy: an id and what it draws. */
	const chromeInside = (page) =>
		copy(page)
			.locator('section.pane')
			.evaluateAll((all) =>
				all.map((p) => [
					p.dataset.pane,
					[...p.querySelectorAll(':scope > .titlebar button.add, :scope > .titlebar button.close')].map((b) =>
						b.classList.contains('add') ? '+' : '×'
					)
				])
			);

	/** Open a menu, read its lines, close it. */
	async function linesOf(page, open) {
		await open();
		const lines = await menuLines(page);
		await page.keyboard.press('Escape');
		await page.locator('[role="menu"]').waitFor({ state: 'detached' });
		return lines;
	}

	test('its + and × and those inside the copy are layout mode’s, all alike', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await embed(app);
			const id = await holderId(page);
			const [a, b] = (await chromeInside(page)).map(([paneId]) => paneId);
			assert.deepEqual([a, b], ['m-a', 'm-b'], 'the copy does not hold the panes this test drives');
			assert.deepEqual(await chrome(page, id), ['+', '×'], 'the holder');
			assert.deepEqual(await chromeInside(page), [[a, ['+', '×']], [b, ['+', '×']]], 'inside the copy');

			await leaveLayoutMode(page);
			assert.deepEqual(await chrome(page, id), [], 'the holder kept its chrome in normal mode');
			assert.deepEqual(await chromeInside(page), [[a, []], [b, []]]);
			assert.deepEqual(app.errors, []);
		});
	});

	test('the + on a sublayout’s pane adds a tab beside it, in the parent’s tree', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await embed(app);
			const id = await holderId(page);

			await addTab(page, id, 'tail');
			await pane(page, id).locator('> .titlebar [role="tab"]', { hasText: 'tail' }).waitFor();
			await settled(page);
			assert.deepEqual(await pane(page, id).locator('> .titlebar [role="tab"]').allInnerTexts(), ['mine', 'tail']);

			// The new tab is the HOLDER's, not a tab of `mine`: the save is
			// untouched and the copy still holds its own two panes.
			const saved = await layouts(app, 'loadLayout', 'mine');
			assert.equal(saved.root.tabs.length, 1, JSON.stringify(saved));
			assert.deepEqual(app.errors, []);
		});
	});

	test('Change Widget leads the holder’s layout-mode menu, and it replaces the sublayout', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await embed(app);
			const id = await holderId(page);
			const before = await layouts(app, 'loadLayout', 'mine');

			await enterLayoutMode(page);
			const lines = await linesOf(page, () => menuButton(page, id).click());
			assert.equal(lines[0], 'Change Widget', lines.join(' | '));

			await manage(page, id, 'Change Widget');
			await chooser(page).waitFor();
			await chooser(page).locator('button').filter({ hasText: exactly('tail') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			await settled(page);

			// The copy is gone and the pane is the widget, and the save is as it was.
			assert.equal(await page.locator('section.pane[data-title="mine"]').count(), 0);
			assert.equal(await pane(page, id).getAttribute('data-title'), 'tail');
			assert.deepEqual(await layouts(app, 'loadLayout', 'mine'), before);
			assert.deepEqual(app.errors, []);
		});
	});

	// vividn, asked and answered: «yes, the + should always stay on the root
	// layout, so that you can always add new layouts and views». The root's
	// tabs are the page's LAYOUTS (da-0a5o).
	test('the root’s + is drawn in either mode', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const add = page.locator('.root-bar button[aria-label="open a layout"]');
			assert.equal(await add.count(), 1, 'normal mode took the root’s +');
			await enterLayoutMode(page);
			assert.equal(await add.count(), 1, 'layout mode took the root’s +');
			await leaveLayoutMode(page);
			await add.click();
			await chooser(page).locator('button').filter({ hasText: exactly('stats') }).click();
			await chooser(page).waitFor({ state: 'detached' });
			await page.locator('.root-bar [role="tab"]', { hasText: 'stats' }).waitFor();
			assert.deepEqual(app.errors, []);
		});
	});
});
