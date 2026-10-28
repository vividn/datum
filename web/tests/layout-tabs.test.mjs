// Every tab of a pane is mounted, not only the one in front (da-rjmg).
//
// vividn, 2026-09-16: "preload and keep fresh the display of all tabs in a
// widget even when not displayed so that swiping looks nicer and all the
// data is ready to go." So a tab behind is drawn before it is swiped to,
// the swipe runs nothing, a write refreshes every tab and not only the one
// in front, the widget's own controls on the bar are the FRONT tab's alone,
// and a tab behind takes no focus and no key.
//
// Runs are counted through the seam (`spyOnRunner`) with the clock pinned,
// so the minute tick cannot land a run inside a count (da-r61i).

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { sharedApp, startApp, stopApp, todayAt, withApp } from './support/app.mjs';
import { DESKTOP, PHONE, activeTab, addTab, leaveLayoutMode, pane, swipeOver } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

// ONE APP for the two desktop tests with no clock (da-6b3dj, sharedApp in
// support/app.mjs): the tabs they add are `main`'s edits, which `solo` puts
// back. The rest keep apps of their own: each is alone on its options, and
// one opens a second tab.
const shared = sharedApp(DESKTOP);

/** A phone's `main`: strip / CLI widget / [tail, nowview, custom] / fields. */
const PHONE_DAY = { ...PHONE, clock: todayAt(15) };

/** The bodies of `id`'s tabs that are behind the one in front. */
const behind = (page, id) => pane(page, id).locator('> .content > .tab-body.behind');

describe('every tab of a pane is mounted', () => {
	test('a tab behind is drawn before the swipe, and the swipe runs nothing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			assert.deepEqual(await pane(page, 'side').locator('[role="tab"]').allInnerTexts(), ['tail', 'nowview', 'custom']);
			assert.equal(await behind(page, 'side').count(), 2);
			// The nowview, second, has its picture already.
			await behind(page, 'side').first().locator('svg').first().waitFor({ state: 'attached' });

			const seen = await app.spyOnRunner();
			await app.idle();
			const content = pane(page, 'side').locator('> .content');
			await swipeOver(page, content, -150, 0);
			await until(() => activeTab(page, 'side'), (name) => name === 'nowview');
			await page.waitForFunction(() => !document.querySelector('section[data-pane="side"] > .content.moving'));
			await app.idle();
			assert.deepEqual(await seen(), [], 'the swipe ran a command');
			assert.ok(await content.locator('> .tab-body:not(.behind) svg').first().isVisible(), 'the nowview is not on screen');
			assert.deepEqual(app.errors, []);
		}, PHONE_DAY);
	});

	test('a write in another tab refreshes every tab, once each', async () => {
		await withApp(async (first) => {
			const second = await first.newTab();
			const seen = await second.spyOnRunner();
			await second.idle();
			assert.equal(await first.run(['occur', 'kettle']).then((r) => r.code), 0);
			// The tail in front, and the nowview and custom behind it.
			await until(
				() => second.page.locator('section[data-pane="side"] .behind').first().innerHTML(),
				(html) => html.includes('kettle')
			);
			await second.idle();
			// A day's data layer drawn (`--pictures`) is the nowview's and the
			// strip's at once (da-m6i2a); a check (`--digests`) is the strip's of
			// the days before today, as it ever was.
			const runs = (await seen()).map((argv) => (argv.includes('--pictures') ? 'data-layer' : argv[0]));
			const count = (name) => runs.filter((r) => r === name).length;
			assert.equal(count('fields'), 1, `not one refresh: ${runs}`);
			// tail's `tail --json -n N`, the nowview's day's data layer (its overlay holds
			// no data, da-0e06p), custom's default `now --json`.
			assert.equal(count('data-layer'), 1, `the nowview behind: ${runs}`);
			assert.equal(count('now'), 1, `the custom pane behind: ${runs}`);
			assert.ok(count('tail') >= 1, `the tail in front: ${runs}`);
			assert.deepEqual(second.errors, []);
		}, PHONE_DAY);
	});
});

