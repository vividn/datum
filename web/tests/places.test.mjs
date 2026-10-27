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
import { DESKTOP, PHONE, along, atRest, chooseRoot, swipeOver, widgetPane } from './support/layout.mjs';
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

/** Whether the history entry the browser is on is a place's: Kit keeps its state serialized inside it. */
const onPlaceEntry = (page) => page.evaluate(() => /\\?"place\\?"/.test(JSON.stringify(history.state ?? null)));

/** Open the place `kind` and wait for it to have slid in: a tap during the slide is a tap on the tab it left. */
async function openPlace(page, kind) {
	await page.evaluate((kind) => window.datum.shell.openPlace(kind), kind);
	await until(() => shown(page), (t) => t === kind);
	await atRest(rootContent(page));
}

/** Every transform the root's tab in front takes from now on, while it is `moving`, and whether it faded. */
const watch = (page) =>
	rootContent(page).evaluate((el) => {
		window.__strip = { moved: [], faded: false };
		new MutationObserver(() => {
			if (el.classList.contains('moving'))
				window.__strip.moved.push(el.querySelector(':scope > .tab-body:not(.behind)')?.style.transform ?? '');
			if (el.classList.contains('fading')) window.__strip.faded = true;
		}).observe(el, { attributes: true, attributeFilter: ['style', 'class'], subtree: true });
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

// The CLI over a commands pane, whose `Sync` is the door these open by: the
// shipped tree had one until da-mwp4m took it off collect.
const WITH_COMMANDS = JSON.stringify(along('column', widgetPane('repl', 'cli'), widgetPane('cmd', 'command')));
const desktop = sharedApp({ ...DESKTOP, desk: null });
const onDesktop = (fn) => desktop.solo(fn, WITH_COMMANDS);
const phone = sharedApp(PHONE);

describe('a place on a desktop', () => {
	test('the Sync button opens it in a modal, not a tab, and the browser’s back closes it', async () => {
		await onDesktop(async ({ page, errors }) => {
			const sync = page.locator('.bar.commands [data-command="sync"]');
			assert.equal((await sync.innerText()).trim(), 'Sync');
			assert.equal(await sync.getAttribute('data-act'), 'open');
			await sync.click();
			await modal(page).locator('.sync header').waitFor();
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
			await onDesktop(async ({ page }) => {
				await page.evaluate(() => window.datum.shell.openPlace('sync'));
				await modal(page).locator('.sync header').waitFor();
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
				await page.locator('.host[data-widget="sync"] .sync header').waitFor();
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
		await onDesktop(async ({ page }) => {
			await page.evaluate(() => window.datum.shell.openPlace('dayview'));
			const dayview = modal(page, 'dayview');
			await dayview.locator('.strip .today-line').waitFor();
			assert.equal((await dayview.locator('.sheet-head > span').innerText()).trim(), 'dayview');
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
		await onDesktop(async ({ page }) => {
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
			const grip = modal(page, 'dayview').locator('.today-line');
			await grip.waitFor();
			await until(() => grip.getAttribute('aria-valuenow').then(Number), (n) => n !== 85);
		});
	});

	test('a root tab holding it is brought forward, and no modal opens', async () => {
		await onDesktop(async (app) => {
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
		await onDesktop(async ({ page }) => {
			const tab = await page.evaluate(() => {
				const [first] = window.datum.layout.current().root.tabs[0].content.children;
				return first.tabs[0].id;
			});
			await page.evaluate((tab) => window.datum.shell.widgetInto(tab, 'sync'), tab);
			await page.locator('section.pane[data-title="sync"] .sync header').waitFor();
			// A root tab holding the kind is what `openPlace` looks for, not a pane.
			await page.evaluate(() => window.datum.shell.openPlace('sync'));
			await modal(page).locator('.sync header').waitFor();
			assert.deepEqual(await rootTabs(page), ['main']);
		});
	});

	test('/sync/approve#… opens the Sync place', async () => {
		await onDesktop(async (app) => {
			await app.goto('/sync/approve#eyJ9');
			await modal(app.page).locator('.sync header').waitFor();
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
			await page.locator('.host[data-widget="sync"] .sync header').waitFor();
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
	test('the raw subcommands are under More, and fill the line', async () => {
		await phone.solo(async ({ page }) => {
			// More is a chained database's: with sync off, turning it on is the one act.
			assert.equal((await page.evaluate(() => window.datum.runQuiet(['sync', 'new-invite']))).code, 0);
			await openPlace(page, 'sync');
			const widget = page.locator('.host[data-widget="sync"]');
			await widget.getByRole('button', { name: /^More/ }).click();
			const advanced = widget.locator('.more .advanced');
			const subs = await advanced.locator('button').evaluateAll((all) => all.map((b) => b.textContent.trim()));
			assert.ok(subs.includes('sync status') && subs.includes('sync chain'), subs.join(' '));
			await advanced.locator('button', { hasText: /^sync status$/ }).click();
			assert.equal(await page.locator('.prompt-bar input').inputValue(), 'sync status ');
		});
	});

	test('opens right of whichever tab you are on; a tap back slides back and closes it', async () => {
		await withApp(
			async ({ page, errors }) => {
				await openPlace(page, 'sync');
				assert.deepEqual(await rootTabs(page), ['main', 'sync', 'tail', 'cli']);
				await watch(page);
				await page.locator('.root-bar [role="tab"]', { hasText: 'main' }).click();
				await until(() => rootTabs(page), (t) => t.join() === 'main,tail,cli');
				assert.equal(await shown(page), 'main');
				assert.ok(
					(await strip(page)).moved.some((t) => /translateX\(\d/.test(t)),
					`the view did not slide back: ${JSON.stringify(await strip(page))}`
				);

				await page.locator('.root-bar [role="tab"]', { hasText: 'tail' }).click();
				await openPlace(page, 'sync');
				assert.deepEqual(await rootTabs(page), ['main', 'tail', 'sync', 'cli']);
				// Leaving it for a tab that is not the one it came from closes it too.
				await page.locator('.root-bar [role="tab"]', { hasText: 'cli' }).click();
				await until(() => rootTabs(page), (t) => t.join() === 'main,tail,cli');
				assert.equal(await shown(page), 'cli');
				assert.deepEqual(errors, []);
			},
			{ ...PHONE, ...opened('main', 'tail', 'cli') }
		);
	});

	// canary-kimi-k3 on #349 (13440, 13491): a tab asked for while a slide is
	// landing waits for it and is found by its id, after the slide's own
	// close has moved every index right of the place.
	test('a tap during the slide back lands on the tab tapped', async () => {
		await withApp(
			async ({ page }) => {
				await openPlace(page, 'sync');
				assert.deepEqual(await rootTabs(page), ['main', 'sync', 'tail', 'cli']);
				await page.evaluate(async () => {
					const { shell } = window.datum;
					shell.activate(0);
					// Two frames in: the 150 ms slide back is under way.
					for (let i = 0; i < 2; i++) await new Promise((r) => requestAnimationFrame(r));
					shell.activate(3);
				});
				await until(() => shown(page), (t) => t === 'cli');
				await atRest(rootContent(page));
				assert.deepEqual(await rootTabs(page), ['main', 'tail', 'cli']);
				assert.equal(await shown(page), 'cli');
			},
			{ ...PHONE, ...opened('main', 'tail', 'cli') }
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
			await page.locator('section.pane[data-title="sync"] .sync header').waitFor();
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

	// da-ria1a: the place unmounts the tab it left, and with it the commands
	// widget; what its bar was showing is kept, and so is its clearing. The
	// root keeps one layout here: a layout once opened stays mounted
	// (da-tajsm), and these are about the bar that comes back anew.
	const commands = (page) => page.locator('.bar.commands');
	/** `opened`, with one layout kept mounted at the root. */
	const alone = (...names) => ({
		prepare: async (context) => {
			await context.addInitScript(() => (window.__datumKeptLayouts = 1));
			await opened(...names).prepare(context);
		}
	});
	const back = (page) => commands(page).locator('button[data-back]');
	/** To the place and back to `command`, by a tap on its tab, the bar remounted. */
	async function roundTrip(page) {
		const bar = await commands(page).elementHandle();
		await openPlace(page, 'sync');
		await page.locator('.root-bar [role="tab"]', { hasText: 'command' }).click();
		await until(() => rootTabs(page), (t) => t.join() === 'command');
		await commands(page).waitFor();
		assert.equal(await bar.evaluate((el) => el.isConnected), false, 'the place did not unmount the bar');
	}

	test('the commands widget is still inside `db` when you come back; Back there is kept too', async () => {
		await withApp(
			async ({ page, errors }) => {
				await commands(page).locator('button[data-command="db"]').click();
				await back(page).waitFor();
				await roundTrip(page);
				assert.equal(await back(page).count(), 1, 'the bar came back at the top');
				assert.equal(await commands(page).locator('button[data-command="db migrate"]').count(), 1);

				await back(page).click();
				await back(page).waitFor({ state: 'detached' });
				await roundTrip(page);
				assert.equal(await back(page).count(), 0, 'Back was undone by the round trip');
				assert.equal(await commands(page).locator('button[data-command="db"]').count(), 1);
				assert.deepEqual(errors, []);
			},
			{ ...PHONE, reducedMotion: 'reduce', ...alone('command') }
		);
	});

	// canary-kimi-k3 on #357: kept as a session setting is, so it goes with
	// the tab — a built-in layout reopened mints the same ids, and must not
	// come back drilled.
	test('closing the tab and reopening its layout starts the bar at the top', async () => {
		await withApp(
			async ({ page, errors }) => {
				await commands(page).locator('button[data-command="db"]').click();
				await back(page).waitFor();
				const id = await page.evaluate(() => window.datum.layout.current().root.tabs[0].id);
				await page.evaluate(() => window.datum.shell.close(0));
				await until(() => rootTabs(page), (t) => t.join() === 'tail');
				await page.evaluate(() => window.datum.shell.open('command'));
				await until(() => shown(page), (t) => t === 'command');
				await commands(page).waitFor();
				assert.equal(await page.evaluate(() => window.datum.layout.current().root.tabs.find((t) => t.layout === 'command').id), id, 'a new id: this proves nothing');
				assert.equal(await back(page).count(), 0, 'the reopened tab came back drilled');
				assert.deepEqual(errors, []);
			},
			{ ...PHONE, reducedMotion: 'reduce', ...opened('command', 'tail') }
		);
	});

	test('a held command comes back with its flags, and the line with the ones tapped', async () => {
		await withApp(
			async ({ page, errors }) => {
				const box = await commands(page).locator('button[data-command="occur"]').boundingBox();
				await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
				await page.mouse.down();
				await back(page).waitFor();
				await page.mouse.up();
				await commands(page).locator('button[data-flag="occur -q"]').click();
				await page.waitForFunction(() => document.querySelector('.prompt-bar input').value === 'occur -q ');
				await roundTrip(page);
				assert.equal(await commands(page).locator('button[data-flag="occur -t"]').count(), 1, 'the flags are gone');
				assert.equal(await page.locator('.prompt-bar input').inputValue(), 'occur -q ');

				await page.keyboard.press('Escape');
				await back(page).waitFor({ state: 'detached' });
				await roundTrip(page);
				assert.equal(await back(page).count(), 0, 'Escape was undone by the round trip');
				assert.deepEqual(errors, []);
			},
			{ ...PHONE, reducedMotion: 'reduce', ...alone('command') }
		);
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

// vividn, 2026-10-15: «yes typing sync should definitely open the sync widget
// on the web, also add a sync menu in the gear» (da-s9r58). Before, a fresh
// phone's typed `sync` left its answer behind the folded bar.
describe('the ways into Sync, on a fresh phone', () => {
	const fresh = { ...PHONE, home: null };

	test('a typed bare `sync` opens the Sync place', async () => {
		await withApp(async ({ page, errors }) => {
			// Typed, not `submit`: a fresh phone's page has no REPL to echo it in.
			const input = page.locator('.prompt-bar input');
			await input.fill('sync');
			await input.press('Enter');
			await until(() => shown(page), (t) => t === 'sync');
			await page.locator('.host[data-widget="sync"] .sync header').waitFor();
			assert.deepEqual(errors, []);
		}, fresh);
	});

	test('the gear lists Sync, which opens it', async () => {
		await withApp(async ({ page, errors }) => {
			await page.locator('.root-bar button.gear').click();
			await page.getByRole('menuitem', { name: 'Sync', exact: true }).click();
			await until(() => shown(page), (t) => t === 'sync');
			await page.locator('.host[data-widget="sync"] .sync header').waitFor();
			assert.deepEqual(errors, []);
		}, fresh);
	});
});
