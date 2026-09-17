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
// The ranking itself is `lanes-pure.test.mjs`, in node.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp, workerClock } from './support/app.mjs';
import { PHONE } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** 15:00 sharp: the nowview's 30-minute window is then 14:30 → 15:00. */
const NOW = Date.UTC(2026, 8, 10, 15, 0, 0);

/**
 * One field of each shape, all four drawn inside that window: an open
 * block, an open switch, a CLOSED block and a dot.
 *
 * Alphabetically — the CLI's one ordering rule, and so the bar's usual
 * order — they are coffee, mood, run, sleep.
 */
const SEED = [
	['start', 'sleep', '-t', '14:10'],
	['switch', 'mood', 'happy', '-t', '14:20'],
	// Ends at 14:40 and lasts five minutes, so the block is 14:35–14:40
	// (da-zwct) and `run` is OFF — the one block a `start` can be spent on.
	['start', 'run', '5m', '-t', '14:40'],
	['occur', 'coffee', '-t', '14:50']
];

/** The usual order: alphabetical, and what the bar returns to. */
const USUAL = ['coffee', 'mood', 'run', 'sleep'];

const BAR = 'section.pane[data-title="fields"] .bar';
const NOWVIEW = 'section.pane[data-title="nowview"]';

/** The app with that seed, its clock pinned on the page and in the worker. */
async function withFields(fn, options = {}) {
	const clock = workerClock(NOW);
	await withApp(
		async (app) => {
			for (const argv of SEED) {
				const logged = await app.run(argv);
				assert.equal(logged.code, 0, logged.stderr);
			}
			await app.reload();
			await app.page.locator(`${BAR} [data-field="sleep"]`).waitFor();
			await fn(app);
		},
		{
			...options,
			prepare: async (context) => {
				await context.clock.setFixedTime(new Date(NOW));
				await clock.prepare(context);
				await options.prepare?.(context);
			}
		}
	);
}

/** Every field button the bar draws, in the order it draws them. */
const shownFields = (page) =>
	page.$$eval(`${BAR} [data-field]`, (els) => els.map((el) => el.dataset.field));

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

				// The scroll rule flows a bar of one or two rows column by
				// column (da-hs6c), so a promoted field is at the LEFT edge.
				const rows = await page.$eval(BAR, (bar) => Number(bar.dataset.rows));
				assert.equal(rows, 2, 'this screen was chosen for a two-row bar');
				const boxes = await page.$$eval(`${BAR} [data-field]`, (els) =>
					els.map((el) => ({ field: el.dataset.field, ...el.getBoundingClientRect().toJSON() }))
				);
				const left = Math.min(...boxes.map((b) => b.left));
				const column = boxes.filter((b) => b.left <= left + 1).sort((a, b) => a.top - b.top);
				assert.equal(column[0].field, 'run', `the left column reads ${JSON.stringify(column)}`);

				// The ✕ on the selection's pill. Clearing the selection clears
				// the lanes it published, and the bar is as it was.
				await page.locator('.prompt-bar .pill.time button.remove').last().click();
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

	test('a dot is reached by DISTANCE: on it coffee is promoted, along its row it is not', async () => {
		await withFields(async ({ page }) => {
			const dot = await markOf(page, 'coffee', '[data-at]');
			await clickAt(page, dot);
			await until(
				() => marked(page),
				(seen) => seen.promoted.join() === 'coffee'
			);
			assert.deepEqual(await shownFields(page), USUAL, 'a dot outranks nothing here, and hides nothing');

			// The same ROW, well off the dot in time. A dot is a few px
			// across and a finger is not, so the reach is a distance in both
			// directions at once — beside it in time is no more on it than
			// beside it in height. `coffee` draws nothing else at that height.
			await clickAt(page, { x: dot.x - 80, y: dot.y });
			await until(
				() => marked(page),
				(seen) => seen.promoted.length === 0
			);
			assert.deepEqual(await shownFields(page), USUAL);
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
			await saysFields(page, ['mood', 'run', 'sleep']);

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
	test('a press on a chip in the snapshot selects no time and puts that field first', async () => {
		await withFields(
			async ({ page }) => {
				// `mobile` draws the snapshot (`history: '0'`) beside the tail
				// and the fields bar under them both: chips, and no time axis
				// for a press to select an instant on (vividn, 17:01).
				await page.locator(`${NOWVIEW} svg.svg .now-chunk`).first().waitFor();
				await clickAt(page, await markOf(page, 'mood', '.now-chunk'));

				await saysFields(page, ['mood', ...USUAL.filter((f) => f !== 'mood')]);
				assert.deepEqual((await marked(page)).promoted, ['mood']);
				assert.deepEqual(await pillsOf(page), [], 'the snapshot has no time to select');
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
				const box = await page.locator(`${BAR} [data-field="mood"]`).boundingBox();
				await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
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
