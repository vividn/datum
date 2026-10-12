// The Sync widget's actions (da-bjxts): turn sync on, add a device, lock a
// chain with a passkey, revoke with it, and approve a record from its link.
//
// A passkey is Chrome's virtual authenticator (CDP `WebAuthn`), and the app
// is served at https://datum.day, the one origin the CLI takes an assertion
// from (`chain::verify_assertion`). What a test asserts is what the CLI then
// holds — `sync chain list --json` — as well as what the widget draws.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync } from 'node:crypto';
import { startApp, stopApp } from './support/app.mjs';
import { nativeCopy, pairWith, relayed, startRelay, withWallClock } from './support/sync.mjs';
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

const WIDGET = 'section.pane[data-title="sync"]';
const DESKTOP = { width: 1280, height: 800 };
const AT_DATUM_DAY = { origin: 'https://datum.day', viewport: DESKTOP };

const endpointId = () =>
	generateKeyPairSync('ed25519').publicKey.export({ format: 'der', type: 'spki' }).subarray(-32).toString('hex');

/** `datum <argv>` in the page, whatever it exits with. */
const runIn = (app, argv, stdin = '') => app.page.evaluate(([a, s]) => window.datum.runQuiet(a, s), [argv, stdin]);

async function cli(app, argv, stdin) {
	const r = await runIn(app, argv, stdin);
	assert.equal(r.code, 0, `${argv.join(' ')}: ${r.stderr}`);
	return r.stdout;
}

/** A passkey the page can make and use, with the user verified. */
async function authenticator(page) {
	const cdp = await page.context().newCDPSession(page);
	await cdp.send('WebAuthn.enable');
	await cdp.send('WebAuthn.addVirtualAuthenticator', {
		options: {
			protocol: 'ctap2',
			transport: 'internal',
			hasResidentKey: true,
			hasUserVerification: true,
			isUserVerified: true,
			automaticPresenceSimulation: true
		}
	});
}

/** `home` in an open chain with a member `label` beside this copy, admitted as a session admits one; its endpoint. */
async function openChain(app, label = 'phone') {
	const { secret } = JSON.parse(await cli(app, ['--db', 'home', 'sync', 'new-invite', '--expires', '1d']));
	const phone = endpointId();
	await cli(app, ['--db', 'home', 'sync', 'handshake', phone, '--label', label, '--addr', '{}', '--invite', secret, '--chain', '']);
	return phone;
}

/** Open the widget's More. */
const more = (page) => page.locator(`${WIDGET} .actions`).getByRole('button', { name: /^More/ }).click();

/** Lock `home`'s chain with a new passkey, through the widget's More. */
async function addAdmin(page) {
	await more(page);
	await page.locator(`${WIDGET} .chain[data-db="home"]`).getByRole('button', { name: 'add admin' }).click();
	await page.locator('dialog.ask input').fill('my phone');
	await page.locator('dialog.ask').getByRole('button', { name: 'add' }).click();
	await page.locator(`${WIDGET} .chain[data-db="home"] .admin`).waitFor();
}

const members = async (app) => JSON.parse(await cli(app, ['--db', 'home', 'sync', 'chain', 'list', '--json']));
const revokedOf = async (app, id) => (await members(app)).members.find((m) => m.endpoint === id).revoked;

/** A passkey's public half as `sync admin add` takes it: a key this browser never made. */
const foreignPasskey = (label) =>
	JSON.stringify({
		alg: -7,
		publicKey: generateKeyPairSync('ec', { namedCurve: 'P-256' }).publicKey.export({ format: 'der', type: 'spki' }).toString('base64url'),
		label
	});

