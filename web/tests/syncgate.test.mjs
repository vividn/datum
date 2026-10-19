// One session applies into a page at a time (da-dwm2y), as into a native
// copy (da-h31wt), and a member refused meanwhile leaves the join's count
// shown.
//
// The page's join is held mid-apply (`datum.syncHoldApplyAfter`: its first
// chunk applies, the next never ends). b, a member, learns of the page from
// a — whose invite serves it meanwhile — and dials it: the page refuses b
// as busy, at once, which is no failure. Without the gate the page read b's
// handshake and refused it as not a member, an error: a joining page holds
// the chain's records only once its history is in. Either refusal's end
// blanked the join's count, until its next chunk.

import test, { after, before } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp } from './support/app.mjs';
import { backup, countOf, nativeCopy, pairWith, relayed, startRelay, withWallClock } from './support/sync.mjs';
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

/** `copy`'s `sync`, or null if it has not ended in `ms`. */
async function syncWithin(copy, ms) {
	let timer;
	const late = new Promise((r) => (timer = setTimeout(() => r(null), ms)));
	return Promise.race([copy.run(['sync']), late]).finally(() => clearTimeout(timer));
}

test('a member dialing a page whose join is applying is refused as busy (da-dwm2y)', async () => {
	const [a, b] = [await nativeCopy(relay), await nativeCopy(relay)];
	try {
		const restored = await a.run(['db', 'restore', '-', '--yes'], backup(200));
		assert.equal(restored.code, 0, restored.stderr);
		const first = a.spawn(['sync', 'chain', 'invite']);
		const [, ticket] = await until(() => /datum sync chain connect '([^']+)'/.exec(first.stdout), Boolean);
		assert.equal((await b.run(['sync', 'chain', 'connect', ticket])).code, 0);
		await withWallClock(
			async (app) => {
				await app.page.evaluate(() => localStorage.setItem('datum.syncHoldApplyAfter', '1'));
				const serving = a.spawn(['sync', 'chain', 'invite']);
				const [, t] = await until(() => /datum sync chain connect '([^']+)'/.exec(serving.stdout), Boolean);
				const joining = app.page.evaluate((t) => window.datum.sync.connect('datum', t).catch(() => {}), t).catch(() => {});
				await until(() => serving.stderr, (s) => s.includes('waiting for it to apply')).catch((e) => {
					throw new Error(`${e.message}\n${serving.stdout}\n${serving.stderr}`);
				});
				// The first hears of the page from a, the second dials it.
				assert.ok(await syncWithin(b, 60_000), 'b synced with a');
				const r = await syncWithin(b, 60_000);
				assert.ok(r, 'b waited on the page');
				assert.match(r.stdout + r.stderr, /datum\.day.*busy applying another sync's changes/);
				// The page hears of the refusal once its connection closes, 5 s at most.
				await new Promise((r) => setTimeout(r, 6000));
				const applying = await app.page.evaluate(() => window.datum.sync.states().datum?.applying);
				assert.equal(applying?.done, 64, JSON.stringify(applying));
				void joining;
			},
			{ prepare: relayed(relay) }
		);
	} finally {
		await a.close();
		await b.close();
	}
});

/**
 * An init script: the most sessions the page has had open at once, at the
 * database worker's door — each runs `sync handshake <peer>` once admitted
 * and `sync record-session <peer>` as it ends, and a dial that reached no
 * one the second alone.
 */
function countSessions() {
	window.__sessions = { open: new Set(), most: 0 };
	const post = Worker.prototype.postMessage;
	Worker.prototype.postMessage = function (m, ...rest) {
		const s = window.__sessions;
		const peer = (word) => m.argv[m.argv.indexOf(word) + 1];
		if (m?.op === 'cli' && m.argv.includes('handshake')) s.open.add(peer('handshake'));
		if (m?.op === 'cli' && m.argv.includes('record-session')) s.open.delete(peer('record-session'));
		s.most = Math.max(s.most, s.open.size);
		return post.call(this, m, ...rest);
	};
}

// After a big join the page dialed every member at once, and its CLI runs
// one process at a time: each session's handshake and vector waited behind
// another's apply or verify, minutes long, with nothing sent, and every
// member but one timed out on it. A dial waits its turn now.
test('a page pushing to two members runs one session at a time (da-dwm2y)', async () => {
	const [a, b] = [await nativeCopy(relay), await nativeCopy(relay)];
	try {
		const first = a.spawn(['sync', 'chain', 'invite']);
		const [, ticket] = await until(() => /datum sync chain connect '([^']+)'/.exec(first.stdout), Boolean);
		assert.equal((await b.run(['sync', 'chain', 'connect', ticket])).code, 0);
		await withWallClock(
			async (app) => {
				assert.equal((await pairWith(app, a)).verified, true);
				// b hears of the page from a, and the page of b.
				// Each applies slowly, so a session with either lasts long enough to overlap.
				const slow = { DATUM_SYNC_TEST_APPLY_DELAY_MS: '1500' };
				a.spawn(['sync', 'run'], slow);
				const heard = await b.run(['sync']);
				assert.equal(heard.code, 0, heard.stdout + heard.stderr);
				b.spawn(['sync', 'run'], slow);
				await until(() => app.page.evaluate(() => Object.keys(window.datum.sync.states().datum?.peers ?? {}).length), (n) => n === 2);
				await app.page.evaluate(() => (window.__sessions.most = window.__sessions.open.size));
				for (const t of ['0800', '0900', '1000']) await app.run(['occur', 'tea', '-t', t]);
				await until(async () => [await a.count('tea'), await b.count('tea')], ([x, y]) => x === 3 && y === 3, 60_000);
				assert.equal(await app.page.evaluate(() => window.__sessions.most), 1);
				assert.equal(await countOf(app, 'tea'), 3);
			},
			{ prepare: async (context) => (await relayed(relay)(context), context.addInitScript(countSessions)), syncTimeout: 20 }
		);
	} finally {
		await a.close();
		await b.close();
	}
});
