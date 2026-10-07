// datum.day syncs with several tabs open on one database (da-6kpj5).
//
// vividn, 2026-09-29: «Has there been a test of syncing when you have multiple
// browser tabs open each writing data into the same database? That seems
// like a tricky edge case.» Every tab runs its own CLI processes against the
// one file, so ONE of them — whichever holds the `datum-sync:<db>` Web Lock —
// runs the endpoint, and pushes every tab's writes. The peer is a native
// copy running `datum sync run` on a local relay.
//
// Who leads, and the hand-off when a leader closes, are
// sync-tabs-leader.test.mjs: one file ran at its 300 s cap on a loaded CI
// runner (da-80xkt).

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { countOf, startRelay, verifyOf, withTabs } from './support/sync.mjs';
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

describe('two tabs writing at once', () => {
	test('lose nothing, with no peer', async () => {
		await withTabs(
			relay,
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
		await withTabs(relay, async (first, second, native) => {
			const base = await countOf(first, 'burst');
			await bothBurst(first, second);
			await until(() => native.count('burst'), (n) => n >= base + 30);
			await until(async () => [await native.verify(), await verifyOf(first)], ([a, b]) => a === b);
			assert.equal(await native.count('burst'), base + 30);
			assert.equal(await countOf(first, 'burst'), base + 30);
		});
	});
});

describe("a peer's write", () => {
	test('appears in every open tab', async () => {
		await withTabs(relay, async (first, second, native) => {
			assert.equal((await native.run(['occur', 'fromnative', '-t', '1000'])).code, 0);
			for (const tab of [first, second]) await until(() => tab.paneText('tail'), (text) => text.includes('fromnative'));
		});
	});
});
