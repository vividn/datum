// The shell (da-0a5o): the root tab bar in place of the header, layouts as
// root tabs that a URL can open, the four acts in the active tab's own
// menu (da-wjob — the icon row that held them is gone), and the fixed
// bottom bar with the brand, the prompt and the CLI's receipt.
//
// Driven through the built app in a real browser. Every assertion names
// what is on screen — tab labels, the receipt's text, a hid from the
// listing — never "more tabs than before". Where the shell talks to the
// store, the store is read back through `window.datum.layouts`, which is
// CLI commands through the seam.
//
// ONE APP behind most of this file (da-xamd). A browser context is 2.5 s of
// wasm compile, database and pane commands, and this file used to open
// thirty-one of them. What a shared app costs is shared state, and for the
// shell that state is the LAYOUT: what a test splits, saves and locks is
// what the next test opens onto. So a test that mutates takes a ROOM —
// `app.room('acts')` is `main`'s tree as a preview tab under a name of its
// own, written nowhere — and `app.solo()` puts the world back to `main`
// alone before each one. A room's panes carry the room's name (`inRoom`),
// because a second copy of one layout has its ids re-minted (shell.ts).
//
// What still opens an app of its own, and why:
//
//   - a test about ANOTHER DEVICE, which is another browser context by
//     definition — the second half of the `+` test and of the lock test;
//   - the phone, the memory-only storage prelude and the fallback-only
//     server, each of which is a different browser.
//
// The tests about `main` itself being in front are `solo`'s now, which puts
// a modified `main` back (`restoreMain`) — the ones that save `main` and
// reload onto it (the cycle guard, the lock) included, since a reload of
// `main` is not a room's to survive — and the bar tests that LOG share too:
// `solo` and `empty` start each on an empty database (da-6b3dj). A second
// TAB of the same device is `app.newTab()` on the shared context, closed
// before the test ends so nothing it runs outlives it (da-iwikb).

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, devices } from 'playwright';
import { inWorker, sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';
import {
	chooseRoot,
	chooser,
	enterLayoutMode,
	fill,
	leaveLayoutMode,
	offers,
	openChooser,
	pickFrom,
	rootMenuButton,
	settled,
	titledMenuButton,
	titledPane,
	withBar
} from './support/layout.mjs';

before(startApp);
after(stopApp);

const DESKTOP = { viewport: { width: 1280, height: 900 } };

/** The one desktop app most of this file shares (da-xamd). */
const shared = sharedApp(DESKTOP);

/**
 * The same desktop on the one-pane repl layout.
 *
 * For the tests about the BOTTOM BAR alone: the bar is drawn by the app's
 * own layout above whatever tree is in front (`+layout.svelte`), so it is
 * the same bar on every page, and the day frame behind it costs a
 * fortnight of `dayview` bands on every open — better than a second a test
 * (da-x6ea). The bar tests that DO reach into the frame — the widget-bar
 * press, the orange alarm — keep the day page.
 */
// The CLI beside a static pane, not the CLI alone, which draws no output
// section (da-mwp4m).
const BAR_ONLY = { ...DESKTOP, prepare: withBar };

/**
 * The one repl-page app the bar tests share, `empty`'d before each: the
 * PILLS, which a test that leaves one behind would hand to the next, and
 * the rows a test logged. Not `shared.solo`, which puts the root bar back
 * to `main` alone and would take the cheap page out from under them.
 */
const barShared = sharedApp(BAR_ONLY);
const onBar = async (fn) => fn(await (await barShared()).empty());

/** The root bar's tab labels, in order, the modified dot stripped. */
const rootTabs = (page) =>
	page
		.locator('.root-bar [role="tab"]')
		.evaluateAll((all) => all.map((b) => b.textContent.replace('•', '').trim()));

const activeRootTab = (page) =>
	page.locator('.root-bar [role="tab"][aria-selected="true"]').evaluate((b) => b.textContent.replace('•', '').trim());

/** Which root tabs carry the modified dot. */
const dotted = (page) =>
	page.locator('.root-bar [role="tab"].modified').evaluateAll((all) => all.map((b) => b.textContent.replace('•', '').trim()));

/** The widget panes' titles, in DOM order. */
const titles = (page) =>
	page.evaluate(() => [...document.querySelectorAll('section.pane')].map((p) => p.dataset.title));

/** A menu item's whole label — a checked one carries a ✓ before it. */
const exactly = (label) =>
	new RegExp(`^\\s*✓?\\s*${label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*$`);

/** Open the root bar's `+` — the one chooser (da-uyx1). */
const add = (page) => page.locator('.root-bar button[aria-label="open a layout"]');

/** Open the `+` and pick `name`. */
async function pick(page, name) {
	await add(page).click();
	await chooser(page).locator('button').filter({ hasText: exactly(name) }).click();
	await chooser(page).waitFor({ state: 'detached' });
}

/** What the `+` offers, in order. */
async function offered(page) {
	await add(page).click();
	await chooser(page).waitFor();
	const items = await offers(page);
	await page.keyboard.press('Escape');
	await chooser(page).waitFor({ state: 'detached' });
	return items.map((t) => t.trim());
}

/**
 * Open a pane's menu — its title — and choose an item by its exact label.
 *
 * By TITLE, not by the id `main` gives the pane: inside a room the same
 * pane is `tail@<room>` (shell.ts re-mints a second copy's ids) and its
 * title is `tail` in both.
 */
const choose = (page, title, label) => pickFrom(page, titledMenuButton(page, title), label);

/**
 * The same in layout mode (da-vvcxc): Split, Break out, Rename tab, Close.
 * By TITLE, like `choose` above, because a room's pane ids are re-minted
 * and its titles are not.
 */
async function manage(page, title, label) {
	await enterLayoutMode(page);
	await choose(page, title, label);
}

/** In layout mode, add a tab to a titled pane: the `+`, then `name` in the chooser it opens. */
async function addTab(page, title, name) {
	await enterLayoutMode(page);
	await titledPane(page, title).locator('> .titlebar button.add').first().click();
	await chooser(page).waitFor();
	await chooser(page).locator('button').filter({ hasText: exactly(name) }).click();
	await chooser(page).waitFor({ state: 'detached' });
}

/** Bring a root tab to the front by its label, and wait for it — out of layout mode, accepting. */
async function front(page, label) {
	await leaveLayoutMode(page);
	await page.locator('.root-bar [role="tab"]', { hasText: exactly(label) }).click();
	await until(() => activeRootTab(page), (t) => t === label);
}

/** The acts, as the root menu names them (da-wjob). */
const ACT = { save: 'Save layout', restore: 'Restore layout', 'save-as': 'Save layout as' };

/** Run one of the layout's acts from the root menu. */
const act = (page, name) => chooseRoot(page, ACT[name]);

/** The root menu's items in normal mode, open: label, whether it is marked, and its colour. */
async function rootItems(page) {
	await leaveLayoutMode(page);
	return openItems(page, () => rootMenuButton(page).click());
}

/** The root's menu in layout mode (da-vvcxc): what is DONE to the page. */
async function rootManageItems(page) {
	await enterLayoutMode(page);
	return openItems(page, () => rootMenuButton(page).click());
}

async function openItems(page, open) {
	await open();
	const seen = await page.locator('[role="menu"]').evaluate((menu) =>
		// One entry a LINE, as `menuLines` reads it: a row of arrows is its
		// label and its glyphs, not four items.
		[...menu.children].map((el) =>
			el.getAttribute('role') === 'group'
				? {
						label: [el.getAttribute('aria-label'), ...[...el.querySelectorAll('button')].map((b) => b.textContent.trim())].join(' '),
						marked: false,
						color: getComputedStyle(el).color
					}
				: { label: el.textContent.trim(), marked: el.classList.contains('marked'), color: getComputedStyle(el).color }
		)
	);
	await page.keyboard.press('Escape');
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
	return seen;
}

/** The gear's menu — the page's settings (da-pp1a) — open: label and whether it is disabled. */
async function gearItems(page) {
	await page.locator('.root-bar button.gear').click();
	const seen = await page
		.locator('[role="menu"]')
		.evaluate((menu) => [...menu.children].map((el) => ({ label: el.textContent.trim(), disabled: el.disabled === true })));
	await page.keyboard.press('Escape');
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
	return seen;
}

/** One root menu item, as `rootItems` reports it. */
const rootItem = async (page, label) => (await rootItems(page)).find((i) => i.label === label);

const layouts = (app, fn, ...args) =>
	app.page.evaluate(([fn, args]) => window.datum.layouts[fn](...args), [fn, args]);

/**
 * The bar's receipt: the first line of a document section in the result
 * area since da-xfa4, where it used to be a span of its own that was
 * always in the DOM. There is none before the first command.
 */
const receipt = (page) => page.locator('.prompt-bar .group:last-child .doc .receipt');

/**
 * The output section's raw lines — what a command with no action echo shows.
 *
 * No `.result` wrapper any more: the bar's output section is ROW ONE OF THE
 * HISTORY now (da-ckw3, vividn 2026-09-21), so it is a `CliEntry` like every
 * row below it, and the raw lines are `RawLines.svelte`'s own box. The input
 * widget's receipt still has the wrapper, because it is still a `BarResult`.
 */
const rawResult = (page) => page.locator('.prompt-bar .group:last-child .raw');

const PRONOUNCED = 'rgb(255, 209, 102)';

describe('the shell', () => {
	test('no header; the root bar shows day active; the bottom bar has the brand, beta, the prompt, an empty result area', async () => {
		await shared.solo(async ({ page, errors }) => {
			assert.equal(await page.locator('header').count(), 0, 'the header is still there');
			assert.equal(await page.locator('aside.beta').count(), 0, 'the beta banner is still there');
			assert.deepEqual(await rootTabs(page), ['main']);
			assert.equal(await activeRootTab(page), 'main');
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'nowview', 'tail', 'custom', 'fields']);

			const bar = page.locator('.prompt-bar');
			// The mark is the database's NAME since da-y992, and this page is
			// on the default database, which is called `datum`. The `beta`
			// mark went up to the root bar (da-y992), and is a banner on the
			// gear now (da-eue7y).
			assert.equal(await bar.locator('.brand').innerText(), 'datum');
			assert.equal(await page.locator('.root-bar .gear .beta').evaluate((el) => el.textContent), 'beta');
			assert.equal(await bar.locator('.tag').count(), 0, 'the beta mark is still on the bottom bar');
			assert.equal(await bar.locator('input').count(), 1);
			// Nothing has run, so the result area has nothing to draw.
			assert.equal(await receipt(page).count(), 0);
			assert.equal(await rawResult(page).count(), 0);
			// The build stamp is in the GEAR's menu now (da-pp1a): version ·
			// sha, disabled, with the word on the beta under it (da-eue7y),
			// which is words and not an item at all. It was the bottom bar's
			// until da-wjob and the root bar's until that bead; neither bar
			// draws it any more.
			assert.equal(await bar.locator('.build').count(), 0, 'the build stamp is still on the bottom bar');
			assert.equal(await page.locator('.root-bar .build').count(), 0, 'the build stamp is still on the root bar');
			// Above it, the gear's acts: «Backups» (da-9ow8m), «Sync»
			// (da-s9r58), and «Reset
			// default layouts» (da-jbcv), which is the narrow half of the
			// «Reset all layouts…» that deleted every saved layout on every
			// device and went for it (da-luwe).
			const gear = await gearItems(page);
			assert.deepEqual(
				gear.map((i) => i.label.replace(/^\S+ · \S+$/, 'the build stamp').replace(/^datum\.day is early software\. .+/, 'the beta note')),
				['Backups', 'Sync', 'Reset default layouts', 'the build stamp', 'the beta note']
			);
			assert.deepEqual(gear.map((i) => i.disabled), [false, false, false, true, false]);
			// The stamp is the web's build, `3.0.<height>-beta · <sha7>`, and
			// patch 0 where a shallow CI checkout cannot count (da-5172a, da-cv118).
			assert.match(gear[3].label, /^3\.0\.\d+-beta · ([0-9a-f]{7}\+?|unknown)$/);
			// The acts are in the active tab's own menu, no icon row.
			assert.equal(await page.locator('.root-bar .act').count(), 0, 'the icon row is still there');
			// The NAME's slice in normal mode: what the tab shows and what it
			// is saved as. `main` is a built-in, so `Reset layout` is there
			// (da-luwe).
			assert.deepEqual(
				(await rootItems(page)).map((i) => i.label),
				['Save layout', 'Restore layout', 'Reset layout', 'Save layout as']
			);
			// Layout mode's: what is DONE to the page and the ways out, Close
			// last and wearing its × (da-ygc9). The arrows read in vim order.
			assert.deepEqual(
				(await rootManageItems(page)).map((i) => i.label),
				['Change Widget', 'Split ← ↓ ↑ →', 'Accept', 'Save layout', 'Discard changes', '× Close tab']
			);
			assert.deepEqual(errors, []);
		});
	});

	test('+ lists day, the widget kinds and the saved layouts; nowview opens a tab; a reload keeps it, another device starts fresh', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			assert.deepEqual(await offered(page), ['New layout', 'main', 'collect', 'dayview', 'cli', 'nowview', 'tail', 'custom', 'command', 'stats', 'chart', 'fields', 'aliases', 'sync', 'help', 'welcome', 'backups', 'conflicts', 'setup', 'input']);

			await pick(page, 'nowview');
			await until(() => rootTabs(page), (t) => t.includes('nowview'));
			assert.deepEqual(await rootTabs(page), ['main', 'nowview']);
			assert.equal(await activeRootTab(page), 'nowview');
			await page.locator('section.pane[data-title="nowview"] svg.svg').waitFor();
			assert.deepEqual(await titles(page), ['nowview']);
			// Two root tabs are the root bar's alone: the root pane draws no strip of its own.
			assert.equal(await page.locator('section.root > .titlebar').count(), 0, 'the root drew a second strip');

			await app.reload();
			assert.deepEqual(await rootTabs(page), ['main', 'nowview']);
			assert.equal(await activeRootTab(page), 'nowview');

			// A saved layout is offered by name, after day and before the kinds.
			page.once('dialog', (dialog) => dialog.accept('mine'));
			await act(page, 'save-as');
			await until(() => rootTabs(page), (t) => t.includes('mine'));
			// The chooser reads the saved layouts each time it opens, so the
			// save is on the list the first time it is asked.
			assert.deepEqual(await offered(page), ['New layout', 'main', 'collect', 'mine', 'dayview', 'cli', 'nowview', 'tail', 'custom', 'command', 'stats', 'chart', 'fields', 'aliases', 'sync', 'help', 'welcome', 'backups', 'conflicts', 'setup', 'input']);
		});

		// Which tabs are open is per device (vividn: the selection is not synced).
		await withApp(async ({ page }) => {
			assert.deepEqual(await rootTabs(page), ['main']);
		}, DESKTOP);
	});
});

