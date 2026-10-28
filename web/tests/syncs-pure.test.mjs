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
});
