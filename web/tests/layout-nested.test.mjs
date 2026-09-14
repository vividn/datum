// A layout keeps its identity at any depth (da-crp5).
//
// A tab carrying `layout` is a COPY of that saved layout, wherever it sits
// — the root bar's own tabs, or a pane deep in a tree. What that means,
// and what this file holds the app to: the copy's subtree is compared to
// that layout's save, so it wears its own orange dot; save, restore, save
// as and lock reach it through its pane's menu; its unsaved edits are kept
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
import { startApp, stopApp, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';
import { DESKTOP, bar, choose, chooseRoot, fill, menuButton, settled, widgetPane } from './support/layout.mjs';

before(startApp);
after(stopApp);

const layouts = (app, fn, ...args) =>
	app.page.evaluate(([fn, args]) => window.datum.layouts[fn](...args), [fn, args]);

/**
 * `mine`: two panes side by side, so there is an INSIDE to edit. A saved
 * layout of one pane is lifted into the tab that holds it (shell.ts), and
 * a lifted one-widget layout has no inside — splitting the pane it draws
 * in splits the tree AROUND it, not the layout.
 */
const MINE = {
	version: 1,
	name: 'mine',
	root: {
		id: 'm',
		active: 0,
		bar,
		tabs: [
			{
				id: 'm-tab',
				content: {
					id: 'm-split',
					direction: 'row',
					ratios: [0.5, 0.5],
					children: [widgetPane('m-a', 'nowview'), widgetPane('m-b', 'fields')]
				}
			}
		]
	}
};

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

/** Open a copy's own menu and choose an item by its words. */
async function act(page, label, holder = copy(page)) {
	await holder.locator('> .titlebar button.title, > .titlebar [role="tab"][aria-selected="true"]').first().click();
	await page.getByRole('menuitem', { name: label, exact: true }).click();
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
	await settled(page);
}

/** Save `mine`, then embed it in a pane split off the day layout's tail. */
async function embed(app) {
	const { page } = app;
	await layouts(app, 'saveLayout', MINE);
	await choose(page, 'tail', 'split: a new pane to the right');
	await fill(page, 'mine');
	await page.locator('section.pane[data-title="mine"]').first().waitFor();
	await settled(page);
}

describe('an edit inside an embedded layout', () => {
	test('survives a reload — the reproduction', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await embed(app);
			assert.deepEqual(await inside(copy(page)), ['nowview', 'fields']);

			// An edit INSIDE `mine`: a pane split off its nowview.
			await choose(page, 'm-a', 'split: a new pane to the right');
			assert.deepEqual(await inside(copy(page)), ['nowview', 'empty', 'fields']);

			// It is kept the way a root tab's is — in `mine`'s own draft, by
			// name — so it is still there after a reload.
			await app.reload();
			assert.deepEqual(await inside(copy(page)), ['nowview', 'empty', 'fields']);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test("does not go into the outer layout's save: day keeps mine by reference", async () => {
		await withApp(async (app) => {
			const { page } = app;
			await embed(app);
			await choose(page, 'm-a', 'split: a new pane to the right');

			// Save `day` with the edit inside `mine` standing.
			await chooseRoot(page, 'Save layout');
			await until(() => rootDotted(page, 'day'), (dot) => dot === false);

			// What `day` stored is the REFERENCE: a tab naming `mine`, its
			// content a stub — never the panes of `mine` inlined into it.
			const day = await layouts(app, 'loadLayout', 'day');
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
			assert.equal(JSON.stringify(day).includes('nowview'), true, 'day has its own nowview');
			assert.equal(JSON.stringify(found[0]).includes('m-a'), false, "mine's panes were inlined into day's save");
		}, DESKTOP);
	});
});

describe('the dot and the acts on a nested layout', () => {
	test('the nested pane wears its own dot, and its menu saves it away', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await embed(app);
			// Embedding changed `day`, not `mine`.
			await chooseRoot(page, 'Save layout');
			await until(() => rootDotted(page, 'day'), (dot) => dot === false);
			assert.equal(await dotted(copy(page)), false);

			await choose(page, 'm-a', 'split: a new pane to the right');
			assert.equal(await dotted(copy(page)), true, 'the edit inside mine did not dot mine');
			assert.equal(await rootDotted(page, 'day'), false, "the edit inside mine dotted day, which holds it by reference");

			await act(page, 'Save layout');
			await until(() => dotted(copy(page)), (dot) => dot === false);

			// And the save is `mine`'s own: reopened as a root tab it has
			// the new pane.
			const saved = await layouts(app, 'loadLayout', 'mine');
			assert.equal(JSON.stringify(saved).includes('"empty"'), true, JSON.stringify(saved));
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('lock reaches a nested layout, and unlock is still reachable under it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await embed(app);
			const inner = page.locator('section.pane[data-title="mine"]').first();

			await act(page, 'Lock layout');
			// Nothing under the lock can be split or resized: no menu on the
			// panes inside, and the border between them is static.
			assert.equal(await inner.locator('section.pane button.title').count(), 0);
			assert.equal(await inner.locator('.border.static').count(), 1);

			// The layout's own menu is still there — a lock you cannot undo
			// is a trap.
			await act(page, 'Unlock layout');
			assert.equal(await inner.locator('section.pane button.title').count(), 2);
			assert.equal(await inner.locator('.border.static').count(), 0);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

describe('two copies of one layout', () => {
	/** `mine` embedded twice: in a pane beside the tail, and in one beside the repl. */
	async function embedTwice(app) {
		const { page } = app;
		await embed(app);
		await choose(page, 'repl', 'split: a new pane below');
		await fill(page, 'mine');
		await until(() => page.locator('section.pane[data-title="mine"]').count(), (n) => n === 2);
		await settled(page);
	}

	test('each wears its own dot; saving one moves the other’s', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await embedTwice(app);
			const one = copy(page, 'm-a');
			const two = copy(page, 'm-a@mine');
			assert.deepEqual(await inside(one), ['nowview', 'fields']);
			assert.deepEqual(await inside(two), ['nowview', 'fields']);
			assert.deepEqual([await dotted(one), await dotted(two)], [false, false]);

			// Edit the first copy: it differs from the save, the second does not.
			await choose(page, 'm-a', 'split: a new pane to the right');
			assert.deepEqual([await dotted(one), await dotted(two)], [true, false]);

			// Saving the first makes ITS tree the save — so the second, which
			// still holds the old one, is now the copy that differs.
			await act(page, 'Save layout', one);
			await until(() => dotted(two), (dot) => dot === true);
			assert.equal(await dotted(one), false);
			assert.deepEqual(await inside(two), ['nowview', 'fields'], 'the other copy is not rewritten by the save');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('the ids stay apart across a save and a reload', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await embedTwice(app);
			await chooseRoot(page, 'Save layout');
			await until(() => rootDotted(page, 'day'), (dot) => dot === false);

			await app.reload();
			await until(() => page.locator('section.pane[data-title="mine"]').count(), (n) => n === 2);
			await settled(page);
			// Both copies drew, from the one save, with ids that did not
			// collide — and nothing drifted, so `day` is not modified by
			// having been loaded.
			assert.deepEqual(await inside(copy(page, 'm-a')), ['nowview', 'fields']);
			assert.deepEqual(await inside(copy(page, 'm-a@mine')), ['nowview', 'fields']);
			const ids = await page.locator('section.pane').evaluateAll((all) => all.map((p) => p.dataset.pane));
			assert.equal(new Set(ids).size, ids.length, `duplicate pane ids: ${ids.join(', ')}`);
			assert.equal(await rootDotted(page, 'day'), false, 'a reload reported day as modified');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

describe('a save in another tab', () => {
	test('reaches a nested copy of that layout', async () => {
		await withApp(async (app) => {
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
