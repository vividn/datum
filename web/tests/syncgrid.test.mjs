// The Sync widget's grid (da-n0uj1.7): every device against every database,
// a dropdown per cell and «grant like…» per column, each act a CLI command —
// `sync grant` with the sync worker's ticket, `sync chain expel | unjoin |
// wipe | role`, `sync grant --like --defer` — and every expel, unjoin and wipe
// confirmed in a dialog saying what happens to whose data.
//
// A device in the grid is a native copy paired with the page's `datum` (so
// bound to its device key, which a grant is sealed to); `home` is a chain of
// its own with a member admitted as a session admits one, bound to no key,
// whose address is none, so no ticket of `home`'s is made: a grant goes to
// `work`, a chain with this copy alone.

import test, { after, before, describe } from 'node:test';
import { createHash, generateKeyPairSync, sign } from 'node:crypto';
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

/**
 * A software passkey answering the page's `navigator.credentials.get` as one at datum.day would (P-256,
 * user present and verified), so a native copy reached through the local relay can share the test: a
 * virtual authenticator needs the page at https://datum.day, which reaches no local relay. Its admin
 * key's JSON, as `sync admin add` takes it; `window.passkeyAsks` counts its approvals.
 */
async function softPasskey(page) {
	const { privateKey, publicKey } = generateKeyPairSync('ec', { namedCurve: 'prime256v1' });
	const sha = (b) => createHash('sha256').update(b).digest();
	await page.exposeFunction('softAssert', (challenge) => {
		const auth = Buffer.concat([sha('datum.day'), Buffer.from([0x05, 0, 0, 0, 0])]);
		const client = Buffer.from(JSON.stringify({ type: 'webauthn.get', origin: 'https://datum.day', challenge: Buffer.from(challenge).toString('base64url') }));
		const signature = sign('sha256', Buffer.concat([auth, sha(client)]), { key: privateKey, dsaEncoding: 'der' });
		return [auth, client, signature].map((b) => [...b]);
	});
	await page.evaluate(() => {
		window.passkeyAsks = 0;
		navigator.credentials.get = async (o) => {
			window.passkeyAsks++;
			const [a, c, s] = (await window.softAssert([...new Uint8Array(o.publicKey.challenge)])).map((b) => new Uint8Array(b).buffer);
			return { response: { authenticatorData: a, clientDataJSON: c, signature: s } };
		};
	});
	return JSON.stringify({ alg: -7, publicKey: publicKey.export({ format: 'der', type: 'spki' }).toString('base64url'), label: 'soft' });
}

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
					assert.ok((await asked.innerText()).includes('work'));
					assert.equal(await asked.locator('input').inputValue(), 'work');
					await asked.getByRole('button', { name: 'grant' }).click();
					const said = page.locator(`${WIDGET} .grid .said`);
					await said.waitFor();
					// Granted as a member, carried by datum's chain.
					assert.match(await said.innerText(), /^granted\b.*\bmember\b.*\bdatum\b/);

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
					// Each asks its own act, of phone's data in home.
					const words = { expel: /^Expel\b/, unjoin: /^Unjoin\b/, wipe: /^Wipe\b/ };
					for (const [act, said] of Object.entries(words)) {
						await select.selectOption(act);
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

	test('«grant like…» starts a database in no chain with another’s admins, and grants it each of that one’s devices', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await cli(app, ['--db', 'notes', 'occur', 'jot']);
					const { device } = await gridWith(app, native);
					await page.locator(`${WIDGET} .grid th[data-db="notes"]`).getByRole('button', { name: 'grant like…' }).click();
					const dialog = page.locator('dialog.confirm');
					assert.ok((await dialog.innerText()).includes('notes'));
					await dialog.getByRole('button', { name: 'datum' }).click();
					const said = page.locator(`${WIDGET} .grid .said`);
					await said.waitFor();
					// The chain it started, then the grant, as a member.
					const { chain } = JSON.parse(await cli(app, ['--db', 'notes', 'sync', 'status', '--json']));
					assert.ok(chain);
					const lines = (await said.innerText()).split('\n');
					assert.ok(lines[0].includes(chain), lines[0]);
					assert.match(lines[1], /^granted\b.*\bmember\b/);

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

	test('«grant like…» a passkey-locked chain asks the passkey once for the new chain and every invite (da-7bjai)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await cli(app, ['--db', 'notes', 'occur', 'jot']);
					const { device } = await gridWith(app, native, 'datum');
					await cli(app, ['--db', 'datum', 'sync', 'admin', 'add', await softPasskey(page)]);
					await page.locator(`${WIDGET} .grid th[data-db="notes"]`).getByRole('button', { name: 'grant like…' }).click();
					await page.locator('dialog.confirm').getByRole('button', { name: 'datum' }).click();
					const said = page.locator(`${WIDGET} .grid .said`);
					await said.waitFor();
					assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0, await page.locator(WIDGET).innerText());
					assert.match(await said.innerText(), /\bgranted\b.*\bmember\b/);
					assert.equal(await page.evaluate(() => window.passkeyAsks), 1);
					const admins = JSON.parse(await cli(app, ['--db', 'notes', 'sync', 'admin', 'list', '--json'])).admins;
					assert.equal(admins.length, 1);

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
