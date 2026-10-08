// A state field's states, drawn in the fields widget in place of the
// fields (da-g635).
//
// The five: `fields` (the pure logic, and the widget as it is drawn),
// `fields-press` (what a press runs), `fields-fit` (at a screen's size,
// and its ink), `fields-box` (the pane's own box) and `fields-states` (a
// state field's states). There was a sixth, `fields-bubbles`, over the
// hold panel's `-t` / `dur` bubbles; da-e78x6 removed the panel and the
// file with it. What more than one of them needs is
// `./support/fields.mjs`; a helper one file uses stays in that file.
//
// Budget (da-lt0k): node's `--test-timeout` is per TEST and a FILE is a
// test, so 120 s is this file's whole budget — `before(startApp)`, the
// Chromium it launches, and every test in it. One file held all of this,
// reached 32 tests, and `test timed out after 120000ms` took main's Web
// job red on four of five pushes; a cancelled file hides every other
// file's failures behind it. Measured node 22, at concurrency 3 on the
// 24-core build box under a load average of 95: fields 47s, press 60s,
// fit 63s, states 53s, box 36s (and bubbles, now gone, 35s) — the one
// file was 215s serial. A group that costs a minute wants a file of its own, not another
// `describe` in one of these.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { pressArgv } from '../src/lib/fields.ts';
import { sharedApp, startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { GREEN, MOOD, seed, untilFieldStyle } from './support/fields.mjs';

// A state field asks WHICH, so its button reads `name:` and a press fills
// the widget with that field's states in place of the fields — `< Back`
// first, then `end` (da-cwieh), `+ Add state` last (da-g635), the states around the tap (da-7q3ka). Nothing floats: the list is the widget's own
// content, so these tests read it out of `.bar` exactly as they read the
// fields, and the pane's box is unchanged either way.
describe('a state field fills the widget with its states', () => {
	before(startApp);
	after(stopApp);

	// ONE APP at 15:00 (da-6b3dj, sharedApp in support/app.mjs): `solo`
	// empties the database each test wrote, and the REPL is read from its
	// end. A test may end with the states still drawn: the field going with
	// the database is Back (da-mv8ng), so the next test's `mood:` comes. The
	// scroll test keeps an app of its own, the one on its screen.
	const shared = sharedApp({ clock: todayAt(15) });

	/** Every button the bar is showing, in order, by the word on it. */
	const bar = (page) =>
		page.locator('.bar').evaluate((el) =>
			[...el.querySelectorAll('button')].map((b) => b.textContent.trim())
		);

	test('the button reads `mood:`, a press fills the bar with its states, and one logs a switch', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			assert.equal(await page.locator('.bar [data-field="mood"]').innerText(), 'mood:');

			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-state="happy"]').waitFor();
			assert.deepEqual(await bar(page), ['< Back', 'end', 'calm', 'happy', '+ Add state']);
			assert.equal(await page.locator('.bar [data-field]').count(), 0, 'a field button is still drawn');

			const before = (await app.echoes()).length;
			await page.locator('.bar [data-state="happy"]').click();
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
			// The argv the menu sent before this bead — `pressArgv`'s, so the
			// bar's pills still reach it. (The hold panel's bubbles reached it
			// too until da-e78x6 removed them; the pills are what is left.)
			assert.equal((await app.echoes()).at(-1), 'switch mood happy');
			await page.locator('.bar [data-field="mood"]').waitFor();
			assert.deepEqual(await bar(page), ['+', 'mood:']);
		});
	});

	test('`< Back` runs nothing and the fields are back', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			const before = (await app.echoes()).length;

			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-back]').click();

			await page.locator('.bar [data-field="mood"]').waitFor();
			assert.deepEqual(await bar(page), ['+', 'mood:']);
			assert.equal((await app.echoes()).length, before, 'Back logged something');
		});
	});

	// The field going away under an open list — a delete, a restore, another
	// tab, here the database emptied — is Back (da-mv8ng). Held instead, the
	// list waited for a field of that name and took the bar over again.
	test('the states give way to the fields when their field is gone, and stay gone when it returns', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-state="happy"]').waitFor();

			await app.empty();
			await seed(app, ['occur coffee'], 'coffee');
			assert.equal(await page.locator('.bar [data-back]').count(), 0, 'the states outlived their field');

			await seed(app, MOOD, 'mood');
			assert.equal(await page.locator('.bar [data-back]').count(), 0, 'the states came back with the field');
		});
	});

	test('`+` takes a state the field has never been in, and it is in the list next time', async () => {
		await shared.solo(async (app) => {
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
			assert.deepEqual(await bar(page), ['< Back', 'end', 'calm', 'happy', 'sad', '+ Add state']);
		});
	});

	test('the states are painted the colours `fields --json` reports for them', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const rgb = (hex) =>
				`rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			await seed(
				app,
				[
					'spec set mood --verb switch --state good --state bad --state-color good=#008000 --state-color bad=#ff0000 --color #800080',
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
			const ink = (page) =>
				page.evaluate(() => {
					const probe = document.body.appendChild(document.createElement('span'));
					probe.style.color = 'var(--ink-bright)';
					const color = getComputedStyle(probe).color;
					probe.remove();
					return color;
				});
			const painted = await page.locator('.bar').evaluate((el) =>
				[...el.querySelectorAll('[data-state]')].map((b) => {
					const cs = getComputedStyle(b);
					return { state: b.dataset.state, bg: cs.backgroundColor, border: cs.borderTopColor, color: cs.color };
				})
			);
			// `good` is what mood is in now, so it is marked the way the field's
			// own chip marks it: filled with the state's colour inside the
			// field's outline. `bad` is a light TINT of its own colour, a quarter
			// of it, in its own outline and under the app's ink (da-7q3ka: the
			// hollow `< Back`, `+ Add state` and `end` stand out against it).
			// `end` first, hollow in the field's colour, on every switch field (da-cwieh).
			assert.deepEqual(painted, [
				{ state: 'false', bg: 'rgba(0, 0, 0, 0)', border: rgb('#800080'), color: rgb('#800080') },
				{ state: 'good', bg: rgb('#008000'), border: rgb('#800080'), color: 'rgb(255, 255, 255)' },
				{ state: 'bad', bg: 'color(srgb 1 0 0 / 0.25)', border: rgb('#ff0000'), color: await ink(page) }
			]);
		});
	});

	test('the states are in the Tab order in that sequence, and Escape is Back', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			const before = (await app.echoes()).length;
			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-state="happy"]').waitFor();

			const focused = () => page.evaluate(() => document.activeElement?.textContent.trim());
			await page.locator('.bar [data-back]').focus();
			const order = [await focused()];
			for (let i = 0; i < 4; i++) {
				await page.keyboard.press('Tab');
				order.push(await focused());
			}
			assert.deepEqual(order, ['< Back', 'end', 'calm', 'happy', '+ Add state']);

			await page.keyboard.press('Escape');
			await page.locator('.bar [data-field="mood"]').waitFor();
			assert.equal((await app.echoes()).length, before, 'Escape logged something');
		});
	});

	// The states are fewer than the fields, so the shorter list scrolls the
	// pane's own scroller back up under the reader. Where they were is given
	// back — otherwise Back lands somewhere they never asked for.
	test("the pane's scroll position survives Back", async () => {
		await withApp(async (app) => {
			const { page } = app;
			for (const field of ['alfa', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot', 'golf', 'hotel'])
				await app.submit(`spec set ${field} --verb occur`);
			await seed(app, MOOD, 'mood');

			// The scroller is the BAR, and which way it goes is the scroll
			// rule's (da-ltux): a pane this short is one row reached sideways.
			const bar = page.locator('section[data-pane="bar"] .bar');
			const at = await bar.evaluate((el) => {
				el.scrollLeft = el.scrollWidth;
				el.scrollTop = el.scrollHeight;
				return { left: el.scrollLeft, top: el.scrollTop };
			});
			assert.ok(at.left > 0 || at.top > 0, 'the bar fits its pane — there is nothing this test can prove');

			// Pressed where it is, unscrolled to: mood is on, so it stands at
			// the start of the bar (da-vu7pi), and a click that scrolled to it
			// would move the bar to where Back is meant to leave it anyway.
			await page.locator('.bar [data-field="mood"]').dispatchEvent('click');
			await page.locator('.bar [data-back]').dispatchEvent('click');
			await page.locator('.bar [data-field="mood"]').waitFor();
			await page.waitForFunction(
				(was) => {
					const el = document.querySelector('section[data-pane="bar"] .bar');
					return el.scrollLeft === was.left && el.scrollTop === was.top;
				},
				at
			);
		}, { clock: todayAt(15), viewport: { width: 390, height: 640 } });
	});
});
