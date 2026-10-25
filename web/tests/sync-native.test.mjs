// datum.day syncs with a native copy (da-6kpj5): the page's endpoint and
// `datum` itself, through a local relay. Split from sync.test.mjs, whose
// groups together ran near the runner's 300-s per-file cap (da-oiurn).
//
// Every copy here is real: the browser's is the wire crate in the sync
// worker with the CLI as its store, a native one is `datum` itself, and the
// relay is iroh's. What a test asserts is what the copies HOLD — their
// `sync pipe verify` hashes and their entries — never what the page says it did.

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
 * An init script: count the leader's pushes — its `sync pipe vector` reads;
 * the sync worker's session asks its own as `sync --as …` — and its dials, at
 * the workers' doors, with the pull's 30-s tick taken away so that only
 * writes push.
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
	// 30000 is `PULL_MS` in src/lib/sync.ts: a tick this misses dials with no
	// write, which is what `dial` would count.
	const every = window.setInterval;
	window.setInterval = (f, ms, ...a) => (ms === 30000 ? 0 : every(f, ms, ...a));
}

/** The origin of `field`'s newest entry's latest op, as the copy `run` runs on holds it. */
async function originOf(run, field) {
	const { rows } = JSON.parse((await run(['tail', field, '--show', 'json'])).stdout);
	const versions = JSON.parse((await run(['history', rows.at(-1).hid, '--show', 'json'])).stdout);
	return versions[0].origin;
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
					// Each op names who and what signed it, read on the other copy
					// (da-zjc9w): the page's says datum.day, with no user or host,
					// which a page cannot know; the native one's says datum.
					const web = await originOf(native.run, 'coffee');
					assert.equal(web.program, 'datum.day', JSON.stringify(web));
					assert.equal(web.user, undefined, JSON.stringify(web));
					assert.equal(web.host, undefined, JSON.stringify(web));
					assert.ok(web.installation && web.version, JSON.stringify(web));
					const cli = await originOf((argv) => app.run(argv), 'tea');
					assert.equal(cli.program, 'datum', JSON.stringify(cli));
					assert.ok(cli.host && cli.installation && cli.version, JSON.stringify(cli));
				},
				{ prepare: relayed(relay) }
			);
		} finally {
			await native.close();
		}
	});

	test('the page answers a native copy\'s `sync diff`: what it holds of each entry that differs, and nothing moves (da-n0ruf)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					await app.run(['occur', 'coffee', '-t', '0900']);
					await pairWith(app, native);
					const web = (await endpointOf(app)).slice(0, 12);
					// The invite's own process may still hold the native copy's key for a moment.
					const same = await until(() => native.run(['sync', 'diff', web]), (r) => r.code === 0);
					assert.match(same.stdout, /hold the same data \(\d+ entries compared\)$/);

					// From here neither serves the other: the native copy runs nothing, so
					// the page's push finds no one, and each keeps an entry of its own.
					assert.equal((await native.run(['occur', 'tea', '-t', '0800'])).code, 0);
					await app.run(['occur', 'juice', '-t', '1000']);
					const held = await verifyOf(app);
					const differ = await native.run(['sync', 'diff', web, '--json']);
					assert.equal(differ.code, 0, differ.stderr);
					// Each entry and the spec its log declared, by its field.
					const all = JSON.parse(differ.stdout).differ;
					const found = Object.fromEntries(all.map((d) => [d.slug.split(':')[0], d]));
					assert.equal(all.length, 4, differ.stdout);
					assert.deepEqual(Object.keys(found).sort(), ['juice', 'spec', 'tea']);
					assert.equal(found.tea.there.row, null, differ.stdout);
					assert.deepEqual(found.tea.there.ops, [], differ.stdout);
					assert.equal(found.tea.here.ops[0].kind, 'create', differ.stdout);
					assert.equal(found.juice.here.row, null, differ.stdout);
					assert.equal(found.juice.there.row.field, 'juice', differ.stdout);
					assert.equal(found.juice.there.ops[0].kind, 'create', differ.stdout);
					assert.equal(await verifyOf(app), held);
					// Answering was no session: the page still says the last one verified.
					const status = JSON.parse((await app.run(['sync', 'status', '--json'])).stdout);
					assert.equal(status.members[0].last_verify.match, true, JSON.stringify(status));
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
					const said = await until(() => /datum sync chain connect '([^']+)'/.exec(serving.stdout), Boolean);
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

	// The native copy applies slowly, so the page, its first round applied,
	// waits a round on it: the purge lands there. A session that went on took
	// the whole chain back, as a copy in no chain does.
	test('a purge between an open session\'s rounds leaves the database empty (da-otz0d)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					await pairWith(app, native);
					await app.run(['occur', 'tea', '-t', '0900']);
					assert.equal((await native.run(['occur', 'juice', '-t', '1000'])).code, 0);
					const runs = native.spawn(['sync', 'run'], { DATUM_SYNC_TEST_APPLY_DELAY_MS: '2000' });
					await until(() => countOf(app, 'juice'), (n) => n === 1);
					assert.equal((await app.run(['db', 'purge', '--leave', '--force', '--yes'])).code, 0);

					// Its session with the page over, one way or the other: a line past «is up».
					await until(() => native.count('tea'), (n) => n === 1);
					await until(() => (runs.stdout + runs.stderr).trim().split('\n'), (lines) => lines.length > 1);
					assert.equal(await countOf(app, 'juice'), 0);
					assert.equal((await app.run(['sync', 'pipe', 'ops'])).stdout, '');
					assert.notEqual((await app.run(['sync', 'chain', 'list'])).code, 0);
				},
				{ prepare: relayed(relay) }
			);
		} finally {
			await native.close();
		}
	});

	// The views follow the apply, not the verify after it, which hashes every
	// row on both sides: seconds on a big history, the stale picture an
	// opened page showed until it was over. Held here, it never is: held
	// before `sync run` starts, so no session after the pairing ends. Which
	// side dials is a race (the run steps aside while the invite is still at
	// work), and `reach` says `online` only for the page's own dial, so the
	// witness is `lastSeen`, which a session ending sets either way (da-h8n4e).
	test('what a peer sends is on the leading tab once it applies, before its session verifies (da-ajk4z)', async () => {
		const native = await nativeCopy(relay);
		try {
			await withWallClock(
				async (app) => {
					await pairWith(app, native);
					const seen = () => app.page.evaluate(() => Object.values(window.datum.sync.states().datum.peers).map((p) => p.lastSeen));
					await app.page.evaluate(() => localStorage.setItem('datum.syncHoldVerify', ''));
					const paired = await seen();
					assert.equal(paired.filter(Number.isFinite).length, 1, 'the pairing was seen');
					native.spawn(['sync', 'run']);
					assert.equal((await native.run(['occur', 'arrived'])).code, 0);
					await until(() => app.paneText('tail'), (text) => text.includes('arrived'));
					// A verify not held ends its session within the second.
					await new Promise((r) => setTimeout(r, 1000));
					assert.deepEqual(await seen(), paired, 'the session ended: its verify was not held');
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
			await withWallClock(
				async (app) => {
					await pairWith(app, native);
					const before = await endpointOf(app);
					await app.page.close();
					// Nobody answers for the page's key now: a dial fails. (Before
					// `sync run`, which would be a second endpoint on the native key.)
					// Offline, its words under `-v` (da-a4fxr): the only member, so
					// nothing synced and it fails (da-9d6uy).
					const now = await native.run(['sync', '-v'], '', { DATUM_SYNC_TIMEOUT_S: '3' });
					assert.notEqual(now.code, 0, now.stdout);
					assert.match(now.stdout, /: offline, last seen /);
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
