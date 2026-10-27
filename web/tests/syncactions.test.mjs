// The Sync widget's actions (da-bjxts): turn sync on, add a device, lock a
// chain with a passkey, expel with it, and approve a record from its link.
//
// A passkey is Chrome's virtual authenticator (CDP `WebAuthn`), and the app
// is served at https://datum.day, the one origin the CLI takes an assertion
// from (`chain::verify_assertion`). What a test asserts is what the CLI then
// holds — `sync chain list --json` — as well as what the widget draws.
//
// Budget (da-lt0k): on node 22 a FILE was the test `run()` timed
// (da-l5k9e: CI now runs node 24), so 300 s was this file's whole budget — `before(startApp)`, the
// Chromium it launches, and every test in it. `syncactions`, `-leave` and
// `-purge` were one file, its six leave and purge tests waiting out real
// 20 s budgets and idle timeouts: 189 s alone, 243 s in a whole run, and cut
// at 300 s on CI (da-xwjul), before `withApp` shortened them (da-eqjri). A
// cancelled file hides every other file's failures behind it. A group that
// costs a minute wants a file of its own, not another `describe` in one of
// these. What more than one of them needs is `./support/syncactions.mjs`.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync } from 'node:crypto';
import { startApp, stopApp } from './support/app.mjs';
import { nativeCopy, pairWith, relayed, startRelay, withWallClock } from './support/sync.mjs';
import { AT_DATUM_DAY, DESKTOP, WIDGET, addAdmin, authenticator, cli, endpointId, members, more, openChain, runIn } from './support/syncactions.mjs';
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

	test('`sync chain invite` typed in the bar opens the widget with the invite under way, which a native copy redeems; a flagged one runs as typed', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					const { page } = app;
					await app.goto('/');
					assert.equal(await page.locator(WIDGET).count(), 0, 'the sync widget is not on the default page');
					// A flag the widget has no word for is the CLI's, which has no network.
					await app.submit('sync chain invite --reader');
					assert.match((await app.outputs()).at(-1), /not in this build/);
					assert.equal(await page.locator(WIDGET).count(), 0);

					await app.submit('sync chain invite');
					assert.match((await app.stdouts()).at(-1), /the invite is in the sync widget/);
					const offer = page.locator(`${WIDGET} .offer[data-db="datum"]`);
					await offer.locator('.qr svg').waitFor();
					await offer.locator('summary', { hasText: 'connect from a terminal' }).click();
					const ticket = /datum sync chain connect '([^']+)'/.exec(await offer.locator('.ticket').innerText())[1];
					const joined = await native.run(['sync', 'chain', 'connect', ticket]);
					assert.equal(joined.code, 0, joined.stderr);
					await until(async () => (await page.locator(`${WIDGET} .devices li`).count()) === 2, Boolean);
				},
				{ prepare: relayed(relay), viewport: DESKTOP }
			);
		} finally {
			await native.close();
		}
	});

	test('a passkey locks the chain; an expel is approved with it and the device moves to More as expelled', async () => {
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

			// Locked: the expel stops at an approve link, which the passkey signs.
			await page.locator(`${WIDGET} .more .device[data-client="${phone}"]`).getByRole('button', { name: 'Expel' }).click();
			const asked = page.locator('dialog.confirm');
			assert.match(await asked.innerText(), /Expel phone from home\? It can no longer sync home until a new invite admits it again\. It keeps its own copy, and what it wrote stays on every device\./);
			await asked.getByRole('button', { name: 'expel' }).click();
			await page.locator(`${WIDGET} p.revoked`).waitFor();
			assert.equal(await page.locator(`${WIDGET} .devices li[data-client="${phone}"]`).count(), 0);
			assert.equal(await page.locator(`${WIDGET} .more .device[data-endpoint="${phone}"] .name`).innerText(), 'phone · expelled');
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

	test('an unjoin previews what goes and takes the device’s name typed, and leaves no unpurge (da-q4rau)', async () => {
		await withWallClock(
			async (app) => {
				const { page } = app;
				const phone = await openChain(app);
				const of = async () => (await members(app)).members.find((m) => m.endpoint === phone);
				await app.goto('/sync?db=home');
				await more(page);
				await page.locator(`${WIDGET} .more .device[data-client="${phone}"]`).getByRole('button', { name: 'Unjoin', exact: true }).click();
				const dialog = page.locator('dialog.confirm');
				assert.match(await dialog.innerText(), /Unjoin phone from home\? It made no change to an entry: this only expels it\./);
				assert.match(await dialog.innerText(), /It is expelled too: it syncs no more, though it keeps its own copy, until a fresh invite brings it and its changes back\./);
				// A wrong name is a no.
				await dialog.locator('input').fill('phon');
				await dialog.getByRole('button', { name: 'unjoin', exact: true }).click();
				assert.equal((await of()).purged, null);

				await page.locator(`${WIDGET} .more .device[data-client="${phone}"]`).getByRole('button', { name: 'Unjoin', exact: true }).click();
				await dialog.locator('input').fill('phone');
				await dialog.getByRole('button', { name: 'unjoin', exact: true }).click();
				const gone = page.locator(`${WIDGET} .more .device[data-endpoint="${phone}"]`);
				await gone.filter({ hasText: 'phone · unjoined' }).waitFor();
				assert.equal(await gone.getByRole('button').count(), 0);
				// Listed once: its device line, with Rename and Expel, is gone.
				assert.equal(await page.locator(`${WIDGET} .more .device[data-client="${phone}"]`).count(), 0);
				assert.equal(await page.locator(`${WIDGET} .failure`).count(), 0);
				assert.ok((await of()).purged);
			},
			{ viewport: DESKTOP }
		);
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
			assert.match(await dialog.innerText(), /This unlocks the chain: any device can add, expel or rename devices/);
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
	test('an unacknowledged notice is a ⚠ in the root bar with no Sync widget on the page: its dialog names the device and the Sync widget, OK acknowledges it, the next device brings it back, and its revoke revokes it', async () => {
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
			// By label, on this page's clock, with an expel one step away (da-o9xdd).
			assert.match(said, /«home»: Another device joined: phone, at \d\d:\d\d$/m);
			assert.doesNotMatch(said, /via|Z\b|[0-9a-f]{10}/);
			assert.match(said, /Not yours\? Expel it/);
			assert.match(said, /To manage devices, open the Sync widget/);
			assert.equal(await warn.innerText(), '⚠');
			await dialog.getByRole('button', { name: 'expel phone' }).waitFor();
			// Closed without an OK, nothing is acknowledged.
			await dialog.getByRole('button', { name: 'not now' }).click();
			assert.equal((await notices()).length, 1);

			await warn.click();
			await dialog.getByRole('button', { name: 'OK', exact: true }).click();
			await until(() => warn.count(), (n) => n === 0);
			assert.deepEqual(await notices(), []);

			// With the page open: no reload brings the next one.
			await openChain(app, 'tablet');
			await warn.click();
			assert.match(await dialog.innerText(), /Another device joined: tablet, at /);
			assert.doesNotMatch(await dialog.innerText(), /phone/);
			// Not this reader's: its expel expels it, and acknowledges what was shown.
			await dialog.getByRole('button', { name: 'expel tablet' }).click();
			await until(() => warn.count(), (n) => n === 0);
			assert.deepEqual(await notices(), []);
			const tablet = (await members(app)).members.find((m) => m.label === 'tablet');
			assert.ok(tablet?.revoked, JSON.stringify(tablet));

			// Two devices one label: each button names its own by its id too.
			const pads = [await openChain(app, 'pad'), await openChain(app, 'pad')];
			await until(() => page.evaluate(() => window.datum.sync.states().home?.notices?.length), (n) => n === 2);
			await warn.click();
			for (const pad of pads) await dialog.getByRole('button', { name: `expel pad (${pad.slice(0, 8)})`, exact: true }).waitFor();
			await dialog.getByRole('button', { name: 'not now' }).click();
		}, { viewport: DESKTOP });
	});

	// The joiner's side (da-o9xdd): the members it found in the chain it
	// joined were there before it, so they are no news of a device joining.
	// vremya: «on the joiner, one plain line in place of a ⚠ per member».
	test('a database that joins a chain is told its members once, plainly, under an ⓘ and not a ⚠ per member', async () => {
		await withWallClock(async (app) => {
			const { page } = app;
			await openChain(app);
			const { secret } = JSON.parse(await cli(app, ['--db', 'home', 'sync', 'new-invite', '--expires', '1d']));
			const away = (await cli(app, ['--db', 'away', 'sync', 'endpoint-key'])).trim();
			await cli(app, ['--db', 'home', 'sync', 'handshake', away, '--label', 'away', '--addr', '{}', '--invite', secret, '--chain', '']);
			const vector = (await cli(app, ['--db', 'away', 'sync', 'pipe', 'vector'])).trim();
			await cli(app, ['--db', 'away', 'sync', 'pipe', 'apply'], await cli(app, ['--db', 'home', 'sync', 'pipe', 'ops', '--since', vector]));
			const notices = async () => JSON.parse(await cli(app, ['--db', 'away', 'sync', 'status', '--json'])).notices;
			assert.deepEqual((await notices()).map((n) => [n.kind, n.who]), [['member', 'joined']]);

			await app.goto('/repl?db=away');
			const told = page.locator('.root-bar').getByRole('button', { name: 'sync notices' });
			assert.equal(await told.innerText(), 'ⓘ');
			await told.click();
			const dialog = page.locator('dialog.confirm');
			const said = await dialog.innerText();
			assert.match(said, /«away»: You joined this sync chain; its members: .*phone \(since \d\d:\d\d\)/);
			assert.doesNotMatch(said, /Another device joined|Expel/);
			await dialog.getByRole('button', { name: 'OK' }).click();
			await until(() => told.count(), (n) => n === 0);
			assert.deepEqual(await notices(), []);
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
					assert.match(await page.locator('dialog.confirm').innerText(), /«datum»: Another device joined: tablet, at /);
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
			const asked = await runIn(app, ['--db', 'home', 'sync', 'chain', 'expel', phone]);
			const link = /https:\/\/datum\.day\/sync\/approve#\S+/.exec(asked.stderr)[0];

			await app.goto(`/sync/approve#${link.split('#')[1]}`);
			const card = page.locator(`${WIDGET} .approve`);
			// The card names what the passkey signs: a link is anyone's to send.
			await card.locator('.what').waitFor();
			assert.equal(await card.locator('.what').innerText(), `Expel phone (${phone.slice(0, 10)}) in home: it syncs no more, keeping its copy; what it wrote stays`);
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
				`Expel an unknown device (${phone.slice(0, 10)}) in a chain this browser holds no copy of: it syncs no more, keeping its copy; what it wrote stays`
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
