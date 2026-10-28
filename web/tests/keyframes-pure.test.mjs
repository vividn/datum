// The input line's key frames (da-31d1d), in node: which keys a line has
// frames for and where each stands, what a space at the end writes, and
// what Enter adds before the line runs. The frames as they are DRAWN, and
// a reader typing into them, are `keyframes.test.mjs`.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { advance, complete, framesOf } = await import('../src/lib/keyframes.ts');
const { keysOf, stateKeysOf } = await import('../src/lib/fields.ts');
const { shellSpans, shellSplit } = await import('../src/lib/shellSplit.ts');

/** tea: a required key, an optional one, then one with a default — `setup F --json`'s parsed `keys`. */
const TEA = {
	keys: [
		{ name: 'amount', kind: 'required', in_slug: false },
		{ name: 'size', kind: 'optional', in_slug: false },
		{ name: 'cups', kind: 'default', value: '1', in_slug: false },
		{ name: 'brand', kind: 'assign', value: 'acme', in_slug: false }
	]
};
/** mood: `sad` declares a key of its own (da-cej7q). */
const MOOD = { keys: [], keys_by_state: { sad: [{ name: 'reason', kind: 'required', in_slug: false }] } };
const FIELDS = [
	{ field: 'tea', keys: keysOf(TEA) },
	{ field: 'mood', keys: keysOf(MOOD), stateKeys: stateKeysOf(MOOD) }
];

describe('keysOf', () => {
	// da-p3hxn: the CLI parses the spellings; the web reads its list and splits nothing.
	test("the CLI's list in its order; an assigned key is fixed, and gets no frame", () => {
		assert.deepEqual(keysOf(TEA), [
			{ name: 'amount', required: true },
			{ name: 'size', required: false },
			{ name: 'cups', default: '1', required: false }
		]);
		assert.deepEqual(stateKeysOf(MOOD), { sad: [{ name: 'reason', required: true }] });
	});

	test('a spelling is never split: a spec without the parsed list has no keys', () => {
		assert.deepEqual(keysOf({ data_keys: ['amount', 'cups=1'] }), []);
	});
});

describe('shellSpans', () => {
	test("each token's place in the line, and an open quote at the end", () => {
		const line = 'occur  tea "a b" x\\ y ';
		const { tokens, quoted } = shellSpans(line);
		assert.deepEqual(tokens.map((t) => line.slice(t.from, t.to)), ['occur', 'tea', '"a b"', 'x\\ y']);
		assert.deepEqual(tokens.map((t) => t.text), shellSplit(line));
		assert.equal(quoted, false);
		assert.equal(shellSpans('occur tea note="a ').quoted, true);
	});
});

describe('framesOf', () => {
	test('bare words fill the keys in order, as the CLI binds them', () => {
		const line = 'occur tea 3 2';
		const frames = framesOf(line, FIELDS);
		assert.deepEqual(frames.map((f) => [f.name, f.required, f.value]), [['amount', true, '3'], ['size', false, '2'], ['cups', false, undefined]]);
		assert.equal(line.slice(frames[1].from, frames[1].to), '2');
	});

	test('a named key takes its own frame, and the bare words the rest', () => {
		const frames = framesOf('occur tea cups=5 3', FIELDS);
		assert.deepEqual(frames.map((f) => [f.name, f.value]), [['amount', '3'], ['size', undefined], ['cups', '5']]);
	});

	test("a flag is no key's, nor the word a valued flag takes", () => {
		const frames = framesOf('occur tea -q -t 1400 3', FIELDS, ['-t']);
		assert.deepEqual(frames.map((f) => [f.name, f.value]), [['amount', '3'], ['size', undefined], ['cups', undefined]]);
	});

	test('none for a field with no keys, an unknown field, or another verb', () => {
		for (const line of ['occur coffee', 'occur', 'tail tea', 'switch mood']) assert.equal(framesOf(line, FIELDS), null, line);
	});

	test("a switch frames its state's own keys, after the state", () => {
		assert.deepEqual(framesOf('switch mood sad tired', FIELDS).map((f) => [f.name, f.value]), [['reason', 'tired']]);
		assert.equal(framesOf('switch mood happy', FIELDS), null);
	});

	// da-o4jaa, vividn: «generally speaking I don't save any extra_data in this one».
	test('none after end; start and add keep them', () => {
		assert.equal(framesOf('end tea', FIELDS), null);
		assert.deepEqual(framesOf('start tea', FIELDS).map((f) => f.name), ['amount', 'size', 'cups']);
		assert.deepEqual(framesOf('add tea', FIELDS).map((f) => f.name), ['amount', 'size', 'cups']);
	});
});

describe('advance: a space at the end', () => {
	test('is only a space after a word: the next bare word falls to the next key', () => {
		assert.equal(advance('occur tea', FIELDS), null);
		assert.equal(advance('occur tea 3', FIELDS), null);
	});

	test('in an empty frame, skips it: the key after it is named, as no bare word reaches it', () => {
		assert.equal(advance('occur tea 3 ', FIELDS), 'occur tea 3 cups=');
		assert.equal(advance('occur tea 3 cups=', FIELDS), 'occur tea 3 ');
		assert.equal(advance('occur tea ', FIELDS), 'occur tea size=');
	});

	test('is only a space inside quotes, with every key written, or with no frames', () => {
		assert.equal(advance('occur tea "a ', FIELDS), null);
		assert.equal(advance('occur tea 3 2 1 ', FIELDS), null);
		assert.equal(advance('occur coffee ', FIELDS), null);
	});
});

describe('complete: the line as Enter runs it', () => {
	test('writes no default: the CLI applies it, and an empty key= comes off', () => {
		assert.equal(complete('occur tea 3', FIELDS), 'occur tea 3');
		assert.equal(complete('occur tea 3 cups=', FIELDS), 'occur tea 3');
		assert.equal(complete('occur tea cups= 3', FIELDS), 'occur tea  3');
	});

	test('a written key and a line with no frames are left as they are', () => {
		assert.equal(complete('occur tea 3 cups=2', FIELDS), 'occur tea 3 cups=2');
		assert.equal(complete('occur coffee', FIELDS), 'occur coffee');
	});

	test('end frames nothing, and a key typed on it runs as typed', () => {
		assert.equal(complete('end tea cups=', FIELDS), 'end tea cups=');
		assert.equal(complete('end tea amount=x', FIELDS), 'end tea amount=x');
	});

	test('a line ending inside an open quote runs as typed', () => {
		assert.equal(complete('occur tea cups= amount="a', FIELDS), 'occur tea cups= amount="a');
	});
});
