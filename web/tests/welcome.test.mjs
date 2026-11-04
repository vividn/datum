// A first visit opens welcome · setup · collect (da-u7vz8; the second tab was `track` until da-rgyq8).
//
// vividn, 2026-10-08: «let's actually start on first open with a different
// set of tabs. So maybe a welcome widget as first tab that explains what
// datum is, some basic ideas to be aware of, and a basic user flow. The
// second tab can be the track/spec widget directly … Then comes the
// "collect" tab, which is like "main" today», and «All tabs are closeable,
// and welcome would be no different. Maybe the welcome widget could explain
// how to close tabs».
//
// So: a device that has never been here opens the three, `welcome` in
// front; `setup` is the next tab; it closes the way it says it does,
// leaving the other two, and the `+` brings it back; and a device that has
// been here keeps the tabs it had. What the WORDS claim about the page is
// done to the page here, by the controls they name.
//
// And it closes by a button of its own (da-zhz0u). vividn, 2026-10-09:
// «maybe start unlocked. Let's put a button at the bottom of the welcome
// widget that closes the widget too … I am imagining that the welcome
// widget will have some scrolling and reading rather than all fitting on
// one page».
//
// And it is short (da-uzyih.19). vividn, 2026-10-22: «trim down the welcome
// page considerably after the tutorial is in … it should mostly just welcome
// you, give a tag line, tell you to point your agent there, maybe link to the
// repo? and give docs». What it taught is the tutorial's Introduction
// (tutorial.test.mjs reads it there).

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ON_COLLECT, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, activeTab, addTab, chooser, enterLayoutMode, exactly, leaveLayoutMode, manage, manageRoot, pane, rootMenuButton, titledPane, titles } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const FIRST = ['welcome', 'setup', 'collect', 'dayview'];
/** What those tabs read: `collect` is called «collection» (da-jzica). */
const SHOWN = ['welcome', 'setup', 'collection', 'dayview'];
const FRESH_DESKTOP = { ...DESKTOP, home: null, desk: null };
const FRESH_PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, home: null };

/** The names the root bar's tabs hold, in order. */
const rootNames = (page) =>
	page.evaluate(() => window.datum.shell.rootTabs(window.datum.layout.current()).map((t) => t.layout));

/** The widget kind the root tab in front shows, if it shows one. */
const front = (page) =>
	page.evaluate(() => {
		const { tabs, active } = window.datum.layout.current().root;
		return [tabs[active].layout, tabs[active].content.kind];
	});

/** The welcome widget's own text. */
const words = (page) => page.locator('.host.welcome').innerText();

/** The widget's last button (da-zhz0u). */
const closeWelcome = (page) => page.getByRole('button', { name: 'Close welcome', exact: true });

