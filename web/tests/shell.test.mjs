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

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium, devices } from 'playwright';
import { inWorker, startApp, stopApp, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';
import { addTab, chooseRoot, chooser, fill, menuLines, offers, openChooser, rootMenuButton, settled } from './support/layout.mjs';

before(startApp);
after(stopApp);

const DESKTOP = { viewport: { width: 1280, height: 900 } };

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

/** Open a pane's menu — its title — and choose an item by its exact label. */
async function choose(page, id, label) {
	await page.locator(`section[data-pane="${id}"] .titlebar button.title`).first().click();
	await page.getByRole('menuitem', { name: label, exact: true }).click();
	await page.locator('[role="menu"]').waitFor({ state: 'detached' });
}

/** The four acts, as the root menu names them (da-wjob). */
const ACT = { save: 'Save layout', restore: 'Restore layout', 'save-as': 'Save layout as…', lock: 'Lock layout', unlock: 'Unlock layout' };

/** Run one of the layout's acts from the root menu. */
const act = (page, name) => chooseRoot(page, ACT[name]);

/** The root menu's items, open: label, whether it is marked, and its colour. */
async function rootItems(page) {
	await rootMenuButton(page).click();
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
const receipt = (page) => page.locator('.prompt-bar .doc .receipt');

/** The result area's raw lines — what a command with no action echo shows. */
const rawResult = (page) => page.locator('.prompt-bar .result .raw');

const PRONOUNCED = 'rgb(255, 209, 102)';

describe('the shell', () => {
	test('no header; the root bar shows day active; the bottom bar has the brand, beta, the prompt, an empty result area', async () => {
		await withApp(async ({ page, errors }) => {
			assert.equal(await page.locator('header').count(), 0, 'the header is still there');
			assert.equal(await page.locator('aside.beta').count(), 0, 'the beta banner is still there');
			assert.deepEqual(await rootTabs(page), ['day']);
			assert.equal(await activeRootTab(page), 'day');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'command', 'fields']);

			const bar = page.locator('.prompt-bar');
			assert.equal(await bar.locator('.brand').innerText(), 'datum.day');
			assert.equal(await bar.locator('.tag').evaluate((el) => el.textContent), 'beta');
			assert.equal(await bar.locator('input').count(), 1);
			// Nothing has run, so the result area has nothing to draw.
			assert.equal(await receipt(page).count(), 0);
			assert.equal(await rawResult(page).count(), 0);
			// The build stamp is in the GEAR's menu now (da-pp1a): version ·
			// sha, the bottom item and disabled. It was the bottom bar's
			// until da-wjob and the root bar's until that bead; neither bar
			// draws it any more.
			assert.equal(await bar.locator('.build').count(), 0, 'the build stamp is still on the bottom bar');
			assert.equal(await page.locator('.root-bar .build').count(), 0, 'the build stamp is still on the root bar');
			// And it is all the gear holds: «Reset all layouts…», which
			// deleted every saved layout on every device, is gone (da-luwe)
			// — `Reset layout` on a built-in's own tab is what replaced it.
			const gear = await gearItems(page);
			assert.deepEqual(gear.map((i) => i.label.replace(/^\S+ · \S+$/, 'the build stamp')), ['the build stamp']);
			assert.deepEqual(gear.map((i) => i.disabled), [true]);
			// The acts are in the active tab's own menu, no icon row.
			assert.equal(await page.locator('.root-bar .act').count(), 0, 'the icon row is still there');
			assert.deepEqual(
				(await rootItems(page)).map((i) => i.label),
				// Close last, after `export backup…` (da-ygc9), wearing its ×.
				// `day` is a built-in, so it has the fifth act (da-luwe).
				[
					'Choose…',
					'Break out ← → ↑ ↓',
					'Save layout',
					'Restore layout',
					'Reset layout',
					'Save layout as…',
					'Lock layout',
					'export backup…',
					'× Close tab'
				]
			);
			assert.deepEqual(errors, []);
		}, DESKTOP);
	});

	test('+ lists day, the widget kinds and the saved layouts; nowview opens a tab; a reload keeps it, another device starts fresh', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.deepEqual(await offered(page), ['day', 'dayview', 'repl', 'nowview', 'tail', 'command', 'stats', 'fields', 'aliases']);

			await pick(page, 'nowview');
			await until(() => rootTabs(page), (t) => t.includes('nowview'));
			assert.deepEqual(await rootTabs(page), ['day', 'nowview']);
			assert.equal(await activeRootTab(page), 'nowview');
			await page.locator('section.pane[data-title="nowview"] svg.svg').waitFor();
			assert.deepEqual(await titles(page), ['nowview']);
			// Two root tabs are the root bar's alone: the root pane draws no strip of its own.
			assert.equal(await page.locator('section.root > .titlebar').count(), 0, 'the root drew a second strip');

			await app.reload();
			assert.deepEqual(await rootTabs(page), ['day', 'nowview']);
			assert.equal(await activeRootTab(page), 'nowview');

			// A saved layout is offered by name, after day and before the kinds.
			page.once('dialog', (dialog) => dialog.accept('mine'));
			await act(page, 'save-as');
			await until(() => rootTabs(page), (t) => t.includes('mine'));
			// The chooser reads the saved layouts each time it opens, so the
			// save is on the list the first time it is asked.
			assert.deepEqual(await offered(page), ['day', 'mine', 'dayview', 'repl', 'nowview', 'tail', 'command', 'stats', 'fields', 'aliases']);
		}, DESKTOP);

		// Which tabs are open is per device (vividn: the selection is not synced).
		await withApp(async ({ page }) => {
			assert.deepEqual(await rootTabs(page), ['day']);
		}, DESKTOP);
	});
});

