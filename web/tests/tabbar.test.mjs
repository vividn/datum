// THE TAB BAR AT THE TOP, THE GEAR AND HELP AT ITS LEFT (da-mt32t, da-6no9e).
// vividn, 2026-10-31: «let's move the tab bar back to the top, i don't like it
// at the bottom. But keep the gear and the help button on the left like they
// are now».
//
// Top to bottom: the tab bar, the dashboard, the input, nothing between them
// and nothing past them, so the dashboard is what the two bars leave of the
// screen. What opens from the bar opens under it and stays on screen.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { DESKTOP, chooseRoot, enterLayoutMode, rootMenuButton } from './support/layout.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const PHONE = { viewport: { width: 390, height: 844 }, device: { isMobile: true, hasTouch: true } };
const SIZES = [
	['a phone', PHONE],
	['a desktop', DESKTOP]
];

const BAR = '.root-bar';
const edges = (page, selector) =>
	page.locator(selector).first().evaluate((el) => {
		const { top, bottom, left, right } = el.getBoundingClientRect();
		return { top, bottom, left, right };
	});

/** `selector` drawn whole inside the viewport. */
async function onScreen(page, selector, what) {
	await page.locator(selector).first().waitFor();
	const box = await edges(page, selector);
	const screen = await page.evaluate(() => ({ width: innerWidth, height: innerHeight }));
	assert.ok(
		box.top >= 0 && box.left >= 0 && box.bottom <= screen.height && box.right <= screen.width,
		`${what} is at ${JSON.stringify(box)} on ${JSON.stringify(screen)}`
	);
	return box;
}

const close = async (page, selector) => {
	await page.keyboard.press('Escape');
	await page.locator(selector).waitFor({ state: 'detached' });
};

for (const [name, size] of SIZES) {
	describe(name, () => {
		test('the bar is on the screen’s top, over the panes, the input on its foot under them', async () => {
			await withApp(async ({ page }) => {
				const screen = await page.evaluate(() => innerHeight);
				const [main, prompt, bar, pane] = await Promise.all(
					['main', '.prompt-bar', BAR, 'main section[data-title]'].map((s) => edges(page, s))
				);
				assert.ok(Math.abs(bar.top) <= 0.5, `the bar's top is ${bar.top}`);
				assert.ok(bar.bottom <= pane.top + 0.5, `the bar ends at ${bar.bottom}, a pane starts at ${pane.top}`);
				assert.ok(pane.bottom <= prompt.top + 0.5, `a pane ends at ${pane.bottom}, the input bar starts at ${prompt.top}`);
				assert.ok(Math.abs(prompt.bottom - screen) <= 0.5, `the input bar's bottom ${prompt.bottom} is not the screen's ${screen}`);
				assert.ok(Math.abs(main.top - bar.bottom) <= 0.5 && Math.abs(main.bottom - prompt.top) <= 0.5, `the dashboard is ${main.top}–${main.bottom}, between ${bar.bottom} and ${prompt.top}`);
			}, size);
		});

		test('from the left: the gear, the help, the tabs, the +', async () => {
			await withApp(async ({ page }) => {
				await page.locator(`${BAR} [role="tab"]`).first().waitFor();
				const [gear, help, first, last, add] = await Promise.all([
					edges(page, `${BAR} button.gear`),
					edges(page, `${BAR} button.help`),
					edges(page, `${BAR} [role="tab"]`),
					page.locator(`${BAR} [role="tab"]`).last().evaluate((el) => el.getBoundingClientRect().toJSON()),
					edges(page, `${BAR} button.add`)
				]);
				assert.ok(gear.right <= help.left && help.right <= first.left, `gear ${gear.right}, help ${help.left}–${help.right}, first tab ${first.left}`);
				assert.ok(last.right <= add.left, `last tab ${last.right}, + ${add.left}`);
			}, size);
		});

		test('each menu opens under the bar, inside the viewport', async () => {
			await withApp(async ({ page }) => {
				// Below what opened it: a button is inside the bar, a little over its foot.
				const below = async (trigger, selector, what) =>
					assert.ok((await onScreen(page, selector, what)).top >= (await trigger.evaluate((el) => el.getBoundingClientRect().bottom)), `${what} is not below its button`);

				await rootMenuButton(page).click();
				await below(rootMenuButton(page), '[role="menu"]', 'the tab’s menu');
				await close(page, '[role="menu"]');

				const gear = page.locator(`${BAR} button.gear`);
				await gear.click();
				await below(gear, '[role="menu"][aria-label="settings"]', 'the gear’s menu');
				await close(page, '[role="menu"]');

				await page.locator(`${BAR} button.add`).click();
				await onScreen(page, 'dialog.chooser[open]', 'the + chooser');
				await close(page, 'dialog.chooser');

				await chooseRoot(page, 'Restore layout');
				await below(rootMenuButton(page), `${BAR} .versions`, 'the versions');
				await chooseRoot(page, 'Restore layout');
				await page.locator(`${BAR} .versions`).waitFor({ state: 'detached' });

				await enterLayoutMode(page);
				await rootMenuButton(page).click();
				await below(rootMenuButton(page), '[role="menu"]', 'the layout-mode menu');
			}, size);
		});
	});
}

/** The visible viewport `height` tall with no window resized, as a keyboard leaves it on iOS. */
const visible = (page, height) =>
	page.evaluate((height) => {
		Object.defineProperty(visualViewport, 'height', { configurable: true, get: () => height });
		visualViewport.dispatchEvent(new Event('resize'));
	}, height);

test('a phone’s keyboard leaves the bar at the top and the input sitting on it', async () => {
	await withApp(async ({ page }) => {
		await page.locator('.prompt-bar input').focus();
		await visible(page, 480);
		await until(() => edges(page, '.prompt-bar'), (box) => Math.abs(box.bottom - 480) <= 0.5);
		assert.ok(await page.locator(BAR).isVisible(), 'the keyboard hid the bar');
		assert.ok(Math.abs((await edges(page, BAR)).top) <= 0.5);
	}, PHONE);
});