describe('a new layout', () => {
	// da-1a9fh, vividn 2026-10-28: «hitting the + button to make a new tab
	// should give the option of creating a new layout, which you then name
	// which starts out blank».
	test('+ «New layout»: cancel makes nothing, a taken name is refused saying so, a free one opens blank and survives a reload', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const ask = page.locator('dialog.ask');
			const answer = async (name) => {
				await ask.locator('input').fill(name);
				await ask.getByRole('button', { name: 'create' }).click();
			};

			await pick(page, 'New layout');
			await ask.getByRole('button', { name: 'cancel' }).click();
			await ask.waitFor({ state: 'detached' });
			await settled(page);
			assert.deepEqual(await rootTabs(page), ['main']);
			assert.deepEqual(await layouts(app, 'listLayouts').then((l) => l.filter((x) => x.source === 'user')), []);

			// A name the `+` already opens — a widget kind's, an open tab's — is
			// said to be taken and asked again, holding what was typed.
			await pick(page, 'New layout');
			for (const name of ['nowview', 'main']) {
				await answer(name);
				await until(() => ask.locator('label span').textContent(), (t) => t === `«${name}» is taken. Name the new layout:`);
				assert.equal(await ask.locator('input').inputValue(), name);
			}
			assert.deepEqual(await rootTabs(page), ['main']);

			await answer('blank');
			await until(() => rootTabs(page), (t) => t.includes('blank'));
			assert.deepEqual(await rootTabs(page), ['main', 'blank']);
			assert.equal(await activeRootTab(page), 'blank');
			// Blank: one pane holding nothing, not a copy of the tab it came from.
			assert.deepEqual(await titles(page), ['empty']);
			const saved = await layouts(app, 'loadLayout', 'blank');
			assert.deepEqual(saved.root.tabs.map((t) => t.content.kind), ['empty']);

			await app.reload();
			assert.deepEqual(await rootTabs(page), ['main', 'blank']);
			assert.equal(await activeRootTab(page), 'blank');
			assert.deepEqual(await titles(page), ['empty']);
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('a URL', () => {
	test('/nowview, /stats and /now open that tab; a reload on /stats lands on stats', async () => {
		await shared.wipe(async (app) => {
			const { page } = app;
			await app.goto('/nowview');
			assert.deepEqual(await rootTabs(page), ['main', 'nowview']);
			assert.equal(await activeRootTab(page), 'nowview');
			assert.equal(new URL(page.url()).pathname, '/', 'the URL did not go back to /');

			await app.goto('/stats');
			assert.equal(await activeRootTab(page), 'stats');
			await page.locator('section.pane[data-title="stats"]').waitFor();
			await app.reload();
			assert.equal(await activeRootTab(page), 'stats');

			// The old route is an alias: the same tab, not a second one.
			await app.goto('/now');
			assert.deepEqual(await rootTabs(page), ['main', 'nowview', 'stats']);
			assert.equal(await activeRootTab(page), 'nowview');
			assert.deepEqual(app.errors, []);
		});
	});

	// Before a test that navigates first (`wipe` then `goto`): a notice page
	// draws no tree, and the next `solo` or `room` would boot `/` again to
	// get off it — a boot the navigation here pays for anyway (da-iwikb).
	test('/no-such-thing is a quiet notice with a link home', async () => {
		await shared.wipe(async (app) => {
			await app.page.goto(new URL('/no-such-thing', app.page.url()).href);
			const notice = app.page.locator('main .notice');
			await notice.waitFor();
			assert.match(await notice.innerText(), /no-such-thing/);
			assert.equal(await notice.locator('a[href="/"]').count(), 1);
			assert.deepEqual(await rootTabs(app.page), ['main']);
		});
	});

	// The widget bar became the fields widget (da-c7u8); its old name is an alias.
	test('/fields, /widgets and /widgetbar open the fields widget alone', async () => {
		await shared.wipe(async (app) => {
			const { page } = app;
			for (const path of ['/fields', '/widgets', '/widgetbar']) {
				await app.goto(path);
				assert.deepEqual(await rootTabs(page), ['main', 'fields']);
				assert.equal(await activeRootTab(page), 'fields');
				await page.locator('section.pane[data-title="fields"] .hint.empty').waitFor();
				assert.deepEqual(await titles(page), ['fields']);
			}
			assert.deepEqual(app.errors, []);
		});
	});

	// The widget's files are called Fields* now (da-850j) but its KIND ID is
	// `widgetbar` still, because that id is the key in every layout already
	// saved. This is the JSON an older build wrote: it must still draw.
	test('a layout saved under the old kind id still opens the fields widget', async () => {
		await shared.wipe(async (app) => {
			const { page } = app;
			await layouts(app, 'saveLayout', {
				version: 1,
				name: 'old',
				root: { id: 'o', tabs: [{ id: 'o-tab', content: { id: 'o-w', kind: 'widgetbar', settings: {} } }], active: 0, bar: { edge: 'top', align: 'start' } }
			});
			await app.goto('/old');
			assert.deepEqual(await titles(page), ['fields']);
			await page.locator('section.pane[data-title="fields"] .hint.empty').waitFor();
			assert.deepEqual(app.errors, []);
		});
	});

	// The kind `command` became `custom` (da-zqop), which freed the id for
	// the commands widget that took it (da-ltux). Nobody had saved a layout
	// naming it, so it was a PLAIN rename: no `RETIRED_KINDS` arm, no alias
	// — and the absence of a migration is what lets the id mean the new
	// widget now. An arm added «to be safe» would have migrated this widget
	// away the day it landed. Both names are a widget of their own here, and
	// a layout naming either opens as that one.
	test('/custom opens the custom widget and /command the commands widget, each under its own name', async () => {
		await shared.wipe(async (app) => {
			const { page } = app;
			await app.goto('/custom');
			assert.deepEqual(await rootTabs(page), ['main', 'custom']);
			assert.equal(await activeRootTab(page), 'custom');
			assert.deepEqual(await titles(page), ['custom']);
			// Alone at the root the pane draws no title strip, so its ✎ is
			// hidden; what says the widget is there is its own hint.
			await page
				.locator('section.pane[data-title="custom"] .body', { hasText: 'type a datum command' })
				.waitFor();

			const kinds = await offered(page);
			assert.ok(kinds.includes('custom'), `the chooser does not offer custom: ${kinds}`);
			assert.ok(kinds.includes('command'), `the chooser does not offer command: ${kinds}`);

			await app.goto('/command');
			assert.deepEqual(await titles(page), ['command']);
			await page.locator('section.pane[data-title="command"] button[data-command="undo"]').waitFor();

			await layouts(app, 'saveLayout', {
				version: 1,
				name: 'a-command',
				root: { id: 'c', tabs: [{ id: 'c-tab', content: { id: 'c-w', kind: 'command', settings: {} } }], active: 0, bar: { edge: 'top', align: 'start' } }
			});
			// Through the `+`, not a third boot: which kind the saved id
			// draws is the claim, and both doors open a layout by its name.
			await pick(page, 'a-command');
			await until(() => activeRootTab(page), (t) => t === 'a-command');
			assert.deepEqual(await titles(page), ['command']);
			await page.locator('section.pane[data-title="command"] button[data-command="occur"]').waitFor();
			assert.deepEqual(app.errors, []);
		});
	});

	// The deployed site serves every path from adapter-static's fallback
	// (vite.config.ts), the shape of a plain bucket — so `/stats` has to
	// work with no route file of its own, against the BUILT output. The same
	// bare server isolation.test.mjs uses: no headers, index.html for any
	// path it does not have.
	test('/stats works from the built output behind a fallback-only server', async () => {
		const build = fileURLToPath(new URL('../build', import.meta.url));
		const server = http.createServer((req, res) => {
			let file = path.join(build, decodeURIComponent(new URL(req.url, 'http://x').pathname));
			if (!file.startsWith(build) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) file = path.join(build, 'index.html');
			const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.wasm': 'application/wasm', '.json': 'application/json', '.webmanifest': 'application/manifest+json', '.png': 'image/png' };
			res.writeHead(200, { 'content-type': types[path.extname(file)] ?? 'application/octet-stream' });
			fs.createReadStream(file).pipe(res);
		});
		await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
		const browser = await chromium.launch();
		try {
			const page = await browser.newPage({ ...DESKTOP });
			await page.goto(`http://127.0.0.1:${server.address().port}/stats`);
			await page.locator('.root-bar [role="tab"][aria-selected="true"]', { hasText: 'stats' }).waitFor();
			await page.locator('section.pane[data-title="stats"]').waitFor();
		} finally {
			await browser.close();
			await new Promise((resolve) => server.close(resolve));
		}
	});
});

describe('the acts', () => {
	test('a split shows the dot and marks Save and Restore; restore undoes it; save keeps it', async () => {
		// In a ROOM (da-xamd): the tree is `main`'s, the name is this test's,
		// and the save this makes is nobody else's to meet.
		await shared.room('act-split', async (app, room) => {
			const { page } = app;
			assert.deepEqual(await dotted(page), []);
			// `Restore layout` and not `Save layout`, which leads the menu
			// since da-z8klv: a menu opens on its first item, and once the
			// keyboard has been used the browser paints that focus, so the
			// first item's colour is not the same fact read twice.
			const quiet = (await rootItem(page, 'Restore layout')).color;

			await manage(page, 'tail', 'split: a new pane to the right');
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'nowview', 'tail', 'empty', 'custom', 'fields']);
			assert.deepEqual(await dotted(page), [room]);
			// The dot's meaning, in the menu beside it: exactly the two acts
			// that answer it are marked, and in the dot's own amber.
			const marked = await rootItems(page);
			assert.deepEqual(marked.filter((i) => i.marked).map((i) => i.label), ['Save layout', 'Restore layout']);
			for (const i of marked.filter((i) => i.marked)) assert.equal(i.color, PRONOUNCED, `${i.label} is ${i.color}`);

			await act(page, 'restore');
			await until(() => dotted(page), (d) => d.length === 0);
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'nowview', 'tail', 'custom', 'fields']);
			assert.deepEqual((await rootItems(page)).filter((i) => i.marked), []);
			assert.equal((await rootItem(page, 'Restore layout')).color, quiet);

			await manage(page, 'tail', 'split: a new pane to the right');
			await fill(page, 'nowview');
			await page.locator('section.pane[data-title="nowview"]').nth(1).locator('svg.svg').waitFor();
			await act(page, 'save');
			await until(() => dotted(page), (d) => d.length === 0);
			assert.deepEqual(
				(await layouts(app, 'listLayouts')).map((l) => l.name),
				[room]
			);
			const saved = await layouts(app, 'loadLayout', room);
			// The save is the tree as shown: the tail's row split, nowview beside it.
			const kinds = JSON.stringify(saved.root).match(/"kind":"nowview"/g);
			assert.equal(kinds.length, 2, 'the second nowview was not saved');
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'nowview', 'tail', 'nowview', 'custom', 'fields']);
		});

		// A second device loads the saved tree.
		// (A second CONTEXT is a second OPFS, so the save is carried across
		// by hand: what this checks is that a saved `main` shadows the built-in.)
	});

	test('save as renames this copy to mine and leaves the layout it came from alone', async () => {
		// The room stands in for what `main` was here: `save as` moves THIS
		// copy to the new name, and the layout it was a copy of keeps its own.
		await shared.room('act-as', async (app) => {
			const { page } = app;
			await manage(page, 'tail', 'split: a new pane to the right');
			page.once('dialog', (dialog) => dialog.accept('mine'));
			await act(page, 'save-as');
			await until(() => rootTabs(page), (t) => t.includes('mine'));
			assert.deepEqual(await rootTabs(page), ['main', 'mine']);
			assert.deepEqual(await dotted(page), []);
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'nowview', 'tail', 'empty', 'custom', 'fields']);
			assert.deepEqual(
				(await layouts(app, 'listLayouts')).map((l) => l.name),
				['mine']
			);
			// day was never saved and keeps no draft of the edit; it is the
			// default still.
			await front(page, 'main');
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'nowview', 'tail', 'custom', 'fields']);
			assert.deepEqual(await dotted(page), []);

			// Both open, both tail panes editable on their own: a split in
			// mine's tail leaves main's alone.
			await front(page, 'mine');
			await addTab(page, 'tail', 'aliases');
			assert.deepEqual(await page.locator('section.pane [role="tablist"] [role="tab"]').allInnerTexts(), ['tail', 'aliases']);
			await front(page, 'main');
			assert.equal(await page.locator('section.pane [role="tablist"]').count(), 0, 'the tab strip leaked into day');
			assert.deepEqual(await dotted(page), ['mine']);
			assert.deepEqual(app.errors, []);
		});
	});

	test('an unmodified restore lists the versions, and one of them can be picked', async () => {
		await shared.room('act-trail', async (app) => {
			const { page } = app;
			await manage(page, 'custom', 'Close pane');
			await act(page, 'save');
			await until(() => dotted(page), (d) => d.length === 0);
			await manage(page, 'nowview', 'Close pane');
			await act(page, 'save');
			await until(() => dotted(page), (d) => d.length === 0);
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'tail', 'fields']);

			// Nothing to discard, so Restore offers the save's versions under
			// the bar instead — the one affordance that did NOT move (da-wjob).
			await act(page, 'restore');
			const items = page.locator('.root-bar .versions button');
			await items.first().waitFor();
			assert.deepEqual(
				(await items.allInnerTexts()).map((t) => t.split(' ·')[0]),
				['→ forward', 'v2', 'v1']
			);
			await items.filter({ hasText: /^v1 / }).click();
			await until(() => titles(page), (t) => t.length === 5);
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'nowview', 'tail', 'fields']);
			assert.deepEqual(await dotted(page), []);
		});
	});

	test('a saved day is what another device opens', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await manage(page, 'custom', 'Close pane');
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'nowview', 'tail', 'fields']);
			await act(page, 'save');
			await until(() => dotted(page), (d) => d.length === 0);
			const saved = await layouts(app, 'loadLayout', 'main');

			// Another tab of the same browser, opened fresh: no draft of its
			// own (the draft is per name, and this one was cleared by the
			// save), so what it shows is the save.
			await page.evaluate(() => localStorage.removeItem('datum.layout.root.v1'));
			const other = await app.newTab();
			try {
				assert.deepEqual(await titles(other.page), ['dayview', 'cli', 'nowview', 'tail', 'fields']);
				assert.deepEqual(await dotted(other.page), []);
				assert.deepEqual(await layouts(other, 'loadLayout', 'main'), saved);
			} finally {
				await other.page.close();
			}
		});
	});

	test('a room after a test that SAVED main is main as it ships, not the save (da-x92rt)', async () => {
		await shared.solo(async ({ page }) => {
			await manage(page, 'custom', 'Close pane');
			await act(page, 'save');
			await until(() => dotted(page), (d) => d.length === 0);
		});
		await shared.room('after-save', async ({ page }) => {
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'nowview', 'tail', 'custom', 'fields']);
		});
	});

	// The review thread on #168: the cross-tab refresh decided "unmodified"
	// before its awaited load, so an edit made meanwhile was overwritten.
	test('an edit in tab A survives a save from tab B, dot and all', async () => {
		await shared.solo(async (a) => {
			const b = await a.newTab();
			try {
				await manage(a.page, 'tail', 'split: a new pane to the right');
				assert.deepEqual(await dotted(a.page), ['main']);
				assert.deepEqual(await dotted(b.page), []);

				const heard = await a.spyOnRunner();
				await manage(b.page, 'custom', 'Close pane');
				await act(b.page, 'save');
				await until(() => dotted(b.page), (d) => d.length === 0);
				// A hears the save — it reads the saved layout back from
				// `_internal` — and once that has answered, its baseline has
				// moved and its edit has not.
				await until(heard, (argvs) => argvs.some((argv) => argv[0] === '--db' && argv[1] === '_internal'));
				await a.idle();
				await settled(a.page);
				assert.deepEqual(await titles(a.page), ['dayview', 'cli', 'nowview', 'tail', 'empty', 'custom', 'fields']);
				assert.deepEqual(await dotted(a.page), ['main']);
				// And a tab that was not edited follows: a third tab, and B itself.
				assert.deepEqual(await titles(b.page), ['dayview', 'cli', 'nowview', 'tail', 'fields']);
			} finally {
				await b.page.close();
			}
		});
	});
});

