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
const { cursorArg, dateLabel, pillSections, timeArg, timeArgAt } = await import('../src/lib/cursor.ts');

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

// da-fir4a. The CLI reads a clock time the day has twice at its FIRST
// occurrence, so the second has to say which it is, or the entry lands an
// hour before the line it was drawn at.
describe('a clock time the day reads twice', () => {
	/** Run `fn` with the page in `zone`: node re-reads `TZ` when it is set. */
	const inZone = (zone, fn) => {
		const was = process.env.TZ;
		process.env.TZ = zone;
		try {
			fn();
		} finally {
			if (was === undefined) delete process.env.TZ;
			else process.env.TZ = was;
		}
	};

	test('Berlin, 2026-10-25: the first 02:30 is bare and the second says +01:00', () =>
		inZone('Europe/Berlin', () => {
			const first = new Date('2026-10-25T00:30Z');
			const second = new Date('2026-10-25T01:30Z');
			const that = new Date(2026, 9, 25, 12, 0);
			const later = new Date(2026, 9, 27, 12, 0);

			assert.equal(timeArg(first, that), '02:30');
			assert.equal(timeArg(first, later), '2026-10-25T02:30');
			// Dated even on the day itself: the CLI takes an offset on a date-time alone.
			assert.equal(timeArg(second, that), '2026-10-25T02:30+01:00');
			assert.equal(timeArg(second, later), '2026-10-25T02:30+01:00');

			// Both edges of a range dragged on the strip, each as its own instant.
			const range = { at: second, span: 40, from: 'daystrip' };
			assert.equal(cursorArg(range, later), 'end_time=2026-10-25T02:30+01:00');
			assert.equal(timeArg(new Date(+second - 40 * 60_000), later), '2026-10-25T02:50');

			// The face tells the lanes apart, and the date pressed away leaves a bare clock.
			assert.deepEqual(pillSections(first, that), [{ text: '02:30' }]);
			assert.deepEqual(pillSections(second, that), [{ text: '02:30+01:00' }]);
			assert.deepEqual(pillSections(second, later), [
				{ text: '-yy', rest: { text: '-t 02:30', sections: [{ text: '02:30' }] } },
				{ text: '02:30+01:00' }
			]);
		}));

	// Every minute of the night the clocks go back, where the offset is
	// zero, negative, and half an hour: what is spelled reads back as itself.
	for (const [zone, night, offset] of [
		['Europe/Berlin', '2026-10-25T00:00Z', '+01:00'],
		['Europe/London', '2026-10-25T00:00Z', '+00:00'],
		['America/New_York', '2026-11-01T05:00Z', '-05:00'],
		['Australia/Lord_Howe', '2026-04-04T14:30Z', '+10:30']
	])
		test(`${zone}: every minute reads back as the instant it was spelled from`, () =>
			inZone(zone, () => {
				const now = new Date(Date.parse(night) + 12 * 3_600_000);
				const said = new Set();
				for (let t = Date.parse(night) - 3 * 3_600_000; t < Date.parse(night) + 3 * 3_600_000; t += 60_000) {
					const arg = timeArg(new Date(t), now);
					assert.equal(timeArgAt(arg, now)?.getTime(), t, `${arg} for ${new Date(t).toISOString()}`);
					if (/[+-]\d\d:\d\d$/.test(arg)) said.add(arg.slice(-6));
				}
				// And the repeated stretch was in the window: its offset was said.
				assert.deepEqual([...said], [offset]);
			}));
});
