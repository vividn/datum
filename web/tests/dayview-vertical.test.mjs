// The vertical dayview fills its pane (da-nopcd). vividn, 2026-11-07: «I made a
// tiny full height column at the left of the collection layout, but it drew 8
// days in a very tiny box at the top of the widget rather than 1 day filling the
// whole column». The pane asked for a week whatever its width, and the week came
// back wider than the box and was scaled into it. Now the days are as many as
// the width holds (`DAY_COLUMN` in `#lib/panes.ts`), and each runs the height.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { pane, pickOption, titledPane } from './support/layout.mjs';
import { resolveLibAsSource } from './support/libhook.mjs';

resolveLibAsSource();
const { DAY_COLUMN, paneArgv } = await import('../src/lib/panes.ts');

before(startApp);
after(stopApp);

/**
 * Pin the pane to `width` by `height` at the page's corner, and wait for its
 * picture asked at that size: the picture's height is its box's, before the
 * fix and after, so it says the run has landed and nothing about the days.
 */
async function sizePane(page, id, width, height) {
	await pane(page, id).evaluate(
		(el, [w, h]) => Object.assign(el.style, { position: 'fixed', left: '0', top: '0', width: `${w}px`, height: `${h}px`, zIndex: 50 }),
		[width, height]
	);
	await page.waitForFunction((id) => {
		const body = document.querySelector(`section[data-pane="${id}"] .body`);
		const s = getComputedStyle(body);
		const height = Math.floor(body.clientHeight - parseFloat(s.paddingTop) - parseFloat(s.paddingBottom));
		return Number(body.querySelector('svg.svg')?.getAttribute('height')) === height;
	}, id);
}

/** What the pane drew: its body's content box, the picture on screen, its columns, and each column's time axis. */
const drawn = (page, id) =>
	pane(page, id).evaluate((section) => {
		const body = section.querySelector('.body');
		const s = getComputedStyle(body);
		const content = {
			width: body.clientWidth - parseFloat(s.paddingLeft) - parseFloat(s.paddingRight),
			height: body.clientHeight - parseFloat(s.paddingTop) - parseFloat(s.paddingBottom)
		};
		const svg = body.querySelector('svg.svg');
		const rect = svg.getBoundingClientRect();
		const days = [...svg.querySelectorAll('g.day-data[data-axis="y"]')];
		return {
			content,
			shown: { width: rect.width, height: rect.height },
			height: Number(svg.getAttribute('height')),
			columns: days.length,
			axes: days.map((d) => ({ lo: Number(d.dataset.lo), hi: Number(d.dataset.hi) }))
		};
	});

describe('the vertical dayview fills its pane', () => {
	// Every spelling `parse_dayview_args` reads, the last orientation winning
	// (the panel on #1826): a typed pane can say any of them.
	test('a day count is added to a vertical dayview naming none, in any spelling', () => {
		const box = { width: 3 * DAY_COLUMN, height: 600 };
		// What `paneArgv` puts between the line and its `--width W --height H`.
		const days = (line) => paneArgv(line.split(' '), box).slice(line.split(' ').length, -4).join(' ') || null;
		for (const line of ['dayview --orientation vertical', 'dayview --orientation=vertical', 'dayview --vertical', 'dayview --orientation v', 'dayview --horizontal --vertical'])
			assert.equal(days(line), '--days 3', line);
		for (const line of ['dayview', 'dayview --orientation horizontal', 'dayview --vertical --orientation=h', 'nowview --vertical', 'dayview --vertical --days 2', 'dayview --vertical --days=2', 'dayview --vertical -n 2', 'dayview --vertical -n2', 'dayview --vertical --n-days 2', 'dayview --vertical --start-date -3'])
			assert.equal(days(line), null, line);
	});

	test('a narrow tall pane draws today down its height; a wide one as many days as it holds', async () => {
		await withApp(
			async ({ page, submit, errors }) => {
				await submit('start work -t -60');
				const id = await titledPane(page, 'dayview').first().getAttribute('data-pane');
				await pickOption(page, id, 'orientation', 'vertical');

				await sizePane(page, id, 90, 700);
				const narrow = await drawn(page, id);
				assert.equal(narrow.columns, 1, `a 90px column drew ${narrow.columns} days`);
				assert.ok(Math.abs(narrow.shown.height - narrow.content.height) <= 2, `the day is ${narrow.shown.height}px tall in a ${narrow.content.height}px pane`);
				assert.ok(Math.abs(narrow.shown.width - narrow.content.width) <= 2, `the day is ${narrow.shown.width}px wide in a ${narrow.content.width}px pane`);
				// Its time runs from under the date to the foot of the pane.
				const [axis] = narrow.axes;
				assert.ok(narrow.height - axis.hi <= 3, `the time axis ends ${narrow.height - axis.hi}px above the foot`);
				assert.ok(axis.hi - axis.lo > 0.8 * narrow.height, `the time axis spans ${axis.hi - axis.lo} of ${narrow.height}px`);

				await sizePane(page, id, 700, 600);
				const wide = await drawn(page, id);
				const holds = Math.floor(wide.content.width / DAY_COLUMN);
				assert.ok(holds > 1, `a 700px pane holds ${holds}`);
				assert.equal(wide.columns, holds, `a ${wide.content.width}px pane drew ${wide.columns} days, not the ${holds} it holds`);
				assert.ok(Math.abs(wide.shown.width - wide.content.width) <= 2, `${wide.columns} days are ${wide.shown.width}px wide in ${wide.content.width}px`);
				assert.ok(Math.abs(wide.shown.height - wide.content.height) <= 2, `the days are ${wide.shown.height}px tall in ${wide.content.height}px`);
				assert.deepEqual(errors, []);
			},
			{ viewport: { width: 1280, height: 900 } }
		);
	});
});
