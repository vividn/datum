// The tail narrows to the bar's state pill (da-w48r3). vividn, 2026-11-09:
// «when you have a state (or states) selected, filter the tail to only show
// entries of that state/s». The pill is the CLI's `tail --state`: one state
// is its entries, the multi-select's array any of its states, and cleared
// the tail is the field's again. The flag itself is `tail_state_flag.rs`.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, todayAt } from './support/app.mjs';
import { along, widgetPane } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const PAGE = JSON.stringify(along('column', widgetPane('tail', 'tail'), widgetPane('bar', 'widgetbar')));
const OPEN = 'section[data-pane="bar"] .bar .collect.open';
const shared = sharedApp({ clock: todayAt(15) });

/** The tail pane's rows, field and state, oldest first. */
const listed = (page) =>
	page.locator('section[data-pane="tail"] tbody tr[data-hid]').evaluateAll((rows) =>
		rows.map((tr) => `${tr.querySelector('td.field')?.textContent.trim()} ${tr.querySelector('td.state')?.textContent.trim()}`)
	);

test('a state pill narrows the tail to its entries, two to either, and cleared it is the field again', async () => {
	await shared.solo(async (app) => {
		const { page } = app;
		await page.evaluate(() => window.datum.layout.forgetSession());
		for (const line of [
			'setup mood --verb switch --states false,good,bad,meh',
			'change mood good -t 9',
			'change mood bad -t 10',
			'change mood meh -t 11',
			'change mood good -t 12',
			'moment tea -t 13'
		]) {
			const logged = await app.run(line.split(' '));
			assert.equal(logged.code, 0, `${line}: ${logged.stderr}`);
		}
		await app.redraw();
		await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
		const same = (want) => (got) => JSON.stringify(got) === JSON.stringify(want);
		await until(() => listed(page), (rows) => rows.some((r) => r.startsWith('tea')));

		await page.locator(`section[data-pane="bar"] .bar .collect [data-field="mood"]`).click();
		const mood = ['mood good', 'mood bad', 'mood meh', 'mood good'];
		await until(() => listed(page), same(mood));

		// A hold puts the state's pill on the bar, and the tail lists it alone.
		const state = (s) => page.locator(`${OPEN} [data-state="${s}"]`);
		await state('good').click({ button: 'right' });
		await until(() => listed(page), same(['mood good', 'mood good']));
		// A second state gathered is either.
		await state('bad').click();
		await until(() => listed(page), same(['mood good', 'mood bad', 'mood good']));
		// Each taken out again, the field's whole listing.
		await state('good').click();
		await until(() => listed(page), same(['mood bad']));
		await state('bad').click();
		await until(() => listed(page), same(mood));
	}, PAGE);
});
