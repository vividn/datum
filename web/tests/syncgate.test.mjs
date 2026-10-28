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
				// The page holds its apply after the first chunk.
				await until(() => app.page.evaluate(() => window.datum.sync.states().datum?.applying?.done), (n) => n === 64).catch((e) => {
					throw new Error(`${e.message}\n${serving.stdout}\n${serving.stderr}`);
				});
				// The first hears of the page from a, the second dials it.
				assert.ok(await syncWithin(b, 60_000), 'b synced with a');
				const r = await syncWithin(b, 60_000);
				assert.ok(r, 'b waited on the page');
				// Refused, and no failure: busy (da-h31wt).
				assert.match(r.stdout + r.stderr, /datum\.day.*refused/);
				assert.equal(r.code, 0, r.stdout + r.stderr);
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

/**
 * An init script: while `window.__hold.on`, the sync worker's `sync status`
 * runs wait for `window.__release()` — the first process of a page's join
 * once it holds its turn, and of a dial once it has reached its peer.
 */
function holdStatus() {
	const hold = (window.__hold = { on: false, held: [] });
	window.__release = () => {
		hold.on = false;
		hold.held.splice(0).forEach((f) => f());
	};
	const d = Object.getOwnPropertyDescriptor(Worker.prototype, 'onmessage');
	Object.defineProperty(Worker.prototype, 'onmessage', {
		...d,
		set(fn) {
			d.set.call(this, function (e) {
				if (hold.on && 'run' in Object(e.data) && e.data.argv.includes('status')) hold.held.push(() => fn.call(this, e));
				else fn.call(this, e);
			});
		}
	});
}

const offer = (app) => app.page.evaluate(() => window.datum.sync.offer('datum'));
const join = (app, t) =>
	app.page.evaluate((t) => window.datum.sync.connect('datum', t).then(() => 'joined', (e) => e.message), t);
const holding = (app) => app.page.evaluate(() => window.__hold.held.length);
/** Neither page has a session in flight, for a second: what the first join set going has ended. */
async function quiet(...apps) {
	const online = () =>
		Promise.all(apps.map((app) => app.page.evaluate(() => Object.values(window.datum.sync.states().datum?.peers ?? {}).some((p) => p.reach === 'online'))));
	for (let calm = 0; calm < 4; ) {
		calm = (await online()).some(Boolean) ? 0 : calm + 1;
		await new Promise((r) => setTimeout(r, 250));
	}
}

// b rejoins a's chain as a's push reaches it (da-tqg19): b holds its turn
// for the join, a holds its own for the push, and each refuses the other's
// session as busy. b refused its own dial for the session it was refusing,
// and with that gone a refused b's: the join failed either way. A join
// refused as busy gives up its turn and tries again.
test('a join that meets a member pushing to it, each page holding its turn, joins (da-tqg19)', async () => {
	const prepare = async (context) => (await relayed(relay)(context), context.addInitScript(holdStatus));
	await withWallClock(
		(a) =>
			withWallClock(
				async (b) => {
					await a.run(['occur', 'left', '-t', '0800']);
					assert.equal(await join(b, await offer(a)), 'joined');
					await until(() => countOf(b, 'left'), (n) => n === 1);
					const ticket = await offer(a);
					await quiet(a, b);
					await b.page.evaluate(() => (window.__hold.on = true));
					const again = join(b, ticket);
					await until(() => holding(b), (n) => n > 0);
					await a.page.evaluate(() => (window.__hold.on = true));
					await a.run(['occur', 'later', '-t', '1000']);
					await until(() => holding(a), (n) => n > 0);
					// b has a's connection in hand.
					await new Promise((r) => setTimeout(r, 1000));
					await b.page.evaluate(() => window.__release());
					const early = await Promise.race([again, new Promise((r) => setTimeout(() => r(null), 3000))]);
					await a.page.evaluate(() => window.__release());
					assert.equal(early ?? (await again), 'joined');
					await until(() => countOf(b, 'later'), (n) => n === 1);
				},
				{ prepare }
			),
		{ prepare }
	);
});

