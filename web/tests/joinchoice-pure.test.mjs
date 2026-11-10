// The join card's database choice, its words alone (da-bedyz): a new
// database's name suggested from the inviter's label and refused where it
// cannot be one, and what joining says for the database chosen.

import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { joinWords, newDbName, newDbRefusal } = await import('../src/lib/syncView.ts');

const about = { from: 'Laptop (Home)', chain: 'c0ffee12345678', expires: 0 };
const NOW = Date.UTC(2026, 10, 10);

test('a new database is named after the inviter, as a name none here has', () => {
	assert.equal(newDbName('Laptop (Home)', ['datum']), 'laptop-home');
	assert.equal(newDbName('Laptop (Home)', ['laptop-home', 'laptop-home-2']), 'laptop-home-3');
	assert.equal(newDbName(undefined, []), 'shared');
	assert.equal(newDbName('…', ['shared']), 'shared-2');
	for (const from of ['a/b', '.hidden', 'x.db', '_internal']) assert.equal(newDbRefusal(newDbName(from, []), []), undefined, from);
});

test('a new database’s name is refused where it is none, or names one here', () => {
	assert.match(newDbRefusal('', ['datum']), /Name the new database/);
	for (const name of ['a/b', '.x', 'x.db', '_internal']) assert.match(newDbRefusal(name, []), /cannot be a database’s name/, name);
	assert.match(newDbRefusal('datum', ['datum']), /«datum» is here already: choose it from the list/);
	assert.equal(newDbRefusal('friends', ['datum']), undefined);
});

test('the words follow the database chosen: a new one, one of this browser’s, one in another chain', () => {
	const fresh = joinWords(about, 'laptop-home', undefined, NOW, false, true);
	assert.match(fresh.words[0], /with a new database «laptop-home» in this browser/);
	assert.equal(fresh.refused, undefined);

	const mine = joinWords(about, 'datum', { db: 'datum', chain: null, members: [] }, NOW);
	assert.match(mine.words[0], /with this browser’s database «datum»: what each holds, both will/);
	assert.equal(mine.refused, undefined);

	const taken = joinWords(about, 'work', { db: 'work', chain: 'beef', members: [] }, NOW, false);
	assert.match(taken.refused, /«work» is in another sync chain/);
	// The card offers the choice, so the refusal no longer sends anyone to edit the link.
	assert.doesNotMatch(taken.refused, /\?db=/);
});
