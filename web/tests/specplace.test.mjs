// A field's spec is a PLACE (da-hbrfg): «a modal asks; a place is a tab».
// The `spec` widget takes the field, and `spec:<field>` is its tab — opened
// beside the tab you were on, focused again for the same field, closed when
// left. The tab tests run on a phone, where a place is a tab whatever a
// desktop comes to open it as (da-jozgv).

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { DESKTOP, PHONE } from './support/layout.mjs';
import { SPEC, cancelSpec, openSpec } from './support/spec.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const rootTabs = (page) =>
	page.locator('.root-bar [role="tab"]').evaluateAll((all) => all.map((b) => b.textContent.replace('•', '').trim()));

const shown = (page) =>
	page.locator('.root-bar [role="tab"][aria-selected="true"]').evaluate((b) => b.textContent.replace('•', '').trim());

const described = async (app, field) => JSON.parse((await app.run(['spec', 'show', field])).stdout).description;

describe('the spec place, on a phone', () => {
	const phone = sharedApp(PHONE);

	test('opens beside the tab you were on, one tab per field, and back closes it', async () => {
		await phone.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'mood', '--verb', 'switch']);
			const was = await rootTabs(page);
			const from = was.indexOf(await shown(page));

			await openSpec(page, 'mood');
			await until(() => shown(page), (t) => t === 'spec:mood');
			const beside = [...was.slice(0, from + 1), 'spec:mood', ...was.slice(from + 1)];
			assert.deepEqual(await rootTabs(page), beside);
			assert.equal((await page.locator(`${SPEC} h2`).innerText()).trim(), 'mood');

			// The same field again is the same tab, and whoever opened it first
			// is answered — nothing is left waiting (canary on #360).
			const first = page.evaluate(() => window.datum.spec('mood'));
			await page.locator(SPEC).waitFor();
			await page.evaluate(() => void window.datum.spec('mood'));
			assert.equal(await first, null);
			await page.locator(SPEC).waitFor();
			assert.deepEqual(await rootTabs(page), beside, 'a second open made a second tab');

			await page.goBack();
			await until(() => rootTabs(page), (t) => t.join() === was.join());
			assert.equal(await page.locator(SPEC).count(), 0);
		});
	});

	test('Done saves in one `spec set` and returns; Cancel returns and writes nothing', async () => {
		await phone.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			const was = await rootTabs(page);

			let { saved } = await openSpec(page, 'tea');
			await page.locator(`${SPEC} input.description`).fill('green');
			await page.locator(`${SPEC} button.commit`).click();
			const result = await saved;
			assert.equal(result.code, 0, result.stderr);
			assert.match(result.argv.join(' '), /^spec set tea --from-json /);
			const sent = JSON.parse(result.argv[result.argv.indexOf('--from-json') + 1]);
			assert.equal(sent.description, 'green', 'what Done sent');
			await until(() => rootTabs(page), (t) => t.join() === was.join());
			assert.equal(await described(app, 'tea'), 'green');

			({ saved } = await openSpec(page, 'tea'));
			await page.locator(`${SPEC} input.description`).fill('black');
			await cancelSpec(page);
			assert.equal(await saved, null);
			await until(() => rootTabs(page), (t) => t.join() === was.join());
			assert.equal(await described(app, 'tea'), 'green');
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

			const { saved } = await openSpec(page, 'tea');
			await page.locator(`${SPEC} input.description`).fill('green');
			const big = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><path d="${'M0 0'.repeat(1200)}"/></svg>`;
			await page.locator(`${SPEC} .drop input[type="file"]`).setInputFiles({ name: 'big.svg', mimeType: 'image/svg+xml', buffer: Buffer.from(big) });
			await page.locator(`${SPEC} .drop`, { hasText: 'big.svg' }).waitFor();
			await page.locator(`${SPEC} button.commit`).click();
			const result = await saved;
			assert.notEqual(result.code, 0);
			assert.match(result.stderr, /at most 4 kB/);

			assert.equal(await shown(page), 'spec:tea', 'the refusal left the place');
			assert.equal(await page.locator(`${SPEC} input.description`).inputValue(), 'green', 'the edit was lost');
			assert.equal(await described(app, 'tea'), null, 'the refusal wrote');

			await cancelSpec(page);
			await until(() => rootTabs(page), (t) => t.join() === was.join());
		});
	});
});

// On a desktop a place is a modal (da-jozgv): the same widget, the same form.
describe('the spec place, on a desktop', () => {
	const desktop = sharedApp(DESKTOP);

	test('opens in the place modal, and Done saves, closes it and answers', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			await app.run(['track', 'tea', '--verb', 'occur']);
			const was = await rootTabs(page);

			const { saved } = await openSpec(page, 'tea');
			const modal = page.locator('dialog.sheet[data-place="spec:tea"]');
			await modal.locator(`${SPEC} h2`).waitFor();
			assert.equal((await modal.locator(`${SPEC} h2`).innerText()).trim(), 'tea');
			assert.deepEqual(await rootTabs(page), was, 'a desktop opened a tab');

			await modal.locator(`${SPEC} input.description`).fill('green');
			await modal.locator(`${SPEC} button.commit`).click();
			const result = await saved;
			assert.equal(result.code, 0, result.stderr);
			await modal.waitFor({ state: 'detached' });
			assert.equal(await described(app, 'tea'), 'green');
		});
	});
});

describe('the spec widget in a pane', () => {
	const desktop = sharedApp(DESKTOP);

	test('asks which field, then edits it where it stands', async () => {
		await desktop.solo(async (app) => {
			const { page } = app;
			// Through the bar, as a person would, so the fields it offers have it.
			await app.submit('track tea --verb occur');
			const was = await rootTabs(page);
			const tab = await page.evaluate(() => window.datum.layout.current().root.tabs[0].content.children[0].tabs[0].id);
			await page.evaluate((tab) => window.datum.shell.widgetInto(tab, 'spec'), tab);
			const pane = page.locator('section.pane[data-title="spec"]');

			await pane.locator('button[data-field="tea"]').click();
			assert.equal((await pane.locator(`${SPEC} h2`).innerText()).trim(), 'tea');
			await pane.locator(`${SPEC} input.description`).fill('green');
			await pane.locator(`${SPEC} button.commit`).click();
			await until(() => described(app, 'tea'), (d) => d === 'green');

			// No place to leave: the pane keeps the form, read again from the save.
			assert.deepEqual(await rootTabs(page), was);
			await until(() => pane.locator(`${SPEC} input.description`).inputValue(), (v) => v === 'green');
		});
	});
});
