// The tutorial's frame (da-6cvrd): a database of its own, «tutorial», that
// the welcome's «Tutorial» button switches to.
//
// The bead's own «done when», on a phone and a fresh datum.day: the tap
// recolors the app, the Introduction shows the way back, going back lands
// on «datum» untouched — no tutorial entry in it, and its own tabs — and a
// second visit to «tutorial» keeps what was there. `sync` there is refused.
//
// One of 4 files split from tutorial.test.mjs by topic, so each stays well
// inside test.mjs's 300 s file cap (da-5rpcr); the helpers they share are
// support/tutorial.mjs.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { ready, startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';
import { until } from './support/until.mjs';
import { LESSON_NAMES, tabs, front, press, viaSwitcher, lessonTab, tabLabel } from './support/tutorial.mjs';

before(startApp);
after(stopApp);

const hue = (page) => page.evaluate(() => document.documentElement.style.getPropertyValue('--hue'));

const db = (page) => page.evaluate(() => new URLSearchParams(location.search).get('db'));

test('the Tutorial button opens the tutorial database, and the way back leaves datum untouched', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			const home = await tabs(page);
			const datumHue = await hue(page);
			assert.deepEqual(home, ['welcome', 'setup', 'collection', 'dayview'], 'a fresh datum.day opens on the welcome');

			await press(page, 'Tutorial', 'tutorial');
			assert.equal(await db(page), 'tutorial');
			assert.notEqual(await hue(page), datumHue, 'the app is recolored');
			assert.deepEqual(await tabs(page), LESSON_NAMES, 'its lessons, in order');
			// A game's tab says its title (da-83y6w, vividn, 2026-11-09: «let's title the first tab "Moments in a Café"»,
			// «let's retitle the other's too: "turn the TV off" "change of scenery"»).
			assert.deepEqual(
				await page.locator('.root-bar [role="tab"]').allInnerTexts(),
				['intro', 'Moments in a Café', 'Turn the TV off', 'Change of scenery', 'input-bar', 'tabs']
			);
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
			assert.deepEqual(await tabs(page), LESSON_NAMES, "the tutorial's, not datum's");
			const kept = JSON.parse((await app.run(['tail', 'practice', '--json'])).stdout).rows;
			assert.equal(kept.length, 1, 'the practice entry is kept');
			const layouts = JSON.parse((await app.run(['tail', 'layout', '--json', '-n', '50'])).stdout).rows;
			assert.deepEqual(
				layouts.filter((r) => r.data?.kit_id === 'tutorial').map((r) => r.data.name).sort(),
				[...LESSON_NAMES].sort(),
				'each tab installed once'
			);
			await lessonTab(page, 'moment').click();
			await app.reload();
			assert.deepEqual(await tabs(page), LESSON_NAMES, "a reload keeps the tutorial's");
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
			await lessonTab(page, 'on-off').click();
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
			await intro.getByRole('button', { name: 'Next: Moments in a Café', exact: true }).click();
			await until(() => front(page), (f) => f === 'moment');
			await page.locator('[data-pane="moment-lesson"] .game').waitFor();
			assert.equal(await db(page), 'tutorial', 'the same database, another tab');
			assert.deepEqual(await tabs(page), LESSON_NAMES);
		},
		{ ...PHONE, home: null }
	);
});

// da-uzyih.4892, vividn, 2026-10-28: «add advancement buttons at the bottom of every page on the tutorial like there
// is on the introduction. "Next: on/off"». Each names the next lesson as its tab does (da-83y6w).
test('every lesson ends on a button to the next, from the Introduction to the last, which has none', async () => {
	await withApp(
		async ({ page }) => {
			await press(page, 'Tutorial', 'tutorial');
			const labels = ['Moments in a Café', 'Turn the TV off', 'Change of scenery', 'The input bar', 'Tabs and widgets'];
			for (const [i, label] of labels.entries()) {
				assert.equal(await front(page), LESSON_NAMES[i]);
				const next = page.getByRole('button', { name: `Next: ${label}`, exact: true });
				// The Introduction's Next has its reset under it (da-uzyih.4893), and nothing after that.
				const atEnd = await next.evaluate(
					(b, intro) =>
						b.parentElement.classList.contains('tutorial') &&
						(intro ? b.nextElementSibling?.textContent === 'Reset the tutorial' && !b.nextElementSibling.nextElementSibling : !b.nextElementSibling),
					i === 0
				);
				assert.ok(atEnd, `${LESSON_NAMES[i]} ends on the next lesson, ${label}`);
				await next.click();
				await page.waitForFunction(
					(name) => document.querySelector('.root-bar [role="tab"][aria-selected="true"]')?.textContent.trim() === name,
					tabLabel(LESSON_NAMES[i + 1])
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
			await intro.getByRole('button', { name: 'Next: Moments in a Café', exact: true }).waitFor();
			const parts = await intro.evaluate((root) => [...root.children].map((el) => (el.tagName === 'BUTTON' ? el.textContent.trim() : el.tagName)));
			assert.deepEqual(parts, ['H1', 'P', 'P', 'P', 'Next: Moments in a Café', 'Reset the tutorial']);
			for (const tab of LESSON_NAMES) {
				await lessonTab(page, tab).click();
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
				await until(() => tabs(page), (t) => JSON.stringify(t) === JSON.stringify(LESSON_NAMES));
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
				await lessonTab(page, tab).click();
				const pane = page.locator('.host[data-widget="tutorial"]:visible');
				await pane.locator('h1', { hasText: title }).waitFor();
				assert.ok(await pane.locator('ul li').count(), `${tab} lists what it teaches`);
				assert.equal(await pane.getByRole('button', { name: 'Start this tab over' }).count(), 0);
			}
		},
		{ ...PHONE, home: null }
	);
});
