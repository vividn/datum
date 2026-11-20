// Signing in to sync with a passkey (da-nf1p4): its WebAuthn PRF output, from
// which the CLI derives the account key — an admin of every chain made signed
// in, and what signs the invite a device of the account redeems to join one
// with no other device's invite. The tab keeps what reads the account's
// record and nothing that signs: each act the account signs asks the passkey
// again (da-k04lr), and a test not about the prompt hands the run the PRF
// output, as the page does once it has asked (`withPrf`).
//
// A passkey is Chromium's virtual authenticator (CDP `WebAuthn`), with PRF
// (`hasPrf`) or without; the page is served as https://datum.day, the passkey's
// relying party. The two devices are two browser contexts on a local relay.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { countOf, relayed, startRelay, verifyOf, withWallClock } from './support/sync.mjs';
import { AT_DATUM_DAY, WIDGET, authenticator } from './support/syncactions.mjs';
import { pkarrRelay } from './support/pkarr.mjs';
import { until } from './support/until.mjs';

/** The PRF output whose account key `chain.rs`'s derivation test names: the web's is the same CLI's. */
const PRF = '07'.repeat(32);
const KEY = 'ed25519:f71518e0672bdcd79e5cb2e5a1143eaaa5699998a08f28b2fa10716998e761ef';

let relay;
before(async () => {
	await startApp();
	relay = await startRelay();
});
after(async () => {
	await relay?.stop();
	await stopApp();
});

const signIn = (app, secret) => app.page.evaluate((s) => window.datum.sync.signIn(undefined, s), secret);
/** `argv` run with the passkey's PRF output in hand, as the page runs what it just asked the passkey for. */
const withPrf = (app, argv, prf = PRF) =>
	app.page.evaluate(([a, p]) => window.datum.runQuiet(a, undefined, undefined, undefined, undefined, [`DATUM_ACCOUNT=${p}`]), [argv, prf]);
/** A `sync` act in `datum` as the Sync widget runs one: a locked chain's approved with the passkey. */
const act = (app, argv) => app.page.evaluate((a) => window.datum.sync.act('datum', a), argv);
/** How many times the passkey has been asked: its signature count. */
const asked = async ({ cdp, authenticatorId }) =>
	(await cdp.send('WebAuthn.getCredentials', { authenticatorId })).credentials.reduce((n, c) => n + c.signCount, 0);
const account = (page) => page.locator(`${WIDGET} .account`);
/**
 * Every account record publish under way done, then the relay's signed time read
 * (`readRecordSigned`, which waits on them). A sign-in and an act each publish,
 * the act's not awaited: one still running when the clock moves signs then.
 */
const settled = (app) => app.page.evaluate(() => window.datum.sync.recordSigned());
const chainOf = async (app) => JSON.parse((await app.run(['sync', 'chain', 'list', '--json'])).stdout);

/** The passkeys the virtual authenticator holds. */
const credentials = async ({ cdp, authenticatorId }) => (await cdp.send('WebAuthn.getCredentials', { authenticatorId })).credentials;
/** Every value the page stores, by name: `localStorage`, `sessionStorage` and every IndexedDB store. */
const stored = (page) =>
	page.evaluate(async () => {
		const out = [...Object.entries(localStorage), ...Object.entries(sessionStorage)];
		const done = (r) => new Promise((ok, no) => ((r.onsuccess = () => ok(r.result)), (r.onerror = () => no(r.error))));
		for (const { name } of await indexedDB.databases()) {
			const db = await done(indexedDB.open(name));
			for (const s of db.objectStoreNames) out.push([`${name}/${s}`, JSON.stringify(await done(db.transaction(s).objectStore(s).getAll()))]);
			db.close();
		}
		return out;
	});
const PASSKEYS = 'datum.accountPasskeys';

/**
 * Make a passkey from the widget, which signs the tab in. Once the widget's grid has landed: it lands above
 * the account row, moving the button 108 px down, and Playwright checks a click's target on the press alone,
 * so a press before it and a release after it click nothing (da-843s9).
 */
