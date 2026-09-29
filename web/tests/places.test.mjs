// A modal asks; a place is a widget (da-6du3o, da-jozgv).
//
// vividn, 2026-09-29: «always open the new tab directly next to the current
// one, so that the user can quickly return to the previously open tab
// (which would then close this temporary tab.) There will be a nice
// animation sliding to the right too … That makes it obvious that the user
// can then swipe back». So `openPlace(kind)` brings forward the root tab
// already holding the kind, or opens a TRANSIENT one right of the tab you
// are on and slides onto it; leaving it by any road closes it, and `Keep
// tab` keeps it. Sync is its first user: the commands widget's one `Sync`
// button, and `/sync/approve#…`.
//
// That is a PHONE's (da-jozgv). vividn, 2026-09-29: «I think this is good on
// mobile, but the experience on desktop is not great … I would still have
// it be a modal on desktop». So on a desktop the same `openPlace` draws the
// widget in a modal (layout/PlaceModal.svelte), whose `Open as tab` hands
// it to the tab a phone gets.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, PHONE, atRest, chooseRoot, swipeOver } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const rootTabs = (page) =>
	page.locator('.root-bar [role="tab"]').evaluateAll((all) => all.map((b) => b.textContent.replace('•', '').trim()));

const shown = (page) =>
	page.locator('.root-bar [role="tab"][aria-selected="true"]').evaluate((b) => b.textContent.replace('•', '').trim());

const italic = (page) =>
	page
		.locator('.root-bar [role="tab"]')
		.evaluateAll((all) =>
			all.filter((b) => getComputedStyle(b).fontStyle === 'italic').map((b) => b.textContent.replace('•', '').trim())
		);

/** This device's open layouts, as the shell wrote them. */
const openList = (page) => page.evaluate(() => JSON.parse(localStorage.getItem('datum.layout.root.v1') ?? 'null'));

const rootContent = (page) => page.locator('section.root > .content');

/** The modal a place opens on a desktop. */
const modal = (page, kind = 'sync') => page.locator(`dialog.sheet[open][data-place="${kind}"]`);

/** Whether the history entry the browser is on is a place's. */
const onPlaceEntry = (page) => page.evaluate(() => JSON.stringify(history.state ?? null).includes('"place"'));

/** Open the place `kind` and wait for it to have slid in: a tap during the slide is a tap on the tab it left. */
async function openPlace(page, kind) {
	await page.evaluate((kind) => window.datum.shell.openPlace(kind), kind);
	await until(() => shown(page), (t) => t === kind);
	await atRest(rootContent(page));
}

/** Every `--front` the root's strip takes from now on, while it is `moving`, and whether it faded. */
const watch = (page) =>
	rootContent(page).evaluate((el) => {
		window.__strip = { moved: [], faded: false };
		new MutationObserver(() => {
			if (el.classList.contains('moving')) window.__strip.moved.push(el.style.getPropertyValue('--front'));
			if (el.classList.contains('fading')) window.__strip.faded = true;
		}).observe(el, { attributes: true, attributeFilter: ['style', 'class'] });
	});
const strip = (page) => page.evaluate(() => window.__strip);

/** The open list `names`, the first in front. */
const opened = (...names) => ({
	prepare: (context) =>
		context.addInitScript(
			(json) => localStorage.setItem('datum.layout.root.v1', json),
			JSON.stringify({ names, active: 0 })
		)
});

const desktop = sharedApp({ ...DESKTOP, desk: null });
const phone = sharedApp(PHONE);

