// The fields bar's rows (da-xtkcp). vividn, 2026-09-24 13:42: «the frecency
// is left to right, the y spec is top to bottom», and at 13:38 a stable
// order, re-sorted at quiet moments and not while in use.
//
// So: rows are equal-count bands of `y`, each most used first
// (`fields --sort frecency`); the order is taken on load and when the app
// comes back to the foreground; and nothing a person does while it is
// open — a press, a field becoming active, a refresh — moves a button.
// That last is what da-8i1kn saw go wrong: a press on `coffee` set off a
// refresh that put `mood` (just active, so «far left») where `coffee` had
// been, under the next tap. A field on now no longer moves at all, and
// nothing moves until the next sort — which the second test holds to by
// reading the rows after a real press and after writes that WOULD change
// the order, then again after the page comes back to the foreground.
//
// The arrangement itself is pure (`fieldRows` in fields.test.mjs,
// `rowsUnder` in lanes-pure.test.mjs); this is the built app on a seeded
// database, read as rows of names.

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, seedLines } from './support/app.mjs';
import { rowsOf } from './support/rows.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const BAR = 'section.pane[data-title="fields"] .bar';

/** A field with its `y` set, the way a spec sets it. */
const at = (field, verb, y) => `spec ${field} --from-json '${JSON.stringify({ field, verb, y })}'`;

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

/** Top to bottom `a b c | d e f`; along each, most used first. */
const SORTED = [
	['b', 'c', 'a'],
	['f', 'd', 'e']
];

const rows = (page) => rowsOf(page.locator(BAR), '[data-field]');
const saysRows = (page, want) =>
	until(
		() => rows(page),
		(seen) => JSON.stringify(seen) === JSON.stringify(want)
	);

/** The app on that seed, loaded after it, with a two-row bar. */
async function withRows(fn) {
	await withApp(
		async (app) => {
			for (const line of SEED) assert.equal((await app.runLine(line)).code, 0, line);
			await app.reload();
			await app.page.locator(`${BAR} [data-field="e"]`).waitFor();
			await app.idle();
			assert.equal(
				await app.page.$eval(BAR, (bar) => Number(bar.dataset.rows)),
				2,
				'this screen was chosen for a two-row bar'
			);
			await fn(app);
		},
		// Tall enough that the fields pane's share is two button-rows
		// (fieldsorder.test.mjs measured it).
		{ viewport: { width: 1280, height: 1100 } }
	);
}

test('rows are equal-count bands of y, top to bottom, and along each the most used comes first', async () => {
	await withRows(async ({ page }) => {
		await saysRows(page, SORTED);
	});
});

test('nothing moves while the app is in use; coming back to the foreground re-sorts', async () => {
	await withRows(async (app) => {
		const { page } = app;
		await saysRows(page, SORTED);

		// A real press: `e` starts, and is ACTIVE — which under the 09-22
		// rule put it at the far left of the bar (da-8i1kn's `mood`).
		await page.locator(`${BAR} [data-field="e"]`).click();
		await page.locator(`${BAR} [data-field="e"].on`).waitFor();
		await app.idle();
		assert.deepEqual(await rows(page), SORTED, 'the press moved a button');

		// Writes that WOULD reorder the bar — `d` is now the most used low
		// field — and a field that did not exist, all through the bar's own
		// line, so the page refreshes on them as on any command.
		await seedLines(app, [
			'occur d -t 09:00',
			'occur d -t 09:01',
			'occur d -t 09:02',
			at('g', 'occur', 0.65),
			'occur g -t 09:00'
		]);
		await page.locator(`${BAR} [data-field="g"]`).waitFor();
		// The new field ends the row its `y` falls in; nothing else moved.
		assert.deepEqual(await rows(page), [SORTED[0], [...SORTED[1], 'g']], 'a refresh moved a button');

		// The page comes back to the foreground: sorted again, from what the
		// CLI now says. Seven fields cut 3 | 4 by y, `d` leading its row,
		// and `g` before `e` — used as much, and higher up.
		await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
		await saysRows(page, [
			['b', 'c', 'a'],
			['d', 'f', 'g', 'e']
		]);
	});
});