async function newPasskey(page) {
	await page.locator(`${WIDGET} section.grid`).waitFor();
	await account(page).getByRole('button', { name: 'New passkey' }).click();
	await page.locator('dialog.ask input').fill('my phone');
	await page.locator('dialog.ask').getByRole('button', { name: 'make' }).click();
}

describe('signing in to sync with a passkey', () => {
	for (const [width, viewport] of [
		['desktop', AT_DATUM_DAY.viewport],
		['phone', { width: 390, height: 844 }]
	]) {
		test(`a known PRF output signs in to the key the CLI derives for it; signed out, the widget offers the passkey again (${width})`, async () => {
			await withWallClock(async (app) => {
				const { page } = app;
				await app.goto('/sync');
				assert.equal(await signIn(app, PRF), KEY);
				await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
				assert.equal(await page.locator(`${WIDGET} .signed-in`).getAttribute('title'), KEY);
				await account(page).getByRole('button', { name: 'Sign out' }).click();
				await account(page).getByRole('button', { name: 'Sign in with a passkey' }).waitFor();
				// Signed out, nothing is signed in for the CLI either.
				assert.notEqual((await app.run(['sync', 'account'])).code, 0);
			}, { ...AT_DATUM_DAY, viewport });
		});
	}

	test('a new passkey with PRF signs in, and a chain made then is the account’s: its key the admin, and the passkey is asked for each invite after (da-k04lr)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const passkey = await authenticator(page, { hasPrf: true });
			await app.goto('/sync');
			await newPasskey(page);
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			const key = await page.locator(`${WIDGET} .signed-in`).getAttribute('title');
			assert.match(key, /^ed25519:[0-9a-f]{64}$/);
			// Signed in, the tab holds nothing that signs: the run stops at the passkey.
			const held = await app.run(['sync', 'new-invite']);
			assert.notEqual(held.code, 0);
			assert.match(held.stderr, /https:\/\/datum\.day\/sync\/approve#/);
			assert.equal((await chainOf(app)).chain, null);
			// As the widget runs it, the passkey approves the chain and its first invite at once.
			let before = await asked(passkey);
			const made = await act(app, ['sync', 'new-invite']);
			assert.equal(made.code, 0, made.stderr);
			assert.equal(await asked(passkey), before + 1);
			// Its admins: the account, and the passkey it signed in with (da-wsx4k).
			const chain = await chainOf(app);
			assert.deepEqual(
				chain.admins.map((a) => [a.key.replace(/^(es256|eddsa):.*/, 'passkey'), a.label]),
				[
					[key, 'account'],
					['passkey', 'my phone']
				]
			);
			// Locked by the account, the next invite asks the passkey again.
			before = await asked(passkey);
			const again = await act(app, ['sync', 'new-invite']);
			assert.equal(again.code, 0, again.stderr);
			assert.equal(await asked(passkey), before + 1);
			assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
		}, { ...AT_DATUM_DAY, prepare: pkarrRelay().prepare });
	});

	test('the relay drops the account record 7 days after its signing: past 5 the widget says when, and its button re-signs it with one passkey tap, never unasked (da-6vmak)', async () => {
		const pkarr = pkarrRelay();
		const DAY = 86_400_000;
		await withApp(async (app) => {
			const { page } = app;
			const passkey = await authenticator(page, { hasPrf: true });
			await app.goto('/sync');
			await newPasskey(page);
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			// A chain made, the approval publishes the record, signed at the held clock.
			const signed = app.clock.at();
			assert.equal((await act(app, ['sync', 'new-invite'])).code, 0);
			await settled(app);
			assert.equal(pkarr.held.size, 1);
			const [first] = pkarr.held.values();
			assert.equal(Number(first.readBigUInt64BE(64) / 1000n), signed);
			const lapsing = page.locator(`${WIDGET} .account .lapsing`);
			const renew = account(page).getByRole('button', { name: 'Renew with passkey' });
			const before = await asked(passkey);

			// A day on, nothing is said.
			await app.clock.set(signed + DAY);
			await page.waitForTimeout(1500);
			assert.equal(await lapsing.count(), 0);
			// Six days on, the notice, and the passkey still not asked.
			await app.clock.set(signed + 6 * DAY);
			await lapsing.waitFor();
			assert.match(await lapsing.innerText(), /lapses in 1 day\b/);
			assert.equal(await asked(passkey), before);

			// The press asks it once, and the relay holds a record signed now.
			await renew.click();
			await until(() => Number([...pkarr.held.values()][0].readBigUInt64BE(64) / 1000n), (t) => t === signed + 6 * DAY);
			assert.equal(await asked(passkey), before + 1);
			await lapsing.waitFor({ state: 'detached' });
			assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
		}, { ...AT_DATUM_DAY, prepare: pkarr.prepare });
	});

	test('with no record on the relay (lapsed), the widget says so where a chain here is the account’s, and its button publishes one the next read finds; an account with no chain is told nothing (da-6vmak)', async () => {
		const pkarr = pkarrRelay();
		await withApp(async (app) => {
			const { page } = app;
			const passkey = await authenticator(page, { hasPrf: true });
			// The Sync widget's half-hourly read, now.
			const read = () => page.evaluate(() => window.datum.sync.recordSigned());
			await app.goto('/sync');
			await newPasskey(page);
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			const lapsing = page.locator(`${WIDGET} .account .lapsing`);
			// No chain of the account's: nothing to publish, and nothing said.
			await read();
			await page.waitForTimeout(1500);
			assert.equal(pkarr.held.size, 0);
			assert.equal(await lapsing.count(), 0);

			// A chain the account is admin of, its record then gone from the relay.
			assert.equal((await act(app, ['sync', 'new-invite'])).code, 0);
			await settled(app);
			assert.equal(pkarr.held.size, 1);
			pkarr.held.clear();
			const before = await asked(passkey);
			await read();
			await lapsing.waitFor();
			assert.match(await lapsing.innerText(), /isn’t on the relay/);
			assert.equal(await asked(passkey), before);

			// The press asks the passkey once and publishes it; the next read finds it.
			await account(page).getByRole('button', { name: 'Renew with passkey' }).click();
			await until(() => pkarr.held.size, (n) => n === 1);
			assert.equal(await asked(passkey), before + 1);
			await read();
			await lapsing.waitFor({ state: 'detached' });
			assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
		}, { ...AT_DATUM_DAY, prepare: pkarr.prepare });
	});

	test('signed in, a reload stays signed in and nothing stored signs; «Sign out» outlasts a reload too (da-wsx4k)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await app.goto('/sync');
			assert.equal(await signIn(app, PRF), KEY);
			await app.reload();
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			assert.equal(await page.locator(`${WIDGET} .signed-in`).getAttribute('title'), KEY);
			assert.equal(JSON.parse((await app.run(['sync', 'account'])).stdout).key, KEY);
			// Kept, it reads the account and signs nothing: the account's act stops at the passkey.
			const held = await app.run(['sync', 'new-invite']);
			assert.match(held.stderr, /https:\/\/datum\.day\/sync\/approve#/);
			const all = await stored(page);
			assert.ok(all.some(([, v]) => v.includes(KEY)), 'the account is stored nowhere: the search tests nothing');
			assert.deepEqual(
				all.filter(([, v]) => v.includes(PRF)).map(([k]) => k),
				[]
			);
			await account(page).getByRole('button', { name: 'Sign out' }).click();
			await app.reload();
			await page.locator(`${WIDGET} .account[data-signed="out"]`).waitFor();
			assert.notEqual((await app.run(['sync', 'account'])).code, 0);
		}, AT_DATUM_DAY);
	});

	test('signed in, «Add admin» makes no passkey: the chain’s admin is the one signed in with, and it approves the chain’s acts (da-wsx4k)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const passkey = await authenticator(page, { hasPrf: true });
			await app.goto('/sync');
			// An open chain, started signed out.
			assert.equal((await app.run(['sync', 'new-invite'])).code, 0);
			await newPasskey(page);
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			assert.equal((await credentials(passkey)).length, 1);
			const chain = page.locator(`${WIDGET} .chain[data-db="datum"]`);
			await chain.getByRole('button', { name: 'Add admin' }).click();
			await chain.locator('.admin').waitFor();
			assert.equal((await credentials(passkey)).length, 1);
			assert.equal(await page.locator('dialog.ask').count(), 0);
			const { admins } = await chainOf(app);
			assert.deepEqual(
				admins.map((a) => [a.key.replace(/^(es256|eddsa):.*/, 'passkey'), a.label]),
				[['passkey', 'my phone']]
			);
			// Locked by it, an act asks it once, and is done.
			const before = await asked(passkey);
			const made = await act(app, ['sync', 'new-invite']);
			assert.equal(made.code, 0, made.stderr);
			assert.equal(await asked(passkey), before + 1);
			assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
		}, { ...AT_DATUM_DAY, prepare: pkarrRelay().prepare });
	});

	test('a browser that did not make the sign-in passkey knows it among the chain’s admins by its sign-in’s signature, and offers no other (da-wsx4k)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const passkey = await authenticator(page, { hasPrf: true });
			await app.goto('/sync');
			await newPasskey(page);
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			assert.equal((await act(app, ['sync', 'new-invite'])).code, 0);
			await until(() => page.locator(`${WIDGET} .chain[data-db="datum"] .admin`).count(), (n) => n === 2);
			// As a device it was synced to: the passkey's public key never known here.
			const made = await page.evaluate((k) => localStorage.getItem(k), PASSKEYS);
			await page.evaluate((k) => localStorage.removeItem(k), PASSKEYS);
			await account(page).getByRole('button', { name: 'Sign out' }).click();
			await account(page).getByRole('button', { name: 'Sign in with a passkey' }).click();
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			assert.equal(await until(() => page.evaluate((k) => localStorage.getItem(k), PASSKEYS), (v) => v !== null), made);
			await until(() => account(page).locator('.make-one').count(), (n) => n === 0);
			assert.equal((await credentials(passkey)).length, 1);
			// After a reload nothing tells a passkey from before from this one: nothing is offered, and the key stays kept.
			await app.reload();
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			assert.equal(await account(page).locator('.make-one').count(), 0);
			assert.equal(await page.evaluate((k) => localStorage.getItem(k), PASSKEYS), made);
		}, { ...AT_DATUM_DAY, prepare: pkarrRelay().prepare });
	});

	test('a sign-in passkey from before is offered once to become one passkey for signing in and admin: the chain takes it and its account, the older one approving; and the new one approves alone (da-wsx4k)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const passkey = await authenticator(page, { hasPrf: true });
			await app.goto('/sync');
			await newPasskey(page);
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			const old = await page.locator(`${WIDGET} .signed-in`).getAttribute('title');
			// As before da-wsx4k: no passkey of the account known, and its chain locked by the account alone.
			await page.evaluate((k) => localStorage.removeItem(k), PASSKEYS);
			assert.equal((await act(app, ['sync', 'new-invite'])).code, 0);
			assert.deepEqual(
				(await chainOf(app)).admins.map((a) => a.key),
				[old]
			);
			await page.locator(`${WIDGET} .chain[data-db="datum"] .admin`).waitFor();
			const [{ credentialId: older }] = await credentials(passkey);
			const make = account(page).locator('.make-one');
			await make.click();
			await page.locator('dialog.ask input').fill('one passkey');
			await page.locator('dialog.ask').getByRole('button', { name: 'make' }).click();
			await account(page).locator('.one-said').waitFor();
			assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
			assert.equal((await credentials(passkey)).length, 2);
			const key = await page.locator(`${WIDGET} .signed-in`).getAttribute('title');
			assert.notEqual(key, old);
			assert.deepEqual(
				(await chainOf(app)).admins.map((a) => [a.key.replace(/^(es256|eddsa):.*/, 'passkey'), a.label]),
				[
					[old, 'account'],
					[key, 'account'],
					['passkey', 'one passkey']
				]
			);
			assert.equal(await make.count(), 0);
			// The older passkey gone from the password manager, the new one approves alone.
			await passkey.cdp.send('WebAuthn.removeCredential', { authenticatorId: passkey.authenticatorId, credentialId: older });
			const before = await asked(passkey);
			const again = await act(app, ['sync', 'new-invite']);
			assert.equal(again.code, 0, again.stderr);
			assert.equal(await asked(passkey), before + 1);
		}, { ...AT_DATUM_DAY, prepare: pkarrRelay().prepare });
	});

	test('«Not now» ends the offer of one passkey for that account: signing in again does not bring it back (da-wsx4k)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await authenticator(page, { hasPrf: true });
			await app.goto('/sync');
			await newPasskey(page);
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			await page.evaluate((k) => localStorage.removeItem(k), PASSKEYS);
			assert.equal((await act(app, ['sync', 'new-invite'])).code, 0);
			const make = account(page).locator('.make-one');
			await make.waitFor();
			await account(page).getByRole('button', { name: 'Not now' }).click();
			await make.waitFor({ state: 'detached' });
			await account(page).getByRole('button', { name: 'Sign out' }).click();
			await account(page).getByRole('button', { name: 'Sign in with a passkey' }).click();
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			await page.locator(`${WIDGET} .chain[data-db="datum"] .admin`).waitFor();
			assert.equal(await make.count(), 0);
		}, { ...AT_DATUM_DAY, prepare: pkarrRelay().prepare });
	});

	test('a chain the move to one passkey misses is said, with the older passkey kept for it (da-wsx4k)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const passkey = await authenticator(page, { hasPrf: true });
			const { cdp, authenticatorId } = passkey;
			await app.goto('/sync');
			await newPasskey(page);
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			const old = await page.locator(`${WIDGET} .signed-in`).getAttribute('title');
			await page.evaluate((k) => localStorage.removeItem(k), PASSKEYS);
			assert.equal((await act(app, ['sync', 'new-invite'])).code, 0);
			const chain = page.locator(`${WIDGET} .chain[data-db="datum"]`);
			await chain.locator('.admin').waitFor();
			// The older passkey away as the move asks it: the chain is not moved, and the widget says so.
			const [older] = await credentials(passkey);
			await cdp.send('WebAuthn.removeCredential', { authenticatorId, credentialId: older.credentialId });
			await account(page).locator('.make-one').click();
			await page.locator('dialog.ask input').fill('one passkey');
			await page.locator('dialog.ask').getByRole('button', { name: 'make' }).click();
			const said = account(page).locator('.one-said');
			await said.waitFor();
			assert.match(await said.innerText(), /datum did not take it/);
			assert.match(await said.innerText(), /keep the older passkeys/);
			assert.deepEqual(
				(await chainOf(app)).admins.map((a) => a.key),
				[old]
			);
			assert.notEqual(await page.locator(`${WIDGET} .signed-in`).getAttribute('title'), old);
		}, { ...AT_DATUM_DAY, prepare: pkarrRelay().prepare });
	});

	test('a passkey key stored for the account is no admin until an assertion proves it: another key written there is listed by no chain and refused by «Add passkey», the true one proven by the approval (da-wsx4k)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await authenticator(page, { hasPrf: true });
			await app.goto('/sync');
			await newPasskey(page);
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			const key = await page.locator(`${WIDGET} .signed-in`).getAttribute('title');
			const made = JSON.parse(await page.evaluate((k) => localStorage.getItem(k), PASSKEYS))[key];
			// Another public key written for the account, as anything on the origin could.
			const other = await page.evaluate(async () => {
				const pair = await crypto.subtle.generateKey({ name: 'ECDSA', namedCurve: 'P-256' }, true, ['sign', 'verify']);
				const spki = new Uint8Array(await crypto.subtle.exportKey('spki', pair.publicKey));
				const publicKey = btoa(String.fromCharCode(...spki)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
				return JSON.stringify({ alg: -7, publicKey, label: 'not mine' });
			});
			const write = (passkey) => page.evaluate(([k, a, p]) => localStorage.setItem(k, JSON.stringify({ [a]: p })), [PASSKEYS, key, passkey]);
			await write(other);
			const admins = async () => (await chainOf(app)).admins.map((a) => [a.key.replace(/^(es256|eddsa):.*/, 'passkey'), a.label]);
			// A chain started signed in lists the account alone.
			assert.equal((await act(app, ['sync', 'new-invite'])).code, 0);
			assert.deepEqual(await admins(), [[key, 'account']]);
			// «Add passkey» is approved by the true passkey, which is not the key stored: refused, nothing written.
			const chain = page.locator(`${WIDGET} .chain[data-db="datum"]`);
			await chain.locator('.admin').waitFor();
			await chain.getByRole('button', { name: 'Add passkey' }).click();
			await page.locator(`${WIDGET} .failure`).waitFor();
			assert.match(await page.locator(`${WIDGET} .failure`).innerText(), /not the one kept for your account/);
			assert.deepEqual(await admins(), [[key, 'account']]);
			// The true key, unproven after a reload, is proven by the approval and added.
			await write(made);
			await app.reload();
			await chain.locator('.admin').waitFor();
			await chain.getByRole('button', { name: 'Add passkey' }).click();
			await until(() => chain.locator('.admin').count(), (n) => n === 2);
			assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
			assert.deepEqual(await admins(), [
				[key, 'account'],
				['passkey', 'my phone']
			]);
		}, { ...AT_DATUM_DAY, prepare: pkarrRelay().prepare });
	});

	test('a passkey that gives no PRF is said plainly, and the tab stays signed out', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await authenticator(page);
			await app.goto('/sync');
			await newPasskey(page);
			const failure = page.locator(`${WIDGET} .failure`);
			await failure.waitFor();
			assert.match(await failure.innerText(), /PRF/);
			assert.match(await failure.innerText(), /invite/);
			assert.equal(await page.locator(`${WIDGET} .account[data-signed="out"]`).count(), 1);
		}, AT_DATUM_DAY);
	});

	test('the welcome signs in with the Sync widget’s sign-in: offered while signed out, gone once in, and the Sync widget then signed in to the same account (da-zkmjo)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const welcome = page.locator('.host.welcome');
			const button = welcome.getByRole('button', { name: 'Sign in with a passkey' });
			await button.waitFor();
			// The account's passkey, made in Sync, then signed out of.
			await authenticator(page, { hasPrf: true });
			await page.evaluate(() => window.datum.shell.openPlace('sync'));
			await newPasskey(page);
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			const key = await page.locator(`${WIDGET} .signed-in`).getAttribute('title');
			await account(page).getByRole('button', { name: 'Sign out' }).click();
			await page.evaluate(() => window.datum.shell.open('welcome'));
			await button.click();
			await button.waitFor({ state: 'detached' });
			// Signed in, the welcome offers no sign-in at all.
			assert.equal(await welcome.getByRole('button', { name: /sign in|passkey/i }).count(), 0);
			assert.equal(await welcome.locator('.failure').count(), 0);
			await page.evaluate(() => window.datum.shell.openPlace('sync'));
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			assert.equal(await page.locator(`${WIDGET} .signed-in`).getAttribute('title'), key);
		}, { ...AT_DATUM_DAY, home: null, prepare: pkarrRelay().prepare });
	});

	test('the welcome says what the Sync widget says of a passkey with no PRF, and stays signed out (da-zkmjo)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await authenticator(page);
			// No passkey here: the welcome's sign-in makes none, so the one it asks for is the widget's.
			await page.evaluate(() => window.datum.shell.openPlace('sync'));
			await newPasskey(page);
			const said = await page.locator(`${WIDGET} .failure`).innerText();
			await page.evaluate(() => window.datum.shell.open('welcome'));
			const welcome = page.locator('.host.welcome');
			await welcome.getByRole('button', { name: 'Sign in with a passkey' }).click();
			const failure = welcome.locator('.failure');
			await failure.waitFor();
			assert.equal(await failure.innerText(), said);
			assert.equal(await welcome.getByRole('button', { name: 'Sign in with a passkey' }).count(), 1);
		}, { ...AT_DATUM_DAY, home: null });
	});

	test('a second device signed in to the same account joins its chain with no invite, and syncs once the first is online', async () => {
		// Served as itself: a page at https://datum.day reaches no plain-http relay.
		const opts = { prepare: relayed(relay) };
		await withWallClock(
			(a) =>
				withWallClock(async (b) => {
					assert.equal(await signIn(a, PRF), KEY);
					assert.equal((await a.run(['occur', 'tea'])).code, 0);
					// The first invite starts the chain; b never sees it.
					assert.equal((await withPrf(a, ['sync', 'new-invite'])).code, 0);
					const { chain } = await chainOf(a);
					const endpoint = await until(() => a.page.evaluate(() => window.datum.sync.states().datum?.endpoint), Boolean);
					// Part 2's account record says where its members are; here, the test does.
					const member = JSON.stringify({ id: endpoint, addrs: [{ Relay: relay.url }] });

					// The first device paused: it is reached and refuses, and the join is kept.
					assert.equal((await a.run(['sync', 'pause'])).code, 0);
					assert.equal(await signIn(b, PRF), KEY);
					const kept = await b.page.evaluate(([c, m, p]) => window.datum.sync.accountJoin('datum', c, [m], p), [chain, member, PRF]);
					assert.equal(kept.pending, true, JSON.stringify(kept));
					assert.match(kept.said ?? '', /paused/, JSON.stringify(kept));
					assert.equal((await chainOf(b)).chain, null);

					// Online again, b's leader redeems it at its next pull: admitted, synced.
					assert.equal((await a.run(['sync', 'resume'])).code, 0);
					await until(() => countOf(b, 'tea'), (n) => n === 1, 75000);
					const joined = await chainOf(b);
					assert.equal(joined.chain, chain);
					assert.ok(!joined.pending_join, JSON.stringify(joined));
					assert.equal((await chainOf(a)).members.length, 2);
					await until(async () => (await verifyOf(a)) === (await verifyOf(b)), Boolean, 30000);
				}, opts),
			opts
		);
	});

	test('the account record: a device signed in publishes its chains, a second of the account reads it and joins each with no invite, and another account reads nothing of it', async () => {
		const pkarr = pkarrRelay();
		const opts = {
			prepare: async (context) => {
				await relayed(relay)(context);
				await pkarr.prepare(context);
			}
		};
		const record = (app, prf = PRF) => app.page.evaluate((p) => window.datum.sync.accountRecord(p), prf);
		await withWallClock(
			(a) =>
				withWallClock(async (b) => {
					assert.equal(await signIn(a, PRF), KEY);
					// Signed in with no chain of the account's, a device publishes nothing.
					await record(a);
					assert.equal(pkarr.held.size, 0);
					assert.equal((await a.run(['occur', 'tea'])).code, 0);
					assert.equal((await withPrf(a, ['sync', 'new-invite'])).code, 0);
					const { chain } = await chainOf(a);
					await until(() => a.page.evaluate(() => window.datum.sync.states().datum?.endpoint), Boolean);
					await record(a);
					assert.equal(pkarr.held.size, 1);
					const [body] = pkarr.held.values();

					// Another account's secret opens nothing of it, and finds no record of its own.
					await signIn(b, '08'.repeat(32));
					const other = await b.run(['sync', 'account', '--read', body.toString('hex')]);
					assert.notEqual(other.code, 0);
					assert.match(other.stderr, /not this account's record/);
					await record(b, '08'.repeat(32));
					assert.equal((await chainOf(b)).chain, null);

					// The same account's: it joins a's chain, which never invited it, and syncs.
					assert.equal(await signIn(b, PRF), KEY);
					await record(b);
					await until(() => countOf(b, 'tea'), (n) => n === 1, 75000);
					assert.equal((await chainOf(b)).chain, chain);
					await until(async () => (await verifyOf(a)) === (await verifyOf(b)), Boolean, 30000);
					// In the chain now, b publishes it too, and a reads nothing new to join.
					await record(b);
					const [again] = pkarr.held.values();
					assert.notDeepEqual(again, body);
					const read = JSON.parse((await a.run(['sync', 'account', '--read', again.toString('hex')])).stdout);
					assert.deepEqual(read.joins, []);
				}, opts),
			opts
		);
	});
});
