// The tutorial's frame (da-6cvrd): a database of its own, «tutorial», that
// the welcome's «Tutorial» button switches to.
//
// The bead's own «done when», on a phone and a fresh datum.day: the tap
// recolors the app, the Introduction shows the way back, going back lands
// on «datum» untouched — no tutorial entry in it, and its own tabs — and a
// second visit to «tutorial» keeps what was there. `sync` there is refused.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { HELD_AT, ready, startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';
import { cuedRoom, isStatus, settled, startSwitch, statuses } from './support/tutorial.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

// The lessons' layouts, by name; the change lesson's is named `switch`, and its tab says its kind (`LAYOUT_TITLES`, da-kjkpx).
const LESSON_NAMES = ['intro', 'moment', 'on-off', 'switch', 'input-bar', 'tabs'];
/** What a lesson's tab says. */
const tabLabel = (name) => (name === 'switch' ? 'change' : name);
const LESSON_TABS = LESSON_NAMES.map(tabLabel);
const tabs = (page) => page.locator('.root-bar [role="tab"]').allInnerTexts();
const front = (page) => page.locator('.root-bar [role="tab"][aria-selected="true"]').innerText();
const hue = (page) => page.evaluate(() => document.documentElement.style.getPropertyValue('--hue'));
const db = (page) => page.evaluate(() => new URLSearchParams(location.search).get('db'));

/** Press `name` and wait for the page it loads, on database `to` (null: datum). */
async function press(page, name, to) {
	await Promise.all([
		page.waitForURL((url) => url.searchParams.get('db') === to),
		page.getByRole('button', { name, exact: true }).click()
	]);
	await ready(page);
}

/** Choose `name` in the database switcher, the bottom bar's name, and wait for its page. */
async function viaSwitcher(page, name, to) {
	await page.getByRole('button', { name: /choose a database/ }).click();
	await Promise.all([
		page.waitForURL((url) => url.searchParams.get('db') === to),
		page.getByRole('menuitemradio', { name, exact: true }).click()
	]);
	await ready(page);
}

test('the Tutorial button opens the tutorial database, and the way back leaves datum untouched', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const home = await tabs(page);
			const datumHue = await hue(page);
			assert.deepEqual(home, ['welcome', 'premade', 'setup', 'collection', 'dayview'], 'a fresh datum.day opens on the welcome');

			await press(page, 'Tutorial', 'tutorial');
			assert.equal(await db(page), 'tutorial');
			assert.notEqual(await hue(page), datumHue, 'the app is recolored');
			assert.deepEqual(await tabs(page), LESSON_TABS, 'its lessons, in order');
			const named = await page.locator('section.pane').first().locator('p b').allInnerTexts();
			assert.ok(named.includes('datum'), `the Introduction names the database to go back to: ${named}`);

			// Practice, and the refusal: the kit says the database never syncs.
			assert.equal((await app.run(['occur', 'practice'])).code, 0);
			const sync = await app.run(['sync']);
			assert.notEqual(sync.code, 0, 'sync is refused');
			assert.match(sync.stderr, /never syncs/);

			await viaSwitcher(page, 'datum', null);
			assert.equal(await db(page), null);
			assert.equal(await hue(page), datumHue);
			assert.equal(await page.evaluate(() => document.documentElement.dataset.pointAt), undefined);
			const leaked = JSON.parse((await app.run(['tail', '--json', '-n', '50'])).stdout).rows;
			assert.deepEqual(leaked, [], 'no tutorial entry in datum');

			// Each database's open tabs are its own, both ways and over a reload (vividn, 19:13: «tabs
			// should be per database»).
			assert.deepEqual(await tabs(page), home, "datum's tabs, not the tutorial's");
			assert.equal(await front(page), 'welcome');
			await app.reload();
			assert.deepEqual(await tabs(page), home, "a reload keeps datum's");

			// A second visit keeps the practice, its tabs, and one copy of each.
			await press(page, 'Tutorial', 'tutorial');
			assert.deepEqual(await tabs(page), LESSON_TABS, "the tutorial's, not datum's");
			const kept = JSON.parse((await app.run(['tail', 'practice', '--json'])).stdout).rows;
			assert.equal(kept.length, 1, 'the practice entry is kept');
			const layouts = JSON.parse((await app.run(['tail', 'layout', '--json', '-n', '50'])).stdout).rows;
			assert.deepEqual(
				layouts.filter((r) => r.data?.kit_id === 'tutorial').map((r) => r.data.name).sort(),
				[...LESSON_NAMES].sort(),
				'each tab installed once'
			);
			await page.locator('.root-bar [role="tab"]', { hasText: 'moment' }).click();
			await app.reload();
			assert.deepEqual(await tabs(page), LESSON_TABS, "a reload keeps the tutorial's");
			assert.equal(await front(page), 'moment');

			// And the way the Introduction teaches, the switcher, there and back.
			await viaSwitcher(page, 'datum', null);
			assert.deepEqual(await tabs(page), home);
			await viaSwitcher(page, 'tutorial', 'tutorial');
			assert.equal(await front(page), 'moment', 'with the lesson the reader left in front');
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, home: null }
	);
});

