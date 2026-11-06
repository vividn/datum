// The tutorial's setup and its lines: a first open shown as one command, a
// game's start and end in the CLI and the output, and «Reset the tutorial».
// One of 4 files split from tutorial.test.mjs by topic, so each stays well
// inside test.mjs's 300 s file cap (da-5rpcr); the helpers they share are
// support/tutorial.mjs.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { ready, startApp, stopApp, withApp } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';
import { settled, statuses, LESSON_NAMES, tabs, front, press, viaSwitcher } from './support/tutorial.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

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
			await until(() => tabs(page), (t) => JSON.stringify(t) === JSON.stringify(LESSON_NAMES));
			await app.reload();
			await until(() => tabs(page), (t) => JSON.stringify(t) === JSON.stringify(LESSON_NAMES));
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
			assert.equal(await layouts(), LESSON_NAMES.length);

			await page.locator('[data-pane] button', { hasText: 'coffee' }).first().click();
			await until(() => groups.count(), (n) => n === 2);
			const [before, tap] = await page.locator('.prompt-bar .section.output .group .echo').allInnerTexts();
			assert.match(before, /setup-tutorial$/);
			assert.match(tap, /coffee/);
			assert.equal(await groups.nth(1).locator('.step').count(), 1, "the tap's undo");
			assert.equal((await app.run(['undo'])).code, 0, 'a bare undo takes the tap');
			assert.deepEqual(JSON.parse((await app.run(['tail', 'coffee', '--json'])).stdout).rows, []);
			assert.notEqual((await app.run(['undo'])).code, 0, 'past the tap, the setup still stands');
			assert.equal(await layouts(), LESSON_NAMES.length);
			assert.deepEqual(app.errors, []);
		},
		{ ...PHONE, home: null }
	);
});

// da-e3rdf: the change lesson's layout is `change`, the glossary's word, so the setup a newcomer first reads names it
// so. A tutorial set up while it was `switch` holds that row still, as the kit install leaves a layout the kit no
// longer lists; its open `switch` tab is the change tab, so it ends with one change tab and no switch.
test('the setup names the change lesson `change`, and a tutorial set up when it was `switch` has one change tab', async () => {
	await withApp(
		async (app) => {
			const { page } = app;
			await press(page, 'Tutorial', 'tutorial');
			await until(() => history(page), (h) => h.length === 1);
			const output = page.locator('.prompt-bar .section.output');
			const setup = () => output.innerText();
			await until(setup, (t) => /\+ change\b/.test(t));
			assert.doesNotMatch(await setup(), /\bswitch\b/);

			// As it was: the lesson's row is `switch`, its tab open under that name, and no `change` row.
			const layouts = async () => JSON.parse((await app.run(['tail', 'layout', '--json', '-n', '50'])).stdout).rows;
			const { root } = (await layouts()).find((r) => r.data.name === 'change').data;
			const kit = readFileSync(new URL('../../kits/tutorial.toml', import.meta.url), 'utf8');
			const old = `${kit}\n[[layout]]\nname = "switch"\nroot = ${JSON.stringify(JSON.stringify(root))}\n`;
			assert.equal((await app.run(['kit', 'install', '-', '--force', '--yes'], old)).code, 0);
			assert.equal((await app.run(['delete', 'layout:change'])).code, 0);
			const names = LESSON_NAMES.map((n) => (n === 'change' ? 'switch' : n));
			await page.evaluate((names) => localStorage.setItem('datum.layout.root.v1.tutorial', JSON.stringify({ names, active: 3 })), names);

			await page.reload();
			await ready(page);
			await until(() => layouts(), (rows) => rows.some((r) => r.data.name === 'change'));
			await until(() => tabs(page), (t) => JSON.stringify(t) === JSON.stringify(LESSON_NAMES));
			assert.equal(await front(page), 'change', 'the tab it was on, in front');
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
			assert.deepEqual(await tabs(page), LESSON_NAMES);
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
