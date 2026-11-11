// Signing in to sync with a passkey (da-nf1p4): its WebAuthn PRF output, held
// in the tab, from which the CLI derives the account key — an admin of every
// chain made signed in, and what signs the invite a device of the account
// redeems to join one with no other device's invite.
//
// A passkey is Chromium's virtual authenticator (CDP `WebAuthn`), with PRF
// (`hasPrf`) or without; the page is served as https://datum.day, the passkey's
// relying party. The two devices are two browser contexts on a local relay.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { createPublicKey, verify } from 'node:crypto';
import { startApp, stopApp } from './support/app.mjs';
import { countOf, relayed, startRelay, verifyOf, withWallClock } from './support/sync.mjs';
import { AT_DATUM_DAY, WIDGET, authenticator } from './support/syncactions.mjs';
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

/**
 * A pkarr relay (da-rnxp4), routed in the browser: a PUT is kept only if
 * it is what n0's relay would take (BEP44: signed by the key the path
 * names over `3:seqi<t>e1:v<n>:<packet>`, a packet of 1000 bytes at most),
 * and a GET answers it or 404. One for every context it prepares.
 */
const PKARR = 'https://pkarr.test/pkarr';
function pkarrRelay() {
	const held = new Map();
	const z32 = (s) => {
		let bits = '';
		for (const c of s) bits += 'ybndrfg8ejkmcpqxot1uwisza345h769'.indexOf(c).toString(2).padStart(5, '0');
		return Buffer.from(bits.slice(0, 256).match(/.{8}/g).map((b) => parseInt(b, 2)));
	};
	const cors = { 'access-control-allow-origin': '*', 'access-control-allow-methods': 'GET, PUT', 'access-control-allow-headers': '*' };
	const prepare = async (context) => {
		await context.addInitScript((url) => localStorage.setItem('datum.pkarrRelay', url), PKARR);
		await context.route(`${PKARR}/*`, async (route) => {
			const req = route.request();
			const name = new URL(req.url()).pathname.split('/').pop();
			if (req.method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors });
			if (req.method() === 'GET') return held.has(name) ? route.fulfill({ status: 200, headers: cors, body: held.get(name) }) : route.fulfill({ status: 404, headers: cors });
			const body = req.postDataBuffer();
			const key = createPublicKey({ key: { kty: 'OKP', crv: 'Ed25519', x: z32(name).toString('base64url') }, format: 'jwk' });
			const packet = body.subarray(72);
			const signed = Buffer.concat([Buffer.from(`3:seqi${body.readBigUInt64BE(64)}e1:v${packet.length}:`), packet]);
			const ok = packet.length <= 1000 && verify(null, signed, key, body.subarray(0, 64));
			if (ok) held.set(name, body);
			return route.fulfill({ status: ok ? 200 : 400, headers: cors });
		});
	};
	return { held, prepare };
}

const signIn = (app, secret) => app.page.evaluate((s) => window.datum.sync.signIn(undefined, s), secret);
const account = (page) => page.locator(`${WIDGET} .account`);
const chainOf = async (app) => JSON.parse((await app.run(['sync', 'chain', 'list', '--json'])).stdout);

/** Make a passkey from the widget, which signs the tab in. */
async function newPasskey(page) {
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

	test('a new passkey with PRF signs in, and a chain made then is the account’s: its key the admin, which signs the next invite', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await authenticator(page, { hasPrf: true });
			await app.goto('/sync');
			await newPasskey(page);
			await page.locator(`${WIDGET} .account[data-signed="in"]`).waitFor();
			const key = await page.locator(`${WIDGET} .signed-in`).getAttribute('title');
			assert.match(key, /^ed25519:[0-9a-f]{64}$/);
			assert.equal((await app.run(['sync', 'new-invite'])).code, 0);
			const chain = await chainOf(app);
			assert.deepEqual(
				chain.admins.map((a) => a.key),
				[key]
			);
			// Locked by the account, a second invite needs no passkey prompt: the account signs it.
			const again = await app.run(['sync', 'new-invite']);
			assert.equal(again.code, 0, again.stderr);
			assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
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

	test('a second device signed in to the same account joins its chain with no invite, and syncs once the first is online', async () => {
		// Served as itself: a page at https://datum.day reaches no plain-http relay.
		const opts = { prepare: relayed(relay) };
		await withWallClock(
			(a) =>
				withWallClock(async (b) => {
					assert.equal(await signIn(a, PRF), KEY);
					assert.equal((await a.run(['occur', 'tea'])).code, 0);
					// The first invite starts the chain; b never sees it.
					assert.equal((await a.run(['sync', 'new-invite'])).code, 0);
					const { chain } = await chainOf(a);
					const endpoint = await until(() => a.page.evaluate(() => window.datum.sync.states().datum?.endpoint), Boolean);
					// Part 2's account record says where its members are; here, the test does.
					const member = JSON.stringify({ id: endpoint, addrs: [{ Relay: relay.url }] });

					// The first device paused: it is reached and refuses, and the join is kept.
					assert.equal((await a.run(['sync', 'pause'])).code, 0);
					assert.equal(await signIn(b, PRF), KEY);
					const kept = await b.page.evaluate(([c, m]) => window.datum.sync.accountJoin('datum', c, [m]), [chain, member]);
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
		const record = (app) => app.page.evaluate(() => window.datum.sync.accountRecord());
		await withWallClock(
			(a) =>
				withWallClock(async (b) => {
					assert.equal(await signIn(a, PRF), KEY);
					// Signed in with no chain of the account's, a device publishes nothing.
					await record(a);
					assert.equal(pkarr.held.size, 0);
					assert.equal((await a.run(['occur', 'tea'])).code, 0);
					assert.equal((await a.run(['sync', 'new-invite'])).code, 0);
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
					await record(b);
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
