// The fields widget answers the selection (da-qu16).
//
// vividn, 2026-09-17 16:44: «if a duration is selected, filter down to
// start/switch based fields … I want to have a smart filter of fields
// depending on the y position of your click. So fields that have a span
// over that y position and occur dots that are close to that position
// should be prioritized in the fields menu.» And at 17:01: «clicking in
// the now region should also reorder fields based on y position.»
//
// Two rules of different kinds, and this file keeps them apart: the
// `dur=` pill FILTERS, the click's height ORDERS. Nothing the height does
// may remove a field, so every case here reads the WHOLE list back.
//
// Delta-blind: every row is pressed where the CLI drew its own mark
// (`.field[data-field]` and the blocks, dots and chips inside it), every
// pill is read off the bar's store, and the bar is asserted as an ordered
// list of names — never as "something moved".
//
// The ranking itself is `lanes-pure.test.mjs`, in node — which is where
// the ORDER within a rank lives too (da-q5jq: «The fields with the closest
// y should appear first in the fields list», and a block outranking a dot
// only while its field is on). No press on this seed's picture can put a
// dot nearer the finger than a row it also reached — the lanes are 6px
// tall and 19px apart — so those cases are made of boxes, not of clicks.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, sharedApp } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';
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

/**
 * The usual order, and what the bar returns to, read row by row (da-xtkcp):
 * each used once, so every row is in the order of the `y` the CLI draws
 * it at (`fields --json`) — and a field on now moves nowhere.
 */
const USUAL = ['coffee', 'run', 'sleep', 'mood'];

const BAR = 'section.pane[data-title="fields"] .bar';
const NOWVIEW = 'section.pane[data-title="nowview"]';

// ONE app for the tests on the default screen (sharedApp in support/app.mjs),
// each in a room of its own so no test meets the widgets the last one moved.
// A test that says how its app is to be opened opens its own.
const shared = sharedApp({ clock: NOW });

/** The app with that seed, its clock pinned on the page and in the worker. */
async function withFields(fn, options) {
	const body = async (app) => {
		for (const argv of SEED) {
			const logged = await app.run(argv);
			assert.equal(logged.code, 0, logged.stderr);
		}
		await app.redraw();
		await app.page.locator(`${BAR} [data-field="sleep"]`).waitFor();
		await fn(app);
	};
	if (!options) return shared.fresh(body);
	await withApp(body, { ...options, clock: NOW });
}

/**
 * Every field button the bar draws, row by row. A press's fields head the
 * rows and every other keeps its own (`rowsUnder`), so one promoted field
 * reads as that field, then the usual order without it.
 */
const shownFields = async (page) => (await rowsOf(page.locator(BAR), '[data-field]')).flat();

/** Wait until the bar reads exactly `want`, and say what it read if it never does. */
const saysFields = (page, want) =>
	until(
		() => shownFields(page),
		(seen) => seen.join() === want.join()
	);

/** The fields the bar MARKED as promoted, and the one the hairline follows. */
const marked = (page) =>
	page.$$eval(`${BAR} .slot`, (slots) => {
		const field = (s) => s.querySelector('[data-field]')?.dataset.field;
		return {
			promoted: slots.filter((s) => s.hasAttribute('data-promoted')).map(field),
			rule: slots.filter((s) => getComputedStyle(s).borderRightWidth !== '0px').map(field)
		};
	});