describe('the bottom bar', () => {
	test('a typed line is run and receipted with the hid from the listing; a press ghosts in the field; foo is an error', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const echoed = (await app.echoes()).length;
			const input = page.locator('.prompt-bar input');
			// Tracked, so the fields widget has a button to press (da-c7u8).
			await app.run(['setup', 'coffee', '--verb', 'occur']);
			await input.fill('occur coffee');
			await input.press('Enter');
			await until(() => receipt(page).innerText(), (t) => t.startsWith('CREATE '));
			await app.idle();
			const { rows } = await app.tailRows();
			assert.equal(rows.length, 1);
			const [word, hid] = (await receipt(page).innerText()).split(' ');
			assert.equal(word, 'CREATE');
			assert.ok(hid.length >= 4 && rows[0].hid.startsWith(hid), `the receipt's ${hid} is not the row's ${rows[0].hid}`);
			assert.equal(await receipt(page).evaluate((el) => el.classList.contains('error')), false);
			// As if typed at the CLI widget: it is in its scrollback.
			assert.deepEqual((await app.echoes()).slice(echoed), ['occur coffee']);

			// A widget-bar press: the bar's field GHOSTS the command — beside
			// the prompt, never as the box's value (da-jprg) — and the
			// receipt is the press's. A second on: the clock is held, and a
			// second coffee at the first one's instant is the first again.
			await app.clock.forward(1000);
			const saw = await app.watchGhosts();
			await page.locator('.bar button.widget', { hasText: 'coffee' }).click();
			await saw('occur coffee');
			assert.equal(await input.inputValue(), '', 'the press became the input\'s value');
			await until(() => receipt(page).innerText(), (t) => t.startsWith('CREATE ') && t !== `CREATE ${hid}`);
			await app.idle();
			assert.equal((await app.tailRows()).rows.length, 2);
			await page.waitForFunction(() => !document.querySelector('.prompt-bar .ghost'));
			assert.equal(await input.inputValue(), '');

			// An error echoes no document, so the result area shows the CLI's
			// own stderr lines and is marked (da-xfa4).
			await input.fill('foo');
			await input.press('Enter');
			await page.locator('.prompt-bar .group:last-child .raw.error').waitFor();
			assert.equal(await receipt(page).count(), 0, 'an error drew a document');
			const said = await app.runLine('foo');
			const firstLine = said.stderr.replace(/\x1b\[[0-9;]*m/g, '').split('\n').find((l) => l.trim()).trim();
			assert.equal((await rawResult(page).innerText()).split('\n')[0], firstLine);
			assert.equal(
				await rawResult(page).evaluate((el) => getComputedStyle(el).color),
				'rgb(244, 100, 90)'
			);
			assert.deepEqual(app.errors, []);
		});
	});

	// The page's one command line and its status line (da-rfs9): tall
	// enough to stand out — absolute heights, not "bigger than before" —
	// and the CLI widget is only the scrollback it types into. da-xfa4 gave it a
	// result area above the input row and fixed the whole thing at 6rem;
	// da-ckw3 made the two sections EQUAL (vividn: "so 50/50") at 3.6rem
	// each, and then vividn on that build: "Decrease the height of the input
	// bar. There is a lot of wasted padding" and "increase the size of the
	// font in the input bar slightly". So they are no longer equal: the
	// input is 2.9rem — one row of 1.25rem text over the 1.1rem strip the
	// ACTION words stand in — the output is still 3.6rem, and the hairline
	// between the bar and the dashboard is the odd pixel. 46.4 + 57.6 + 1.
	// da-mwp4m made the output a share of the page, «where the command
	// widget is now by default»: 15% of it between 3.6rem and 8rem, so at
	// 1280x800 the OPEN bar, which every screen starts on, is 46.4 + 114 +
	// 1. The window stays absolute.
	test('stands out at 150–175px with a ≥1.05rem prompt; the CLI has no prompt, and the bar and a chip land in its scrollback', async () => {
		// The shared desktop, resized: `home` puts its own size back for the
		// next test. The scrollback is the shared page's, so the echoes and
		// documents read are the ones past what was there when this began.
		await shared.solo(async (app) => {
			const { page } = app;
			await page.setViewportSize({ width: 1280, height: 800 });
			// Two frames, so the bar has measured the page it is a share of.
			await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
			const echoed = (await app.echoes()).length;
			const { height, font, rem } = await page.evaluate(() => ({
				height: document.querySelector('.prompt-bar').getBoundingClientRect().height,
				font: parseFloat(getComputedStyle(document.querySelector('.prompt-bar input')).fontSize),
				rem: parseFloat(getComputedStyle(document.documentElement).fontSize)
			}));
			assert.ok(height >= 150 && height <= 175, `the bottom bar is ${height}px tall at 1280x800`);
			assert.ok(font >= 1.05 * rem, `the prompt's input is ${font}px on a ${rem}px rem`);

			const repl = page.locator('section[data-pane="repl"]');
			assert.equal(await repl.count(), 1);
			assert.equal(await repl.locator('input, .line').count(), 0, 'the CLI widget still has a prompt of its own');

			// Tracked, so the fields widget has a button to press (da-c7u8).
			await app.run(['setup', 'coffee', '--verb', 'occur']);
			const docs = await repl.locator('.doc').count();
			await app.submit('occur coffee');
			await until(() => receipt(page).innerText(), (t) => /^CREATE \S+$/.test(t));
			assert.deepEqual((await app.echoes()).slice(echoed), ['occur coffee']);
			// The scrollback carries the command's DOCUMENT, not its printed
			// lines: the web asks every command it runs for `--show json`
			// (da-xfa4), so both surfaces draw the same sections. This
			// command's: the pane's newest, past the ones already there.
			await until(() => repl.locator('.doc').count(), (n) => n > docs);
			assert.match(
				await repl.locator('.doc').last().innerText(),
				/CREATE[\s\S]*coffee/,
				"the CLI's action is not in the scrollback"
			);

			await app.pressWidget('coffee');
			assert.deepEqual((await app.echoes()).slice(echoed), ['occur coffee', 'occur coffee']);

			await app.submit('tail');
			const input = page.locator('.prompt-bar input');
			await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === '');
			await input.press('ArrowUp');
			assert.equal(await input.inputValue(), 'tail');
		});
	});

	// REFUSED rather than never settled: both leave the app in memory, and a
	// handle that never settles holds the boot for `ACQUIRE_DEADLINE_MS`
	// (5 s, #lib/cli.ts) first. That path is storage.test.mjs's (`NEVER`);
	// what this test reads is the bar once memory is where the rows are.
	test('memory-only storage turns the bar orange, with the storage sentence as its title', async () => {
		await withApp(
			async ({ page }) => {
				const bar = page.locator('.prompt-bar');
				await page.waitForFunction(() => document.querySelector('.prompt-bar').classList.contains('alarm'));
				assert.equal(await bar.evaluate((el) => getComputedStyle(el).borderTopColor), 'rgb(232, 130, 12)');
				assert.match(await bar.getAttribute('title'), /held in memory only/);
			},
			{
				...DESKTOP,
				prepare: inWorker(
					"FileSystemFileHandle.prototype.createSyncAccessHandle = () => Promise.reject(new DOMException('refused', 'InvalidStateError'));"
				)
			}
		);
	});

	test('a pill set programmatically shows, and a press on it removes it', async () => {
		await onBar(async (app) => {
			const { page } = app;
			await page.evaluate(() => window.datum.bar.setPills([{ id: 'when', text: '-t 0700', kind: 'time' }]));
			const pill = page.locator('.prompt-bar .pill');
			// The × beside the label is a mark inside the remove button now
			// (da-tnv3), so the pill's own text is the label's.
			assert.equal(await pill.locator('.label').textContent(), '-t 0700');
			await pill.locator('button.remove').click();
			assert.equal(await page.locator('.prompt-bar .pill').count(), 0);
			assert.deepEqual(await page.evaluate(() => window.datum.bar.pills()), []);
		});
	});

	test('a submit runs typed + pills, then an unlocked pill clears on success', async () => {
		await onBar(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			await page.evaluate(() => window.datum.bar.setPills([{ id: 'when', text: '-t 0700', kind: 'time' }]));
			const input = page.locator('.prompt-bar input');
			await input.fill('occur tea');
			await input.press('Enter');
			// After, not before: the CLI takes no flag before its verb (da-c7u8).
			await until(seen, (argvs) => argvs.some((a) => a[0] === 'occur'));
			assert.ok(
				(await seen()).some((a) => JSON.stringify(a) === JSON.stringify(['occur', 'tea', '-t', '0700'])),
				JSON.stringify(await seen())
			);
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill').length === 0);
			assert.deepEqual(await page.evaluate(() => window.datum.bar.pills()), []);
		});
	});

	test('a locked pill survives a successful press; an unlocked one does not', async () => {
		await onBar(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			await page.evaluate(() =>
				window.datum.bar.setPills([{ id: 'when', text: '-t 0700', kind: 'time', locked: true }])
			);
			await page.locator('.prompt-bar .pill button.lock').click();
			assert.equal(await page.evaluate(() => window.datum.bar.pills()[0].locked), false);

			// Re-lock it, then run twice: the pill and its time survive both.
			await page.locator('.prompt-bar .pill button.lock').click();
			assert.ok((await page.locator('.prompt-bar .pill').getAttribute('class')).split(' ').includes('locked'));

			for (let i = 0; i < 2; i++) {
				const input = page.locator('.prompt-bar input');
				await input.fill('occur tea');
				await input.press('Enter');
				await until(seen, (argvs) => argvs.filter((a) => a[0] === 'occur').length === i + 1);
			}
			assert.deepEqual(
				(await seen()).filter((a) => a[0] === 'occur'),
				[
					['occur', 'tea', '-t', '0700'],
					['occur', 'tea', '-t', '0700']
				]
			);
			assert.equal(await page.locator('.prompt-bar .pill').count(), 1, 'the locked pill survives');
		});
	});

	test('a failed command leaves its pill in place', async () => {
		await onBar(async (app) => {
			const { page } = app;
			await page.evaluate(() => window.datum.bar.setPills([{ id: 'when', text: '-t 0700', kind: 'time' }]));
			const input = page.locator('.prompt-bar input');
			await input.fill('foo');
			await input.press('Enter');
			// Two links, waited for one at a time, so a timeout says which
			// broke (da-w5m40): run #649 waited 30 s on the drawn error with
			// its neighbours fast and nothing in flight after, so the command
			// was either never run or run and not drawn — one wait on the
			// drawing could not tell those apart. The CLI widget's echo is the
			// run; the bar's red line is the drawing; a submit that threw is
			// a page error, which this test never read.
			await until(
				async () => ({ echoes: await app.echoes(), errors: app.errors }),
				(seen) => seen.errors.length > 0 || seen.echoes.some((e) => e.startsWith('foo'))
			);
			assert.deepEqual(app.errors, []);
			await page.locator('.prompt-bar .group:last-child .raw.error').waitFor();
			assert.equal(await page.locator('.prompt-bar .pill').count(), 1, 'a failed command keeps the pill');
		});
	});

	// The pills moved INSIDE the prompt in da-xfa4 — vividn: "move the -t
	// pills to be just slightly to the right of the >, so that they are
	// visible as you type text" — so what "after the input" means is no
	// longer DOM order but where they are drawn: past the caret, over the
	// box, and inside it.
	test('the pills are drawn inside the box, past the caret', async () => {
		await onBar(async (app) => {
			const { page } = app;
			await page.evaluate(() => window.datum.bar.setPills([{ id: 'when', text: '-t 0700', kind: 'time' }]));
			await page.locator('.prompt-bar .pill').waitFor();
			const seen = await page.evaluate(() => {
				const box = (s) => document.querySelector(s).getBoundingClientRect();
				return {
					pill: box('.prompt-bar .pill'),
					caret: box('.prompt-bar .line > .prompt'),
					field: box('.prompt-bar .field')
				};
			});
			assert.ok(seen.pill.left > seen.caret.right, 'the pill is not past the ❯');
			assert.ok(
				seen.pill.left >= seen.field.left - 1 && seen.pill.right <= seen.field.right + 1,
				'the pill is outside the box'
			);
		});
	});
});