// The inviter serves one at a time, so it refuses one join as busy, or both
// when their handshakes cross: each tries again, at a jittered wait.
test('two pages joining one chain at once both join (da-tqg19)', async () => {
	const prepare = relayed(relay);
	await withWallClock(
		(a) =>
			withWallClock(
				(b) =>
					withWallClock(
						async (c) => {
							await a.run(['occur', 'left', '-t', '0800']);
							const [t1, t2] = [await offer(a), await offer(a)];
							assert.deepEqual(await Promise.all([join(b, t1), join(c, t2)]), ['joined', 'joined']);
							await until(
								() => Promise.all([countOf(b, 'left'), countOf(c, 'left')]),
								([x, y]) => x === 1 && y === 1
							);
						},
						{ prepare }
					),
				{ prepare }
			),
		{ prepare }
	);
});

/**
 * An init script: a dial to an endpoint in `window.__unsent` never reaches
 * the sync worker, so the page dials only the members a test leaves it.
 */
function steerDials() {
	window.__unsent = [];
	const post = Worker.prototype.postMessage;
	Worker.prototype.postMessage = function (m, ...rest) {
		if (m?.op === 'dial' && window.__unsent.some((id) => m.addr.includes(id))) return;
		return post.call(this, m, ...rest);
	};
}

const endpointOf = async (copy) => JSON.parse((await copy.run(['sync', 'status', '--json'])).stdout).this.endpoint;
const unsend = async (app, copy) => app.page.evaluate((id) => window.__unsent.push(id), await endpointOf(copy));
/** Whether `app` has a session with `copy` in flight: a dial waiting, connecting or under way, or one served. */
const inFlight = async (app, copy) =>
	app.page.evaluate((id) => window.datum.sync.states().datum?.peers?.[id]?.reach === 'online', await endpointOf(copy));
const steered = async (context) => (await relayed(relay)(context), context.addInitScript(steerDials));

// A page's dial to a member that is offline held its one session until its
// connect timed out, and refused every member that dialed it meanwhile as
// busy (da-gvia0). A dial arriving is a member online with something to
// send: it takes the turn of a dial still connecting, which dials again at
// its next.
test('a member dialing a page whose own dial is still connecting syncs (da-gvia0)', async () => {
	const [a, c] = [await nativeCopy(relay), await nativeCopy(relay)];
	try {
		await withWallClock(
			async (app) => {
				// Each joins through the page, and goes offline as its connect
				// ends. The page dials c alone: a dial to a, offline, would
				// hold the page's turn through c's join.
				for (const copy of [a, c]) {
					const joined = await copy.run(['sync', 'chain', 'connect', await offer(app)]);
					assert.equal(joined.code, 0, joined.stdout + joined.stderr);
					if (copy !== a) continue;
					await unsend(app, a);
					await quiet(app);
				}
				await app.run(['occur', 'tea', '-t', '0800']);
				await until(() => inFlight(app, c), Boolean);
				await new Promise((r) => setTimeout(r, 1000));
				const r = await a.run(['sync']);
				assert.match(r.stdout + r.stderr, /datum\.day.*verify ok/, r.stdout + r.stderr);
				assert.doesNotMatch(r.stdout + r.stderr, /datum\.day.*refused/, r.stdout + r.stderr);
				assert.equal(await a.count('tea'), 1);
			},
			{ prepare: steered, syncTimeout: 30 }
		);
	} finally {
		await a.close();
		await c.close();
	}
});

