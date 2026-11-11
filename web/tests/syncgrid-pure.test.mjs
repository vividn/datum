// The grid's words alone (da-dxng1): the time since a device last synced,
// in one unit, and why a cell's acts are grayed for this device's role;
// and which databases a device's menu reaches, and why not the rest (da-jlyyr).

import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { ago, barred, cellActs, deviceActs, expelWords, loginCode } = await import('../src/lib/syncView.ts');

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
	// A writer writes no chain record (da-o5o6m): who syncs is a member's to change.
	assert.match(barred(d({ this: { reader: false, role: 'writer' } })), /only logs to home/);
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
	// A writer (da-o5o6m) is in it: made a member or a reader, or removed.
	assert.deepEqual(cellActs('writer', false), ['member', 'reader', 'expel', 'unjoin', 'wipe']);
});

test('a device’s menu renames and removes it where it is in, adds it where it is not, and says why it leaves any out (da-jlyyr)', () => {
	const chains = [
		{ chain: 'c-home', db: 'home', held: true },
		{ chain: 'c-work', db: 'work', held: true },
		{ chain: 'c-far', db: 'far', held: false }
	];
	const db = (name, chain, more) => ({ db: name, chain, members: [], this: chain ? { reader: false } : undefined, admins: [], ...more });
	const phone = (cells, more) => ({ device: 'dev', endpoint: null, label: 'phone', this: false, cells, ...more });
	const cell = (state, endpoint) => ({ state, endpoint, apart: null });
	const dbs = [db('home', 'c-home'), db('work', 'c-work'), db('notes', null)];
	const cells = { 'c-home': cell('member', 'e1'), 'c-work': null, 'c-far': cell('member', 'e9') };

	const open = deviceActs(phone(cells), chains, dbs);
	// Where it is in and this device holds; never a chain held nowhere here.
	assert.deepEqual(open.change, [{ db: 'home', endpoint: 'e1' }]);
	assert.equal(open.changeWhy, '');
	// Every other database here, one in no chain too, which starts its chain.
	assert.deepEqual(open.add, [{ db: 'work', chain: 'c-work' }, { db: 'notes', chain: null }]);
	assert.equal(open.addWhy, '');
	// An invite waiting is not made again; an expelled device is in nothing.
	assert.deepEqual(deviceActs(phone(cells, { grants: { 'c-work': { state: 'waiting' } } }), chains, dbs).add, [{ db: 'notes', chain: null }]);
	const out = deviceActs(phone({ 'c-home': cell('expelled', 'e1') }), chains, dbs);
	assert.deepEqual(out.change, []);
	assert.match(out.changeWhy, /^phone is in no database here yet/);
	assert.match(out.addWhy, /nothing carries an invite/);

	// A reader of what it shares with the device changes nothing, and carries no invite.
	const reads = deviceActs(phone(cells), chains, [db('home', 'c-home', { this: { reader: true } }), db('notes', null)]);
	assert.deepEqual([reads.change, reads.add], [[], []]);
	assert.match(reads.changeWhy, /only reads home/);
	assert.match(reads.addWhy, /^This device only reads what it shares with phone/);

	// A chain locked by a key file: left out and said; the rest goes on.
	const file = { key: `ed25519:${'ab'.repeat(32)}`, label: 'laptop key' };
	const locked = deviceActs(phone({ ...cells, 'c-work': cell('member', 'e2') }), chains, [db('home', 'c-home'), db('work', 'c-work', { admins: [file] }), db('notes', null)]);
	assert.deepEqual(locked.change, [{ db: 'home', endpoint: 'e1' }]);
	assert.match(locked.changeWhy, /work’s chain is locked: only its admin \(laptop key\)/);
	// In everything already, and no device key to seal an invite to.
	assert.match(deviceActs(phone(cells), chains, [db('home', 'c-home')]).addWhy, /already syncs every database here/);
	assert.match(deviceActs(phone(cells, { device: null }), chains, dbs).addWhy, /no device key/);
});

test('removing a device names each database and points a lost one to Unjoin (da-fstz5)', () => {
	assert.match(expelWords('phone', ['home']), /^Expel phone from home\? It can no longer sync home until/);
	assert.match(expelWords('phone', ['home', 'work', 'notes']), /^Expel phone from home, work and notes\? It can no longer sync them until/);
	assert.match(expelWords('phone', ['home']), /Unjoin it instead/);
});

test('a login link’s code is its device key’s first 16 hex characters, in groups of 4 (da-ntp5c)', () => {
	const A = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
	const json = JSON.stringify({ addr: { id: `8a88e3dd7409f195${'ab'.repeat(24)}` }, nonce: 'n' });
	const bits = [...new TextEncoder().encode(json)].map((b) => b.toString(2).padStart(8, '0')).join('');
	const base32 = (bits.match(/.{1,5}/g) ?? []).map((b) => A[parseInt(b.padEnd(5, '0'), 2)]).join('');
	assert.equal(loginCode(`https://datum.day/sync/login#${base32}`), '8a88 e3dd 7409 f195');
	assert.equal(loginCode('https://datum.day/sync/login#not-a-login'), '');
});
