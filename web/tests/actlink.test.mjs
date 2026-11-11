// datum's action links (da-uzl16), pressed in the app. vividn, 2026-11-11:
// «can we allow buttons with a limited action vocabulary (like changing tabs,
// logging data, etc?) inside text widgets … embedding other data controls,
// like a standard collect widget field button», and «maybe using links … then
// the text widget detects this and renders it the correct way».
//
// So in a text widget each action is a button that does its one thing, a run
// outside the vocabulary its words and why, and `act:field/<name>` alone on a
// line the collect button itself: a tap logs, a hold opens. Opened from
// outside, `/act/…` asks before it runs. actions-pure.test.mjs holds the parse.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, leaveWidget, modifyWidget, rootMenuButton, titledPane } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** `/text` alone, holding `source` drawn. */
async function written(app, source) {
	await app.goto('/text');
	const pane = titledPane(app.page, 'text');
	await pane.waitFor();
	const id = await pane.getAttribute('data-pane');
	await modifyWidget(app.page, id);
	await pane.locator('textarea').fill(source);
	await leaveWidget(app.page, id);
	await pane.locator('.text').waitFor();
	return pane;
}

/** How many entries `field` has: none while it is no field. */
const count = async (app, field) => {
	const { code, stdout } = await app.run(['tail', field, '--json']);
	return code === 0 ? JSON.parse(stdout).rows.length : 0;
};

describe('an action in a text widget', () => {
	test('each is a button that does its one thing; a run outside the vocabulary is its words and why, and runs nothing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const pane = await written(app, '[Coffee](act:run?line=moment+coffee+-t+-1h+milk=oat) [Sync](act:place/sync) [Tail](act:tab/tail) [Gone](act:run?line=delete+coffee)');
			const button = (name) => pane.locator('.text button.command', { hasText: name });
			// Each says what it does in its title: the line, the place, the tab.
			assert.deepEqual(
				await pane.locator('.text button.command').evaluateAll((els) => els.map((el) => [el.textContent, el.dataset.act, el.title.includes(['moment coffee', 'sync', 'tail'][els.indexOf(el)])])),
				[
					['Coffee', 'run', true],
					['Sync', 'open', true],
					['Tail', 'show', true]
				]
			);
			const refused = await pane.locator('.text .refused').textContent();
			assert.ok(refused.startsWith('Gone') && refused.includes('delete'), refused);

			await button('Coffee').click();
			await until(() => count(app, 'coffee'), (n) => n === 1);
			const [row] = JSON.parse((await app.run(['tail', 'coffee', '--json'])).stdout).rows;
			assert.ok(JSON.stringify(row).includes('"oat"'), 'the run lost its data');

			await button('Sync').click();
			const sync = page.locator('dialog.sheet[open][data-place="sync"]');
			await sync.waitFor();
			await page.keyboard.press('Escape');
			await sync.waitFor({ state: 'detached' });

			await button('Tail').click();
			await until(() => rootMenuButton(page).textContent(), (t) => t?.includes('tail'));
			assert.equal(await count(app, 'coffee'), 1, 'the refused run, or another, wrote');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});

	test("act:field/<name> alone on a line is that field's collect button: a tap logs it, a hold opens it in collect", async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.run(['setup', 'coffee', '--kind', 'moment', '--color', '#ffa500']);
			const pane = await written(app, 'Had one?\n\n[coffee](act:field/coffee)');
			const chip = pane.locator('.text [data-embed="field"] button[data-field="coffee"]');
			await chip.waitFor();
			assert.equal(await chip.getAttribute('data-act'), 'run', 'a moment logs on a tap, a double outline');
			assert.match(await chip.getAttribute('style'), /--line: #ffa500/);

			await chip.click();
			await until(() => count(app, 'coffee'), (n) => n === 1);

			await chip.click({ button: 'right' });
			await until(() => page.locator('.prompt-bar .pill.field').textContent(), (t) => t?.includes('coffee'));
			await page.locator('dialog.sheet[open][data-place="fields"]').waitFor();
			assert.equal(await count(app, 'coffee'), 1, 'the hold logged');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});

describe('an action opened from outside', () => {
	test('says what it would do and runs only on a tap; a refused one is a notice', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await app.goto('/act/run?line=moment+coffee');
			const dialog = page.locator('dialog.confirm[open]');
			await dialog.waitFor();
			assert.ok((await dialog.textContent()).includes('moment coffee'), 'the dialog does not say what it runs');
			assert.equal(await count(app, 'coffee'), 0, 'it ran before the tap');
			await dialog.getByRole('button', { name: 'run', exact: true }).click();
			await until(() => count(app, 'coffee'), (n) => n === 1);

			await app.goto('/act/run?line=moment+coffee');
			await dialog.waitFor();
			await dialog.getByRole('button', { name: 'cancel', exact: true }).click();
			await dialog.waitFor({ state: 'detached' });

			await app.goto('/act/run?line=delete+coffee');
			await page.locator('p.notice', { hasText: 'delete' }).waitFor();
			assert.equal(await dialog.count(), 0, 'a refused run asked to run');

			// vremya on #2014: a logging line whose flags write more than its entry is refused, naming the flag.
			for (const [line, flag] of [
				['moment+coffee+--alias+w+--alias-overwrite', '--alias'],
				['moment+coffee+--id+fixedslug+--conflict+replace', '--id']
			]) {
				await app.goto(`/act/run?line=${line}`);
				await page.locator('p.notice', { hasText: flag }).waitFor();
				assert.equal(await dialog.count(), 0, `${flag} asked to run`);
			}
			const aliases = JSON.parse((await app.run(['alias', 'list', '--json'])).stdout);
			assert.deepEqual(aliases.filter((a) => a.name === 'w'), [], 'an alias was saved');
			assert.equal(await count(app, 'coffee'), 1, 'a refused run wrote');
			assert.deepEqual(app.errors, []);
		}, DESKTOP);
	});
});