// A tutorial made before its kit's fields changed has its tabs already, and
// still gets the fields (da-uzyih.4881): the kit installs at every load.
test('a tutorial whose tabs are current still gets a kit field it lacks, at the next load', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			// `until`, not waitForFunction: that takes the promise a run returns as truthy and waits for nothing.
			const listed = () => page.evaluate(() => window.datum.runQuiet(['fields', '--json']).then((r) => r.stdout.includes('"field": "tv"')));
			const tv = (want) => until(listed, (seen) => seen === want);
			await tv(true);
			assert.equal((await app.run(['delete', 'spec:tv'])).code, 0);
			await tv(false); // as in a tutorial from before the kit had it
			await app.reload();
			await tv(true);
			await page.locator('.root-bar [role="tab"]', { hasText: 'on-off' }).click();
			await page.locator('[data-pane$="-fields"] .fields button[data-field="tv"]').waitFor();
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-uzyih.4869, vividn, 2026-10-27: «the first paragraph should mention how to switch back to the main database.
// Like: ues the menu at the bottom to switch databases to leave the tutorial (where?) where is a link that animates
// an arrow to the bottom and highlights the switching menu. It doesn't need to be constantly flashing like right now».
// vividn's own rewording (da-uzyih.4898) puts it after the welcome and how to move through the tabs.
test('the Introduction says how to leave, and «where?» rings the switcher for a moment, not before', async () => {
	await withApp(
		async ({ page }) => {
			await press(page, 'Tutorial', 'tutorial');
			const leave = page.locator('section.pane').first().locator('p', { has: page.getByRole('button', { name: 'where?' }) });
			// It names the database to leave and the one to choose, and holds «where?», clicked below.
			assert.deepEqual(await leave.locator('b').allInnerTexts(), ['tutorial', 'datum']);
			const brand = page.getByRole('button', { name: /choose a database/ });
			const ring = () => brand.evaluate((b) => getComputedStyle(b).outlineStyle);
			assert.equal(await ring(), 'none', 'nothing rings the switcher before «where?»');
			assert.equal(await page.evaluate(() => document.documentElement.dataset.pointAt), undefined);

			await leave.getByRole('button', { name: 'where?', exact: true }).click();
			assert.equal(await page.evaluate(() => document.documentElement.dataset.pointAt), 'switcher');
			assert.equal(await ring(), 'solid', 'the switcher is ringed after «where?»');
			// The arrow falls to it, and is gone after.
			await page.locator('.point-arrow').waitFor({ state: 'attached' });
			await page.locator('.point-arrow').waitFor({ state: 'detached' });
			// For a moment: the page's held clock does not hold timers.
			await page.waitForFunction(() => document.documentElement.dataset.pointAt === undefined);
			assert.equal(await ring(), 'none');
		},
		{ ...PHONE, home: null }
	);
});

// da-uzyih.4871, vividn, 2026-10-27: «on the introduction at the bottom, but a button to continue to the next tab
// rather than go back to datum, maybe with a little let's learn a bit more about each shape line».
// da-uzyih.4908: the next tab is the café; the shapes are docs/basics.md.
test('the Introduction ends on the next tab, the café, and not on the way back to datum', async () => {
	await withApp(
		async ({ page }) => {
			await press(page, 'Tutorial', 'tutorial');
			assert.equal(await front(page), 'intro');
			const intro = page.locator('section.pane').first();
			// Its one way on is the next tab: no way back.
			await intro.getByRole('button', { name: 'where?', exact: true }).waitFor();
			assert.equal(await intro.locator('.tutorial').getByRole('button', { name: /back/i }).count(), 0);
			await intro.getByRole('button', { name: 'Next: moment', exact: true }).click();
			await page.waitForFunction(
				() => document.querySelector('.root-bar [role="tab"][aria-selected="true"]')?.textContent.trim() === 'moment'
			);
			await page.locator('[data-pane="moment-lesson"] .game').waitFor();
			assert.equal(await db(page), 'tutorial', 'the same database, another tab');
			assert.deepEqual(await tabs(page), LESSON_TABS);
		},
		{ ...PHONE, home: null }
	);
});

// da-uzyih.4892, vividn, 2026-10-28: «add advancement buttons at the bottom of every page on the tutorial like there
// is on the introduction. "Next: on/off"». Each names the next lesson by the word its title leads with.
test('every lesson ends on a button to the next, from the Introduction to the last, which has none', async () => {
	await withApp(
		async ({ page }) => {
			await press(page, 'Tutorial', 'tutorial');
			const labels = ['moment', 'on/off', 'change', 'The input bar', 'Tabs and widgets'];
			for (const [i, label] of labels.entries()) {
				assert.equal(await front(page), LESSON_TABS[i]);
				const next = page.getByRole('button', { name: `Next: ${label}`, exact: true });
				// The Introduction's Next has its reset under it (da-uzyih.4893), and nothing after that.
				const atEnd = await next.evaluate(
					(b, intro) =>
						b.parentElement.classList.contains('tutorial') &&
						(intro ? b.nextElementSibling?.textContent === 'Reset the tutorial' && !b.nextElementSibling.nextElementSibling : !b.nextElementSibling),
					i === 0
				);
				assert.ok(atEnd, `${LESSON_TABS[i]} ends on the next lesson, ${label}`);
				await next.click();
				await page.waitForFunction(
					(name) => document.querySelector('.root-bar [role="tab"][aria-selected="true"]')?.textContent.trim() === name,
					LESSON_TABS[i + 1]
				);
			}
			assert.equal(await front(page), 'tabs');
			await until(() => page.getByRole('button', { name: /^Next:/ }).count(), (n) => n === 0);
		},
		{ ...PHONE, home: null }
	);
});

// Under reduced motion the ring comes alone, without the arrow, and does not pulse.
test('«where?» under reduced motion rings the switcher with no arrow', async () => {
	await withApp(
		async ({ page }) => {
			await press(page, 'Tutorial', 'tutorial');
			await page.getByRole('button', { name: 'where?', exact: true }).click();
			const brand = page.getByRole('button', { name: /choose a database/ });
			assert.deepEqual(await brand.evaluate((b) => [getComputedStyle(b).outlineStyle, getComputedStyle(b).animationName]), [
				'solid',
				'none'
			]);
			assert.equal(await page.locator('.point-arrow').count(), 0);
			// And it clears, as with motion.
			await page.waitForFunction(() => document.documentElement.dataset.pointAt === undefined);
			assert.equal(await brand.evaluate((b) => getComputedStyle(b).outlineStyle), 'none');
		},
		{ ...PHONE, home: null, reducedMotion: 'reduce' }
	);
});

// da-uzyih.19, vividn, 2026-10-22: «A lot of the information on the welcome screen should go inside the tutorial
// welcome screen instead». da-uzyih.4908, vividn, 2026-10-29: «Let's remove the "first of all" section … The first 3
// paragraphs are fine, but then we should transition to the cafe»: why keep data, the terms and the shapes are
// docs/basics.md, and no tab holds them.
test('the Introduction is its three paragraphs and its way on; no tab holds why keep data, the terms or the shapes', async () => {
	await withApp(
		async ({ page }) => {
			await press(page, 'Tutorial', 'tutorial');
			const intro = page.locator('section.pane').first().locator('.tutorial');
			await intro.getByRole('button', { name: 'Next: moment', exact: true }).waitFor();
			const parts = await intro.evaluate((root) => [...root.children].map((el) => (el.tagName === 'BUTTON' ? el.textContent.trim() : el.tagName)));
			assert.deepEqual(parts, ['H1', 'P', 'P', 'P', 'Next: moment', 'Reset the tutorial']);
			for (const tab of LESSON_TABS) {
				await page.locator('.root-bar [role="tab"]', { hasText: new RegExp(`^${tab}$`) }).click();
				await until(() => front(page), (f) => f === tab);
				await page.locator('.host[data-widget="tutorial"]:visible h1').first().waitFor();
				const held = await page.locator('.host[data-widget="tutorial"]:visible').evaluateAll((hosts) =>
					hosts.flatMap((h) => [...h.querySelectorAll('h3, table, ul.shapes')].map((el) => el.outerHTML.slice(0, 60)))
				);
				assert.deepEqual(held, [], `${tab} holds no section, terms table or shapes`);
				const text = await page.locator('.host[data-widget="tutorial"]:visible').allInnerTexts();
				assert.doesNotMatch(text.join('\n'), /why keep data/i);
			}
		},
		{ ...PHONE, home: null }
	);
});

// da-uzyih.4898: a tutorial opened before keeps its first tab, renamed; the tab it was on stays in front. da-uzyih.4908:
// its shapes tab goes, and if it was in front, the café comes to the front. da-uzyih.4930: its occur tab is moment.
test('a tutorial opened before: «introduction» is «intro», «occur» is «moment», «shapes» goes, and its front tab stays, the café for the shapes', async () => {
	await withApp(
		async ({ page }) => {
			await press(page, 'Tutorial', 'tutorial');
			for (const [names, active, expected] of [
				[['introduction', 'occur', 'on-off', 'switch', 'input-bar', 'tabs'], 2, 'on-off'],
				[['intro', 'shapes', 'occur', 'on-off', 'switch', 'input-bar', 'tabs'], 3, 'on-off'],
				[['intro', 'shapes', 'occur', 'on-off', 'switch', 'input-bar', 'tabs'], 1, 'moment'],
				[['intro', 'occur', 'on-off', 'switch', 'input-bar', 'tabs'], 1, 'moment']
			]) {
				await page.evaluate(
					([names, active]) => localStorage.setItem('datum.layout.root.v1.tutorial', JSON.stringify({ names, active })),
					[names, active]
				);
				await page.reload();
				await ready(page);
				await until(() => tabs(page), (t) => JSON.stringify(t) === JSON.stringify(LESSON_TABS));
				assert.equal(await front(page), expected, `${names[active]} in front of ${names}`);
			}
		},
		{ ...PHONE, home: null }
	);
});

test('the two text tabs after the switch lesson: the input bar, then tabs and widgets, with nothing to start over', async () => {
	await withApp(
		async ({ page }) => {
			await press(page, 'Tutorial', 'tutorial');
			const tabNames = await tabs(page);
			assert.deepEqual(tabNames.slice(tabNames.indexOf('change')), ['change', 'input-bar', 'tabs']);
			for (const [tab, title] of [
				['input-bar', 'The input bar'],
				['tabs', 'Tabs and widgets']
			]) {
				await page.locator('.root-bar [role="tab"]', { hasText: new RegExp(`^${tab}$`) }).click();
				const pane = page.locator('.host[data-widget="tutorial"]:visible');
				await pane.locator('h1', { hasText: title }).waitFor();
				assert.ok(await pane.locator('ul li').count(), `${tab} lists what it teaches`);
				assert.equal(await pane.getByRole('button', { name: 'Start this tab over' }).count(), 0);
			}
		},
		{ ...PHONE, home: null }
	);
});

// da-uzyih.4864, vividn: «a start over button, that just resets the game
// and makes any states that are currently active for that game false, but
// do not delete any data. Do not prompt about the starting over». Since
// da-uzyih.4910 (vividn, 2026-10-29: «get rid of the "Undo" and "Start Over"
// buttons … After the game is over the start button can read "Reset" which
// restarts the game to it's prestarted state») it is Start's, once a game is
// over: no game has an Undo or a Start over.
test('a game over, Start reads «Reset»: the neutral scene again, its fields’ active states ended, nothing deleted or asked; no Undo, no Start over', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const active = () => app.run(['now', '--json']).then((r) => JSON.parse(r.stdout).active.map((a) => `${a.field}=${a.state}`).sort());
			const listed = async (fields) => (await Promise.all(fields.map((f) => app.run(['tail', f, '--json', '-n', '50']).then((r) => JSON.parse(r.stdout).rows)))).flat();
			// A field of no game's, running all along.
			assert.equal((await app.run(['start', 'other', '-t', '-5'])).code, 0);

			for (const tab of ['moment', 'on-off', 'switch']) {
				const lesson = await page.evaluate((tab) => window.datum.tutorial.lessons.find((l) => l.name === tab), tab);
				const pane = `[data-pane="${tab}-lesson"]`;
				const game = page.locator(`${pane} .game`);
				const named = (name) => page.getByRole('button', { name, exact: true });
				// The game's pane and its fields' pane: the bar's own Undo, under its `❯`, is not the game's.
				const panes = page.locator(`${pane}, [data-pane="${tab}-fields"]`);
				const gone = async (when) => {
					assert.equal(await panes.count(), 2, `${tab}: both panes`);
					for (const name of ['Start over', '↶ Undo', 'Undo']) assert.equal(await panes.getByRole('button', { name, exact: true }).count(), 0, `${tab}, ${when}: no ${name}`);
				};
				await page.locator('.root-bar [role="tab"]', { hasText: tabLabel(tab) }).click();
				await gone('at rest');
				if (tab === 'switch') await startSwitch(page);
				else await named('Start').click();
				await named('End now').waitFor();
				await settled(page, pane);
				await gone('mid-game');
				await app.clock.forward(1000);
				await named('End now').click();
				await page.locator(`${pane} .game[data-phase="done"][aria-busy="false"]`).waitFor();
				await gone('over');
				assert.equal(await game.getByRole('button').allInnerTexts().then((t) => t.join()), 'Reset', `${tab}: over, Start reads Reset`);

				// After the game, the reader logs on: Reset ends what is active of the game's fields.
				await app.clock.forward(1000);
				const logs = { moment: [], 'on-off': [['start', 'tv'], ['start', 'reading']], switch: [['switch', 'room', 'kitchen']] }[tab];
				for (const argv of logs) assert.equal((await app.run(argv)).code, 0);
				const states = { moment: [], 'on-off': ['reading=true', 'tv=true'], switch: ['room=kitchen'] }[tab];
				assert.deepEqual((await active()).filter((a) => !a.startsWith('other')), states, `${tab}: the states logged after it active`);
				const before = (await listed(lesson.fields)).map((row) => row.hid);

				await app.clock.forward(1000);
				await named('Reset').click();
				await game.and(page.locator('[aria-busy="true"]')).waitFor({ state: 'detached' });
				assert.equal(await game.getAttribute('data-phase'), 'neutral', `${tab}: the game at rest again`);
				if (tab === 'switch') await game.locator('.cue').waitFor();
				else await named('Start').waitFor();
				assert.equal(await named('Reset').count(), 0, `${tab}: at rest, no Reset`);
				assert.equal(await page.locator('[role="dialog"], [role="alertdialog"]').count(), 0, `${tab}: nothing asked`);
				assert.deepEqual(await active(), ['other=true'], `${tab}: none of the game's states active, another field's untouched`);
				const after = (await listed(lesson.fields)).map((row) => row.hid);
				assert.ok(before.every((h) => after.includes(h)), `${tab}: nothing deleted`);
				// An end a state, or room's IGNORED, room being never_false.
				assert.equal(after.length, before.length + states.length, `${tab}: one write a state`);
			}
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-2chdp, vividn, 2026-11-05: «nowview, game (should be called game widget) and collect widget». A lesson's game
// is the `game` kind, its pane named «game widget»; a layout saved while the game was the `tutorial` kind's still
// runs its game.
test('a lesson’s game is a pane named «game widget» that runs a round; a tab saved with the old tutorial game pane still runs it', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const pane = page.locator('[data-pane="moment-lesson"]');
			/** The moment tab's game pane's kind and title, and a round played in it to its score. */
			const round = async () => {
				await page.locator('.root-bar [role="tab"]', { hasText: /^moment$/ }).click();
				await pane.locator('.game').waitFor();
				const kind = await page.locator('.host:has(> [data-pane="moment-lesson"])').getAttribute('data-widget');
				const title = await pane.getAttribute('data-title');
				await pane.getByRole('button', { name: 'Start', exact: true }).click();
				await pane.getByRole('button', { name: 'End now', exact: true }).waitFor();
				// The begin's TRACKED landed before the clock moves: run after it, it would share End now's instant.
				await settled(page, '[data-pane="moment-lesson"]');
				await app.clock.forward(1000);
				await pane.getByRole('button', { name: 'End now', exact: true }).click();
				await pane.locator('.game[data-phase="done"][aria-busy="false"] .result').waitFor();
				await pane.getByRole('button', { name: 'Reset', exact: true }).click();
				await pane.getByRole('button', { name: 'Start', exact: true }).waitFor();
				return [kind, title];
			};
			assert.deepEqual(await round(), ['game', 'game widget']);

			// The tab as a layout saved before the game was its own kind: the game pane the `tutorial` kind's.
			const old = await page.evaluate(() => JSON.parse(JSON.stringify(window.datum.layouts.kitLayout('moment')).replaceAll('"kind":"game"', '"kind":"tutorial"')));
			// The about, the discussion (da-zb1dr), and the game pane, the tutorial kind again.
			assert.equal(JSON.stringify(old).match(/"kind":"tutorial"/g).length, 3, 'the game pane is the tutorial kind again');
			await page.evaluate((l) => window.datum.layouts.saveLayout(l), old);
			await app.reload();
			assert.deepEqual(await round(), ['tutorial', 'tutorial']);
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-uzyih.4882, vividn, 2026-10-27 18:22: «the fields on the other tutorial
// pages shoudl also be IGNORED after the game ends, and TRACKED when it
// begins». Each game is played twice, ended by its time and by «End now»: its
// fields TRACKED once it has begun and IGNORED once it has ended, read back
// from `datum now --json -A`. Reset then leaves them IGNORED and writes
// nothing. The switch game begins again, after Reset brings its cue back
// (da-uzyih.4880), by a tap on `room` while it is IGNORED: the collect widget
// draws an IGNORED field, and a tap on it logs.
test('each game’s fields are TRACKED once it begins and IGNORED once it ends, by its time and by «End now»', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const count = async (fields) =>
				(await Promise.all(fields.map((f) => app.run(['tail', f, '--json', '-n', '200']).then((r) => JSON.parse(r.stdout).rows.length)))).reduce((a, b) => a + b);
			for (const tab of ['moment', 'on-off', 'switch']) {
				const { fields } = await page.evaluate((tab) => window.datum.tutorial.lessons.find((l) => l.name === tab), tab);
				const pane = `[data-pane="${tab}-lesson"]`;
				const game = page.locator(`${pane} .game`);
				const all = (status) => Object.fromEntries(fields.map((f) => [f, status]));
				await page.locator('.root-bar [role="tab"]', { hasText: tabLabel(tab) }).click();
				for (const how of ['time', 'End now']) {
					if ((await game.getAttribute('data-phase')) === 'done') {
						await page.getByRole('button', { name: 'Reset', exact: true }).click();
						await settled(page, pane);
					}
					if (tab === 'switch') await startSwitch(page);
					else await page.getByRole('button', { name: 'Start', exact: true }).click();
					await settled(page, pane);
					assert.deepEqual(await statuses(app, fields), all('TRACKED'), `${tab}, before ${how}: TRACKED once it begins`);

					// The clock is held: an IGNORED at the TRACKED's instant would collide with it.
					await app.clock.forward(1000);
					if (how === 'time') await app.clock.forward(3_600_000);
					else await page.getByRole('button', { name: how, exact: true }).click();
					await page.locator(`${pane} .game[data-phase="done"][aria-busy="false"]`).waitFor();
					assert.deepEqual(await statuses(app, fields), all('IGNORED'), `${tab}, ended by ${how}: IGNORED`);
					assert.equal(await game.locator('.failed').count(), 0, `${tab}, ${how}: nothing failed`);
					await app.clock.forward(1000);
				}
				const before = await count(fields);
				await page.getByRole('button', { name: 'Reset', exact: true }).click();
				await settled(page, pane);
				assert.deepEqual(await statuses(app, fields), all('IGNORED'), `${tab}: Reset leaves them IGNORED`);
				assert.equal(await count(fields), before, `${tab}: and writes nothing`);
			}
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// vividn, 2026-10-27 15:35: «The fields widget should always be active, even
// when the game is not going, so that you can experiment with logging data
// outside the game». A tap before Start and one after the game logs as any
// tap does; neither starts a game or is scored. The switch game starts by a
// switch of room to the room its cue names, so its two taps are of
// other rooms.
test('the fields log outside a game too, and no game starts or scores by it', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const count = (field) => app.run(['tail', field, '--json', '-n', '50']).then((r) => JSON.parse(r.stdout).rows.length);
			const tap = async (tab, field, state) => {
				const before = await count(field);
				await page.locator(`[data-pane="${tab}-fields"] .fields button[data-field="${field}"]`).click();
				if (state) await page.locator(`[data-pane="${tab}-fields"] .fields .ring button[data-state="${state}"]`).click();
				await until(() => count(field), (n) => n === before + 1);
			};
			/** A room neither the cue's, where it is drawn (at rest alone, da-uzyih.4880), nor `held`. */
			const another = async (held) => {
				const cue = (await page.locator('[data-pane="switch-lesson"] .game .cue').count()) ? await cuedRoom(page) : null;
				return ['kitchen', 'bedroom', 'bathroom'].find((r) => r !== cue && r !== held);
			};
			await page.evaluate(() => (window.datum.tutorial.cafe.round.seed = 7));
			for (const { tab, first, then } of [
				{ tab: 'moment', first: async () => ['tea'], then: async () => ['coffee'] },
				{ tab: 'on-off', first: async () => ['tv'], then: async () => ['phone'] },
				{ tab: 'switch', first: async () => ['room', await another()], then: async (held) => ['room', await another(held)] }
			]) {
				await page.locator('.root-bar [role="tab"]', { hasText: tabLabel(tab) }).click();
				const game = page.locator(`[data-pane="${tab}-lesson"] .game`);
				const result = () => game.locator('.result').innerText();
				// At their own instants: the clock is held, and two writes at one would collide.
				await app.clock.forward(1000);
				await tap(tab, ...(await first()));
				assert.equal(await game.getAttribute('data-phase'), 'neutral', `${tab}: a tap before Start starts no game`);

				await app.clock.forward(1000);
				const held = tab === 'switch' ? await startSwitch(page) : await page.getByRole('button', { name: 'Start', exact: true }).click();
				await settled(page, `[data-pane="${tab}-lesson"]`);
				await app.clock.forward(500);
				await page.getByRole('button', { name: 'End now', exact: true }).click();
				await game.locator('.result').waitFor();
				const scored = await result();
				if (tab === 'moment') assert.match(scored, /accuracy\s+—/, 'nothing passed and the tap before Start is not scored');
				else assert.match(scored, /^accuracy\s+\d+%$/, scored);

				await app.clock.forward(1000);
				await tap(tab, ...(await then(held)));
				assert.equal(await game.getAttribute('data-phase'), 'done', `${tab}: a tap after the game starts none`);
				assert.equal(await result(), scored, `${tab}: and is not scored`);
			}
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

test('Reset refuses outside the tutorial database, and ends nothing there', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			// A field of datum's that shares a lesson field's name.
			assert.equal((await app.run(['start', 'tv'])).code, 0);
			const refused = await page.evaluate(() =>
				window.datum.tutorial.startOver(['tv']).then(
					() => null,
					(e) => e.message
				)
			);
			assert.match(refused ?? 'it ran', /\btutorial\b/, 'the refusal names the database it is for');
			const { active } = JSON.parse((await app.run(['now', '--json'])).stdout);
			assert.deepEqual(active.map((a) => a.field), ['tv'], "datum's state is still on");
		},
		PHONE
	);
});

test('Reset ends a state only as it read it: one the reader ended meanwhile is not ended twice (da-v2d29)', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			for (const field of ['tv', 'phone']) assert.equal((await app.run(['start', field])).code, 0);
			// The page's clock is held: the states have lasted a while.
			await app.clock.forward(120_000);
			// The reader turns the TV off between Reset's read and its end.
			await page.evaluate(async () => {
				const previous = window.datum.setRunner(async (argv, ...rest) => {
					if (argv[0] === 'end' && argv[1] === 'tv') {
						const { code, stderr } = await previous(['end', 'tv', '-t', '-1'], ...rest);
						if (code !== 0) throw new Error(`the reader's end: ${stderr}`);
					}
					return previous(argv, ...rest);
				});
				try {
					await window.datum.tutorial.startOver(['tv', 'phone']);
				} finally {
					window.datum.setRunner(previous);
				}
			});
			const { active } = JSON.parse((await app.run(['now', '--json'])).stdout);
			assert.deepEqual(active, [], 'both are off');
			const { stdout } = await app.run(['check', 'integrity']);
			assert.doesNotMatch(stdout, /repeated_state/, 'and the TV is ended once');
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// The switch game starts itself, and its cue stands in Start's place (da-uzyih.4867).
test('at phone size each lesson’s start button is in sight without scrolling, under the nowview and not covered by it', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			for (const tab of ['moment', 'on-off', 'switch']) {
				await page.locator('.root-bar [role="tab"]', { hasText: tabLabel(tab) }).click();
				const name = tab === 'switch' ? 'its cue' : 'Start';
				const button =
					tab === 'switch' ? page.locator('[data-pane="switch-lesson"] .game .cue') : page.getByRole('button', { name, exact: true });
				await button.waitFor();
				// What is drawn at each corner is the button itself: inside the viewport, and covered by no pane.
				const hits = await button.evaluate((el) => {
					const r = el.getBoundingClientRect();
					return [
						[r.left + 2, r.top + 2],
						[r.right - 2, r.top + 2],
						[r.left + 2, r.bottom - 2],
						[r.right - 2, r.bottom - 2]
					].map(([x, y]) => el.contains(document.elementFromPoint(x, y)));
				});
				assert.deepEqual(hits, [true, true, true, true], `«${name}» on ${tab} is wholly in sight`);
				// What is watched while tapping, right above what is tapped (vividn, 2026-10-22): the nowview over the lesson.
				const [now, lesson] = await Promise.all(
					['now', 'lesson'].map((id) => page.locator(`[data-pane="${tab}-${id}"]`).boundingBox())
				);
				assert.ok(now.y + now.height <= lesson.y + 1, `on ${tab} the nowview is above the lesson`);
			}
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

/** Where a picture is DRAWN, by its viewBox on screen: an svg's own box says nothing under `meet`. */
const drawn = (svg) =>
	svg.evaluate((s) => {
		const m = s.getScreenCTM();
		const { x, y, width, height } = s.viewBox.baseVal;
		return { left: m.e + x * m.a, top: m.f + y * m.d, right: m.e + (x + width) * m.a, height: height * m.d };
	});

// da-uzyih.4917: a game's picture was 20 px tall on a phone, the den's 1.9, under a row of controls; Playwright counts
// 1.9 px as visible. vividn, 2026-10-30: Start is smaller and part of the game, on its right, the scene on its left.
test('on a phone each game draws a picture a finger can read, the control beside it in every phase', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			for (const [tab, picture] of [['moment', 'svg.cafe'], ['on-off', '.thing svg'], ['switch', 'svg.house']]) {
				const pane = `[data-pane="${tab}-lesson"]`;
				await page.locator('.root-bar [role="tab"]', { hasText: tabLabel(tab) }).click();
				await page.locator(`${pane} ${picture}`).first().waitFor();
				const pictures = await Promise.all((await page.locator(`${pane} ${picture}`).all()).map(drawn));
				const control = page.locator(`${pane} .game ${tab === 'switch' ? '.cue' : '.go'}`);
				const at = await control.boundingBox();
				for (const p of pictures) {
					assert.ok(p.height >= 50, `${tab}: its picture is drawn ${p.height} px tall`);
					assert.ok(p.right <= at.x, `${tab}: the picture (right ${p.right}) is clear of the control (left ${at.x})`);
				}
				const game = await page.locator(`${pane} .game`).boundingBox();
				assert.ok(at.y >= game.y && at.y + at.height <= game.y + game.height, `${tab}: the control is inside the box`);
				// The other states of one control stand where Start stands: «End now» as the game runs, «Reset» once over. The
				// switch game has no Start, its cue standing there at rest, so they stand where each other does, clear of the house.
				const start = app.clock.at();
				if (tab === 'switch') await startSwitch(page);
				else await page.getByRole('button', { name: 'Start', exact: true }).click();
				await settled(page, pane);
				await app.clock.set(start + 1500);
				const ending = page.getByRole('button', { name: 'End now', exact: true });
				await page.waitForFunction((pane) => parseFloat(document.querySelector(`${pane} .game .go .spent`)?.style.inlineSize) > 0, pane);
				const place = await ending.boundingBox();
				if (tab !== 'switch') assert.deepEqual(place, at, `${tab}: «End now» is where Start was`);
				for (const p of pictures) assert.ok(p.right <= place.x, `${tab}: the picture (right ${p.right}) is clear of «End now» (left ${place.x})`);
				await ending.click();
				const reset = page.getByRole('button', { name: 'Reset', exact: true });
				await reset.waitFor();
				assert.deepEqual(await reset.boundingBox(), place, `${tab}: «Reset» is where «End now» was`);
				const shown = await page.locator(`${pane} .game .result`).boundingBox();
				assert.ok(shown.x + shown.width <= place.x, `${tab}: the result is clear of it`);
				await reset.click();
				await settled(page, pane);
			}
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-qx1it, vividn, 2026-10-31: «on the tutorial move the placemat and everything back to the middle. There is enough
// room, only start pushing it to the left on extremely narrow phones where the start button would start overlapping».
// `iPhone 12` and a 280 px phone; the café fits centered on the first, the house and the den, at #1439's heights, do not.
for (const [screen, options, centered] of [
	['iPhone 12', PHONE, ['moment']],
	['280 px', { ...PHONE, viewport: { width: 280, height: 653 } }, []]
]) {
	test(`on a ${screen} screen each game's picture is in the middle of the stage, or as near it as its control leaves room for`, async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				for (const [tab, picture] of [['moment', 'svg.cafe'], ['on-off', '.thing svg'], ['switch', 'svg.house']]) {
					const pane = `[data-pane="${tab}-lesson"]`;
					await page.locator('.root-bar [role="tab"]', { hasText: tabLabel(tab) }).click();
					await page.locator(`${pane} ${picture}`).first().waitFor();
					const stage = await page.locator(`${pane} .stage`).boundingBox();
					const middle = stage.x + stage.width / 2;
					// All a game's pictures, the den's three, as one.
					const scene = async () => {
						const all = await Promise.all((await page.locator(`${pane} ${picture}`).all()).map(drawn));
						const left = Math.min(...all.map((p) => p.left));
						const right = Math.max(...all.map((p) => p.right));
						return { left, right, center: (left + right) / 2 };
					};
					const rest = await scene();
					const control = await page.locator(`${pane} .game ${tab === 'switch' ? '.cue' : '.go'}`).boundingBox();
					const column = await page.locator(`${pane} .controls`).boundingBox();
					assert.ok(rest.right <= control.x, `${tab}: the picture (right ${rest.right}) is clear of the control (left ${control.x})`);
					assert.ok(rest.right <= column.x, `${tab}: the picture (right ${rest.right}) is clear of the controls' column (left ${column.x})`);
					assert.ok(rest.left >= stage.x, `${tab}: the picture (left ${rest.left}) is inside the stage (left ${stage.x})`);
					// As near the middle as the column lets it: the center it would have against the column, or the stage's.
					const nearest = Math.min(middle, column.x - 4 - (rest.right - rest.left) / 2);
					assert.ok(Math.abs(rest.center - nearest) <= 2, `${tab}: the picture is centered at ${rest.center}, and could be at ${nearest}`);
					if (centered.includes(tab)) assert.ok(Math.abs(rest.center - middle) <= 2, `${tab}: the picture is centered at ${rest.center}, the stage at ${middle}`);
					if (screen === '280 px') assert.ok(rest.center < middle - 2, `${tab}: the picture (center ${rest.center}) has moved left of the middle (${middle})`);

					const start = app.clock.at();
					if (tab === 'switch') await startSwitch(page);
					else await page.getByRole('button', { name: 'Start', exact: true }).click();
					await settled(page, pane);
					await app.clock.set(start + 1500);
					const ending = page.getByRole('button', { name: 'End now', exact: true });
					await ending.waitFor();
					const place = await ending.boundingBox();
					const playing = await scene();
					assert.ok(Math.abs(playing.center - rest.center) <= 1, `${tab}: the picture stays put as the game runs (${rest.center}, then ${playing.center})`);
					assert.ok(playing.right <= place.x, `${tab}: the picture (right ${playing.right}) is clear of «End now» (left ${place.x})`);
					await ending.click();
					const reset = page.getByRole('button', { name: 'Reset', exact: true });
					await reset.waitFor();
					const again = await reset.boundingBox();
					const shown = await page.locator(`${pane} .game .result`).boundingBox();
					assert.ok(shown.x + shown.width <= again.x, `${tab}: the result (right ${shown.x + shown.width}) is clear of «Reset» (left ${again.x})`);
					await reset.click();
					await settled(page, pane);
					await page.locator(`${pane} ${picture}`).first().waitFor();
				}
			},
			{ ...options, query: '?db=tutorial' }
		);
	});
}

// da-uzyih.18, vividn: «[nowview] [game] [fields widget] [explanation] that
// way, as you're tapping and paying attention to the game, you also have the
// nowview close by so that you can see what is happening there without the
// explanation in between». da-uzyih.4908, vividn, 2026-10-29: «1/3 for the
// description, 1/6 for nowview … 1/4 for game and 1/4 for field widget. Let's
// have the game and the fields widgets be separate widgets here». Each game,
// before, during and after a round: its title and what its shape is, then the
// nowview, then its game box, with nothing of the lesson above it, then its
// fields, then what just happened, its terms (da-zb1dr). That the nowview,
// the game and collect are in sight together past the intro is
// tutorial-scroll.test.mjs's.
test('each game reads title and shape, nowview, game, fields widget, discussion, in every phase', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			/** The tab's parts top to bottom, and whether any of the lesson is drawn above its game. */
			const parts = (tab) =>
				page.locator(`[data-pane="${tab}-lesson"] .game-widget`).evaluate((root, tab) => {
					const box = (el) => el && el.getBoundingClientRect();
					const pane = (part) => document.querySelector(`[data-pane="${tab}-${part}"]`);
					const game = root.querySelector('.game');
					const named = {
						title: box(pane('about').querySelector('h1')),
						shape: box(pane('about').querySelector('p')),
						now: box(pane('now')),
						game: box(game),
						act: box(pane('fields').querySelector('.fields')),
						discuss: box(pane('discuss').querySelector('h2'))
					};
					const above = [...root.querySelectorAll('*')]
						.filter((el) => !el.contains(game) && !game.contains(el) && box(el).height > 0 && box(el).top < box(game).top - 0.5)
						.map((el) => el.outerHTML.slice(0, 60));
					/** How far each pane under the about is scrolled. */
					const scrolled = ['lesson', 'fields'].map((part) => {
						let scroller = pane(part).querySelector('.game-widget, .fields');
						while (scroller && getComputedStyle(scroller).overflowY === 'visible') scroller = scroller.parentElement;
						return scroller.scrollTop;
					});
					return { ...named, above, scrolled };
				}, tab);
			const inOrder = (tab, phase, p) => {
				const order = ['title', 'shape', 'now', 'game', 'act', 'discuss'];
				for (const [a, b] of order.slice(1).map((b, i) => [order[i], b]))
					assert.ok(p[a].bottom <= p[b].top + 0.5, `${tab}, ${phase}: ${a} (bottom ${p[a].bottom}) above ${b} (top ${p[b].top})`);
				assert.deepEqual(p.above, [], `${tab}, ${phase}: nothing of the lesson above the game`);
			};
			// Each game's shape, by the name the setup widget gives it, and the term its discussion names.
			const words = { moment: ['moment', 'entry'], 'on-off': ['on/off', 'state'], switch: ['change', 'state'] };

			for (const tab of ['moment', 'switch', 'on-off']) {
				await page.locator('.root-bar [role="tab"]', { hasText: tabLabel(tab) }).click();
				await page.locator(`[data-pane="${tab}-lesson"] .game`).waitFor();
				inOrder(tab, 'before', await parts(tab));
				assert.deepEqual(await page.locator(`[data-pane="${tab}-about"] p b`).allInnerTexts(), words[tab].slice(0, 1));
				assert.deepEqual(await page.locator(`[data-pane="${tab}-discuss"] p b`).allInnerTexts(), words[tab].slice(1));

				const start = app.clock.at();
				if (tab === 'switch') await startSwitch(page);
				else await page.getByRole('button', { name: 'Start', exact: true }).click();
				await settled(page, `[data-pane="${tab}-lesson"]`);
				await page.locator(`[data-pane="${tab}-fields"] .fields button[data-field]`).first().waitFor();
				const during = await parts(tab);
				inOrder(tab, 'during', during);
				assert.deepEqual(during.scrolled, [0, 0], `${tab}: neither the game nor the fields is scrolled`);
				// Past the longest round: the café's, 3 s + 11 gaps of at most 6 s + MATCH_MS + 1 s.
				await app.clock.set(start + 75_000);
				await page.locator(`[data-pane="${tab}-lesson"] .game .result`).waitFor();
				inOrder(tab, 'after', await parts(tab));
			}
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-uzyih.4894, vividn: «be a bit more intentional about the colors and y
// placement and heights of the bars … the bars overlap quite a bit … some
// variation in the size of the bars». On each lesson's tab, no two of the
// lesson's own lanes drawn in the nowview overlap, and they differ in height.
// Two lessons' lanes may share the band: one game is played at a time
// (vividn: «prioritize distinctness within a lesson rather than across lessons»).
const ago = (s) => ['-t', new Date(HELD_AT - s * 1000).toISOString()];
const SEED = [
	['occur', 'coffee', ...ago(100)],
	['occur', 'tea', ...ago(85)],
	['occur', 'croissant', ...ago(70)],
	['occur', 'cake', ...ago(55)],
	['occur', 'bagel', ...ago(40)],
	['start', 'reading', ...ago(110)],
	['start', 'tv', ...ago(90)],
	['start', 'phone', ...ago(70)],
	['switch', 'room', 'kitchen', ...ago(110)],
	['switch', 'room', 'garden', ...ago(50)]
];

test('on each lesson’s tab no two of its fields’ lanes in the nowview overlap, and they differ in height', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await page.locator('.root-bar [role="tab"]', { hasText: 'moment' }).click();
			await page.locator('[data-pane$="-fields"] .fields button[data-field]').first().waitFor();
			for (const argv of SEED) assert.equal((await app.run(argv)).code, 0, argv.join(' '));
			// A quiet run tells no pane: each draws them afresh.
			await page.reload();
			await ready(page);
			const lessons = await page.evaluate(() => window.datum.tutorial.lessons.filter((l) => l.fields.length));
			assert.deepEqual([...new Set(SEED.map((argv) => argv[1]))].sort(), lessons.flatMap((l) => l.fields).sort(), 'every lesson field is seeded');
			for (const { name, fields } of lessons) {
				const tab = page.locator('.root-bar [role="tab"]', { hasText: tabLabel(name) });
				// The open tab's name opens its menu: click only another.
				if ((await tab.getAttribute('aria-selected')) !== 'true') await tab.click();
				const now = `[data-pane="${name}-now"]`;
				for (const field of fields) await page.locator(`${now} g.field[data-field="${field}"] :is(.block, .point-bg)`).first().waitFor({ state: 'attached' });
				// A lane's extent: the top of its highest mark to the bottom of its lowest, blocks and dots alike.
				const lanes = await page.locator(now).evaluate((pane, own) => {
					const out = {};
					for (const field of own) {
						const rects = [...pane.querySelectorAll(`g.field[data-field="${field}"] :is(.block, .point-bg)`)]
							.map((m) => m.getBoundingClientRect())
							.filter((r) => r.width && r.height);
						out[field] = { top: Math.min(...rects.map((r) => r.top)), bottom: Math.max(...rects.map((r) => r.bottom)) };
					}
					return out;
				}, fields);
				for (const [i, a] of fields.entries())
					for (const b of fields.slice(i + 1)) {
						const [p, q] = [lanes[a], lanes[b]];
						// The nearest lanes are 0.015 of the band apart, about 0.9px on a phone: a slack of float error alone.
						assert.ok(p.bottom <= q.top + 0.01 || q.bottom <= p.top + 0.01, `on ${name}: ${a} ${JSON.stringify(p)} and ${b} ${JSON.stringify(q)} overlap`);
					}
				if (fields.length > 1) {
					const heights = fields.map((f) => Math.round(lanes[f].bottom - lanes[f].top));
					assert.ok(new Set(heights).size > 1, `on ${name} its lanes differ in height: ${heights}`);
				}
			}
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

/** The CLI's history in this database, oldest first, as the page keeps it. */
const history = (page) =>
	page.evaluate(() => JSON.parse(localStorage.getItem('datum.repl.v1:tutorial') ?? '{"scrollback":[]}').scrollback.map((c) => c.line));

/** The setup a first open runs, shown: one command (da-uzyih.4937), its tabs each written once. */
async function assertSetup(app, lines) {
	assert.deepEqual(lines, ['setup-tutorial']);
	const layouts = JSON.parse((await app.run(['tail', 'layout', '--json', '-n', '50'])).stdout).rows;
	assert.deepEqual(layouts.map((r) => r.data.name).sort(), [...LESSON_NAMES].sort());
}

// da-uzyih.4893, vividn, 2026-10-28: «on the tutorial, let's not hide the plumbing»; da-uzyih.4937, vividn, 2026-11-01:
// «put them in the output section too. But it should be done as a single command». A reload, its tabs current, sets
// nothing up, and shows nothing.
test('a first open shows its setup as one command, in the CLI and the output; a reload adds nothing', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await press(page, 'Tutorial', 'tutorial');
			const first = await until(() => history(page), (h) => h.length > 0);
			await assertSetup(app, first);
			await until(() => tabs(page), (t) => JSON.stringify(t) === JSON.stringify(LESSON_TABS));
			await app.reload();
			await until(() => tabs(page), (t) => JSON.stringify(t) === JSON.stringify(LESSON_TABS));
			assert.deepEqual(await history(page), first);
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, home: null }
	);
});

// da-uzyih.4937: the one block a fresh tutorial's output section holds is that command's, with nothing of what it
// expands to beside it and no undo: a bare `undo` refuses, and after a tap it takes the tap alone. What it runs is
// there for someone curious, in `alias list` and `kit show`.
test('a fresh tutorial’s output section holds one setup block, off the undo stack, and the first tap lands after it', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await press(page, 'Tutorial', 'tutorial');
			await until(() => history(page), (h) => h.length === 1);
			await page.locator('.root-bar [role="tab"]', { hasText: 'moment' }).click();
			const groups = page.locator('.prompt-bar .section.output .group');
			await groups.first().waitFor();
			assert.equal(await groups.count(), 1, 'one block');
			const setup = groups.first();
			assert.match(await setup.locator('.echo').innerText(), /setup-tutorial$/);
			assert.equal(await setup.locator('.step').count(), 0, 'no undo on it');
			assert.doesNotMatch(await page.locator('.prompt-bar .section.output').innerText(), /kit_id|occur layout/);
			assert.match((await app.run(['alias', 'list'])).stdout, /setup-tutorial → datum kit install - --force --yes/);
			assert.match((await app.run(['kit', 'show', 'tutorial'])).stdout, /moment/);

			const refused = await app.run(['undo']);
			assert.notEqual(refused.code, 0, 'a bare undo takes no setup back');
			assert.match(refused.stderr, /Nothing to undo/);
			assert.notEqual((await app.run(['del'])).code, 0, 'nor is the setup _LAST');
			assert.notEqual((await app.run(['retime', '-t', '9:00'])).code, 0, 'nor is the setup _LAST');
			const layouts = async () => JSON.parse((await app.run(['tail', 'layout', '--json', '-n', '50'])).stdout).rows.length;
			assert.equal(await layouts(), LESSON_TABS.length);

			await page.locator('[data-pane] button', { hasText: 'coffee' }).first().click();
			await until(() => groups.count(), (n) => n === 2);
			const [before, tap] = await page.locator('.prompt-bar .section.output .group .echo').allInnerTexts();
			assert.match(before, /setup-tutorial$/);
			assert.match(tap, /coffee/);
			assert.equal(await groups.nth(1).locator('.step').count(), 1, "the tap's undo");
			assert.equal((await app.run(['undo'])).code, 0, 'a bare undo takes the tap');
			assert.deepEqual(JSON.parse((await app.run(['tail', 'coffee', '--json'])).stdout).rows, []);
			assert.notEqual((await app.run(['undo'])).code, 0, 'past the tap, the setup still stands');
			assert.equal(await layouts(), LESSON_TABS.length);
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, home: null }
	);
});

