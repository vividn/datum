// The collect widget's box holds its fields (da-syu4f, da-lw7ue). vividn,
// 2026-10-27, asked for a few fields «more in the center of the fields
// widget»; 2026-11-04 replaced the centre: «on the far left (maybe 20%) of the
// collect widget, let's draw the current state … So then you pan to the right
// to see more and more distantly used fields». So a few fields stand inside
// the box, past the strip; more than it holds run past its right edge, and a
// drag reaches them. Where each stands is collect-rest.test.mjs.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines } from './support/app.mjs';
import { along, draft, widgetPane } from './support/layout.mjs';

const PAGE = JSON.stringify(along('column', widgetPane('repl', 'cli'), widgetPane('bar', 'widgetbar')));
const BAR = 'section[data-pane="bar"] .bar';

/** How many fields the room holds, and whether every one lies wholly inside it, past the strip. */
const group = (page) =>
	page.locator(`${BAR} .room`).evaluate((room) => {
		const box = room.getBoundingClientRect();
		const rects = [...room.querySelectorAll('[data-field]')].map((b) => b.getBoundingClientRect());
		const strip = box.left + box.width / 5;
		return {
			count: rects.length,
			inside: rects.every((r) => r.left >= strip && r.right <= box.right && r.top >= box.top && r.bottom <= box.bottom)
		};
	});

describe('a few fields in a large collect widget', () => {
	before(startApp);
	after(stopApp);

	for (const [name, viewport] of [
		['a phone', { width: 390, height: 844 }],
		['a desktop', { width: 1280, height: 900 }]
	])
		test(`on ${name}, two fields stand inside the box, past the strip`, async () => {
			await withApp(
				async (app) => {
					const { page } = app;
					await seedLines(app, ['setup coffee --verb occur', 'setup mood --verb switch --states false,good,bad']);
					await page.locator(`${BAR} [data-field="mood"]`).waitFor();
					const g = await group(page);
					assert.equal(g.count, 2);
					assert.ok(g.inside, 'a field stands past the box, or over the strip');
				},
				{ viewport, prepare: (context) => context.addInitScript(draft, PAGE) }
			);
		});

	test('more fields than the box holds run past its right edge, and a drag reaches them', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				const many = 'alfa bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november oscar papa quebec romeo sierra tango'.split(' ');
				await seedLines(app, many.map((f) => `setup ${f} --verb occur`));
				await page.locator(`${BAR} [data-field="tango"]`).waitFor();
				const g = await group(page);
				assert.equal(g.count, many.length);
				assert.ok(!g.inside, 'every field fits: the case is not tested');
				// The field nearest past the room's right edge, dragged into it.
				const room = await page.locator(`${BAR} .room`).boundingBox();
				const right = room.x + room.width;
				const out = await page.locator(`${BAR} .room [data-field]`).evaluateAll((bs, right) => bs.map((b) => [b.dataset.field, b.getBoundingClientRect().right]).filter(([, x]) => x > right).sort((a, b) => a[1] - b[1])[0], right);
				assert.ok(out, 'nothing past the right edge');
				const [field, x] = out;
				const y = room.y + room.height / 2;
				await page.mouse.move(right - 20, y);
				await page.mouse.down();
				await page.mouse.move(right - 40, y, { steps: 3 });
				await page.mouse.move(right - 40 - (x - right) - 10, y, { steps: 5 });
				await page.mouse.up();
				const now = await page.locator(`${BAR} [data-field="${field}"]`).boundingBox();
				assert.ok(now.x + now.width <= right + 1, `${field} still past the edge: ${now.x + now.width} > ${right}`);
			},
			{ viewport: { width: 390, height: 480 }, prepare: (context) => context.addInitScript(draft, PAGE) }
		);
	});
});
