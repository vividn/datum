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
