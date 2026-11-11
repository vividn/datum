// The Sync widget's grid (da-n0uj1.7): every device against every database,
// a cell tapped opening that pair's details and acts (da-dxng1), each act a CLI command — `sync chain invite` with the
// sync worker's ticket, `sync chain expel [--unjoin] [--wipe] | role` — and
// every expel, unjoin and wipe confirmed in a dialog saying what happens to
// whose data. A device expelled, unjoined or wiped is drawn no more (da-5wlnr).
// A device's menu's Sync all is an invite to it for every database here it is
// not in (da-jlyyr); its Rename and Remove are syncdevice's.
//
// A device in the grid is a native copy paired with the page's `datum` (so
// bound to its device key, which a grant is sealed to); `home` is a chain of
// its own with a member admitted as a session admits one, bound to no key,
// whose address is none, so no ticket of `home`'s is made: a grant goes to
// `work`, a chain with this copy alone.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { countOf, nativeCopy, pairWith, relayed, startRelay, withWallClock } from './support/sync.mjs';
import { DESKTOP, WIDGET, cellAct, cli, deviceMenu, members, openChain } from './support/syncactions.mjs';
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

/**
 * One sync of `db` that got through: the page's own session refusing it
 * leaves it none synced, which the next one gets past. Any other failure
 * fails in its words. The CLI's words only steer the wait: reworded, this
 * is a plain assert again, which fails saying them.
 */
const syncs = async (native, db) => {
	const { code, stderr } = await until(() => native.run(['--db', db, 'sync']), (r) => r.code === 0 || !r.stderr.includes('none of'));
	assert.equal(code, 0, stderr);
};

/** The native copy paired with the page's `datum`, `home` in a chain of its own with `phone`; the page on `db`'s sync widget. */
async function gridWith(app, native, db = 'home') {
	assert.equal((await pairWith(app, native)).verified, true);
	const device = JSON.parse((await native.run(['sync', 'status', '--json'])).stdout).this.device;
	const phone = await openChain(app);
	await app.goto(`/sync?db=${db}`);
	// Every database's column: the grid opens on `db`'s alone (da-opbpa).
	const fold = app.page.locator(`${WIDGET} .grid button.fold`);
	if ((await fold.getAttribute('aria-expanded')) === 'false') await fold.click();
	await cellOf(app.page, device, 'datum').waitFor();
	return { device, phone };
}

