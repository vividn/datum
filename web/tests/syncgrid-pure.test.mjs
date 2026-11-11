// The grid's words alone (da-dxng1): the time since a device last synced,
// in one unit, and why a cell's acts are grayed for this device's role.

import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { ago, barred, cellActs } = await import('../src/lib/syncView.ts');

const NOW = Date.UTC(2026, 10, 11, 12);
const S = 1000;

test('the time since a last sync is one whole unit: now, minutes, hours, days', () => {
	assert.equal(ago(NOW, NOW), 'now');
	assert.equal(ago(NOW - 59 * S, NOW), 'now');
	assert.equal(ago(NOW - 2 * 60 * S, NOW), '2m');
	assert.equal(ago(NOW - 59 * 60 * S - 59 * S, NOW), '59m');
	assert.equal(ago(NOW - 3 * 3600 * S - 1, NOW), '3h');
	assert.equal(ago(NOW - 4 * 86400 * S, NOW), '4d');
	// A clock behind the copy that synced reads «now», never a negative.
	assert.equal(ago(NOW + 60 * S, NOW), 'now');
});

test('a cell’s acts are grayed for a reader, and in a chain locked by key files alone', () => {
	const d = (more) => ({ db: 'home', this: { reader: false }, admins: [], ...more });
	assert.equal(barred(d()), '', 'an open chain: any writer may');
	assert.match(barred(d({ this: { reader: true } })), /only reads home/);
	const file = { key: `ed25519:${'ab'.repeat(32)}`, label: 'laptop key' };
	assert.match(barred(d({ admins: [file] })), /locked: only its admin \(laptop key\)/);
	// A passkey is asked for as the act runs, and an account signed in here signs itself.
	assert.equal(barred(d({ admins: [file, { key: 'es256:AAAA', label: 'my phone' }] })), '');
	assert.equal(barred(d({ admins: [{ key: 'eddsa:AAAA', label: 'key' }] })), '');
	assert.equal(barred(d({ admins: [file] }), file.key), '');
});

test('a cell offers a device a role and, while it is in, its removal', () => {
	assert.deepEqual(cellActs(undefined, true), ['member', 'reader']);
	assert.deepEqual(cellActs(undefined, false), []);
	assert.deepEqual(cellActs('member', true), ['reader', 'expel', 'unjoin', 'wipe']);
	assert.deepEqual(cellActs('reader', false), ['member', 'expel', 'unjoin', 'wipe']);
});
