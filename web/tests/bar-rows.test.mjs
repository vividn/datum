// The fields bar at one, two and three button-rows high (da-hs6c,
// da-vu7pi): it draws as many rows as fit, each packed on its own, and is
// always reached sideways — with eight tracked fields. A field's states
// stand around the tap instead (da-7q3ka, `fields-cluster.test.mjs`). The commands bar's half of this is
// `commands.test.mjs`; the helpers both use are `./support/rows.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines, todayAt } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';
import { atRows, axes, boxes, gaps, lastReached, packed } from './support/rows.mjs';

before(startApp);
after(stopApp);

const PAGE = JSON.stringify(along('column', widgetPane('repl', 'repl'), widgetPane('bar', 'widgetbar')));

const EIGHT = ['alfa', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot', 'golf', 'hotel'];
const STATES = ['one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight'];
const SEED = [
	...EIGHT.slice(1).map((f) => `spec set ${f} --verb occur`),
	`spec set alfa --verb switch ${STATES.map((s) => `--state ${s}`).join(' ')}`
];

const bar = (page) => page.locator('section[data-pane="bar"] .bar');
const BUTTONS = 'button:not([tabindex="-1"])';

/** The bar at `want` rows: its shape, nothing clipped, every row packed, every button reached. */
async function holds(page, want, first) {
	await atRows(page, 'bar', want);
	const shape = await axes(bar(page));
	const drawn = await boxes(bar(page), BUTTONS);
	assert.deepEqual(
		[shape.x, shape.y, shape.display, shape.rows, shape.fits],
		['auto', 'hidden', 'flex', want, true],
		`${want} rows of room`
	);
	assert.deepEqual(drawn.filter((b) => !b.inside).map((b) => b.name), [], `${want} rows: clipped`);
	packed(await gaps(bar(page), BUTTONS), `${want} rows`);
	assert.ok(await lastReached(bar(page), BUTTONS), `${want} rows: the last button is not reached`);
	// Dealt by index: the first `want` start the rows, top to bottom.
	const starts = drawn.filter((b) => b.left === drawn[0].left).sort((a, b) => a.line - b.line);
	assert.equal(starts[0].name, first, 'the first row does not start with the first button');
	assert.equal(starts.length, want, `${want} rows, but ${starts.length} start at the left edge`);
}

describe('the fields bar and the scroll rule', () => {
	test('eight fields at one, two and three rows high', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seedLines(app, SEED);
				await page.locator('.bar [data-field="hotel"]').waitFor();
				for (const want of [1, 2, 3]) await holds(page, want, 'Track +');
			},
			{
				// Narrow enough that nine buttons wrap onto three lines.
				viewport: { width: 280, height: 500 },
				clock: todayAt(15),
				prepare: (context) => context.addInitScript(draft, PAGE)
			}
		);
	});
});