// The session that takes a dial's turn is one served before the dial is
// past its handshake: a member dialing a page whose own dial is applying is
// refused as busy, and the dial goes on.
test('a page dial past its handshake is not given way (da-gvia0)', async () => {
	const [a, b] = [await nativeCopy(relay), await nativeCopy(relay)];
	try {
		const first = a.spawn(['sync', 'chain', 'invite']);
		const [, ticket] = await until(() => /datum sync chain connect '([^']+)'/.exec(first.stdout), Boolean);
		assert.equal((await b.run(['sync', 'chain', 'connect', ticket])).code, 0);
		await withWallClock(
			async (app) => {
				assert.equal((await pairWith(app, a)).verified, true);
				await unsend(app, b);
				await quiet(app);
				const restored = await a.run(['db', 'restore', '-', '--merge'], backup(200));
				assert.equal(restored.code, 0, restored.stderr);
				// An invite serves and dials no one: what reaches the page is its own dial.
				const serving = a.spawn(['sync', 'chain', 'invite']);
				await until(() => serving.stdout, (s) => s.includes('datum sync chain connect'));
				await app.page.evaluate(() => localStorage.setItem('datum.syncHoldApplyAfter', '1'));
				await app.run(['occur', 'tea', '-t', '0800']);
				const applying = () => app.page.evaluate(() => window.datum.sync.states().datum?.applying?.done);
				await until(applying, (n) => n === 64);
				// The first hears of the page from a, the second dials it.
				assert.ok(await syncWithin(b, 60_000), 'b synced with a');
				const r = await syncWithin(b, 60_000);
				assert.ok(r, 'b waited on the page');
				// Refused, and no failure: busy (da-h31wt).
				assert.match(r.stdout + r.stderr, /datum\.day.*refused/);
				assert.equal(r.code, 0, r.stdout + r.stderr);
				await new Promise((r) => setTimeout(r, 6000));
				assert.equal(await applying(), 64);
			},
			{ prepare: steered }
		);
	} finally {
		await a.close();
		await b.close();
	}
});

/** An init script: while `window.__dials.on`, a page's dials wait for `window.__dials.release()`. */
function holdDials() {
	const hold = (window.__dials = { on: false, held: [] });
	hold.release = () => {
		hold.on = false;
		hold.held.splice(0).forEach((f) => f());
	};
	const post = Worker.prototype.postMessage;
	Worker.prototype.postMessage = function (m, ...rest) {
		if (hold.on && m?.op === 'dial') return void hold.held.push(() => post.call(this, m, ...rest));
		return post.call(this, m, ...rest);
	};
}

/** An init script: `window.__busy` lists each session the page saw refused as busy, dialed or served. */
function busyRefusals() {
	window.__busy = [];
	const d = Object.getOwnPropertyDescriptor(Worker.prototype, 'onmessage');
	Object.defineProperty(Worker.prototype, 'onmessage', {
		...d,
		set(fn) {
			d.set.call(this, function (e) {
				if (/runs one at a time/.test(e.data?.error ?? '')) window.__busy.push(e.data.error);
				return fn.call(this, e);
			});
		}
	});
}

// Two pages that dial each other at once each hold a turn, and the first
// hello to arrive found both dials open: refused as busy, and the pages
// synced only if the other hello came after that refusal. Each dial
// reaches the other and waits there, before its hello; the lower endpoint
// id's goes on, as natively (da-vj3ly), and carries both ways (da-gvia0).
test('two pages dialing each other at once converge, neither refused as busy (da-gvia0)', async () => {
	const prepare = async (context) => (
		await relayed(relay)(context),
		await context.addInitScript(holdStatus),
		await context.addInitScript(holdDials),
		context.addInitScript(busyRefusals)
	);
	await withWallClock(
		(a) =>
			withWallClock(
				async (b) => {
					await a.run(['occur', 'left', '-t', '0800']);
					assert.equal(await join(b, await offer(a)), 'joined');
					await until(() => countOf(b, 'left'), (n) => n === 1);
					await quiet(a, b);
					const both = (f) => Promise.all([a, b].map((app) => app.page.evaluate(f)));
					await both(() => (window.__hold.on = window.__dials.on = true));
					await Promise.all([a.run(['occur', 'east', '-t', '0900']), b.run(['occur', 'west', '-t', '0900'])]);
					// Each page takes its turn for its dial before the other's reaches it.
					await until(() => both(() => window.__dials.held.length), (n) => n.every(Boolean));
					await both(() => window.__dials.release());
					await until(async () => (await holding(a)) + (await holding(b)), (n) => n > 0);
					await new Promise((r) => setTimeout(r, 2000));
					await both(() => window.__release());
					await until(
						() => Promise.all([countOf(a, 'west'), countOf(b, 'east')]),
						([x, y]) => x === 1 && y === 1
					);
					for (const app of [a, b]) assert.deepEqual(await app.page.evaluate(() => window.__busy), []);
				},
				{ prepare }
			),
		{ prepare }
	);
});
