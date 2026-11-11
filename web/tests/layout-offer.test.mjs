// A new default, tab by tab (da-cr0w2, da-3eegv).
//
// vividn, 2026-10-08: «Do we have a way to trigger a prompt to reset to
// default views? … That way users can decide». And 2026-11-04: «tabs that
// haven't been changed get the new layout automatically / new default tabs
// that get added get added for everyone / tabs that have been closed stay
// closed / tabs that have been changed prompt about the changes for their
// particular tab / if there are multiple tbas that have potential new changes
// one of th ebutton son the popup could be upddate all tabs».
//
// So a device that started on an earlier default is asked only about a tab
// it saved or drafted that the new default touches, with that tab's own
// sentences, and each answer is remembered. A device that has never been here
// starts on the newest and is never asked.
//
// The suite ships its OWN new defaults (`window.__datumNewDefaults`, as
// support/desk.mjs pins a tree): datum's list is whatever it is today, and
// «a newer default than the one this device started on» has to be true
// whatever that is.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, titles } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const UPDATE = 'Update';
const ALL = 'Update all tabs';
const KEEP = 'Keep mine';
const ON_COLLECT = 'collect moved its day strip.';
const ON_WELCOME = 'welcome says more.';

/** Where a device keeps how many new defaults it has seen, and the shipped tabs it was given (layout/draft.ts). */
const SEEN = 'datum.layout.defaults.v1';
const GIVEN = 'datum.layout.given.v1';

/** collect's panes as datum ships it on a desktop. */
const COLLECT = ['dayview', 'tail', 'collect', 'nowview', 'stats'];

/**
 * The tabs a first visit opens, by how many new defaults datum had shipped
 * when it opened them.
 *
 * ⚑ WHEN THE LAST TEST FAILS the default moved. Append an entry saying what
 * changed, and the tabs it changed, to `NEW_DEFAULTS` (layout/defaults.ts) —
 * that is what brings it to everybody already here — and its tabs to this list.
 */
const FIRST_VISIT = [
	['main'],
	['welcome', 'setup', 'collect'],
	['welcome', 'setup', 'collect'],
	['welcome', 'setup', 'collect'],
	['welcome', 'setup', 'collect'],
	['welcome', 'setup', 'collect', 'dayview'],
	['welcome', 'setup', 'collect', 'dayview'],
	['welcome', 'setup', 'collect', 'dayview'],
	['welcome', 'setup', 'collect', 'dayview'],
	['welcome', 'premade', 'setup', 'collect', 'dayview'],
	['welcome', 'setup', 'collect', 'dayview']
];

/** A layout of a single stats pane: the reader's own version of `name`. */
const theirs = (name) => ({
	version: 1,
	name,
	root: {
		id: `mine-${name}`,
		tabs: [{ id: `mine-${name}-tab`, content: { id: `mine-${name}-w`, kind: 'stats', settings: {} } }],
		active: 0,
		bar: { edge: 'top', align: 'start' }
	}
});

/**
 * A device that has been here before `news` shipped and seen none of them:
 * its open `tabs`, the shipped tabs it was `given` (none: a device from before
 * the record), and its `drafts` of shipped layouts.
 */
const here = ({ news, tabs, given = null, drafts = [] }) => ({
	...DESKTOP,
	home: null,
	desk: null,
	prepare: (context) =>
		context.addInitScript(
			({ news, tabs, given, drafts, SEEN, GIVEN }) => {
				window.__datumNewDefaults = news;
				if (localStorage.getItem(SEEN) !== null) return;
				localStorage.setItem(SEEN, '0');
				localStorage.setItem('datum.layout.root.v1', JSON.stringify({ names: tabs, active: 0 }));
				if (given) localStorage.setItem(GIVEN, JSON.stringify(given));
				for (const draft of drafts) localStorage.setItem(`datum.layout.draft.v1.${draft.name}`, JSON.stringify(draft));
			},
			{ news, tabs, given, drafts, SEEN, GIVEN }
		)
});

const rootNames = (page) =>
	page.evaluate(() => window.datum.shell.rootTabs(window.datum.layout.current()).map((t) => t.layout));

const seen = (page) => page.evaluate((key) => localStorage.getItem(key), SEEN);

const saved = (page) =>
	page.evaluate(async () => (await window.datum.layouts.listLayouts()).filter((l) => l.source === 'user').map((l) => l.name));

const shows = async (page, name, panes) => {
	await page.evaluate((name) => window.datum.shell.open(name), name);
	await until(() => titles(page), (t) => t.join() === panes.join());
};

/**
 * The offer, if this load made one. The app is `ready`, so the boot is over
 * — and the boot is what asks, in the same turn it finishes in — so no
 * dialog now is no dialog at all.
 */
const offer = (page) => page.locator('dialog.confirm');
const answer = (page, name) => offer(page).getByRole('button', { name, exact: true }).click();

