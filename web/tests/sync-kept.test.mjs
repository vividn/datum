// Two native copies keep their connection open between sessions (da-3jcth),
// through the local relay: the next session, dialed or served, runs on a new
// stream of it, and a kept connection whose peer went away costs a fresh
// connect once the peer is back. No browser: the wire crate is the page's
// too, and these are its sessions.

import test, { after, before, describe } from 'node:test';
import assert from 'node:assert/strict';
import { nativeCopy, startRelay } from './support/sync.mjs';
import { until } from './support/until.mjs';

let relay;
before(async () => {
	relay = await startRelay();
});
after(async () => {
	await relay?.stop();
});

/** The session lines a `sync run` printed, each `{ kept }`. */
const sessions = (run) =>
	run.stdout
		.split('\n')
		.filter((l) => /: sent \d+, got \d+/.test(l))
		.map((line) => ({ line, kept: line.endsWith('(on the connection kept)') }));

/** The first session `run` printed past its `from`th that sent something. A
 * copy applies a round before the verify its peer prints the line after, so
 * the receiver can hold a write before the sender has said the session. */
const carried = (run, from) =>
	until(() => sessions(run).slice(from).find((s) => !/: sent 0,/.test(s.line)), Boolean);

/** How many sessions `run` had printed by the first past its `from`th that
 * `sent` something, or got something: what is past it came after. */
const past = (run, from, sent) =>
	until(() => sessions(run).findIndex((s, i) => i >= from && !(sent ? /: sent 0,/ : /, got 0,/).test(s.line)) + 1, Boolean);

/** `a` and `b` in one chain, each with its `sync run`, settled. */
async function pair() {
	const [a, b] = [await nativeCopy(relay), await nativeCopy(relay)];
	const inviting = a.spawn(['sync', 'chain', 'invite']);
	const [, ticket] = await until(() => /datum sync chain connect '([^']+)'/.exec(inviting.stdout), Boolean);
	assert.equal((await b.run(['sync', 'chain', 'connect', ticket])).code, 0);
	// A short session timeout, so a dead connection's stream fails in seconds.
	const env = { DATUM_SYNC_TIMEOUT_S: '5', DATUM_SYNC_TEST_RETRY_MS: '500' };
	const runs = { a: a.spawn(['sync', 'run'], env), b: b.spawn(['sync', 'run'], env) };
	// Each dials the other as it starts: once both have said a session, they are up.
	await until(() => [runs.a, runs.b].every((r) => sessions(r).length > 0), Boolean);
	return { a, b, runs, env };
}

/** Log `field` on `from`, and wait until `to` holds it. */
async function arrives(from, to, field) {
	assert.equal((await from.run(['occur', field])).code, 0);
	await until(() => to.count(field), (n) => n === 1);
}

describe('a kept connection', () => {
	test('two sessions in a row: the second opens no new connection, and both sync', async () => {
		const { a, b, runs } = await pair();
		try {
			const [fromA, fromB] = [sessions(runs.a).length, sessions(runs.b).length];
			await arrives(a, b, 'tea');
			// Counted past each side's line for the session that brought tea,
			// which can print after b holds it: read too soon, that first
			// session, on a connection of its own, was counted as the second's
			// (da-r29g3).
			const [seenA, seenB] = [await past(runs.a, fromA, true), await past(runs.b, fromB, false)];
			await arrives(a, b, 'coffee');
			// The push that carried it says so once its verify is in.
			await until(() => sessions(runs.a).length, (n) => n > seenA);
			await until(async () => [await a.verify(), await b.verify()], ([x, y]) => x === y);
			const since = [...sessions(runs.a).slice(seenA), ...sessions(runs.b).slice(seenB)];
			assert.ok(
				since.every((s) => s.kept),
				`a session after the first opened a connection:\n${since.map((s) => s.line).join('\n')}`
			);
		} finally {
			await a.close();
			await b.close();
		}
	});

	test('whose peer went away: the next session connects afresh and syncs once the peer is back', async () => {
		const { a, b, runs, env } = await pair();
		try {
			await arrives(a, b, 'tea');
			// Gone with no goodbye, as a laptop shut: the connection a keeps is dead.
			runs.b.child.kill('SIGKILL');
			await new Promise((r) => runs.b.child.once('exit', r));
			assert.equal((await a.run(['occur', 'coffee'])).code, 0);
			// The push fails on the dead connection's stream, then connects afresh and reaches no one.
			await until(() => runs.a.stderr, (e) => /could not reach/.test(e));
			const gone = sessions(runs.a).length;
			const back = b.spawn(['sync', 'run'], env);
			await until(() => b.count('coffee'), (n) => n === 1);
			// b's process is new: whichever session brought it, b had no connection to keep.
			const [first] = await until(() => sessions(back), (s) => s.length > 0);
			assert.ok(!first.kept, first.line);
			// And the connection it made is kept for the next: counted past a's
			// line for the session that brought coffee, which can print after b holds it.
			const coffee = await past(runs.a, gone, true);
			await arrives(a, b, 'water');
			const water = await carried(runs.a, coffee);
			assert.ok(water.kept, runs.a.stdout);
			// a's push begun while b was away was still connecting as b's own
			// dial brought coffee: it runs on the connection that one left
			// kept, and opens none of its own (da-qv0do). It ends after water's
			// session, so it is read once a has said nothing for a second.
			await until(async () => {
				const n = sessions(runs.a).length;
				await new Promise((r) => setTimeout(r, 1000));
				return sessions(runs.a).length === n;
			}, Boolean);
			assert.ok(
				sessions(runs.a)
					.slice(coffee)
					.every((s) => s.kept),
				runs.a.stdout
			);
		} finally {
			await a.close();
			await b.close();
		}
	});
});
