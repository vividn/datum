// A few fields sit in the middle of the collect widget's box (da-syu4f,
// da-w6437). vividn, 2026-10-27: «in general fields should appear more in
// the center of the fields widget when there are few, to give space around
// them»; 2026-11-03: the most used in the middle, the rest around it.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';

const PAGE = JSON.stringify(along('column', widgetPane('repl', 'cli'), widgetPane('bar', 'widgetbar')));
const BAR = 'section[data-pane="bar"] .bar';

/** The room's center, the first-ranked field's, and whether every field lies wholly inside the room. */
const group = (page) =>
	page.locator(`${BAR} .room`).evaluate((room) => {
		const box = room.getBoundingClientRect();
		const mid = (r) => ({ x: r.left + r.width / 2, y: r.top + r.height / 2 });
		const rects = [...room.querySelectorAll('[data-field]')].map((b) => b.getBoundingClientRect());
		return {
			room: mid(box),
			first: mid(room.querySelector('.spot [data-field]').getBoundingClientRect()),
			count: rects.length,
			inside: rects.every((r) => r.left >= box.left && r.right <= box.right && r.top >= box.top && r.bottom <= box.bottom)
		};
	});

const near = (a, b) => Math.abs(a.x - b.x) <= 2 && Math.abs(a.y - b.y) <= 2;

// da-w6437: the fields stand in a cluster, the first-ranked in the middle
// of the widget and the rest around it — vividn: «moving the frecent
// entries to the middle of the pane».
describe('a few fields in a large collect widget', () => {
	before(startApp);
	after(stopApp);

	for (const [name, viewport] of [
		['a phone', { width: 390, height: 844 }],
		['a desktop', { width: 1280, height: 900 }]
	])
		test(`on ${name}, the first of two fields stands in the middle of the box, the other beside it`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await seedLines(app, ['setup coffee --verb occur', 'setup mood --verb switch --states false,good,bad']);
					await page.locator(`${BAR} [data-field="mood"]`).waitFor();
					const g = await group(page);
					assert.equal(g.count, 2);
					assert.ok(near(g.first, g.room), `the first at ${JSON.stringify(g.first)}, the middle at ${JSON.stringify(g.room)}`);
					assert.ok(g.inside, 'a field stands past the box');
				},
				{ viewport, prepare: (context) => context.addInitScript(draft, PAGE) }
			);
		});

	test('more fields than the box holds: the first still in the middle, and a drag reaches the rest', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const many = 'alfa bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november oscar papa quebec romeo sierra tango'.split(' ');
				await seedLines(app, many.map((f) => `setup ${f} --verb occur`));
				await page.locator(`${BAR} [data-field="tango"]`).waitFor();
				const g = await group(page);
				assert.equal(g.count, many.length);
				assert.ok(!g.inside, 'every field fits: the case is not tested');
				assert.ok(near(g.first, g.room), `the first at ${JSON.stringify(g.first)}, the middle at ${JSON.stringify(g.room)}`);
				// A field past the room's left edge, dragged into it.
				const room = await page.locator(`${BAR} .room`).boundingBox();
				const out = await page.locator(`${BAR} .room [data-field]`).evaluateAll((bs, left) => bs.map((b) => [b.dataset.field, b.getBoundingClientRect().left]).filter(([, x]) => x < left).sort((a, b) => a[1] - b[1])[0], room.x);
				assert.ok(out, 'nothing past the left edge');
				const [field, x] = out;
				const y = room.y + room.height / 2;
				await page.mouse.move(room.x + 20, y);
				await page.mouse.down();
				await page.mouse.move(room.x + 40, y, { steps: 3 });
				await page.mouse.move(room.x + 40 + (room.x - x) + 10, y, { steps: 5 });
				await page.mouse.up();
				const now = await page.locator(`${BAR} [data-field="${field}"]`).boundingBox();
				assert.ok(now.x >= room.x - 1, `${field} still past the edge: ${now.x} < ${room.x}`);
			},
			{ viewport: { width: 390, height: 480 }, prepare: (context) => context.addInitScript(draft, PAGE) }
		);
	});
});