describe('the sync widget’s grid', () => {
	test('an invite from a cell: the device joins the database when it next syncs, and its cell says member', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await cli(app, ['--db', 'work', 'sync', 'new-invite']);
					const { device } = await gridWith(app, native, 'work');
					assert.equal(await stateOf(page, device, 'datum'), 'member');
					assert.equal(await stateOf(page, device, 'work'), 'none');
					// This device's cells offer no act: it stops syncing from a database's menu.
					await page.locator(`${WIDGET} .grid tr.here td[data-db="work"] button.cell`).click();
					await page.locator(`${WIDGET} .grid .pair[data-device="this"][data-db="work"]`).waitFor();
					assert.equal(await page.locator(`${WIDGET} .grid .pair button`).count(), 0);

					const label = await page.locator(`${WIDGET} .grid tr[data-device="${device}"] th`).innerText();
					await cellAct(page, label, 'work', 'member');
					const asked = page.locator('dialog.ask');
					assert.ok((await asked.innerText()).includes('work'));
					assert.equal(await asked.locator('input').inputValue(), 'work');
					await asked.getByRole('button', { name: 'invite' }).click();
					const said = page.locator(`${WIDGET} .grid .said`);
					await said.waitFor();
					// Invited as a member, carried by datum's chain.
					assert.match(await said.innerText(), /^invited\b.*\bmember\b.*\bdatum\b/);
					// Waiting on its row until it is taken (da-pofki).
					const grantOf = () => cellOf(page, device, 'work').getAttribute('data-grant');
					await until(grantOf, (s) => s === 'waiting');
					assert.equal(await cellOf(page, device, 'datum').getAttribute('data-grant'), null);

					assert.match((await joins(native, 'work')).stdout, /joined «work»/);
					// Its describe, binding the endpoint to its device, reaches the page on its next sync.
					await syncs(native, 'work');
					await until(() => stateOf(page, device, 'work'), (s) => s === 'member');
					await until(grantOf, (s) => s === null);
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
					const dialog = page.locator('dialog.confirm');
					// Each asks its own act, of phone's data in home.
					const words = { expel: /^Expel\b/, unjoin: /^Unjoin\b/, wipe: /^Wipe\b/ };
					for (const [act, said] of Object.entries(words)) {
						await cellAct(page, 'phone', 'home', act);
						await dialog.waitFor();
						const text = await dialog.innerText();
						assert.match(text, said, act);
						for (const fact of ['phone', 'home']) assert.ok(text.includes(fact), `${act}: ${fact}`);
						await dialog.getByRole('button', { name: 'cancel' }).click();
						await dialog.waitFor({ state: 'detached' });
						assert.equal(await stateOf(page, phone, 'home'), 'member', `${act} cancelled`);
					}
					const it = (await members(app)).members.find((m) => m.endpoint === phone);
					assert.ok(!it.revoked, 'nothing was written');

					await cellAct(page, 'phone', 'home', 'wipe');
					await dialog.locator('input').fill('phone');
					await dialog.getByRole('button', { name: 'wipe' }).click();
					// Asked of the device, which is expelled: drawn no more (da-5wlnr).
					await cellOf(page, phone, 'home').waitFor({ state: 'detached' });
					const { devices, chains } = JSON.parse(await cli(app, ['--db', 'home', 'sync', 'grid', '--json']));
					const home = chains.find((c) => c.db === 'home').chain;
					assert.equal(devices.find((d) => d.endpoint === phone || d.cells[home]?.endpoint === phone).cells[home].state, 'wiping');
					assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});

	test('Sync all invites the device to every database here it is not in, one in no chain starting its chain: it joins each, and a read-only copy may not (da-jlyyr)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					assert.equal((await pairWith(app, native)).verified, true);
					const device = JSON.parse((await native.run(['sync', 'status', '--json'])).stdout).this.device;
					// `work` in a chain with this copy alone, `notes` in none.
					await cli(app, ['--db', 'work', 'sync', 'new-invite']);
					await cli(app, ['--db', 'notes', 'occur', 'tea']);
					await app.goto('/sync');
					await page.locator(`${WIDGET} .grid button.fold`).click();
					await cellOf(page, device, 'notes').waitFor();
					const label = await page.locator(`${WIDGET} .grid tr[data-device="${device}"] th`).innerText();

					const menu = await deviceMenu(page, label);
					const all = menu.locator('[data-do="all"]');
					assert.equal(await all.isDisabled(), false);
					assert.equal(await menu.locator('.why').count(), 0);
					await all.click();
					const said = page.locator(`${WIDGET} .grid .said`);
					await said.waitFor();
					assert.equal((await said.innerText()).split('\n').length, 2, await said.innerText());
					// Waiting in each column it was not in; `notes` has a chain now.
					for (const db of ['work', 'notes']) await until(() => cellOf(page, device, db).getAttribute('data-grant'), (s) => s === 'waiting');
					assert.ok(JSON.parse(await cli(app, ['--db', 'notes', 'sync', 'status', '--json'])).chain);

					for (const db of ['work', 'notes']) {
						await joins(native, db);
						await syncs(native, db);
					}
					for (const db of ['datum', 'work', 'notes']) await until(() => stateOf(page, device, db), (s) => s === 'member');
					assert.match(await (await deviceMenu(page, label)).locator('.why[data-for="all"]').innerText(), /already syncs every database here/);

					// This copy made a reader of each: it renames, removes and invites no one, and says why.
					for (const db of ['datum', 'work', 'notes']) {
						const me = JSON.parse(await cli(app, ['--db', db, 'sync', 'status', '--json'])).this.endpoint;
						await cli(app, ['--db', db, 'sync', 'chain', 'role', me, 'reader']);
					}
					await app.goto('/sync');
					const ro = await deviceMenu(page, label);
					for (const a of ['rename', 'remove', 'all']) assert.equal(await ro.locator(`[data-do="${a}"]`).isDisabled(), true, a);
					assert.match(await ro.locator('.why[data-for="change"]').innerText(), /only reads datum/);
					assert.match(await ro.locator('.why[data-for="all"]').innerText(), /only reads what it shares/);
				},
				{ prepare: relayed(relay), viewport: { width: 390, height: 844 } }
			);
		} finally {
			await native.close();
		}
	});

	test('a join across a sync floor joins: the grid shows the chain at once, the inviter marked for this copy to update, and no entry moves (da-52vdj)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await app.run(['occur', 'coffee', '-t', '0900']);
					await native.run(['occur', 'walk']);
					// The inviter says it runs schema 999, its sync floor there: the page is below it.
					const over = { DATUM_SYNC_TEST_SCHEMA: '999', DATUM_SYNC_TEST_FLOOR: '999' };
					const serving = native.spawn(['sync', 'chain', 'invite'], over);
					const said = await until(() => /datum sync chain join '([^']+)'/.exec(serving.stdout), Boolean);
					const report = await page.evaluate((t) => window.datum.sync.connect('datum', t), said[1]);
					assert.match(report.floor ?? '', /: it needs schema 999, this copy runs \d+/);
					const device = JSON.parse((await native.run(['sync', 'status', '--json'])).stdout).this.device;
					const chain = JSON.parse((await native.run(['sync', 'status', '--json'])).stdout).chain;
					assert.equal(JSON.parse((await app.run(['sync', 'status', '--json'])).stdout).chain, chain);
					await app.goto('/sync');
					await until(() => cellOf(page, device, 'datum').getAttribute('data-apart'), (s) => s === 'this');
					assert.equal(await stateOf(page, device, 'datum'), 'member');
					assert.equal(await countOf(app, 'walk'), 0);
					assert.equal(await native.count('coffee'), 0);
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});
});
