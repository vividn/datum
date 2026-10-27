// A nowview drawn ahead keeps its history drawn beside another of its size
// in a tab behind (da-wvdz2). vividn, on the tutorial: «the 2 minutes of
// history only shows very briefly before being blanked out. If you pan
// around, then you can see the data but you can't otherwise».
//
// A picture drawn ahead (`--ahead`) lays its lanes under a clip at the now
// line, whose id is the clip's geometry, so every 2-minute nowview of one
// size names the same `flow-past-…`. `url(#…)` takes the first on the page,
// and a tab behind is `visibility: hidden`: a clip there whose shape is not
// visible clipped the lanes of the tab in front away entirely. A pan asks
// for a picture not drawn ahead, which has no clip, so it showed them.
//
// Whether a lane is drawn is read by hit-testing: a clipped-away mark is not
// under the pointer, and `elementFromPoint` agrees with what is painted.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, todayAt, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

const bar = { edge: 'top', align: 'start' };

/** A lesson's tab as the tutorial lays it out (`withNowview`): a 2-minute nowview over a tail. */
const lesson = (name) => ({
	id: `${name}-tab`,
	content: {
		id: `${name}-split`,
		direction: 'column',
		ratios: [0.38, 0.62],
		children: [
			{
				id: `${name}-now`,
				tabs: [{ id: `${name}-now-tab`, content: { id: `${name}-now-w`, kind: 'nowview', settings: { history: '2m', orientation: 'horizontal' } } }],
				active: 0,
				bar
			},
			{ id: `${name}-tail`, tabs: [{ id: `${name}-tail-tab`, content: { id: `${name}-tail-w`, kind: 'tail', settings: {} } }], active: 0, bar }
		]
	}
});

/** `names` as tabs of one pane, the last in front. */
const desk = (...names) => ({
	version: 1,
	name: 'default',
	designedFor: 'desktop',
	root: { id: 'root', tabs: names.map(lesson), active: names.length - 1, bar }
});

/** Each lamp block in the nowview in front: whether it is under the pointer halfway from its start to the now line, or what is. */
const drawn = (page) =>
	page.evaluate(() =>
		[...document.querySelectorAll('svg.svg rect.block[data-field="lamp"]')].filter((rect) => !rect.closest('.behind')).map((rect) => {
			const line = rect.ownerSVGElement.querySelector('.current-time-line').getBoundingClientRect();
			const { left, top, height } = rect.getBoundingClientRect();
			const hit = document.elementFromPoint((left + line.left) / 2, top + height / 2);
			return hit === rect || `${hit?.tagName}.${hit?.getAttribute('class')}`;
		})
	);

describe('a nowview drawn ahead beside another in a tab behind', () => {
	for (const names of [['b'], ['a', 'b']])
		test(`a span written a minute ago stays drawn, after the write and as it slides (${names.length} tab${names.length > 1 ? 's' : ''})`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await page.clock.runFor(500);
					await app.idle();
					assert.equal(await page.locator('svg.svg:has([clip-path^="url(#flow-past"])').count(), names.length, 'each nowview is drawn ahead');
					// Loud, as a lesson's tap is: every nowview is asked again.
					assert.equal((await page.evaluate((a) => window.datum.run(a), ['start', 'lamp', '-t', '-1'])).code, 0);
					await app.clock.forward(500);
					await app.idle();
					assert.deepEqual(await drawn(page), [true], 'the lamp drawn after the write');
					await app.clock.forward(30_000);
					await app.idle();
					assert.deepEqual(await drawn(page), [true], 'the lamp drawn as it slides');
					assert.deepEqual(app.errors, []);
				},
				{ clock: todayAt(15), holdTimers: true, desk: desk(...names) }
			);
		});
});
