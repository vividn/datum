// The app may not answer with a precision the record does not have.
//
// `occur coffee -y` printed `00:00 coffee`. The entry never claimed to
// have happened at midnight: midnight is the anchor a date-only entry is
// stored at, and the row says so in `occur_time_uncertainty` — `P1D`, an
// ISO-8601 duration meaning "precise to one day". The CLI reads that field
// and prints the bare date; the browser formatted a clock unconditionally
// and invented a time of day for every entry logged into the past (da-kb8p).
//
// Same family as da-sgvh, where the /now cards filled an empty value slot
// with the stored verb: a view answering more precisely than the data it
// was given. The suites that compare what an argv STORES cannot see it —
// both clients store the identical row — so this reads what is RENDERED.
//
// All three sides of the trap are asserted in one test on purpose, because
// the plausible overshoot is to suppress the clock whenever the row carries
// an uncertainty at all. `P1D` is the date-only marker, not an uncertainty
// (`format_uncertainty_suffix` in crates/datum-commands/src/render/
// action.rs returns None for it and only it); `±5m` is a real one, and an
// entry that has it still happened at a knowable minute. A test written
// only for the -y case would call stripping those a pass.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { rowTexts } from './support/tail.mjs';

before(startApp);
after(stopApp);

describe('the time an entry is rendered with', () => {
	test('a date-only entry shows its date; entries that have a clock keep it', async () => {
		await withApp(async ({ run, entries, reload, paneTable }) => {
			for (const argv of [
				['occur', 'coffee', '-y'],
				['occur', 'tea', '-t', '0915'],
				['occur', 'water', '-t', '1000±5m']
			]) {
				assert.equal((await run(argv)).code, 0, argv.join(' '));
			}

			// The record's own words, first — the assertions below are about
			// honouring this column, so a test that did not read it would be
			// asserting against a guess.
			const [water, tea, coffee] = await entries(3);
			assert.equal(coffee.occur_time.uncertainty, 'P1D', 'the -y entry lost its day-precision');
			assert.equal(tea.occur_time.uncertainty, undefined, 'the -t entry gained an uncertainty');
			assert.equal(water.occur_time.uncertainty, 'PT5M', 'the ±5m entry lost its uncertainty');

			// The zone is UTC (support/app.mjs), so the stored instant's date
			// is the date the entry names.
			const yesterday = coffee.occur_time.utc.slice(0, 10);

			const tail = await run(['tail']);
			assert.equal(tail.code, 0, tail.stderr);
			// These are the CLI's own table rows (the wasm renderer): a P1D
			// row degrades to its bare date, a clock keeps its clock, and
			// the `±5m` the CLI paints after a stamp is carried too. The
			// listing's shared UTC offset rides in the header (da-lkji).
			// Each field's TRACK document (da-c8mh) reads a millisecond above
			// its entry; the entries are the rows that are not declarations.
			assert.deepEqual(rowTexts(tail.stdout).filter((r) => !/^_\d+ TRACK/.test(r)).slice(1), [
				`_5 ${yesterday} coffee ∅● <hid>`,
				'_3 09:15:00 tea ∅● <hid>',
				'_1 10:00:00±5m water ∅● <hid>'
			]);

			// And on the screen, which is where the invented midnight was
			// read. The panes run their command on mount, so the reload is
			// how the tail pane comes to know about the entries.
			await reload();
			const table = await paneTable('tail');
			// Each field's TRACK document (da-c8mh) reads just above its
			// entry, so the entries are every second row.
			const times = table.rows
				.filter((_, i) => i % 2 === 1)
				.map((cells) => cells[table.keys.indexOf('time')]);
			// The date the coffee owns, the clocks the other two own — and
			// no midnight invented for the day-precision row.
			assert.deepEqual(times, [yesterday, '09:15:00', '10:00:00±5m']);
		}, { viewport: { width: 1280, height: 1200 } });
	});
});
