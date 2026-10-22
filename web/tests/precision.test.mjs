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
// an uncertainty at all. On a time `P1D` is the date-only marker, not an
// uncertainty (`format_timestamp_ref` in crates/datum-commands/src/render/
// action.rs draws the bare date for it and only it); `±5m` is a real one, and an
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
		await withApp(async ({ page, run, entries, redraw, paneTable }) => {
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
			// an uncertain one keeps its minute and colon, a gray `±` in its
			// seconds' place and no amount (da-dj8v0, da-d4xhl), which the
			// record above keeps.
			// The listing's shared UTC offset rides in the header (da-lkji).
			//
			// EVERY ROW IS AN ENTRY: nothing is filtered, and the quick ids
			// run straight. da-vug0d filtered the declarations out by their
			// `TRACKED` state cell and ASSERTED the drop, because "a filter
			// matching nothing reads exactly like a filter with nothing to
			// drop, and that indistinguishability IS the defect". That is
			// right, and it is why BOTH the filter and its drop-count go
			// here rather than only the count: da-gvcen gives a first log
			// ONE document, so there are no declaration rows left to drop —
			// and a filter that can never match is precisely what that
			// assertion existed to catch. Asserting the whole list
			// unfiltered makes the indistinguishability impossible instead
			// of merely guarded against.
			assert.deepEqual(rowTexts(tail.stdout).slice(1), [
				`_3 ${yesterday} coffee ∅● <hid>`,
				'_2 09:15:00 tea ∅● <hid>',
				'_1 10:00:± water ∅● <hid>'
			]);

			// And on the screen, which is where the invented midnight was
			// read. The seam's quiet door moves no pane, so the redraw is
			// how the tail pane comes to know about the entries.
			await redraw();
			const table = await paneTable('tail');
			// One row per log (da-gvcen), so every row is an entry.
			const times = table.rows.map((cells) => cells[table.keys.indexOf('time')]);
			// The date the coffee owns, the clocks the other two own — and
			// no midnight invented for the day-precision row.
			assert.deepEqual(times, [yesterday, '09:15:00', '10:00:± ']);

			// As wide as a clock with seconds, so the minutes stand in one
			// column: the space after the ± is drawn, not collapsed away.
			const cells = page.locator('section.pane[data-title="tail"] tbody td.time');
			const widths = await cells.evaluateAll((tds) =>
				tds.map((td) => {
					const range = document.createRange();
					range.selectNodeContents(td);
					return range.getBoundingClientRect().width;
				})
			);
			assert.equal(widths[2], widths[1], `the uncertain clock is as wide as an exact one: ${widths}`);
			// And the ± gray, the clock not.
			const colors = await cells.nth(2).evaluate((td) =>
				[...td.querySelectorAll('span')].map((s) => [s.textContent, getComputedStyle(s).color])
			);
			const color = (text) => colors.find(([t]) => t.includes(text))?.[1];
			assert.ok(color('±') && color('±') !== color('10:00'), `the ± is painted apart: ${JSON.stringify(colors)}`);
		}, { viewport: { width: 1280, height: 1200 } });
	});
});