describe('a new default, tab by tab', () => {
	test('an unchanged tab takes it unasked, a new shipped tab is added at its place, a closed one stays closed, and the order is kept', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await until(() => rootNames(page), (names) => names.join() === 'collect,dayview,welcome');
			assert.equal(await offer(page).count(), 0);
			assert.deepEqual(await titles(page), COLLECT);
			assert.equal(await seen(page), '1');

			await app.reload();
			assert.equal(await offer(page).count(), 0);
			assert.deepEqual(await rootNames(page), ['collect', 'dayview', 'welcome']);
		}, here({ news: [{ tabs: ['collect', 'dayview'], text: ON_COLLECT }], tabs: ['collect', 'welcome'], given: ['welcome', 'setup', 'collect'] }));
	});

	test('a device with no record keeps a closed tab closed', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal(await offer(page).count(), 0);
			assert.deepEqual(await rootNames(page), ['welcome', 'collect']);
			assert.equal(await seen(page), '1');
		}, here({ news: [{ tabs: ['setup', 'dayview'], text: ON_COLLECT }], tabs: ['welcome', 'collect'] }));
	});

	test('a changed tab is asked about with its own sentence; «Update» gives it the new layout and keeps theirs as «collect (mine)»', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const text = await offer(page).locator('p').innerText();
			assert.ok(text.includes(ON_COLLECT) && !text.includes(ON_WELCOME), text);
			assert.ok(text.includes('collect (mine)'), text);
			assert.deepEqual(await offer(page).getByRole('button').allInnerTexts(), [UPDATE, KEEP]);
			await answer(page, UPDATE);
			// The tabs a device that changed collect had never been given are added too.
			await until(() => rootNames(page), (names) => names.join() === 'welcome,collect,dayview');
			await shows(page, 'collect', COLLECT);
			assert.ok((await saved(page)).includes('collect (mine)'));
			await shows(page, 'collect (mine)', ['stats']);

			await app.reload();
			assert.equal(await offer(page).count(), 0);
		}, here({
			news: [{ tabs: ['collect'], text: ON_COLLECT }, { tabs: ['welcome'], text: ON_WELCOME }],
			tabs: ['welcome', 'collect'],
			given: ['welcome', 'setup', 'collect'],
			drafts: [theirs('collect')]
		}));
	});

	for (const [how, keep] of [
		[KEEP, (page) => answer(page, KEEP)],
		['Escape', (page) => page.keyboard.press('Escape')]
	])
		test(`${how} keeps a collect it saved, and is not asked again`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				// Unchanged at first, so taken unasked; then saved, and the default shipped again.
				await until(() => seen(page), (n) => n === '1');
				await page.evaluate((l) => window.datum.layouts.saveLayout(l), theirs('collect'));
				await page.evaluate((key) => localStorage.setItem(key, '0'), SEEN);
				await app.reload();
				await offer(page).waitFor();
				await keep(page);
				await offer(page).waitFor({ state: 'detached' });
				assert.deepEqual(await rootNames(page), ['collect']);
				await until(() => titles(page), (t) => t.join() === 'stats');

				await app.reload();
				assert.equal(await offer(page).count(), 0);
				await until(() => titles(page), (t) => t.join() === 'stats');
			}, here({ news: [{ tabs: ['collect'], text: ON_COLLECT }], tabs: ['collect'], given: FIRST_VISIT.at(-1) }));
		});

	test('three changed tabs offer «Update all tabs», which updates every one and asks nothing more', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const text = await offer(page).locator('p').innerText();
			assert.ok(text.includes('setup'), text);
			assert.deepEqual(await offer(page).getByRole('button').allInnerTexts(), [UPDATE, ALL, KEEP]);
			await answer(page, ALL);
			// Written once every tab is rebuilt.
			await until(() => seen(page), (n) => n === '1');
			assert.equal(await offer(page).count(), 0);
			const mine = await saved(page);
			for (const name of ['setup', 'collect', 'dayview']) assert.ok(mine.includes(`${name} (mine)`), mine.join());
			await shows(page, 'collect', COLLECT);
			await page.evaluate(() => window.datum.shell.open('setup'));
			await until(() => titles(page), (t) => t.length > 0 && t.join() !== 'stats');
		}, here({
			news: [{ tabs: ['setup', 'collect', 'dayview'], text: ON_COLLECT }],
			tabs: ['setup', 'collect', 'dayview'],
			given: FIRST_VISIT.at(-1),
			drafts: [theirs('setup'), theirs('collect'), theirs('dayview')]
		}));
	});

	test('never to a device that starts on it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal(await offer(page).count(), 0);
			assert.equal(await seen(page), '2');
			assert.deepEqual(JSON.parse(await page.evaluate((key) => localStorage.getItem(key), GIVEN)), FIRST_VISIT.at(-1));

			await app.reload();
			assert.equal(await offer(page).count(), 0);
		}, {
			...DESKTOP,
			home: null,
			prepare: (context) =>
				context.addInitScript(
					(news) => (window.__datumNewDefaults = news),
					[{ tabs: ['collect'], text: ON_COLLECT }, { tabs: ['welcome'], text: ON_WELCOME }]
				)
		});
	});
});

// A first visit is marked with how many new defaults datum had shipped, so
// this is read against datum's OWN list — nothing shipped by the suite.
test('the tabs a first visit opens are the ones the last new default was announced with', async () => {
	await withApp(async (app) => {
		assert.deepEqual(await rootNames(app.page), FIRST_VISIT[Number(await seen(app.page))]);
	}, { ...DESKTOP, home: null });
});
