// Whether a database syncs is read from `sync chain list --json`'s answer,
// not its exit code: in no chain it answers `chain: null` (da-y6oh6), and a
// boot that took that for a member would ask `sync status`, which mints an
// endpoint key on every fresh device.

import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { syncs } = await import('../src/lib/sync.ts');

const answer = (out, code = 0) => ({ argv: [], code, stdout: JSON.stringify(out), stderr: '' });

test('only a copy in a chain, joining one or passing on a dissolved one syncs', () => {
	const none = { chain: null, admins: [], members: [] };
	assert.equal(syncs(answer(none)), false, 'in no chain');
	assert.equal(syncs(answer({ ...none, joining: [] })), false, 'in no chain, an empty join named');
	assert.equal(syncs({ argv: [], code: 1, stdout: '', stderr: 'Error: …' }), false, 'a failed read');
	assert.equal(syncs(answer({ ...none, chain: 'c0ffee' })), true, 'a member');
	assert.equal(syncs(answer({ ...none, joining: [{ endpoint: 'e', label: 'a' }] })), true, 'joining');
	assert.equal(syncs(answer({ ...none, dissolved: 'c0ffee' })), true, 'dissolved');
	// A join kept until a member admits it syncs; one a member refused for good has nothing to try (da-4qpwf).
	const pending = { from: 'laptop', chain: 'c0ffee', expires: 1, since: '2026-11-10T20:00:00Z' };
	assert.equal(syncs(answer({ ...none, pending_join: pending })), true, 'a pending join');
	assert.equal(syncs(answer({ ...none, pending_join: { ...pending, refused: 'invite used' } })), false, 'a refused one');
});

test('a pending join says it waits for its first sync, or why it was refused and what to do (da-4qpwf)', async () => {
	const { pendingWords } = await import('../src/lib/sync.ts');
	const p = { from: 'laptop', since: '2026-11-10T20:00:00Z' };
	assert.match(pendingWords(p), /waiting for its first sync: it starts when laptop or another device in the chain is online/);
	const refused = pendingWords({ ...p, refused: 'invite expired' });
	assert.match(refused, /^Could not join laptop’s sync chain: invite expired\. Ask laptop for a new link\.$/);
});

test('a page dials the member seen or synced last first, and one its last try never reached last (da-612uv)', async () => {
	const { likeliest } = await import('../src/lib/sync.ts');
	const m = (endpoint, state, last_synced, last_seen) => ({ endpoint, state, last_synced, last_seen });
	const order = (ms) => likeliest(ms).map((x) => x.endpoint);
	const joined = [
		m('first', 'synced', '2026-11-14T08:00:00Z', '2026-11-14T08:00:00Z'),
		m('down', 'offline', '2026-11-15T09:00:00Z', '2026-11-15T09:00:00Z'),
		m('never', 'never', null, null),
		m('seen', 'error', '2026-11-13T08:00:00Z', '2026-11-15T08:00:00Z'),
		m('last', 'synced', '2026-11-15T07:00:00Z', null)
	];
	assert.deepEqual(order(joined), ['seen', 'last', 'first', 'never', 'down']);
	assert.deepEqual(joined.map((x) => x.endpoint), ['first', 'down', 'never', 'seen', 'last'], 'the list itself is left as it was');
});
