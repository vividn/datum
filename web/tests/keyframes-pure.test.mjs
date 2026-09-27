// The input line's key frames (da-31d1d), in node: which keys a line has
// frames for and where each stands, what a space at the end writes, and
// what Enter adds before the line runs. The frames as they are DRAWN, and
// a reader typing into them, are `keyframes.test.mjs`.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource(import.meta.url);
const { advance, complete, framesOf } = await import('../src/lib/keyframes.ts');
const { keysOf } = await import('../src/lib/fields.ts');
const { shellSpans, shellSplit } = await import('../src/lib/shellSplit.ts');

/** tea: one required key, then one optional with a default. */
const FIELDS = [{ field: 'tea', keys: keysOf({ data_keys: ['amount', 'cups=1'] }) }];

describe('keysOf', () => {
	test('the declared order: -K keys, -k keys, subfields; a default makes a key optional', () => {
		assert.deepEqual(
			keysOf({
				id_keys: ['lang'],
				data_keys: ['n=1', 'lang'],
				subfields: { src: { required: true, default: null }, kind: { required: true, default: 'verb' }, note: {} }
			}),
			[
				{ name: 'lang', required: true },
				{ name: 'n', default: '1', required: false },
				{ name: 'src', required: true },
				{ name: 'kind', default: 'verb', required: false },
				{ name: 'note', required: false }
			]
		);
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
	test('one frame per key, in order, written where a token carries it', () => {
		const line = 'occur tea cups=2';
		const frames = framesOf(line, FIELDS);
		assert.deepEqual(frames.map((f) => [f.name, f.required, f.value]), [['amount', true, undefined], ['cups', false, '2']]);
		assert.equal(line.slice(frames[1].from, frames[1].to), 'cups=2');
	});

	test('none for a field with no keys, an unknown field, or another verb', () => {
		for (const line of ['occur coffee', 'occur', 'switch tea x', 'tail tea']) assert.equal(framesOf(line, FIELDS), null, line);
	});

	// da-o4jaa, vividn: «generally speaking I don't save any extra_data in this one».
	test('none after end; start keeps them', () => {
		assert.equal(framesOf('end tea', FIELDS), null);
		assert.deepEqual(framesOf('start tea', FIELDS).map((f) => f.name), ['amount', 'cups']);
	});
});

describe('advance: a space at the end', () => {
	test('writes the next key after the field, and after each value', () => {
		assert.equal(advance('occur tea', FIELDS), 'occur tea amount=');
		assert.equal(advance('occur tea ', FIELDS), 'occur tea amount=');
		assert.equal(advance('occur tea amount=3', FIELDS), 'occur tea amount=3 cups=');
	});

	test('in an empty frame, skips it for the next one', () => {
		assert.equal(advance('occur tea amount=', FIELDS), 'occur tea cups=');
		assert.equal(advance('occur tea cups=', FIELDS), 'occur tea amount=');
	});

	test('is only a space inside quotes, with every key written, or with no frames', () => {
		assert.equal(advance('occur tea amount="a', FIELDS), null);
		assert.equal(advance('occur tea amount=3 cups=2', FIELDS), null);
		assert.equal(advance('occur coffee', FIELDS), null);
	});
});

describe('complete: the line as Enter runs it', () => {
	test('an optional key left unwritten or empty gets its default, on the line', () => {
		assert.equal(complete('occur tea amount=3', FIELDS), 'occur tea amount=3 cups=1');
		assert.equal(complete('occur tea amount=3 cups=', FIELDS), 'occur tea amount=3 cups=1');
		assert.equal(complete('occur tea cups= amount=3', FIELDS), 'occur tea cups=1 amount=3');
	});

	test('a written key, a required one, and a line with no frames are left as they are', () => {
		assert.equal(complete('occur tea amount=3 cups=2', FIELDS), 'occur tea amount=3 cups=2');
		assert.equal(complete('occur tea', FIELDS), 'occur tea cups=1');
		assert.equal(complete('occur coffee', FIELDS), 'occur coffee');
	});

	test('end gets no defaults, and a key typed on it runs as typed; start still gets them', () => {
		assert.equal(complete('end tea', FIELDS), 'end tea');
		assert.equal(complete('end tea cups=', FIELDS), 'end tea cups=');
		assert.equal(complete('end tea amount=x', FIELDS), 'end tea amount=x');
		assert.equal(complete('start tea amount=3', FIELDS), 'start tea amount=3 cups=1');
	});

	test('a line ending inside an open quote runs as typed', () => {
		assert.equal(complete('occur tea amount="a', FIELDS), 'occur tea amount="a');
	});

	test('a default that would split is quoted', () => {
		const fields = [{ field: 'tea', keys: keysOf({ data_keys: ["cup=my mug's"] }) }];
		const line = complete('occur tea', fields);
		assert.deepEqual(shellSplit(line), ['occur', 'tea', "cup=my mug's"]);
	});
});