describe('a place on a desktop', () => {
	test('the Sync button opens it in a modal, not a tab, and the browser’s back closes it', async () => {
		await desktop.solo(async ({ page, errors }) => {
			const sync = page.locator('.bar.commands [data-command="sync"]');
			assert.equal((await sync.innerText()).trim(), 'Sync');
			assert.equal(await sync.getAttribute('data-act'), 'open');
			await sync.click();
			await modal(page).locator('table').waitFor();
			assert.equal((await modal(page).locator('.sheet-head > span').innerText()).trim(), 'sync');
			// Found as a pane is, by its title, and named once: the head says it, not the widget too.
			const pane = modal(page).locator('section.pane[data-title="sync"]');
			assert.equal(await pane.count(), 1);
			assert.equal(await pane.locator('h2').isVisible(), false);
			assert.deepEqual(await rootTabs(page), ['main']);
			assert.ok(await onPlaceEntry(page), 'opening it pushed no history entry');
			await page.goBack();
			await modal(page).waitFor({ state: 'detached' });
			assert.deepEqual(await rootTabs(page), ['main']);
			assert.deepEqual(errors, []);
		});
	});

	for (const [how, close] of [
		['Escape', (page) => page.keyboard.press('Escape')],
		['the backdrop', (page) => page.mouse.click(4, 4)],
		['×', (page) => modal(page).locator('.sheet-head .close').click()]
	]) {
		test(`${how} closes it, and takes its history entry with it`, async () => {
			await desktop.solo(async ({ page }) => {
				await page.evaluate(() => window.datum.shell.openPlace('sync'));
				await modal(page).locator('table').waitFor();
				await close(page);
				await modal(page).waitFor({ state: 'detached' });
				await until(() => onPlaceEntry(page), (on) => !on);
				assert.deepEqual(await rootTabs(page), ['main']);
			});
		});
	}

	test('Open as tab puts it in a transient tab beside the current one, which the back closes', async () => {
		await withApp(
			async ({ page }) => {
				await page.evaluate(() => window.datum.shell.openPlace('sync'));
				const asTab = modal(page).locator('.as-tab');
				assert.equal(await asTab.getAttribute('data-act'), 'open');
				await asTab.click();
				await modal(page).waitFor({ state: 'detached' });
				await until(() => shown(page), (t) => t === 'sync');
				await atRest(rootContent(page));
				assert.deepEqual(await rootTabs(page), ['main', 'sync', 'tail']);
				assert.deepEqual(await italic(page), ['sync'], 'the place is transient');
				await page.locator('.host[data-widget="sync"] table').waitFor();
				// The modal's entry was handed on, not a second one pushed: one back is the way home.
				await page.goBack();
				await until(() => rootTabs(page), (t) => t.join() === 'main,tail');
				assert.equal(await onPlaceEntry(page), false);
			},
			{ ...DESKTOP, ...opened('main', 'tail') }
		);
	});

	// vremya on #356: a place is a NAME, which the tab resolves through its
	// layout; the modal has to draw what that name resolves to, not a kind of
	// the same spelling. `dayview` is `only('dayview', 'daystrip', …)`.
	test('a place whose name is not its kind opens the widget its layout holds', async () => {
		await desktop.solo(async ({ page }) => {
			await page.evaluate(() => window.datum.shell.openPlace('dayview'));
			const dayview = modal(page, 'dayview');
			await dayview.locator('button.freeze').waitFor();
			assert.equal((await dayview.locator('.sheet-head > span').innerText()).trim(), 'dayview');
			assert.equal(await dayview.locator('button.freeze').getAttribute('aria-checked'), 'true');
			await dayview.locator('.sheet-head .close').click();
			await dayview.waitFor({ state: 'detached' });
			await until(() => onPlaceEntry(page), (on) => !on);
		});
	});

	// canary-deepseek-v4-pro on #356: the tab migrates what it resolves
	// (`inline` → `migrated`), and so must the modal. A save of `dayview`
	// carrying the retired `dayHeight: 85` (da-r0ydu) draws a day that
	// follows its box in the tab; in the modal it has to as well.
	test('a place’s save is migrated in the modal as it is in the tab', async () => {
		await desktop.solo(async ({ page }) => {
			const settings = { dayHeight: 85, freezeToday: true };
			await page.evaluate(
				(settings) =>
					window.datum.layouts.saveLayout({
						version: 1,
						name: 'dayview',
						root: {
							id: 'dv',
							active: 0,
							bar: { edge: 'top', align: 'start' },
							tabs: [{ id: 'dv-tab', content: { id: 'dv-w', kind: 'daystrip', settings } }]
						}
					}),
				settings
			);
			await page.evaluate(() => window.datum.shell.openPlace('dayview'));
			const grip = modal(page, 'dayview').locator('button.day-grip');
			await grip.waitFor();
			await until(() => grip.getAttribute('aria-valuenow').then(Number), (n) => n !== 85);
		});
	});

	test('a root tab holding it is brought forward, and no modal opens', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await page.evaluate(() => window.datum.shell.openPlace('sync', true));
			await until(() => shown(page), (t) => t === 'sync');
			await chooseRoot(page, 'Keep tab');
			await page.locator('.root-bar [role="tab"]', { hasText: 'main' }).click();
			await until(() => shown(page), (t) => t === 'main');
			await page.locator('.bar.commands [data-command="sync"]').click();
			await until(() => shown(page), (t) => t === 'sync');
			assert.equal(await modal(page).count(), 0);
			assert.deepEqual(await rootTabs(page), ['main', 'sync']);
		});
	});

	test('in a pane, it is just the widget', async () => {
		await desktop.solo(async ({ page }) => {
			const tab = await page.evaluate(() => {
				const [first] = window.datum.layout.current().root.tabs[0].content.children;
				return first.tabs[0].id;
			});
			await page.evaluate((tab) => window.datum.shell.widgetInto(tab, 'sync'), tab);
			await page.locator('section.pane[data-title="sync"] table').waitFor();
			// A root tab holding the kind is what `openPlace` looks for, not a pane.
			await page.evaluate(() => window.datum.shell.openPlace('sync'));
			await modal(page).locator('table').waitFor();
			assert.deepEqual(await rootTabs(page), ['main']);
		});
	});

	test('/sync/approve#… opens the Sync place', async () => {
		await desktop.solo(async (app) => {
			await app.goto('/sync/approve#eyJ9');
			await modal(app.page).locator('table').waitFor();
		});
	});
});