describe('the sync widget’s actions', () => {
	test('turn on sync, then add a device: the ticket as a QR and a command, which a native copy redeems', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await app.goto('/sync');
					const actions = page.locator(`${WIDGET} .actions[data-db="datum"]`);
					await actions.getByRole('button', { name: 'Turn on sync' }).click();
					const offer = page.locator(`${WIDGET} .offer[data-db="datum"]`);
					await offer.locator('.qr svg').waitFor();
					await offer.locator('summary', { hasText: 'connect from a terminal' }).click();
					const ticket = /datum sync chain connect '([^']+)'/.exec(await offer.locator('.ticket').innerText())[1];
					// This browser is the chain's first member, and the chain is open.
					const chain = JSON.parse(await cli(app, ['sync', 'chain', 'list', '--json']));
					assert.equal(chain.members.length, 1);
					assert.deepEqual(chain.admins, []);

					const joined = await native.run(['sync', 'chain', 'connect', ticket]);
					assert.equal(joined.code, 0, joined.stderr);
					await until(async () => (await page.locator(`${WIDGET} .devices li`).count()) === 2, Boolean);
					await offer.getByRole('button', { name: 'Done' }).click();
					assert.deepEqual(await actions.locator('button').allInnerTexts(), ['Add device', 'Rename this device', 'More ▸']);
					await more(page);
					assert.match(await page.locator(`${WIDGET} .chain[data-db="datum"]`).innerText(), /no admin: any device can add devices/);
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});

	test('a passkey locks the chain; a revoke is approved with it and the device moves to More as revoked', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await authenticator(page);
			const phone = await openChain(app);
			await app.goto('/sync?db=home');
			await addAdmin(page);
			const chain = await members(app);
			assert.equal(chain.admins.length, 1);
			assert.match(chain.admins[0].key, /^(es256|eddsa):/);
			assert.equal(chain.admins[0].label, 'my phone');

			// Locked: the revoke stops at an approve link, which the passkey signs.
			await page.locator(`${WIDGET} .more .device[data-client="${phone}"]`).getByRole('button', { name: 'Revoke' }).click();
			await page.locator('dialog.confirm').getByRole('button', { name: 'revoke' }).click();
			await page.locator(`${WIDGET} p.revoked`).waitFor();
			assert.equal(await page.locator(`${WIDGET} .devices li[data-client="${phone}"]`).count(), 0);
			assert.equal(await page.locator(`${WIDGET} .more .device[data-endpoint="${phone}"] .name`).innerText(), 'phone · revoked');
			assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
			assert.ok(await revokedOf(app, phone));
		}, AT_DATUM_DAY);
	});

	test('a refused action reads the CLI’s reason, without its «Error: » (da-s6o5z)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const phone = await openChain(app);
			await app.goto('/sync?db=home');
			await more(page);
			await page.locator(`${WIDGET} .more .device[data-client="${phone}"]`).getByRole('button', { name: 'Rename' }).click();
			await page.locator('dialog.ask input').fill('x'.repeat(65));
			await page.locator('dialog.ask').getByRole('button', { name: 'rename' }).click();
			const failure = page.locator(`${WIDGET} .failure`);
			await failure.waitFor();
			assert.equal(await failure.innerText(), 'a label is at most 64 characters, not 65');
		}, { viewport: DESKTOP });
	});

	test('a revert previews what goes and takes the device’s name typed, and leaves no unpurge (da-q4rau)', async () => {
		await withWallClock(
			async (app) => {
				const { page } = app;
				const phone = await openChain(app);
				const of = async () => (await members(app)).members.find((m) => m.endpoint === phone);
				await app.goto('/sync?db=home');
				await more(page);
				await page.locator(`${WIDGET} .more .device[data-client="${phone}"]`).getByRole('button', { name: 'Revert', exact: true }).click();
				const dialog = page.locator('dialog.confirm');
				assert.match(await dialog.innerText(), /Revert phone in home\? It made no change to an entry: this only revokes it\./);
				assert.match(await dialog.innerText(), /It is revoked too, until a fresh invite brings it and its changes back\./);
				// A wrong name is a no.
				await dialog.locator('input').fill('phon');
				await dialog.getByRole('button', { name: 'revert', exact: true }).click();
				assert.equal((await of()).purged, null);

				await page.locator(`${WIDGET} .more .device[data-client="${phone}"]`).getByRole('button', { name: 'Revert', exact: true }).click();
				await dialog.locator('input').fill('phone');
				await dialog.getByRole('button', { name: 'revert', exact: true }).click();
				const gone = page.locator(`${WIDGET} .more .device[data-endpoint="${phone}"]`);
				await gone.filter({ hasText: 'phone · reverted' }).waitFor();
				assert.equal(await gone.getByRole('button').count(), 0);
				// Listed once: its device line, with Rename and Revoke, is gone.
				assert.equal(await page.locator(`${WIDGET} .more .device[data-client="${phone}"]`).count(), 0);
				assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
				assert.ok((await of()).purged);
			},
			{ viewport: DESKTOP }
		);
	});

	test('leaving the chain asks first, then keeps every entry under a new key, and sync can be turned on afresh (da-j8c4l)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await openChain(app);
			await cli(app, ['--db', 'home', 'occur', 'tea']);
			const key = await cli(app, ['--db', 'home', 'sync', 'endpoint-key']);
			await app.goto('/sync?db=home');
			await more(page);
			const leave = page.locator(`${WIDGET} .more .actions`).getByRole('button', { name: 'Leave chain' });

			await leave.click();
			const dialog = page.locator('dialog.confirm');
			assert.match(await dialog.innerText(), /Every entry is kept, this device’s own and those it received/);
			await dialog.getByRole('button', { name: 'cancel' }).click();
			assert.equal((await members(app)).members.length, 2);

			await leave.click();
			await dialog.getByRole('button', { name: 'leave' }).click();
			// The phone never syncs, so no member holds the departure: it asks again, to leave telling no one.
			await dialog.getByRole('button', { name: 'leave anyway' }).waitFor();
			assert.match(await dialog.innerText(), /No device of the chain has this device’s departure yet/);
			assert.match(JSON.stringify(await members(app)), /"left":"/);
			await dialog.getByRole('button', { name: 'leave anyway' }).click();
			await page.locator(`${WIDGET} .actions[data-db="home"]`).getByRole('button', { name: 'Turn on sync' }).waitFor();
			assert.equal(JSON.parse(await cli(app, ['--db', 'home', 'sync', 'status', '--json'])).chain, null);
			assert.match(await cli(app, ['--db', 'home', 'tail']), /tea/);
			assert.notEqual(await cli(app, ['--db', 'home', 'sync', 'endpoint-key']), key);
			assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
		}, AT_DATUM_DAY);
	});

	test('removing the last admin asks first, and only a yes unlocks the chain', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await authenticator(page);
			await openChain(app);
			await app.goto('/sync?db=home');
			await addAdmin(page);
			const remove = page.locator(`${WIDGET} .chain[data-db="home"]`).getByRole('button', { name: 'remove my phone' });

			await remove.click();
			const dialog = page.locator('dialog.confirm');
			assert.match(await dialog.innerText(), /This unlocks the chain: any device can add, revoke or rename devices/);
			await dialog.getByRole('button', { name: 'cancel' }).click();
			assert.equal((await members(app)).admins.length, 1);

			await remove.click();
			await dialog.getByRole('button', { name: 'unlock' }).click();
			await page.getByText('no admin: any device can add devices').waitFor();
			assert.deepEqual((await members(app)).admins, []);
			// Every copy keeps the notice until it is acknowledged.
			const notice = page.locator(`${WIDGET} .notice[data-db="home"]`, { hasText: 'unlocked' });
			await notice.waitFor();
			await notice.getByRole('button', { name: 'ok' }).click();
			await until(async () => (await notice.count()) === 0, Boolean);
		}, AT_DATUM_DAY);
	});

	// The root bar's ⚠ (da-8fucg). vividn: «a little warning icon in the upper
	// bar next to the gear … If you click on it it displays a message box
	// saying that another device joined. ok to dismiss. tells user to go to
	// sync to manage». The Sync widget's own line was the only place a notice
	// showed, so this page has none.
	test('an unacknowledged notice is a ⚠ in the root bar with no Sync widget on the page: its dialog names the device and the Sync widget, OK acknowledges it, and the next device brings it back', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			const notices = async () => JSON.parse(await cli(app, ['--db', 'home', 'sync', 'status', '--json'])).notices;
			await openChain(app);
			await app.goto('/repl?db=home');
			assert.equal(await page.locator(WIDGET).count(), 0);
			const warn = page.locator('.root-bar').getByRole('button', { name: 'sync notices' });
			const dialog = page.locator('dialog.confirm');

			await warn.click();
			const said = await dialog.innerText();
			assert.match(said, /Another device joined \(«home»\): new sync endpoint phone /);
			assert.match(said, /To manage devices, open the Sync widget/);
			// Closed without an OK, nothing is acknowledged.
			await dialog.getByRole('button', { name: 'not now' }).click();
			assert.equal((await notices()).length, 1);

			await warn.click();
			await dialog.getByRole('button', { name: 'OK' }).click();
			await until(() => warn.count(), (n) => n === 0);
			assert.deepEqual(await notices(), []);

			// With the page open: no reload brings the next one.
			await openChain(app, 'tablet');
			await warn.click();
			assert.match(await dialog.innerText(), /new sync endpoint tablet /);
			assert.doesNotMatch(await dialog.innerText(), /phone/);
		}, { viewport: DESKTOP });
	});

	// A device another member let in reaches this copy as an op, in a session
	// with a peer it already knows: only what came in says the chain changed.
	test('a device another member admitted is a ⚠ here once a sync brings its admit', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await pairWith(app, native);
					const { secret } = JSON.parse((await native.run(['sync', 'new-invite', '--expires', '1d'])).stdout);
					const admit = ['sync', 'handshake', endpointId(), '--label', 'tablet', '--addr', '{}', '--invite', secret, '--chain', ''];
					assert.equal((await native.run(admit)).code, 0);
					native.spawn(['sync', 'run']);
					const said = () => page.evaluate(() => (window.datum.sync.states().datum?.notices ?? []).map((n) => n.label));
					await until(said, (labels) => labels.includes('tablet'));
					await page.locator('.root-bar').getByRole('button', { name: 'sync notices' }).click();
					assert.match(await page.locator('dialog.confirm').innerText(), /Another device joined \(«datum»\): new sync endpoint tablet /);
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});

	test('the approve link writes the record into a copy of its chain, with no code to paste; another chain’s shows the code', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await authenticator(page);
			const phone = await openChain(app);
			await app.goto('/sync?db=home');
			await addAdmin(page);
			// What a CLI in the chain prints when it wants a passkey.
			const asked = await runIn(app, ['--db', 'home', 'sync', 'chain', 'revoke', phone]);
			const link = /https:\/\/datum\.day\/sync\/approve#\S+/.exec(asked.stderr)[0];

			await app.goto(`/sync/approve#${link.split('#')[1]}`);
			const card = page.locator(`${WIDGET} .approve`);
			// The card names what the passkey signs: a link is anyone's to send.
			await card.locator('.what').waitFor();
			assert.equal(await card.locator('.what').innerText(), `Revoke phone (${phone.slice(0, 10)}) in home`);
			await card.getByRole('button', { name: 'approve with passkey' }).click();
			await page.locator(`${WIDGET} .approve .done, ${WIDGET} .failure`).first().waitFor();
			assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0, await page.locator(`${WIDGET}`).innerText());
			assert.match(await card.innerText(), /Approved in home\. .*nothing to paste/);
			assert.equal(await card.locator('.code').count(), 0);
			assert.ok(await revokedOf(app, phone));

			// The same act in a chain this browser holds no copy of.
			const record = JSON.parse(Buffer.from(link.split('#')[1], 'base64url').toString());
			record.chain = 'elsewhere';
			await card.getByRole('button', { name: 'dismiss' }).click();
			await app.goto(`/sync/approve#${Buffer.from(JSON.stringify(record)).toString('base64url')}`);
			assert.equal(
				await card.locator('.what').innerText(),
				`Revoke an unknown device (${phone.slice(0, 10)}) in a chain this browser holds no copy of`
			);
			await card.getByRole('button', { name: 'approve with passkey' }).click();
			await card.locator('.code').waitFor();
			assert.match(await card.locator('.code').innerText(), /^[A-Za-z0-9_-]{100,}$/);
		}, AT_DATUM_DAY);
	});

	test('a link adding an admin says whose key it is: its label, its fingerprint, and that this browser did not make it', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await authenticator(page);
			await openChain(app);
			await app.goto('/sync?db=home');
			await addAdmin(page);
			// «Please approve my new phone», carrying the sender's own key.
			const asked = await runIn(app, ['--db', 'home', 'sync', 'admin', 'add', foreignPasskey('my new phone')]);
			const link = /https:\/\/datum\.day\/sync\/approve#\S+/.exec(asked.stderr)[0];
			const { key } = JSON.parse(Buffer.from(link.split('#')[1], 'base64url').toString());

			await app.goto(`/sync/approve#${link.split('#')[1]}`);
			const what = page.locator(`${WIDGET} .approve .what`);
			await what.waitFor();
			assert.deepEqual(await what.locator('li').allInnerTexts(), [
				`Add admin «my new phone» (es256 ${key.slice(6, 18)}…) in home`,
				'Not a passkey made in this browser: approve only if you know whose it is.'
			]);
		}, AT_DATUM_DAY);
	});
});