describe('a URL', () => {
	test('/nowview, /stats and /now open that tab; a reload on /stats lands on stats', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.goto('/nowview');
			assert.deepEqual(await rootTabs(page), ['day', 'nowview']);
			assert.equal(await activeRootTab(page), 'nowview');
			assert.equal(new URL(page.url()).pathname, '/', 'the URL did not go back to /');

			await app.goto('/stats');
			assert.equal(await activeRootTab(page), 'stats');
			await page.locator('section.pane[data-title="stats"]').waitFor();
			await app.reload();
			assert.equal(await activeRootTab(page), 'stats');

			// The old route is an alias: the same tab, not a second one.
			await app.goto('/now');
			assert.deepEqual(await rootTabs(page), ['day', 'nowview', 'stats']);
			assert.equal(await activeRootTab(page), 'nowview');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// The widget bar became the fields widget (da-c7u8); its old name is an alias.
	test('/fields, /widgets and /widgetbar open the fields widget alone', async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const path of ['/fields', '/widgets', '/widgetbar']) {
				await app.goto(path);
				assert.deepEqual(await rootTabs(page), ['day', 'fields']);
				assert.equal(await activeRootTab(page), 'fields');
				await page.locator('section.pane[data-title="fields"] button.track').waitFor();
				assert.deepEqual(await titles(page), ['fields']);
			}
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// The widget's files are called Fields* now (da-850j) but its KIND ID is
	// `widgetbar` still, because that id is the key in every layout already
	// saved. This is the JSON an older build wrote: it must still draw.
	test('a layout saved under the old kind id still opens the fields widget', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await layouts(app, 'saveLayout', {
				version: 1,
				name: 'old',
				root: { id: 'o', tabs: [{ id: 'o-tab', content: { id: 'o-w', kind: 'widgetbar', settings: {} } }], active: 0, bar: { edge: 'top', align: 'start' } }
			});
			await app.goto('/old');
			assert.deepEqual(await titles(page), ['fields']);
			await page.locator('section.pane[data-title="fields"] button.track').waitFor();
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('/no-such-thing is a quiet notice with a link home', async () => {
		await withApp(async (app) => {
			await app.page.goto(new URL('/no-such-thing', app.page.url()).href);
			const notice = app.page.locator('main .notice');
			await notice.waitFor();
			assert.match(await notice.innerText(), /no-such-thing/);
			assert.equal(await notice.locator('a[href="/"]').count(), 1);
			assert.deepEqual(await rootTabs(app.page), ['day']);
		}, DESKTOP);
	});

	// The deployed site serves every path from adapter-static's fallback
	// (svelte.config.js), the shape of a plain bucket — so `/stats` has to
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
		await withApp(async (app) => {
			const { page } = app;
			assert.deepEqual(await dotted(page), []);
			const quiet = (await rootItem(page, 'Save layout')).color;

			await choose(page, 'tail', 'split: a new pane to the right');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'empty', 'command', 'fields']);
			assert.deepEqual(await dotted(page), ['day']);
			// The dot's meaning, in the menu beside it: exactly the two acts
			// that answer it are marked, and in the dot's own amber.
			const marked = await rootItems(page);
			assert.deepEqual(marked.filter((i) => i.marked).map((i) => i.label), ['Save layout', 'Restore layout']);
			for (const i of marked.filter((i) => i.marked)) assert.equal(i.color, PRONOUNCED, `${i.label} is ${i.color}`);

			await act(page, 'restore');
			await until(() => dotted(page), (d) => d.length === 0);
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'command', 'fields']);
			assert.deepEqual((await rootItems(page)).filter((i) => i.marked), []);
			assert.equal((await rootItem(page, 'Save layout')).color, quiet);

			await choose(page, 'tail', 'split: a new pane to the right');
			await fill(page, 'nowview');
			await page.locator('section.pane[data-title="nowview"]').nth(1).locator('svg.svg').waitFor();
			await act(page, 'save');
			await until(() => dotted(page), (d) => d.length === 0);
			assert.deepEqual(
				(await layouts(app, 'listLayouts')).map((l) => l.name),
				['day']
			);
			const saved = await layouts(app, 'loadLayout', 'day');
			// The save is the tree as shown: the tail's row split, nowview beside it.
			const kinds = JSON.stringify(saved.root).match(/"kind":"nowview"/g);
			assert.equal(kinds.length, 2, 'the second nowview was not saved');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'nowview', 'command', 'fields']);
		}, DESKTOP);

		// A second device loads the saved tree.
		// (A second CONTEXT is a second OPFS, so the save is carried across
		// by hand: what this checks is that a saved `day` shadows the built-in.)
	});

	test('a saved day is what another device opens', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await choose(page, 'command', 'Close pane');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'fields']);
			await act(page, 'save');
			await until(() => dotted(page), (d) => d.length === 0);
			const saved = await layouts(app, 'loadLayout', 'day');

			// Another tab of the same browser, opened fresh: no draft of its
			// own (the draft is per name, and this one was cleared by the
			// save), so what it shows is the save.
			await page.evaluate(() => localStorage.removeItem('datum.layout.root.v1'));
			const other = await app.newTab();
			assert.deepEqual(await titles(other.page), ['dayview', 'repl', 'nowview', 'tail', 'fields']);
			assert.deepEqual(await dotted(other.page), []);
			assert.deepEqual(await layouts(other, 'loadLayout', 'day'), saved);
		}, DESKTOP);
	});

	test('save as makes a mine tab beside day, and both are saved', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await choose(page, 'tail', 'split: a new pane to the right');
			page.once('dialog', (dialog) => dialog.accept('mine'));
			await act(page, 'save-as');
			await until(() => rootTabs(page), (t) => t.includes('mine'));
			assert.deepEqual(await rootTabs(page), ['mine']);
			assert.deepEqual(await dotted(page), []);
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'empty', 'command', 'fields']);
			assert.deepEqual(
				(await layouts(app, 'listLayouts')).map((l) => l.name),
				['mine']
			);
			// day was never saved and keeps no draft of the edit; opening it
			// again is the default.
			await pick(page, 'day');
			await until(() => rootTabs(page), (t) => t.includes('day'));
			assert.deepEqual(await rootTabs(page), ['mine', 'day']);
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'command', 'fields']);
			assert.deepEqual(await dotted(page), []);

			// Both open, both tail panes editable on their own: a split in
			// mine's tail leaves day's alone.
			await page.locator('.root-bar [role="tab"]', { hasText: 'mine' }).click();
			await addTab(page, 'tail', 'aliases');
			assert.deepEqual(await page.locator('section.pane [role="tablist"] [role="tab"]').allInnerTexts(), ['tail', 'aliases']);
			await page.locator('.root-bar [role="tab"]', { hasText: 'day' }).click();
			assert.equal(await page.locator('section.pane [role="tablist"]').count(), 0, 'the tab strip leaked into day');
			assert.deepEqual(await dotted(page), ['mine']);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('lock is per device: kept across a reload here, absent on another device and in the save; never the dot', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const locked = () => page.evaluate(() => !!window.datum.layout.current().root.tabs[0].locked);
			assert.equal(await locked(), false);
			await act(page, 'lock');
			await until(locked, (l) => l === true);
			// Under the lock the menu is the acts alone (the pane rule,
			// da-crp5): nothing to split or close, but Unlock is reachable.
			assert.deepEqual(
				(await rootItems(page)).map((i) => i.label),
				['Save layout', 'Restore layout', 'Reset layout', 'Save layout as…', 'Unlock layout']
			);
			assert.deepEqual(await dotted(page), [], 'the lock alone showed the modified dot');
			assert.equal(await page.evaluate(() => localStorage.getItem('datum.layout.locked.v1.day')), '1');

			await app.reload();
			assert.equal(await locked(), true);
			assert.ok((await rootItems(page)).some((i) => i.label === 'Unlock layout'));

			// Saved while locked: the entry carries no lock.
			await act(page, 'save');
			await until(() => layouts(app, 'listLayouts'), (l) => l.length === 1);
			const saved = await layouts(app, 'loadLayout', 'day');
			assert.doesNotMatch(JSON.stringify(saved), /"locked"/, 'the lock went into the entry');
			assert.deepEqual(await dotted(page), []);

			await act(page, 'unlock');
			await until(locked, (l) => l === false);
			assert.ok((await rootItems(page)).some((i) => i.label === 'Lock layout'));
			assert.equal(await page.evaluate(() => localStorage.getItem('datum.layout.locked.v1.day')), null);
		}, DESKTOP);

		await withApp(async ({ page }) => {
			assert.equal(await page.evaluate(() => !!window.datum.layout.current().root.tabs[0].locked), false);
			assert.ok((await rootItems(page)).some((i) => i.label === 'Lock layout'));
		}, DESKTOP);
	});

	test('an unmodified restore lists the versions, and one of them can be picked', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await choose(page, 'command', 'Close pane');
			await act(page, 'save');
			await until(() => dotted(page), (d) => d.length === 0);
			await choose(page, 'nowview', 'Close pane');
			await act(page, 'save');
			await until(() => dotted(page), (d) => d.length === 0);
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'tail', 'fields']);

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
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'fields']);
			assert.deepEqual(await dotted(page), []);
		}, DESKTOP);
	});

	// The review thread on #168: the cross-tab refresh decided "unmodified"
	// before its awaited load, so an edit made meanwhile was overwritten.
	test('an edit in tab A survives a save from tab B, dot and all', async () => {
		await withApp(async (a) => {
			const b = await a.newTab();
			await choose(a.page, 'tail', 'split: a new pane to the right');
			assert.deepEqual(await dotted(a.page), ['day']);
			assert.deepEqual(await dotted(b.page), []);

			const heard = await a.spyOnRunner();
			await choose(b.page, 'command', 'Close pane');
			await act(b.page, 'save');
			await until(() => dotted(b.page), (d) => d.length === 0);
			// A hears the save — it reads the saved layout back from
			// `_internal` — and once that has answered, its baseline has
			// moved and its edit has not.
			await until(heard, (argvs) => argvs.some((argv) => argv[0] === '--db' && argv[1] === '_internal'));
			await a.idle();
			await settled(a.page);
			assert.deepEqual(await titles(a.page), ['dayview', 'repl', 'nowview', 'tail', 'empty', 'command', 'fields']);
			assert.deepEqual(await dotted(a.page), ['day']);
			// And a tab that was not edited follows: a third tab, and B itself.
			assert.deepEqual(await titles(b.page), ['dayview', 'repl', 'nowview', 'tail', 'fields']);
		}, DESKTOP);
	});
});

