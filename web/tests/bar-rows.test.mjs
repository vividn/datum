// The fields bar at one, two and three button-rows high (da-hs6c,
// da-vu7pi): it draws as many rows as fit, each packed on its own, and is
// always reached sideways — with eight tracked fields. A field's states
// stand around the tap instead (da-7q3ka, `fields-cluster.test.mjs`). The commands bar's half of this is
// `commands.test.mjs`; the helpers both use are `./support/rows.mjs`.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines, todayAt } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';
import { MAX_ROWS, atRows, axes, boxes, gaps, lastReached, packed, rowsOf } from './support/rows.mjs';

before(startApp);
after(stopApp);

const PAGE = JSON.stringify(along('column', widgetPane('repl', 'repl'), widgetPane('bar', 'widgetbar')));

const EIGHT = ['alfa', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot', 'golf', 'hotel'];
const STATES = ['one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight'];
const SEED = [
	...EIGHT.slice(1).map((f) => `setup ${f} --verb occur`),
	`setup alfa --verb switch --states ${STATES.join(',')}`
];

const bar = (page) => page.locator('section[data-pane="bar"] .bar');
const BUTTONS = 'button:not([tabindex="-1"])';

/** The bar at `want` rows: its shape, nothing clipped, every row packed, every button reached. */
async function holds(page, want) {
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
	assert.ok(EIGHT.includes(starts[0].name), `the first row starts with ${starts[0].name}, not a field`);
	assert.equal(starts.length, want, `${want} rows, but ${starts.length} start at the left edge`);
}

describe('the fields bar and the scroll rule', () => {
	test('eight fields at one, two and three rows high', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seedLines(app, SEED);
				await page.locator('.bar [data-field="hotel"]').waitFor();
				for (const want of [1, 2, 3]) await holds(page, want);
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

// vividn, 2026-10-07: «the fields widget seems to only ever get 3 rows for
// some reason? It should expand to have more rows as it gets taller.» The
// fields bar has no cap (`maxRows` on ButtonRows, da-arms4); the commands
// bar beside it, at the same height, keeps its three.
describe('the fields bar takes every row its box fits', () => {
	const SIDE = JSON.stringify(
		along('column', widgetPane('repl', 'repl'), along('row', widgetPane('cmd', 'command'), widgetPane('bar', 'widgetbar')).root)
	);
	/** Eighteen fields, each its own `y`, so six rows of three and three of six are both equal counts. */
	const FIELDS = Array.from({ length: 18 }, (_, i) => `f${String(i).padStart(2, '0')}`);
	const at = (field, i) => `setup ${field} --from-json '${JSON.stringify({ field, verb: 'occur', y: i / 18 })}'`;
	/** The fields cut top to bottom into `n` bands of `y`, each read as a set. */
	const banded = (n) =>
		Array.from({ length: n }, (_, k) => FIELDS.slice((k * 18) / n, ((k + 1) * 18) / n));
	const drawnRows = async (page) =>
		(await rowsOf(bar(page), '[data-field]')).map((row) => [...row].sort());

	test('six rows in a tall pane, three in a short one; the commands beside it stop at three', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				await seedLines(app, FIELDS.map(at));
				await app.reload();
				await page.locator('section[data-pane="bar"] [data-field="f17"]').waitFor();

				await atRows(page, 'bar', 6, Infinity);
				assert.equal((await axes(bar(page))).rows, 6, 'the fields bar draws six rows');
				assert.deepEqual(await drawnRows(page), banded(6), 'six bands of y, three each');
				const cmd = page.locator('section[data-pane="cmd"] .bar');
				const fits = await cmd.evaluate((el) => {
					const probe = el.closest('section').querySelector('.probe button').offsetHeight;
					return Math.floor((el.clientHeight + 6.4) / (probe + 6.4));
				});
				assert.ok(fits > MAX_ROWS, `the commands box fits ${fits} rows, so its cap is what holds it`);
				assert.equal(await cmd.getAttribute('data-rows'), String(MAX_ROWS), 'the commands bar is capped');
				assert.equal((await axes(cmd)).rows, MAX_ROWS, 'the commands bar draws three rows');

				await atRows(page, 'bar', 3, Infinity);
				assert.equal((await axes(bar(page))).rows, 3, 'the fields bar draws three rows');
				assert.deepEqual(await drawnRows(page), banded(3), 'three bands of y, six each');
			},
			{
				viewport: { width: 1280, height: 900 },
				clock: todayAt(15),
				prepare: (context) => context.addInitScript(draft, SIDE)
			}
		);
	});
});
