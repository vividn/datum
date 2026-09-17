// The shell fits the VISIBLE viewport, whatever told it the size (da-31ts).
//
// vividn, 2026-09-17: an installed PWA cut its bottom bar off on first load,
// «it goes away when rotating and coming back». Nothing here can launch an
// installed PWA or raise a real keyboard, so this file does not claim the
// bug is fixed — it holds the MECHANISM: the shell is not sized in viewport
// units, and it follows the visual viewport on each event a phone sends
// when that changes (its own `resize`, `pageshow`, `orientationchange`),
// including a change that sends the window no `resize` at all, which is
// what an on-screen keyboard is on iOS. The phone is the real check.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { chromium, webkit } from 'playwright';
import { startApp, stopApp, withApp } from './support/app.mjs';
import { until } from './support/until.mjs';

before(startApp);
after(stopApp);

const PHONE = { width: 390, height: 640 };

/**
 * The phone the bug was seen on is a Pixel 8 Pro running Brave, installed
 * (Android Chromium): chromium is asked as a mobile device, webkit, the
 * second engine, at a phone's size.
 */
const onPhone = (engine) => ({
	engine,
	viewport: PHONE,
	...(engine === chromium ? { device: { isMobile: true, hasTouch: true } } : {})
});

/** The bottoms of the shell and of the bottom bar, and the visible height. */
const edges = (page) =>
	page.evaluate(() => ({
		shell: Math.round(document.querySelector('.shell').getBoundingClientRect().bottom),
		bar: Math.round(document.querySelector('.prompt-bar').getBoundingClientRect().bottom),
		visible: Math.round(visualViewport.height)
	}));

/** Wait until both bottoms sit on the visible viewport's bottom, `height`. */
async function fitsTo(page, height) {
	await until(
		() => edges(page),
		(e) => e.visible === height && e.shell === height && e.bar === height,
		5000
	);
}

/**
 * Make the visual viewport `height` tall without resizing the window — a
 * keyboard, or a launch that settled late — and send `event` on `target`.
 */
const shrinkVisible = (page, height, target, event) =>
	page.evaluate(
		([height, target, event]) => {
			Object.defineProperty(visualViewport, 'height', { configurable: true, get: () => height });
			(target === 'visualViewport' ? visualViewport : window).dispatchEvent(new Event(event));
		},
		[height, target, event]
	);

// CI launches chromium only; a webkit leg that cannot launch is skipped with
// the reason on record (see dbio.test.mjs).
async function launchable(engine) {
	try {
		await (await engine.launch()).close();
		return true;
	} catch {
		return false;
	}
}
const available = new Map(
	await Promise.all([chromium, webkit].map(async (engine) => [engine, await launchable(engine)]))
);
assert.ok([...available.values()].some(Boolean), 'no browser can be launched here — nothing examined');

for (const engine of [chromium, webkit]) {
	const skip = !available.get(engine) && `${engine.name()} cannot be launched here`;
	describe(engine.name(), { skip }, () => {
		test('the shell is sized by no viewport unit', async () => {
			await withApp(
				async ({ page }) => {
					const units = await page.evaluate(() =>
						[...document.styleSheets]
							.flatMap((sheet) => [...sheet.cssRules])
							.filter((rule) => /\.(shell|prompt-bar|expansion)\b/.test(rule.selectorText ?? ''))
							.map((rule) => rule.cssText)
							.filter((text) => /\d[dsl]?v(h|b|max|min)\b/.test(text))
					);
					assert.deepEqual(units, []);
				},
				onPhone(engine)
			);
		});

		test('the shell follows the window shrinking and growing (a URL bar)', async () => {
			await withApp(
				async ({ page }) => {
					await fitsTo(page, 640);
					await page.setViewportSize({ width: 390, height: 560 });
					await fitsTo(page, 560);
					await page.setViewportSize({ width: 390, height: 700 });
					await fitsTo(page, 700);
				},
				onPhone(engine)
			);
		});

		for (const [target, event] of [
			['visualViewport', 'resize'],
			['window', 'pageshow'],
			['window', 'orientationchange']
		])
			test(`a visible viewport that shrank without a window resize is re-measured on ${event}`, async () => {
				await withApp(
					async ({ page }) => {
						await fitsTo(page, 640);
						await shrinkVisible(page, 400, target, event);
						await fitsTo(page, 400);
					},
					onPhone(engine)
				);
			});
	});
}