describe('the cycle guard', () => {
	test('a layout that embeds itself loads as a notice, not a recursion', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			// A saved `main` whose tail is a tab holding `main`.
			const tree = await page.evaluate(() => window.datum.shell.extract(window.datum.layout.current().root.tabs[0]));
			const tail = JSON.stringify(tree).replace(/\{"id":"tail-tab","content":\{"id":"tail-widget","kind":"tail","settings":\{\}\}\}/, '{"id":"tail-tab","layout":"main","content":{"id":"tail-widget","kind":"empty","settings":{}}}');
			assert.notEqual(tail, JSON.stringify(tree), 'the tail tab was not found to replace');
			await layouts(app, 'saveLayout', JSON.parse(tail));
			await app.reload();
			// The tab is the `main` layout's, so it wears that name (da-9puz);
			// what it holds, since it cannot hold itself, is the notice.
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'nowview', 'main', 'custom', 'fields']);
			assert.match(await app.paneText('main'), /cannot hold itself/);
			assert.deepEqual(app.errors, []);
		});
	});

	test('the chooser offers day inside a pane, and a nested layout draws', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			// mine: the nowview alone, then embedded in main's tail.
			await layouts(app, 'saveLayout', { version: 1, name: 'mine', root: { id: 'm', tabs: [{ id: 'm-tab', content: { id: 'm-w', kind: 'nowview', settings: {} } }], active: 0, bar: { edge: 'top', align: 'start' } } });
			await manage(page, 'tail', 'split: a new pane to the right');
			await openChooser(page);
			assert.deepEqual(await chooser(page).locator('button.layout').allInnerTexts(), ['collect', 'mine'], 'main, open above, must not be offered');
			await chooser(page).locator('button', { hasText: exactly('mine') }).click();
			// The pane wears the LAYOUT's name, not its one widget's (da-9puz).
			await page.locator('section.pane[data-title="mine"]').locator('svg.svg').waitFor();
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'nowview', 'tail', 'mine', 'custom', 'fields']);
		});
	});
});

