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
