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
			const pressed = Date.now();
			await dialog.getByRole('button', { name: 'leave' }).click();
			// The phone never syncs, so no member holds the departure: it waits for a sync, then asks again, to leave telling no one.
			assert.equal(await page.locator(`${WIDGET} .waiting`).innerText(), 'Telling the chain’s other devices that this one is leaving…');
			await dialog.getByRole('button', { name: 'leave anyway' }).waitFor({ timeout: 30_000 });
			assert.match(await dialog.innerText(), /No device of the chain could be reached to receive this device’s departure/);
			// The phone has an address, so it is dialled: only the 20 s budget ends the wait.
			assert.ok(Date.now() - pressed >= 19_000, `asked again after ${Date.now() - pressed} ms`);
			assert.match(JSON.stringify(await members(app)), /"left":"/);
			await dialog.getByRole('button', { name: 'leave anyway' }).click();
			await page.locator(`${WIDGET} .actions[data-db="home"]`).getByRole('button', { name: 'Turn on sync' }).waitFor();
			assert.equal(JSON.parse(await cli(app, ['--db', 'home', 'sync', 'status', '--json'])).chain, null);
			assert.match(await cli(app, ['--db', 'home', 'tail']), /tea/);
			assert.notEqual(await cli(app, ['--db', 'home', 'sync', 'endpoint-key']), key);
			assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
		}, AT_DATUM_DAY);
	});

	// The refusal can go stale while its question is open: «leave anyway» syncs
	// and leaves again, forcing only if still unheard (da-48g3o).
	test('a member that comes up while «leave anyway» is asked gets the departure, and lists this copy LEFT', async () => {
		const native = await nativeCopy(relay);
		const left = async () => /"left":"/.test((await native.run(['sync', 'chain', 'list', '--json'])).stdout);
		try {
			// A native datum from before the departure record (da-dsob1) never lists anyone LEFT.
			const help = await native.run(['sync', 'chain', 'leave', '--help']);
			assert.match(help.stdout + help.stderr, /--force/, 'the native datum predates `chain leave`’s departure: build this tree’s');
			await withWallClock(
				async (app) => {
					const { page } = app;
					const serve = async () => {
						const serving = native.spawn(['sync', 'chain', 'invite']);
						return [serving, (await until(() => /datum sync chain connect '([^']+)'/.exec(serving.stdout), Boolean))[1]];
					};
					const [pairing, ticket] = await serve();
					await page.evaluate((t) => window.datum.sync.connect('datum', t), ticket);
					pairing.child.kill();
					await until(() => pairing.child.exitCode !== null || pairing.child.signalCode !== null, Boolean);
					await app.goto('/sync');
					await more(page);

					// The only other member is down: no one holds the departure.
					await page.locator(`${WIDGET} .more .actions`).getByRole('button', { name: 'Leave chain' }).click();
					const dialog = page.locator('dialog.confirm');
					await dialog.getByRole('button', { name: 'leave', exact: true }).click();
					await dialog.getByRole('button', { name: 'leave anyway' }).waitFor({ timeout: 30_000 });

					// It comes up, serving and dialing no one, while the question is open.
					await serve();
					assert.equal(await left(), false);
					await dialog.getByRole('button', { name: 'leave anyway' }).click();
					// First the dial begun while it was down, which a dead peer holds for the wire's 60 s idle timeout.
					await page.locator(`${WIDGET} .actions[data-db="datum"]`).getByRole('button', { name: 'Turn on sync' }).waitFor({ timeout: 90_000 });
					assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
					assert.equal(await left(), true);
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});

	test('one press leaves: the departure waits for this page’s sync to deliver it, then the leave completes (da-2o19r)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await pairWith(app, native);
					native.spawn(['sync', 'run']);
					const me = JSON.parse(await cli(app, ['sync', 'status', '--json'])).this.endpoint;
					await app.goto('/sync');
					await more(page);
					await page.locator(`${WIDGET} .more .actions`).getByRole('button', { name: 'Leave chain' }).click();
					const dialog = page.locator('dialog.confirm');
					await dialog.getByRole('button', { name: 'leave' }).click();
					await page.locator(`${WIDGET} .actions[data-db="datum"]`).getByRole('button', { name: 'Turn on sync' }).waitFor({ timeout: 30_000 });
					assert.equal(await dialog.count(), 0);
					assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
					// The native member holds the departure: it lists this copy as left.
					const { members } = JSON.parse((await native.run(['sync', 'chain', 'list', '--json'])).stdout);
					assert.ok(members.find((m) => m.endpoint === me).left);
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});

	test('db purge --leave with a member is one run: the departure waits for this page’s sync to deliver it, then the purge completes (da-a7kf7)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await pairWith(app, native);
					native.spawn(['sync', 'run']);
					const me = JSON.parse(await cli(app, ['sync', 'status', '--json'])).this.endpoint;
					const dialog = page.locator('dialog.confirm');
					const said = app.submit('db purge --leave');
					await dialog.locator('input').fill('datum');
					await dialog.locator('input').press('Enter');
					await said;
					assert.match((await app.outputs()).at(-1), /left sync chain \S+\n\s+its departure is with: /);
					assert.equal(await dialog.count(), 0);
					const { members } = JSON.parse((await native.run(['sync', 'chain', 'list', '--json'])).stdout);
					assert.ok(members.find((m) => m.endpoint === me).left);
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});

	test('db purge --leave with no member reachable waits, then offers to purge telling no one (da-a7kf7)', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await openChain(app);
			await cli(app, ['--db', 'home', 'occur', 'tea']);
			await app.goto('/?db=home');
			const dialog = page.locator('dialog.confirm');
			void app.submit('db purge --leave').catch(() => {});
			await dialog.locator('input').fill('home');
			await dialog.locator('input').press('Enter');
			// The phone never syncs: the purge waits for a sync, then asks again.
			await page.locator('.repl .progress', { hasText: 'Telling the chain’s other devices that this copy is leaving…' }).waitFor();
			const anyway = dialog.getByRole('button', { name: 'purge anyway' });
			await anyway.waitFor({ timeout: 30_000 });
			assert.match(await dialog.innerText(), /No device of sync chain \S+ could be reached to receive this copy’s departure\. Purge home now/);
			assert.equal((await runIn(app, ['--db', 'home', 'count', 'tea'])).stdout.trim(), '1');
			await anyway.click();
			await until(() => app.outputs(), (o) => /left sync chain/.test(o.at(-1) ?? ''), 90_000);
			assert.equal((await runIn(app, ['--db', 'home', 'count', 'tea'])).stdout.trim(), '0');
		}, { viewport: DESKTOP });
	});

	test('a member that comes up while «purge anyway» is asked gets the departure, and lists this copy LEFT (da-a7kf7, as da-48g3o)', async () => {
		const native = await nativeCopy(relay);
		const left = async () => /"left":"/.test((await native.run(['sync', 'chain', 'list', '--json'])).stdout);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					const serve = async () => {
						const serving = native.spawn(['sync', 'chain', 'invite']);
						return [serving, (await until(() => /datum sync chain connect '([^']+)'/.exec(serving.stdout), Boolean))[1]];
					};
					const [pairing, ticket] = await serve();
					await page.evaluate((t) => window.datum.sync.connect('datum', t), ticket);
					pairing.child.kill();
					await until(() => pairing.child.exitCode !== null || pairing.child.signalCode !== null, Boolean);

					// The only other member is down: no one holds the departure.
					const dialog = page.locator('dialog.confirm');
					// Its echo can come later than `submit` waits: the output is waited on instead.
					void app.submit('db purge --leave').catch(() => {});
					await dialog.locator('input').fill('datum');
					await dialog.locator('input').press('Enter');
					const anyway = dialog.getByRole('button', { name: 'purge anyway' });
					await anyway.waitFor({ timeout: 30_000 });

					// It comes up, serving and dialing no one, while the question is open.
					await serve();
					assert.equal(await left(), false);
					await anyway.click();
					// First the dial begun while it was down, which a dead peer holds for the wire's 60 s idle timeout.
					const out = await until(() => app.outputs(), (o) => /left sync chain/.test(o.at(-1) ?? ''), 90_000);
					assert.match(out.at(-1), /left sync chain \S+\n\s+its departure is with: /);
					assert.equal(await left(), true);
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
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
