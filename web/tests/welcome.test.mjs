// A first visit opens welcome · track · collect (da-u7vz8).
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
// front; its button lands on `track`; it closes the way it says it does,
// leaving the other two, and the `+` brings it back; and a device that has
// been here keeps the tabs it had. What the WORDS claim about the page is
// done to the page here, by the controls they name.
//
// And it closes by a button of its own (da-zhz0u). vividn, 2026-10-09:
// «maybe start unlocked. Let's put a button at the bottom of the welcome
// widget that closes the widget too … I am imagining that the welcome
// widget will have some scrolling and reading rather than all fitting on
// one page».

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, activeTab, addTab, chooser, exactly, manage, manageButton, manageRoot, pane, rootMenuButton, titledPane, titles, toggleRootLock } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const FIRST = ['welcome', 'track', 'collect'];
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

/** Whether the root tab in front is locked. */
const locked = (page) =>
	page.evaluate(() => {
		const { tabs, active } = window.datum.layout.current().root;
		return !!tabs[active].locked;
	});

describe('a first visit', () => {
	for (const [what, device, collect] of [
		['a desktop', FRESH_DESKTOP, ['dayview', 'tail', 'nowview', 'stats', 'fields', 'command']],
		['a phone', FRESH_PHONE, ['dayview', 'tail', 'nowview', 'fields', 'nowview', 'command']]
	])
		test(`${what} opens welcome · track · collect on welcome; its button lands on track; collect is the page it opened before`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				assert.deepEqual(await rootNames(page), FIRST);
				assert.deepEqual(await page.locator('.root-bar [role="tab"]').allInnerTexts(), FIRST);
				assert.deepEqual(await front(page), ['welcome', 'welcome']);
				// Nothing about it is an edit: no dot, and no offer of a new
				// default to a device that starts on it.
				assert.equal(await page.locator('.root-bar [role="tab"].modified').count(), 0);
				assert.equal(await page.locator('dialog.confirm').count(), 0);

				await page.getByRole('button', { name: 'Choose what to track', exact: true }).click();
				await until(() => front(page), ([name]) => name === 'track');
				assert.deepEqual(await front(page), ['track', 'field']);
				// The field widget, on a new field.
				await page.locator('.host.field button.new[aria-current="true"]').waitFor();
				assert.deepEqual(await rootNames(page), FIRST);

				await page.locator('.root-bar [role="tab"]', { hasText: exactly('collect') }).click();
				await until(() => front(page), ([name]) => name === 'collect');
				await app.idle();
				assert.deepEqual(await titles(page), collect);
				assert.deepEqual(app.errors, []);
			}, device);
		});

	test('what it says: datum, a field, an entry, the four kinds as the Track form draws them, the flow, and the tabs', async () => {
		await withApp(async ({ page }) => {
			const text = await words(page);
			for (const said of [
				'A record of your life, kept on your device. No account.',
				'A field is one thing you track',
				'An entry is one time you logged it.',
				'Choose a few things to track.',
				'Log them: tap, or type in the bar below.',
				'Watch your day and your stats fill in.',
				'Close a tab: ▾ beside the +, Close tab.',
				'Open a widget, or a tab you closed: the + at the top.',
				'Move a tab: drag it. On a phone, hold it first.',
				'The other pages start locked, with no ▾. The padlock, top right, unlocks the one in front.',
				'Split a pane: ▾ on its bar, Split.'
			])
				assert.ok(text.includes(said), `not said: ${said}\n${text}`);
			// «a button at the bottom of the welcome widget»: the last thing in it.
			assert.ok(text.trimEnd().endsWith('Close welcome'), text);
			// The four kinds, glyph and all, are the ones the Track form offers.
			const kinds = await page.locator('.host.welcome .kinds li').allInnerTexts();
			assert.deepEqual(kinds.map((k) => k.replace(/\s+/g, ' ')), [
				'● happens',
				'█ lasts (start/end)',
				'▞ switches between states',
				'¢ a note'
			]);
			// «Readable in about half a minute»: under 150 words, 36 s at 250 a minute.
			const count = text.split(/\s+/).filter(Boolean).length;
			assert.ok(count < 150, `${count} words`);
		}, FRESH_DESKTOP);
	});

	test('it fits a phone without scrolling', async () => {
		await withApp(async ({ page }) => {
			const box = await page.locator('.host.welcome').evaluate((host) => {
				let el = host.querySelector('div.welcome');
				const bottom = el.getBoundingClientRect().bottom;
				while (el && getComputedStyle(el).overflowY === 'visible') el = el.parentElement;
				return { bottom, within: el.getBoundingClientRect().bottom, wide: el.scrollWidth - el.clientWidth };
			});
			assert.ok(box.bottom <= box.within, `the text ends at ${box.bottom}, its pane at ${box.within}`);
			assert.equal(box.wide, 0, 'the widget scrolls sideways');
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
				await until(() => rootNames(page), (names) => names.join() === 'track,collect');
				assert.deepEqual(await front(page), ['track', 'field']);

				await app.reload();
				assert.deepEqual(await rootNames(page), ['track', 'collect']);

				await page.locator('.root-bar button.add').first().click();
				await chooser(page).locator('button', { hasText: exactly('welcome') }).click();
				await until(() => rootNames(page), (names) => names.join() === 'track,collect,welcome');
				await closeWelcome(page).waitFor();
				assert.deepEqual(app.errors, []);
			}, device);
		});

	test('the button is the tab\'s own ×: gone under the padlock, and on the last tab of the page', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await closeWelcome(page).waitFor();
			await toggleRootLock(page);
			assert.equal(await locked(page), true);
			assert.equal(await closeWelcome(page).count(), 0);
			await toggleRootLock(page);
			await closeWelcome(page).waitFor();

			// The root's last tab does not close (tree.ts), so nothing offers to.
			await page.evaluate(() => window.datum.shell.close(2));
			await page.evaluate(() => window.datum.shell.close(1));
			await until(() => rootNames(page), (names) => names.join() === 'welcome');
			assert.equal(await closeWelcome(page).count(), 0);
			assert.deepEqual(app.errors, []);
		}, FRESH_DESKTOP);
	});

	test('in a pane of another page it closes that tab alone, and the page\'s lock takes it away', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await addTab(page, 'tail', 'welcome');
			await until(() => activeTab(page, 'tail'), (name) => name === 'welcome');
			await toggleRootLock(page);
			assert.equal(await closeWelcome(page).count(), 0);
			await toggleRootLock(page);

			await closeWelcome(page).click();
			// One tab again, so no strip: the pane is titled by what is left.
			await until(() => pane(page, 'tail').getAttribute('data-title'), (title) => title === 'tail');
			assert.equal(await pane(page, 'tail').locator('[role="tab"]').count(), 0);
			assert.equal(await page.locator('.host.welcome').count(), 0);
			assert.deepEqual(await rootNames(page), ['main']);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test('welcome closes the way it says: the ▾ beside the +, Close tab, with no padlock first; track and collect stay, and the + brings it back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// Unlocked from the first (da-zhz0u): the ▾ is there to find.
			assert.equal(await locked(page), false);
			await manageRoot(page, 'Close tab');
			await until(() => rootNames(page), (names) => names.join() === 'track,collect');
			assert.deepEqual(await front(page), ['track', 'field']);

			await app.reload();
			assert.deepEqual(await rootNames(page), ['track', 'collect']);

			// «Open a widget, or a tab you closed: the + at the top.»
			await page.locator('.root-bar button.add').first().click();
			await chooser(page).locator('button', { hasText: exactly('welcome') }).click();
			await until(() => rootNames(page), (names) => names.join() === 'track,collect,welcome');
			assert.ok((await words(page)).includes('A record of your life'));

			// And collect, which is no widget kind, comes back from it too.
			await rootMenuButton(page).waitFor();
			await page.evaluate(() => window.datum.shell.close(1));
			await until(() => rootNames(page), (names) => names.join() === 'track,welcome');
			await page.locator('.root-bar button.add').first().click();
			await chooser(page).locator('button.layout', { hasText: exactly('collect') }).click();
			await until(() => rootNames(page), (names) => names.join() === 'track,welcome,collect');
			assert.deepEqual(app.errors, []);
		}, FRESH_DESKTOP);
	});

	test('a pane splits the way it says: the padlock, the ▾ on its bar, Split', async () => {
		await withApp(async ({ page }) => {
			assert.equal(await manageButton(page, 'tail').count(), 0);
			await toggleRootLock(page);
			await manage(page, 'tail', 'split: a new pane to the right');
			await titledPane(page, 'empty').waitFor();
		}, { ...FRESH_DESKTOP, home: ON_COLLECT });
	});
});

describe('a device that has been here', () => {
	test('keeps the tabs it had, and the tree under them', async () => {
		for (const [device, name, shown] of [
			[{ ...DESKTOP, desk: null }, 'main', ['dayview', 'tail', 'nowview', 'stats', 'fields', 'command']],
			[{ ...FRESH_PHONE, home: 'mobile' }, 'mobile', ['dayview', 'tail', 'nowview', 'fields', 'nowview', 'command']]
		])
			await withApp(async ({ page }) => {
				assert.deepEqual(await rootNames(page), [name]);
				assert.deepEqual(await titles(page), shown);
			}, device);
	});

	// Against datum's OWN list of new defaults (layout/defaults.ts), which
	// the harness otherwise empties for a device with a stored list: this
	// is the sentence that tells vividn's and an early user's devices.
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

	test('its button opens the field widget where there is no track tab: a place', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await page.evaluate(() => window.datum.shell.open('welcome'));
			await page.getByRole('button', { name: 'Choose what to track', exact: true }).click();
			// A desktop's place is a modal (da-jozgv); the tabs do not move.
			await page.locator('dialog.sheet[data-place="field"] button.new[aria-current="true"]').waitFor();
			assert.deepEqual(await rootNames(page), ['main', 'welcome']);
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});
