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
// Its own «Close welcome» button went (da-6tk9t, vividn, 2026-11-09: «let's
// remove the "close welcome" button on the welcome tab»): it closes as any tab.
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
import { DESKTOP, chooser, exactly, manage, manageRoot, pane, rootMenuButton, titledPane, titles } from './support/layout.mjs';
import { until } from './support/until.mjs';
import { BAR, fold, unfold } from './support/bar.mjs';
import { relayed, startRelay, withWallClock } from './support/sync.mjs';

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

/** The widget kind the root tab in front shows, if it shows one: in a pane of its own (da-7nh4l). */
const front = (page) =>
	page.evaluate(() => {
		const { tabs, active } = window.datum.layout.current().root;
		const { content } = tabs[active];
		const [only] = content.children?.length === 1 ? content.children[0].tabs : [];
		return [tabs[active].layout, content.kind ?? only?.content.kind];
	});

/** The welcome widget's own text. */
const words = (page) => page.locator('.host.welcome').innerText();

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
				await page.locator('.host.setup .field-form button.track-something').waitFor();
				assert.deepEqual(await rootNames(page), FIRST);

				await page.locator('.root-bar [role="tab"]', { hasText: exactly('collection') }).click();
				await until(() => front(page), ([name]) => name === 'collect');
				await app.idle();
				assert.deepEqual(await titles(page), collect);
				assert.deepEqual(app.errors, []);
			}, device);
		});

	// ONE STATE FOR EVERY TAB (da-qu9j8), open on a first visit. vividn, 2026-11-11: «we should keep the output section
	// consistently open or closed (open by default) even on setup and welcome. That will prevent awkward resizes as
	// you switch tabs». It reversed setup's (da-339ec) and welcome's (da-djys5), which each shut it on arriving.
	for (const [what, device] of [
		['a desktop', FRESH_DESKTOP],
		['a phone', FRESH_PHONE]
	])
		test(`on ${what}, the output section opens on welcome and switching tabs never opens or closes it`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				const output = page.locator(`${BAR} .section.output`);
				await page.locator('.host.welcome').waitFor();
				await output.waitFor();
				const tour = async (open) => {
					for (const name of ['setup', 'collect', 'welcome', 'setup', 'collect']) {
						await page.evaluate((n) => window.datum.shell.open(n), name);
						await until(() => front(page), ([n]) => n === name);
						await app.idle();
						assert.equal(await output.count(), open ? 1 : 0, `on ${name} the output section is ${open ? 'closed' : 'open'}`);
					}
				};
				await tour(true);
				await fold(page);
				await tour(false);
				await unfold(page);
				await tour(true);
				assert.deepEqual(app.errors, []);
			}, device);
		});

	// Empty, it says what it is (da-qu9j8). vividn, 2026-11-11: «when the output section is empty maybe we shuold put
	// some help text (not too auffällig, but still legible)», and «give the output section a background texture/slightly
	// different color to differentiate it more from the input bar».
	test("an empty output section says what it is until the first command, on a ground of its own", async () => {
		await withApp(async (app) => {
			const { page } = app;
			const empty = page.locator(`${BAR} .section.output .empty`);
			await empty.waitFor();
			assert.match(await empty.innerText(), /output section/i);
			const ground = (sel) => page.locator(`${BAR} ${sel}`).evaluate((el) => getComputedStyle(el).backgroundColor);
			const [output, input, bar] = [await ground('.section.output .roll'), await ground('.section.input'), await ground('')];
			const drawn = input === 'rgba(0, 0, 0, 0)' ? bar : input;
			assert.notEqual(output, drawn, 'the output section is on the input row\'s ground');
			assert.notEqual(output, 'rgba(0, 0, 0, 0)');

			await page.locator(`${BAR} input`).fill('moment tea');
			await page.locator(`${BAR} input`).press('Enter');
			await page.locator(`${BAR} .section.output .group`).waitFor();
			assert.equal(await empty.count(), 0, 'the empty words stayed beside a command');
			assert.deepEqual(app.errors, []);
		}, FRESH_DESKTOP);
	});

	// What it HAS and DOES, not its sentences (vividn, 2026-10-28: «seems like tests that quote word for word are kind
	// of useless»): the copy is his to change without a test following it.
	// The docs are a button since da-0d8l6, opening the docs place: `docs.test.mjs`.
	// The guide for agents is in the «Integrate with AI» modal since da-rlxux.
	const BUTTONS = ['Track something', 'Tutorial', 'Read the docs', 'Integrate with AI'];
	const LINKS = ['https://github.com/vividn/datum'];

	test('what it has: the name, local first, Track something, the Tutorial button, the docs, Integrate with AI and the repo, and no button of its own to close it', async () => {
		await withApp(async ({ page }) => {
			const host = page.locator('.host.welcome');
			assert.equal(await host.getByRole('heading', { level: 1 }).innerText(), 'datum');
			// Back from the Introduction (da-uzyih.4869, vividn, 2026-10-27).
			assert.match(await words(page), /local first/i);
			const controls = await host.locator('div.welcome').locator('button, a').evaluateAll((all) =>
				all.map((e) => (e.tagName === 'A' ? e.getAttribute('href') : e.textContent.trim()))
			);
			assert.deepEqual(controls, [...BUTTONS, ...LINKS]);
			// No worded button outside them: no «Close welcome» (da-6tk9t). The frame's own are icons.
			const worded = (await host.getByRole('button').allInnerTexts()).map((t) => t.trim()).filter(Boolean);
			assert.deepEqual(worded, BUTTONS);
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

	// da-a6xba, vividn, 2026-11-10: «Let's move the local first paragraph down beneath the AI paragraph (so right above
	// the open source section). Above the Tutorial, let's add a filled in button. "track something" that redirects you to
	// the setup tab.»
	for (const [what, device] of [
		['a phone', FRESH_PHONE],
		['a desktop', FRESH_DESKTOP]
	])
		test(`on ${what}, Track something stands filled above Tutorial and opens setup; local first comes after the AI button, before open source`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				const welcome = page.locator('.host.welcome div.welcome');
				const track = welcome.getByRole('button', { name: 'Track something', exact: true });
				const tutorial = welcome.getByRole('button', { name: 'Tutorial', exact: true });
				// Above Tutorial, drawn filled in the accent where Tutorial is the page's ground.
				assert.ok((await track.boundingBox()).y < (await tutorial.boundingBox()).y, 'Track something is not above Tutorial');
				const [filled, ground, accent] = await track.evaluate((button) => {
					const probe = document.createElement('div');
					probe.style.color = 'var(--accent)';
					button.parentElement.append(probe);
					const accent = getComputedStyle(probe).color;
					probe.remove();
					const tutorial = [...button.closest('div.welcome').querySelectorAll('button')].find((b) => b.textContent.trim() === 'Tutorial');
					return [getComputedStyle(button).backgroundColor, getComputedStyle(tutorial).backgroundColor, accent];
				});
				assert.equal(filled, accent);
				assert.notEqual(ground, accent);

				// The paragraphs' order: the AI button, then local first, then open source, last.
				const order = await welcome.locator('p').evaluateAll((all) =>
					all.map((p) => p.textContent).flatMap((t) => (/Integrate with AI/.test(t) ? ['ai'] : /local first/i.test(t) ? ['local'] : /open source/i.test(t) ? ['source'] : []))
				);
				assert.deepEqual(order, ['ai', 'local', 'source']);

				await track.click();
				await until(() => front(page), ([name]) => name === 'setup');
				assert.deepEqual(await front(page), ['setup', 'setup']);
				await page.locator('.host.setup h3', { hasText: 'What do you want to track?' }).waitFor();
				assert.deepEqual(await rootNames(page), FIRST);
				assert.deepEqual(app.errors, []);
			}, device);
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
	// there to help you get setup with your data.» A button since da-rlxux, on a phone's first screen.
	test('its Integrate with AI button is on a phone\'s first screen', async () => {
		await withApp(async ({ page }) => {
			const line = page.locator('.host.welcome p', { has: page.getByRole('button', { name: 'Integrate with AI', exact: true }) });
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

	// da-rlxux, vividn, 2026-11-12: «an "integrate with AI" button that pops up a modal with an explanation as well
	// as … a copyable prompt to paste into a chat ai». It opens a modal holding the guide for agents and a prompt
	// naming it, its copy puts that prompt on the clipboard, and it closes as the app's other sheets do.
	for (const [what, device] of [
		['a phone', FRESH_PHONE],
		['a desktop', FRESH_DESKTOP]
	])
		test(`on ${what}, Integrate with AI opens a modal whose Copy prompt copies its prompt; ×, Escape and the backdrop close it`, async () => {
			await withApp(async (app) => {
				const { page } = app;
				await page.evaluate(() => {
					window.__copied = [];
					Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: (t) => (window.__copied.push(t), Promise.resolve()) } });
				});
				const button = page.locator('.host.welcome').getByRole('button', { name: 'Integrate with AI', exact: true });
				const sheet = page.locator('dialog[data-ai]');
				const opened = async () => {
					await button.click();
					await sheet.waitFor();
					assert.equal(await sheet.evaluate((d) => d.open && d.matches(':modal')), true, 'not a modal');
				};

				// It opens a sheet, so wears the `›`; Copy prompt acts at once (act.ts).
				assert.equal(await button.getAttribute('data-act'), 'open');
				await opened();
				assert.equal(await sheet.getByRole('button', { name: 'Copy prompt', exact: true }).getAttribute('data-act'), 'run');
				assert.equal(await sheet.locator('a[href="https://datum.day/agents.md"]').getAttribute('target'), '_blank');
				const prompt = await sheet.locator('pre.prompt').textContent();
				assert.match(prompt, /https:\/\/datum\.day\/agents\.md/);
				await sheet.getByRole('button', { name: 'Copy prompt', exact: true }).click();
				await until(() => page.evaluate(() => window.__copied), (c) => c.length === 1);
				assert.deepEqual(await page.evaluate(() => window.__copied), [prompt]);
				await sheet.getByRole('status').filter({ hasText: /\S/ }).waitFor();

				await sheet.getByRole('button', { name: 'close' }).click();
				await sheet.waitFor({ state: 'detached' });
				await opened();
				// Said once copied, and not again on the next open.
				assert.equal((await sheet.getByRole('status').textContent()).trim(), '');
				await page.keyboard.press('Escape');
				await sheet.waitFor({ state: 'detached' });
				await opened();
				await page.mouse.click(2, 2);
				await sheet.waitFor({ state: 'detached' });
				assert.deepEqual(app.errors, []);
			}, device);
		});

	// «fits a phone screen without scrolling» again (da-uzyih.19), as it was until 2026-10-14.
	test('on a phone it fits the screen without scrolling', async () => {
		await withApp(async ({ page }) => {
			const box = await page.locator('.host.welcome').evaluate((host) => {
				let el = host.querySelector('div.welcome');
				const bottom = Math.max(...[...host.querySelectorAll('div.welcome')].map((e) => e.getBoundingClientRect().bottom));
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
	// the harness otherwise empties for a device with a stored list. A device
	// from before the record of shipped tabs it was given counts as given every
	// tab shipped now (da-3eegv, vividn: «sure»), so an early device on `main`
	// alone is asked nothing and keeps it: the tabs are in the `+`.
	test('a device from before the welcome tab keeps its own tabs, unasked', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal(await page.locator('dialog.confirm').count(), 0);
			assert.deepEqual(await rootNames(page), ['main']);

			await app.reload();
			assert.equal(await page.locator('dialog.confirm').count(), 0);
			assert.deepEqual(await rootNames(page), ['main']);
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
// sentence that says what a field is (WelcomeText.svelte), of its «Track something» button and of the gear's «Track timezone» (da-yw6o2,
// vividn's word for it): no title, button, tab or help of the widget. The docs the
// docs widget bundles (da-0d8l6) are the repo's `docs/` as written, not the app's
// words, so a chunk that is one (it holds a doc's title line) is not read.
test('the built app says «Track» only as a verb', () => {
	const build = fileURLToPath(new URL('../build', import.meta.url));
	const docs = new URL('../../docs/', import.meta.url);
	const titles = readdirSync(docs, { recursive: true })
		.filter((f) => f.endsWith('.md'))
		.map((f) => readFileSync(new URL(f, docs), 'utf8').split('\n')[0]);
	const said = readdirSync(build, { recursive: true })
		.filter((f) => /\.(js|html)$/.test(f))
		.map((f) => readFileSync(join(build, f), 'utf8'))
		.filter((text) => !titles.some((title) => text.includes(title)))
		.flatMap((text) => text.match(/\bTrack\b(?! timezone| something).{0,20}/g) ?? []);
	assert.deepEqual([...new Set(said)], ['Track what matters to you']);
});

// da-f7orx, vividn, 2026-11-12: «a way to generate a nice AI code». The modal makes an agent's code with the Sync
// widget's own form (AgentForm.svelte), 30 days preselected, and a database in no chain starts one first, as its first
// share does (da-5wlnr): `sync chain agent` refuses a copy in none.
describe('Generate an AI code', () => {
	let relay;
	before(async () => (relay = await startRelay()));
	after(() => relay?.stop());

	const chainOf = async (app) => JSON.parse((await app.run(['sync', 'chain', 'list', '--json'])).stdout).chain;

	/** The modal opened, «Generate an AI code» pressed, saying it starts a chain where `fresh`, and its code made: the card. */
	async function generate(page, fresh) {
		await page.locator('.host.welcome').getByRole('button', { name: 'Integrate with AI', exact: true }).click();
		const sheet = page.locator('dialog[data-ai]');
		await sheet.getByRole('button', { name: 'Generate an AI code', exact: true }).click();
		const form = sheet.locator('form.card.agent');
		assert.equal(await form.getByLabel('It expires').inputValue(), '30d');
		assert.equal(await sheet.getByText('starts its sync chain first').count(), fresh ? 1 : 0);
		await form.getByLabel('Name').fill('claude');
		await form.getByRole('button', { name: 'Make code' }).click();
		const card = sheet.locator('.agentcode');
		assert.match((await card.locator('.ticket').textContent()).trim(), /^datum-agent:[A-Z2-7]+$/);
		// What the code lets it do, and until when.
		assert.equal((await card.locator('.can').textContent()).trim(), 'A reader: it reads everything and writes nothing.');
		const on = await page.evaluate(() => new Date(Date.now() + 30 * 864e5).toLocaleDateString([], { dateStyle: 'medium' }));
		assert.ok((await card.textContent()).includes(on), on);
		// The chain started, the line saying it would goes.
		assert.equal(await sheet.getByText('starts its sync chain first').count(), 0);
	}

	test('on a database in no chain, it starts one, says so, and makes a code; 30 days preselected', async () => {
		await withWallClock(
			async (app) => {
				assert.equal(await chainOf(app), null);
				await generate(app.page, true);
				assert.notEqual(await chainOf(app), null);
				assert.deepEqual(app.errors, []);
			},
			{ ...FRESH_DESKTOP, prepare: relayed(relay) }
		);
	});

	test('on a chained database, it makes a code in that chain and starts no other', async () => {
		await withWallClock(
			async (app) => {
				assert.equal((await app.run(['sync', 'new-invite'])).code, 0);
				const chain = await chainOf(app);
				assert.notEqual(chain, null);
				await generate(app.page, false);
				assert.equal(await chainOf(app), chain);
				assert.deepEqual(app.errors, []);
			},
			{ ...FRESH_DESKTOP, prepare: relayed(relay) }
		);
	});
});
