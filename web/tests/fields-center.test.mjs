// A few fields sit in the middle of the fields widget's box (da-syu4f).
// vividn, 2026-10-27: «in general fields should appear more in the center
// of the fields widget when there are few, to give space around them». A
// widget whose fields fill its width keeps today's rows, from the start
// edge.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';

const PAGE = JSON.stringify(along('column', widgetPane('repl', 'repl'), widgetPane('bar', 'widgetbar')));
const BAR = 'section[data-pane="bar"] .bar';

/** The bar's box and the box around every field button in it. */
const group = (page) =>
	page.locator(BAR).evaluate((bar) => {
		const box = bar.getBoundingClientRect();
		const rects = [...bar.querySelectorAll('[data-field]')].map((b) => b.getBoundingClientRect());
		return {
			bar: { left: box.left, right: box.right, top: box.top, bottom: box.bottom },
			left: Math.min(...rects.map((r) => r.left)),
			right: Math.max(...rects.map((r) => r.right)),
			top: Math.min(...rects.map((r) => r.top)),
			bottom: Math.max(...rects.map((r) => r.bottom)),
			scrolls: bar.scrollWidth > bar.clientWidth + 1
		};
	});

describe('a few fields in a large fields widget', () => {
	before(startApp);
	after(stopApp);

	for (const [name, viewport] of [
		['a phone', { width: 390, height: 844 }],
		['a desktop', { width: 1280, height: 900 }]
	])
		test(`on ${name}, two fields stand in the middle of the box, both ways`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await seedLines(app, ['setup coffee --verb occur', 'setup mood --verb switch --states false,good,bad']);
					await page.locator(`${BAR} [data-field="mood"]`).waitFor();
					const g = await group(page);
					const free = { left: g.left - g.bar.left, right: g.bar.right - g.right, top: g.top - g.bar.top, bottom: g.bar.bottom - g.bottom };
					assert.ok(free.left > 40 && free.top > 20, `the box has no room to center in: ${JSON.stringify(free)}`);
					assert.ok(Math.abs(free.left - free.right) <= 3, `not centered across: ${JSON.stringify(free)}`);
					assert.ok(Math.abs(free.top - free.bottom) <= 3, `not centered down: ${JSON.stringify(free)}`);
				},
				{ viewport, prepare: (context) => context.addInitScript(draft, PAGE) }
			);
		});

	test('enough fields to fill the width stand in rows from the start edge, as before', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const many = 'alfa bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november oscar papa quebec romeo sierra tango'.split(' ');
				await seedLines(app, many.map((f) => `setup ${f} --verb occur`));
				await page.locator(`${BAR} [data-field="tango"]`).waitFor();
				const g = await group(page);
				assert.ok(g.scrolls, 'the fields do not fill the width: the case is not tested');
				assert.ok(Math.abs(g.left - g.bar.left) <= 1, `the rows do not start at the edge: ${g.left - g.bar.left}px in`);
				const lanes = await page
					.locator(`${BAR} .lane`)
					.evaluateAll((ls) => ls.map((l) => l.firstElementChild.getBoundingClientRect().left));
				assert.ok(lanes.length > 1, 'one row: the wrap is not tested');
				assert.ok(lanes.every((x) => Math.abs(x - lanes[0]) <= 1), `the rows do not all start at the edge: ${lanes}`);
			},
			{ viewport: { width: 390, height: 480 }, prepare: (context) => context.addInitScript(draft, PAGE) }
		);
	});
});
