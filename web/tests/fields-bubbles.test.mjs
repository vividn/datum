// The long-press panel's bubbles, and the states list they must not
// follow a press into — the describe below says why.
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
import { startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { MOOD, seed } from './support/fields.mjs';

describe('the panel\'s bubbles and the states list', () => {
	before(startApp);
	after(stopApp);

	// A `-t` belongs to the panel it was set in. It used to outlive it:
	// dismissing the panel left `adjust` standing, and the next plain press
	// spent it on the in-place list — a state logged fifteen minutes ago with
	// nothing on screen saying so (da-vlc0, a regression of da-g635). The list
	// draws no bubbles, so it must carry none.

	/** Long-press `mood:` and set `when` to -15m, then dismiss the panel. */
	async function setThenDismiss(page) {
		await page.locator('.bar [data-field="mood"]').click({ button: 'right' });
		await page.locator('.bar .panel .bubbles button', { hasText: '-15m' }).click();
		await page.keyboard.press('Escape');
		await page.locator('.bar .panel').waitFor({ state: 'detached' });
	}

	/** mood's newest stored entry — `entries()` is the CLI's own rows. */
	const newestMood = async (app) => (await app.entries()).find((e) => e.field === 'mood');

	test('a bubble from a dismissed panel does not follow the plain press into the list', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			await setThenDismiss(page);

			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-state="happy"]').waitFor();
			// The cheap witness: a state's title IS the argv its press runs, so
			// a bubble that ever leaks again says so here first.
			assert.equal(
				await page.locator('.bar [data-state="happy"]').getAttribute('title'),
				'switch mood happy'
			);

			const before = (await app.echoes()).length;
			await page.locator('.bar [data-state="happy"]').click();
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
			assert.equal((await app.echoes()).at(-1), 'switch mood happy');

			const stored = await newestMood(app);
			assert.equal(Date.parse(stored.occur_time.utc), todayAt(15), JSON.stringify(stored.occur_time));
		}, { clock: todayAt(15) });
	});

	test('nor into the `+` at the end of that list', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			await setThenDismiss(page);

			const before = (await app.echoes()).length;
			await page.locator('.bar [data-field="mood"]').click();
			await page.locator('.bar [data-add-state]').click();
			await page.locator('.bar input.new-state').fill('sad');
			await page.locator('.bar input.new-state').press('Enter');
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
			assert.equal((await app.echoes()).at(-1), 'switch mood sad');

			const stored = await newestMood(app);
			assert.equal(stored.state, 'sad', JSON.stringify(stored.state));
			assert.equal(Date.parse(stored.occur_time.utc), todayAt(15), JSON.stringify(stored.occur_time));
		}, { clock: todayAt(15) });
	});

	// The other half of the same claim: the panel is where a bubble IS spent,
	// and it still is.
	test('the long press spends its own bubble on the state it picks', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');

			await page.locator('.bar [data-field="mood"]').click({ button: 'right' });
			await page.locator('.bar .panel .bubbles button', { hasText: '-15m' }).click();

			const before = (await app.echoes()).length;
			await page.locator('.bar .panel .states button', { hasText: 'happy' }).click();
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
			assert.equal((await app.echoes()).at(-1), 'switch mood happy -t -15');

			const stored = await newestMood(app);
			assert.equal(
				Date.parse(stored.occur_time.utc),
				todayAt(14, 45),
				JSON.stringify(stored.occur_time)
			);
		}, { clock: todayAt(15) });
	});

	// Why that test used to fail, and the claim on its own: the panel is held
	// open by field NAME (da-0n3k). The bar's buttons are rebuilt from the
	// fields store on every beat, so an `open === button` identity check went
	// false the next time anything ran — and what ran was usually nothing the
	// reader did, just the refresh still in flight from the last command.
	test('the panel, and the bubble set in it, outlive a beat', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await seed(app, MOOD, 'mood');
			await app.idle();

			await page.locator('.bar [data-field="mood"]').click({ button: 'right' });
			await page.locator('.bar .panel .bubbles button', { hasText: '-15m' }).click();

			// A beat with nothing near the bar: a read-only command through the
			// seam, which bumps the epoch and rebuilds every button all the same.
			await page.evaluate(async () => {
				await window.datum.run(['tail']);
			});
			await app.idle();

			await page.locator('.bar .panel .states button').filter({ hasText: 'happy' }).waitFor();
			assert.equal(
				await page.locator('.bar .panel .bubbles button.on', { hasText: '-15m' }).count(),
				1,
				'the bubble set before the beat'
			);

			const before = (await app.echoes()).length;
			await page.locator('.bar .panel .states button').filter({ hasText: 'happy' }).click();
			await page.waitForFunction((n) => document.querySelectorAll('.repl .echo').length > n, before);
			assert.equal((await app.echoes()).at(-1), 'switch mood happy -t -15');
		}, { clock: todayAt(15) });
	});
});
