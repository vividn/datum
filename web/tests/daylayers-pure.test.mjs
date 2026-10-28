// Which days the nowview asks for, in node (da-31hhd).
//
// The nowview asks the data layer of every local day its drawing reaches.
// Counted in 24 hours, a window starting in the hour before a midnight
// that a spring-forward night followed skipped that day: an empty stretch
// at the start of the band.

import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { dateBack, daysBack, spanDates } = await import('../src/lib/dayLayers.ts');

/** Run `fn` with the page in `zone`: node re-reads `TZ` when it is set. */
function inZone(zone, fn) {
	const was = process.env.TZ;
	process.env.TZ = zone;
	try {
		fn();
	} finally {
		if (was === undefined) delete process.env.TZ;
		else process.env.TZ = was;
	}
}

test('a drawing reaches back by local days, across a 23- and a 25-hour day', () => {
	inZone('Europe/Berlin', () => {
		// 2026-03-29, 23 hours: 23:30 the night before it is still that day.
		const spring = new Date(Date.UTC(2026, 2, 30, 10));
		assert.equal(daysBack(Date.UTC(2026, 2, 28, 22, 30), spring), 2);
		assert.equal(dateBack(2, spring), '2026-03-28');
		assert.equal(daysBack(Date.UTC(2026, 2, 28, 23, 0), spring), 1);
		// 2026-10-25, 25 hours: 00:30 on it is that day, 23:30 before it the one before.
		const fall = new Date(Date.UTC(2026, 9, 26, 10));
		assert.equal(daysBack(Date.UTC(2026, 9, 24, 22, 30), fall), 1);
		assert.equal(daysBack(Date.UTC(2026, 9, 24, 21, 30), fall), 2);
		// Today, and a drawing that starts ahead of it.
		assert.equal(daysBack(fall.getTime(), fall), 0);
		assert.ok(daysBack(Date.UTC(2026, 9, 27, 10), fall) < 0);
	});
});

// da-vnfr3: the nowview asks the days a drawing spans, however far back, by
// the same local days, and today with them (its now strip's states).
test('the days a drawing spans are its local days, across a 23-hour day, and today', () => {
	inZone('Europe/Berlin', () => {
		const spring = new Date(Date.UTC(2026, 2, 30, 10));
		// From 23:30 the night before 2026-03-29 on past now: 24 hours would count one day back, and leave 03-28 out.
		assert.deepEqual(spanDates(Date.UTC(2026, 2, 28, 22, 30), Date.UTC(2026, 2, 30, 12), spring), ['2026-03-30', '2026-03-29', '2026-03-28']);
		// A drawing days back: its own days, and today, and none between.
		assert.deepEqual(spanDates(Date.UTC(2026, 2, 26, 10), Date.UTC(2026, 2, 28, 22, 30), spring), ['2026-03-30', '2026-03-28', '2026-03-27', '2026-03-26']);
	});
});