describe('the bottom bar', () => {
	test('a typed line is run and receipted with the hid from the listing; a press ghosts in the field; foo is an error', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const input = page.locator('.prompt-bar input');
			// Tracked, so the fields widget has a button to press (da-c7u8).
			await app.run(['spec', 'set', 'coffee', '--default-verb', 'occur']);
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
			// As if typed at the REPL: it is in its scrollback.
			assert.deepEqual(await app.echoes(), ['occur coffee']);

			// A widget-bar press: the bar's field GHOSTS the command — beside
			// the prompt, never as the box's value (da-jprg) — and the
			// receipt is the press's.
			await page.locator('.bar button.widget', { hasText: 'coffee' }).click();
			await page.waitForFunction(
				() => document.querySelector('.prompt-bar .ghost')?.textContent === 'occur coffee'
			);
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
			await page.locator('.prompt-bar .result.error').waitFor();
			assert.equal(await receipt(page).count(), 0, 'an error drew a document');
			const said = await app.runLine('foo');
			const firstLine = said.stderr.replace(/\x1b\[[0-9;]*m/g, '').split('\n').find((l) => l.trim()).trim();
			assert.equal((await rawResult(page).innerText()).split('\n')[0], firstLine);
			assert.equal(
				await rawResult(page).evaluate((el) => getComputedStyle(el).color),
				'rgb(244, 100, 90)'
			);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	// The page's one command line and its status line (da-rfs9): tall
	// enough to stand out — absolute heights, not "bigger than before" —
	// and the REPL is only the scrollback it types into. da-xfa4 gave it a
	// result area above the input row and fixed the whole thing at 6rem,
	// so the window is around that number and still absolute.
	test('stands out at 92–104px with a ≥1.05rem prompt; the REPL has no prompt, and the bar and a chip land in its scrollback', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const { height, font, rem } = await page.evaluate(() => ({
					height: document.querySelector('.prompt-bar').getBoundingClientRect().height,
					font: parseFloat(getComputedStyle(document.querySelector('.prompt-bar input')).fontSize),
					rem: parseFloat(getComputedStyle(document.documentElement).fontSize)
				}));
				assert.ok(height >= 92 && height <= 104, `the bottom bar is ${height}px tall at 1280x800`);
				assert.ok(font >= 1.05 * rem, `the prompt's input is ${font}px on a ${rem}px rem`);

				const repl = page.locator('section[data-pane="repl"]');
				assert.equal(await repl.count(), 1);
				assert.equal(await repl.locator('input, .line').count(), 0, 'the REPL still has a prompt of its own');

				// Tracked, so the fields widget has a button to press (da-c7u8).
				await app.run(['spec', 'set', 'coffee', '--default-verb', 'occur']);
				await app.submit('occur coffee');
				await until(() => receipt(page).innerText(), (t) => /^CREATE \S+$/.test(t));
				assert.deepEqual(await app.echoes(), ['occur coffee']);
				// The scrollback carries the command's DOCUMENT, not its printed
				// lines: the web asks every command it runs for `--show json`
				// (da-xfa4), so both surfaces draw the same sections.
				assert.match(
					await page.locator('.repl .doc').first().innerText(),
					/CREATE[\s\S]*coffee/,
					"the CLI's action is not in the scrollback"
				);

				await app.pressWidget('coffee');
				assert.deepEqual(await app.echoes(), ['occur coffee', 'occur coffee']);

				await app.submit('tail');
				const input = page.locator('.prompt-bar input');
				await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === '');
				await input.press('ArrowUp');
				assert.equal(await input.inputValue(), 'tail');
			},
			{ viewport: { width: 1280, height: 800 } }
		);
	});

	test('memory-only storage turns the bar orange, with the storage sentence as its title', async () => {
		await withApp(
			async ({ page }) => {
				const bar = page.locator('.prompt-bar');
				await page.waitForFunction(() => document.querySelector('.prompt-bar').classList.contains('alarm'));
				assert.equal(await bar.evaluate((el) => getComputedStyle(el).borderTopColor), 'rgb(232, 130, 12)');
				assert.match(await bar.getAttribute('title'), /held in memory only/);
			},
			{ ...DESKTOP, prepare: inWorker('FileSystemFileHandle.prototype.createSyncAccessHandle = () => new Promise(() => {});') }
		);
	});

	test('a pill set programmatically shows, its x removes it', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.evaluate(() => window.datum.bar.setPills([{ id: 'when', text: '-t 0700', kind: 'time' }]));
			const pill = page.locator('.prompt-bar .pill');
			assert.equal(await pill.evaluate((el) => el.firstChild.textContent), '-t 0700');
			await pill.locator('button.remove').click();
			assert.equal(await page.locator('.prompt-bar .pill').count(), 0);
			assert.deepEqual(await page.evaluate(() => window.datum.bar.pills()), []);
		}, DESKTOP);
	});

	test('a submit runs typed + pills, then an unlocked pill clears on success', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const seen = await app.spyOnRunner();
			await page.evaluate(() => window.datum.bar.setPills([{ id: 'when', text: '-t 0700', kind: 'time' }]));
			const input = page.locator('.prompt-bar input');
			await input.fill('occur tea');
			await input.press('Enter');
			// After, not before: the CLI takes no flag before its verb (da-c7u8).
			await until(seen, (argvs) => argvs.some((a) => a[0] === 'occur'));
			// `--show json` at the end is the web's own (da-xfa4).
			assert.ok(
				(await seen()).some(
					(a) => JSON.stringify(a) === JSON.stringify(['occur', 'tea', '-t', '0700', '--show', 'json'])
				),
				JSON.stringify(await seen())
			);
			await page.waitForFunction(() => document.querySelectorAll('.prompt-bar .pill').length === 0);
			assert.deepEqual(await page.evaluate(() => window.datum.bar.pills()), []);
		}, DESKTOP);
	});

	test('a locked pill survives a successful press; an unlocked one does not', async () => {
		await withApp(async (app) => {
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
					['occur', 'tea', '-t', '0700', '--show', 'json'],
					['occur', 'tea', '-t', '0700', '--show', 'json']
				]
			);
			assert.equal(await page.locator('.prompt-bar .pill').count(), 1, 'the locked pill survives');
		}, DESKTOP);
	});

	test('a failed command leaves its pill in place', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.evaluate(() => window.datum.bar.setPills([{ id: 'when', text: '-t 0700', kind: 'time' }]));
			const input = page.locator('.prompt-bar input');
			await input.fill('foo');
			await input.press('Enter');
			await page.locator('.prompt-bar .result.error').waitFor();
			assert.equal(await page.locator('.prompt-bar .pill').count(), 1, 'a failed command keeps the pill');
		}, DESKTOP);
	});

	// The pills moved INSIDE the prompt in da-xfa4 — vividn: "move the -t
	// pills to be just slightly to the right of the >, so that they are
	// visible as you type text" — so what "after the input" means is no
	// longer DOM order but where they are drawn: past the caret, over the
	// box, and inside it.
	test('the pills are drawn inside the box, past the caret', async () => {
		await withApp(async (app) => {
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
		}, DESKTOP);
	});

	test('the hint opens the beta text and the storage sentence; Escape closes it', async () => {
		await withApp(async ({ page }) => {
			assert.equal(await page.locator('.beta-hint').count(), 0);
			await page.locator('.prompt-bar .hint-button').click();
			const text = await page.locator('.beta-hint p').innerText();
			assert.match(text, /^datum\.day is early software\. Your entries are stored in this browser only/);
			await page.keyboard.press('Escape');
			await page.locator('.beta-hint').waitFor({ state: 'detached' });
			assert.equal(await page.locator('.prompt-bar .hint-button').getAttribute('aria-expanded'), 'false');
		}, DESKTOP);
	});
});