/** The centre of a mark the picture drew for `field`, on screen. */
async function markOf(page, field, kind = '[data-from]') {
	const box = await page.locator(`${NOWVIEW} svg.svg .field[data-field="${field}"] ${kind}`).first().boundingBox();
	assert.ok(box, `nothing drawn for ${field} in the nowview`);
	return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

const clickAt = (page, { x, y }) => page.mouse.click(x, y);

/** The pills as `kind:text` — what each PUTS on the command line, which is what it means. */
const pillsOf = (page) =>
	page.evaluate(() => window.datum.bar.pills().map((p) => `${p.kind}:${p.text}`));

/**
 * Select a RANGE on the band — shift+drag, the desktop's long press
 * ($lib/nowpoint.ts) — and wait for the `dur=` pill it carries.
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

// ── the click's height: it ORDERS ──────────────────────────────────────

describe('a click on the picture puts the fields drawn there first', () => {
	test('the row clicked leads a two-row bar, nothing is hidden, and the ✕ gives the order back', async () => {
		await withFields(
			async ({ page }) => {
				assert.deepEqual(await shownFields(page), USUAL, 'the usual order');

				// `run`'s own block, as the CLI drew it: its row and its time.
				await clickAt(page, await markOf(page, 'run'));
				await saysFields(page, ['run', ...USUAL.filter((f) => f !== 'run')]);

				const seen = await marked(page);
				assert.deepEqual(seen.promoted, ['run'], 'the promoted field says so');
				assert.deepEqual(seen.rule, ['run'], 'and the hairline falls after it');

				// The fields a press reached head the rows, from the top
				// (da-xtkcp), so the one promoted field is the first on the top
				// row as DRAWN — after the (+), which starts it.
				const rows = await page.$eval(BAR, (bar) => Number(bar.dataset.rows));
				assert.equal(rows, 2, 'this screen was chosen for a two-row bar');
				const boxes = await page.$$eval(`${BAR} [data-field]`, (els) =>
					els.map((el) => ({ field: el.dataset.field, ...el.getBoundingClientRect().toJSON() }))
				);
				const top = Math.min(...boxes.map((b) => b.top));
				const row = boxes.filter((b) => b.top <= top + 1).sort((a, b) => a.left - b.left);
				assert.equal(row[0].field, 'run', `the top row reads ${JSON.stringify(row)}`);

				// The ✕ on the selection's pill — the entry's, since a tap on a
				// mark selects it (da-xg1vi). Clearing the selection clears
				// the lanes it published, and the bar is as it was.
				await page.locator('.prompt-bar .pill.hid button.remove').last().click();
				await until(
					() => pillsOf(page),
					(seen) => seen.length === 0
				);
				await saysFields(page, USUAL);
				assert.deepEqual((await marked(page)).promoted, [], 'and the mark goes with it');
			},
			// Tall enough that the fields pane's share is two button-rows
			// (measured: 86px at 1100, one row at 720).
			{ viewport: { width: 1280, height: 1100 } }
		);
	});

	test('a dot is reached by its HEIGHT, wherever along the row the press fell', async () => {
		await withFields(async ({ page }) => {
			const dot = await markOf(page, 'coffee', '[data-at]');
			await clickAt(page, dot);
			await until(
				() => marked(page),
				(seen) => seen.promoted.join() === 'coffee'
			);
			assert.deepEqual(await shownFields(page), ['coffee', ...USUAL.filter((f) => f !== 'coffee')], 'a dot hides nothing');

			// The SAME row, at another mark's x entirely — minutes away in
			// TIME. vividn, 2026-09-21, on his phone: «it seems occur dots
			// aren't well filtered when clicking at a certain y in the now
			// view. Let's filter.» A block spans every x and so is reached by
			// height alone; a dot was reached by a RADIUS round its centre,
			// so this press — the dot's exact height, and `run`'s block's
			// x — found nothing at all. It is the dot's HEIGHT that decides.
			const run = await markOf(page, 'run');
			assert.ok(Math.abs(run.x - dot.x) > 24, `the two marks are ${Math.abs(run.x - dot.x)}px apart in time`);
			await clickAt(page, { x: run.x, y: dot.y });
			await until(
				() => marked(page),
				(seen) => seen.promoted.join() === 'coffee'
			);
			assert.deepEqual(await shownFields(page), ['coffee', ...USUAL.filter((f) => f !== 'coffee')]);

			// Height still gates it, so the dot is not dragged into every
			// press: `mood`'s own block is a lane and a half below it, and
			// the press that lands squarely there does not reach it.
			await clickAt(page, await markOf(page, 'mood'));
			await until(
				() => marked(page),
				(seen) => seen.promoted.length > 0 && !seen.promoted.includes('coffee')
			);
			assert.ok((await shownFields(page)).includes('coffee'), 'and it is still there to press');
		});
	});
});

// ── a duration on the bar: it FILTERS ──────────────────────────────────

describe('a duration on the bar leaves the fields it can be spent on', () => {
	test('a `dur=` pill leaves the blocks, and dismissing it brings the rest back', async () => {
		await withFields(async ({ page }) => {
			await dragARange(page);
			// The blocks — a `start` field and a `switch`. `coffee`, a point,
			// has no length to be given.
			await saysFields(page, ['run', 'sleep', 'mood']);

			await page.locator('.prompt-bar .pill.dur button.remove').last().click();
			await saysFields(page, USUAL);
		});
	});

	test('it composes with a pending verb: both filters apply', async () => {
		await withFields(async ({ page }) => {
			await page.locator('.prompt-bar input').fill('occur ');
			await saysFields(page, ['coffee']);

			await dragARange(page);
			// The verb leaves the points; the duration takes them away. Both
			// apply, so nothing is left but the `+` that tracks a new field.
			await saysFields(page, []);
			await page.locator(`${BAR} [data-add-field]`).waitFor();

			await page.locator('.prompt-bar .pill.dur button.remove').last().click();
			await saysFields(page, ['coffee']);
		});
	});
});

// ── the now region: a height and no time at all ────────────────────────

describe('the now region re-orders by height alone, on a phone', () => {
	test('a press on a chip in the snapshot selects its entry, no time, and puts that field at the top', async () => {
		await withFields(
			async ({ page }) => {
				// `mobile` draws the snapshot (`history: '0'`) beside the tail
				// and the fields bar under them both: chips, and no time axis
				// for a press to select an instant on (vividn, 17:01).
				await page.locator(`${NOWVIEW} svg.svg .now-chunk`).first().waitFor();
				await clickAt(page, await markOf(page, 'mood', '.now-chunk'));

				await saysFields(page, ['mood', ...USUAL.filter((f) => f !== 'mood')]);
				assert.deepEqual((await marked(page)).promoted, ['mood']);
				const pills = await pillsOf(page);
				assert.equal(pills.length, 1, `the press made ${JSON.stringify(pills)}`);
				assert.match(pills[0], /^hid:/, 'a tap on a chip is its entry (da-xg1vi), and the snapshot has no time');
			},
			{ ...PHONE, home: 'mobile' }
		);
	});

	test('a finger on the bar holds the order until it leaves', async () => {
		await withFields(
			async ({ page }) => {
				await page.locator(`${NOWVIEW} svg.svg .now-chunk`).first().waitFor();
				const mood = await markOf(page, 'mood', '.now-chunk');
				const sleep = await markOf(page, 'sleep', '.now-chunk');

				await page.touchscreen.tap(mood.x, mood.y);
				await saysFields(page, ['mood', ...USUAL.filter((f) => f !== 'mood')]);

				// A pointer comes to rest ON the bar, and a second press
				// elsewhere would re-order it. No button moves out from under
				// it: the bar keeps the order it is being read in.
				// `hover` scrolls it in first: its centre can lie past the
				// bar's edge, off the bar (da-vteda's wider `Track +`).
				await page.locator(`${BAR} [data-field="mood"]`).hover();
				const box = await page.locator(`${BAR} [data-field="mood"]`).boundingBox();
				await page.touchscreen.tap(sleep.x, sleep.y);
				await until(
					() => marked(page),
					(seen) => seen.promoted.join() === 'sleep',
					// The press DID land: the lanes it published are what the
					// bar is holding back from drawing.
					1000
				).then(
					() => assert.fail('the bar re-ordered under the pointer'),
					() => {}
				);
				assert.deepEqual(await shownFields(page), ['mood', ...USUAL.filter((f) => f !== 'mood')]);

				// And it catches up the moment the pointer leaves.
				await page.mouse.move(box.x + box.width / 2, box.y - 200);
				await saysFields(page, ['sleep', ...USUAL.filter((f) => f !== 'sleep')]);
			},
			{ ...PHONE, home: 'mobile' }
		);
	});
});
