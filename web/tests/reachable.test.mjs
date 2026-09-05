// Every control the day frame offers has to be on the screen it is drawn on.
//
// On a phone the frame's rows add up to more than the viewport, so the column
// scrolls — and the widget bar, the last row, started below the bottom edge.
// The document itself does not scroll, so the reader's first gesture (flick
// the page) does nothing, and the way to log without typing was gone: the
// field chips and the `+` were in the DOM at y≈707 with the screen ending at
// 664 (da-hkpd).
//
// The trap this file exists to avoid: every other test in this suite runs at
// Playwright's 1280x720, where the bar fits and everything passes. And a test
// that asserts the bar EXISTS passes even while it is off-screen — it was in
// the DOM the whole time. So these run at a phone's viewport and assert
// REACHABILITY: the control's box lies inside the screen, with no scrolling
// first.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/** A phone in portrait, once the browser's own chrome is subtracted. */
const PHONE = { width: 390, height: 640 };

/** The shortest screen worth claiming to support — an SE with chrome. */
const SHORT = { width: 390, height: 560 };

/**
 * Where `selector` is, against the screen it is drawn on.
 *
 * `getBoundingClientRect` is viewport-relative, so this is the question the
 * reader is asking with their eyes: is the thing on the screen right now?
 */
function boxOf(page, selector) {
	return page.evaluate((s) => {
		const el = document.querySelector(s);
		if (!el) return null;
		const { top, bottom } = el.getBoundingClientRect();
		return { top, bottom, screen: document.documentElement.clientHeight };
	}, selector);
}

async function assertOnScreen(page, selector, what) {
	const box = await boxOf(page, selector);
	assert.ok(box, `${what} is not on the page at all`);
	assert.ok(
		box.top >= 0 && box.bottom <= box.screen,
		`${what} is off-screen: ${box.top.toFixed(0)}–${box.bottom.toFixed(0)} on a ${box.screen}px screen`
	);
}

describe('on a phone-sized screen', () => {
	for (const viewport of [PHONE, SHORT]) {
		const size = `${viewport.width}x${viewport.height}`;

		test(`the widget bar's + is on screen at ${size}`, async () => {
			await withApp(async ({ page }) => {
				await assertOnScreen(page, '.bar button.add-button', 'the add-a-field (+) button');
			}, { viewport });
		});

		test(`a logged field's chip is on screen at ${size}`, async () => {
			await withApp(async ({ page, submit }) => {
				await submit('occur coffee');
				await page.locator('.bar button.widget', { hasText: 'coffee' }).waitFor();
				await assertOnScreen(page, '.bar button.widget', "the coffee field's chip");
			}, { viewport });
		});
	}

	// A button whose panel opens off the bottom edge is no more reachable
	// than a button that is off it: the bar is the frame's bottom row, so
	// its panels hang upward.
	test('the form the + opens is on screen', async () => {
		await withApp(async ({ page }) => {
			await page.locator('.bar button.add-button').click();
			await assertOnScreen(page, '.panel.add', "the add-a-field form");
		}, { viewport: PHONE });
	});

	test("a field's state menu is on screen", async () => {
		await withApp(async ({ page, submit }) => {
			await submit('switch mood happy');
			const chip = page.locator('.bar button.widget', { hasText: 'mood' });
			await chip.waitFor();
			await chip.click();
			await assertOnScreen(page, '.bar .panel', "mood's state menu");
		}, { viewport: PHONE });
	});
});
