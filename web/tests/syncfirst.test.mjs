// A page's first sync after a load reaches the live member in seconds
// (da-612uv). It runs one session at a time (da-dwm2y), so the order and the
// limit of each dial decide the wait: it dialed in join order, the first
// admitted first, and gave each the idle limit, so one offline member that
// joined first cost a minute before the live one. Now the member seen last
// goes first, one its last try never reached goes last, and a dial a person
// waits on reaches its member in 5 s, as `datum sync`'s (da-75rbi).
//
// `syncTimeout: 30` is the idle limit here, the minute production waits, so
// the old order would wait 30 s for a, offline, before it dialed b.

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
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

/** The page's members, as its `sync status --json` has them, by endpoint. */
const members = async (app) =>
	Object.fromEntries(JSON.parse((await app.run(['sync', 'status', '--json'])).stdout).members.map((m) => [m.endpoint, m]));

/** `copy`'s endpoint id, as a member's `sync status` names it. */
const idOf = async (copy) => JSON.parse((await copy.run(['sync', 'status', '--json'])).stdout).this.endpoint;

test('with the first-admitted member offline, a load syncs with the live one within seconds (da-612uv)', async () => {
	const [a, b] = [await nativeCopy(relay), await nativeCopy(relay)];
	try {
		const first = a.spawn(['sync', 'chain', 'invite']);
		const [, ticket] = await until(() => /datum sync chain join '([^']+)'/.exec(first.stdout), Boolean);
		assert.equal((await b.run(['sync', 'chain', 'join', ticket])).code, 0);
		await withWallClock(
			async (app) => {
				assert.equal((await pairWith(app, a)).verified, true);
				const [ida, idb] = [await idOf(a), await idOf(b)];
				const run = a.spawn(['sync', 'run']);
				// b hears of the page from a, and the page of b.
				await until(() => b.run(['sync']), (heard) => heard.code === 0, 60_000);
				// b dials nothing again in the test's time, so a session with it
				// after the load is the page's own dial.
				b.spawn(['sync', 'run'], { DATUM_SYNC_TEST_RETRY_MS: '600000' });
				await until(() => members(app), (ms) => ms[idb]?.last_synced, 90_000);
				// a goes down, and the page's next push finds it so.
				run.child.kill();
				await new Promise((r) => run.child.once('exit', r));
				await app.run(['occur', 'tea', '-y', '-t', '0800']);
				await until(() => members(app), (ms) => ms[ida]?.state === 'offline', 90_000);
				const joined = JSON.parse((await app.run(['sync', 'status', '--json'])).stdout).members.map((m) => m.endpoint);
				assert.deepEqual(joined.slice(0, 2), [ida, idb], 'a joined first, so the old order dials it first');
				const loaded = Date.now();
				const since = Math.floor(loaded / 1000) * 1000;
				await app.reload();
				await until(() => members(app), (ms) => Date.parse(ms[idb]?.last_synced ?? '') >= since, 90_000);
				const took = Date.now() - loaded;
				console.log(`# first sync with the live member ${took} ms after the load`);
				assert.ok(took < 15_000, `${took} ms`);
			},
			{ prepare: relayed(relay), syncTimeout: 30 }
		);
	} finally {
		await a.close();
		await b.close();
	}
});
