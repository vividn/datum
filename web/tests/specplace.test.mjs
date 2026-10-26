// A field's spec is edited in a PLACE (da-hbrfg): «a modal asks; a place is
// a tab». The place is the field widget's (da-fu29p), ONE tab whatever the
// field — opened beside the tab you were on, put on another field where it
// stands, closed when left. The tab tests run on a phone, where a place is a
// tab whatever a desktop comes to open it as (da-jozgv).

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { DESKTOP, PHONE } from './support/layout.mjs';
import { FIELD_FORM, leaveField, openField, pickImage } from './support/field.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const rootTabs = (page) =>
	page.locator('.root-bar [role="tab"]').evaluateAll((all) => all.map((b) => b.textContent.replace('•', '').trim()));

const shown = (page) =>
	page.locator('.root-bar [role="tab"][aria-selected="true"]').evaluate((b) => b.textContent.replace('•', '').trim());

const described = async (app, field) => JSON.parse((await app.run(['setup', field, '--json'])).stdout).description;

describe('the spec place, on a phone', () => {
	const phone = sharedApp(PHONE);

	test('opens beside the tab you were on, one tab for every field, and back closes it', async () => {
		await phone.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'mood', '--verb', 'switch']);
			await app.run(['track', 'tea', '--verb', 'occur']);
			const was = await rootTabs(page);
			const from = was.indexOf(await shown(page));

			await openField(page, 'mood');
			await until(() => shown(page), (t) => t === 'setup');
			const beside = [...was.slice(0, from + 1), 'setup', ...was.slice(from + 1)];
			assert.deepEqual(await rootTabs(page), beside);
			assert.equal((await page.locator(`${FIELD_FORM} h2`).innerText()).trim(), 'mood');

			// The same field again is the same tab, and whoever opened it first
			// is answered — nothing is left waiting (canary on #360).
			const first = page.evaluate(() => window.datum.field('mood'));
			await page.locator(FIELD_FORM).waitFor();
			await page.evaluate(() => void window.datum.field('mood'));
			assert.equal(await first, null);
			await page.locator(FIELD_FORM).waitFor();
			assert.deepEqual(await rootTabs(page), beside, 'a second open made a second tab');

			// And another field is that tab too, put on it where it stands.
			await page.evaluate(() => void window.datum.field('tea'));
			await until(async () => (await page.locator(`${FIELD_FORM} h2`).allInnerTexts()).join().trim(), (t) => t === 'tea');
			assert.deepEqual(await rootTabs(page), beside, 'a second field made a second tab');

			await page.goBack();
			await until(() => rootTabs(page), (t) => t.join() === was.join());
			assert.equal(await page.locator(FIELD_FORM).count(), 0);
		});
	});

	// da-hda4j: Save is a save point, and leaving saves what is not saved yet.
	test('Save saves in one `setup F` and stays; leaving returns and saves the rest', async () => {
		await phone.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			const was = await rootTabs(page);

			const { saved } = await openField(page, 'tea');
			await page.locator(`${FIELD_FORM} input.description`).fill('green');
			await page.locator(`${FIELD_FORM} button.commit`).click();
			const result = await saved;
			assert.equal(result.code, 0, result.stderr);
			assert.match(result.argv.join(' '), /^setup tea --from-json /);
			const sent = JSON.parse(result.argv[result.argv.indexOf('--from-json') + 1]);
			assert.equal(sent.description, 'green', 'what Save sent');
			assert.equal(await described(app, 'tea'), 'green');
			assert.equal(await shown(page), 'setup', 'Save left the place');
			// The form is read again from the save: a save point, with nothing to save.
			await until(() => page.locator(`${FIELD_FORM} button.commit`).isDisabled(), (d) => d);

			await page.locator(`${FIELD_FORM} input.description`).fill('black');
			await leaveField(page);
			await until(() => rootTabs(page), (t) => t.join() === was.join());
			await until(() => described(app, 'tea'), (d) => d === 'black');
		});
	});
});

// A refusal is the CLI's to make (da-45wgz: an icon over 4 kB), and the
// place keeps what was typed so it can be put right rather than lost.
describe('a refused save', () => {
	const phone = sharedApp(PHONE);

	test('answers with the refusal and keeps the place open with the edits', async () => {
		await phone.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			const was = await rootTabs(page);

			const { saved } = await openField(page, 'tea');
			await page.locator(`${FIELD_FORM} input.description`).fill('green');
			const big = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><path d="${'M0 0'.repeat(1200)}"/></svg>`;
			await pickImage(page, 'big.svg', big);
			await page.locator(`${FIELD_FORM} button.commit`).click();
			const result = await saved;
			assert.notEqual(result.code, 0);
			assert.match(result.stderr, /at most 4 kB/);

			assert.equal(await shown(page), 'setup', 'the refusal left the place');
			assert.equal(await page.locator(`${FIELD_FORM} input.description`).inputValue(), 'green', 'the edit was lost');
			assert.equal(await described(app, 'tea'), null, 'the refusal wrote');

			await leaveField(page);
			await until(() => rootTabs(page), (t) => t.join() === was.join());
			assert.equal(await described(app, 'tea'), null, 'a refused save was written on leaving');
		});
	});
});

// On a desktop a place is a modal (da-jozgv): the same widget, the same form.
describe('the spec place, on a desktop', () => {
	const desktop = sharedApp(DESKTOP);

	test('opens in the place modal, and Save saves and answers; closing it leaves', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			const was = await rootTabs(page);

			const { saved } = await openField(page, 'tea');
			const modal = page.locator('dialog.sheet[data-place="setup"]');
			await modal.locator(`${FIELD_FORM} h2`).waitFor();
			assert.equal((await modal.locator(`${FIELD_FORM} h2`).innerText()).trim(), 'tea');
			assert.deepEqual(await rootTabs(page), was, 'a desktop opened a tab');

			await modal.locator(`${FIELD_FORM} input.description`).fill('green');
			await modal.locator(`${FIELD_FORM} button.commit`).click();
			const result = await saved;
			assert.equal(result.code, 0, result.stderr);
			assert.equal(await described(app, 'tea'), 'green');
			await leaveField(page);
			await modal.waitFor({ state: 'detached' });
		});
	});
});

describe('the field widget in a pane', () => {
	const desktop = sharedApp(DESKTOP);

	test('«fields» picks the field, which is edited where it stands', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			// Through the bar, as a person would, so the fields it offers have it.
			await app.submit('track tea --verb occur');
			const was = await rootTabs(page);
			const tab = await page.evaluate(() => window.datum.layout.current().root.tabs[0].content.children[0].tabs[0].id);
			await page.evaluate((tab) => window.datum.shell.widgetInto(tab, 'setup'), tab);
			const pane = page.locator('section.pane[data-title="setup"]');

			await pane.locator('button.pick').click();
			await page.locator('dialog.sheet[data-picker] button[data-chip="tea"]').click();
			await pane.locator(`${FIELD_FORM} h2`).waitFor();
			assert.equal((await pane.locator(`${FIELD_FORM} h2`).innerText()).trim(), 'tea');
			await pane.locator(`${FIELD_FORM} input.description`).fill('green');
			await pane.locator(`${FIELD_FORM} button.commit`).click();
			await until(() => described(app, 'tea'), (d) => d === 'green');

			// No place to leave: the pane keeps the form, read again from the save.
			assert.deepEqual(await rootTabs(page), was);
			await until(() => pane.locator(`${FIELD_FORM} input.description`).inputValue(), (v) => v === 'green');
		});
	});
});