describe('a place on a phone', () => {
	test('it opens beside the tab you were on, sliding in; a second open focuses it', async () => {
		await phone.solo(async ({ page, errors }) => {
			await watch(page);
			await openPlace(page, 'sync');
			assert.deepEqual(await rootTabs(page), ['main', 'sync']);
			assert.deepEqual(await italic(page), ['sync'], 'the place is transient');
			assert.ok(
				(await strip(page)).moved.some((t) => /translateX\(-\d/.test(t)),
				`the view did not slide right onto it: ${JSON.stringify(await strip(page))}`
			);
			await page.locator('.host[data-widget="sync"] table').waitFor();
			assert.equal(await modal(page).count(), 0);
			// Written nowhere: a reload would not know it.
			assert.deepEqual(await openList(page), { names: ['main'], active: 0 });

			const id = await page.evaluate(() => window.datum.layout.current().root.tabs[1].id);
			await openPlace(page, 'sync');
			assert.deepEqual(await rootTabs(page), ['main', 'sync']);
			assert.equal(await page.evaluate(() => window.datum.layout.current().root.tabs[1].id), id);
			assert.deepEqual(errors, []);
		});
	});
	test('the raw subcommands are under Advanced, and fill the line', async () => {
		await phone.solo(async ({ page }) => {
			await openPlace(page, 'sync');
			const advanced = page.locator('.host[data-widget="sync"] details.advanced');
			await advanced.locator('summary').click();
			const subs = await advanced.locator('button').evaluateAll((all) => all.map((b) => b.textContent.trim()));
			assert.ok(subs.includes('status') && subs.includes('chain'), subs.join(' '));
			await advanced.locator('button', { hasText: /^status$/ }).click();
			assert.equal(await page.locator('.prompt-bar input').inputValue(), 'sync status ');
		});
	});

	test('opens right of whichever tab you are on; a tap back slides back and closes it', async () => {
		await withApp(
			async ({ page, errors }) => {
				await openPlace(page, 'sync');
				assert.deepEqual(await rootTabs(page), ['main', 'sync', 'tail', 'repl']);
				await watch(page);
				await page.locator('.root-bar [role="tab"]', { hasText: 'main' }).click();
				await until(() => rootTabs(page), (t) => t.join() === 'main,tail,repl');
				assert.equal(await shown(page), 'main');
				assert.ok(
					(await strip(page)).moved.some((t) => /translateX\(\d/.test(t)),
					`the view did not slide back: ${JSON.stringify(await strip(page))}`
				);

				await page.locator('.root-bar [role="tab"]', { hasText: 'tail' }).click();
				await openPlace(page, 'sync');
				assert.deepEqual(await rootTabs(page), ['main', 'tail', 'sync', 'repl']);
				// Leaving it for a tab that is not the one it came from closes it too.
				await page.locator('.root-bar [role="tab"]', { hasText: 'repl' }).click();
				await until(() => rootTabs(page), (t) => t.join() === 'main,tail,repl');
				assert.equal(await shown(page), 'repl');
				assert.deepEqual(errors, []);
			},
			{ ...PHONE, ...opened('main', 'tail', 'repl') }
		);
	});

	// canary-kimi-k3 on #349 (13440, 13491): a tab asked for while a slide is
	// landing waits for it and is found by its id, after the slide's own
	// close has moved every index right of the place.
	test('a tap during the slide back lands on the tab tapped', async () => {
		await withApp(
			async ({ page }) => {
				await openPlace(page, 'sync');
				assert.deepEqual(await rootTabs(page), ['main', 'sync', 'tail', 'repl']);
				await page.evaluate(async () => {
					const { shell } = window.datum;
					shell.activate(0);
					// Two frames in: the 150 ms slide back is under way.
					for (let i = 0; i < 2; i++) await new Promise((r) => requestAnimationFrame(r));
					shell.activate(3);
				});
				await until(() => shown(page), (t) => t === 'repl');
				await atRest(rootContent(page));
				assert.deepEqual(await rootTabs(page), ['main', 'tail', 'repl']);
				assert.equal(await shown(page), 'repl');
			},
			{ ...PHONE, ...opened('main', 'tail', 'repl') }
		);
	});

	test('the browser’s back returns to the tab it opened beside, and closes it', async () => {
		await phone.solo(async ({ page }) => {
			await openPlace(page, 'sync');
			await page.goBack();
			await until(() => rootTabs(page), (t) => t.join() === 'main');
			assert.equal(await shown(page), 'main');
		});
	});

	// canary-kimi-k3 on #349: one `pushed` flag for every place made leaving
	// the first pop the second's history entry, and the pop slide the second
	// closed too.
	test('a place opened from a place replaces it, and the back leaves the second alone until pressed', async () => {
		await phone.solo(async ({ page }) => {
			await openPlace(page, 'sync');
			await openPlace(page, 'aliases');
			await until(() => rootTabs(page), (t) => t.join() === 'main,aliases');
			await atRest(rootContent(page));
			assert.equal(await shown(page), 'aliases');
			await page.goBack();
			await until(() => rootTabs(page), (t) => t.join() === 'main');
		});
	});

	test('Keep tab keeps it, across a reload', async () => {
		await phone.solo(async (app) => {
			const { page } = app;
			await openPlace(page, 'sync');
			await chooseRoot(page, 'Keep tab');
			assert.deepEqual(await italic(page), []);
			assert.deepEqual(await openList(page), { names: ['main', 'sync'], active: 1 });
			// Kept, it is a tab like any other: leaving it leaves it where it is.
			await page.locator('.root-bar [role="tab"]', { hasText: 'main' }).click();
			await app.reload();
			assert.deepEqual(await rootTabs(page), ['main', 'sync']);
			await openPlace(page, 'sync');
			assert.deepEqual(await rootTabs(page), ['main', 'sync'], 'opening it again made a second');
		});
	});

	test('in a pane, it is just the widget', async () => {
		await phone.solo(async ({ page }) => {
			const tab = await page.evaluate(() => {
				const [first] = window.datum.layout.current().root.tabs[0].content.children;
				return first.tabs[0].id;
			});
			await page.evaluate((tab) => window.datum.shell.widgetInto(tab, 'sync'), tab);
			await page.locator('section.pane[data-title="sync"] table').waitFor();
			// A root tab holding the kind is what `openPlace` looks for, not a pane.
			await openPlace(page, 'sync');
			assert.deepEqual(await rootTabs(page), ['main', 'sync']);
		});
	});

	test('/sync/approve#… opens the Sync place', async () => {
		await phone.solo(async (app) => {
			await app.goto('/sync/approve#eyJ9');
			await until(() => shown(app.page), (t) => t === 'sync');
			assert.deepEqual(await rootTabs(app.page), ['main', 'sync']);
		});
	});

	test('a swipe back returns to the tab it opened beside, and closes it', async () => {
		await withApp(
			async ({ page }) => {
				await openPlace(page, 'sync');
				await atRest(rootContent(page));
				assert.deepEqual(await rootTabs(page), ['main', 'sync', 'tail']);
				await swipeOver(page, rootContent(page), 150, 0);
				await until(() => rootTabs(page), (t) => t.join() === 'main,tail');
				assert.equal(await shown(page), 'main');
			},
			{ ...PHONE, ...opened('main', 'tail') }
		);
	});

	// vremya on #349: with no slide to wait out, the closer runs at once, so
	// the history entry has to be the new place's before the tab switches.
	test('under reduced motion, a place opened from a place replaces it too', async () => {
		await withApp(
			async ({ page }) => {
				await openPlace(page, 'sync');
				await openPlace(page, 'aliases');
				await until(() => rootTabs(page), (t) => t.join() === 'main,aliases');
				await page.goBack();
				await until(() => rootTabs(page), (t) => t.join() === 'main');
			},
			{ ...PHONE, reducedMotion: 'reduce' }
		);
	});

	test('under reduced motion it fades, and nothing slides', async () => {
		await withApp(
			async ({ page }) => {
				await watch(page);
				await openPlace(page, 'sync');
				assert.equal(await shown(page), 'sync');
				await page.locator('.root-bar [role="tab"]', { hasText: 'main' }).click();
				await until(() => rootTabs(page), (t) => t.join() === 'main');
				const { moved, faded } = await strip(page);
				assert.deepEqual(moved, [], 'a transform moved the strip');
				assert.ok(faded, 'it did not fade');
			},
			{ ...PHONE, reducedMotion: 'reduce' }
		);
	});
});
