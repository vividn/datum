// A device's menu in the Sync widget's grid (da-jlyyr): its name, tapped, offers
// Rename, Remove and Sync all, each the chain's own record, grayed with the
// reason where this copy may not. vividn, 2026-11-10: «Clicking on a device
// should let you rename, remove, and sync all … (but you shouldn't be able to
// for example rename if you only have read access)».
//
// `phone` is a member admitted as a session admits one, bound to no device key,
// so Sync all, an invite sealed to a key, is grayed for it here: syncgrid's
// native copy is the device it adds every database to.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { withWallClock } from './support/sync.mjs';
import { DESKTOP, WIDGET, cli, deviceMenu, gridLabels, members, openChain } from './support/syncactions.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const PHONE = { width: 390, height: 844 };
const labelOf = async (app, endpoint) => (await members(app)).members.find((m) => m.endpoint === endpoint)?.label;

describe('a device’s menu', () => {
	test('another device’s name renames it and removes it from the grid, each the chain’s record; a read-only copy grays them, saying why', async () => {
		for (const viewport of [PHONE, DESKTOP])
			await withWallClock(
				async (app) => {
					const { page } = app;
					const phone = await openChain(app);
					await app.goto('/sync?db=home');
					await page.locator(`${WIDGET} .grid tbody th`).getByRole('button', { name: 'phone' }).waitFor();
					// This device's own name opens nothing: its ✎ renames it, a database's menu stops it syncing.
					assert.equal(await page.locator(`${WIDGET} .grid tr.here th button`).count(), 0);

					const menu = await deviceMenu(page, 'phone');
					const acts = menu.locator('button[data-do]');
					assert.deepEqual(await acts.allInnerTexts(), ['Rename', 'Remove', 'Sync all']);
					assert.equal(await menu.locator('[data-do="rename"]').isDisabled(), false);
					assert.equal(await menu.locator('[data-do="remove"]').isDisabled(), false);
					assert.equal(await menu.locator('.why[data-for="change"]').count(), 0);
					// No device key to seal an invite to, and the reason says so.
					assert.equal(await menu.locator('[data-do="all"]').isDisabled(), true);
					assert.match(await menu.locator('.why[data-for="all"]').innerText(), /no device key/);

					await menu.locator('[data-do="rename"]').click();
					const asked = page.locator('dialog.ask');
					assert.equal(await asked.locator('input').inputValue(), 'phone');
					await asked.locator('input').fill('tablet');
					await asked.getByRole('button', { name: 'rename' }).click();
					await until(() => gridLabels(page), (labels) => labels.includes('tablet'));
					assert.equal(await labelOf(app, phone), 'tablet');

					await (await deviceMenu(page, 'tablet')).locator('[data-do="remove"]').click();
					const dialog = page.locator('dialog.confirm');
					// The device, the database, and the act a lost or stolen device takes instead (da-fstz5).
					for (const fact of ['tablet', 'home', 'Unjoin']) assert.ok((await dialog.innerText()).includes(fact), fact);
					await dialog.getByRole('button', { name: 'remove' }).click();
					await until(() => gridLabels(page), (labels) => !labels.includes('tablet'));
					assert.ok((await members(app)).members.find((m) => m.endpoint === phone).revoked);
					assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);

					// This copy made a reader: it renames and removes no one, and says why.
					const kindle = await openChain(app, 'kindle');
					const me = JSON.parse(await cli(app, ['--db', 'home', 'sync', 'status', '--json'])).this.endpoint;
					await cli(app, ['--db', 'home', 'sync', 'chain', 'role', me, 'reader']);
					await app.goto('/sync?db=home');
					const ro = await deviceMenu(page, 'kindle');
					for (const a of ['rename', 'remove', 'all']) assert.equal(await ro.locator(`[data-do="${a}"]`).isDisabled(), true, a);
					assert.match(await ro.locator('.why[data-for="change"]').innerText(), /only reads home/);
					assert.equal(await labelOf(app, kindle), 'kindle');
				},
				{ viewport }
			);
	});
});
