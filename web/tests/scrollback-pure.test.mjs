// How many entries stand side by side, and where the seams fall, in node
// (da-5iyo).
//
// The widths themselves are measured against the built app
// (`scrollback.test.mjs`); this file is about what the two numbers MEAN —
// the rule "as many as fit at the width an entry wants, and never fewer
// than two where two can be read", and the row each entry falls on when
// an entry the columns cannot carry takes a whole one to itself.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';

registerHooks({
	resolve(specifier, context, next) {
		if (/^\.\/.*\.js$/.test(specifier) && context.parentURL?.includes('/src/lib/'))
			return next(specifier.replace(/\.js$/, '.ts'), context);
		return next(specifier, context);
	}
});

const { GAP, MIN, WANT, columnsIn, rowsOf } = await import('../src/lib/scrollback.ts');

describe('how many columns a box holds', () => {
	test('is one until two of them can be read', () => {
		assert.equal(columnsIn(2 * MIN + GAP - 1), 1);
		assert.equal(columnsIn(2 * MIN + GAP), 2);
	});

	test('is two for as long as two is all that fits at the width an entry wants', () => {
		assert.equal(columnsIn(2 * MIN + GAP), 2);
		assert.equal(columnsIn(2 * WANT + GAP), 2);
		assert.equal(columnsIn(3 * WANT + 2 * GAP - 1), 2);
	});

	test('is as many as fit at that width once there is room for three', () => {
		assert.equal(columnsIn(3 * WANT + 2 * GAP), 3);
		assert.equal(columnsIn(4 * WANT + 3 * GAP), 4);
	});

	test('is one for a box too narrow, and for a box not measured yet', () => {
		assert.equal(columnsIn(MIN), 1);
		assert.equal(columnsIn(0), 1);
		assert.equal(columnsIn(null), 1);
	});
});

describe('the row an entry falls on', () => {
	/** Every entry an action echo, which is what a column carries. */
	const echoes = (n) => new Array(n).fill(false);

	test('is the list read across, in one column and in several', () => {
		assert.deepEqual(rowsOf(echoes(4), 1), [0, 1, 2, 3]);
		assert.deepEqual(rowsOf(echoes(5), 2), [0, 0, 1, 1, 2]);
		assert.deepEqual(rowsOf(echoes(5), 3), [0, 0, 0, 1, 1]);
	});

	test('gives a table, a picture or an error a row of its own', () => {
		// A `tail` in the middle: it ends the row it lands in, and the
		// entries after it start a fresh one.
		assert.deepEqual(rowsOf([false, true, false, false], 2), [0, 1, 2, 2]);
		// And at the head of a row it does not need to break one first.
		assert.deepEqual(rowsOf([false, false, true, false], 2), [0, 0, 1, 2]);
		assert.deepEqual(rowsOf([true, true], 2), [0, 1]);
	});

	test('is the seam: every entry past the first row has one above it', () => {
		// n rows of entries have n-1 seams between them, wherever the
		// columns divide them — which is what `CliScrollback.svelte` draws the
		// hairline from (da-qdm7).
		const rows = rowsOf([false, false, true, false, false], 2);
		assert.deepEqual(
			rows.map((row) => row > 0),
			[false, false, true, true, true]
		);
	});

	test('is nothing at all for an empty scrollback', () => {
		assert.deepEqual(rowsOf([], 2), []);
	});
});