// The trap da-ndjo left: the bar's `.own` box hosts the ACTIVE widget's
// controls, which was the same as the mounted one until every tab was.
describe("the widget's own controls, with every tab mounted", () => {
	/** The ✎ buttons of `id` that a reader can see. */
	const pencils = (page, id) =>
		pane(page, id)
			.locator('button.edit')
			.evaluateAll((els) => els.filter((el) => el.checkVisibility({ visibilityProperty: true })).length);

	test('the ✎ is on the bar once, for the tab in front, and moves with it', async () => {
		await shared.solo(async ({ page }) => {
			await addTab(page, 'custom', 'custom');
			await addTab(page, 'custom', 'aliases');
			const custom = pane(page, 'custom');
			const tabs = custom.locator('> .titlebar [role="tab"]');
			assert.deepEqual(await tabs.allInnerTexts(), ['custom', 'custom', 'aliases']);
			const own = custom.locator('> .titlebar .own');

			await tabs.nth(0).click();
			await own.locator('button.edit').waitFor();
			assert.equal(await pencils(page, 'custom'), 1);
			// Give the first its own command, so the two are told apart.
			await own.locator('button.edit').click();
			await own.locator('input.command').fill('tail -n 1');
			await own.locator('input.command').press('Enter');
			await own.filter({ hasText: 'tail -n 1' }).waitFor();

			await tabs.nth(1).click();
			await until(() => own.innerText(), (text) => !text.includes('tail -n 1'));
			assert.equal(await pencils(page, 'custom'), 1, 'the second tab does not have the bar alone');

			await tabs.nth(2).click();
			await own.waitFor({ state: 'detached' });
			assert.equal(await pencils(page, 'custom'), 0, 'a ✎ is on screen over the aliases');

			await tabs.nth(0).click();
			await own.filter({ hasText: 'tail -n 1' }).waitFor();
			assert.equal(await pencils(page, 'custom'), 1);
		});
	});
});

describe('a tab behind takes no focus and no key', () => {
	test('its input is not reached by Tab or by focus(), and its Enter runs nothing', async () => {
		await withApp(async (app) => {
			const { page } = app;
			await addTab(page, 'tail', 'input');
			const tail = pane(page, 'tail');
			await tail.locator('[role="tab"]', { hasText: 'tail' }).click();
			const hidden = behind(page, 'tail').locator('input, textarea, [contenteditable="true"]').first();
			await hidden.waitFor({ state: 'attached' });

			const seen = await app.spyOnRunner();
			await app.idle();
			// Typed and entered where the input is: focus() does not take, so
			// the keys go nowhere that runs.
			await hidden.evaluate((el) => el.focus());
			assert.equal(await hidden.evaluate((el) => document.activeElement === el), false, 'focus() reached the input behind');
			await page.keyboard.type('occur kettle');
			await page.keyboard.press('Enter');
			await app.idle();
			assert.deepEqual(await seen(), [], 'a key reached the tab behind');

			for (let i = 0; i < 40; i++) {
				await page.keyboard.press('Tab');
				assert.equal(
					await page.evaluate(() => !!document.activeElement?.closest('.behind')),
					false,
					`Tab ${i + 1} landed behind the tail`
				);
			}
		}, { ...DESKTOP, clock: todayAt(15) });
	});

	// A prefill (the fields widget's `add <field> `, a record's press) puts
	// the caret in the newest input widget: one mounted in a tab behind
	// cannot take it, so it goes to the widget line in front, and with none
	// there to the bar's. It was the hold panel's `note` until da-e78x6
	// removed that panel; both are the same `prefill` store.
	test('a prefill skips an input widget behind: to the widget in front, else the bar', async () => {
		await shared.solo(async (app) => {
			const { page } = app;
			await app.submit('setup coffee --verb add');
			await addTab(page, 'custom', 'input');
			await addTab(page, 'tail', 'input');
			// Out of layout mode, where Escape would ask how to leave it.
			await leaveLayoutMode(page);
			await pane(page, 'tail').locator('[role="tab"]', { hasText: 'tail' }).click();
			await page.locator('.bar [data-field="coffee"]').waitFor();

			/** Press coffee, clear the line, and say where the caret went. */
			const note = async () => {
				await page.locator('.bar [data-field="coffee"]').click();
				await page.waitForFunction(() => document.activeElement?.value === 'add coffee ');
				const where = await page.evaluate(() =>
					document.activeElement.closest('.prompt-bar') ? 'bar' : document.activeElement.closest('section[data-pane]')?.dataset.pane
				);
				await page.keyboard.press('Escape');
				await page.evaluate(() => document.activeElement?.blur());
				return where;
			};
			assert.equal(await note(), 'custom', 'not the input widget in front');

			await pane(page, 'custom').locator('[role="tab"]', { hasText: 'custom' }).click();
			assert.equal(await note(), 'bar', 'not the bar, with every input widget behind');
			assert.deepEqual(app.errors, []);
		});
	});
});
