// Layouts saved as datum entries in the `_internal` database (da-wpgy).
//
// A saved layout is an entry of field `layout` in a second datum database,
// which the worker keeps on OPFS beside the user's: saving is `update`,
// restoring is `undo`, the versions are the entry's history. Everything
// here goes through `window.datum.layouts` — the store, which is CLI
// commands through the seam — or through the icons on the root bar, which
// act on the active root tab's layout (da-0a5o): `day`, on a fresh device.
//
// Where a test needs a modified layout it writes the draft the way the
// shell does: into `localStorage`, under the layout's name, as the layout
// the tab would save (`shellTree`: the shape `extract` produces, so a
// save reads back equal).

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { isDeepStrictEqual } from 'node:util';
import { inWorker, startApp, stopApp, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';
import { chooseRoot, chooser, offers } from './support/layout.mjs';

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
							tabs: [{ id: 'tab-3', content: { id: 'w-2', kind: 'repl', settings: {} } }]
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

const DRAFT = 'datum.layout.draft.v1.day';
const draft = (app) => app.page.evaluate((key) => localStorage.getItem(key), DRAFT);
const setDraft = (app, layout) =>
	app.page.evaluate(([key, value]) => localStorage.setItem(key, value), [DRAFT, JSON.stringify(layout)]);

/** Every argv that went to `_internal` — the controls re-reading the saves. */
const internal = (argv) => argv[0] === '--db' && argv[1] === '_internal';

before(startApp);
after(stopApp);

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
		await withApp(async (app) => {
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
		});
	});

	// The fallback covers both files or neither: half the data durable is
	// worse than all of it in memory, because nobody can see which half.
	test('with storage refused, both fall to memory together and both come back', async () => {
		/** A sync access handle refused until `until` (epoch ms), then real. */
		const notBefore = (until) => `
			const real = FileSystemFileHandle.prototype.createSyncAccessHandle;
			FileSystemFileHandle.prototype.createSyncAccessHandle = function (...args) {
				if (Date.now() < ${until}) return Promise.reject(new DOMException('not yet', 'InvalidStateError'));
				return real.apply(this, args);
			};
		`;
		const heals = Date.now() + 8000;
		await withApp(
			async (app) => {
				assert.match(await app.bannerText(), /held in memory only/);
				assert.ok(await app.page.locator('section.pane.alarm').count(), 'no alarm frame');

				const home = tree('home', 'daystrip');
				assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
				await layouts(app, 'saveLayout', home);
				const internalRead = await app.run(['--db', '_internal', 'tail', 'layout']);
				assert.equal(internalRead.storage, 'memory');

				await app.page.waitForFunction(() => !document.querySelector('section.pane.alarm'));
				await app.reload();
				assert.doesNotMatch(await app.bannerText(), /memory only/);
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

			const edited = shellTree('day', 'nowview');
			await setDraft(app, edited);
			await app.reload();
			await controlsReady(app);
			await app.reload();
			await controlsReady(app);
			assert.equal(await isModified(app), true, 'the draft did not survive a reload');

			await chooseRoot(app.page, 'Save layout');
			await until(() => isModified(app), (m) => m === false);
			assert.deepEqual(await layouts(app, 'loadLayout', 'day'), edited);
			assert.equal(await draft(app), null);

			await setDraft(app, shellTree('day', 'repl'));
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
			assert.deepEqual(await layouts(app, 'loadLayout', 'day'), edited);
			assert.deepEqual(app.errors, []);
		});
	});

	test('with localStorage blocked there is no draft, and no error', async () => {
		await withApp(
			async (app) => {
				await controlsReady(app);
				assert.equal(await isModified(app), false);
				await chooseRoot(app.page, 'Save layout');
				await controlsReady(app);
				assert.equal(await layouts(app, 'loadLayout', 'day').then((l) => l.name), 'day');
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
		await withApp(async (app) => {
			await controlsReady(app);
			const first = shellTree('day', 'daystrip');
			await layouts(app, 'saveLayout', first);
			await layouts(app, 'saveLayout', shellTree('day', 'nowview'));

			await chooseRoot(app.page, 'Restore layout');
			const items = app.page.locator('.root-bar .versions button');
			await items.first().waitFor();
			assert.deepEqual(
				(await items.allInnerTexts()).map((t) => t.split(' ·')[0]),
				['→ forward', 'v2', 'v1']
			);
			await items.filter({ hasText: /^v1 / }).click();
			await until(() => layouts(app, 'loadLayout', 'day'), (l) => isDeepStrictEqual(l, first));
			assert.equal(await isModified(app), false);
			// And the tab shows it: the strip alone, titled by the draft's tab.
			await until(
				() => app.page.evaluate(() => [...document.querySelectorAll('section.pane')].map((p) => p.dataset.title)),
				(t) => isDeepStrictEqual(t, ['dayview', 'repl'])
			);
		});
	});

	test('Save as makes a new layout and shows it as the tab', async () => {
		await withApp(async (app) => {
			await controlsReady(app);
			const edited = shellTree('day', 'nowview');
			await setDraft(app, edited);
			await app.reload();
			await controlsReady(app);

			app.page.once('dialog', (dialog) => dialog.accept('phone'));
			await chooseRoot(app.page, 'Save layout as…');
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
			await assert.rejects(layouts(app, 'loadLayout', 'day'));
		});
	});
});

