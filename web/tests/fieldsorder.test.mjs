// The collect widget answers the selection (da-qu16).
//
// vividn, 2026-09-17 16:44: «if a duration is selected, filter down to
// start/switch based fields». So a length pill FILTERS: it leaves the fields
// a length can be spent on, and composes with a pending command.
//
// The same ask ordered the fields by the height of a press on a picture;
// that went with the collect widget's redesign (da-g5y5o, vividn,
// 2026-11-11: «fields always appear in their given row anyway, so this
// filtering basically does nothing anymore»).
//
// Delta-blind: every pill is read off the bar's store, and the bar is
// asserted as an ordered list of names — never as "something moved".

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, sharedApp } from './support/app.mjs';
import { until } from './support/until.mjs';
import { rowsOf } from './support/rows.mjs';

before(startApp);
after(stopApp);

/** 15:00 sharp: the nowview's 30-minute window is then 14:30 → 15:00. */
const NOW = Date.UTC(2026, 8, 10, 15, 0, 0);

/**
 * One field of each shape, all four drawn inside that window: an open
 * block, an open switch, a CLOSED block and a dot.
 */
const SEED = [
	['start', 'sleep', '-t', '14:10'],
	['switch', 'mood', 'happy', '-t', '14:20'],
	// Ends at 14:40 and lasts five minutes, so the block is 14:35–14:40
	// (da-zwct) and `run` is OFF — the one block a `start` can be spent on.
	['start', 'run', '5m', '-t', '14:40'],
	['occur', 'coffee', '-t', '14:50']
];

const BAR = 'section.pane[data-title="collect"] .bar';
const NOWVIEW = 'section.pane[data-title="nowview"]';

// ONE app for the tests on the default screen (sharedApp in support/app.mjs),
// each in a room of its own so no test meets the widgets the last one moved.
const shared = sharedApp({ clock: NOW });

/** The app with that seed, its clock pinned on the page and in the worker. */
const withFields = (fn) =>
	shared.fresh(async (app) => {
		for (const argv of SEED) {
			const logged = await app.run(argv);
			assert.equal(logged.code, 0, logged.stderr);
		}
		await app.redraw();
		await app.page.locator(`${BAR} [data-field="sleep"]`).waitFor();
		await fn(app);
	});

/** Every field button the bar draws, row by row. */
const shownFields = async (page) => (await rowsOf(page.locator(BAR), '[data-field]')).flat();

/** Wait until the bar reads exactly `want`, and say what it read if it never does. */
const saysFields = (page, want) =>
	until(
		() => shownFields(page),
		(seen) => seen.join() === want.join()
	);

/**
 * The centre of a mark the picture drew for `field`, on screen, or the nearest
 * point along it no window handle takes: a diamond stands on its axis's
 * rightmost tick (da-273ug) and is grabbed a little way into the band.
 */
async function markOf(page, field, kind = '[data-from]') {
	const box = await page.locator(`${NOWVIEW} svg.svg .field[data-field="${field}"] ${kind}`).first().boundingBox();
	assert.ok(box, `nothing drawn for ${field} in the nowview`);
	const y = box.y + box.height / 2;
	for (const dx of [0, -40, 40, -80, 80]) {
		const x = box.x + box.width / 2 + dx;
		if (x < box.x || x > box.x + box.width) continue;
		const free = await page.evaluate(({ x, y }) => !document.elementFromPoint(x, y)?.closest('.nowview-handles'), { x, y });
		if (free) return { x, y };
	}
	assert.fail(`every point along ${field}'s mark is under a handle`);
}

/** The pills as `kind:text` — what each PUTS on the command line, which is what it means. */
const pillsOf = (page) =>
	page.evaluate(() => window.datum.bar.pills().map((p) => `${p.kind}:${p.text}`));

/**
 * Select a RANGE on the band — shift+drag, the desktop's long press
 * (#lib/nowpoint.ts) — and wait for the length pill (`dur`) it carries.
 *
 * Backwards along `sleep`'s block, which runs the whole window across, so
 * the drag stays inside the picture whatever width the pane is given.
 */
async function dragARange(page) {
	const from = await markOf(page, 'sleep');
	await page.mouse.move(from.x, from.y);
	await page.keyboard.down('Shift');
	await page.mouse.down();
	await page.mouse.move(from.x - 60, from.y, { steps: 8 });
	await page.mouse.up();
	await page.keyboard.up('Shift');
	await until(
		() => pillsOf(page),
		(seen) => seen.some((p) => p.startsWith('dur:'))
	);
}

// ── a duration on the bar: it FILTERS ──────────────────────────────────

describe('a duration on the bar leaves the fields it can be spent on', () => {
	test('a length pill leaves the blocks, and dismissing it brings the rest back', async () => {
		await withFields(async ({ page }) => {
			const usual = await shownFields(page);
			await dragARange(page);
			// The blocks — a `start` field and a `switch`. `coffee`, a point,
			// has no length to be given.
			await saysFields(page, usual.filter((f) => f !== 'coffee'));

			await page.locator('.prompt-bar .pill.dur button.remove').last().click();
			await saysFields(page, usual);
		});
	});

	test('it composes with a pending verb: both filters apply', async () => {
		await withFields(async ({ page }) => {
			await page.locator('.prompt-bar input').fill('occur ');
			await saysFields(page, ['coffee']);

			await dragARange(page);
			// The verb leaves the points; the duration takes them away. Both
			// apply, so nothing is left.
			await saysFields(page, []);

			await page.locator('.prompt-bar .pill.dur button.remove').last().click();
			await saysFields(page, ['coffee']);
		});
	});
});
