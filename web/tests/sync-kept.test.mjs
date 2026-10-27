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
			await arrives(a, b, 'tea');
			const [seenA, seenB] = [sessions(runs.a).length, sessions(runs.b).length];
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
			const back = b.spawn(['sync', 'run'], env);
			await until(() => b.count('coffee'), (n) => n === 1);
			// b's process is new: whichever session brought it, b had no connection to keep.
			const [first] = await until(() => sessions(back), (s) => s.length > 0);
			assert.ok(!first.kept, first.line);
			// And the connection it made is kept for the next.
			const pushed = sessions(runs.a).length;
			await arrives(a, b, 'water');
			await until(() => sessions(runs.a).length, (n) => n > pushed);
			assert.ok(sessions(runs.a).at(-1).kept, runs.a.stdout);
		} finally {
			await a.close();
			await b.close();
		}
	});
});
