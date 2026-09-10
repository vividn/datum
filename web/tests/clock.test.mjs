// One clock convention, under a reader whose locale disagrees with it.
//
// The tail pane printed the time through `toLocaleTimeString([])` — `[]`
// being "ask the browser" — while the nowview axis built `HH:MM` out of
// `getHours()`. On an en-US machine that put "04:07 PM coffee" beside an
// axis reading "16:07": the same instant, two conventions, one screenshot
// (da-6o81). datum is 24-hour everywhere else, including the CLI's own
// `render_entry_line`, which the web renderer's docstring claims parity
// with.
//
// Why this suite's other 47 tests never saw it: `toLocaleTimeString([])`
// is exactly the call whose answer depends on the machine running it, and
// CI's machine is already 24-hour. So a test that does not pin the locale
// is not a test of this at all — it passes before the fix. This one pins
// en-US and then *checks that it took*, so the day the pin stops working
// the test says so rather than going quietly vacuous.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { rowTexts } from './support/tail.mjs';

before(startApp);
after(stopApp);

/** A locale that renders times as `04:07 PM`. */
const TWELVE_HOUR = 'en-US';

/** Run `fn` against an app whose reader is on a 12-hour locale. */
const withTwelveHourReader = (fn) => withApp(fn, { locale: TWELVE_HOUR });

describe('the clock the web app prints', () => {
	test('a 12-hour reader still gets datum 24-hour time, in every pane', async () => {
		await withTwelveHourReader(async ({ page, run, reload, paneTable, paneSvg }) => {
			// First: the pin is real. This is the call the bug was made of,
			// made in the page itself — if it comes back 24-hour here then
			// the locale did not take and every assertion below is vacuous.
			const asked = await page.evaluate(() =>
				new Date(Date.UTC(2026, 8, 5, 16, 7)).toLocaleTimeString([], {
					hour: '2-digit',
					minute: '2-digit'
				})
			);
			assert.match(asked, /PM/, `locale ${TWELVE_HOUR} did not take: browser said "${asked}"`);

			// The zone is UTC (support/app.mjs), so 16:07 is 16:07.
			assert.equal((await run(['occur', 'coffee', '-t', '1607'])).code, 0);

			const tail = await run(['tail']);
			assert.equal(tail.code, 0, tail.stderr);
			// The CLI's own table (crates/datum-commands/src/render), via the
			// wasm renderer: a header carrying the listing's UTC offset, then
			// a 24-hour clock, the field, and `∅` for a field's first entry.
			assert.deepEqual(rowTexts(tail.stdout), [
				'time (+0) field s _n hid',
				'16:07:00 coffee ∅● _1 <hid>'
			]);

			// And on the screen, which is where the two conventions sat
			// next to each other. The panes run their command on mount, so
			// the reload is how they come to know about the entry.
			await reload();
			const table = await paneTable('tail');
			assert.deepEqual(table.rows[0][table.keys.indexOf('time')], '16:07:00');
			const shown = table.rows.flat().join(' ');
			assert.doesNotMatch(shown, /\b[AP]M\b/i, `the tail pane is still 12-hour: ${shown}`);

			// The other half of the screenshot. Its clock was never the
			// broken one; asserting it here is what makes this a test of
			// the two panes agreeing rather than of one of them.
			const now = await paneSvg('nowview');
			assert.match(now, /<text[^>]*>\d{2}:\d{2}<\/text>/);
			assert.doesNotMatch(now, /\b[AP]M\b/i, 'the nowview axis went 12-hour');
		});
	});
});

describe('the date the web app prints', () => {
	test('an entry older than a day gets a datum date, not the reader\'s', async () => {
		await withTwelveHourReader(async ({ page, run, entries, reload, paneTable }) => {
			// The same pin, on the call *this* bug is made of. `9/4/2026` is
			// what an en-US browser answers; a runner that already writes
			// dates the way we want would make everything below vacuous.
			const asked = await page.evaluate(() => new Date().toLocaleDateString());
			assert.match(
				asked,
				/^\d{1,2}\/\d{1,2}\/\d{4}$/,
				`locale ${TWELVE_HOUR} did not take: browser said "${asked}"`
			);

			// Older than 24 hours, or the row reads as a clock and the branch
			// that formats a date is never reached at all.
			assert.equal((await run(['switch', 'reading', 'book', '-y'])).code, 0);
			const [stored] = await entries(1);
			// The zone is UTC (support/app.mjs), so the stored instant's own
			// date is the date the reader is owed.
			const day = stored.occur_time.utc.slice(0, 10);

			await reload();
			const table = await paneTable('tail');
			const shown = table.rows[0][table.keys.indexOf('time')];
			assert.equal(shown, day, `the tail pane is on the reader's calendar: "${shown}"`);

			// And /now, which asked `toLocaleDateString()` once (da-6eou).
			await page.locator('header nav a[href="/now"]').click();
			await page.locator('.now-view pre').first().waitFor();
			const now = await page.locator('.now-view').innerText();
			assert.match(now, /reading/);
			assert.doesNotMatch(now, /\d{1,2}\/\d{1,2}\/\d{4}/, `/now is locale-formatted: "${now}"`);
		});
	});
});
