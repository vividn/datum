// Layouts saved as datum entries in the `_internal` database (da-wpgy).
//
// A saved layout is an entry of field `layout` in a second datum database,
// which the worker keeps on OPFS beside the user's: saving is `update`,
// restoring is `undo`, the versions are the entry's history. Everything
// here goes through `window.datum.layouts` — the store, which is CLI
// commands through the seam — or through the icons on the root bar, which
// act on the active root tab's layout (da-0a5o): `main`, on a fresh device.
//
// Where a test needs a modified layout it writes the draft the way the
// shell does: into `localStorage`, under the layout's name, as the layout
// the tab would save (`shellTree`: the shape `extract` produces, so a
// save reads back equal).

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { isDeepStrictEqual } from 'node:util';
import { inWorker, sharedApp, startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';
import {
	chooseRoot,
	chooser,
	manageRoot,
	menuLines,
	offers,
	rootMenuButton,
	titles
} from './support/layout.mjs';

/** A tree with every part of the contract in it, nested once. */
const tree = (name, kind) => ({
	version: 1,
	name,
	root: {
		id: 'root',
		active: 0,
		bar: { edge: 'left', align: 'center' },
		tabs: [
			{
				id: 'tab-1',
				title: 'mine',
				content: {
					id: 'split-1',
					direction: 'row',
					ratios: [1 / 3, 2 / 3],
					children: [
						{
							id: 'pane-1',
							active: 0,
							bar: { edge: 'top', align: 'start' },
							tabs: [
								{
									id: 'tab-2',
									content: { id: 'w-1', kind, settings: { argv: ['tail', '-n', '5'], note: 'a=b c' } }
								}
							]
						},
						{
							id: 'pane-2',
							active: 0,
							bar: { edge: 'bottom', align: 'end' },
							tabs: [{ id: 'tab-3', content: { id: 'w-2', kind: 'cli', settings: {} } }]
						}
					]
				}
			}
		]
	}
});

/** `window.datum.layouts[fn](...args)` in `app`'s page. */
const layouts = (app, fn, ...args) =>
	app.page.evaluate(([fn, args]) => window.datum.layouts[fn](...args), [fn, args]);

/** The same tree as the shell saves it: a root of one tab, `root`/`root-tab`, the bar at the top. */
const shellTree = (name, kind) => {
	const t = tree(name, kind);
	return { ...t, root: { ...t.root, bar: { edge: 'top', align: 'start' }, tabs: [{ ...t.root.tabs[0], id: 'root-tab' }] } };
};

/** The shell has read the save (and so `_internal.db` exists). */
// Loaded, and no act still in flight: the bar says so (`aria-busy`), since
// the acts that used to be disabled buttons are menu items now (da-wjob).
const controlsReady = (app) => app.page.locator('.root-bar[aria-busy="false"] [role="tab"]').first().waitFor();

const isModified = (app) =>
	app.page.locator('.root-bar').evaluate((el) => el.classList.contains('modified'));

const DRAFT = 'datum.layout.draft.v1.main';
const draft = (app) => app.page.evaluate((key) => localStorage.getItem(key), DRAFT);
const setDraft = (app, layout) =>
	app.page.evaluate(([key, value]) => localStorage.setItem(key, value), [DRAFT, JSON.stringify(layout)]);

/** Every argv that went to `_internal` — the controls re-reading the saves. */
const internal = (argv) => argv[0] === '--db' && argv[1] === '_internal';

before(startApp);
after(stopApp);

// ONE APP for the tests that stay on this page (da-6b3dj, sharedApp in
// support/app.mjs): `solo` deletes every layout the reader saved and puts
// `main` back. The rest open their own: they reload, open a second tab,
// refuse storage, hold the clock, or leave the root without `main` (Save
// layout as), which `solo` cannot undo.
const shared = sharedApp();

describe('the store', () => {
	test('a saved layout is listed, and loads back whole, after a reload', async () => {
		await withApp(async (app) => {
			const home = tree('home', 'daystrip');
			// A name that reads as a number is still a name.
			const phone = { ...tree('123', 'widgetbar'), designedFor: 'mobile' };
			await layouts(app, 'saveLayout', home);
			await layouts(app, 'saveLayout', phone);

			await app.reload();
			const listed = await layouts(app, 'listLayouts');
			assert.deepEqual(
				listed.map(({ name, designedFor }) => ({ name, designedFor })).sort((a, b) => a.name.localeCompare(b.name)),
				[
					{ name: '123', designedFor: 'mobile' },
					{ name: 'home', designedFor: undefined }
				]
			);
			assert.ok(listed.every((l) => !Number.isNaN(Date.parse(l.savedAt))), JSON.stringify(listed));
			assert.deepEqual(await layouts(app, 'loadLayout', 'home'), home);
			assert.deepEqual(await layouts(app, 'loadLayout', '123'), phone);
		});
	});

	test('two saves are two versions, either of which can be restored; save-as copies', async () => {
		await shared.solo(async (app) => {
			const first = tree('home', 'daystrip');
			const second = tree('home', 'nowview');
			await layouts(app, 'saveLayout', first);
			await layouts(app, 'saveLayout', second);

			assert.deepEqual(
				(await layouts(app, 'versions', 'home')).map((v) => v.version),
				[2, 1]
			);
			assert.deepEqual(await layouts(app, 'restore', 'home', 1), first);
			assert.deepEqual(await layouts(app, 'loadLayout', 'home'), first);
			// Forward: undo the restore, back to the newest save.
			assert.deepEqual(await layouts(app, 'restore', 'home'), second);
			assert.deepEqual(await layouts(app, 'loadLayout', 'home'), second);

			const copy = await layouts(app, 'saveAs', second, 'desk');
			assert.deepEqual(copy, { ...second, name: 'desk' });
			assert.deepEqual(await layouts(app, 'loadLayout', 'desk'), copy);
			assert.deepEqual(await layouts(app, 'loadLayout', 'home'), second);
			// A taken name is refused, and what holds it is untouched.
			await assert.rejects(layouts(app, 'saveAs', first, 'desk'));
			assert.deepEqual(await layouts(app, 'loadLayout', 'desk'), copy);

			await layouts(app, 'deleteLayout', 'desk');
			assert.deepEqual(
				(await layouts(app, 'listLayouts')).map((l) => l.name),
				['home']
			);
		});
	});
});

describe('the internal database', () => {
	test('persists through a reload and into a second tab, beside the user’s rows', async () => {
		await withApp(async (first) => {
			assert.equal((await first.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			const second = await first.newTab();
			await controlsReady(first);
			await controlsReady(second);
			const seen = await second.spyOnRunner();

			const home = tree('home', 'daystrip');
			await layouts(first, 'saveLayout', home);
			// The second tab is told, without a reload: its controls re-read.
			await until(seen, (argvs) => argvs.some(internal));
			assert.deepEqual(
				(await layouts(second, 'listLayouts')).map((l) => l.name),
				['home']
			);

			for (const tab of [first, second]) {
				await tab.reload();
				assert.deepEqual(await layouts(tab, 'loadLayout', 'home'), home);
				assert.deepEqual(
					(await tab.entries()).map((e) => e.field),
					['coffee']
				);
			}
		});
	});

	test('a write is measured per database, and a layout save re-runs no pane', async () => {
		await withApp(async (first) => {
			const second = await first.newTab();
			await controlsReady(first);
			await controlsReady(second);

			const saved = await first.run([
				'--db', '_internal', 'occur', 'layout', '--id', '%name',
				'-k', 'name="home"', '-k', 'version=1', '-k', 'root={}'
			]);
			assert.equal(saved.code, 0, saved.stderr);
			assert.deepEqual(saved.changed, ['_internal']);
			const read = await first.run(['tail']);
			assert.deepEqual(read.changed, []);

			const seen = await second.spyOnRunner();
			await layouts(first, 'saveLayout', tree('home', 'nowview'));
			await until(seen, (argvs) => argvs.some(internal));
			// Whatever this tab does on the save, it does on hearing it: once
			// that re-read has answered, a pane that was going to re-run has.
			await second.idle();
			assert.deepEqual(
				(await seen()).filter((argv) => !internal(argv)),
				[],
				'a layout save re-ran a pane on datum.db'
			);

			// The control: a write to datum.db is one the panes do hear.
			const logged = await first.run(['occur', 'coffee', '-t', '0700']);
			assert.deepEqual(logged.changed, ['datum']);
			await until(seen, (argvs) => argvs.some((argv) => argv[0] === 'tail'));
			// Pinned: a minute tick in the window re-runs the strip and the
			// nowview, which is not the save re-running them (da-6ehq).
		}, { clock: todayAt(12) });
	});

	// The fallback covers both files or neither: half the data durable is
	// worse than all of it in memory, because nobody can see which half.
	test('with storage refused, both fall to memory together and both come back', async () => {
		/** Refused until `until` (epoch ms, on the wall clock — `Date` is held), then real. */
		const notBefore = (until) => `
			const real = FileSystemFileHandle.prototype.createSyncAccessHandle;
			FileSystemFileHandle.prototype.createSyncAccessHandle = function (...args) {
				if (performance.timeOrigin + performance.now() < ${until}) return Promise.reject(new DOMException('not yet', 'InvalidStateError'));
				return real.apply(this, args);
			};
		`;
		// Five seconds, not eight. The worker goes back for the files 2 s
		// after it gave up and then doubles — so its tries land at 2, 6 and
		// 14 s, and an outage that ends AT the 8 s mark is a coin toss
		// between the 6 s try (too early) and the 14 s one. It came up tails
		// often enough to make this the slowest test in the suite at 17 s.
		// Ending it at 5 s puts it squarely between two tries, so the 6 s one
		// heals it — the same outage, read the same way, in half the time
		// (da-x6ea).
		const heals = Date.now() + 5000;
		await withApp(
			async (app) => {
				assert.match(await app.bannerText(), /\bmemory\b/);
				assert.ok(await app.page.locator('section.pane.alarm').count(), 'no alarm frame');

				const home = tree('home', 'daystrip');
				assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
				await layouts(app, 'saveLayout', home);
				const internalRead = await app.run(['--db', '_internal', 'tail', 'layout']);
				assert.equal(internalRead.storage, 'memory');

				await app.page.waitForFunction(() => !document.querySelector('section.pane.alarm'));
				await app.reload();
				assert.doesNotMatch(await app.bannerText(), /\bmemory\b/);
				assert.deepEqual(
					(await app.entries()).map((e) => e.field),
					['coffee']
				);
				assert.deepEqual(await layouts(app, 'loadLayout', 'home'), home);
			},
			{ prepare: inWorker(notBefore(heals)) }
		);
	});
});

describe('the draft', () => {
	test('survives a reload, and Save or Restore settles it', async () => {
		await withApp(async (app) => {
			await controlsReady(app);
			assert.equal(await isModified(app), false);
			assert.equal(await draft(app), null);

			const edited = shellTree('main', 'nowview');
			await setDraft(app, edited);
			await app.reload();
			await controlsReady(app);
			await app.reload();
			await controlsReady(app);
			assert.equal(await isModified(app), true, 'the draft did not survive a reload');

			await chooseRoot(app.page, 'Save layout');
			await until(() => isModified(app), (m) => m === false);
			assert.deepEqual(await layouts(app, 'loadLayout', 'main'), edited);
			assert.equal(await draft(app), null);

			await setDraft(app, shellTree('main', 'cli'));
			await app.reload();
			await controlsReady(app);
			assert.equal(await isModified(app), true);
			await chooseRoot(app.page, 'Restore layout');
			await until(() => isModified(app), (m) => m === false);
			assert.equal(await draft(app), null);

			// Unmodified against the save, and the save is still the first edit.
			await app.reload();
			await controlsReady(app);
			assert.equal(await isModified(app), false);
			assert.deepEqual(await layouts(app, 'loadLayout', 'main'), edited);
			assert.deepEqual(app.errors, []);
		});
	});

	test('with localStorage blocked there is no draft, and no error', async () => {
		await withApp(
			async (app) => {
				// Nothing remembers a visit here, so this is a first one
				// (da-u7vz8): on `welcome`, whose layout saves like any.
				await controlsReady(app);
				assert.equal(await isModified(app), false);
				await chooseRoot(app.page, 'Save layout');
				await controlsReady(app);
				assert.equal(await layouts(app, 'loadLayout', 'welcome').then((l) => l.name), 'welcome');
				assert.equal(await app.page.locator('.root-bar .error').count(), 0);
				assert.deepEqual(app.errors, []);
			},
			{
				prepare: (context) =>
					context.addInitScript(() => {
						Object.defineProperty(window, 'localStorage', {
							configurable: true,
							get() {
								throw new DOMException('storage is blocked', 'SecurityError');
							}
						});
					})
			}
		);
	});
});

describe('the controls', () => {
	test('an unmodified Restore lists the versions, and one of them can be picked', async () => {
		await shared.solo(async (app) => {
			await controlsReady(app);
			const first = shellTree('main', 'daystrip');
			await layouts(app, 'saveLayout', first);
			await layouts(app, 'saveLayout', shellTree('main', 'nowview'));

			await chooseRoot(app.page, 'Restore layout');
			const items = app.page.locator('.root-bar .versions button');
			await items.first().waitFor();
			assert.deepEqual(
				(await items.allInnerTexts()).map((t) => t.split(' ·')[0]),
				['→ forward', 'v2', 'v1']
			);
			await items.filter({ hasText: /^v1 / }).click();
			await until(() => layouts(app, 'loadLayout', 'main'), (l) => isDeepStrictEqual(l, first));
			assert.equal(await isModified(app), false);
			// And the tab shows it: the strip alone, titled by the draft's tab.
			await until(
				() => app.page.evaluate(() => [...document.querySelectorAll('section.pane')].map((p) => p.dataset.title)),
				(t) => isDeepStrictEqual(t, ['dayview', 'cli'])
			);
		});
	});

	test('Save as makes a new layout and shows it as the tab', async () => {
		await withApp(async (app) => {
			await controlsReady(app);
			const edited = shellTree('main', 'nowview');
			await setDraft(app, edited);
			await app.reload();
			await controlsReady(app);

			app.page.once('dialog', (dialog) => dialog.accept('phone'));
			await chooseRoot(app.page, 'Save layout as');
			await until(() => isModified(app), (m) => m === false);
			assert.deepEqual(await layouts(app, 'loadLayout', 'phone'), { ...edited, name: 'phone' });
			assert.deepEqual(
				await app.page.locator('.root-bar [role="tab"]').allInnerTexts(),
				['phone']
			);
			assert.deepEqual(
				JSON.parse(await app.page.evaluate(() => localStorage.getItem('datum.layout.root.v1'))),
				{ names: ['phone'], active: 0 }
			);
			// The original was never saved, and keeps no draft of the edit.
			assert.equal(await draft(app), null);
			await assert.rejects(layouts(app, 'loadLayout', 'main'));
		});
	});
});

// `Reset layout` (da-luwe): back to the tree that SHIPPED under this
// name, as a DRAFT and nothing else.
//
// vividn, 16:29: "when reset is clicked it should restore the default
// settings, but not actually save them to the db, so that the user can
// restore to their other layout, then upon save it would sync." And 18:15:
// "Definitely don't want to delete *all* layouts, rather just reset the
// built-in layouts that ship with datum ... probably it's better just to
// have a single Reset Layout, rather than do batch. That would only appear
// in preshipped layouts, not in user's custom layouts."
//
// So this is da-pp1a's «Reset all layouts…» replaced by one act on one
// layout: no Confirm (nothing is written, and Restore beside it is the
// undo), no `del`, and no key cleared on the device.
//
// The built-in is captured the way the app itself writes a tree down: a
// fresh device's `Save layout`, read back through the store. So nothing
// about `defaults.ts` is copied into this file to go stale, and the tree
// a reset lands is compared against the app's own record of what ships.
describe('Reset layout', () => {
	/** This device's tree, as the shell writes it down: save it, then read the save. */
	async function treeOf(app) {
		await chooseRoot(app.page, 'Save layout');
		return until(() => layouts(app, 'loadLayout', 'main').catch(() => null), (l) => l !== null);
	}

	/**
	 * The tree the `main` tab is HOLDING, without writing anything: the
	 * copy, through `extract` — the same form a save is stored in, which is
	 * what lets it be compared with one. A test that saved to read would
	 * destroy the fact it is checking, since the point of a reset is that
	 * the save does not move.
	 */
	const held = (app) =>
		app.page.evaluate(() => {
			const { extract, layoutTabs } = window.datum.shell;
			return extract(layoutTabs(window.datum.layout.current()).find((t) => t.layout === 'main'));
		});

	const paneCount = (page) => page.locator('section.pane').count();

	/** Reset the active layout and wait for the shipped tree to land. */
	async function reset(app, panes) {
		await chooseRoot(app.page, 'Reset layout');
		await until(() => paneCount(app.page), (n) => n === panes);
	}

	test('the shipped tree comes back as a draft: nothing is written, and Restore is the way back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await controlsReady(app);
			const builtin = await treeOf(app);
			const panes = await paneCount(page);

			// Modified and SAVED: the save now shadows `defaults.ts`, which is
			// the state a reset exists to undo.
			await manageRoot(page, 'split: a new pane to the left');
			await until(() => paneCount(page), (n) => n === panes + 1);
			const edited = await treeOf(app);
			assert.notDeepEqual(edited.root, builtin.root);

			// It survives a reload — so what draws it is the save, not a draft.
			await app.reload();
			await controlsReady(app);
			assert.equal(await paneCount(page), panes + 1);

			await reset(app, panes);

			// What shipped is what is on screen...
			assert.deepEqual((await held(app)).root, builtin.root);
			// ...and the SAVE is untouched: the reset only wrote the draft.
			assert.deepEqual((await layouts(app, 'loadLayout', 'main')).root, edited.root);
			await until(() => draft(app), (d) => d !== null);
			// So the two differ, and the dot says so.
			await until(() => isModified(app), (m) => m === true);

			// Restore, the act above it, puts the save back (vividn, 16:29:
			// "so that the user can restore to their other layout").
			await chooseRoot(page, 'Restore layout');
			await until(() => paneCount(page), (n) => n === panes + 1);
			assert.deepEqual((await held(app)).root, edited.root);
			await until(() => isModified(app), (m) => m === false);

			// And reset then SAVED is how the shipped tree becomes the save —
			// "upon save it would sync", as any save does.
			await reset(app, panes);
			await chooseRoot(page, 'Save layout');
			await until(
				() => layouts(app, 'loadLayout', 'main').then((l) => l.root),
				(root) => isDeepStrictEqual(root, builtin.root)
			);
			await until(() => isModified(app), (m) => m === false);
			assert.deepEqual(app.errors, []);
		});
	});

	test('with no save to shadow it, a reset just discards this device’s draft', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await controlsReady(app);
			const panes = await paneCount(page);

			await manageRoot(page, 'split: a new pane to the left');
			await until(() => paneCount(page), (n) => n === panes + 1);
			await until(() => draft(app), (d) => d !== null);

			await reset(app, panes);

			// Nothing was ever saved under `main`, so there is nothing to
			// differ from: the draft goes and the dot clears.
			await until(() => draft(app), (d) => d === null);
			await until(() => isModified(app), (m) => m === false);
			await assert.rejects(layouts(app, 'loadLayout', 'main'));
			assert.deepEqual(app.errors, []);
		});
	});

	test('a layout the reader saved has no Reset layout — only a built-in does', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await controlsReady(app);
			// `main`, which ships, has it.
			await rootMenuButton(page).click();
			assert.ok((await menuLines(page)).includes('Reset layout'));
			await page.keyboard.press('Escape');

			// `mine`, which the reader just made, has nothing that shipped to
			// go back to — so it has the other four acts and not this one.
			page.once('dialog', (dialog) => dialog.accept('mine'));
			await chooseRoot(page, 'Save layout as');
			await page.locator('.root-bar [role="tab"]', { hasText: 'mine' }).waitFor();
			await rootMenuButton(page).click();
			const lines = await menuLines(page);
			assert.ok(!lines.includes('Reset layout'), lines.join(' | '));
			for (const act of ['Save layout', 'Restore layout', 'Save layout as'])
				assert.ok(lines.includes(act), `${act} is missing: ${lines.join(' | ')}`);
			await page.keyboard.press('Escape');
			await page.locator('[role="menu"]').waitFor({ state: 'detached' });
			assert.deepEqual(app.errors, []);
		});
	});

	test('on a phone the shipped tree is the MOBILE one', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await controlsReady(app);
			const builtin = await treeOf(app);
			const panes = await paneCount(page);

			await manageRoot(page, 'split: a new pane to the left');
			await until(() => paneCount(page), (n) => n === panes + 1);

			await reset(app, panes);

			assert.deepEqual((await held(app)).root, builtin.root);
			// The phone's own tree (`defaults.ts`), not the desktop's six
			// panes: the strip, the CLI widget, one pane of tabs and the fields.
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'tail', 'fields']);
			assert.deepEqual(app.errors, []);
		}, { viewport: { width: 390, height: 640 } });
	});
});
