// datum.day syncs (da-6kpj5): the page's endpoint, through a local relay.
//
// Every copy here is real: the browser's is the wire crate in the sync
// worker with the CLI as its store, a native one is `datum` itself, and the
// relay is iroh's. What a test asserts is what the copies HOLD — their
// `sync pipe verify` hashes and their entries — never what the page says it did.
// The native-copy cases are sync-native.test.mjs; the several-tabs cases
// are sync-tabs.test.mjs and sync-tabs-leader.test.mjs.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { countOf, nativeCopy, relayed, startRelay, verifyOf, withWallClock } from './support/sync.mjs';
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

describe('a sync chain started in the page', () => {
	test('holds this browser as its first member, under its own client, and the tab starts an endpoint', async () => {
		await withWallClock(
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

describe("a browser's device key (da-n0uj1.2)", () => {
	test('binds each database in the profile to one device, kept across a reload', async () => {
		await withWallClock(
			async (app) => {
				const deviceOf = async (db) => {
					const list = await app.run(['--db', db, 'sync', 'chain', 'list', '--json']);
					assert.equal(list.code, 0, list.stderr);
					return JSON.parse(list.stdout).members[0].device;
				};
				for (const db of ['datum', 'other']) {
					const invite = await app.run(['--db', db, 'sync', 'new-invite']);
					assert.equal(invite.code, 0, invite.stderr);
				}
				const device = await deviceOf('datum');
				assert.match(device, /^[0-9a-f]{64}$/);
				assert.equal(await deviceOf('other'), device);
				await app.reload();
				const status = JSON.parse((await app.run(['sync', 'status', '--json'])).stdout);
				assert.equal(status.this.device, device);
			},
			{ prepare: relayed(relay) }
		);
	});
});

/** Two browser profiles, `a` and `b`, each syncing through the relay. */
const twoBrowsers = (body) =>
	withWallClock((a) => withWallClock((b) => body(a, b), { prepare: relayed(relay) }), { prepare: relayed(relay) });

/** A join link for a new invite on `a`'s `datum` (da-l9kdr), as its Sync widget shows one. */
const linkFrom = (a) => a.page.evaluate(async () => `${location.origin}/sync/join#${await window.datum.sync.offer('datum')}`);

/** `left` written on `a` and `right` on `b` are on both, the copies agree, and a write on `a` after reaches `b` by itself. */
async function converged(a, b) {
	assert.equal(await verifyOf(a), await verifyOf(b));
	assert.equal(await countOf(b, 'left'), 1);
	assert.equal(await countOf(a, 'right'), 1);
	await a.run(['occur', 'later', '-t', '1000']);
	await until(() => countOf(b, 'later'), (n) => n === 1);
}

describe('a browser joins a chain (da-l9kdr)', () => {
	test('through the join link: the card names the inviter, its chain and the database, and joins on a tap', async () => {
		await twoBrowsers(async (a, b) => {
			await a.run(['occur', 'left', '-t', '0800']);
			await b.run(['occur', 'right', '-t', '0900']);
			const link = await linkFrom(a);
			const chain = JSON.parse((await a.run(['sync', 'status', '--json'])).stdout).chain;
			await b.goto(link);
			// The ticket left the address bar with the `goto` to `/`.
			assert.equal(new URL(b.page.url()).hash, '');
			const card = b.page.locator('.card.join');
			const join = card.getByRole('button', { name: 'Join', exact: true });
			await join.waitFor();
			// What it names: the inviter, its chain, this database, and when the link expires.
			const words = await card.locator('.what').innerText();
			for (const fact of ['«datum.day»', chain.slice(0, 10), '«datum»']) assert.ok(words.includes(fact), `${fact}: ${words}`);
			assert.match(words, /\b\d\d:\d\d\b/);
			await join.click();
			await card.locator('.done').waitFor();
			await converged(a, b);

			// Opened again, the link offers to join again, as a revoked device rejoins (da-j8c4l).
			await b.goto(link);
			// It says one thing more, of «datum»: that it is in this chain already.
			const lines = card.locator('.what li');
			await until(() => lines.count(), (n) => n === 3);
			assert.ok((await lines.nth(1).innerText()).includes('«datum»'));
			assert.equal(await join.count(), 1);
		});
	});

	test('in a chain of its own, it joins: its chain is replaced, its entries come along (da-h7pnc)', async () => {
		await twoBrowsers(async (a, b) => {
			await a.run(['occur', 'left', '-t', '0800']);
			await b.run(['occur', 'right', '-t', '0900']);
			assert.equal((await b.run(['sync', 'new-invite'])).code, 0);
			const link = await linkFrom(a);
			const chain = JSON.parse((await a.run(['sync', 'status', '--json'])).stdout).chain;
			await b.goto(link);
			const card = b.page.locator('.card.join');
			const join = card.getByRole('button', { name: 'Join', exact: true });
			await join.waitFor();
			// It says one thing more, of «datum»: that its own chain is replaced.
			const lines = card.locator('.what li');
			assert.equal(await lines.count(), 3);
			assert.ok((await lines.nth(1).innerText()).includes('«datum»'));
			await join.click();
			await card.locator('.done').waitFor();
			await converged(a, b);
			assert.equal(JSON.parse((await b.run(['sync', 'status', '--json'])).stdout).chain, chain);

			// In a chain of two now, an invite to that chain admits it again, as a revoked device rejoins (da-j8c4l).
			const ticket = (await linkFrom(a)).split('#')[1];
			const again = await b.page.evaluate(
				(t) => window.datum.sync.connect('datum', t).then(() => 'joined', (e) => e.message),
				ticket
			);
			assert.equal(again, 'joined');
			assert.equal(JSON.parse((await b.run(['sync', 'status', '--json'])).stdout).chain, chain);
		});
	});

	test('through the prompt bar: `sync chain connect <link>` joins, and no history keeps the ticket', async () => {
		await twoBrowsers(async (a, b) => {
			await a.run(['occur', 'left', '-t', '0800']);
			await b.run(['occur', 'right', '-t', '0900']);
			const link = await linkFrom(a);
			const ticket = link.split('#')[1];
			await b.submit(`sync chain connect ${link}`);
			assert.match((await b.stdouts()).at(-1), /verify ok/);
			assert.equal((await b.echoes()).at(-1), 'sync chain connect <ticket>');
			await converged(a, b);
			const kept = await b.page.evaluate(() => localStorage.getItem('datum.repl.v1'));
			assert.ok(kept.includes('sync chain connect <ticket>'), kept);
			assert.ok(!kept.includes(ticket), 'the ticket is in the stored scrollback or history');
		});
	});
});

describe('the name a browser joins as (da-uyyyg)', () => {
	/** `app`'s own label and endpoint, as its `sync status` has them. */
	const self = async (app) => JSON.parse((await app.run(['sync', 'status', '--json'])).stdout).this;

	/** Open a join link to `native`'s chain in `app`, the card's name field filled in as it comes. */
	async function card(app, native) {
		const serving = native.spawn(['sync', 'chain', 'invite']);
		const [, link] = await until(() => /datum sync chain connect '([^']+)'/.exec(serving.stdout), Boolean);
		await app.goto(`/sync/join#${link.split('#').pop()}`);
		const card = app.page.locator('.card.join');
		const name = card.getByLabel('This device’s name');
		await name.waitFor();
		assert.equal(await name.inputValue(), 'datum.day (datum)');
		return { card, name, join: card.getByRole('button', { name: 'Join', exact: true }) };
	}

	test('two browsers joining one chain, neither typing a name, are named apart alike on every copy: the chain assembles both (da-xijlw)', async () => {
		const native = await nativeCopy(relay);
		try {
			await twoBrowsers(async (a, b) => {
				const first = await card(a, native);
				await first.join.click();
				await first.card.locator('.done').waitFor();
				// Alone on its host, a browser is its host: datum.day.
				assert.equal((await self(a)).label, 'datum.day');

				const second = await card(b, native);
				await second.join.click();
				await second.card.locator('.done').waitFor();
				// Two on one host and one database: each its database and short id,
				// on every copy, once it holds both describes. The invites served
				// only the joins: native hears the describes on a run of its own,
				// and each browser the other's at a session a write of its pushes.
				native.spawn(['sync', 'run']);
				const ids = [(await self(a)).endpoint, (await self(b)).endpoint];
				const want = JSON.stringify(ids.map((id) => `datum.day (datum) ${id.slice(0, 10)}`).sort());
				const shown = async (run) =>
					JSON.stringify(
						JSON.parse((await run(['sync', 'chain', 'list', '--json'])).stdout)
							.members.map((m) => m.label)
							.filter((l) => l.startsWith('datum.day'))
							.sort()
					);
				await until(() => shown(native.run), (ls) => ls === want);
				for (const [app, field] of [[a, 'pokea'], [b, 'pokeb']]) {
					await app.run(['occur', field, '-t', '1000']);
					await until(() => shown((argv) => app.run(argv)), (ls) => ls === want);
				}
			});
		} finally {
			await native.close();
		}
	});

	test('a name typed is checked before the join, and kept as typed though a member carries it', async () => {
		const native = await nativeCopy(relay);
		try {
			assert.equal((await native.run(['sync', 'new-invite'])).code, 0);
			assert.equal((await native.run(['sync', 'chain', 'rename', 'kitchen'])).code, 0);
			await withWallClock(
				async (app) => {
					const { card: c, name, join } = await card(app, native);
					await name.fill('bell\u0007');
					await join.click();
					await until(() => app.page.locator('.failure[role=alert]').innerText(), (t) => /control character/.test(t));
					assert.equal(JSON.parse((await app.run(['sync', 'chain', 'list', '--json'])).stdout).chain, null);

					await name.fill('kitchen');
					await join.click();
					await c.locator('.done').waitFor();
					assert.equal((await self(app)).label, 'kitchen');
				},
				{ prepare: relayed(relay) }
			);
		} finally {
			await native.close();
		}
	});
});

describe('two browsers', () => {
	test('pair through the relay, converge, and a write in one appears in the other untouched', async () => {
		await withWallClock(
			async (a) => {
				await withWallClock(
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