describe('a first visit', () => {
	for (const [what, device, collect] of [
		['a desktop', FRESH_DESKTOP, ['dayview', 'tail', 'collect', 'nowview', 'stats']],
		['a phone', FRESH_PHONE, ['tail', 'nowview', 'collect']]
	])
		test(`${what} opens welcome · setup · collect on welcome; setup is the setup widget; collect is the page it opened before`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				assert.deepEqual(await rootNames(page), FIRST);
				assert.deepEqual(await page.locator('.root-bar [role="tab"]').allInnerTexts(), SHOWN);
				assert.deepEqual(await front(page), ['welcome', 'welcome']);
				// Nothing about it is an edit: no dot, and no offer of a new
				// default to a device that starts on it.
				assert.equal(await page.locator('.root-bar [role="tab"].modified').count(), 0);
				assert.equal(await page.locator('dialog.confirm').count(), 0);

				await page.locator('.root-bar [role="tab"]', { hasText: exactly('setup') }).click();
				await until(() => front(page), ([name]) => name === 'setup');
				assert.deepEqual(await front(page), ['setup', 'setup']);
				// The setup widget, on a new field.
				await page.locator('.host.setup .field-form input.field').waitFor();
				assert.deepEqual(await rootNames(page), FIRST);

				await page.locator('.root-bar [role="tab"]', { hasText: exactly('collection') }).click();
				await until(() => front(page), ([name]) => name === 'collect');
				await app.idle();
				assert.deepEqual(await titles(page), collect);
				assert.deepEqual(app.errors, []);
			}, device);
		});

	// What it HAS and DOES, not its sentences (vividn, 2026-10-28: «seems like tests that quote word for word are kind
	// of useless»): the copy is his to change without a test following it.
	const LINKS = ['https://github.com/vividn/datum/blob/main/docs/README.md', 'https://datum.day/agents.md', 'https://github.com/vividn/datum'];

	test('what it has: the name, local first, the Tutorial button, the docs, the guide for agents and the repo, then «Close welcome»', async () => {
		await withApp(async ({ page }) => {
			const host = page.locator('.host.welcome');
			assert.equal(await host.getByRole('heading', { level: 1 }).innerText(), 'datum');
			// Back from the Introduction (da-uzyih.4869, vividn, 2026-10-27).
			assert.match(await words(page), /local first/i);
			const controls = await host.locator('div.welcome').locator('button, a').evaluateAll((all) =>
				all.map((e) => (e.tagName === 'A' ? e.getAttribute('href') : e.textContent.trim()))
			);
			assert.deepEqual(controls, ['Tutorial', ...LINKS]);
			// «a button at the bottom of the welcome widget»: the last thing in it.
			assert.equal((await host.getByRole('button').last().innerText()).trim(), 'Close welcome');
			for (const link of await host.getByRole('link').all()) assert.equal(await link.getAttribute('target'), '_blank');
		}, FRESH_DESKTOP);
	});

	test('what it does: its Tutorial button opens the tutorial database', async () => {
		await withApp(async ({ page }) => {
			await Promise.all([
				page.waitForURL((url) => url.searchParams.get('db') === 'tutorial'),
				page.locator('.host.welcome').getByRole('button', { name: 'Tutorial', exact: true }).click()
			]);
		}, FRESH_DESKTOP);
	});

	// The Help place's front page is the same short welcome, under its own line (da-uzyih.19): no full copy is kept.
	test('Help\'s front page is the short welcome under its own line', async () => {
		await withApp(async ({ page }) => {
			await page.evaluate(() => window.datum.shell.open('help'));
			const help = page.locator('.host[data-widget="help"]');
			assert.equal(await help.locator('p.quiet + div.welcome').count(), 1);
			const links = await help.locator('div.welcome').getByRole('link').evaluateAll((all) => all.map((a) => a.getAttribute('href')));
			assert.deepEqual(links, LINKS);
			const text = await help.innerText();
			for (const gone of ['Field shapes', 'The input bar', 'Tabs and widgets', 'How to track', 'Learn more'])
				assert.ok(!text.includes(gone), `still says ${gone}: ${text}`);
		}, DESKTOP);
	});

	// da-s1ek1, vividn, 2026-10-21: «It should be clear on the front page of datum.day that you can point your agent
	// there to help you get setup with your data.» One line, linking the guide an agent reads, on a phone's first screen.
	test('its line for AI agents links the guide for agents, and is on a phone\'s first screen', async () => {
		await withApp(async ({ page }) => {
			const line = page.locator('.host.welcome p', { has: page.locator('a[href="https://datum.day/agents.md"]') });
			assert.equal(await line.count(), 1);
			// Read before any scrolling: the line ends inside the pane that scrolls the welcome, which is at its top.
			const at = await line.evaluate((p) => {
				let el = p.parentElement;
				while (el && getComputedStyle(el).overflowY === 'visible') el = el.parentElement;
				return { bottom: p.getBoundingClientRect().bottom, within: el.getBoundingClientRect().bottom, scrolled: el.scrollTop };
			});
			assert.equal(at.scrolled, 0);
			assert.ok(at.bottom <= at.within, `the line ends at ${at.bottom}, below the pane's ${at.within}`);
		}, FRESH_PHONE);
	});

	// «fits a phone screen without scrolling» again (da-uzyih.19), as it was until 2026-10-14: its close button too.
	test('on a phone it fits the screen without scrolling', async () => {
		await withApp(async ({ page }) => {
			const box = await page.locator('.host.welcome').evaluate((host) => {
				let el = host.querySelector('div.welcome');
				const bottom = Math.max(...[...host.querySelectorAll('div.welcome, button.command')].map((e) => e.getBoundingClientRect().bottom));
				while (el && getComputedStyle(el).overflowY === 'visible') el = el.parentElement;
				return { bottom, within: el.getBoundingClientRect().bottom, tall: el.scrollHeight - el.clientHeight, wide: el.scrollWidth - el.clientWidth };
			});
			assert.ok(box.bottom <= box.within, `the welcome ends at ${box.bottom}, below its pane's ${box.within}`);
			assert.equal(box.tall, 0, 'the widget scrolls down');
			assert.equal(box.wide, 0, 'the widget scrolls sideways');
		}, FRESH_PHONE);
	});

	// vividn, 2026-10-14: «make the track button on the welcome page more visible like make the outline colored»; the
	// Tutorial button is the one thing to do next now.
	test('its Tutorial button is outlined in the accent at rest, not only when pointed at', async () => {
		await withApp(async ({ page }) => {
			const [border, accent] = await page.locator('.host.welcome div.welcome button.command', { hasText: 'Tutorial' }).evaluate((button) => {
				const probe = document.createElement('div');
				probe.style.color = 'var(--accent)';
				button.parentElement.append(probe);
				const accent = getComputedStyle(probe).color;
				probe.remove();
				return [getComputedStyle(button).borderTopColor, accent];
			});
			assert.equal(border, accent);
		}, FRESH_PHONE);
	});

	for (const [what, device] of [
		['a desktop', FRESH_DESKTOP],
		['a phone', FRESH_PHONE]
	])
		test(`${what}: «Close welcome» closes the tab in one tap, and it stays closed; the + brings it back with its button`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await closeWelcome(page).click();
				await until(() => rootNames(page), (names) => names.join() === 'setup,collect,dayview');
				assert.deepEqual(await front(page), ['setup', 'setup']);

				await app.reload();
				assert.deepEqual(await rootNames(page), ['setup', 'collect', 'dayview']);

				await page.locator('.root-bar button.add').first().click();
				await chooser(page).locator('button', { hasText: exactly('welcome') }).click();
				await until(() => rootNames(page), (names) => names.join() === 'setup,collect,dayview,welcome');
				await closeWelcome(page).waitFor();
				assert.deepEqual(app.errors, []);
			}, device);
		});

	test('the button is the tab\'s own close: there in either mode, and gone on the last tab of the page', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await closeWelcome(page).waitFor();
			await enterLayoutMode(page);
			await closeWelcome(page).waitFor();
			await leaveLayoutMode(page);

			// The root's last tab does not close (tree.ts), so nothing offers to.
			for (const at of [3, 2, 1]) await page.evaluate((at) => window.datum.shell.close(at), at);
			await until(() => rootNames(page), (names) => names.join() === 'welcome');
			assert.equal(await closeWelcome(page).count(), 0);
			assert.deepEqual(app.errors, []);
		}, FRESH_DESKTOP);
	});

	test('in a pane of another page it closes that tab alone, and normal mode takes it away', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await addTab(page, 'tail', 'welcome');
			await until(() => activeTab(page, 'tail'), (name) => name === 'welcome');
			await leaveLayoutMode(page);
			assert.equal(await closeWelcome(page).count(), 0);
			await enterLayoutMode(page);

			await closeWelcome(page).click();
			// One tab again, so no strip: the pane is titled by what is left.
			await until(() => pane(page, 'tail').getAttribute('data-title'), (title) => title === 'tail');
			assert.equal(await pane(page, 'tail').locator('[role="tab"]').count(), 0);
			assert.equal(await page.locator('.host.welcome').count(), 0);
			assert.deepEqual(await rootNames(page), ['main']);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('welcome closes from layout mode: its tab held, Close tab; setup and collect stay, and the + brings it back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await manageRoot(page, 'Close tab');
			await until(() => rootNames(page), (names) => names.join() === 'setup,collect,dayview');
			assert.deepEqual(await front(page), ['setup', 'setup']);

			await app.reload();
			assert.deepEqual(await rootNames(page), ['setup', 'collect', 'dayview']);

			// «Open a widget, or a tab you closed: the + beside the tabs.»
			await page.locator('.root-bar button.add').first().click();
			await chooser(page).locator('button', { hasText: exactly('welcome') }).click();
			await until(() => rootNames(page), (names) => names.join() === 'setup,collect,dayview,welcome');
			assert.ok((await words(page)).includes('Record your life'));

			// And collect, which is no widget kind, comes back from it too.
			await rootMenuButton(page).waitFor();
			await page.evaluate(() => window.datum.shell.close(1));
			await until(() => rootNames(page), (names) => names.join() === 'setup,dayview,welcome');
			await page.locator('.root-bar button.add').first().click();
			await chooser(page).locator('button.layout', { hasText: exactly('collection') }).click();
			await until(() => rootNames(page), (names) => names.join() === 'setup,dayview,welcome,collect');
			assert.deepEqual(app.errors, []);
		}, FRESH_DESKTOP);
	});

	test('a pane splits the way it says: layout mode, the pane\'s name, Split', async () => {
		await withApp(async ({ page }) => {
			assert.equal(await pane(page, 'tail').locator('button.add').count(), 0);
			await manage(page, 'tail', 'split: a new pane to the right');
			await titledPane(page, 'empty').waitFor();
		}, { ...FRESH_DESKTOP, home: ON_COLLECT });
	});
});

