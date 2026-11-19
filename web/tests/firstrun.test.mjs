// The first run, end to end, as a newcomer takes it on a phone (da-gy470): welcome, setup, the tour, and the
// first entry shown. Each piece has its own test (welcome, field, tour, fields-press); this one holds the PATH
// between them, which none of those can fail on: a tab no longer reached, a tour that never lets go, a first
// entry that logs and is not shown. vividn named it a standing priority: «vastly improving user onboarding flow».
//
// Every step is a tap but one: the field's name is typed, as there is no other way to give it one. No words of
// the welcome, setup or the tour are read here (bar.test.mjs drives the typed path).

import test, { before, after } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { FIELD_FORM, nameField, setupCommand } from './support/field.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

/** A phone that has never been here, the first field's tour still to come. */
const NEVER_HERE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true }, home: null, tour: true };

/** The root tab in front, by the layout it holds. */
const front = (page) =>
	page.evaluate(() => {
		const { tabs, active } = window.datum.layout.current().root;
		return tabs[active].layout;
	});

test('a never-visited phone: Track something, a field named and tracked, the tour tapped through, one tap on its chip, and the entry shown', async () => {
	await withApp(async (app) => {
		const { page } = app;
		const rows = async () => JSON.parse((await app.run(['tail', 'water', '--json'])).stdout).rows;

		// 1. Welcome in front; Track something brings setup, at its question.
		await until(() => front(page), (name) => name === 'welcome');
		await page.locator('.host.welcome').getByRole('button', { name: 'Track something', exact: true }).tap();
		await until(() => front(page), (name) => name === 'setup');
		await page.locator('.host.setup h3', { hasText: 'What do you want to track?' }).waitFor();

		// 2. The field named: the one typed step of the flow.
		await nameField(page, 'water');

		// 3. A moment, and start tracking: water is tracked.
		await page.locator(`${FIELD_FORM} [data-verb="occur"]`).tap();
		await setupCommand(page);
		const tracked = await until(rows, (r) => r.some((row) => row.state === 'TRACKED'));

		// 4. The tour takes every tap until it ends: its button, until it is gone.
		const tour = page.locator('.tour');
		const said = () => page.evaluate(() => document.querySelector('.tour .say p')?.textContent ?? null);
		await tour.waitFor();
		let stops = 0;
		for (let now = await said(); now !== null; stops++) {
			assert.ok(stops < 10, 'the tour never let go');
			await tour.locator('button.next').tap();
			now = await until(said, (next) => next !== now);
		}
		assert.ok(stops > 0, 'no tour stood after the first field');
		assert.equal(await tour.count(), 0);
		assert.deepEqual(await rows(), tracked, 'the tour logged something');

		// 5. One tap on the chip logs it, and the entry is SHOWN: its row in the tail pane on screen. The clock is
		// held, so it moves on as a person's would (4 s on datum.day, 11-18): at TRACKED's own instant the tap
		// would be the same entry.
		await app.clock.forward(4000);
		await page.locator('[data-field="water"][data-act="run"]:visible').tap();
		const [logged] = await until(
			async () => (await rows()).filter((row) => !tracked.some((t) => t.hid === row.hid)),
			(fresh) => fresh.length === 1
		);
		const row = page.locator(`section.pane[data-title="tail"] tbody tr[data-hid="${logged.hid}"]`);
		await row.waitFor(); // visible, not only in the DOM
		assert.deepEqual(app.errors, []);
	}, NEVER_HERE);
});