describe('the one chooser', () => {
	// da-uyx1: the empty pane's button and the root bar's `+` open the SAME
	// modal, so what they offer cannot drift apart — one list, one order.
	test("the root + and an empty pane offer the same items in the same order, minus the pane's ancestors", async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			// A saved layout, so the Layouts section holds more than `main`.
			await layouts(app, 'saveLayout', { version: 1, name: 'mine', root: { id: 'm', tabs: [{ id: 'm-tab', content: { id: 'm-w', kind: 'nowview', settings: {} } }], active: 0, bar: { edge: 'top', align: 'start' } } });
			const fromRoot = await offered(page);
			assert.deepEqual(fromRoot, ['New layout', 'main', 'collect', 'mine', 'dayview', 'cli', 'nowview', 'tail', 'custom', 'command', 'stats', 'chart', 'fields', 'aliases', 'sync', 'help', 'welcome', 'backups', 'conflicts', 'setup', 'input']);

			await manage(page, 'tail', 'split: a new pane to the right');
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'nowview', 'tail', 'empty', 'custom', 'fields']);
			await openChooser(page);
			// The same list, in the same order — less `main`, which is open
			// above this pane and may not be offered inside itself, and less
			// «New layout», which names a root tab (da-1a9fh).
			assert.deepEqual(await offers(page), fromRoot.filter((n) => n !== 'main' && n !== 'New layout'));

			// The keyboard is inside the dialog, on its first item, and Tab
			// walks the rest in the order they are listed.
			const focused = () => page.evaluate(() => document.activeElement?.textContent?.trim());
			assert.equal(await page.evaluate(() => document.activeElement?.closest('dialog.chooser') !== null), true);
			assert.equal(await focused(), fromRoot[2]);
			await page.keyboard.press('Tab');
			assert.equal(await focused(), fromRoot[3]);

			// Escape closes it, hands focus back to the button that opened
			// it, and changes nothing: the pane is still empty.
			await page.keyboard.press('Escape');
			await chooser(page).waitFor({ state: 'detached' });
			assert.equal(await focused(), 'Change Widget');
			await settled(page);
			assert.deepEqual(await titles(page), ['dayview', 'cli', 'nowview', 'tail', 'empty', 'custom', 'fields']);
			assert.deepEqual(app.errors, []);
		});
	});
});

