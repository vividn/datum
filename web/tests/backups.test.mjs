// «Backups…» (da-9ow8m, $lib/backups.ts, $lib/BackupsSheet.svelte): the
// gear's sheet of Tower of Hanoi plans, driven here against the one
// destination this bead ships, `memory` — a bucket in the tab, offered on
// the DEBUG database alone (vremya on #217), which is why the plan tests
// open `?db=DEBUG`. S3 is da-pstzw. Every number the sheet shows is the CLI's, so what is asserted
// is what the sheet SAYS and what the database HOLDS, never slot arithmetic.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const HOUR = 3_600_000;

/** The one database the in-memory destination is offered on. */
const DEBUG = { query: '?db=DEBUG' };

async function openSheet(page) {
	await page.locator('.root-bar button.gear').click();
	await page.getByRole('menuitem', { name: 'Backups…', exact: true }).click();
	const sheet = page.locator('dialog.sheet', { hasText: 'Backups' });
	await sheet.waitFor();
	return sheet;
}

/** Add an hourly, uncapped plan named `name` on bucket `bucket`, for this page's database. */
async function addPlan(sheet, name, bucket) {
	await sheet.locator('input[data-plan-field="name"]').fill(name);
	await sheet.locator('input[data-plan-field="bucket"]').fill(bucket);
	await sheet.locator('input[data-plan-field="bucket"]').dispatchEvent('change');
	await sheet.locator('button[data-backup="add"]').click();
	return sheet.locator(`section.plan[data-plan="${name}"]`);
}

/** What the sheet says about slot `n` of a plan: its age. */
const ageOf = (plan, n) => plan.locator(`li[data-slot="${n}"] .age`).innerText();

describe('«Backups…»', () => {
	test('is the gear’s, and the root tab menu has no export backup… any more', async () => {
		await withApp(async ({ page }) => {
			const sheet = await openSheet(page);
			assert.match(await sheet.innerText(), /not encrypted/);
			assert.match(await sheet.innerText(), /closed browser takes no backups/);
			await page.keyboard.press('Escape');
			await sheet.waitFor({ state: 'detached' });
			await page.locator('.root-bar [role="tab"][aria-selected="true"]').click();
			const items = await page.locator('[role="menu"] button').allInnerTexts();
			assert.ok(items.length > 0);
			assert.ok(!items.some((t) => /export backup/.test(t)), `the root menu reads ${items.join(' | ')}`);
		});
	});

	// Off the DEBUG database there is no destination to offer until S3: a
	// tab's bucket behind a plan that survives the reload would look like
	// protection and hold nothing. So no form, and a line saying why.
	test('offers no plan form off the DEBUG database, only the file buttons and why', async () => {
		await withApp(async ({ page }) => {
			const sheet = await openSheet(page);
			assert.equal(await sheet.locator('[data-no-destination]').innerText(), 'Plans need a destination to keep backups at; S3 is next.');
			assert.equal(await sheet.locator('button[data-backup="add"]').count(), 0);
			assert.equal(await sheet.locator('option', { hasText: 'memory' }).count(), 0);
			assert.equal(await sheet.locator('button[data-backup="save"]').count(), 1);
			assert.equal(await sheet.locator('button[data-backup="restore-file"]').count(), 1);
		});
	});

	test('a plan writes slot 0, then the next period’s slot, and shows both with their ages', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			const sheet = await openSheet(page);
			// The explanation is the CLI's `--plan`: files and reach, per span.
			await sheet.locator('.add .explain').waitFor();
			assert.match(await sheet.locator('.add .explain').innerText(), /after 1 d, \d+ files reaching back/);

			// A new plan is due at once: its first backup is slot 0.
			const plan = await addPlan(sheet, 'hourly', 'b1');
			await plan.locator('li[data-slot="0"]').waitFor();
			assert.equal(await ageOf(plan, 0), '0 s ago');
			assert.match(await plan.locator('[data-last]').innerText(), /^last backup 0 s ago$/);

			// An hour on, «Back up now» takes the next period's: slot 1.
			await app.clock.forward(HOUR);
			await plan.locator('button[data-backup="now"]').click();
			await plan.locator('li[data-slot="1"]').waitFor();
			assert.equal(await ageOf(plan, 1), '0 s ago');
			assert.equal(await ageOf(plan, 0), '1 h ago');
			// Newest first.
			assert.deepEqual(await plan.locator('li[data-slot]').evaluateAll((li) => li.map((l) => l.dataset.slot)), ['1', '0']);

			// Another plan on the same bucket is told what is already there
			// before it writes anything.
			await sheet.locator('input[data-plan-field="bucket"]').fill('b1');
			await sheet.locator('input[data-plan-field="bucket"]').dispatchEvent('change');
			await sheet.getByRole('button', { name: 'Look for backups there', exact: true }).click();
			await sheet.locator('[data-found]').waitFor();
			const found = '2 backups of DEBUG found, newest 0 s ago Add plan continues that rotation.';
			assert.equal(await sheet.locator('[data-found]').innerText(), found);

			// And «Add plan» without looking first says the same, and writes nothing yet.
			await sheet.locator('input[data-plan-field="bucket"]').fill('b1');
			await sheet.locator('input[data-plan-field="bucket"]').dispatchEvent('change');
			await sheet.locator('input[data-plan-field="name"]').fill('again');
			await sheet.locator('button[data-backup="add"]').click();
			await sheet.locator('[data-found]').waitFor();
			assert.equal(await sheet.locator('[data-found]').innerText(), found);
			assert.equal(await sheet.locator('section.plan[data-plan="again"]').count(), 0, 'the plan was added unseen');
			// Added, it reads the same rotation.
			await sheet.locator('button[data-backup="add"]').click();
			const again = sheet.locator('section.plan[data-plan="again"]');
			await again.locator('li[data-slot="1"]').waitFor();
			assert.deepEqual(await again.locator('li[data-slot]').evaluateAll((li) => li.map((l) => l.dataset.slot)), ['1', '0']);
			assert.equal(await ageOf(again, 1), '0 s ago');
			assert.deepEqual(app.errors, []);
		}, DEBUG);
	});

	test('restore shows the dry run’s counts, and after yes the database is the backup’s', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			const sheet = await openSheet(page);
			const plan = await addPlan(sheet, 'hourly', 'b2');
			await plan.locator('li[data-slot="0"]').waitFor();
			const backedUp = (await app.allEntries(500)).map((e) => e.hid).sort();

			await app.clock.forward(1000);
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			assert.equal((await app.allEntries(500)).length, backedUp.length + 1);

			await plan.locator('li[data-slot="0"] button', { hasText: 'Restore…' }).click();
			const confirm = page.locator('dialog.confirm');
			await confirm.waitFor();
			assert.match(await confirm.innerText(), /: 1 deleted, 0 added, 0 changed\./);
			await confirm.getByRole('button', { name: 'restore', exact: true }).click();
			await sheet.locator('[role="status"]', { hasText: 'DEBUG restored' }).waitFor();
			assert.deepEqual((await app.allEntries(500)).map((e) => e.hid).sort(), backedUp);
			assert.deepEqual(app.errors, []);
		}, DEBUG);
	});
});
