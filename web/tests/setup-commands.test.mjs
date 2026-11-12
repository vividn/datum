// The setup widget's actions are commands in the bar's palette (da-r674m), vividn, 2026-11-11: «the command
// pallete is how you should save stuff from the setup screen! save should be the primary action», «yes the normal
// buttons at the bottom of the setup widget all go», and «even when the output section is closed, if the command
// pallete is open, the setup widget should adjust it's height so that you can still save stuff». Its undo is the
// form's own: «an undo that undoes the last thing you changed in the setup widget (doesn't trigger a cli undo …
// unless the last action was itself to save)». On a phone, where the setup place is a tab and the palette the bar's;
// and on a desktop, where it is a modal the bar stays live beside (da-njt0f), vividn, 2026-11-12: «Bar live over setup».

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { DESKTOP, PHONE, fold, unfold } from './support/bar.mjs';
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

describe('setup on a desktop: the bar stays live beside its modal', () => {
	const desktop = sharedApp(DESKTOP);
	const sheet = (page, place) => page.locator(`dialog.sheet[open][data-place="${place}"]`);
	const input = (page) => page.locator('.prompt-bar input');
	/** Whether the bar's box takes the focus: an inert one does not. */
	const live = (page) =>
		input(page).evaluate((el) => {
			el.focus();
			return document.activeElement === el;
		});

	test('the bar takes typing, its palette offers save and undo, and the modal has no row of its own', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await openField(page, 'tea');
			await description(page).waitFor();
			assert.equal(await sheet(page, 'setup').evaluate((d) => d.matches(':modal')), false, 'the setup sheet is modal');
			assert.equal(await sheet(page, 'setup').locator('[data-setup]').count(), 0, 'a command row under the widget');
			// The palette opened with the sheet, as the row showed them, no ❯ pressed.
			await page.locator(SETUP()).waitFor();

			await input(page).click();
			await page.keyboard.type('tail tea');
			assert.equal(await input(page).inputValue(), 'tail tea');
			await input(page).fill('');

			await openPalette(page);
			assert.equal(await undo(page).count(), 1, 'no undo in the palette');
			assert.equal(await page.locator(SETUP()).getAttribute('data-setup'), 'save');
			// Pressed by a click, which the sheet would take were it over the palette.
			await description(page).fill('green');
			await page.locator(SETUP('save')).click();
			await until(() => described(app, 'tea'), (d) => d === 'green');
			assert.equal(await sheet(page, 'setup').isVisible(), true, 'save left the place');

			// Escape in the sheet closes it, as a modal's does.
			await description(page).press('Escape');
			await sheet(page, 'setup').waitFor({ state: 'detached' });
			// And the palette it opened goes with it.
			await page.locator('.prompt-bar button[aria-controls="bar-commands"][aria-expanded="false"]').waitFor();
			assert.deepEqual(app.errors, []);
		});
	});

	// vremya on #2041: a sheet shown, not shown modal, leaves the page behind its veil focusable; so all but the bar is inert.
	test('with setup open, Tab reaches only the sheet and the bar, and nothing behind the veil takes the focus', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			await openField(page, 'tea');
			await description(page).waitFor();
			const behind = page.locator('main button, .root-bar button').first();
			await behind.waitFor({ state: 'attached' });
			assert.equal(await behind.evaluate((el) => (el.focus(), document.activeElement === el)), false, 'a button behind the veil took the focus');

			await description(page).focus();
			const where = new Set();
			for (let i = 0; i < 150; i++) {
				await page.keyboard.press('Tab');
				where.add(
					await page.evaluate(() => {
						const el = document.activeElement;
						if (!el || el === document.body) return 'nowhere';
						if (el.closest('dialog.sheet[data-place="setup"]')) return 'sheet';
						if (el.closest('.prompt-bar')) return 'bar';
						return el.outerHTML.slice(0, 80);
					})
				);
			}
			assert.ok(where.has('sheet') && where.has('bar'), `Tab never reached both: ${[...where]}`);
			assert.deepEqual([...where].filter((w) => !['sheet', 'bar', 'nowhere'].includes(w)), [], 'Tab reached past the veil');
			// Closed by its ×, the palette it opened goes with it.
			await sheet(page, 'setup').locator('.sheet-head button.close').click();
			await sheet(page, 'setup').waitFor({ state: 'detached' });
			await page.locator('.prompt-bar button[aria-controls="bar-commands"][aria-expanded="false"]').waitFor();
			assert.deepEqual(app.errors, []);
		});
	});

	test('another place\'s modal still makes the bar inert', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			assert.equal(await live(page), true, 'the bar is inert with no modal');
			await page.evaluate(() => window.datum.shell.openPlace('docs'));
			await sheet(page, 'docs').waitFor();
			assert.equal(await sheet(page, 'docs').evaluate((d) => d.matches(':modal')), true);
			assert.equal(await live(page), false, 'the bar took the focus under the docs modal');
			await page.keyboard.press('Escape');
			await sheet(page, 'docs').waitFor({ state: 'detached' });
			assert.deepEqual(app.errors, []);
		});
	});
});
