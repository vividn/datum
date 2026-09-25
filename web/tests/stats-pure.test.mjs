// The stats widget's two pure answers, in node (da-vpuxg).
//
// `summaryTiles` is what the widget says when nothing is selected, and
// `formatDur` is the spelling it says the time in. The tiles are asserted
// against the real CLI's real numbers in `stats.test.mjs`; this file is
// about the two things a browser cannot check for you:
//
//  · that `formatDur` is the MIRROR it claims to be. The duration in a
//    tile sits directly over the same duration in the CLI's own text, so
//    a divergence is two spellings of one number on one screen. The cases
//    are `format_human_duration`'s own, copied from its test
//    (`crates/datum-parse/src/time/dur.rs`) — every boundary of it, not
//    the one a seeded database happens to produce.
//  · that the tiles come out of the CLI's JSON untouched.

import test, { describe } from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource(import.meta.url);
const { formatDur, summaryTiles, windowLabel } = await import('../src/lib/stats.ts');

describe('a span in the CLI’s own spelling', () => {
	// `human_duration_names_two_units_unpadded`, case for case.
	test('two units, never three, and seconds fall off above the hour', () => {
		assert.equal(formatDur(0), '0s');
		assert.equal(formatDur(45), '45s');
		assert.equal(formatDur(125), '2m5s');
		assert.equal(formatDur(240), '4m');
		assert.equal(formatDur(3600), '1h');
		assert.equal(formatDur(3725), '1h2m');
		assert.equal(formatDur(93_600), '26h');
		assert.equal(formatDur(-125), '2m5s');
	});
});

describe('the tiles when nothing is selected', () => {
	const stats = {
		fields: [
			{ field: 'work', color: '#aabbcc', count: 4, duration_seconds: 7200, ratio: 0.8 },
			{ field: 'coffee', color: '#ccbbaa', count: 2, duration_seconds: 0, ratio: 0 }
		],
		total_count: 6,
		tracked_duration_seconds: 9000,
		earliest: null,
		latest: null
	};

	test('are the JSON’s own numbers: its entries, its tracked time, its rows', () => {
		assert.deepEqual(summaryTiles(stats), [
			['entries', '6'],
			['tracked', '2h30m'],
			['fields', '2']
		]);
	});

	// A database with nothing in the window is the widget working, so it
	// still answers — with zeroes, which is the true answer.
	test('an empty window is three zeroes, not three blanks', () => {
		assert.deepEqual(summaryTiles({ fields: [], total_count: 0, tracked_duration_seconds: 0, earliest: null, latest: null }), [
			['entries', '0'],
			['tracked', '0s'],
			['fields', '0']
		]);
	});
});

describe('since when', () => {
	const at = (earliest, latest) => summaryTiles({ fields: [], total_count: 1, tracked_duration_seconds: 0, earliest, latest })[3];

	// The day the entry was logged on, in ITS zone: 23:30 at -05:00 is
	// already the 5th in UTC, and a `Date` would say so.
	test('is the first counted entry’s own day, read off the string', () => {
		assert.deepEqual(at('2026-09-04T23:30:00-05:00', '2026-09-24T08:00:00-05:00'), ['since', '4 Sep']);
	});

	test('says the year only when the window ends in another one', () => {
		assert.deepEqual(at('2025-12-31T09:00:00+01:00', '2026-01-02T09:00:00+01:00'), ['since', '31 Dec 2025']);
	});

	test('is no tile when the window counts nothing', () => {
		assert.equal(at(null, null), undefined);
	});
});

describe('the window in words', () => {
	test('every named window has one', () => {
		assert.deepEqual(
			['all', 'today', 'week', 'month'].map(windowLabel),
			['all time', 'today', 'this week', 'this month']
		);
	});

	test('a custom window says its edges, and neither edge is all time again', () => {
		assert.equal(windowLabel({ since: '2026-01-01T00:00', until: '' }), 'since 2026-01-01T00:00');
		assert.equal(windowLabel({ since: '', until: '2026-02-01T00:00' }), 'until 2026-02-01T00:00');
		assert.equal(
			windowLabel({ since: '2026-01-01T00:00', until: '2026-02-01T00:00' }),
			'2026-01-01T00:00 → 2026-02-01T00:00'
		);
		assert.equal(windowLabel({ since: '', until: '' }), 'all time');
	});
});