describe('a device that has been here', () => {
	test('keeps the tabs it had, and the tree under them', async () => {
		for (const [device, name, shown] of [
			[{ ...DESKTOP, desk: null }, 'main', ['dayview', 'tail', 'collect', 'nowview', 'stats']],
			[{ ...FRESH_PHONE, home: 'mobile' }, 'mobile', ['tail', 'nowview', 'collect']]
		])
			await withApp(async ({ page }) => {
				assert.deepEqual(await rootNames(page), [name]);
				assert.deepEqual(await titles(page), shown);
			}, device);
	});

	// da-rgyq8: the second tab was `track` until the widget it holds became
	// the setup widget; a device that kept it open keeps it, under the new name.
	test('with the old track tab open, opens it as setup', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const names = ['welcome', 'setup', 'collect'];
			assert.deepEqual(await rootNames(page), names);
			assert.deepEqual(await front(page), ['setup', 'setup']);
			await app.reload();
			assert.deepEqual(await rootNames(page), names);
			assert.deepEqual(app.errors, []);
		}, { ...DESKTOP, home: { names: ['welcome', 'track', 'collect'], active: 1 } });
	});

	// Against datum's OWN list of new defaults (layout/defaults.ts), which
	// the harness otherwise empties for a device with a stored list: this
	// is the sentence that tells the early users' devices.
	test('is offered the new tabs once, in words that name them, and «Use the new layout» opens them', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const offer = page.locator('dialog.confirm');
			const text = await offer.locator('p').innerText();
			for (const name of FIRST) assert.ok(text.includes(name), `the offer does not name ${name}: ${text}`);
			assert.deepEqual(await rootNames(page), ['main']);

			await offer.getByRole('button', { name: 'Use the new layout', exact: true }).click();
			await until(() => rootNames(page), (names) => names.join() === FIRST.join());
			assert.deepEqual(await front(page), ['welcome', 'welcome']);

			await app.reload();
			assert.equal(await offer.count(), 0);
			assert.deepEqual(await rootNames(page), FIRST);
		}, {
			...DESKTOP,
			home: null,
			prepare: (context) =>
				context.addInitScript(() => {
					if (!localStorage.getItem('datum.layout.root.v1'))
						localStorage.setItem('datum.layout.root.v1', JSON.stringify({ names: ['main'], active: 0 }));
				})
		});
	});
});

// da-rgyq8, vividn: an early user «suggested we call it "setup" widget and I would
// agree». What the app ships says «Track» nowhere but as the verb of the
// sentence that says what a field is (WelcomeText.svelte) and of the gear's «Track timezone» (da-yw6o2,
// vividn's word for it): no title, button, tab or help of the widget.
test('the built app says «Track» only as a verb', () => {
	const build = fileURLToPath(new URL('../build', import.meta.url));
	const said = readdirSync(build, { recursive: true })
		.filter((f) => /\.(js|html)$/.test(f))
		.flatMap((f) => readFileSync(join(build, f), 'utf8').match(/\bTrack\b(?! timezone).{0,20}/g) ?? []);
	assert.deepEqual([...new Set(said)], ['Track what matters to you']);
});
