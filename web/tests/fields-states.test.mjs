// A state field's states, drawn in the fields widget in place of the
// fields (da-g635, da-vg8vr).
//
// The five: `fields` (the pure logic, and the widget as it is drawn),
// `fields-press` (what a press runs), `fields-fit` (at a screen's size,
// and its ink), `fields-box` (the pane's own box) and `fields-states` (a
// state field's states). There was a sixth, `fields-bubbles`, over the
// hold panel's `-t` / `dur` bubbles; da-e78x6 removed the panel and the
// file with it. What more than one of them needs is
// `./support/fields.mjs`; a helper one file uses stays in that file.
//
// Budget (da-lt0k): on node 22 a FILE was the test `run()` timed
// (da-l5k9e: CI now runs node 24), so 120 s was this file's whole budget — `before(startApp)`, the
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
import { until } from './support/until.mjs';

// A state field asks WHICH, so its button reads `name:` and a press puts
// that field's states in place of the fields — the back icon first, then
// `end` (da-cwieh), the `+` last (da-g635), the states around `end`
// (da-7q3ka, da-vg8vr). They are written inside the bar, over its hidden
// rows, so these tests read them out of `.bar` as they read the fields;
// where they stand is `fields-cluster.test.mjs`.
describe('a state field fills the widget with its states', () => {
	before(startApp);
	after(stopApp);

	// ONE APP at 15:00 (da-6b3dj, sharedApp in support/app.mjs): `solo`
	// empties the database each test wrote, and the CLI widget is read from its
	// end. A test may end with the states still drawn: the field going with
	// the database is Back (da-mv8ng), so the next test's `mood:` comes. The
	// scroll test keeps an app of its own, the one on its screen.
	const shared = sharedApp({ clock: todayAt(15) });

	/** Every button under `selector`, in order, by the word on it, the back icon by its name, a state on collect's strip by its field's (da-lw7ue). */
	const words = (page, selector) =>
		page
			.locator(selector)
			.evaluate((el) =>
				[...el.querySelectorAll('button')].map((b) => (b.dataset.back !== undefined ? 'back' : b.dataset.nowField ? `now:${b.dataset.nowField}` : b.textContent.trim()))
			);
	/** Every button collect is showing. */
	const bar = (page) => words(page, 'section.pane[data-title="collect"] .bar');
	/** The buttons of a field's view: back, its time, its states, its keys. */
	const ring = (page) => words(page, '.bar .collect.open');
	const closed = (page) => page.locator('.bar .collect.open').waitFor({ state: 'detached' });

	// da-w6437: a press opens the field, and a state tapped logs at once with
	// the pills, which the run spends, and collect is back at rest.
	test('the button reads `mood:`, a press opens its states, and a state logs at once and closes them', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			assert.equal(await page.locator('.bar [data-field="mood"]').innerText(), 'mood:');

			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-state="happy"]').waitFor();
			assert.deepEqual(await ring(page), ['back', '−q', '−m', '5m', 'calm', 'happy', '+ data']);
			assert.equal(await page.locator('.bar [data-field="mood"]').count(), 0, 'the field is still drawn under its states');

			// A state logs at once (vividn, 16:07: one click).
			const before = (await app.echoes()).length;
			await page.locator('.bar [data-state="happy"]').click();
			await page.waitForFunction((n) => document.querySelectorAll('.cli .echo').length > n, before);
			await app.idle();
			assert.deepEqual((await app.tailRows()).rows.at(-1).state, 'happy');
			// The run spent the pills, and the view closes with them.
			await closed(page);
			assert.deepEqual(await bar(page), ['now:mood', 'mood:']);
		});
	});

	test('the back icon runs nothing and the fields are back', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			const before = (await app.echoes()).length;

			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-back]').click();

			await closed(page);
			assert.deepEqual(await bar(page), ['now:mood', 'mood:']);
			assert.equal(await page.locator('.prompt-bar .pill').count(), 0, 'Back left a pill');
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

	test('the states are painted the colours `fields --json` reports for them', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			const rgb = (hex) =>
				`rgb(${[1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)).join(', ')})`;
			await seed(
				app,
				[
					'setup mood --verb switch --states good,bad --state-color good=#008000 --state-color bad=#ff0000 --color #800080',
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
			const painted = await page.locator('section.pane[data-title="collect"] .bar').evaluate((el) =>
				[...el.querySelectorAll('[data-state]')].map((b) => {
					const cs = getComputedStyle(b);
					return { state: b.dataset.state, bg: cs.backgroundColor, border: cs.borderTopColor, color: cs.color };
				})
			);
			// `good` is what mood is in now, so it is marked the way the field's
			// own chip marks it: filled with the state's colour inside the
			// field's outline. `bad` is a light TINT of its own colour, a quarter
			// of it, in its own outline and under the app's ink (da-7q3ka: the
			// hollow back icon, the `+` and `end` stand out against it).
			// `end` is the commands' now (da-w6437), not a state here.
			assert.deepEqual(painted, [
				{ state: 'good', bg: rgb('#008000'), border: rgb('#800080'), color: 'rgb(255, 255, 255)' },
				{ state: 'bad', bg: 'color(srgb 1 0 0 / 0.25)', border: rgb('#ff0000'), color: await ink(page) }
			]);
		});
	});

	test('the view is in the Tab order top to bottom, and Escape closes it', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			const before = (await app.echoes()).length;
			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-state="happy"]').waitFor();

			const focused = () =>
				page.evaluate(() => (document.activeElement?.dataset.back !== undefined ? 'back' : document.activeElement?.textContent.trim()));
			await page.locator('.bar [data-back]').focus();
			const order = [await focused()];
			for (let i = 0; i < 6; i++) {
				await page.keyboard.press('Tab');
				order.push(await focused());
			}
			assert.deepEqual(order, ['back', '−q', '−m', '5m', 'calm', 'happy', '+ data']);

			await page.keyboard.press('Escape');
			await closed(page);
			assert.equal((await app.echoes()).length, before, 'Escape logged something');
		});
	});

	// The fields are hidden under the view, not taken out: closed, they are
	// panned where they were.
	test('the fields’ pan survives Back', async () => {
		await withApp(async (app) => {
			const { page } = app;
			// One row of more fields than the room holds, so the rows pan (da-lw7ue).
			for (const field of ['alfa', 'bravo', 'charlie', 'delta', 'echo', 'foxtrot', 'golf', 'hotel'])
				await app.submit(`setup ${field} --from-json '${JSON.stringify({ field, verb: 'occur', y: 0.5 })}'`);
			await seed(app, MOOD, 'mood');
			const room = page.locator('section[data-pane="bar"] .room');
			assert.equal(await room.getAttribute('data-pans'), 'x', 'the fields fit their pane — there is nothing this test can prove');
			/** How far the rows are panned, once a glide has stopped. */
			const panned = async () => {
				let last = null;
				await until(
					() => page.locator('section[data-pane="bar"] .spot.shelf[data-section="recent"]').first().evaluate((s) => new DOMMatrix(getComputedStyle(s).transform).m41),
					(now) => ((last === now) || ((last = now), false))
				);
				return last;
			};
			const was = await panned();
			const box = await room.boundingBox();
			const y = box.y + box.height / 2;
			await page.mouse.move(box.x + box.width - 10, y);
			await page.mouse.down();
			await page.mouse.move(box.x + box.width - 30, y, { steps: 3 });
			await page.mouse.move(box.x + box.width - 60, y, { steps: 3 });
			await page.mouse.up();
			const at = await panned();
			assert.ok(at < was, 'the drag did not pan');

			await page.locator('.bar [data-field="mood"]').dispatchEvent('click');
			await page.locator('.bar [data-back]').dispatchEvent('click');
			await closed(page);
			assert.equal(await panned(), at);
		}, { clock: todayAt(15), viewport: { width: 390, height: 640 } });
	});
});
