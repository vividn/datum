// The cursor's argv and its date label, in node (da-mgxdt).
//
// `cursor.ts` reaches `$app/environment` through `bar.ts` → `layout/engine.ts`
// → `db.ts` → `changes.ts`, so it could not be imported here until
// `libhook.mjs` stubbed it. What this file checks is what `cursor.test.mjs`
// cannot put a clock on cheaply: the boundaries of `dateLabel`, and the
// three spellings of `cursorArg`. The five ordinary days, as a reader
// meets them in the pill, stay in `cursor.test.mjs`.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource(import.meta.url);
const { cursorArg, dateLabel, timeArg } = await import('../src/lib/cursor.ts');

describe('the date a pill says', () => {
	test('-y and -yy count calendar days, across a new year before the year is spelled', () => {
		const newYear = new Date(2026, 0, 1, 0, 5);
		assert.equal(dateLabel(new Date(2025, 11, 31, 23, 55), newYear), '-y');
		assert.equal(dateLabel(new Date(2025, 11, 30, 9, 0), newYear), '-yy');
		assert.equal(dateLabel(new Date(2025, 11, 29, 9, 0), newYear), '2025-12-29');
	});

	test('today says nothing, a later day this year is short, a later year is whole', () => {
		const now = new Date(2026, 8, 10, 14, 5);
		assert.equal(dateLabel(new Date(2026, 8, 10, 0, 0), now), null);
		assert.equal(dateLabel(new Date(2026, 8, 11, 9, 0), now), 'sep11');
		assert.equal(dateLabel(new Date(2027, 0, 2, 9, 0), now), '2027-01-02');
	});
});

describe('the argv a selection puts on the line', () => {
	const now = new Date(2026, 8, 10, 14, 5);
	const at = new Date(2026, 8, 9, 9, 0);

	test('an instant is -t, dated off today', () => {
		assert.equal(timeArg(new Date(2026, 8, 10, 9, 0), now), '09:00');
		assert.equal(cursorArg({ at }, now), '-t 2026-09-09T09:00');
	});

	test('relative is minutes back, and 0 is now, which needs no argv', () => {
		assert.equal(cursorArg({ at, rel: 10 }, now), '-qq');
		assert.equal(cursorArg({ at, rel: 7 }, now), '-t -7');
		assert.equal(cursorArg({ at, rel: 0, span: 30 }, now), null);
	});

	test('a range dragged on the strip says its end as end_time=', () => {
		assert.equal(cursorArg({ at, span: 90, from: 'daystrip' }, now), 'end_time=2026-09-09T09:00');
		assert.equal(cursorArg({ at, span: 90, from: 'nowview' }, now), '-t 2026-09-09T09:00');
	});
});
