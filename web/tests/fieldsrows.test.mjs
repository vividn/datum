// The collect widget's order (da-xtkcp, da-w6437, da-lw7ue). vividn,
// 2026-09-24: a stable order, re-sorted at quiet moments and not while in
// use; 2026-11-04: «recency, not frecency, for now», and of a field ending,
// «Since it is the most recent it should always get the first slot».
//
// So: each row stands its fields most recently used first (`fields --sort
// recency`); the order is taken on load and when the app comes back to the
// foreground; and between those nothing moves but a field that starts, into
// the active section, or ends, to the first slot of its row (the ending is
// collect-rest.test.mjs). A press on a moment, a write that would reorder, a
// refresh: none moves a button. da-8i1kn saw that go wrong once: a press on
// `coffee` set off a refresh that put another field under the next tap.
//
// The arrangement itself is pure (`atRest` in fields.test.mjs, `fieldRows`
// in fields.ts); this is the built app on a seeded database, read
// as each row's order.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines } from './support/app.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const BAR = 'section.pane[data-title="collect"] .bar';

/** A field with its `y` set, the way a spec sets it. */
const at = (field, verb, y) => `setup ${field} --from-json '${JSON.stringify({ field, verb, y })}'`;

/** Six fields, three high and three low, used in an order that is neither their names' nor their heights'. */
const SEED = [
	at('a', 'occur', 0.1),
	at('b', 'occur', 0.2),
	at('c', 'occur', 0.3),
	at('d', 'occur', 0.6),
	at('e', 'start', 0.7),
	at('f', 'occur', 0.8),
	'occur d',
	'occur a',
	'occur f',
	'occur c',
	'occur b'
];

/** Last used first; the never used after, by name. */
const SORTED = ['b', 'c', 'f', 'a', 'd', 'e'];

/** The fields not in the active section, row by row, each left to right. */
const rows = (page) =>
	page.locator(`${BAR} .room .spot.shelf[data-section="recent"]`).evaluateAll((spots) => {
		const by = new Map();
		for (const s of spots.sort((a, b) => a.getBoundingClientRect().left - b.getBoundingClientRect().left))
			by.set(s.dataset.row, [...(by.get(s.dataset.row) ?? []), s.querySelector('[data-field]').dataset.field]);
		return [...by.values()];
	});

/** Whether every row stands in `order`'s order, and the rows hold `order` whole. */
const follows = (seen, order) =>
	seen.flat().length === order.length && seen.every((row) => JSON.stringify(row) === JSON.stringify(order.filter((f) => row.includes(f))));

const saysOrder = (page, order) => until(() => rows(page), (seen) => follows(seen, order));

/** The app on that seed, a minute between each line, loaded after it. */
async function withOrder(fn) {
	await withApp(async (app) => {
		for (const line of SEED) {
			assert.equal((await app.runLine(line)).code, 0, line);
			await app.clock.forward(60_000);
		}
		await app.reload();
		await app.page.locator(`${BAR} [data-field="e"]`).waitFor();
		await app.idle();
		await fn(app);
	});
}

test('each row stands its fields last used first', async () => {
	await withOrder(async ({ page }) => {
		await saysOrder(page, SORTED);
	});
});

test('nothing moves while the app is in use but a field starting; coming back to the foreground re-sorts', async () => {
	await withOrder(async (app) => {
		const { page } = app;
		await saysOrder(page, SORTED);

		// A real press: `e` starts, and goes into the active section; the rest stand still.
		await app.pressWidget('e', 'start');
		await page.locator(`${BAR} .spot.shelf[data-section="active"] [data-field="e"]`).waitFor();
		await app.idle();
		const rest = SORTED.filter((f) => f !== 'e');
		assert.ok(follows(await rows(page), rest), `the press moved a button: ${JSON.stringify(await rows(page))}`);

		// Writes that WOULD reorder it — `d` is now the last used — and a field
		// that did not exist, all through the bar's own line, so the page
		// refreshes on them as on any command.
		await app.clock.forward(60_000);
		assert.equal((await app.runLine('occur d')).code, 0);
		await app.clock.forward(60_000);
		await seedLines(app, [at('g', 'occur', 0.65), 'occur g']);
		await page.locator(`${BAR} [data-field="g"]`).waitFor();
		// The new field stands last; nothing else moved.
		assert.ok(follows(await rows(page), [...rest, 'g']), `a refresh moved a button: ${JSON.stringify(await rows(page))}`);

		// The page comes back to the foreground: sorted again, from what the CLI now says.
		await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
		const sorted = JSON.parse((await app.run(['fields', '--json', '--sort', 'recency'])).stdout);
		assert.equal(sorted[0].field, 'g');
		await saysOrder(page, ['g', 'd', 'b', 'c', 'f', 'a']);
	});
});
