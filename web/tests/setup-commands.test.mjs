// The setup widget's actions are commands in the bar's palette (da-r674m), vividn, 2026-11-11: «the command
// pallete is how you should save stuff from the setup screen! save should be the primary action», «yes the normal
// buttons at the bottom of the setup widget all go», and «even when the output section is closed, if the command
// pallete is open, the setup widget should adjust it's height so that you can still save stuff». Its undo is the
// form's own: «an undo that undoes the last thing you changed in the setup widget (doesn't trigger a cli undo …
// unless the last action was itself to save)». On a phone, where the setup place is a tab and the palette the bar's.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { PHONE, fold, unfold } from './support/bar.mjs';
import { FIELD_FORM, PANEL, SETUP, nameField, openField, openPalette, setupCommand, setupCommands } from './support/field.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const described = async (app, field) => JSON.parse((await app.run(['setup', field, '--json'])).stdout).description ?? null;
const description = (page) => page.locator(`${FIELD_FORM} input.description`);
const undo = (page) => page.locator('#bar-commands [data-setup="undo"]');
/** How many versions the spec has, and how many commands the output section has said: what running one moves. */
const ran = async (app, field) => [
	JSON.parse((await app.run(['history', `spec:${field}`, '--json'])).stdout).length,
	await app.page.locator('.prompt-bar .group').count()
];

describe('setup in the palette', () => {
	const phone = sharedApp(PHONE);

	test('a spec: save leads, then revert, redo and cancel; the form has no row of buttons; save writes what it shows', async () => {
		await phone.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await openField(page, 'tea');
			await description(page).waitFor();
			assert.equal(await page.locator(`${FIELD_FORM} :is(.buttons, button.commit, button.untracked)`).count(), 0, 'a button row');
			assert.deepEqual(await setupCommands(page), [['save', false], ['revert', false], ['redo', false], ['cancel', false]]);
			assert.equal(await page.locator(`${SETUP()}`).getAttribute('data-setup'), 'save', 'the primary is not save');

			await description(page).fill('green');
			await until(() => setupCommands(page), (c) => c[0].join() === 'save,true' && c[1].join() === 'revert,true');
			await setupCommand(page, 'save');
			await until(() => described(app, 'tea'), (d) => d === 'green');
			assert.deepEqual(app.errors, []);
		});
	});

	test('a new field: start tracking leads, then save without tracking and cancel', async () => {
		await phone.solo(async (app) => {
			const { page } = app;
			await openField(page);
			await nameField(page, 'tea');
			assert.deepEqual(await setupCommands(page), [['start tracking', false], ['save without tracking', false], ['cancel', true]]);
			await page.locator(`${FIELD_FORM} button[data-verb="occur"]`).click();
			await setupCommand(page);
			await until(async () => JSON.parse((await app.run(['fields', '--json'])).stdout).find((f) => f.field === 'tea'), (f) => f && !f.ignored);
			assert.deepEqual(app.errors, []);
		});
	});

	test('undo takes back the form\'s edits one a press, running nothing; after a save it undoes the save; else it is grayed', async () => {
		await phone.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await openField(page, 'tea');
			await description(page).waitFor();
			await openPalette(page);
			assert.equal(await undo(page).isDisabled(), true, 'nothing to undo, and undo can be pressed');

			// Three edits: a box's words, a kind, a checkbox.
			await description(page).fill('green');
			await page.locator(`${FIELD_FORM} button[data-verb="start"]`).click();
			await page.locator(`${FIELD_FORM} [data-tab="aliases"]`).click();
			await page.locator(`${PANEL} input.autoalias`).click();
			assert.equal(await page.locator(`${PANEL} input.autoalias`).isChecked(), false);
			const before = await ran(app, 'tea');
			const basics = () => page.locator(`${FIELD_FORM} [data-tab="basics"]`).click();

			await undo(page).click();
			await until(() => page.locator(`${PANEL} input.autoalias`).isChecked(), (c) => c);
			assert.equal(await page.locator(`${FIELD_FORM} button[data-verb="start"]`).getAttribute('aria-checked'), 'true', 'two taken back');
			await undo(page).click();
			await until(() => page.locator(`${FIELD_FORM} button[data-verb="occur"]`).getAttribute('aria-checked'), (c) => c === 'true');
			await basics();
			assert.equal(await description(page).inputValue(), 'green', 'the words, an edit of their own, went with the kind');
			await undo(page).click();
			await until(() => description(page).inputValue(), (v) => v === '');
			// The form as it opened, the palette still open, and nothing run.
			assert.equal(await undo(page).isDisabled(), true, 'undo still offered at the form as it opened');
			await app.idle();
			assert.deepEqual(await ran(app, 'tea'), before, 'a local undo ran a command');
			assert.equal(await described(app, 'tea'), null);

			// After a save, undo is the CLI's of the save's spec entry.
			await description(page).fill('black');
			await setupCommand(page, 'save');
			await until(() => described(app, 'tea'), (d) => d === 'black');
			await setupCommand(page, 'undo');
			await until(() => described(app, 'tea'), (d) => d === null);
			await until(() => description(page).inputValue(), (v) => v === '');
			// And then there is nothing more for it to take back.
			await openPalette(page);
			assert.equal(await undo(page).isDisabled(), true);
			assert.deepEqual(app.errors, []);
		});
	});

	for (const output of ['closed', 'open'])
		test(`with the output section ${output}, the palette covers none of the setup widget`, async () => {
			await phone.solo(async (app) => {
				const { page } = app;
				await app.run(['track', 'tea', '--verb', 'occur']);
				await openField(page, 'tea');
				await description(page).waitFor();
				await (output === 'open' ? unfold : fold)(page);
				const palette = await openPalette(page);
				await until(
					async () => {
						const [pane, over] = await Promise.all([
							page.locator('section.pane[data-title="setup"]:not(.beside *)').boundingBox(),
							palette.boundingBox()
						]);
						return pane && over && pane.y + pane.height - over.y;
					},
					(under) => typeof under === 'number' && under <= 0.5
				);
				assert.equal(await page.locator('.prompt-bar .section.output').count(), output === 'open' ? 1 : 0, 'the output section moved');
				assert.deepEqual(app.errors, []);
			});
		});
});
