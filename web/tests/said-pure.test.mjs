// What a bare `sync` on the page says of each member (da-5lpeg) is what this
// round did (da-i97ef): a member the round did not reach (paused, or the
// endpoint changed under it) is not synced, not its last good session. And
// before its setup is read, a read that failed is the answer.

import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { Leader } = await import('../src/lib/sync.ts');

const report = { id: 'p', label: 'phone', sent: 1, got: 0, conflicts: 0, verify: 'verify ok', vector: {} };

/** A leader in a chain with one member, whose round does what `round` does. */
function leader(round) {
	const l = new Leader('datum');
	Object.assign(l, { read: true, endpoint: 'e', peers: [{ endpoint: 'p', label: 'phone', addr: '{}' }] });
	l.syncNow = async () => round(l);
	return l;
}

test('a member the round did not reach reads «not synced», not its earlier session, and none synced', async () => {
	const l = leader(() => {});
	l.done('p', report);
	l.seen.set('p', Date.now() - 60_000);
	assert.deepEqual(await l.said(), { lines: ['syncing with phone', 'phone: not synced'], ok: false });
});

test('a member the round did not reach reads «not synced», not an earlier round\'s error', async () => {
	const l = leader(() => {});
	l.errors.set('p', 'offline');
	l.erred.set('p', Date.now() - 60_000);
	assert.deepEqual(await l.said(), { lines: ['syncing with phone', 'phone: not synced'], ok: false });
});

test('a member the round synced reads its session', async () => {
	const l = leader((l) => l.done('p', report));
	assert.deepEqual(await l.said(), { lines: ['syncing with phone', 'phone: sent 1, got 0, conflicts 0, verify ok'], ok: true });
});

test('a member whose dial failed this round reads its error, over an earlier good session', async () => {
	const l = leader((l) => {
		l.errors.set('p', 'offline');
		l.erred.set('p', Date.now());
	});
	l.done('p', report);
	l.seen.set('p', Date.now() - 60_000);
	assert.deepEqual(await l.said(), { lines: ['syncing with phone', 'phone: offline'], ok: false });
});

test('before the setup is read, a read that failed is the answer', async () => {
	const l = leader(() => assert.fail('dialed with no setup read'));
	l.read = false;
	l.reading = Promise.reject(new Error('the read failed'));
	await assert.rejects(l.said(), /the read failed/);
});