// da-uzyih.4893, vividn: «when hitting start on a game, show the track field1,field2,field3 command that gets run /
// when the game ends, show the ignore field1,field2,field3 command that gets run». And Reset's, after it
// (da-uzyih.4910, vividn: «these ignores should show up in the cli and the output section (don't hide the plumbing)»):
// each in the CLI's history and in the bar's output section.
test('a game’s start and end each show one line naming its fields, track then ignore, and Reset its end, in the CLI and the output', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const { fields } = await page.evaluate(() => window.datum.tutorial.lessons.find((l) => l.name === 'on-off'));
			await page.locator('.root-bar [role="tab"]', { hasText: 'on-off' }).click();
			const before = (await history(page)).length;
			await page.getByRole('button', { name: 'Start', exact: true }).click();
			await settled(page, '[data-pane="on-off-lesson"]');
			await until(() => history(page), (h) => h.length === before + 1);
			await app.clock.forward(1000);
			await page.getByRole('button', { name: 'End now', exact: true }).click();
			await page.locator('[data-pane="on-off-lesson"] .game[data-phase="done"][aria-busy="false"]').waitFor();
			const lines = await until(() => history(page), (h) => h.length === before + 2);
			assert.deepEqual(lines.slice(before), [`track ${fields.join(',')}`, `ignore ${fields.join(',')}`]);
			assert.deepEqual(await statuses(app, fields), Object.fromEntries(fields.map((f) => [f, 'IGNORED'])));

			// The TV turned on after the game: Reset ends it, shown.
			await app.clock.forward(1000);
			assert.equal((await app.run(['start', 'tv'])).code, 0);
			await app.clock.forward(1000);
			await page.getByRole('button', { name: 'Reset', exact: true }).click();
			await settled(page, '[data-pane="on-off-lesson"]');
			const reset = await until(() => history(page), (h) => h.length === before + 3);
			assert.equal(reset.at(-1), 'off tv -L true');
			const output = await until(
				() => page.locator('.prompt-bar .section.output .echo').allInnerTexts(),
				(o) => o.length > 0 && o.at(-1).includes('off tv')
			);
			for (const line of reset.slice(before)) assert.ok(output.some((o) => o.endsWith(line)), `${line} in the output: ${JSON.stringify(output)}`);
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, query: '?db=tutorial' }
	);
});

