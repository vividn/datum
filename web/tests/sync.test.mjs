// datum.day syncs (da-6kpj5): the page's endpoint, through a local relay.
//
// Every copy here is real: the browser's is the wire crate in the sync
// worker with the CLI as its store, a native one is `datum` itself, and the
// relay is iroh's. What a test asserts is what the copies HOLD — their
// `sync pipe verify` hashes and their entries — never what the page says it did.
// The several-tabs cases are sync-tabs.test.mjs.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { countOf, leadsOf, nativeCopy, pairWith, relayed, startRelay, verifyOf, withWallClock } from './support/sync.mjs';
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

/**
 * An init script: count the leader's pushes — the `sync pipe vector` reads
 * the sync worker's session did not ask for — and its dials, at the workers'
 * doors, with the pull's 30-s tick taken away so that only writes push.
 */
function countPushes() {
	window.__pushes = { push: 0, dial: 0 };
	const vector = (argv) => argv.slice(-3).join(' ') === 'sync pipe vector';
	const post = Worker.prototype.postMessage;
	Worker.prototype.postMessage = function (m, ...rest) {
		if (m?.op === 'dial') window.__pushes.dial++;
		if (m?.op === 'cli' && vector(m.argv)) window.__pushes.push++;
		return post.call(this, m, ...rest);
	};
	window.Worker = class extends Worker {
		constructor(...args) {
			super(...args);
			this.addEventListener('message', (e) => e.data?.run && vector(e.data.argv) && window.__pushes.push--);
		}
	};
	// 30000 is `PULL_MS` in src/lib/sync.ts: a tick this misses dials with no
	// write, which is what `dial` would count.
	const every = window.setInterval;
	window.setInterval = (f, ms, ...a) => (ms === 30000 ? 0 : every(f, ms, ...a));
}

/** `app`'s push counts once they and its peers' sessions have stood still for a second. */
async function settled(app) {
	const read = () =>
		app.page.evaluate(() =>
			JSON.stringify([window.__pushes, Object.values(window.datum.sync.states().datum?.peers ?? {}).map((p) => p.reach)])
		);
	let last = await read();
	for (let i = 0; i < 30; i++) {
		await new Promise((r) => setTimeout(r, 1000));
		const now = await read();
		if (now === last && !now.includes('"online"')) return JSON.parse(now)[0];
		last = now;
	}
	throw new Error(`never settled; last saw: ${last}`);
}

describe('datum.day and a native copy', () => {
	test('pair through the relay and converge', async () => {
		const native = await nativeCopy(relay);
		try {
			assert.equal((await native.run(['occur', 'tea', '-t', '0800'])).code, 0);
			await withWallClock(
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
			await withWallClock(
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

	test('one write is one push: what a session itself writes pushes nothing (da-df6cb)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					await pairWith(app, native);
					native.spawn(['sync', 'run']);
					const before = await settled(app);
					await app.run(['occur', 'once', '-t', '1000']);
					await until(() => native.count('once'), (n) => n === 1);
					const after = await settled(app);
					// The write's push dials the peer once. The session's own writes —
					// its handshake, the `record-session` at its end — were heard as
					// writes too, each a push, while the dial was still open.
					assert.deepEqual({ push: after.push - before.push, dial: after.dial - before.dial }, { push: 1, dial: 1 });
				},
				{ prepare: async (context) => (await relayed(relay)(context), context.addInitScript(countPushes)) }
			);
		} finally {
			await native.close();
		}
	});

	test('closing the tab stops its endpoint; reopened, it resumes and takes what was written meanwhile', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
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
			const join = card.getByRole('button', { name: 'join', exact: true });
			await join.waitFor();
			const words = await card.locator('.what').innerText();
			assert.match(words, /Join the sync chain of «datum\.day» \(chain (\w{10})\) with this browser’s database «datum»/);
			assert.equal(/\(chain (\w{10})\)/.exec(words)[1], chain.slice(0, 10));
			assert.match(words, /The link works once, until \d\d:\d\d\./);
			await join.click();
			await card.locator('.done').waitFor();
			await converged(a, b);

			// Opened again, the link says there is nothing to join.
			await b.goto(link);
			await until(() => card.locator('.failure').innerText(), (t) => /«datum» is in this chain already/.test(t));
			assert.equal(await join.count(), 0);
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
			const join = card.getByRole('button', { name: 'join', exact: true });
			await join.waitFor();
			assert.match(await card.locator('.what').innerText(), /«datum»’s own sync chain, just this browser, is replaced by this one\./);
			await join.click();
			await card.locator('.done').waitFor();
			await converged(a, b);
			assert.equal(JSON.parse((await b.run(['sync', 'status', '--json'])).stdout).chain, chain);

			// In a chain of two now, another invite is refused before it dials.
			const ticket = (await linkFrom(a)).split('#')[1];
			const again = await b.page.evaluate(
				(t) => window.datum.sync.connect('datum', t).then(() => 'joined', (e) => e.message),
				ticket
			);
			assert.match(again, /a copy belongs to one chain/);
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
