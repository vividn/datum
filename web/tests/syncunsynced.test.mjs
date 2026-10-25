// The day view dims the stretch since this copy last synced while the
// session's first sync is under way, when that was over an hour ago
// (da-g5jjp, vividn: «maybe dimming the part of the dayview (since last
// open) that is still syncing? I would only do this if it was more than an
// hour.»).
//
// The page's clock is held at the wall clock's, as a native copy runs on it,
// and moved on two hours to make the last sync two hours old. A dim is seen
// by an observer from the page's first script: the strip draws before the
// first sync lands, or a reload lands it unseen.

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { backup, countOf, nativeCopy, relayed, startRelay } from './support/sync.mjs';
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

const HOUR = 3_600_000;
/** How much of a day each band dimmed, each time the dim changed: `[]` while there is none. */
const DIMS = '__dims';

const watched = (relay) => (context) =>
	Promise.all([
		relayed(relay)(context),
		context.addInitScript((key) => {
			window[key] = [];
			let last = '';
			new MutationObserver(() => {
				const dims = [...document.querySelectorAll('.band svg.svg rect.unsynced')].map(
					(r) => +r.getAttribute('width') / r.ownerSVGElement.viewBox.baseVal.width
				);
				const said = JSON.stringify(dims);
				if (said !== last) window[key].push(dims);
				last = said;
			}).observe(document, { subtree: true, childList: true });
		}, DIMS)
	]);

const dims = (app) => app.page.evaluate((key) => window[key], DIMS);
const dimmed = (seen) => seen.filter((d) => d.length);

/**
 * The page and a native copy paired and synced, the native one then
 * serving (`sync run`), or offline (`online: false`), and `body` run.
 */
async function paired(body, { online = true } = {}) {
	const native = await nativeCopy(relay);
	try {
		await withApp(
			async (app) => {
				const inviting = native.spawn(['sync', 'chain', 'invite']);
				const said = await until(() => /datum sync chain connect '([^']+)'/.exec(inviting.stdout), Boolean);
				const report = await app.page.evaluate((t) => window.datum.sync.connect('datum', t), said[1]);
				assert.equal(report.verified, true, JSON.stringify(report));
				inviting.child.kill();
				if (online) native.spawn(['sync', 'run']);
				await body(app, native);
			},
			{ clock: Date.now(), prepare: watched(relay) }
		);
	} finally {
		await native.close();
	}
}

test('over an hour since the last sync: that stretch is dimmed until the first sync lands, then not', async () => {
	await paired(async (app, native) => {
		// What another device wrote meanwhile: enough that its sync takes a while.
		assert.equal((await native.run(['db', 'restore', '-', '--merge', '-y'], backup(600))).code, 0);
		await app.clock.forward(2 * HOUR);
		await app.reload();
		await until(() => countOf(app, 'tea'), (n) => n === 200, 90_000);
		const seen = await until(() => dims(app), (s) => s.length && !s.at(-1).length);
		assert.ok(dimmed(seen).length, `never dimmed: ${JSON.stringify(seen)}`);
		// The two hours since the last sync, across whichever bands hold them.
		for (const d of dimmed(seen)) {
			const day = d.reduce((a, b) => a + b, 0) * 24;
			assert.ok(Math.abs(day - 2) < 0.1, `dimmed ${day.toFixed(2)} h, not the 2 h since the last sync`);
		}
		assert.deepEqual(await app.page.locator('rect.unsynced').count(), 0);
	});
});

test('under an hour since the last sync: nothing is dimmed', async () => {
	await paired(async (app, native) => {
		await native.run(['occur', 'coffee']);
		await app.clock.forward(HOUR - 60_000);
		await app.reload();
		await app.page.locator('.band[data-days-ago="0"] svg.svg').waitFor();
		await until(() => countOf(app, 'coffee'), (n) => n === 1, 60_000);
		assert.deepEqual(dimmed(await dims(app)), []);
	});
});

test('a peer offline: the dim lifts once its dial times out, nothing synced', async () => {
	await paired(
		async (app, native) => {
			await native.run(['occur', 'coffee']);
			await app.clock.forward(2 * HOUR);
			await app.reload();
			await until(() => dims(app), (s) => dimmed(s).length);
			await until(() => app.page.locator('rect.unsynced').count(), (n) => n === 0, 30_000);
			assert.equal(await countOf(app, 'coffee'), 0);
		},
		{ online: false }
	);
});