// da-uzyih.4893, vividn, 2026-10-28: «add a button at the bottom of the introduction page "reset the tutorial" it calls a
// db purge, clears the CLI and does the setup to get the tutorial in its initial state».
test('«Reset the tutorial» purges it, clears the CLI, and sets it up as a first open, shown; outside the tutorial it is refused', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await press(page, 'Tutorial', 'tutorial');
			await until(() => history(page), (h) => h.length === 1);
			assert.equal((await app.run(['occur', 'practice'])).code, 0);

			await page.getByRole('button', { name: 'Reset the tutorial', exact: true }).click();
			const lines = await until(() => history(page), (h) => h[0] === 'db purge --yes' && h.length === 2);
			await assertSetup(app, lines.slice(1));
			await page.getByRole('button', { name: 'Reset the tutorial', exact: true }).and(page.locator(':enabled')).waitFor();
			assert.deepEqual(JSON.parse((await app.run(['tail', 'practice', '--json'])).stdout).rows, [], 'the practice is gone');
			assert.match((await app.run(['fields', '--json'])).stdout, /"field": "tv"/, "the kit's fields are back");
			const layouts = JSON.parse((await app.run(['tail', 'layout', '--json', '-n', '50'])).stdout).rows;
			assert.deepEqual(layouts.map((r) => r.data.name).sort(), [...LESSON_NAMES].sort(), 'each tab installed once');
			assert.deepEqual(await tabs(page), LESSON_TABS);
			assert.equal(await front(page), 'intro');

			// Anywhere else, the reset is refused and purges nothing.
			await viaSwitcher(page, 'datum', null);
			assert.equal((await app.run(['occur', 'kept'])).code, 0);
			const refused = await page.evaluate(() => window.datum.tutorial.reset().then(() => null, (e) => e.message));
			assert.match(refused ?? '', /tutorial/);
			assert.equal(JSON.parse((await app.run(['tail', 'kept', '--json'])).stdout).rows.length, 1, 'nothing purged');
			assert.equal(await page.getByRole('button', { name: 'Reset the tutorial' }).count(), 0);
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, home: null }
	);
});

// da-uzyih.4913: a reset that fails says why under its button, and the tutorial is left as it was. The refusal
// made before any command runs shows nowhere else: here the query names another database under a mounted Introduction.
test('a «Reset the tutorial» that is refused says so under the button and purges nothing', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await press(page, 'Tutorial', 'tutorial');
			await until(() => history(page), (h) => h.length === 1);
			assert.equal((await app.run(['occur', 'practice'])).code, 0);

			await page.evaluate(() => history.replaceState(history.state, '', '?db=datum'));
			const button = page.getByRole('button', { name: 'Reset the tutorial', exact: true });
			await button.click();
			const alert = page.locator('.tutorial [role="alert"]');
			await alert.waitFor();
			assert.match(await alert.innerText(), /not reset: Only the tutorial database is reset/);
			await button.and(page.locator(':enabled')).waitFor();
			assert.equal((await history(page)).length, 1, 'nothing ran');

			await page.evaluate(() => history.replaceState(history.state, '', '?db=tutorial'));
			assert.equal(JSON.parse((await app.run(['tail', 'practice', '--json'])).stdout).rows.length, 1, 'nothing purged');
		},
		{ ...PHONE, home: null }
	);
});
