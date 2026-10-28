// «Backups» to a WebDAV folder (da-bmxbb, #lib/webdav.ts), against a fake
// WebDAV server on another origin (support/webdav.mjs) that does real CORS
// and checks every request's Basic auth.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { LOGIN, startWebDav } from './support/webdav.mjs';

let dav;
before(async () => {
	await startApp();
	dav = await startWebDav();
});
after(async () => {
	await dav?.close();
	await stopApp();
});

async function openSheet(page) {
	await page.locator('.root-bar button.gear').click();
	await page.getByRole('menuitem', { name: 'Backups', exact: true }).click();
	const sheet = page.locator('section.pane[data-title="backups"]');
	await sheet.locator('.backups').waitFor();
	await sheet.locator('select[data-plan-field="kind"]').selectOption('webdav');
	return sheet;
}

/** Fill the new plan's WebDAV settings: the fake's login, in `folder`, with `fields` over them. */
async function fill(sheet, folder, fields = {}) {
	for (const [field, value] of Object.entries({ url: dav.root + folder, ...LOGIN, ...fields })) {
		const input = sheet.locator(`input[data-plan-field="${field}"]`);
		await input.fill(value);
		await input.dispatchEvent('change');
	}
}

describe('a WebDAV destination', () => {
	test('«Test connection» makes the folder, writes, reads and deletes a probe, and names the step that fails and why', async () => {
		await withApp(async ({ page }) => {
			const sheet = await openSheet(page);
			const press = async () => {
				await sheet.locator('button[data-backup="test"]').click();
				await sheet.locator('[data-probe]').waitFor();
				return sheet.locator('[data-probe] li').allInnerTexts();
			};
			await fill(sheet, 'probe/');
			assert.deepEqual(await press(), ['✓ write', '✓ read', '✓ delete']);
			assert.ok(dav.folders.has('/dav/probe/'), 'the folder was not made');
			assert.deepEqual([...dav.files.keys()], [], 'the probe was left behind');

			// One line, the step that failed, naming what to fix and the server's status.
			const failed = async (...facts) => {
				const lines = await press();
				assert.equal(lines.length, 1, lines.join(' | '));
				assert.match(lines[0], /^✗ write\b/);
				for (const fact of facts) assert.ok(lines[0].includes(fact), `${lines[0]} does not name ${fact}`);
			};
			await fill(sheet, 'probe/', { password: 'wrong' });
			await failed('credentials', '401');
			await fill(sheet, 'nowhere/probe/');
			await failed('folder URL', '409');
			// Refused before any request: no CORS header could help it.
			const asked = dav.requests.length;
			await fill(sheet, 'probe/', { url: 'nas.local/dav/' });
			await failed('folder URL', 'https://');
			assert.equal(dav.requests.length, asked);
			dav.cors = false;
			try {
				await fill(sheet, 'probe/');
				await failed('CORS');
			} finally {
				dav.cors = true;
			}
		});
	});

	test('a plan backs up to its device’s folder in the folder: the slot file and the index, found again, by another device too', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.equal((await app.run(['occur', 'coffee'])).code, 0);
			const sheet = await openSheet(page);
			assert.equal(await sheet.locator('input[data-plan-field="password"]').getAttribute('type'), 'password');
			// The password says how it is kept, under it.
			assert.match(await sheet.locator('label:has(input[data-plan-field="password"]) + p.note-field').innerText(), /unencrypted|not encrypted/, 'the note under the password says it is kept unencrypted');
			const origin = await page.evaluate(() => location.origin);
			assert.match(await sheet.locator('[data-cors]').textContent(), new RegExp(`^Access-Control-Allow-Origin: ${origin}$`, 'm'));

			await fill(sheet, 'plan');
			await sheet.locator('input[data-plan-field="name"]').fill('nas');
			await sheet.locator('button[data-backup="add"]').click();
			await sheet.locator('section.plan[data-plan="nas"] li[data-slot="0"]').waitFor();
			const paths = [...dav.files.keys()].filter((p) => p.startsWith('/dav/plan/')).sort();
			const at = `/dav/plan/client-${await page.evaluate(() => localStorage.getItem('datum-client-id'))}/`;
			assert.equal(paths.length, 2, paths.join(' '));
			assert.ok(paths[0].startsWith(at) && /\/datum\.slot0\.\d{8}T\d{6}\.\d{3}Z\.jsonl\.gz$/.test(paths[0]), paths[0]);
			assert.equal(paths[1], `${at}index.json`);

			// Another plan on the same folder finds it: the form starts over at the first kind.
			await sheet.locator('select[data-plan-field="kind"]').selectOption('webdav');
			await fill(sheet, 'plan/');
			await sheet.getByRole('button', { name: 'Look for backups there', exact: true }).click();
			await sheet.locator('[data-found]').waitFor();
			assert.match(await sheet.locator('[data-found]').innerText(), /\b1\b.*\bdatum\b.*\b0 s\b/);

			// Another device lists its folder (PROPFIND) and finds it as that device's.
			await withApp(async (other) => {
				const sheet = await openSheet(other.page);
				await fill(sheet, 'plan/');
				await sheet.getByRole('button', { name: 'Look for backups there', exact: true }).click();
				await sheet.locator('[data-found]').waitFor();
				// The first device's one, under this one's label and so perhaps this device, earlier.
				assert.match(await sheet.locator('[data-found]').innerText(), /\b1\b.*\bdatum\b.*\bChrome on \w+.*\bearlier\b.*\b0 s\b/);
				assert.deepEqual(other.errors, []);
			});
			assert.deepEqual(app.errors, []);
		});
	});
});
