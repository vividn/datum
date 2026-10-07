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

import test, { after, before, describe, mock } from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource(import.meta.url);
const { fieldTiles, formatDur, statsArgv, summaryTiles, windowLabel } = await import('../src/lib/stats.ts');

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

	// The year is weighed against the reader's, so the reader's is held.
	before(() => mock.timers.enable({ apis: ['Date'], now: new Date('2026-09-25T12:00:00Z') }));
	after(() => mock.timers.reset());

	// The day the entry was logged on, in ITS zone: 23:30 at -05:00 is
	// already the 5th in UTC, and a `Date` would say so.
	test('is the first counted entry’s own day, read off the string', () => {
		assert.deepEqual(at('2026-09-04T23:30:00-05:00', '2026-09-24T08:00:00-05:00'), ['since', '4 Sep']);
	});

	test('says the year whenever it is not this one', () => {
		assert.deepEqual(at('2025-12-31T09:00:00+01:00', '2026-01-02T09:00:00+01:00'), ['since', '31 Dec 2025']);
	});

	// da-c6htb: a record wholly in a past year — its latest in the same
	// year as its earliest — is not this year's.
	test('says the year of a record wholly in a past year', () => {
		assert.deepEqual(at('2025-09-04T23:30:00-05:00', '2025-09-24T08:00:00-05:00'), ['since', '4 Sep 2025']);
	});

	test('a narrowed field says it the same way', () => {
		const s = { field: 'work', total_count: 1, total_duration_seconds: 0, tracked_duration_seconds: 0, states: [] };
		assert.deepEqual(fieldTiles({ ...s, earliest: '2026-09-04T23:30:00-05:00', latest: '2026-09-24T08:00:00-05:00' })[3], ['since', '4 Sep']);
		assert.deepEqual(fieldTiles({ ...s, earliest: '2025-09-04T23:30:00-05:00', latest: '2025-09-24T08:00:00-05:00' })[3], ['since', '4 Sep 2025']);
	});

	// da-uv7fu: an occur field's "time" is the gaps between its instants.
	test('a field that took no time has no time tile', () => {
		const s = { field: 'coffee', total_count: 3, total_duration_seconds: 226359, tracked_duration_seconds: 0, states: [], earliest: null, latest: null };
		const frequency = { span_seconds: 226359, per_day: 1.1, per_week: 8, per_month: 35, per_year: 420 };
		assert.deepEqual(fieldTiles({ ...s, frequency: null }).map(([k]) => k), ['entries', 'time', 'states']);
		assert.deepEqual(fieldTiles({ ...s, frequency }), [['entries', '3'], ['states', '0']]);
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

// da-80do8: the default window is this month, not all time — every pane
// re-runs after a write, and all time costs the whole history. The argv is
// read through the registry's own `props`, which is what the pane mounts
// with, so a layout's stored settings and the kind's default both count.
describe('the window a pane opens on', () => {
	const statsWidgets = (node) =>
		!node || typeof node !== 'object'
			? []
			: node.kind === 'stats' && node.settings
				? [node.settings]
				: Object.values(node).flatMap(statsWidgets);

	test('a new stats pane, and every layout that ships, runs stats --month', async () => {
		const { KINDS } = await import('../src/lib/layout/registry.ts');
		const { BUILTIN, DEBUG_LAYOUT_TREE } = await import('../src/lib/layout/defaults.ts');
		const argvOf = (settings) => statsArgv(KINDS.stats.props(settings).settings);

		assert.deepEqual(argvOf({ ...KINDS.stats.meta.defaultSettings }), ['stats', '--month']);
		const shipped = [
			...Object.values(BUILTIN).flatMap((at) => [at(false), at(true)]),
			DEBUG_LAYOUT_TREE
		].flatMap(statsWidgets);
		assert.ok(shipped.length >= 2, `found ${shipped.length} stats panes: this proves nothing`);
		for (const settings of shipped) assert.deepEqual(argvOf(settings), ['stats', '--month']);
	});

	test('a stored all time is not rewritten: it still runs bare stats', async () => {
		const { KINDS } = await import('../src/lib/layout/registry.ts');
		assert.deepEqual(statsArgv(KINDS.stats.props({ range: 'all' }).settings), ['stats']);
	});
});