// A kind's id is a saved-layout key (docs/web-conventions.md #10), so a
// widget's id is what a device's stored layouts and drafts hold it by, and
// that is what makes a widget's RETURN safe as well as its retirement.
//
// These two tests are da-22vg's, inverted. The `input` widget went on
// 09-14 and `RETIRED_KINDS` mapped it to `repl`; vividn asked for it back on
// 09-15 (da-3zon) — he wants the box somewhere other than the foot of the
// screen — so the arm is gone and the same id draws the same kind again.
// A layout saved with an input widget BEFORE 09-14 opens as an input once
// more; one that was migrated in the fortnight between was rewritten to
// `repl` by `extract` on its next save and stays a REPL, which is what it
// says it is.
// ⛔ A SETTING NOBODY CHOSE DOES NOT SURVIVE A SAVE (da-r0ydu, canary
// qwen-3-8-2-4t on PR #73). `defaults.ts` used to write
// `dayHeight: DEFAULT_DAY_HEIGHT` into every shipped tree, and a number in
// the strip's settings is what HOLDS its day against its box for good. So
// every layout saved before the day began following its widget carries an
// 85 the reader never picked, and without an arm in `migrated` that reader
// is the one the feature never reaches. `LAYOUT_VERSION` is still 1 and
// nothing else migrates trees, so this is the seam.
describe('a layout saved with the day height the defaults used to write', () => {
	const ROOT_BAR = { edge: 'top', align: 'start' };
	const stripLayout = (settings) => ({
		version: 1,
		name: 'old',
		root: {
			id: 'o',
			active: 0,
			bar: ROOT_BAR,
			tabs: [{ id: 'o-tab', content: { id: 'o-w', kind: 'daystrip', settings } }]
		}
	});
	/** What the strip's handle says — the height actually drawn. */
	const drawn = (page) =>
		page.locator('.dayview .today-line').getAttribute('aria-valuenow').then(Number);

	test('the OLD DEFAULT is dropped, so the day follows the widget again', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await layouts(app, 'saveLayout', stripLayout({ dayHeight: 85, freezeToday: true }));
			await pick(page, 'old');
			await until(() => rootTabs(page), (t) => t.includes('old'));
			await page.locator('.dayview .today-line').waitFor();

			// One widget filling the root: the box is far taller than 85, so
			// a day that followed it cannot be mistaken for the saved number.
			await until(() => drawn(page), (n) => n !== 85);
			// And it is `dayFilling` of that box — the box less its ruler,
			// clamped as the strip clamps it (panes.ts). At a whole page the
			// derivation lands past the ceiling, so the clamp is the answer.
			const box = await page.locator('section.pane[data-title="dayview"] .strip').evaluate((el) => el.clientHeight);
			assert.equal(await drawn(page), Math.min(240, Math.max(40, box - 15)), 'the day is not what the box derives');

			// And the tab is not dirty: dropping a setting nobody chose is a
			// READ of the layout, not an edit of it.
			assert.deepEqual(await dotted(page), []);
			assert.deepEqual(app.errors, []);
		});
	});

	// ⛔ AND THE DROP RUNS BOTH WAYS (da-r0ydu, canary qwen-3-8-2-4t on PR
	// #73, round 6). The handle steps by 5 and both clamps are multiples of
	// 5, so a reader whose box derives something else CAN land the day on
	// the retired literal and press «Save options with layout», which is
	// the one writer to the tree. If only `inline` dropped it, the two
	// sides of `modified` would disagree from that instant — the tab's own
	// `extract` keeping the key, the baseline made by `baselineOf` dropping
	// it — and the dot would stay lit THROUGH a save, offering «Restore
	// layout» for a difference no reload would keep. `extract` drops it
	// too, so the tree says what the save says, and the trade stays the one
	// the note states: a chosen 85 cannot be held across a reload either.
	test('a day height dragged onto the retired literal is not saved, and does not light the dot', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			// A layout that says NOTHING about the height, so the day follows
			// and the baseline has no key for the comparison to hold onto.
			// It DOES say the label width the widget defaults to (54,
			// `DEFAULT_LABEL_WIDTH`): the handle carries both sizes on every
			// press and the session keeps whatever DIFFERS from the layout,
			// so without this the save would also be writing a label width
			// and the dot below would be reporting that instead.
			await layouts(app, 'saveLayout', stripLayout({ freezeToday: true, labelWidth: 54 }));
			await pick(page, 'old');
			await until(() => rootTabs(page), (t) => t.includes('old'));
			const grip = page.locator('.dayview .today-line');
			await grip.waitFor();
			const box = await page.locator('section.pane[data-title="dayview"] .strip').evaluate((el) => el.clientHeight);
			const follows = Math.min(240, Math.max(40, box - 15));
			await until(() => drawn(page), (n) => n === follows);
			// The box must derive something ELSE, or the handle would hand
			// its owner `undefined` and this would test the other rule.
			assert.notEqual(follows, 85, 'the box derives 85, so nothing here is held');

			// Down onto 85 by DRAGGING the handle, which is how a reader
			// gets there. One drop is one re-render of the stack; the
			// keyboard's 5px step took thirty-one of them and was a quarter
			// of this file's time (da-31464).
			const at = await grip.boundingBox();
			const [x, y] = [at.x + at.width / 2, at.y + at.height / 2];
			await page.mouse.move(x, y);
			await page.mouse.down();
			await page.mouse.move(x, y + follows - 85, { steps: 4 });
			await page.mouse.up();
			await until(() => drawn(page), (n) => n === 85);
			assert.deepEqual(await dotted(page), [], 'the session alone dirtied the layout');

			// «Save options with layout» is the one writer to the tree.
			await chooseRoot(page, 'Save options with layout');
			await until(() => drawn(page), (n) => n === 85);

			// The key is sitting in the tree — and the tree still reads as
			// the save does, because `extract` drops it the way `inline`
			// does. Without the outbound arm the dot lights here and stays
			// lit THROUGH a save, offering «Restore layout» over a
			// difference no reload would keep.
			assert.deepEqual(await dotted(page), [], 'the tab is dirty over a key no save would keep');

			// Now move the OTHER size, which the layout DOES have a value
			// for. That gives the save something observable to carry and a
			// dot that genuinely lit — so the two assertions below are a
			// save landing and a dot CLEARING, not a pair that would read
			// the same if nothing had happened at all.
			await page.locator('.dayview .spine').focus();
			await page.keyboard.press('ArrowRight');
			await chooseRoot(page, 'Save options with layout');
			await until(() => dotted(page), (d) => d.length === 1);

			// And the save carries the label width and no day height —
			// which is the whole bug in one line: without the outbound drop
			// the save writes an 85 the baseline will not have, so the dot
			// never goes out again.
			await chooseRoot(page, 'Save layout');
			const stored = await until(
				() => layouts(app, 'loadLayout', 'old').then((l) => l.root.tabs[0].content.settings),
				(s) => s.labelWidth === 59
			);
			assert.equal(stored.dayHeight, undefined, 'the save wrote a day height no reload can read back');
			await until(() => dotted(page), (d) => d.length === 0);

			// So a reload has the day following its widget again — which is
			// the note's «a chosen 85 cannot be saved as held», seen.
			await page.reload();
			await page.waitForFunction(() => typeof window.datum?.run === 'function');
			await grip.waitFor();
			await until(() => drawn(page), (n) => n === follows);
			assert.deepEqual(app.errors, []);
		});
	});

	test('a day height the reader actually chose is kept', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await layouts(app, 'saveLayout', stripLayout({ dayHeight: 120, freezeToday: true }));
			await pick(page, 'old');
			await until(() => rootTabs(page), (t) => t.includes('old'));
			await page.locator('.dayview .today-line').waitFor();
			await until(() => drawn(page), (n) => n === 120);
			// Held, not derived: the box is nothing like 120 + its ruler.
			const box = await page.locator('section.pane[data-title="dayview"] .strip').evaluate((el) => el.clientHeight);
			assert.notEqual(box - 15, 120, 'the box happens to derive 120, so this proves nothing');
			assert.deepEqual(app.errors, []);
		});
	});
});

