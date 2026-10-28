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
import { pageToday, picturesAsked } from './support/daystrip.mjs';

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
// resize. Each band is now its day's data layer scaled to the band
// (da-m6i2a, #lib/dayLayers.ts), held whatever the size: so a maximize asks
// for no day's data, only the geometry at the new width, and a restore and a
// maximize again ask nothing.
test('a maximize asks for no day, and a restore and a maximize again ask nothing', async () => {
	await withApp(async (app) => {
		await app.idle();
		const press = () => maximizeButton(app.page, 'strip').click();
		const runs = await runsOf(app, press);
		assert.deepEqual(twice(runs), [], runs.join(' | '));
		assert.deepEqual(await days(app, runs), [], `the maximize asked for a day: ${runs.join(' | ')}`);
		assert.ok(runs.some((argv) => argv.startsWith('dayview ') && argv.includes('--show json')), `no geometry at the new width: ${runs.join(' | ')}`);
		for (const step of ['restore', 'maximize again'])
			assert.deepEqual(await days(app, await runsOf(app, press)), [], `${step} drew a day again`);
	}, DESKTOP);
});

/** The days back the strip's runs ask pictures of, in all. */
const days = async (app, runs) => {
	const today = await pageToday(app);
	return runs.flatMap((argv) => picturesAsked(argv.split(' '), today)).sort((a, b) => a - b);
};
