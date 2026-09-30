// datum.day syncs (da-6kpj5): the page's endpoint, through a local relay.
//
// Every copy here is real: the browser's is the wire crate in the sync
// worker with the CLI as its store, a native one is `datum` itself, and the
// relay is iroh's. What a test asserts is what the copies HOLD — their
// `sync pipe verify` hashes and their entries — never what the page says it did.
// The several-tabs cases are sync-tabs.test.mjs.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { countOf, leadsOf, nativeCopy, pairWith, relayed, startRelay, verifyOf } from './support/sync.mjs';
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

/** The endpoint `app`'s tab says its database's sync runs on. */
const endpointOf = (app) => app.page.evaluate(() => window.datum.sync.states().datum?.endpoint);

describe('datum.day and a native copy', () => {
	test('pair through the relay and converge', async () => {
		const native = await nativeCopy(relay);
		try {
			assert.equal((await native.run(['occur', 'tea', '-t', '0800'])).code, 0);
			await withApp(
				async (app) => {
					await app.run(['occur', 'coffee', '-t', '0900']);
					const report = await pairWith(app, native);
					assert.equal(report.verified, true, JSON.stringify(report));
					assert.equal(await native.verify(), await verifyOf(app));
					assert.equal(await countOf(app, 'tea'), 1);
					assert.equal(await native.count('coffee'), 1);
				},
				{ prepare: relayed(relay) }
			);
		} finally {
			await native.close();
		}
	});

	test('a page holding entries is refused a reader invite, by count, and joins nothing (da-tti3w)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withApp(
				async (app) => {
					await app.run(['occur', 'coffee', '-t', '0900']);
					await app.run(['occur', 'coffee', '-t', '1000']);
					const serving = native.spawn(['sync', 'chain', 'invite', '--reader']);
					const said = await until(() => /datum sync chain connect (\S+)/.exec(serving.stdout), Boolean);
					const refused = await app.page.evaluate(
						(t) => window.datum.sync.connect('datum', t).then(
							() => 'joined',
							(e) => String(e?.message ?? e)
						),
						said[1]
					);
					assert.match(refused, /holds 2 entries of its own; a reader writes nothing/);
					assert.equal(JSON.parse((await app.run(['sync', 'status', '--json'])).stdout).chain, null);
					assert.equal(await countOf(app, 'coffee'), 2);
				},
				{ prepare: relayed(relay) }
			);
		} finally {
			await native.close();
		}
	});

	test('closing the tab stops its endpoint; reopened, it resumes and takes what was written meanwhile', async () => {
		const native = await nativeCopy(relay);
		try {
			await withApp(
				async (app) => {
					await pairWith(app, native);
					const before = await endpointOf(app);
					await app.page.close();
					// Nobody answers for the page's key now: a dial fails. (Before
					// `sync run`, which would be a second endpoint on the native key.)
					const now = await native.run(['sync'], '', { DATUM_SYNC_TIMEOUT_S: '3' });
					assert.notEqual(now.code, 0, now.stdout);
					assert.match(now.stdout, /could not reach/);

					assert.equal((await native.run(['occur', 'meanwhile', '-t', '1000'])).code, 0);
					native.spawn(['sync', 'run']);
					const back = await app.newTab();
					await until(() => leadsOf(back), (l) => l.includes('datum'));
					// The same copy, so the same key: a CLI process of its own does not mint one.
					assert.equal(await until(() => endpointOf(back), Boolean), before);
					await until(() => countOf(back, 'meanwhile'), (n) => n === 1);
					// And its own writes go out again.
					await back.run(['occur', 'reopened', '-t', '1100']);
					await until(() => native.count('reopened'), (n) => n === 1);
					await until(async () => [await native.verify(), await verifyOf(back)], ([a, b]) => a === b);
				},
				{ prepare: relayed(relay) }
			);
		} finally {
			await native.close();
		}
	});
});

describe('a sync chain started in the page', () => {
	test('holds this browser as its first member, under its own client, and the tab starts an endpoint', async () => {
		await withApp(
			async (app) => {
				// The first invite starts an open chain: no admin, no key.
				const invite = await app.run(['sync', 'new-invite']);
				assert.equal(invite.code, 0, invite.stderr);
				assert.deepEqual(JSON.parse((await app.run(['sync', 'chain', 'list', '--json'])).stdout).admins, []);
				const client = await app.page.evaluate(() => localStorage.getItem('datum-client-id'));
				const members = JSON.parse((await app.run(['sync', 'chain', 'list', '--json'])).stdout).members;
				assert.deepEqual(
					members.map((m) => [m.client, m.kind, m.this]),
					[[client, 'web', true]]
				);
				assert.equal((await app.run(['sync', 'chain', 'rename', 'kitchen'])).code, 0);
				// A `sync` command that wrote: the leader reads the setup again.
				assert.match(await until(() => endpointOf(app), Boolean), /^[0-9a-f]{64}$/);
			},
			{ prepare: relayed(relay) }
		);
	});
});

describe('two browsers', () => {
	test('pair through the relay, converge, and a write in one appears in the other untouched', async () => {
		await withApp(
			async (a) => {
				await withApp(
					async (b) => {
						await a.run(['occur', 'left', '-t', '0800']);
						await b.run(['occur', 'right', '-t', '0900']);
						const ticket = await a.page.evaluate(() => window.datum.sync.offer('datum'));
						const report = await b.page.evaluate((t) => window.datum.sync.connect('datum', t), ticket);
						assert.equal(report.verified, true, JSON.stringify(report));
						assert.equal(await verifyOf(a), await verifyOf(b));
						assert.equal(await countOf(b, 'left'), 1);
						assert.equal(await countOf(a, 'right'), 1);

						// Nothing is done in `b`: `a` pushes, and `b`'s page shows it.
						await a.run(['occur', 'later', '-t', '1000']);
						await until(() => b.paneText('tail'), (text) => text.includes('later'));
						assert.equal(await verifyOf(a), await verifyOf(b));
					},
					{ prepare: relayed(relay) }
				);
			},
			{ prepare: relayed(relay) }
		);
	});
});
