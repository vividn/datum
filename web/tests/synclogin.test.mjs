// `datum sync login` (da-mzqs4): the native CLI prints a datum.day link and
// waits on its device key; the page, signed in to the account, seals the
// account to that key with the link's nonce and sends it over the relay. The
// CLI keeps it, reads the account record and joins the account's chain.
//
// The page signs in with the known PRF output, as syncaccount.test's relay
// tests do: a page served as https://datum.day, a passkey's relying party,
// reaches no plain-http relay. Every copy here is real, the relay iroh's.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { startApp, stopApp } from './support/app.mjs';
import { countOf, nativeCopy, relayed, startRelay, verifyOf, withWallClock } from './support/sync.mjs';
import { WIDGET } from './support/syncactions.mjs';
import { pkarrRelay } from './support/pkarr.mjs';
import { until } from './support/until.mjs';

/** The PRF output whose account key `chain.rs`'s derivation test names. */
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

const signIn = (app) => app.page.evaluate((s) => window.datum.sync.signIn(undefined, s), PRF);
/** What sending `link`'s login says: `sent`, or the refusal. */
const send = (app, link) => app.page.evaluate((l) => window.datum.sync.login(l).then(() => 'sent', (e) => e.message), link);

/** `code`, a login link's fragment, with another nonce. */
function renonced(code) {
	const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
	let bits = '';
	for (const c of code.toUpperCase()) bits += alphabet.indexOf(c).toString(2).padStart(5, '0');
	const login = JSON.parse(Buffer.from(bits.match(/.{8}/g).map((b) => parseInt(b, 2))).toString());
	login.nonce = 'not-this-login';
	bits = [...Buffer.from(JSON.stringify(login))].map((b) => b.toString(2).padStart(8, '0')).join('');
	return (bits.match(/.{1,5}/g) ?? []).map((b) => alphabet[parseInt(b.padEnd(5, '0'), 2)]).join('');
}

describe('datum sync login', () => {
	test('a page signed in sends the account to the waiting CLI, which keeps it, joins the account’s chain and syncs; another nonce is refused; logout forgets it', async () => {
		const pkarr = pkarrRelay();
		const door = await pkarr.serve();
		const native = await nativeCopy(relay);
		const opts = {
			prepare: async (context) => {
				await relayed(relay)(context);
				await pkarr.prepare(context);
			}
		};
		try {
			await withWallClock(async (app) => {
				const { page } = app;
				// The account's chain, made on the page and published in its record.
				assert.equal(await signIn(app), KEY);
				assert.equal((await app.run(['occur', 'tea'])).code, 0);
				assert.equal((await app.run(['sync', 'new-invite'])).code, 0);
				await until(() => page.evaluate(() => window.datum.sync.states().datum?.endpoint), Boolean);
				await page.evaluate(() => window.datum.sync.accountRecord());
				assert.equal(pkarr.held.size, 1);

				const login = native.spawn(['sync', 'login'], { DATUM_SYNC_PKARR: door });
				const [, code] = await until(() => /https:\/\/datum\.day\/sync\/login#([A-Za-z2-7]+)/.exec(login.stdout), Boolean, 30000);

				// A link with another nonce reaches the CLI and is refused, and the CLI waits on.
				assert.match(await send(app, `https://datum.day/sync/login#${renonced(code)}`), /refused the sign-in: for another login/);
				assert.equal(login.child.exitCode, null);
				assert.equal(existsSync(join(native.home, 'account')), false);

				// The link opened: the Sync widget asks, and once asked, signs the CLI in.
				await app.goto(`/sync/login#${code}`);
				assert.equal(await signIn(app), KEY);
				const card = page.locator(`${WIDGET} .card.login`);
				// It says when to go on, and when not: the link is the account to whoever made it.
				const words = await card.locator('.only').innerText();
				assert.match(words, /Only if you ran datum sync login yourself, just now/);
				assert.match(words, /If someone sent you this link, close it\./);
				// The code it shows is the one the CLI printed.
				const [, shown] = /The page shows the code ([0-9a-f]{6})/.exec(login.stdout);
				assert.equal(await card.locator('.code').innerText(), shown);
				await card.getByRole('button', { name: 'Sign in the CLI' }).click();
				await page.locator(`${WIDGET} .card.login .done`).waitFor();
				const exit = await new Promise((resolve) => (login.child.exitCode === null ? login.child.on('exit', resolve) : resolve(login.child.exitCode)));
				assert.equal(exit, 0, login.stderr);
				assert.match(login.stdout, new RegExp(`signed in to account ${KEY}`));
				assert.match(login.stdout, /joined «datum»/);

				// Signed in, the CLI holds the chain's entry and agrees with the page.
				assert.equal(await native.count('tea'), 1);
				await until(async () => (await native.verify()) === (await verifyOf(app)), Boolean, 30000);
				const status = JSON.parse((await native.run(['sync', 'status', '--json'])).stdout);
				assert.equal(status.this.account, KEY);
				assert.equal(existsSync(join(native.home, 'account')), true);
				assert.equal(status.chain, JSON.parse((await app.run(['sync', 'chain', 'list', '--json'])).stdout).chain);

				// Logged out, it is signed in to nothing.
				assert.match((await native.run(['sync', 'logout'])).stdout, /signed out/);
				assert.equal(JSON.parse((await native.run(['sync', 'status', '--json'])).stdout).this.account, null);
				assert.notEqual((await native.run(['sync', 'account'])).code, 0);
				assert.equal(await countOf(app, 'tea'), 1);
			}, opts);
		} finally {
			await native.close();
			await pkarr.stop();
		}
	});
});
