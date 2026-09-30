// datum.day syncs with several tabs open on one database (da-6kpj5).
//
// vividn, 2026-09-29: «Has there been a test of syncing when you have multiple
// browser tabs open each writing data into the same database? That seems
// like a tricky edge case.» Every tab runs its own CLI processes against the
// one file, so ONE of them — whichever holds the `datum-sync:<db>` Web Lock —
// runs the endpoint, and pushes every tab's writes. The peer is a native
// copy running `datum sync run` on a local relay.

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

/** `app` and one more tab on its database, paired with a running native copy (or none). */
async function withTabs(fn, { peer = true } = {}) {
	const native = peer ? await nativeCopy(relay) : null;
	try {
		await withApp(
			async (first) => {
				await first.declare('burst');
				if (native) {
					assert.equal((await pairWith(first, native)).verified, true);
					native.spawn(['sync', 'run']);
				}
				const second = await first.newTab();
				await fn(first, second, native);
			},
			{ prepare: relayed(relay) }
		);
	} finally {
		await native?.close();
	}
}

/** Fifteen `occur burst`s from `tab`, all fired at once, at `hour`:00–14. */
const burst = (tab, hour) =>
	tab.page.evaluate(
		(h) =>
			Promise.all(
				Array.from({ length: 15 }, (_, i) =>
					window.datum.runQuiet(['occur', 'burst', '-t', `${h}${String(i).padStart(2, '0')}`])
				)
			),
		hour
	);

/** Both tabs' bursts at the same moment; every write succeeded. */
async function bothBurst(first, second) {
	const results = (await Promise.all([burst(first, '08'), burst(second, '09')])).flat();
	assert.deepEqual(
		results.filter((r) => r.code !== 0).map((r) => r.stderr),
		[]
	);
}

/**
 * `native` comes to hold `n` bursts. A miss keeps watching past a pull and a
 * hung dial's 60 s, and fails saying whether the write came late or never,
 * and what `leader`'s sync saw — the question da-zyox0 could not settle from
 * one CI red that no local run reproduced.
 */
async function arrives(native, leader, n) {
	const start = Date.now();
	try {
		await until(() => native.count('burst'), (c) => c === n);
	} catch (e) {
		const came = await until(() => native.count('burst'), (c) => c === n, 70_000).then(
			() => `came late, at ${Math.round((Date.now() - start) / 1000)} s`,
			() => 'never came'
		);
		// Heard, not read: a leader that never posted still shows the last
		// one's state, endpoint and all.
		const heard = await leader.page.evaluate(() => window.datum.sync.states().datum);
		throw new Error(`${e.message}; the write ${came}; the last sync state its tab heard: ${JSON.stringify(heard)}`);
	}
}

describe('two tabs writing at once', () => {
	test('lose nothing, with no peer', async () => {
		await withTabs(
			async (first, second) => {
				const base = await countOf(first, 'burst');
				await bothBurst(first, second);
				assert.equal(await countOf(first, 'burst'), base + 30);
				assert.equal(await countOf(second, 'burst'), base + 30);
			},
			{ peer: false }
		);
	});

	test('while syncing to a peer: every entry from both tabs arrives, once, and the copies verify equal', async () => {
		await withTabs(async (first, second, native) => {
			const base = await countOf(first, 'burst');
			await bothBurst(first, second);
			await until(() => native.count('burst'), (n) => n >= base + 30);
			await until(async () => [await native.verify(), await verifyOf(first)], ([a, b]) => a === b);
			assert.equal(await native.count('burst'), base + 30);
			assert.equal(await countOf(first, 'burst'), base + 30);
		});
	});
});

describe('one endpoint per database', () => {
	test('however many tabs are open: one leads, every tab names its endpoint, the peer knows one copy', async () => {
		await withTabs(async (first, second, native) => {
			const third = await first.newTab();
			const tabs = [first, second, third];
			const leads = async () => (await Promise.all(tabs.map(leadsOf))).map((l) => l.includes('datum'));
			await until(leads, (l) => l.filter(Boolean).length === 1);
			const endpoints = async () =>
				Promise.all(tabs.map((t) => t.page.evaluate(() => window.datum.sync.states().datum?.endpoint)));
			const [endpoint] = await until(endpoints, (e) => e[0] && e.every((x) => x === e[0]));
			// Each tab's write reaches the peer, through the one endpoint.
			const base = await countOf(first, 'burst');
			for (const [i, tab] of tabs.entries()) await tab.run(['occur', 'burst', '-t', `100${i}`]);
			await until(() => native.count('burst'), (n) => n === base + 3);
			const { members } = JSON.parse((await native.run(['sync', 'status', '--json'])).stdout);
			assert.deepEqual(
				members.map((m) => m.endpoint),
				[endpoint]
			);
		});
	});

	test('closing the leading tab hands the endpoint to another, and a write after reaches the peer', async () => {
		await withTabs(async (first, second, native) => {
			await until(async () => [...(await leadsOf(first)), ...(await leadsOf(second))], (l) => l.length === 1);
			const [leader, other] = (await leadsOf(first)).length ? [first, second] : [second, first];
			await leader.page.close();
			await until(() => leadsOf(other), (l) => l.includes('datum'));
			const base = await countOf(other, 'burst');
			const wrote = await other.run(['occur', 'burst', '-t', '1000']);
			assert.equal(wrote.code, 0, wrote.stderr);
			await arrives(native, other, base + 1);
		});
	});

	test('a new leader whose reads of its setup fail backs off, and pushes once one succeeds', async () => {
		await withTabs(async (first, second, native) => {
			await until(async () => [...(await leadsOf(first)), ...(await leadsOf(second))], (l) => l.length === 1);
			const [leader, other] = (await leadsOf(first)).length ? [first, second] : [second, first];
			// Its first three fail: waits of 2, 4 and 8 s, where a flat 2 s is 6 (da-olsuv).
			await other.page.evaluate(() => localStorage.setItem('datum.syncFailRefresh', '3'));
			const closed = Date.now();
			await leader.page.close();
			await until(() => leadsOf(other), (l) => l.includes('datum'));
			const base = await countOf(other, 'burst');
			assert.equal((await other.run(['occur', 'burst', '-t', '1000'])).code, 0);
			await arrives(native, other, base + 1);
			assert.equal(await other.page.evaluate(() => localStorage.getItem('datum.syncFailRefresh')), '0');
			assert.ok(Date.now() - closed >= 14_000, `arrived ${Date.now() - closed} ms after the hand-off`);
		});
	});
});

describe("a peer's write", () => {
	test('appears in every open tab', async () => {
		await withTabs(async (first, second, native) => {
			assert.equal((await native.run(['occur', 'fromnative', '-t', '1000'])).code, 0);
			for (const tab of [first, second]) await until(() => tab.paneText('tail'), (text) => text.includes('fromnative'));
		});
	});
});
