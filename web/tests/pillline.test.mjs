// The pills are elements of the line (da-0eqgg).
//
// vividn, 2026-11-06: «on the input bar, you should be able to put text on
// any side of or in between the pills, like the pills are just elements in
// the text. So if I click after all the pills the cursor should go there»,
// and «similarly pressing backspace when the cursor is immediately after a
// pill should delete it». So the line is its segments and its pills in the
// order the reader gave them, Enter runs them in that order, and every
// surface that draws the line draws the same one.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { ON_COLLECT, startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, PHONE } from './support/layout.mjs';
import { drawnLine } from './support/bar.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const BAR = '.prompt-bar';
const AT = { id: 't', kind: 'time', text: '-t 14:30' };
const EARLIER = { id: 'q', kind: 'note', text: '-q' };

/** Load the pills, and put the commands they open back down (da-lnirx). */
async function load(page, pills) {
	await page.evaluate((all) => window.datum.bar.setPills(all), pills);
	await page.locator(`${BAR} .pill`).nth(pills.length - 1).waitFor();
	await page.keyboard.press('Escape');
	await page.locator('#bar-commands').waitFor({ state: 'detached' });
}

/**
 * A press past every pill: on the line's last segment, which runs to the end of
 * the row. Playwright scrolls the line to it (on a phone two pills are wider
 * than the box) and presses where it stands once it stands still, since the
 * line grows a frame after a `fill`.
 */
const pastThePills = (page, frame = BAR) => page.locator(`${frame} .segment.last`).click();

/** Where the caret is: the box's segment, by its `order` (twice its index), and the offset in it. */
const caret = (page, frame = BAR) =>
	page.locator(`${frame} input`).evaluate((box) => ({
		focused: document.activeElement === box,
		segment: Number(getComputedStyle(box).order) / 2,
		at: box.selectionStart
	}));

