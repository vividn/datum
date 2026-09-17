// The fields bar at one, two and three button-rows high (da-hs6c): at one
// or two it draws as many rows as fit and is reached sideways, at three it
// wraps and is reached down — with eight tracked fields, and with a
// field's states filling the same bar. The commands bar's half of this is
// `commands.test.mjs`; the helpers both use are `./support/rows.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines, todayAt } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';
import { atRows, axes, boxes, lastReached } from './support/rows.mjs';

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

/** The bar at `want` rows: its shape, nothing clipped, and every button reached. */
async function holds(page, want, first) {
	await atRows(page, 'bar', want);
	const shape = await axes(bar(page));
	const drawn = await boxes(bar(page), BUTTONS);
	if (want <= 2) {
		assert.deepEqual(
			[shape.x, shape.y, shape.display, shape.rows, shape.fits],
			['auto', 'hidden', 'grid', want, true],
			`${want} rows of room`
		);
		assert.deepEqual(drawn.filter((b) => !b.inside).map((b) => b.name), [], `${want} rows: clipped`);
		assert.ok(await lastReached(bar(page), BUTTONS), `${want} rows: the last button is not reached`);
		if (want === 2) {
			// Column-major: the first two in DOM order are one column.
			assert.deepEqual(
				[drawn[0].name, drawn[0].left === drawn[1].left, drawn[0].line < drawn[1].line],
				[first, true, true],
				'the first column is not the first two buttons, top over bottom'
			);
		}
	} else {
		assert.deepEqual([shape.x, shape.y, shape.display, shape.fits], ['hidden', 'auto', 'flex', true]);
		assert.ok(shape.rows >= 3, `${want} rows of room, but the bar drew ${shape.rows}`);
	}
}

describe('the fields bar and the scroll rule', () => {
	test('eight fields, and a field’s states, at one, two and three rows high', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seedLines(app, SEED);
				await page.locator('.bar [data-field="hotel"]').waitFor();
				for (const want of [1, 2, 3]) await holds(page, want, 'Track +');

				await page.locator('.bar [data-field="alfa"]').click();
				await page.locator('.bar [data-state="eight"]').waitFor();
				for (const want of [1, 2, 3]) await holds(page, want, '< Back');
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
