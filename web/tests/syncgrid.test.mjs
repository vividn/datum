// The Sync widget's grid (da-n0uj1.7): every device against every database,
// a dropdown per cell and «copy from…» per column, each act a CLI command —
// `sync grant` with the sync worker's ticket, `sync chain expel | unjoin |
// wipe | role`, `sync chain copy --defer` — and every expel, unjoin and wipe
// confirmed in a dialog saying what happens to whose data.
//
// A device in the grid is a native copy paired with the page's `datum` (so
// bound to its device key, which a grant is sealed to); `home` is a chain of
// its own with a member admitted as a session admits one, bound to no key,
// whose address is none, so no ticket of `home`'s is made: a grant goes to
// `work`, a chain with this copy alone.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { nativeCopy, pairWith, relayed, startRelay, withWallClock } from './support/sync.mjs';
import { DESKTOP, WIDGET, cli, members, openChain } from './support/syncactions.mjs';
import { until } from './support/until.mjs';

let relay;
before(async () => {
	await startApp();
	relay = await startRelay();
});
after(async () => {
	await relay?.stop();
	await stopApp();
});

const cellOf = (page, device, db) => page.locator(`${WIDGET} .grid tr[data-device="${device}"] td[data-db="${db}"]`);
const stateOf = (page, device, db) => cellOf(page, device, db).getAttribute('data-state');

/**
 * The native copy synced until it holds `db`, as the grant it was sealed
 * makes it: a sync the page's own push to it is in a session with is
 * refused as busy, and the next one takes the grant up.
 */
const joins = (native, db) =>
	until(
		async () => {
			const synced = await native.run(['sync']);
			const status = await native.run(['--db', db, 'sync', 'status', '--json']);
			return status.code === 0 && JSON.parse(status.stdout).chain ? synced : null;
		},
		Boolean
	);

/** The native copy paired with the page's `datum`, `home` in a chain of its own with `phone`; the page on `db`'s sync widget. */
async function gridWith(app, native, db = 'home') {
	assert.equal((await pairWith(app, native)).verified, true);
	const device = JSON.parse((await native.run(['sync', 'status', '--json'])).stdout).this.device;
	const phone = await openChain(app);
	await app.goto(`/sync?db=${db}`);
	await cellOf(app.page, device, 'datum').waitFor();
	return { device, phone };
}

describe('the sync widget’s grid', () => {
	test('a grant from a cell: the device joins the database when it next syncs, and its cell says member', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await cli(app, ['--db', 'work', 'sync', 'new-invite']);
					const { device } = await gridWith(app, native, 'work');
					assert.equal(await stateOf(page, device, 'datum'), 'member');
					assert.equal(await stateOf(page, device, 'work'), 'none');
					// This device's cells offer nothing: it leaves from More.
					assert.equal(await page.locator(`${WIDGET} .grid tr.here select`).count(), 0);

					await cellOf(page, device, 'work').locator('select').selectOption('member');
					const asked = page.locator('dialog.ask');
					assert.match(await asked.innerText(), /Grant .* membership of work\. The name it takes on that device:/);
					assert.equal(await asked.locator('input').inputValue(), 'work');
					await asked.getByRole('button', { name: 'grant' }).click();
					const said = page.locator(`${WIDGET} .grid .said`);
					await said.waitFor();
					assert.match(await said.innerText(), /^granted .* member of this database: the invite rides sealed to it in datum's sync chain/);

					assert.match((await joins(native, 'work')).stdout, /joined «work»/);
					// Its describe, binding the endpoint to its device, reaches the page on its next sync.
					assert.equal((await native.run(['--db', 'work', 'sync'])).code, 0);
					await until(() => stateOf(page, device, 'work'), (s) => s === 'member');
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});

	test('expel, unjoin and wipe each say what happens to whose data, and a cancel does nothing; a wipe confirmed is asked of the device', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					const { phone } = await gridWith(app, native);
					const select = cellOf(page, phone, 'home').locator('select');
					const dialog = page.locator('dialog.confirm');
					const words = {
						expel: /^Expel phone from home\? It can no longer sync home until a new invite admits it again\. It keeps its own copy, and what it wrote stays on every device\./,
						unjoin: /^Unjoin phone from home\? It made no change to an entry: this only expels it\. It is expelled too/,
						wipe: /^Wipe phone’s copy of home\? It is expelled now, and asked to delete its own copy of home when it next reaches a device of this chain\. A device that never comes back online cannot be made to delete anything: until it obeys, this is only an expel/
					};
					for (const [act, said] of Object.entries(words)) {
						await select.selectOption(act);
						await dialog.waitFor();
						assert.match(await dialog.innerText(), said, act);
						await dialog.getByRole('button', { name: 'cancel' }).click();
						await dialog.waitFor({ state: 'detached' });
						assert.equal(await stateOf(page, phone, 'home'), 'member', `${act} cancelled`);
					}
					const it = (await members(app)).members.find((m) => m.endpoint === phone);
					assert.ok(!it.revoked, 'nothing was written');

					await select.selectOption('wipe');
					await dialog.locator('input').fill('phone');
					await dialog.getByRole('button', { name: 'wipe' }).click();
					await until(() => stateOf(page, phone, 'home'), (s) => s === 'wiping');
					assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});

	test('«copy from…» starts a database in no chain with another’s admins, and grants it each of that one’s devices', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await cli(app, ['--db', 'notes', 'occur', 'jot']);
					const { device } = await gridWith(app, native);
					await page.locator(`${WIDGET} .grid th[data-db="notes"]`).getByRole('button', { name: 'copy from…' }).click();
					const dialog = page.locator('dialog.confirm');
					assert.match(await dialog.innerText(), /^Copy sync settings into notes from which database\? notes starts a sync chain with the same admins/);
					await dialog.getByRole('button', { name: 'datum' }).click();
					const said = page.locator(`${WIDGET} .grid .said`);
					await said.waitFor();
					assert.match(await said.innerText(), /^started sync chain \S+: open, as datum's is\ngranted .* member of this database/);
					assert.ok(JSON.parse(await cli(app, ['--db', 'notes', 'sync', 'status', '--json'])).chain);

					assert.match((await joins(native, 'notes')).stdout, /joined «notes»/);
					assert.equal((await native.run(['--db', 'notes', 'sync'])).code, 0);
					await until(() => stateOf(page, device, 'notes'), (s) => s === 'member');
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});
});