describe('the cycle guard', () => {
	test('a layout that embeds itself loads as a notice, not a recursion', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// A saved `day` whose tail is a tab holding `day`.
			const tree = await page.evaluate(() => window.datum.shell.extract(window.datum.layout.current().root.tabs[0]));
			const tail = JSON.stringify(tree).replace(/\{"id":"tail-tab","content":\{"id":"tail-widget","kind":"tail","settings":\{\}\}\}/, '{"id":"tail-tab","layout":"day","content":{"id":"tail-widget","kind":"empty","settings":{}}}');
			assert.notEqual(tail, JSON.stringify(tree), 'the tail tab was not found to replace');
			await layouts(app, 'saveLayout', JSON.parse(tail));
			await app.reload();
			// The tab is the `day` layout's, so it wears that name (da-9puz);
			// what it holds, since it cannot hold itself, is the notice.
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'day', 'command', 'fields']);
			assert.match(await app.paneText('day'), /cannot hold itself/);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('the chooser offers day inside a pane, and a nested layout draws', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// mine: the nowview alone, then embedded in day's tail.
			await layouts(app, 'saveLayout', { version: 1, name: 'mine', root: { id: 'm', tabs: [{ id: 'm-tab', content: { id: 'm-w', kind: 'nowview', settings: {} } }], active: 0, bar: { edge: 'top', align: 'start' } } });
			await choose(page, 'tail', 'split: a new pane to the right');
			await openChooser(page);
			assert.deepEqual(await chooser(page).locator('button.layout').allInnerTexts(), ['mine'], 'day, open above, must not be offered');
			await chooser(page).locator('button', { hasText: exactly('mine') }).click();
			// The pane wears the LAYOUT's name, not its one widget's (da-9puz).
			await page.locator('section.pane[data-title="mine"]').locator('svg.svg').waitFor();
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'mine', 'command', 'fields']);
		}, DESKTOP);
	});
});

