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
			const chain = await chainOf(app);
			assert.deepEqual(
				chain.admins.map((a) => a.key),
				[key]
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
