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
//
// The second trap, which the first version of this file walked into: it
// varied the axis the bug was found on (viewport height) and left the other
// one — how much is IN the bar — at a single field. Five ordinary fields wrap
// the bar to two rows on a phone, and the second row hung below the screen
// again (da-vyo2). So the cases here run against a POPULATED bar as well as
// an empty one.
//
// And the count that wraps is not a constant: it is what fits across the
// screen, so it falls as the screen narrows — five fields at 390, four at
// 320, three at 280. Fixing a width and hunting for the count would just
// re-learn the width. These log one ordinary handful at every screen in the
// matrix, which wraps at all of them.

import test, { before, after, describe } from 'node:test';
import assert from 'node:assert/strict';
import { startApp, stopApp, withApp } from './support/app.mjs';

before(startApp);
after(stopApp);

/** A phone in portrait, once the browser's own chrome is subtracted. */
const PHONE = { width: 390, height: 640 };

/** The shortest screen worth claiming to support — an SE with chrome. */
const SHORT = { width: 390, height: 560 };

/** An SE's width, which is still common, and the narrowest phone there is. */
const NARROW = { width: 320, height: 664 };
const TINY = { width: 280, height: 664 };

/** Both axes of the bug: how short the screen is and how narrow it is. */
const SCREENS = [PHONE, SHORT, NARROW, TINY];

/**
 * Enough ordinary fields to wrap the bar at every screen in SCREENS.
 *
 * Not an adversarial input — this is a person tracking five things
 * (da-vyo2).
 */
const FIELDS = ['coffee', 'mood', 'sleep', 'water', 'run'];

/** Log `fields` and wait until every one of them has a chip. */
async function track(page, submit, fields) {
	for (const field of fields) await submit(`occur ${field}`);
	await page.locator('.bar button.widget').nth(fields.length - 1).waitFor();
}

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

	for (const viewport of SCREENS) {
		const size = `${viewport.width}x${viewport.height}`;

		// The one that da-hkpd's fix and its verification both missed: the
		// bar was pinned to the bottom, and then wrapped DOWNWARD past it.
		test(`the + is still on screen with ${FIELDS.length} fields at ${size}`, async () => {
			await withApp(async ({ page, submit }) => {
				await track(page, submit, FIELDS);
				await assertOnScreen(page, '.bar button.add-button', 'the add-a-field (+) button');
			}, { viewport });
		});

		// The other edge of a wrapped bar: growing it upward must not push
		// its first row off the top instead.
		test(`the first chip is on screen with ${FIELDS.length} fields at ${size}`, async () => {
			await withApp(async ({ page, submit }) => {
				await track(page, submit, FIELDS);
				await assertOnScreen(page, '.bar button.widget', "the first field's chip");
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

	// The stats widget's own page (da-n7r0): reached by the header's link,
	// and its pickers on the screen it opens on.
	for (const viewport of SCREENS) {
		const size = `${viewport.width}x${viewport.height}`;

		test(`/stats and its pickers are on screen at ${size}`, async () => {
			await withApp(async ({ page, submit }) => {
				await track(page, submit, FIELDS);
				await assertOnScreen(page, 'header nav a[href="/stats"]', 'the stats link');
				await page.locator('header nav a[href="/stats"]').click();
				await page.locator('.stats button.field').nth(FIELDS.length - 1).waitFor();
				await assertOnScreen(page, '.stats button.field', "the first field's chip");
				await assertOnScreen(page, '.stats .range button', 'the range picker');
			}, { viewport });
		});
	}

	test('the form the + opens is on screen with a wrapped bar', async () => {
		await withApp(async ({ page, submit }) => {
			await track(page, submit, FIELDS);
			await page.locator('.bar button.add-button').click();
			await assertOnScreen(page, '.panel.add', 'the add-a-field form');
		}, { viewport: PHONE });
	});
});
