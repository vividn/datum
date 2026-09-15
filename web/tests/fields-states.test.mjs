// A state field's states, drawn in the fields widget in place of the
// fields (da-g635).
//
// The six: `fields` (the pure logic, and the widget as it is drawn),
// `fields-press` (what a press runs), `fields-fit` (at a screen's size,
// and its ink), `fields-box` (the pane's own box), `fields-states` (a
// state field's states) and `fields-bubbles` (the long-press panel's
// bubbles against that list). What more than one of them needs is
// `./support/fields.mjs`; a helper one file uses stays in that file.
//
// Budget (da-lt0k): node's `--test-timeout` is per TEST and a FILE is a
// test, so 120 s is this file's whole budget — `before(startApp)`, the
// Chromium it launches, and every test in it. One file held all of this,
// reached 32 tests, and `test timed out after 120000ms` took main's Web
// job red on four of five pushes; a cancelled file hides every other
// file's failures behind it. Measured node 22, the six at concurrency 3 on
// the 24-core build box under a load average of 95: fields 47s, press 60s,
// fit 63s, states 53s, bubbles 35s, box 36s — the one file was 215s
// serial. A group that costs a minute wants a file of its own, not another
// `describe` in one of these.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { pressArgv } from '../src/lib/fields.ts';
import { startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { CLEAR, GREEN, MOOD, seed, untilFieldStyle } from './support/fields.mjs';

// A state field asks WHICH, so its button reads `name:` and a press fills
// the widget with that field's states in place of the fields — `< Back`
// first, `+` last (da-g635). Nothing floats: the list is the widget's own
// content, so these tests read it out of `.bar` exactly as they read the
// fields, and the pane's box is unchanged either way.
describe('a state field fills the widget with its states', () => {
	before(startApp);
	after(stopApp);

	/** Every button the bar is showing, in order, by the word on it. */
	const bar = (page) =>
		page.locator('.bar').evaluate((el) =>
			[...el.querySelectorAll('button')].map((b) => b.textContent.trim())
		);

	test('the button reads `mood:`, a press fills the bar with its states, and one logs a switch', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			assert.equal(await page.locator('.bar [data-field="mood"]').innerText(), 'mood:');

			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-state="happy"]').waitFor();
			assert.deepEqual(await bar(page), ['< Back', 'calm', 'happy', '+']);
			assert.equal(await page.locator('.bar [data-field]').count(), 0, 'a field button is still drawn');

			const before = (await app.echoes()).length;
			await page.locator('.bar [data-state="happy"]').click();
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
			// The argv the menu sent before this bead — `pressArgv`'s, so the
			// bar's pills and the long press's bubbles still reach it.
			assert.equal((await app.echoes()).at(-1), 'switch mood happy');
			await page.locator('.bar [data-field="mood"]').waitFor();
			assert.deepEqual(await bar(page), ['Track +', 'mood:']);
		}, { clock: todayAt(15) });
	});

	test('`< Back` runs nothing and the fields are back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			const before = (await app.echoes()).length;

			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-back]').click();

			await page.locator('.bar [data-field="mood"]').waitFor();
			assert.deepEqual(await bar(page), ['Track +', 'mood:']);
			assert.equal((await app.echoes()).length, before, 'Back logged something');
		}, { clock: todayAt(15) });
	});

	test('`+` takes a state the field has never been in, and it is in the list next time', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			const before = (await app.echoes()).length;

			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-add-state]').click();
			await page.locator('.bar input.new-state').fill('sad');
			await page.locator('.bar input.new-state').press('Enter');
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
			assert.equal((await app.echoes()).at(-1), 'switch mood sad');

			await page.locator('.bar [data-field="mood"]').waitFor();
			await app.idle();
			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-state="sad"]').waitFor();
			assert.deepEqual(await bar(page), ['< Back', 'calm', 'happy', 'sad', '+']);
		}, { clock: todayAt(15) });
	});

	test('the states are painted the colours `fields --json` reports for them', async () => {
		await withApp(async (app) => {
			const { page } = app;
			const rgb = (hex) =>
				`rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			await seed(
				app,
				[
					'spec set mood --default-verb switch --state good --state bad --state-color good=#008000 --state-color bad=#ff0000 --color #800080',
					'switch mood good -t 0930'
				],
				'mood'
			);
			const [listed] = JSON.parse((await app.run(['fields', '--json'])).stdout);
			assert.deepEqual(listed.state_colors, { good: '#008000', bad: '#ff0000' });

			// mood's own chip filled is the widget's statement that it knows
			// which state mood is in — which is what the list marks.
			await untilFieldStyle(page, 'mood', 'backgroundColor', GREEN);
			await page.locator('.bar [data-field="mood"]').click();
			const painted = await page.locator('.bar').evaluate((el) =>
				[...el.querySelectorAll('[data-state]')].map((b) => {
					const cs = getComputedStyle(b);
					return { state: b.dataset.state, bg: cs.backgroundColor, border: cs.borderTopColor, color: cs.color };
				})
			);
			// `good` is what mood is in now, so it is marked the way the field's
			// own chip marks it: filled with the state's colour inside the
			// field's outline. `bad` is outlined in its own colour instead.
			assert.deepEqual(painted, [
				{ state: 'good', bg: rgb('#008000'), border: rgb('#800080'), color: 'rgb(255, 255, 255)' },
				{ state: 'bad', bg: CLEAR, border: rgb('#ff0000'), color: rgb('#ff0000') }
			]);
		}, { clock: todayAt(15) });
	});

	test('the states are in the Tab order in that sequence, and Escape is Back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			const before = (await app.echoes()).length;
			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-state="happy"]').waitFor();

			const focused = () => page.evaluate(() => document.activeElement?.textContent.trim());
			await page.locator('.bar [data-back]').focus();
			const order = [await focused()];
			for (let i = 0; i < 3; i++) {
				await page.keyboard.press('Tab');
				order.push(await focused());
			}
			assert.deepEqual(order, ['< Back', 'calm', 'happy', '+']);

			await page.keyboard.press('Escape');
			await page.locator('.bar [data-field="mood"]').waitFor();
			assert.equal((await app.echoes()).length, before, 'Escape logged something');
		}, { clock: todayAt(15) });
	});

	// The states are fewer than the fields, so the shorter list scrolls the
	// pane's own scroller back up under the reader. Where they were is given
	// back — otherwise Back lands somewhere they never asked for.
	test("the pane's scroll position survives Back", async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const field of ['alfa', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot', 'golf', 'hotel'])
				await app.submit(`spec set ${field} --default-verb occur`);
			await seed(app, MOOD, 'mood');

			const body = page.locator('section[data-pane="bar"] .body');
			await body.evaluate((el) => (el.scrollTop = el.scrollHeight));
			const at = await body.evaluate((el) => el.scrollTop);
			assert.ok(at > 0, 'the bar fits its pane — there is nothing this test can prove');

			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-back]').click();
			await page.locator('.bar [data-field="mood"]').waitFor();
			await page.waitForFunction(
				(top) => document.querySelector('section[data-pane="bar"] .body').scrollTop === top,
				at
			);
		}, { clock: todayAt(15), viewport: { width: 390, height: 640 } });
	});
});
