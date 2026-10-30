// THE TAB BAR AT THE SCREEN'S FOOT (da-6no9e). vividn, 2026-10-30: «let's move
// the tab bar to the bottom of the screen underneath the input bar. Put the
// gear menu and the help button on the left hand side too».
//
// Top to bottom: the dashboard, the input, the tab bar, nothing between them
// and nothing past it, so the dashboard is what the two bars leave of the
// screen. What opened downward from the top now opens upward and stays on
// screen. A phone's keyboard hides the bar, a default of vremya's.

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
		test('the bar is under the input and on the screen’s foot, and the dashboard is what is left', async () => {
			await withApp(async ({ page }) => {
				const screen = await page.evaluate(() => innerHeight);
				const [main, prompt, input, bar] = await Promise.all(
					['main', '.prompt-bar', '.prompt-bar .section.input', BAR].map((s) => edges(page, s))
				);
				assert.ok(bar.top >= input.bottom - 0.5, `the bar's top ${bar.top} is above the input's bottom ${input.bottom}`);
				assert.ok(Math.abs(bar.bottom - screen) <= 0.5, `the bar's bottom ${bar.bottom} is not the screen's ${screen}`);
				assert.ok(Math.abs(prompt.bottom - bar.top) <= 0.5, `the input ends at ${prompt.bottom}, the bar begins at ${bar.top}`);
				assert.ok(Math.abs(main.top) <= 0.5 && Math.abs(main.bottom - prompt.top) <= 0.5, `the dashboard is ${main.top}–${main.bottom}, the input starts at ${prompt.top}`);
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

		test('each menu opens inside the viewport', async () => {
			await withApp(async ({ page }) => {
				// Above what opened it: a button is inside the bar, a little under its top.
				const above = async (trigger, selector, what) =>
					assert.ok((await onScreen(page, selector, what)).bottom <= (await trigger.evaluate((el) => el.getBoundingClientRect().top)), `${what} is not above its button`);

				await rootMenuButton(page).click();
				await above(rootMenuButton(page), '[role="menu"]', 'the tab’s menu');
				await close(page, '[role="menu"]');

				const gear = page.locator(`${BAR} button.gear`);
				await gear.click();
				await above(gear, '[role="menu"][aria-label="settings"]', 'the gear’s menu');
				await close(page, '[role="menu"]');

				await page.locator(`${BAR} button.add`).click();
				await onScreen(page, 'dialog.chooser[open]', 'the + chooser');
				await close(page, 'dialog.chooser');

				await chooseRoot(page, 'Restore layout');
				await above(rootMenuButton(page), `${BAR} .versions`, 'the versions');
				await chooseRoot(page, 'Restore layout');
				await page.locator(`${BAR} .versions`).waitFor({ state: 'detached' });

				await enterLayoutMode(page);
				await rootMenuButton(page).click();
				await above(rootMenuButton(page), '[role="menu"]', 'the layout-mode menu');
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

const barShown = (page) => page.locator(BAR).isVisible();

describe('a phone’s keyboard', () => {
	test('hides the bar, so the input sits on it, and closing it brings the bar back', async () => {
		await withApp(async ({ page }) => {
			await page.locator('.prompt-bar input').focus();
			await visible(page, 480);
			await until(() => barShown(page), (shown) => !shown);
			await until(() => edges(page, '.prompt-bar'), (box) => Math.abs(box.bottom - 480) <= 0.5);

			await visible(page, 844);
			await until(() => barShown(page), (shown) => shown);
			assert.ok(Math.abs((await edges(page, BAR)).bottom - 844) <= 0.5);
		}, PHONE);
	});

	test('a box let go with the viewport still short brings the bar back, with no viewport event', async () => {
		await withApp(async ({ page }) => {
			const box = page.locator('.prompt-bar input');
			await box.focus();
			await visible(page, 480);
			await until(() => barShown(page), (shown) => !shown);

			await box.blur();
			await until(() => barShown(page), (shown) => shown);
			await box.focus();
			await until(() => barShown(page), (shown) => !shown);
		}, PHONE);
	});

	test('is not a URL bar sliding in, nor a shorter window with no box focused', async () => {
		await withApp(async ({ page }) => {
			await visible(page, 480);
			await until(() => edges(page, '.shell'), (box) => Math.abs(box.bottom - 480) <= 0.5);
			assert.ok(await barShown(page), 'a shorter window with no box focused hid the bar');

			await visible(page, 844);
			await page.locator('.prompt-bar input').focus();
			await visible(page, 760);
			await until(() => edges(page, '.shell'), (box) => Math.abs(box.bottom - 760) <= 0.5);
			assert.ok(await barShown(page), 'a URL bar’s 84px hid the bar');
		}, PHONE);
	});

	test('is no keyboard under a mouse', async () => {
		await withApp(async ({ page }) => {
			await page.locator('.prompt-bar input').focus();
			const height = DESKTOP.viewport.height;
			await visible(page, height / 2);
			await until(() => edges(page, '.shell'), (box) => Math.abs(box.bottom - height / 2) <= 0.5);
			assert.ok(await barShown(page), 'a desktop window made short hid the bar');
		}, DESKTOP);
	});
});
