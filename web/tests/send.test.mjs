// Send runs the line without a keyboard, and the × stands just before it
// (da-1pzwl).
//
// vividn, 2026-09-25: «Let's add a Enter/Send button on the right side of the
// input box so that you don't have to open the keyboard and hit enter when
// you're doing these more complicated commands. Maybe we move the x button
// clear to right before the > so they don't conflict?»

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp } from './support/app.mjs';
import { DESKTOP, along, widgetPane, ONE_PANE } from './support/layout.mjs';

before(startApp);
after(stopApp);

const shared = sharedApp({ ...DESKTOP });
// `main` as a draft of ONE REPL pane: nothing here reads the day frame,
// and drawing it costs what a boot did (da-m1m9a).
const onSeam = (fn) => shared.solo(fn, ONE_PANE);

const BOX = '.prompt-bar input';
const SEND = '.prompt-bar button[aria-label="run the line"]';
const CLEAR = '.prompt-bar button[aria-label="clear the input"]';
const TIME = { id: 't', kind: 'time', text: '-t 09:00' };
const DATA = { id: 'd', kind: 'note', text: 'cups=2' };

/** Load the line without ever touching the box: a command button's way. */
async function load(page, text, pills) {
	await page.evaluate(([text, pills]) => {
		window.datum.bar.setPills(pills);
		const box = document.querySelector('.prompt-bar input');
		box.value = text;
		box.dispatchEvent(new Event('input', { bubbles: true }));
	}, [text, pills]);
}

describe('Send', () => {
	test('runs a pill-composed line, and the box never takes the focus', async () => {
		await onSeam(async (app) => {
			const { page } = app;
			await page.evaluate(() => {
				window.__focused = false;
				document.querySelector('.prompt-bar input').addEventListener('focus', () => (window.__focused = true));
			});
			await load(page, 'occur sendtest', [DATA]);
			await page.locator(SEND).click();
			// Run: the box is empty, the pill spent, and the entry carries the pill.
			await page.locator(SEND).waitFor({ state: 'detached' });
			await page.waitForFunction(() => !document.querySelector('.prompt-bar .pill'));
			const { rows } = JSON.parse((await app.run(['tail', 'sendtest', '--json'])).stdout);
			assert.equal(rows.length, 1, 'Send did not run the line once');
			assert.match(JSON.stringify(rows[0]), /cups/, 'the pill did not go into the argv');
			assert.equal(await page.locator(BOX).inputValue(), '');
			assert.equal(await page.evaluate(() => window.__focused), false, 'Send put the focus in the box');
		});
	});

	test('is shown only while Enter would run something', async () => {
		await onSeam(async ({ page }) => {
			assert.equal(await page.locator(SEND).count(), 0, 'drawn over an empty line');
			// Pills alone are not a line: Enter runs nothing. Send holds its
			// place beside the × there, but is not there to be pressed.
			await load(page, '', [TIME]);
			await page.locator(CLEAR).waitFor();
			await page.locator(SEND).waitFor({ state: 'hidden' });
			assert.equal(await page.locator(SEND).count(), 1, 'Send left the ×’s side');
			await load(page, '   ', [TIME]);
			await page.evaluate(() => new Promise(requestAnimationFrame));
			assert.equal(await page.locator(SEND).isVisible(), false, 'shown over blanks');
			await load(page, 'occur', []);
			await page.locator(SEND).waitFor();
			await load(page, '', []);
			await page.locator(SEND).waitFor({ state: 'detached' });
		});
	});

	// vremya on PR #213: a locked pill outlives the run, so the × stays — and
	// it must not slide into the spot Send was just pressed in, where a
	// second tap would clear the locked pill.
	test('after a run that leaves a locked pill, the × holds still and Send’s spot clears nothing', async () => {
		await onSeam(async (app) => {
			const { page } = app;
			const LOCKED = { id: 'k', kind: 'note', text: 'cups=3', locked: true };
			await load(page, 'occur sendhold', [LOCKED]);
			const before = await page.locator(CLEAR).boundingBox();
			const send = await page.locator(SEND).boundingBox();
			await page.locator(SEND).click();
			await page.waitForFunction(() => !document.querySelector('.prompt-bar input').value);
			await page.locator(SEND).waitFor({ state: 'hidden' });
			const after = await page.locator(CLEAR).boundingBox();
			assert.equal(after.x, before.x, 'the × moved after the run');
			// The same thumb, the same spot, a moment later.
			await page.mouse.click(send.x + send.width / 2, send.y + send.height / 2);
			await page.evaluate(() => new Promise(requestAnimationFrame));
			assert.deepEqual(
				await page.locator('.prompt-bar .pill').evaluateAll((ps) => ps.map((p) => p.dataset.pill)),
				['k'],
				'a second tap on Send’s spot cleared the locked pill'
			);
			const { rows } = JSON.parse((await app.run(['tail', 'sendhold', '--json'])).stdout);
			assert.equal(rows.length, 1, 'the second tap ran something');
			await load(page, '', []);
		});
	});

	test('the × stands left of it, both 24px targets or more, with a gap between', async () => {
		await onSeam(async ({ page }) => {
			await load(page, 'occur coffee', []);
			const send = await page.locator(SEND).boundingBox();
			const clear = await page.locator(CLEAR).boundingBox();
			for (const [name, b] of [['send', send], ['×', clear]]) {
				assert.ok(b.width >= 24 && b.height >= 24, `${name} is ${b.width}×${b.height}`);
			}
			const gap = send.x - (clear.x + clear.width);
			assert.ok(gap >= 8, `the gap between × and Send is ${gap}px`);
			// Double: a thumb for Send is not a thumb for the ×.
			assert.equal(await page.locator(SEND).evaluate((b) => getComputedStyle(b).borderTopStyle), 'double');
			await load(page, '', []);
		});
	});

	// The same line in a second place (da-3zon): an input widget draws its
	// own Send, and it runs the line.
	test('an input widget draws one too, and it runs the line', async () => {
		const page_ = JSON.stringify(along('column', widgetPane('in', 'input'), widgetPane('repl', 'repl')));
		await shared.solo(async (app) => {
			const { page } = app;
			const WIDGET_SEND = '.input-widget button[aria-label="run the line"]';
			await load(page, 'occur sendwidget', []);
			await page.locator(WIDGET_SEND).click();
			await page.locator(WIDGET_SEND).waitFor({ state: 'detached' });
			const { rows } = JSON.parse((await app.run(['tail', 'sendwidget', '--json'])).stdout);
			assert.equal(rows.length, 1, 'the widget’s Send did not run the line once');
		}, page_);
	});
});
