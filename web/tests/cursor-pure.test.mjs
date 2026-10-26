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
const { cursor, cursorArg, dateLabel, pillSections, pointAt, timeArg, timeArgAt } = await import('../src/lib/cursor.ts');
const { get } = await import('svelte/store');

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

// da-9f8ne. A nowview window of a few minutes says its taps to five
// seconds (da-q4z88, `tapStep`), panned off now as much as on it, so the
// absolute `-t` says seconds the CLI reads: `-t 14:03:20` («datum help
// time»: «-t 15:13:45  with seconds»), dated and with an offset alike.
describe('an instant off a whole minute', () => {
	const now = new Date(2026, 8, 10, 14, 5);

	test('-t says its seconds, today and dated, and reads back as itself', () => {
		const today = new Date(2026, 8, 10, 14, 3, 20);
		const before = new Date(2026, 8, 9, 9, 0, 5);
		assert.equal(timeArg(today, now), '14:03:20');
		assert.equal(timeArg(before, now), '2026-09-09T09:00:05');
		assert.equal(timeArg(new Date(2026, 8, 10, 14, 3), now), '14:03', 'a whole minute says none');
		for (const at of [today, before]) assert.equal(timeArgAt(timeArg(at, now), now)?.getTime(), +at);
	});

	test('the pill says them, and its date pressed away leaves them', () =>
		assert.deepEqual(pillSections(new Date(2026, 8, 9, 9, 0, 5), now), [
			{ text: '-y', rest: { text: '-t 09:00:05', sections: [{ text: '09:00:05' }] } },
			{ text: '09:00:05' }
		]));

	test("a selection is to the view's step, and to the minute where it says none", () => {
		const tapped = new Date(2026, 8, 10, 14, 3, 21, 400);
		pointAt({ at: tapped, step: 5, from: 'nowview' });
		assert.equal(cursorArg(get(cursor), now), '-t 14:03:20');
		pointAt({ at: tapped, step: 60, from: 'nowview' });
		assert.equal(cursorArg(get(cursor), now), '-t 14:03');
		pointAt({ at: tapped, from: 'daystrip' });
		assert.equal(cursorArg(get(cursor), now), '-t 14:03', 'the day strip and the dayview, as before');
		pointAt(null);
	});

	// vremya on PR #1048: `back` floored now to the minute, so a `-t -20s`
	// taken at 14:05:40 was drawn at 14:04:40 — a whole minute is floored
	// still, since its pill is whole minutes back from whenever it runs.
	test("a relative selection's instant is that long before now, to the second", () => {
		const real = Date.now;
		Date.now = () => +new Date(2026, 8, 10, 14, 5, 40);
		try {
			pointAt({ rel: 20 / 60, from: 'nowview' });
			assert.equal(+get(cursor).at, +new Date(2026, 8, 10, 14, 5, 20));
			pointAt({ rel: 7, from: 'nowview' });
			assert.equal(+get(cursor).at, +new Date(2026, 8, 10, 13, 58));
		} finally {
			Date.now = real;
			pointAt(null);
		}
	});
});

// da-fir4a. The CLI asks which of a clock time the day has twice is meant
// (da-6i7l1), so each says which it is, or a pill runs into a question.
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

	test('Berlin, 2026-10-25: the first 02:30 says +02:00 and the second +01:00', () =>
		inZone('Europe/Berlin', () => {
			const first = new Date('2026-10-25T00:30Z');
			const second = new Date('2026-10-25T01:30Z');
			const that = new Date(2026, 9, 25, 12, 0);
			const later = new Date(2026, 9, 27, 12, 0);

			assert.equal(timeArg(first, that), '2026-10-25T02:30+02:00');
			assert.equal(timeArg(first, later), '2026-10-25T02:30+02:00');
			// Dated even on the day itself: the CLI takes an offset on a date-time alone.
			assert.equal(timeArg(second, that), '2026-10-25T02:30+01:00');
			assert.equal(timeArg(second, later), '2026-10-25T02:30+01:00');

			// Both edges of a range dragged on the strip, each as its own instant.
			const range = { at: second, span: 40, from: 'daystrip' };
			assert.equal(cursorArg(range, later), 'end_time=2026-10-25T02:30+01:00');
			assert.equal(timeArg(new Date(+second - 40 * 60_000), later), '2026-10-25T02:50+02:00');

			// The face tells the lanes apart, and the date pressed away leaves a bare clock.
			assert.deepEqual(pillSections(first, that), [{ text: '02:30+02:00' }]);
			assert.deepEqual(pillSections(second, that), [{ text: '02:30+01:00' }]);
			assert.deepEqual(pillSections(second, later), [
				{ text: '-yy', rest: { text: '-t 02:30', sections: [{ text: '02:30' }] } },
				{ text: '02:30+01:00' }
			]);
		}));

	// Every minute of the night the clocks go back, where the offset is
	// zero, negative, and half an hour: what is spelled reads back as itself.
	for (const [zone, night, offsets] of [
		['Europe/Berlin', '2026-10-25T00:00Z', ['+02:00', '+01:00']],
		['Europe/London', '2026-10-25T00:00Z', ['+01:00', '+00:00']],
		['America/New_York', '2026-11-01T05:00Z', ['-04:00', '-05:00']],
		['Australia/Lord_Howe', '2026-04-04T14:30Z', ['+11:00', '+10:30']]
	])
		test(`${zone}: every minute reads back as the instant it was spelled from`, () =>
			inZone(zone, () => {
				const now = new Date(Date.parse(night) + 12 * 3_600_000);
				const said = new Set();
				for (let t = Date.parse(night) - 3 * 3_600_000; t < Date.parse(night) + 3 * 3_600_000; t += 60_000) {
					// And 20 s on, which says its seconds before the offset.
					for (const at of [t, t + 20_000]) {
						const arg = timeArg(new Date(at), now);
						assert.equal(timeArgAt(arg, now)?.getTime(), at, `${arg} for ${new Date(at).toISOString()}`);
					}
					const arg = timeArg(new Date(t), now);
					if (/[+-]\d\d:\d\d$/.test(arg)) said.add(arg.slice(-6));
				}
				// And the repeated stretch was in the window: both its offsets were said.
				assert.deepEqual([...said], offsets);
			}));
});
