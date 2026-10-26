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
import { inView } from './support/daystrip.mjs';

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

// da-8khuw: the day strip asked for its whole base stack at every new
// size, fifteen processes on five years of data, the last 1.5 s behind the
// resize. A new size asks for today and the days on screen, the rest kept
// until they are; and the sizes before it are held, so a restore asks
// nothing, nor a maximize again.
test('a maximize asks for the days on screen, and a restore and a maximize again ask nothing', async () => {
	await withApp(async (app) => {
		await app.idle();
		const press = () => maximizeButton(app.page, 'strip').click();
		const runs = await runsOf(app, press);
		assert.deepEqual(twice(runs), [], runs.join(' | '));
		const asked = days(runs);
		assert.ok(asked.includes(0), `the maximize re-drew no today: ${runs.join(' | ')}`);
		const visible = await inView(app);
		assert.deepEqual(asked.filter((end) => !visible.includes(end)), [], `days off screen asked for (on screen: ${visible})`);
		for (const step of ['restore', 'maximize again'])
			assert.deepEqual(days(await runsOf(app, press)), [], `${step} drew a day again`);
	}, DESKTOP);
});

/** The days back each band run asks for, the geometry's JSON run aside. */
const days = (runs) =>
	runs
		.filter((argv) => argv.startsWith('dayview ') && !argv.includes('json'))
		.map((argv) => Number(argv.match(/--end-date (-?\d+)/)?.[1] ?? 0) * -1 || 0)
		.sort((a, b) => a - b);
