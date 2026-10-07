// What a resize costs the panes (da-4glsf). vividn: «Maximizing panes and
// reminizing for example comes with huge lag of reloading the data … this
// needs to continue to be snappy even with large amounts of data.»
//
// A pane re-runs on a resize only when its argv changes: the drawn views
// (dayview, nowview) are drawn at the box's size and run again at the new
// one, once; the tail's `tail -n <n>` reads the same rows at any width and
// runs again only if the height changes its count — five years of data
// made that 1.3 s in the one queue for nothing. Counted at the seam, with
// the clock held, so no tick lands among the runs.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, maximizeButton, settled } from './support/layout.mjs';

before(startApp);
after(stopApp);

/** Every run `act` sets off, until the page is quiet, as argv strings. */
async function runsOf(app, act) {
	const seen = await app.spyOnRunner();
	await act();
	await settled(app.page);
	await app.idle();
	// A run that lands can resize what is under it, and that asks again.
	await app.page.waitForTimeout(500);
	await app.idle();
	return (await seen()).map((argv) => argv.join(' '));
}

const twice = (runs) => runs.filter((argv, i) => runs.indexOf(argv) !== i);

test('a width-only resize runs each drawn view once, and no tail', async () => {
	await withApp(async (app) => {
		await app.idle();
		const runs = await runsOf(app, () => app.page.setViewportSize({ width: 900, height: 900 }));
		assert.deepEqual(
			runs.filter((argv) => argv.startsWith('tail')),
			[],
			runs.join(' | ')
		);
		for (const view of ['dayview', 'nowview'])
			assert.ok(runs.some((argv) => argv.startsWith(`${view} `)), `${view} re-drawn: ${runs.join(' | ')}`);
		assert.deepEqual(twice(runs), [], runs.join(' | '));
	}, DESKTOP);
});

test('a maximize and its restore run a dayview once per size', async () => {
	await withApp(async (app) => {
		await app.idle();
		const press = () => maximizeButton(app.page, 'strip').click();
		for (const step of ['maximize', 'restore']) {
			const runs = await runsOf(app, press);
			assert.ok(runs.some((argv) => argv.startsWith('dayview ')), `${step} re-drew the dayview: ${runs.join(' | ')}`);
			assert.deepEqual(twice(runs), [], `${step}: ${runs.join(' | ')}`);
		}
	}, DESKTOP);
});
