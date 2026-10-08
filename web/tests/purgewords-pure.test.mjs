// The purge confirm's words, in node (da-sivjb): a device that signed only
// definitions loses them too, so its confirm must not say «only revokes».

import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource(import.meta.url);
const { purgeWords } = await import('../src/lib/syncView.ts');

const none = { created: 0, edited: 0, deleted: 0, first: null, last: null };

test('a device that signed nothing is only revoked', () => {
	assert.match(purgeWords('phone', 'home', { ...none, defined: 0 }), /It made no change to an entry: this only revokes it\./);
});

test('a device that signed only definitions is told they go', () => {
	assert.match(purgeWords('phone', 'home', { ...none, defined: 1 }), /only 1 definition: this revokes it and sets that aside\./);
	assert.match(purgeWords('phone', 'home', { ...none, defined: 3 }), /only 3 definitions: this revokes it and sets them aside\./);
});

// da-t0acg: as the CLI's preview, a zero count says nothing and definitions are named.
const span = { first: '2026-10-07T10:00:00Z', last: '2026-10-08T10:00:00Z' };
const changes = (created, edited, deleted, defined) =>
	purgeWords('phone', 'home', { created, edited, deleted, defined, ...span }).match(/device: (.*), 2026-10-07 to 2026-10-08\./)[1];

test('a device that changed entries names only the counts it has, definitions too', () => {
	assert.equal(changes(1, 1, 1, 0), '1 entry it created, 1 it edited and 1 it deleted');
	assert.equal(changes(2, 0, 0, 0), '2 entries it created');
	assert.equal(changes(0, 1, 0, 0), '1 entry it edited');
	assert.equal(changes(0, 0, 3, undefined), '3 entries it deleted');
	assert.equal(changes(1, 0, 1, 0), '1 entry it created and 1 it deleted');
	assert.equal(changes(1, 0, 0, 2), '1 entry it created and 2 definitions it signed');
	assert.equal(changes(0, 2, 1, 1), '2 entries it edited, 1 it deleted and 1 definition it signed');
});