for (const [screen, options] of [
	['desktop', DESKTOP],
	['phone', PHONE]
]) {
	describe(`on a ${screen}`, () => {
		test('text typed after a pill and between two pills runs in the order it stands', async () => {
			await withApp(async (app) => {
				const { page } = app;
				await load(page, [AT, EARLIER]);
				await page.locator(`${BAR} input`).click();
				await page.keyboard.type('occur');
				await pastThePills(page);
				await page.keyboard.type('cups=2');
				await page.locator(`${BAR} .segment.after:not(.last)`).click();
				await page.keyboard.type('coffee');
				assert.deepEqual(await drawnLine(page), ['occur', '[-t 14:30]', 'coffee', '[-q]', 'cups=2']);

				await page.keyboard.press('Enter');
				await until(app.entries, (es) => es.some((e) => e.field === 'coffee'));
				const { line, result } = await page.evaluate(() => window.datum.bar.last());
				assert.equal(result.code, 0, result.stderr);
				assert.equal(line, 'occur -t 14:30 coffee -q cups=2');
				// Both pills reached the argv: 14:30, then five minutes before it (the clock is held, UTC).
				const entry = (await app.entries()).find((e) => e.field === 'coffee');
				assert.match(entry.at.utc, /T14:25/);
				// The line is spent, segments and pills alike.
				assert.deepEqual(await drawnLine(page), []);
				assert.deepEqual(app.errors, []);
			}, options);
		});

		test('a press after the last pill puts the caret there, and what is typed stands after it', async () => {
			await withApp(async (app) => {
				const { page } = app;
				await load(page, [AT]);
				await page.locator(`${BAR} input`).fill('occur coffee');
				await pastThePills(page);
				assert.deepEqual(await caret(page), { focused: true, segment: 1, at: 0 });
				await page.keyboard.type('cups=2');
				assert.deepEqual(await drawnLine(page), ['occur coffee', '[-t 14:30]', 'cups=2']);
				assert.deepEqual(app.errors, []);
			}, options);
		});

		test('Backspace just after a pill takes it, and an arrow crosses a pill as one character', async () => {
			await withApp(async (app) => {
				const { page } = app;
				await load(page, [AT, EARLIER]);
				await page.locator(`${BAR} input`).fill('occur coffee');
				await pastThePills(page);
				await page.keyboard.type('cups=2');
				for (let i = 0; i < 'cups=2'.length; i++) await page.keyboard.press('ArrowLeft');
				await page.keyboard.press('Backspace');
				// `-q` is gone, and what stood after it joined the segment before it, the caret at the seam.
				assert.deepEqual(await drawnLine(page), ['occur coffee', '[-t 14:30]', 'cups=2']);
				assert.deepEqual(await page.evaluate(() => window.datum.bar.pills().map((p) => p.id)), ['t']);
				assert.deepEqual(await caret(page), { focused: true, segment: 1, at: 0 });
				// One step left crosses `-t 14:30` to the end of `occur coffee`, and Backspace there is a character.
				await page.keyboard.press('ArrowLeft');
				assert.deepEqual(await caret(page), { focused: true, segment: 0, at: 'occur coffee'.length });
				await page.keyboard.press('Backspace');
				assert.deepEqual(await drawnLine(page), ['occur coffe', '[-t 14:30]', 'cups=2']);
				assert.deepEqual(app.errors, []);
			}, options);
		});

		// A pill stood for a word boundary, so taking it from between two words
		// leaves a space there (vremya on PR #1804): Backspace, and its ×.
		for (const [how, take] of [
			['Backspace', async (page) => {
				for (let i = 0; i < 'coffee'.length; i++) await page.keyboard.press('ArrowLeft');
				await page.keyboard.press('Backspace');
			}],
			['its ×', (page) => page.locator(`${BAR} .pill button.remove`).click()]
		])
			test(`a pill taken from between two words by ${how} leaves a space, and the line runs`, async () => {
				await withApp(async (app) => {
					const { page } = app;
					await load(page, [AT]);
					await page.locator(`${BAR} input`).fill('occur');
					await pastThePills(page);
					await page.keyboard.type('coffee');
					await take(page);
					assert.deepEqual(await drawnLine(page), ['occur coffee']);
					if (how === 'Backspace')
						// The caret stays just before `coffee`, where it stood after the pill.
						assert.deepEqual(await caret(page), { focused: true, segment: 0, at: 'occur '.length });
					await page.locator(`${BAR} input`).press('Enter');
					await until(app.entries, (es) => es.some((e) => e.field === 'coffee'));
					assert.equal((await page.evaluate(() => window.datum.bar.last())).line, 'occur coffee');
					assert.deepEqual(app.errors, []);
				}, options);
			});

		test('the bar and an input widget draw the same line, and either runs it', async () => {
			await withApp(async (app) => {
				const { page } = app;
				await app.goto('/input');
				await page.locator('.input-widget input').waitFor();
				await load(page, [AT]);
				await pastThePills(page, '.input-widget');
				await page.keyboard.type('occur coffee');
				for (const frame of [BAR, '.input-widget'])
					assert.deepEqual(await drawnLine(page, frame), ['[-t 14:30]', 'occur coffee'], frame);

				// Nothing typed before the pill, so it follows the verb, as a pill always has.
				await page.keyboard.press('Enter');
				await until(app.entries, (es) => es.some((e) => e.field === 'coffee'));
				const entry = (await app.entries()).find((e) => e.field === 'coffee');
				assert.match(entry.at.utc, /T14:30/);
				for (const frame of [BAR, '.input-widget']) assert.deepEqual(await drawnLine(page, frame), [], frame);
				assert.deepEqual(app.errors, []);
			}, options);
		});
	});
}

describe('the collect widget', () => {
	// A tap spends the pending command (da-ltux), and the command may be typed
	// after a pill now: what it spends is that word, wherever it was typed.
	test('a command typed after a pill is spent by a tap on a field', async () => {
		await withApp(
			async (app) => {
				const { page } = app;
				assert.equal((await app.runLine('setup coffee --verb occur')).code, 0);
				await app.reload();
				const coffee = page.locator('section.pane[data-title="collect"] .bar .collect .room [data-field="coffee"]');
				await coffee.waitFor();
				await load(page, [AT]);
				await pastThePills(page);
				await page.keyboard.type('occur');
				await coffee.click();
				await until(app.entries, (es) => es.some((e) => e.field === 'coffee'));
				const entry = (await app.entries()).find((e) => e.field === 'coffee');
				assert.match(entry.at.utc, /T14:30/);
				await until(() => drawnLine(page), (line) => line.length === 0);
				assert.deepEqual(app.errors, []);
			},
			{ ...PHONE, home: ON_COLLECT }
		);
	});
});