describe('the one chooser', () => {
	// da-uyx1: the empty pane's button and the root bar's `+` open the SAME
	// modal, so what they offer cannot drift apart — one list, one order.
	test("the root + and an empty pane offer the same items in the same order, minus the pane's ancestors", async () => {
		await withApp(async (app) => {
			const { page } = app;
			// A saved layout, so the Layouts section holds more than `day`.
			await layouts(app, 'saveLayout', { version: 1, name: 'mine', root: { id: 'm', tabs: [{ id: 'm-tab', content: { id: 'm-w', kind: 'nowview', settings: {} } }], active: 0, bar: { edge: 'top', align: 'start' } } });
			const fromRoot = await offered(page);
			assert.deepEqual(fromRoot, ['day', 'mine', 'dayview', 'repl', 'nowview', 'tail', 'command', 'stats', 'fields', 'aliases']);

			await choose(page, 'tail', 'split: a new pane to the right');
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'empty', 'command', 'fields']);
			await openChooser(page);
			// The same list, in the same order — less `day`, which is open
			// above this pane and may not be offered inside itself.
			assert.deepEqual(await offers(page), fromRoot.filter((n) => n !== 'day'));

			// The keyboard is inside the dialog, on its first item, and Tab
			// walks the rest in the order they are listed.
			const focused = () => page.evaluate(() => document.activeElement?.textContent?.trim());
			assert.equal(await page.evaluate(() => document.activeElement?.closest('dialog.chooser') !== null), true);
			assert.equal(await focused(), fromRoot[1]);
			await page.keyboard.press('Tab');
			assert.equal(await focused(), fromRoot[2]);

			// Escape closes it, hands focus back to the button that opened
			// it, and changes nothing: the pane is still empty.
			await page.keyboard.press('Escape');
			await chooser(page).waitFor({ state: 'detached' });
			assert.equal(await focused(), 'Choose Widget/Layout');
			await settled(page);
			assert.deepEqual(await titles(page), ['dayview', 'repl', 'nowview', 'tail', 'empty', 'command', 'fields']);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

// A kind's id is a saved-layout key (docs/web-conventions.md #10), so a
// widget cannot simply go: every layout and draft already stored on a
// device still names it. The `input` widget went (da-22vg) — the bottom
// bar is the prompt it was — and what named it opens as the REPL, the
// widget that does what it did and more, rather than as an untitled
// empty pane.
describe('a layout naming a widget that went', () => {
	const ROOT_BAR = { edge: 'top', align: 'start' };

	test('a saved layout whose widget is `input` opens as a REPL, unmodified, and saves back as one', async () => {
		await withApp(async (app) => {
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
			assert.deepEqual(await titles(page), ['repl']);
			// The REPL itself, not just a pane wearing its name: its own
			// empty-scrollback line is in the body.
			await page.locator('section.pane[data-title="repl"] p.quiet', { hasText: 'type a command' }).waitFor();

			// Migrated against its own baseline, so reading it is not an
			// edit: no dot on the tab, and nothing logged at the page.
			assert.deepEqual(await dotted(page), []);
			assert.deepEqual(app.errors, []);

			// And the new name is what a save writes back: the migration
			// happens once per layout, not once per load.
			await act(page, 'save');
			await until(
				() => layouts(app, 'loadLayout', 'mine'),
				(saved) => saved.root.tabs[0].content.kind === 'repl'
			);
		}, DESKTOP);
	});

	test('/input opens the REPL, and the chooser no longer offers the widget', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.goto('/input');
			await until(() => rootTabs(page), (t) => t.includes('repl'));
			assert.equal(await activeRootTab(page), 'repl');
			assert.deepEqual(await titles(page), ['repl']);
			assert.equal(await page.locator('.input-widget').count(), 0, 'the input widget is still drawn');
			assert.ok(!(await offered(page)).includes('input'), 'the chooser still offers the input widget');
		}, DESKTOP);
	});
});

describe('on a phone', () => {
	test('the bottom bar is on screen and the root bar scrolls sideways rather than wrapping', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const screen = await page.evaluate(() => document.documentElement.clientHeight);
			const bar = await page.locator('.prompt-bar').boundingBox();
			assert.ok(bar.y >= 0 && bar.y + bar.height <= screen + 1, `the bottom bar is off-screen: ${JSON.stringify(bar)} on ${screen}`);
			assert.ok(bar.height >= 52, `the bottom bar is ${bar.height}px tall on a phone`);
			const input = await page.locator('.prompt-bar input').boundingBox();
			assert.ok(input.width >= 100, `the prompt is ${input.width}px wide`);

			// A pill after the input must not squeeze it away at phone width.
			await page.evaluate(() => window.datum.bar.setPills([{ id: 'when', text: '-t 09:00', kind: 'time' }]));
			const withPill = await page.locator('.prompt-bar input').boundingBox();
			assert.ok(withPill.width >= 100, `the prompt is ${withPill.width}px wide with a pill`);
			await page.evaluate(() => window.datum.bar.removePill('when'));

			for (const name of ['nowview', 'tail', 'stats', 'command', 'repl', 'fields', 'dayview']) {
				await pick(page, name);
				await until(() => rootTabs(page), (t) => t.includes(name));
			}
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