// The reset (da-pp1a). vividn, from the train: "I've started modifying the
// default layout... let's add a Reset all layouts, that way I can always
// reset as we're modifying the defaults."
//
// A saved `day` shadows `defaults.ts` for good — `resolve` reads the save
// first — and so does an unsaved DRAFT of it, which is what a tab is built
// from. Both are what the reset undoes, and what has to come back is the
// BUILT-IN tree, deep-equal and not by eye.
//
// The built-in is captured the way the app itself writes a tree down: a
// fresh device's `Save layout`, read back through the store. The same read
// after the reset is the comparison — so nothing about `defaults.ts` is
// copied into this file to go stale.
describe('the reset', () => {
	/** Open the gear and choose `Reset all layouts…`; the Confirm it opens. */
	async function openReset(page) {
		await page.locator('.root-bar button.gear').click();
		await page.getByRole('menuitem', { name: 'Reset all layouts…', exact: true }).click();
		const dialog = page.locator('dialog.confirm');
		await dialog.waitFor();
		return dialog;
	}

	/** The whole gesture: the gear, the item, `Reset`, and the reload it answers with. */
	async function reset(app) {
		const page = app.page;
		const dialog = await openReset(page);
		await page.evaluate(() => (window.__beforeReset = true));
		await dialog.locator('button', { hasText: 'Reset' }).click();
		// `waitForFunction` re-runs on the new document, so this is the
		// moment the shell's own `location.reload()` has landed.
		await page.waitForFunction(() => window.__beforeReset === undefined);
		await app.reload();
		await controlsReady(app);
	}

	/** This device's tree, as the shell writes it down: save it, then read the save. */
	async function treeOf(app) {
		await chooseRoot(app.page, 'Save layout');
		return until(() => layouts(app, 'loadLayout', 'day').catch(() => null), (l) => l !== null);
	}

	const paneCount = (page) => page.locator('section.pane').count();

	/** The layout keys this device is holding — drafts, locks, the open list. */
	const kept = (page) =>
		page.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith('datum.layout.')).sort());

	/** What the root bar's `+` offers, trimmed, with the chooser dismissed again. */
	async function chooserOffers(page) {
		await page.locator('.root-bar button[aria-label="open a layout"]').click();
		await chooser(page).waitFor();
		const listed = (await offers(page)).map((o) => o.trim());
		await page.keyboard.press('Escape');
		await chooser(page).waitFor({ state: 'detached' });
		return listed;
	}

	test('a saved `day` is deleted and the built-in tree comes back', async () => {
		await withApp(async (app) => {
			await controlsReady(app);
			const builtin = await treeOf(app);
			const panes = await paneCount(app.page);

			// Modified and SAVED: the save now shadows `defaults.ts`, which is
			// the state a reset exists to undo.
			await chooseRoot(app.page, 'break out: a new pane to the left');
			await until(() => paneCount(app.page), (n) => n === panes + 1);
			const edited = await treeOf(app);
			assert.notDeepEqual(edited.root, builtin.root);

			// It survives a reload — so what draws it is the save, not a draft.
			await app.reload();
			await controlsReady(app);
			assert.equal(await paneCount(app.page), panes + 1);

			await reset(app);

			assert.deepEqual(await layouts(app, 'listLayouts'), []);
			await assert.rejects(layouts(app, 'loadLayout', 'day'));
			assert.equal(await paneCount(app.page), panes);
			assert.deepEqual((await treeOf(app)).root, builtin.root);
		});
	});

	// The other half of the shadow, and the one vividn will actually be
	// holding while he edits a default: an edit he has NOT saved.
	test('an unsaved edit goes too, and the device keeps no layout keys', async () => {
		await withApp(async (app) => {
			await controlsReady(app);
			const builtin = await treeOf(app);
			const panes = await paneCount(app.page);

			await chooseRoot(app.page, 'break out: a new pane to the left');
			await until(() => paneCount(app.page), (n) => n === panes + 1);
			await until(() => draft(app), (d) => d !== null);
			assert.equal(await isModified(app), true);

			await reset(app);

			assert.deepEqual(await kept(app.page), []);
			assert.equal(await paneCount(app.page), panes);
			assert.deepEqual((await treeOf(app)).root, builtin.root);
		});
	});

	test('every saved layout goes — a phone one too — and the user’s own rows do not', async () => {
		await withApp(async (app) => {
			await controlsReady(app);
			assert.equal((await app.run(['occur', 'coffee', '-t', '0700'])).code, 0);
			assert.equal((await app.run(['occur', 'tea', '-t', '0800'])).code, 0);
			const mine = await app.entries();
			assert.deepEqual(mine.map((e) => e.field), ['tea', 'coffee']);

			await layouts(app, 'saveLayout', shellTree('mine', 'nowview'));
			await layouts(app, 'saveLayout', { ...shellTree('pocket', 'tail'), designedFor: 'mobile' });
			await until(() => layouts(app, 'listLayouts'), (l) => l.length === 2);
			assert.ok((await chooserOffers(app.page)).includes('mine'), 'the chooser never listed the save');

			await reset(app);

			// «All» means every saved layout, the phone ones included, and
			// what `del` leaves behind is not live: the store's own
			// `tail layout --json` finds nothing.
			assert.deepEqual(await layouts(app, 'listLayouts'), []);
			// And the chooser, which lists the saves beside the built-ins,
			// offers the built-ins alone.
			const listed = await chooserOffers(app.page);
			assert.ok(!listed.includes('mine') && !listed.includes('pocket'), listed.join(', '));

			// The MAIN database was never touched.
			assert.deepEqual(await app.entries(), mine);
		});
	});

	test('cancel deletes nothing', async () => {
		await withApp(async (app) => {
			await controlsReady(app);
			await layouts(app, 'saveLayout', shellTree('mine', 'nowview'));
			await until(() => layouts(app, 'listLayouts'), (l) => l.length === 1);
			const before = await layouts(app, 'listLayouts');
			const keys = await kept(app.page);
			const panes = await paneCount(app.page);

			const dialog = await openReset(app.page);
			await dialog.locator('button', { hasText: 'cancel' }).click();
			await dialog.waitFor({ state: 'detached' });

			assert.deepEqual(await layouts(app, 'listLayouts'), before);
			assert.deepEqual(await layouts(app, 'loadLayout', 'mine'), shellTree('mine', 'nowview'));
			assert.deepEqual(await kept(app.page), keys);
			assert.equal(await paneCount(app.page), panes);
		});
	});
});