describe('a layout naming the widget that went and came back', () => {
	const ROOT_BAR = { edge: 'top', align: 'start' };

	test('a saved layout whose widget is `input` opens as an input widget, unmodified, and saves back as one', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await layouts(app, 'saveLayout', {
				version: 1,
				name: 'mine',
				root: {
					id: 'm',
					active: 0,
					bar: ROOT_BAR,
					tabs: [{ id: 'm-tab', content: { id: 'm-w', kind: 'input', settings: {} } }]
				}
			});
			await pick(page, 'mine');
			await until(() => rootTabs(page), (t) => t.includes('mine'));
			assert.deepEqual(await titles(page), ['input']);
			// The widget itself, not just a pane wearing its name: the command
			// line is in the body, and it is not the bar's.
			await page.locator('section.pane[data-title="input"] .input-widget input').waitFor();

			// Read, not edited: no dot on the tab, and nothing logged at the page.
			assert.deepEqual(await dotted(page), []);
			assert.deepEqual(app.errors, []);

			// And `input` is still what a save writes back — nothing rewrites
			// the id on the way out any more.
			await act(page, 'save');
			await until(
				() => layouts(app, 'loadLayout', 'mine'),
				(saved) => saved.root.tabs[0].content.kind === 'input'
			);
		});
	});

	test('/input opens the input widget, and the chooser offers it again', async () => {
		await shared.wipe(async (app) => {
			const { page } = app;
			await app.goto('/input');
			await until(() => rootTabs(page), (t) => t.includes('input'));
			assert.equal(await activeRootTab(page), 'input');
			assert.deepEqual(await titles(page), ['input']);
			await page.locator('.input-widget input').waitFor();
			assert.ok((await offered(page)).includes('input'), 'the chooser does not offer the input widget');
		});
	});
});

