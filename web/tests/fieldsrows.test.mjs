// The collect widget's order (da-xtkcp, da-w6437). vividn, 2026-09-24: a
// stable order, re-sorted at quiet moments and not while in use; and
// 2026-11-03: «moving the frecent entries to the middle of the pane».
//
// So: the cluster stands the most used first, the first in the middle and
// the rest ring by ring around it (`fields --sort frecency`, ties by `y`);
// the order is taken on load and when the app comes back to the
// foreground; and nothing a person does while it is open — a press, a
// field becoming active, a refresh — moves a button. That last is what
// da-8i1kn saw go wrong: a press on `coffee` set off a refresh that put
// `mood` (just active) where `coffee` had been, under the next tap.
//
// The arrangement itself is pure (`fieldRows` in fields.test.mjs,
// `rowsUnder` in lanes-pure.test.mjs); this is the built app on a seeded
// database, read as the cluster's order.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines } from './support/app.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const BAR = 'section.pane[data-title="collect"] .bar';

/** A field with its `y` set, the way a spec sets it. */
const at = (field, verb, y) => `setup ${field} --from-json '${JSON.stringify({ field, verb, y })}'`;

/**
 * Six fields, three high and three low, used 0–3 times each — so that the
 * bands (by `y`) and the order along them (by use) are different
 * questions with different answers.
 */
const SEED = [
	at('a', 'occur', 0.1),
	at('b', 'occur', 0.2),
	at('c', 'occur', 0.3),
	at('d', 'occur', 0.6),
	at('e', 'start', 0.7),
	at('f', 'occur', 0.8),
	'occur a -t 08:00',
	'occur b -t 08:00',
	'occur b -t 08:01',
	'occur b -t 08:02',
	'occur c -t 08:00',
	'occur c -t 08:01',
	'occur d -t 08:00',
	'occur f -t 08:00',
	'occur f -t 08:01'
];

/** Most used first; a tie by `y`, higher up first. */
const SORTED = ['b', 'c', 'f', 'a', 'd', 'e'];

/** The cluster's order, first (the middle) to last. */
const order = (page) => page.locator(`${BAR} .room .spot [data-field]`).evaluateAll((bs) => bs.map((b) => b.dataset.field));
const saysOrder = (page, want) =>
	until(
		() => order(page),
		(seen) => JSON.stringify(seen) === JSON.stringify(want)
	);

/** The app on that seed, loaded after it. */
async function withOrder(fn) {
	await withApp(async (app) => {
		for (const line of SEED) assert.equal((await app.runLine(line)).code, 0, line);
		await app.reload();
		await app.page.locator(`${BAR} [data-field="e"]`).waitFor();
		await app.idle();
		await fn(app);
	});
}

test('the most used stands first, in the middle, and the rest by use', async () => {
	await withOrder(async ({ page }) => {
		await saysOrder(page, SORTED);
	});
});

test('nothing moves while the app is in use; coming back to the foreground re-sorts', async () => {
	await withOrder(async (app) => {
		const { page } = app;
		await saysOrder(page, SORTED);

		// A real press: `e` starts, and is ACTIVE (da-8i1kn's `mood`).
		await app.pressWidget('e', 'start');
		await page.locator(`${BAR} [data-field="e"].on`).waitFor();
		await app.idle();
		assert.deepEqual(await order(page), SORTED, 'the press moved a button');

		// Writes that WOULD reorder it — `d` is now the most used — and a
		// field that did not exist, all through the bar's own line, so the
		// page refreshes on them as on any command.
		await seedLines(app, [
			'occur d -t 09:00',
			'occur d -t 09:01',
			'occur d -t 09:02',
			at('g', 'occur', 0.65),
			'occur g -t 09:00'
		]);
		await page.locator(`${BAR} [data-field="g"]`).waitFor();
		// The new field stands last; nothing else moved.
		assert.deepEqual(await order(page), [...SORTED, 'g'], 'a refresh moved a button');

		// The page comes back to the foreground: sorted again, from what the CLI now says.
		await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
		const sorted = JSON.parse((await app.run(['fields', '--json', '--sort', 'frecency'])).stdout);
		assert.equal(sorted.reduce((a, b) => ((b.frecency ?? 0) > (a.frecency ?? 0) ? b : a)).field, 'd');
		await until(
			() => order(page),
			(seen) => seen[0] === 'd' && seen.length === 7
		);
	});
});
