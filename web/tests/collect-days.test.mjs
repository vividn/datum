// Collect's nowview pane has a dayview tab beside the nowview (da-jtqtg),
// fitting three days to whatever height the pane has.
//
// vividn, 2026-11-11: «on the default collection layout, add a dayview tab on
// the nowview pane. let's have the default day height be such that you can see
// three days on this dayview by default», and on fixed or fitted: «fit». So on
// a phone and a desktop the pane's tabs are nowview, in front, then dayview;
// opened, the dayview shows three days whole; resized, still three, a desktop
// pane taller than 3 × 240 px included (the reader's range bounds the fit, not
// the one-day fill's).

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, pane } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, home: ON_COLLECT };
const FRESH_DESKTOP = { ...DESKTOP, home: ON_COLLECT, desk: null };

const tabs = (page) => pane(page, 'nowview').locator('> .titlebar [role="tab"]');

/**
 * How much of each of the four newest days the dayview tab's box shows, and
 * how tall each is drawn: three whole is days 0..2 seen in full and day 3 no
 * more than the floor's leftover (under one day's divider and seam).
 */
const days = (page) =>
	pane(page, 'nowview').evaluate((p) => {
		const stack = p.querySelector('.strip .stack').getBoundingClientRect();
		return [0, 1, 2, 3].map((back) => {
			const r = p.querySelector(`.strip [data-days-ago="${back}"]`).getBoundingClientRect();
			return {
				of: Math.round(r.height),
				seen: Math.round(Math.max(0, Math.min(r.bottom, stack.bottom) - Math.max(r.top, stack.top)))
			};
		});
	});

const threeWhole = (d) => d.slice(0, 3).every((x) => x.seen === x.of && x.of === d[0].of) && d[3].seen < 4;

for (const [at, options, taller] of [
	['a phone', PHONE, { width: 390, height: 664 }],
	// The pane is half of two thirds of the screen: at 2600 px tall, over 720.
	['a desktop', FRESH_DESKTOP, { width: 1280, height: 2600 }]
])
	test(`on ${at}, collect's nowview pane has a dayview tab fitting three days, resized too`, async () => {
		await withApp(async ({ page, errors }) => {
			await tabs(page).first().waitFor();
			assert.deepEqual(await tabs(page).allInnerTexts(), ['nowview', 'dayview']);
			assert.equal(await tabs(page).first().getAttribute('aria-selected'), 'true', 'the nowview is not in front');

			await tabs(page).nth(1).click();
			const first = await until(() => days(page), threeWhole);
			await page.setViewportSize(taller);
			const second = await until(() => days(page), (d) => threeWhole(d) && d[0].of !== first[0].of);
			if (at === 'a desktop') {
				const box = await pane(page, 'nowview').locator('.strip').boundingBox();
				assert.ok(box.height > 720, `the pane is ${box.height}px, not over 720`);
				assert.ok(second[0].of > 240, `a ${second[0].of}px day`);
			}
			assert.deepEqual(errors, []);
		}, options);
	});