describe('on a phone', () => {
	test('the bottom bar is on screen and the root bar scrolls sideways rather than wrapping', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const screen = await page.evaluate(() => document.documentElement.clientHeight);
			const bar = await page.locator('.prompt-bar').boundingBox();
			assert.ok(bar.y >= 0 && bar.y + bar.height <= screen + 1, `the bottom bar is off-screen: ${JSON.stringify(bar)} on ${screen}`);
			// A phone starts FOLDED (da-ckw3), so the bar is the input row and
			// the hairline alone — 2.9rem + 1px — and not the open bar's 105.
			// The floor is still a fingertip: the row carries the mark, the
			// clock, the `❯`, the box and the caret, all of them 24px controls.
			assert.ok(bar.height >= 44, `the bottom bar is ${bar.height}px tall on a phone`);
			const input = await page.locator('.prompt-bar input').boundingBox();
			assert.ok(input.width >= 100, `the prompt is ${input.width}px wide`);

			// A pill after the input must not squeeze it away at phone width.
			await page.evaluate(() => window.datum.bar.setPills([{ id: 'when', text: '-t 09:00', kind: 'time' }]));
			const withPill = await page.locator('.prompt-bar input').boundingBox();
			assert.ok(withPill.width >= 100, `the prompt is ${withPill.width}px wide with a pill`);
			await page.evaluate(() => window.datum.bar.removePill('when'));

			// Six through the shell's own door in one go rather than six trips
			// through the `+`, each drawing a widget the next hides: what this
			// reads is the strip holding eight, not how they came. The last
			// through the `+`, so the chooser is still driven at phone width
			// (canary-deepseek-v4-pro on PR #239).
			const names = ['nowview', 'tail', 'stats', 'custom', 'cli', 'fields'];
			await page.evaluate((all) => Promise.all(all.map((n) => window.datum.shell.open(n))), names);
			await until(() => rootTabs(page), (t) => names.every((n) => t.includes(n)));
			await pick(page, 'dayview');
			await until(() => rootTabs(page), (t) => t.includes('dayview'));
			await app.idle();
			const row = await page.locator('.root-bar').evaluate((el) => {
				const strip = el.querySelector('.tabs');
				const box = (s) => el.querySelector(s).getBoundingClientRect();
				return {
					height: el.getBoundingClientRect().height,
					scrolls: strip.scrollWidth > strip.clientWidth,
					tabs: [...el.querySelectorAll('[role="tab"]')].map((t) => Math.round(t.getBoundingClientRect().top)),
					// The `+` stays on screen whatever the strip holds.
					acts: [box('button[aria-label="open a layout"]').right],
					screen: document.documentElement.clientWidth
				};
			});
			assert.ok(row.scrolls, 'eight tabs did not overflow a phone');
			assert.equal(new Set(row.tabs).size, 1, `the tabs wrapped: ${JSON.stringify(row.tabs)}`);
			assert.ok(row.height < 48, `the root bar is ${row.height}px tall`);
			for (const right of row.acts) assert.ok(right <= row.screen, `a control is off-screen at ${right}`);
		}, { viewport: { width: 390, height: 844 }, device: devices['iPhone 12'] });
	});
});

// da-fu29p: the spec widget's work went to the field widget, so a layout
// saved with one opens as the other (`RETIRED_KINDS`) — not as an untitled
// empty pane — and the field it was saved on is no setting of the new one.
// da-rgyq8: and the field widget is the setup widget, so one saved with
// either opens as setup.
describe('a layout saved with the spec or the field widget', () => {
	for (const kind of ['spec', 'field']) test(`${kind} opens as the setup widget`, async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await layouts(app, 'saveLayout', {
				version: 1,
				name: 'old',
				root: {
					id: 'o',
					active: 0,
					bar: { edge: 'top', align: 'start' },
					tabs: [{ id: 'o-tab', content: { id: 'o-w', kind, settings: { field: 'mood' } } }]
				}
			});
			await pick(page, 'old');
			await until(() => rootTabs(page), (t) => t.includes('old'));
			await page.locator('section.pane[data-title="setup"] .field-form input.field').waitFor();
			assert.deepEqual(await dotted(page), [], 'reading a retired kind is not an edit');
			assert.deepEqual(app.errors, []);
		});
	});
});
